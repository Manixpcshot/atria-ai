//! Local LLM (llama.cpp) IPC commands — v0.25.0.
//!
//! Lets the model library "run" a downloaded GGUF model: install the runtime,
//! spawn `llama-server` on 127.0.0.1, report readiness, and persist the
//! port/context settings. The frontend then connects the model to chat via
//! the existing OpenAI-compatible ("Chat Completions") connection plumbing.

use crate::commands::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn llama_status(st: State<'_, AppState>) -> Result<atria_core::llama::LlmStatus, String> {
    let _ = st;
    Ok(atria_core::llama::status(&crate::commands::atria_root()))
}

/// Download (once) + extract the llama.cpp runtime. Emits
/// `atria-llama-progress` {downloaded,total} while downloading.
#[tauri::command]
pub async fn llama_install(
    app: AppHandle,
    st: State<'_, AppState>,
    force: Option<bool>,
) -> Result<serde_json::Value, String> {
    let _ = st;
    let root = crate::commands::atria_root();
    let bin = atria_core::llama::install_runtime(&root, force.unwrap_or(false), |done, total| {
        let _ = app.emit(
            "atria-llama-progress",
            serde_json::json!({ "downloaded": done, "total": total }),
        );
    })
    .await?;
    Ok(serde_json::json!({ "installed": true, "binary": bin.to_string_lossy() }))
}

/// Spawn (or restart) llama-server for a downloaded model file.
#[tauri::command]
pub async fn llama_start(
    st: State<'_, AppState>,
    model_path: String,
) -> Result<serde_json::Value, String> {
    let _ = st;
    let root = crate::commands::atria_root();
    let cfg = atria_core::llama::LlmConfig::load(&root);
    let port = atria_core::llama::spawn(&root, &PathBuf::from(&model_path), &cfg)?;
    Ok(serde_json::json!({ "started": true, "port": port }))
}

#[tauri::command]
pub async fn llama_stop(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let _ = st;
    atria_core::llama::stop();
    Ok(serde_json::json!({ "stopped": true }))
}

#[tauri::command]
pub async fn llama_config(
    st: State<'_, AppState>,
    port: u16,
    ctx_size: u32,
) -> Result<serde_json::Value, String> {
    let _ = st;
    if port == 0 || port > 65535 {
        return Err("پورت معتبر نیست".into());
    }
    if ctx_size < 256 || ctx_size > 262144 {
        return Err("سایز کانتکست باید بین 256 تا 262144 باشد".into());
    }
    let root = crate::commands::atria_root();
    let cfg = atria_core::llama::LlmConfig { port, ctx_size };
    cfg.save(&root)?;
    Ok(serde_json::json!({ "saved": true, "port": cfg.port, "ctx_size": cfg.ctx_size }))
}
