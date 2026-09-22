//! IPC layer between the webview UI and the `atria-core` engine.

use atria_core::{
    run_agent, AgentEvent, ApiKind, ClientConfig, CoreError, MemoryStore, Message, Note,
    StreamEvent,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// Shared app state (managed in `main.rs`).
pub struct AppState {
    pub stop: Arc<AtomicBool>,
    pub running: Arc<AtomicBool>,
    pub mem_path: PathBuf,
}

/// Request payload from the UI (settings + full conversation history).
#[derive(Debug, Clone, Deserialize)]
pub struct ChatPayload {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system: String,
    pub tools_enabled: bool,
    pub stream: bool,
    pub messages: Vec<Message>,
    /// "anthropic" (Messages API) or "openai" (Chat Completions).
    #[serde(default)]
    pub kind: String,
    /// Enable local file tools inside the workspace folder.
    #[serde(default)]
    pub file_tools: bool,
    /// Workspace root for the file tools.
    #[serde(default)]
    pub workspace: String,
}

/// Payload of the `atria:done` event.
#[derive(Debug, Clone, Serialize)]
pub struct DonePayload {
    pub new_messages: Vec<Message>,
    pub final_text: String,
    pub rounds: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Abort the in-flight generation.
#[tauri::command]
pub fn chat_stop(state: State<'_, AppState>) {
    state.stop.store(true, Ordering::Relaxed);
}

/// Static app info for the UI (about panel, badges).
#[tauri::command]
pub fn app_meta() -> serde_json::Value {
    serde_json::json!({
        "name": "Atria",
        "version": env!("CARGO_PKG_VERSION"),
        "default_model": "Atria-Dawn-Preview",
        "engine": format!("Rust {} · Tauri 2", env!("CARGO_PKG_VERSION")),
    })
}

/// List saved memory notes.
#[tauri::command]
pub fn memory_list(state: State<'_, AppState>) -> Vec<Note> {
    MemoryStore::open(&state.mem_path).notes().to_vec()
}

/// Clear all memory notes.
#[tauri::command]
pub fn memory_clear(state: State<'_, AppState>) {
    MemoryStore::open(&state.mem_path).clear();
}

/// Start one agentic turn. Returns immediately; progress is streamed through
/// `atria:*` events and finalized with `atria:done` / `atria:error` /
/// `atria:stopped`.
#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: ChatPayload,
) -> Result<(), String> {
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("یک پاسخ دیگر در حال تولید است".to_string());
    }
    state.stop.store(false, Ordering::Relaxed);

    let stop = state.stop.clone();
    let running = state.running.clone();
    let mem_path = state.mem_path.clone();

    tauri::async_runtime::spawn(async move {
        let cfg = ClientConfig {
            api_key: payload.api_key,
            base_url: payload.base_url,
            model: payload.model,
            max_tokens: payload.max_tokens.clamp(256, 65_536),
            temperature: payload.temperature.clamp(0.0, 1.0),
            system: payload.system,
            stream: payload.stream,
            tools: payload.tools_enabled,
            kind: ApiKind::parse(&payload.kind),
            file_tools: payload.file_tools,
            workspace: payload.workspace,
        };
        let mut mem = MemoryStore::open(&mem_path);

        let result = run_agent(
            &cfg,
            payload.messages,
            &mut mem,
            stop,
            |ev| emit_agent_event(&app, ev),
        )
        .await;

        running.store(false, Ordering::SeqCst);
        match result {
            Ok(out) => {
                let _ = app.emit(
                    "atria:done",
                    DonePayload {
                        new_messages: out.new_messages,
                        final_text: out.final_text,
                        rounds: out.rounds,
                        input_tokens: out.input_tokens,
                        output_tokens: out.output_tokens,
                    },
                );
            }
            Err(CoreError::Stopped) => {
                let _ = app.emit("atria:stopped", serde_json::json!({}));
            }
            Err(e) => {
                let _ = app.emit("atria:error", serde_json::json!({ "message": e.to_string() }));
            }
        }
    });

    Ok(())
}

/// Window controls for the frameless titlebar.
#[tauri::command]
pub fn close_win(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
}

#[tauri::command]
pub fn minimize_win(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
}

#[tauri::command]
pub fn maximize_win(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_maximized().unwrap_or(false) {
            let _ = w.unmaximize();
        } else {
            let _ = w.maximize();
        }
    }
}

fn emit_agent_event(app: &AppHandle, ev: AgentEvent) {
    match ev {
        AgentEvent::Round(n) => {
            let _ = app.emit("atria:round", serde_json::json!({ "round": n }));
        }
        AgentEvent::Stream(StreamEvent::Thinking(delta)) => {
            let _ = app.emit("atria:thinking", serde_json::json!({ "delta": delta }));
        }
        AgentEvent::Stream(StreamEvent::Text(delta)) => {
            let _ = app.emit("atria:text", serde_json::json!({ "delta": delta }));
        }
        AgentEvent::Stream(StreamEvent::ToolStart { id, name }) => {
            let _ = app.emit("atria:tool_pending", serde_json::json!({ "id": id, "name": name }));
        }
        AgentEvent::Stream(StreamEvent::ToolArgs { id, n }) => {
            let _ = app.emit("atria:tool_args", serde_json::json!({ "id": id, "n": n }));
        }
        AgentEvent::ToolStart { id, name, label, input } => {
            let _ = app.emit(
                "atria:tool_start",
                serde_json::json!({ "id": id, "name": name, "label": label, "input": input }),
            );
        }
        AgentEvent::Retry { attempt, message } => {
            let _ = app.emit(
                "atria:retry",
                serde_json::json!({ "attempt": attempt, "message": message }),
            );
        }
        AgentEvent::ToolEnd { id, name, label, ok, output } => {
            let _ = app.emit(
                "atria:tool_end",
                serde_json::json!({ "id": id, "name": name, "label": label, "ok": ok, "output": output }),
            );
        }
    }
}
