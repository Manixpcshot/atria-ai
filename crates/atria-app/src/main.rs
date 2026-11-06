//! Atria — AI desktop assistant (Rust engine + Tauri v2 shell).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod img;
mod local_llm;
mod models;
mod scheduler;
mod secrets;
mod updates;
mod winos;

use commands::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Persistent storage under the user's ~/.atria folder (like official AI apps):
            // chats/ + workspace/ + memory.json
            let root = commands::atria_root();
            let _ = std::fs::create_dir_all(root.join("chats"));
            let _ = std::fs::create_dir_all(root.join("workspace"));
            let mem_path = root.join("memory.json");
            // one-time migration of the old app-data memory file
            if !mem_path.exists() {
                if let Some(old_mem) = app
                    .path()
                    .app_data_dir()
                    .ok()
                    .map(|d| d.join("memory.json"))
                {
                    if old_mem.exists() {
                        let _ = std::fs::copy(&old_mem, &mem_path);
                    }
                }
            }
            app.manage(AppState {
                stop: Arc::new(AtomicBool::new(false)),
                running: Arc::new(AtomicBool::new(false)),
                mem_path,
            });

            // Frameless "Dawn" window (rounded, translucent on Windows).
            // ATRIA_OPAQUE=1 forces an opaque window (used for headless tests).
            let transparent = std::env::var("ATRIA_OPAQUE").is_err();
            // ATRIA_DEMO=1 seeds a sample conversation (design/testing aid).
            let page: String = if std::env::var("ATRIA_DEMO").is_ok() {
                "index.html?demo=1".into()
            } else {
                "index.html".into()
            };
            // Taskbar icon: attach the full 512px pixel icon to the window itself.
            // On Linux this sets _NET_WM_ICON at window-creation time (before the
            // window is first mapped), so the panel has a crisp icon from frame one
            // without depending on the desktop's icon-theme cache (which is the
            // usual cause of the taskbar icon flickering with AppImages).
            let window_icon = tauri::image::Image::from_bytes(include_bytes!(
                "../icons/icon.png"
            ))
            .expect("bundled app icon must be a valid image");
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App(page.into()),
            )
            .title("آتریا — دستیار هوشمند")
            .inner_size(1280.0, 840.0)
            .min_inner_size(900.0, 620.0)
            .decorations(false)
            .transparent(transparent)
            .shadow(true)
            .resizable(true)
            .center()
            .focused(true)
            .icon(window_icon)?
            .build()?;

            // v0.18.0: scheduler thread + global hotkey (Ctrl+Alt+A).
            scheduler::spawn(app.handle().clone());
            let _ = winos::start_hotkey(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_models,
            commands::test_connection,
            commands::secret_set,
            commands::secret_get,
            commands::secret_delete,
            commands::update_check,
            commands::update_install,
            commands::file_apply_edit,
            commands::file_reject_edit,
            commands::file_restore_backup,
            commands::github_connect,
            commands::github_status,
            commands::github_disconnect,
            commands::github_apply_action,
            commands::github_reject_action,
            commands::open_pending_url,
            commands::reject_pending_url,
            commands::command_apply_command,
            commands::command_reject_command,
            commands::dirs_info,
            commands::downloads_root_info,
            commands::chats_load,
            commands::chats_sync,
            commands::reveal_dir,
            commands::reveal_path,
            commands::chat_send,
            commands::chat_stop,
            commands::mcp_test_server,
            commands::files_stash,
            commands::memory_list,
            commands::memory_clear,
            commands::app_meta,
            commands::close_win,
            commands::minimize_win,
            commands::maximize_win,
            commands::clipboard_read,
            commands::notify_run_done,
            commands::github_repo_info,
            commands::memory_extract,
            commands::files_read,
            commands::schedule_list,
            commands::schedule_upsert,
            commands::schedule_delete,
            commands::schedule_set_connection,
            commands::schedule_clear_connection,
            commands::schedule_has_connection,
            commands::schedule_unprocessed,
            commands::schedule_ack,
            // v0.24.0: model library (Hugging Face) + image studio
            models::models_state,
            models::models_set_folder,
            models::models_hf_token,
            models::models_hf_token_get,
            models::models_search,
            models::models_add,
            models::models_start,
            models::models_pause,
            models::models_stop,
            models::models_remove,
            img::img_generate,
            img::img_gallery_list,
            img::img_delete,
            img::img_bytes,
            img::img_folder,
            img::img_set_folder,
            // v0.25.0: local LLM runtime (llama.cpp)
            local_llm::llama_status,
            local_llm::llama_install,
            local_llm::llama_start,
            local_llm::llama_stop,
            local_llm::llama_config,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the Atria app")
        .run(|_app_handle, event| {
            // v0.25.0: never leave a llama-server child behind
            if let tauri::RunEvent::Exit = event {
                atria_core::llama::shutdown();
            }
        });
}
