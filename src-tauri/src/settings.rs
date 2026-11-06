//! 设置:持久化到 tauri-plugin-store(settings.json),
//! 保存时同步到运行中的检测器(阈值热更新)与自启注册。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_store::StoreBuilder;

use crate::engine::DetectorConfig;
use crate::state::AppState;

const STORE_FILE: &str = "settings.json";
const STORE_KEY: &str = "settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // 卡顿检测阈值
    pub cpu_pct: f64,
    pub cpu_sustain_s: u64,
    pub commit_pct: f64,
    pub page_input: f64,
    pub disk_transfer_ms: f64,
    pub disk_sustain_s: u64,
    pub interrupt_pct: f64,
    pub cooldown_s: u64,
    // 行为
    pub close_to_tray: bool,
    pub autostart: bool,
    /// PresentMon 帧卡顿监测
    pub enable_frame_monitor: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let c = DetectorConfig::default();
        Self {
            cpu_pct: c.cpu_pct as f64,
            cpu_sustain_s: c.cpu_sustain_s,
            commit_pct: c.commit_pct,
            page_input: c.page_input,
            disk_transfer_ms: c.disk_sec_transfer * 1000.0,
            disk_sustain_s: c.disk_sustain_s,
            interrupt_pct: c.interrupt_pct,
            cooldown_s: c.cooldown_ms / 1000,
            close_to_tray: true,
            autostart: false,
            enable_frame_monitor: true,
        }
    }
}

impl Settings {
    pub fn to_detector_config(&self) -> DetectorConfig {
        DetectorConfig {
            cpu_pct: self.cpu_pct as f32,
            cpu_sustain_s: self.cpu_sustain_s,
            commit_pct: self.commit_pct,
            page_input: self.page_input,
            disk_sec_transfer: self.disk_transfer_ms / 1000.0,
            disk_sustain_s: self.disk_sustain_s,
            interrupt_pct: self.interrupt_pct,
            interrupt_sustain_s: 5,
            stutter_gap_ms: 100.0,
            cooldown_ms: self.cooldown_s * 1000,
        }
    }
}

fn open_store(
    app: &AppHandle,
) -> Result<std::sync::Arc<tauri_plugin_store::Store<tauri::Wry>>, String> {
    StoreBuilder::new(app, STORE_FILE)
        .build()
        .map_err(|e| format!("打开设置存储失败: {e}"))
}

/// 启动时读取(缺省或损坏时回落默认值)
pub fn load_settings(app: &AppHandle) -> Settings {
    let Ok(store) = open_store(app) else {
        return Settings::default();
    };
    store
        .get(STORE_KEY)
        .and_then(|v| serde_json::from_value::<Settings>(v.clone()).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(load_settings(&app))
}

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    settings: Settings,
) -> Result<(), String> {
    // 持久化
    let store = open_store(&app)?;
    let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store.set(STORE_KEY, value);
    store.save().map_err(|e| format!("保存设置失败: {e}"))?;

    // 热更新检测器
    *state.config.lock().unwrap() = settings.to_detector_config();
    *state.close_to_tray.lock().unwrap() = settings.close_to_tray;

    // 帧监测开关
    crate::commands::set_frame_monitor(&app, &state, settings.enable_frame_monitor);

    // 开机自启
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if settings.autostart {
        autolaunch.enable().map_err(|e| format!("设置自启失败: {e}"))?;
    } else {
        autolaunch.disable().map_err(|e| format!("关闭自启失败: {e}"))?;
    }
    Ok(())
}
