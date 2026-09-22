//! The agent loop: model ⇄ tools until the model produces a final answer.

use crate::client::{send, strip_thinking, ClientConfig, CoreError, StreamEvent};
use crate::memory::MemoryStore;
use crate::tools::{execute, file_tool_catalog, tool_catalog, tool_label};
use crate::types::{Block, Message, Role};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Hard cap on model⇄tool round-trips per user message.
pub const MAX_ROUNDS: u32 = 8;

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

    loop {
        if out.rounds >= MAX_ROUNDS {
            break;
        }
        out.rounds += 1;
        emit(AgentEvent::Round(out.rounds));

        let mut tools = if cfg.tools { tool_catalog() } else { Vec::new() };
        if cfg.file_tools {
            tools.extend(file_tool_catalog());
        }

        let mut attempt = 0u32;
        let turn = loop {
            match send(cfg, &messages, &tools, |ev| emit(AgentEvent::Stream(ev)), stop.clone()).await {
                Ok(t) => break t,
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

        out.input_tokens += turn.usage.input_tokens;
        out.output_tokens += turn.usage.output_tokens;

        // Execute every requested tool locally (sandboxed to the workspace).
        let root = Path::new(&cfg.workspace);
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
                    let res = execute(name, input, mem, root);
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

    out.final_text = messages.last().map(|m| m.plain_text()).unwrap_or_default();
    out.new_messages = messages.split_off(start_len);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
