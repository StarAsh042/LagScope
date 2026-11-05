//! LagScope 卡顿透视 —— 监测电脑卡顿的原因。

mod collectors;
mod commands;
mod engine;
mod state;
mod store;

use std::sync::Arc;

use tauri::Manager;

use state::AppState;
use store::Db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 事件库:appdata/lagscope.db
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("获取 appdata 目录失败");
            std::fs::create_dir_all(&data_dir).ok();
            let db = Db::open(&data_dir.join("lagscope.db"))
                .expect("打开事件数据库失败");

            let state = Arc::new(AppState::new(db));
            app.manage(state.clone());
            collectors::start(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_processes,
            commands::get_latest_sample,
            commands::get_disks,
            commands::kill_process,
            commands::list_lag_events,
            commands::diagnose_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
