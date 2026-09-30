//! The agent loop: model ⇄ tools until the model produces a final answer.

use crate::client::{send, strip_thinking, is_output_limit_stop_reason, ClientConfig, CoreError, StreamEvent};
use crate::memory::MemoryStore;
use crate::tools::{execute_with_data_root, file_tool_catalog, tool_catalog, tool_label};
use crate::types::{Block, Message, Role};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Safety cap on model⇄tool round-trips per user message.
pub const MAX_ROUNDS: u32 = 16;
/// Automatically request more output when the provider reports an output-token cap.
const MAX_OUTPUT_CONTINUATIONS: u32 = 3;
const CONTINUATION_PROMPT: &str = "Continue exactly from where the previous assistant message stopped at the output limit. Do not repeat already generated text. Finish the user's original request and preserve its format.";

/// Everything that happens while a turn is being generated.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A new model round started (1 = first answer attempt).
    Round(u32),
    /// Live generation deltas.
    Stream(StreamEvent),
    /// A tool call is running on the user's machine.
    ToolStart { id: String, name: String, label: String, input: serde_json::Value },
    /// A tool call finished.
    ToolEnd { id: String, name: String, label: String, ok: bool, output: String },
    /// A transient failure is being retried (network / 5xx / 429).
    Retry { attempt: u32, message: String },
}

/// Final result of one user turn.
#[derive(Debug, Clone, Default)]
pub struct AgentOutput {
    /// New messages to append to the visible/persisted history.
    pub new_messages: Vec<Message>,
    /// Text of the final assistant message.
    pub final_text: String,
    /// Number of model rounds used.
    pub rounds: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Transient failures worth retrying: network errors, timeouts, rate limits
/// and gateway hiccups (5xx) — plus "soft" queue/expiry errors that some
/// gateways (e.g. OmniRoute) hide behind a misleading HTTP 400:
/// `[504]: Request exceeded ... requestQueue.maxWaitMs ...`.
fn is_retryable(e: &CoreError) -> bool {
    match e {
        CoreError::Http(_) => true,
        CoreError::Api { status, message } => {
            if matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504 | 522 | 524 | 529) {
                return true;
            }
            let m = message.to_lowercase();
            m.contains("maxwaitms")
                || m.contains("rate-limit")
                || m.contains("rate limit")
                || m.contains("ratelimit")
                || m.contains("too many requests")
                || m.contains("queue")
                || m.contains("expiration")
                || m.contains("timeout")
                || m.contains("timed out")
                || m.contains("overloaded")
                || m.contains("temporarily")
                || m.contains("try again")
                || m.contains("[502]")
                || m.contains("[503]")
                || m.contains("[504]")
                || m.contains("[429]")
        }
        CoreError::Stopped => false,
    }
}

/// Run one agentic turn over `messages` (full history including the new user
/// message). Returns the new messages the agent produced.
pub async fn run_agent(
    cfg: &ClientConfig,
    mut messages: Vec<Message>,
    mem: &mut MemoryStore,
    stop: Arc<AtomicBool>,
    mut emit: impl FnMut(AgentEvent),
) -> Result<AgentOutput, CoreError> {
    let start_len = messages.len();
    let mut out = AgentOutput::default();
    let mut stripped_retry = false;
    let mut output_continuations = 0u32;

    loop {
        if out.rounds >= MAX_ROUNDS {
            return Err(CoreError::Api {
                status: 508,
                message: format!(
                    "agent reached its safety limit of {MAX_ROUNDS} model/tool rounds before producing a final answer; split the task into smaller steps and retry"
                ),
            });
        }
        out.rounds += 1;
        emit(AgentEvent::Round(out.rounds));

        let mut tools = if cfg.tools { tool_catalog() } else { Vec::new() };
        if cfg.file_tools {
            tools.extend(file_tool_catalog());
        }

        let mut attempt = 0u32;
        let mut turn = loop {
            let mut filt = crate::toolfmt::LiveFilter::default();
            let send_res = send(
                cfg,
                &messages,
                &tools,
                |ev| match ev {
                    crate::client::StreamEvent::Text(t) => {
                        for piece in crate::toolfmt::LiveFilter::feed(&mut filt, &t) {
                            emit(AgentEvent::Stream(crate::client::StreamEvent::Text(piece)));
                        }
                    }
                    other => emit(AgentEvent::Stream(other)),
                },
                stop.clone(),
            )
            .await;
            match send_res {
                Ok(t) => {
                    for piece in crate::toolfmt::LiveFilter::finish(&mut filt) {
                        emit(AgentEvent::Stream(crate::client::StreamEvent::Text(piece)));
                    }
                    break t
                }
                Err(CoreError::Stopped) => return Err(CoreError::Stopped),
                // Some gateways reject thinking blocks on continuation — retry once
                // with a stripped history.
                Err(CoreError::Api { status, message })
                    if !stripped_retry
                        && status == 400
                        && message.to_lowercase().contains("thinking") =>
                {
                    stripped_retry = true;
                    strip_thinking(&mut messages);
                    continue;
                }
                // Transient failures (network, timeout, 429/5xx) — auto-retry with backoff.
                Err(e) if attempt < 3 && is_retryable(&e) => {
                    attempt += 1;
                    emit(AgentEvent::Retry { attempt, message: e.to_string() });
                    let wait = 1000u64 * (1u64 << (attempt - 1));
                    let mut waited = 0u64;
                    while waited < wait {
                        if stop.load(Ordering::Relaxed) {
                            return Err(CoreError::Stopped);
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        waited += 100;
                    }
                }
                Err(e) => return Err(e),
            }
        };
        stripped_retry = false;
        // ابزارهای جاسازی‌شده در متن (DSML / <tool>) را به ToolUse واقعی تبدیل کن
        crate::toolfmt::extract_tool_calls(&mut turn.message);
        if turn.stop_reason == "end_turn"
            && turn
                .message
                .content
                .iter()
                .any(|b| matches!(b, crate::types::Block::ToolUse { .. }))
        {
            turn.stop_reason = "tool_use".to_string();
        }

        out.input_tokens += turn.usage.input_tokens;
        out.output_tokens += turn.usage.output_tokens;

        if is_output_limit_stop_reason(&turn.stop_reason) {
            let has_tool_use = turn.message.content.iter().any(|block| matches!(block, Block::ToolUse { .. }));
            if has_tool_use {
                // A complete tool call can still be useful even when the provider used
                // its remaining output tokens immediately afterwards.
                turn.stop_reason = "tool_use".to_string();
            } else {
                if output_continuations >= MAX_OUTPUT_CONTINUATIONS {
                    return Err(CoreError::Api {
                        status: 422,
                        message: format!(
                            "answer reached the output limit {MAX_OUTPUT_CONTINUATIONS} times; raise the per-part output limit or continue in a new message"
                        ),
                    });
                }
                let mut partial = turn.message;
                partial.content.retain(|block| !matches!(block, Block::Thinking { .. }));
                if !partial.plain_text().trim().is_empty() {
                    messages.push(partial);
                }
                messages.push(Message::user_text(CONTINUATION_PROMPT));
                output_continuations += 1;
                continue;
            }
        }

        // Execute tools locally; file writes remain staged until explicit user approval.
        let root = Path::new(&cfg.workspace);
        let data_root = Path::new(&cfg.app_data);
        let mut results = Vec::new();
        if turn.stop_reason == "tool_use" {
            for block in &turn.message.content {
                if let Block::ToolUse { id, name, input } = block {
                    emit(AgentEvent::ToolStart {
                        id: id.clone(),
                        name: name.clone(),
                        label: tool_label(name).to_string(),
                        input: input.clone(),
                    });
                    let res = execute_with_data_root(name, input, mem, root, data_root);
                    emit(AgentEvent::ToolEnd {
                        id: id.clone(),
                        name: name.clone(),
                        label: tool_label(name).to_string(),
                        ok: !res.is_error,
                        output: res.output.clone(),
                    });
                    results.push(Block::ToolResult {
                        tool_use_id: id.clone(),
                        content: Some(res.output),
                        is_error: Some(res.is_error),
                    });
                }
            }
        }

        messages.push(turn.message);
        if results.is_empty() {
            break; // final answer
        }
        messages.push(Message { role: Role::User, content: results });
    }

    out.new_messages = normalize_new_messages(messages.split_off(start_len));
    out.final_text = out
        .new_messages
        .iter()
        .rev()
        .find(|message| message.role == Role::Assistant)
        .map(Message::plain_text)
        .unwrap_or_default();
    Ok(out)
}

fn normalize_new_messages(messages: Vec<Message>) -> Vec<Message> {
    let mut visible: Vec<Message> = Vec::new();
    for mut message in messages {
        if message.role == Role::User
            && matches!(message.content.as_slice(), [Block::Text { text }] if text == CONTINUATION_PROMPT)
        {
            continue;
        }
        if message.role == Role::Assistant {
            message.content.retain(|block| !matches!(block, Block::Thinking { .. }));
            let contains_tool_call = message.content.iter().any(|block| matches!(block, Block::ToolUse { .. }));
            if !contains_tool_call {
                if let Some(previous) = visible.last_mut() {
                    let previous_has_tool = previous.content.iter().any(|block| matches!(block, Block::ToolUse { .. }));
                    if previous.role == Role::Assistant && !previous_has_tool {
                        if let (Some(Block::Text { text: previous_text }), Some(Block::Text { text: next_text })) =
                            (previous.content.last_mut(), message.content.first())
                        {
                            previous_text.push_str(next_text);
                            message.content.remove(0);
                        }
                        previous.content.extend(message.content);
                        continue;
                    }
                }
            }
        }
        visible.push(message);
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_answers_continue_without_leaking_internal_prompt_or_split_bubbles() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: vec![
                    Block::Thinking { thinking: "private reasoning".into(), signature: None },
                    Block::Text { text: "A long answer, part one. ".into() },
                ],
            },
            Message::user_text(CONTINUATION_PROMPT),
            Message {
                role: Role::Assistant,
                content: vec![
                    Block::Thinking { thinking: "more private reasoning".into(), signature: None },
                    Block::Text { text: "Part two.".into() },
                ],
            },
        ];
        let visible = normalize_new_messages(messages);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].plain_text(), "A long answer, part one. Part two.");
        assert!(visible[0].content.iter().all(|block| !matches!(block, Block::Thinking { .. })));
    }

    #[test]
    fn agent_round_cap_is_extended_and_output_continuation_is_bounded() {
        assert_eq!(MAX_ROUNDS, 16);
        assert_eq!(MAX_OUTPUT_CONTINUATIONS, 3);
    }

    #[test]
    fn strip_removes_thinking_only() {
        let mut msgs = vec![Message {
            role: Role::Assistant,
            content: vec![Block::Thinking { thinking: "x".into(), signature: Some("s".into()) }],
        }];
        strip_thinking(&mut msgs);
        assert!(msgs[0].content.iter().all(|b| !matches!(b, Block::Thinking { .. })));
        assert_eq!(msgs[0].content.len(), 1); // placeholder
    }
}

#[cfg(test)]
mod retry_tests {
    use super::is_retryable;
    use crate::client::CoreError;

    fn api(status: u16, message: &str) -> CoreError {
        CoreError::Api { status, message: message.to_string() }
    }

    #[test]
    fn retries_gateway_status_codes() {
        for st in [408u16, 429, 500, 502, 503, 504] {
            assert!(is_retryable(&api(st, "x")), "status {st} should retry");
        }
    }

    #[test]
    fn retries_soft_queue_timeouts_hidden_in_400() {
        // The reported OmniRoute bottleneck error: HTTP 400 wrapping a local [504].
        let e = api(400, "[504]: Request exceeded OmniRoute's local rate-limit execution expiration (legacy resilienceSettings.requestQueue.maxWaitMs=15000ms)");
        assert!(is_retryable(&e));
        assert!(is_retryable(&api(400, "Rate limit exceeded, please try again")));
        assert!(is_retryable(&api(400, "upstream timeout while waiting in queue")));
        assert!(is_retryable(&api(503, "The service is temporarily overloaded")));
    }

    #[test]
    fn does_not_retry_real_client_errors() {
        assert!(!is_retryable(&api(400, "invalid model id")));
        assert!(!is_retryable(&api(401, "invalid api key")));
        assert!(!is_retryable(&api(404, "model not found")));
        assert!(!is_retryable(&CoreError::Stopped));
    }
}
