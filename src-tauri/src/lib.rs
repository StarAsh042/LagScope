//! LagScope 卡顿透视 —— 监测电脑卡顿的原因。

mod collectors;
mod commands;
mod state;

use std::sync::Arc;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = Arc::new(AppState::new());
            app.manage(state.clone());
            collectors::start(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_processes,
            commands::get_latest_sample,
            commands::get_disks,
            commands::kill_process
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
