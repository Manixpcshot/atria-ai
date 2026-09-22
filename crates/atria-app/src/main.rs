//! Atria — AI desktop assistant (Rust engine + Tauri v2 shell).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Persistent storage for agent memory.
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let mem_path = data_dir.join("memory.json");
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
