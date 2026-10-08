//! Atria — AI desktop assistant (Rust engine + Tauri v2 shell).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod secrets;
mod updates;

use commands::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
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
            .build()?;

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
            commands::dirs_info,
            commands::chats_load,
            commands::chats_sync,
            commands::reveal_dir,
            commands::chat_send,
            commands::chat_stop,
            commands::memory_list,
            commands::memory_clear,
            commands::app_meta,
            commands::close_win,
            commands::minimize_win,
            commands::maximize_win,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Atria app");
}
