//! LagScope 卡顿透视 —— 监测电脑卡顿的原因。

mod collectors;
mod commands;
mod engine;
mod settings;
mod state;
mod store;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

use state::AppState;
use store::Db;

/// 主窗口 显示/隐藏 切换(托盘点击)
fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 必须最先注册:二次启动时唤起已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_store::Builder::new().build())
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

            // 设置 → 检测阈值 / 关窗行为
            let settings = settings::load_settings(app.handle());
            let state = Arc::new(AppState::new(
                db,
                settings.to_detector_config(),
                settings.close_to_tray,
            ));
            app.manage(state.clone());
            collectors::start(app.handle().clone(), state);

            // 托盘:左键切换窗口,右键菜单
            let show = MenuItem::with_id(app, "show", "显示 / 隐藏窗口", true, None::<&str>)?;
            let diagnose = MenuItem::with_id(app, "diagnose", "立即诊断", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 LagScope", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &diagnose, &quit])?;
            TrayIconBuilder::with_id("lagscope-tray")
                .icon(app.default_window_icon().expect("缺少应用图标").clone())
                .tooltip("LagScope 卡顿透视")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "show" => toggle_main_window(app),
                    "diagnose" => {
                        let state = app.state::<Arc<AppState>>();
                        if let Err(e) = commands::run_diagnose(app, &state) {
                            eprintln!("[tray] 立即诊断失败: {e}");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗到托盘:由设置控制,进程保持后台采集
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().state::<Arc<AppState>>();
                if *state.close_to_tray.lock().unwrap() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_processes,
            commands::get_latest_sample,
            commands::get_disks,
            commands::kill_process,
            commands::list_lag_events,
            commands::diagnose_now,
            settings::get_settings,
            settings::set_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
