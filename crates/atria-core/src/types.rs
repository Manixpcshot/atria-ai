//! Conversation types — Anthropic Messages API wire format.

use serde::{Deserialize, Serialize};

/// Chat role (`user` / `assistant`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// A content block of a message.
///
/// Wire format matches the Anthropic Messages API:
/// `{"type":"text"|"thinking"|"tool_use"|"tool_result", ...}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// Visible text.
    Text {
        text: String,
    },
    /// Reasoning trace produced by the model (Atria-Dawn is a reasoning model).
    Thinking {
        thinking: String,
        /// Opaque signature — must be echoed back verbatim on tool continuations.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// A tool invocation requested by the model.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Result of a tool invocation, sent back as a `user` message.
    ToolResult {
        tool_use_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// Inline image (vision input): base64 `data` + `media` mime type.
    Image {
        data: String,
        media: String,
    },
}

impl Block {
    /// Lenient parse of one API content block. Unknown shapes are dropped so a
    /// gateway adding new block types can never break the conversation.
    pub fn from_api(v: &serde_json::Value) -> Option<Block> {
        match v.get("type").and_then(|t| t.as_str()) {
            Some("text") => Some(Block::Text {
                text: v.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            }),
            Some("thinking") => Some(Block::Thinking {
                thinking: v.get("thinking").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                signature: v.get("signature").and_then(|t| t.as_str()).map(str::to_string),
            }),
            Some("tool_use") => Some(Block::ToolUse {
                id: v.get("id").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                name: v.get("name").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                input: v.get("input").cloned().unwrap_or(serde_json::Value::Null),
            }),
            Some("tool_result") => Some(Block::ToolResult {
                tool_use_id: v.get("tool_use_id").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                content: v.get("content").and_then(|t| t.as_str()).map(str::to_string),
                is_error: v.get("is_error").and_then(|t| t.as_bool()),
            }),
            Some("image") => {
                let src = v.get("source");
                let data = v
                    .get("data")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| src.and_then(|s| s.get("data")).and_then(serde_json::Value::as_str))
                    .unwrap_or("");
                let media = v
                    .get("media")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| v.get("media_type").and_then(serde_json::Value::as_str))
                    .or_else(|| src.and_then(|s| s.get("media_type")).and_then(serde_json::Value::as_str))
                    .unwrap_or("image/png");
                if data.is_empty() {
                    None
                } else {
                    Some(Block::Image { data: data.to_string(), media: media.to_string() })
                }
            }
            _ => None,
        }
    }

    /// Parse a `content` field that is either a block array or a plain string.
    pub fn from_api_array(v: &serde_json::Value) -> Vec<Block> {
        match v {
            serde_json::Value::Array(items) => items.iter().filter_map(Block::from_api).collect(),
            serde_json::Value::String(s) => vec![Block::Text { text: s.clone() }],
            _ => vec![],
        }
    }
}

/// One chat message with block content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    /// Block array on the wire; a plain string is accepted too (lenient).
    #[serde(deserialize_with = "de_blocks")]
    pub content: Vec<Block>,
}

/// Accept `content` as either a block array or a plain string.
fn de_blocks<'de, D>(d: D) -> Result<Vec<Block>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(Block::from_api_array(&v))
}

impl Message {
    /// Shorthand for a plain-text user message.
    pub fn user_text(s: impl Into<String>) -> Message {
        Message {
            role: Role::User,
            content: vec![Block::Text { text: s.into() }],
        }
    }

    /// Concatenation of all visible text blocks (skips thinking / tool data).
    pub fn plain_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                Block::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// True when any block is a thinking block.
    pub fn has_thinking(&self) -> bool {
        self.content.iter().any(|b| matches!(b, Block::Thinking { .. }))
    }
}

/// Token usage reported by the API.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_wire_format_roundtrip() {
        let b = Block::Text { text: "سلام".into() };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v, json!({"type": "text", "text": "سلام"}));

        let b = Block::ToolUse {
            id: "t1".into(),
            name: "calculator".into(),
            input: json!({"expression": "2+2"}),
        };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["type"], "tool_use");

        let parsed: Block = serde_json::from_value(v).unwrap();
        assert_eq!(parsed, b);
    }

    #[test]
    fn thinking_block_keeps_signature() {
        let raw = json!({"type": "thinking", "thinking": "let me think", "signature": "sig123"});
        let b = Block::from_api(&raw).unwrap();
        match b {
            Block::Thinking { thinking, signature } => {
                assert_eq!(thinking, "let me think");
                assert_eq!(signature.as_deref(), Some("sig123"));
            }
            other => panic!("wrong block: {other:?}"),
        }
    }

    #[test]
    fn unknown_blocks_are_dropped() {
        let raw = json!([
            {"type": "text", "text": "hi"},
            {"type": "mystery_block", "x": 1},
            {"type": "thinking", "thinking": "t"}
        ]);
        let blocks = Block::from_api_array(&raw);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn string_content_is_accepted() {
        let blocks = Block::from_api_array(&json!("hello"));
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn message_plain_text_skips_non_text() {
        let m = Message {
            role: Role::Assistant,
            content: vec![
                Block::Thinking { thinking: "hmm".into(), signature: None },
                Block::Text { text: "part one".into() },
                Block::Text { text: "part two".into() },
            ],
        };
        assert_eq!(m.plain_text(), "part one\npart two");
        assert!(m.has_thinking());
    }
}
