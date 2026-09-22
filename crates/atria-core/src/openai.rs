//! OpenAI-compatible **Chat Completions** backend (`/v1/chat/completions`).
//!
//! Universal dialect spoken by OpenAI, Groq, DeepSeek, OpenRouter, Ollama —
//! and by **Google Gemini** through its OpenAI-compat layer:
//! `https://generativelanguage.googleapis.com/v1beta/openai/`
//!
//! The Atria gateway also exposes this dialect (with `reasoning_content` for
//! the reasoning traces of `Atria-Dawn-Preview`). Note: asking the Atria
//! router to *emit* tool calls on this endpoint is unreliable (can stall) —
//! agent mode on Atria keeps using the Messages API (`/v1/messages`) where
//! tool use is fully supported. Other providers emit `tool_calls` normally.

use crate::client::{extract_error, ClientConfig, CoreError, StreamEvent, Turn};
use crate::sse::SseDecoder;
use crate::types::{Block, Message, Role, Usage};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// HTTP client for `POST /v1/chat/completions`.
/// Normalize a user-entered base URL to the full Chat Completions endpoint.
/// Accepts bare hosts (`https://api.atria-asi.ai` → `/v1/chat/completions`),
/// versioned bases (`https://api.openai.com/v1`, Gemini's `/v1beta/openai`),
/// and full endpoints (kept as-is).
pub fn chat_completions_url(base: &str) -> String {
    let b = base.trim().trim_end_matches('/').to_string();
    if b.ends_with("/chat/completions") {
        return b;
    }
    let after_scheme = b.split_once("://").map(|(_, r)| r).unwrap_or(&b);
    if !after_scheme.contains('/') {
        // bare host — assume the standard /v1 prefix
        return format!("{b}/v1/chat/completions");
    }
    format!("{b}/chat/completions")
}

/// HTTP client for `POST /chat/completions`.
pub struct OpenAiClient {
    http: reqwest::Client,
}

impl Default for OpenAiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(12))
            .read_timeout(std::time::Duration::from_secs(45))
            .build()
            .expect("build http client");
        Self { http }
    }

    pub async fn send(
        &self,
        cfg: &ClientConfig,
        messages: &[Message],
        tools: &[Value],
        mut on_event: impl FnMut(StreamEvent),
        stop: Arc<AtomicBool>,
    ) -> Result<Turn, CoreError> {
        let mut msgs: Vec<Value> = Vec::new();
        if !cfg.system.trim().is_empty() {
            msgs.push(json!({ "role": "system", "content": cfg.system }));
        }
        msgs.extend(to_openai_messages(messages));

        let mut body = json!({
            "model": cfg.model,
            "messages": msgs,
            "temperature": cfg.temperature,
            "max_tokens": cfg.max_tokens,
        });
        if cfg.tools && !tools.is_empty() {
            body["tools"] = Value::Array(tools.iter().map(to_openai_tool).collect());
            body["tool_choice"] = json!("auto");
        }
        if cfg.stream {
            body["stream"] = Value::Bool(true);
        }

        let url = chat_completions_url(&cfg.base_url);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", cfg.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Api {
                status: status.as_u16(),
                message: format!("{} [{}]", extract_error(&text), url),
            });
        }

        if cfg.stream {
            self.read_stream(resp, &mut on_event, &stop).await
        } else {
            if stop.load(Ordering::Relaxed) {
                return Err(CoreError::Stopped);
            }
            let v: Value = resp.json().await?;
            Ok(turn_from_value(&v))
        }
    }

    async fn read_stream(
        &self,
        resp: reqwest::Response,
        on_event: &mut impl FnMut(StreamEvent),
        stop: &Arc<AtomicBool>,
    ) -> Result<Turn, CoreError> {
        use futures::StreamExt;

        let mut stream = resp.bytes_stream();
        let mut dec = SseDecoder::new();
        let mut thinking = String::new();
        let mut text = String::new();
        // tool index -> (id, name, args_json)
        let mut tool_map: BTreeMap<usize, (String, String, String)> = BTreeMap::new();
        let mut stop_reason = String::from("end_turn");
        let mut usage = Usage::default();

        while let Some(chunk) = stream.next().await {
            if stop.load(Ordering::Relaxed) {
                return Err(CoreError::Stopped);
            }
            let chunk = chunk?;

            for ev in dec.feed(&chunk) {
                let v = &ev.data;
                if v["error"].is_object() {
                    return Err(CoreError::Api {
                        status: 400,
                        message: extract_error(&v["error"].to_string()),
                    });
                }
                // data: [DONE] is not JSON — the decoder already drops it.
                let Some(choice) = v.pointer("/choices/0") else { continue };

                if let Some(fr) = choice["finish_reason"].as_str() {
                    stop_reason = map_finish(fr);
                }
                if let Some(u) = v["usage"].as_object() {
                    if let Some(n) = u.get("prompt_tokens").and_then(Value::as_u64) {
                        usage.input_tokens = n as u32;
                    }
                    if let Some(n) = u.get("completion_tokens").and_then(Value::as_u64) {
                        usage.output_tokens = n as u32;
                    }
                }

                if let Some(delta) = choice.get("delta") {
                    // Reasoning models (DeepSeek-style) stream `reasoning_content`.
                    for key in ["reasoning_content", "reasoning"] {
                        if let Some(t) = delta[key].as_str() {
                            if !t.is_empty() {
                                thinking.push_str(t);
                                on_event(StreamEvent::Thinking(t.to_string()));
                            }
                        }
                    }
                    if let Some(t) = delta["content"].as_str() {
                        if !t.is_empty() {
                            text.push_str(t);
                            on_event(StreamEvent::Text(t.to_string()));
                        }
                    }
                    if let Some(calls) = delta["tool_calls"].as_array() {
                        for c in calls {
                            let idx = c["index"].as_u64().unwrap_or(0) as usize;
                            let entry = tool_map
                                .entry(idx)
                                .or_insert_with(|| (String::new(), String::new(), String::new()));
                            let was_empty = entry.0.is_empty() && entry.1.is_empty();
                            if let Some(id) = c["id"].as_str() {
                                if !id.is_empty() {
                                    entry.0 = id.to_string();
                                }
                            }
                            if let Some(n) = c.pointer("/function/name").and_then(Value::as_str) {
                                entry.1.push_str(n);
                            }
                            if let Some(a) =
                                c.pointer("/function/arguments").and_then(Value::as_str)
                            {
                                entry.2.push_str(a);
                            }
                            if was_empty && (!entry.0.is_empty() || !entry.1.is_empty()) {
                                on_event(StreamEvent::ToolStart {
                                    id: entry.0.clone(),
                                    name: entry.1.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        let mut blocks: Vec<Block> = Vec::new();
        if !thinking.is_empty() {
            blocks.push(Block::Thinking { thinking, signature: None });
        }
        if !text.is_empty() {
            blocks.push(Block::Text { text });
        }
        for (_idx, (id, name, args)) in tool_map {
            let input = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            blocks.push(Block::ToolUse {
                id: if id.is_empty() { format!("call_{name}") } else { id },
                name,
                input,
            });
        }

        Ok(Turn {
            message: Message { role: Role::Assistant, content: blocks },
            stop_reason,
            usage,
        })
    }
}

fn map_finish(reason: &str) -> String {
    match reason {
        "tool_calls" | "function_call" => "tool_use".to_string(),
        other => other.to_string(),
    }
}

fn turn_from_value(v: &Value) -> Turn {
    let msg = v.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
    let mut blocks: Vec<Block> = Vec::new();

    for key in ["reasoning_content", "reasoning"] {
        if let Some(t) = msg[key].as_str() {
            if !t.is_empty() {
                blocks.push(Block::Thinking { thinking: t.to_string(), signature: None });
                break;
            }
        }
    }
    if let Some(t) = msg["content"].as_str() {
        if !t.is_empty() {
            blocks.push(Block::Text { text: t.to_string() });
        }
    }
    if let Some(calls) = msg["tool_calls"].as_array() {
        for c in calls {
            let args =
                c.pointer("/function/arguments").and_then(Value::as_str).unwrap_or("{}");
            blocks.push(Block::ToolUse {
                id: c["id"].as_str().unwrap_or_default().to_string(),
                name: c.pointer("/function/name").and_then(Value::as_str).unwrap_or_default().to_string(),
                input: serde_json::from_str(args).unwrap_or_else(|_| json!({})),
            });
        }
    }

    Turn {
        message: Message { role: Role::Assistant, content: blocks },
        stop_reason: map_finish(v.pointer("/choices/0/finish_reason").and_then(Value::as_str).unwrap_or("stop")),
        usage: Usage {
            input_tokens: v.pointer("/usage/prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
            output_tokens: v
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
        },
    }
}

/// Convert unified messages to the OpenAI `messages` array.
///
/// Thinking blocks are dropped (not representable in this dialect);
/// `tool_use`/`tool_result` map to `tool_calls` / `role:"tool"` messages.
pub fn to_openai_messages(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();

    for m in messages {
        match m.role {
            Role::User => {
                // 1) tool results as standalone `tool` messages …
                for b in &m.content {
                    if let Block::ToolResult { tool_use_id, content, is_error } = b {
                        let mut c = content.clone().unwrap_or_default();
                        if is_error == &Some(true) {
                            c = format!("[tool error] {c}");
                        }
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": c,
                        }));
                    }
                }
                // 2) remaining text as a normal user message
                let text = join_texts(&m.content);
                if !text.is_empty() {
                    out.push(json!({ "role": "user", "content": text }));
                }
            }
            Role::Assistant => {
                let text = join_texts(&m.content);
                let calls: Vec<Value> = m
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        Block::ToolUse { id, name, input } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": input.to_string(),
                            }
                        })),
                        _ => None,
                    })
                    .collect();

                let mut msg = json!({ "role": "assistant" });
                if calls.is_empty() {
                    msg["content"] = Value::String(text);
                } else {
                    msg["content"] =
                        if text.is_empty() { Value::Null } else { Value::String(text) };
                    msg["tool_calls"] = Value::Array(calls);
                }
                out.push(msg);
            }
        }
    }
    out
}

fn join_texts(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert our Anthropic-style tool catalog to OpenAI function tools.
pub fn to_openai_tool(t: &Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": t["name"],
            "description": t["description"],
            "parameters": t["input_schema"],
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_messages_with_tools() {
        let msgs = vec![
            Message {
                role: Role::Assistant,
                content: vec![
                    Block::Thinking { thinking: "hmm".into(), signature: Some("s".into()) },
                    Block::ToolUse {
                        id: "call_1".into(),
                        name: "calculator".into(),
                        input: json!({ "expression": "2+2" }),
                    },
                ],
            },
            Message {
                role: Role::User,
                content: vec![
                    Block::ToolResult {
                        tool_use_id: "call_1".into(),
                        content: Some("2+2 = 4".into()),
                        is_error: Some(false),
                    },
                    Block::Text { text: "thanks".into() },
                ],
            },
        ];
        let out = to_openai_messages(&msgs);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0]["tool_calls"][0]["function"]["name"], "calculator");
        assert!(out[0]["content"].is_null());
        assert_eq!(out[1]["role"], "tool");
        assert_eq!(out[1]["tool_call_id"], "call_1");
        assert_eq!(out[2]["role"], "user");
    }

    #[test]
    fn maps_tool_catalog() {
        let cat = crate::tools::tool_catalog();
        let f = to_openai_tool(&cat[0]);
        assert_eq!(f["type"], "function");
        assert_eq!(f["function"]["name"], "calculator");
    }

    #[test]
    fn maps_tool_calls_finish_reason() {
        assert_eq!(map_finish("tool_calls"), "tool_use");
        assert_eq!(map_finish("stop"), "stop");
    }

    #[test]
    fn parses_tool_calls_response() {
        let v = json!({
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "reasoning_content": "let me think",
                    "tool_calls": [{
                        "id": "call_9",
                        "type": "function",
                        "function": { "name": "calculator", "arguments": "{\"expression\":\"17*24+3\"}" }
                    }]
                }
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 20 }
        });
        let turn = turn_from_value(&v);
        assert_eq!(turn.stop_reason, "tool_use");
        assert!(matches!(turn.message.content[0], Block::Thinking { .. }));
        match &turn.message.content[1] {
            Block::ToolUse { id, name, input } => {
                assert_eq!(id, "call_9");
                assert_eq!(name, "calculator");
                assert_eq!(input["expression"], "17*24+3");
            }
            other => panic!("expected tool_use, got {other:?}"),
        }
    }
}
