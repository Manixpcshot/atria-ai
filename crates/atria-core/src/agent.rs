//! The agent loop: model ⇄ tools until the model produces a final answer.

use crate::client::{send, strip_thinking, ClientConfig, CoreError, StreamEvent};
use crate::memory::MemoryStore;
use crate::tools::{execute, file_tool_catalog, tool_catalog, tool_label};
use crate::types::{Block, Message, Role};
use std::path::Path;
use std::sync::atomic::AtomicBool;
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

        let turn = match send(cfg, &messages, &tools, |ev| emit(AgentEvent::Stream(ev)), stop.clone())
            .await
        {
            Ok(t) => t,
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
            Err(e) => return Err(e),
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
