//! Anthropic-compatible **Messages API** client with SSE streaming.

use crate::sse::SseDecoder;
use crate::types::{Block, Message, Role, Usage};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Which wire protocol to speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiKind {
    /// Anthropic Messages API (`/v1/messages`) — full agent tool use on Atria.
    #[default]
    Anthropic,
    /// OpenAI-compatible Chat Completions (`/v1/chat/completions`) —
    /// OpenAI, Gemini (compat layer), Groq, DeepSeek, OpenRouter, ...
    OpenAi,
}

impl ApiKind {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "openai" | "oai" | "openai_compat" | "chat" | "chat_completions"
            | "chat-completions" | "completions" => ApiKind::OpenAi,
            _ => ApiKind::Anthropic,
        }
    }
}

/// Connection + generation settings.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system: String,
    /// Use SSE streaming (`"stream": true`).
    pub stream: bool,
    /// Advertise built-in tools (agent mode).
    pub tools: bool,
    /// Which API dialect to use.
    pub kind: ApiKind,
    /// Local file-access tools (`list_files` / `read_file` / `write_file`).
    pub file_tools: bool,
    /// Sandbox root for the file tools (empty = disabled).
    pub workspace: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.atria-asi.ai".to_string(),
            model: "Atria-Dawn-Preview".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            system: String::new(),
            stream: true,
            tools: true,
            kind: ApiKind::Anthropic,
            file_tools: false,
            workspace: String::new(),
        }
    }
}

/// Live events emitted while a turn is being generated.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Thinking(String),
    Text(String),
    ToolStart { id: String, name: String },
}

/// One completed assistant turn.
#[derive(Debug, Clone)]
pub struct Turn {
    pub message: Message,
    pub stop_reason: String,
    pub usage: Usage,
}

/// Engine-level errors.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("stopped by user")]
    Stopped,
}

/// HTTP client for `POST /v1/messages`.
pub struct AtriaClient {
    http: reqwest::Client,
}

impl Default for AtriaClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AtriaClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(12))
            .read_timeout(std::time::Duration::from_secs(45))
            .build()
            .expect("build http client");
        Self { http }
    }

    /// Send `messages` to the model and return the full assistant turn.
    ///
    /// `on_event` receives live deltas (streaming) or nothing (non-streaming).
    /// `stop` aborts the stream mid-flight with [`CoreError::Stopped`].
    pub async fn send(
        &self,
        cfg: &ClientConfig,
        messages: &[Message],
        tools: &[Value],
        mut on_event: impl FnMut(StreamEvent),
        stop: Arc<AtomicBool>,
    ) -> Result<Turn, CoreError> {
        let mut body = json!({
            "model": cfg.model,
            "max_tokens": cfg.max_tokens,
            "temperature": cfg.temperature,
            "messages": messages,
        });
        if !cfg.system.trim().is_empty() {
            body["system"] = Value::String(cfg.system.clone());
        }
        if cfg.tools && !tools.is_empty() {
            body["tools"] = Value::Array(tools.to_vec());
            body["tool_choice"] = json!({ "type": "auto" });
        }
        if cfg.stream {
            body["stream"] = Value::Bool(true);
        }

        let url = messages_url(&cfg.base_url);
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &cfg.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
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
        let mut blocks: Vec<Acc> = Vec::new();
        let mut stop_reason = String::from("end_turn");
        let mut usage = Usage::default();

        while let Some(chunk) = stream.next().await {
            if stop.load(Ordering::Relaxed) {
                return Err(CoreError::Stopped);
            }
            let chunk = chunk?;

            for ev in dec.feed(&chunk) {
                match ev.name.as_str() {
                    "message_start" => {
                        usage.input_tokens = ev
                            .data
                            .pointer("/message/usage/input_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32;
                    }
                    "content_block_start" => {
                        let cb = &ev.data["content_block"];
                        match cb["type"].as_str().unwrap_or("") {
                            "thinking" => blocks.push(Acc::Thinking {
                                text: cb["thinking"].as_str().unwrap_or("").to_string(),
                                signature: cb["signature"].as_str().map(str::to_string),
                            }),
                            "text" => {
                                blocks.push(Acc::Text(cb["text"].as_str().unwrap_or("").to_string()))
                            }
                            "tool_use" => {
                                let id = cb["id"].as_str().unwrap_or("").to_string();
                                let name = cb["name"].as_str().unwrap_or("").to_string();
                                on_event(StreamEvent::ToolStart { id: id.clone(), name: name.clone() });
                                blocks.push(Acc::ToolUse { id, name, input_json: String::new() });
                            }
                            _ => blocks.push(Acc::Skip),
                        }
                    }
                    "content_block_delta" => {
                        let idx = ev.data["index"].as_u64().unwrap_or(0) as usize;
                        let d = &ev.data["delta"];
                        match d["type"].as_str().unwrap_or("") {
                            "thinking_delta" => {
                                let t = d["thinking"].as_str().unwrap_or("");
                                if let Some(Acc::Thinking { text, .. }) = blocks.get_mut(idx) {
                                    text.push_str(t);
                                }
                                if !t.is_empty() {
                                    on_event(StreamEvent::Thinking(t.to_string()));
                                }
                            }
                            "signature_delta" => {
                                if let Some(Acc::Thinking { signature, .. }) = blocks.get_mut(idx) {
                                    *signature = d["signature"].as_str().map(str::to_string);
                                }
                            }
                            "text_delta" => {
                                let t = d["text"].as_str().unwrap_or("");
                                if let Some(Acc::Text(text)) = blocks.get_mut(idx) {
                                    text.push_str(t);
                                }
                                if !t.is_empty() {
                                    on_event(StreamEvent::Text(t.to_string()));
                                }
                            }
                            "input_json_delta" => {
                                let p = d["partial_json"].as_str().unwrap_or("");
                                if let Some(Acc::ToolUse { input_json, .. }) = blocks.get_mut(idx) {
                                    input_json.push_str(p);
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_delta" => {
                        if let Some(sr) =
                            ev.data.pointer("/delta/stop_reason").and_then(Value::as_str)
                        {
                            stop_reason = sr.to_string();
                        }
                        usage.output_tokens = ev
                            .data
                            .pointer("/usage/output_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32;
                    }
                    "error" => {
                        return Err(CoreError::Api {
                            status: 400,
                            message: ev
                                .data
                                .pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("stream error")
                                .to_string(),
                        });
                    }
                    _ => {} // message_stop, ping, ...
                }
            }
        }

        Ok(Turn {
            message: Message { role: Role::Assistant, content: acc_to_blocks(blocks) },
            stop_reason,
            usage,
        })
    }
}

/// In-flight accumulator for one content block (streaming).
enum Acc {
    Thinking { text: String, signature: Option<String> },
    Text(String),
    ToolUse { id: String, name: String, input_json: String },
    Skip,
}

fn acc_to_blocks(accs: Vec<Acc>) -> Vec<Block> {
    accs.into_iter()
        .filter_map(|a| match a {
            Acc::Thinking { text, signature } => Some(Block::Thinking { thinking: text, signature }),
            Acc::Text(text) => Some(Block::Text { text }),
            Acc::ToolUse { id, name, input_json } => {
                let input = if input_json.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&input_json).unwrap_or_else(|_| json!({}))
                };
                Some(Block::ToolUse { id, name, input })
            }
            Acc::Skip => None,
        })
        .collect()
}

fn turn_from_value(v: &Value) -> Turn {
    Turn {
        message: Message {
            role: Role::Assistant,
            content: Block::from_api_array(&v["content"]),
        },
        stop_reason: v["stop_reason"].as_str().unwrap_or("end_turn").to_string(),
        usage: Usage {
            input_tokens: v.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
            output_tokens: v.pointer("/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        },
    }
}

/// Normalize a user-entered base URL to the full Messages endpoint.
/// Accepts `https://host`, `https://host/`, `https://host/v1`,
/// `https://host/v1/messages`, `https://host/messages` — all resolve to
/// `https://host/v1/messages`.
pub fn messages_url(base: &str) -> String {
    let mut b = base.trim().trim_end_matches('/').to_string();
    for suffix in ["/v1/messages", "/messages", "/v1"] {
        if b.ends_with(suffix) {
            b.truncate(b.len() - suffix.len());
            break;
        }
    }
    format!("{}/v1/messages", b.trim_end_matches('/'))
}

pub(crate) fn extract_error(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        if let Some(m) = v.pointer("/error/message").and_then(Value::as_str) {
            return m.to_string();
        }
        if let Some(m) = v["message"].as_str() {
            return m.to_string();
        }
    }
    text.chars().take(400).collect()
}

/// Unified send: routes to the Messages API or the Chat Completions backend,
/// chosen via [`ClientConfig::kind`]. This is what the agent loop calls.
pub async fn send(
    cfg: &ClientConfig,
    messages: &[Message],
    tools: &[Value],
    on_event: impl FnMut(StreamEvent),
    stop: Arc<AtomicBool>,
) -> Result<Turn, CoreError> {
    match cfg.kind {
        ApiKind::Anthropic => {
            AtriaClient::new().send(cfg, messages, tools, on_event, stop).await
        }
        ApiKind::OpenAi => {
            crate::openai::OpenAiClient::new()
                .send(cfg, messages, tools, on_event, stop)
                .await
        }
    }
}

/// Remove thinking blocks from history (fallback when a gateway rejects them).
pub fn strip_thinking(messages: &mut [Message]) {
    for m in messages.iter_mut() {
        m.content.retain(|b| !matches!(b, Block::Thinking { .. }));
        if m.content.is_empty() {
            m.content.push(Block::Text { text: "(continue)".to_string() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_url_normalizes() {
        for base in [
            "https://api.atria-asi.ai",
            "https://api.atria-asi.ai/",
            "https://api.atria-asi.ai/v1",
            "https://api.atria-asi.ai/v1/messages",
            "https://api.atria-asi.ai/messages",
        ] {
            assert_eq!(messages_url(base), "https://api.atria-asi.ai/v1/messages", "{base}");
        }
    }

    #[test]
    fn chat_url_normalizes() {
        assert_eq!(
            crate::openai::chat_completions_url("https://api.atria-asi.ai"),
            "https://api.atria-asi.ai/v1/chat/completions"
        );
        assert_eq!(
            crate::openai::chat_completions_url("https://api.atria-asi.ai/v1"),
            "https://api.atria-asi.ai/v1/chat/completions"
        );
        assert_eq!(
            crate::openai::chat_completions_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            crate::openai::chat_completions_url("https://api.atria-asi.ai/v1/chat/completions"),
            "https://api.atria-asi.ai/v1/chat/completions"
        );
        assert_eq!(
            crate::openai::chat_completions_url("https://generativelanguage.googleapis.com/v1beta/openai"),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }
}
