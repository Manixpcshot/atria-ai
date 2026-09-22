//! # atria-core
//!
//! Rust engine for the **Atria** desktop app.
//!
//! * Anthropic-compatible **Messages API** client (`/v1/messages`)
//! * **SSE streaming** decoder (`thinking_delta`, `text_delta`, `input_json_delta`, ...)
//! * **Agent loop** with tool use (calculator, time, memory)
//! * Persistent **memory** store for `remember` / `recall`
//!
//! The GUI crate (`atria-app`, Tauri v2) is a thin shell around this engine.

pub mod agent;
pub mod client;
pub mod memory;
pub mod sse;
pub mod tools;
pub mod types;

pub use agent::{run_agent, AgentEvent, AgentOutput, MAX_ROUNDS};
pub use client::{AtriaClient, ClientConfig, CoreError, StreamEvent, Turn};
pub use memory::{MemoryStore, Note};
pub use types::{Block, Message, Role, Usage};
