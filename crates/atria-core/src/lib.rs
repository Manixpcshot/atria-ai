//! # atria-core
//!
//! Rust engine for the **Atria** desktop app.
//!
//! * Anthropic-compatible **Messages API** client (`/v1/messages`) with full
//!   agent tool use — the recommended dialect for Atria
//! * **Chat Completions** client (`/v1/chat/completions`) for OpenAI-compatible
//!   providers (Gemini compat layer, Groq, DeepSeek, OpenRouter, ...)
//! * **SSE streaming** decoder (`thinking_delta`, `text_delta`,
//!   `reasoning_content`, `input_json_delta`, ...)
//! * **Agent loop** with tools: calculator, time, memory, sandboxed file access
//! * Persistent **memory** store for `remember` / `recall`

pub mod agent;
pub mod client;
pub mod memory;
pub mod openai;
pub mod sse;
pub mod tools;
pub mod types;

pub use agent::{run_agent, AgentEvent, AgentOutput, MAX_ROUNDS};
pub use client::{send, ApiKind, AtriaClient, ClientConfig, CoreError, StreamEvent, Turn};
pub use memory::{MemoryStore, Note};
pub use tools::{execute, file_tool_catalog, tool_catalog, tool_label};
pub use types::{Block, Message, Role, Usage};
