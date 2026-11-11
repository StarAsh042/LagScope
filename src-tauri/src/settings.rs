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
    pub page_sustain_s: u64,
    pub mem_sustain_s: u64,
    pub disk_transfer_ms: f64,
    pub disk_sustain_s: u64,
    pub interrupt_pct: f64,
    pub interrupt_sustain_s: u64,
    /// 帧间隔尖峰判定(ms):PresentMon 侧统计卡帧与检测器侧判帧卡顿共用
    pub stutter_gap_ms: f64,
    /// 每秒 DPC 总数阈值
    pub dpc_total_threshold: f64,
    pub cooldown_s: u64,
    /// 事件库保留条数(超出后自动删最旧的)
    pub retain_rows: u32,
    // 行为
    pub close_to_tray: bool,
    pub autostart: bool,
    /// PresentMon 帧卡顿监测
    pub enable_frame_monitor: bool,
    /// ETW DPC 驱动归因(需管理员)
    pub enable_dpc_watch: bool,
    /// 检测到卡顿时弹系统通知
    pub notify_lag: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let c = DetectorConfig::default();
        Self {
            cpu_pct: c.cpu_pct as f64,
            cpu_sustain_s: c.cpu_sustain_s,
            commit_pct: c.commit_pct,
            page_input: c.page_input,
            page_sustain_s: c.page_sustain_s,
            mem_sustain_s: c.mem_sustain_s,
            disk_transfer_ms: c.disk_sec_transfer * 1000.0,
            disk_sustain_s: c.disk_sustain_s,
            interrupt_pct: c.interrupt_pct,
            interrupt_sustain_s: c.interrupt_sustain_s,
            stutter_gap_ms: c.stutter_gap_ms,
            dpc_total_threshold: c.dpc_total_threshold,
            cooldown_s: c.cooldown_ms / 1000,
            retain_rows: crate::store::db::RETAIN_ROWS,
            close_to_tray: true,
            autostart: false,
            enable_frame_monitor: true,
            enable_dpc_watch: true,
            notify_lag: true,
        }
    }
}

/// 正阈值兜底:设置文件是明文 JSON,被手改成 0 会让判定恒真(帧间隔 >= 0
/// 全部算卡帧、DPC >= 0 每秒都算风暴)。0/负数/NaN 一律回落默认值。
fn positive_or(v: f64, fallback: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        fallback
    }
}

impl Settings {
    pub fn to_detector_config(&self) -> DetectorConfig {
        let d = DetectorConfig::default();
        DetectorConfig {
            cpu_pct: self.cpu_pct as f32,
            cpu_sustain_s: self.cpu_sustain_s,
            commit_pct: self.commit_pct,
            page_input: self.page_input,
            page_sustain_s: self.page_sustain_s,
            mem_sustain_s: self.mem_sustain_s,
            disk_sec_transfer: self.disk_transfer_ms / 1000.0,
            disk_sustain_s: self.disk_sustain_s,
            interrupt_pct: self.interrupt_pct,
            interrupt_sustain_s: self.interrupt_sustain_s.max(1),
            stutter_gap_ms: positive_or(self.stutter_gap_ms, d.stutter_gap_ms),
            dpc_total_threshold: positive_or(self.dpc_total_threshold, d.dpc_total_threshold),
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
    *state.notify_lag.lock().unwrap() = settings.notify_lag;
    // 事件库保留条数(改小时顺带立刻裁剪一次,不必等下次写入)
    let _ = state.db.set_retain_rows(settings.retain_rows);

    // 帧监测开关(帧间隔阈值随设置热更新,不必重启 PresentMon)
    crate::commands::set_frame_monitor(
        &app,
        &state,
        settings.enable_frame_monitor,
        settings.stutter_gap_ms,
    );
    // DPC 归因开关
    crate::commands::set_dpc_watch(&state, settings.enable_dpc_watch);

    // 开机自启
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if settings.autostart {
        autolaunch
            .enable()
            .map_err(|e| format!("设置自启失败: {e}"))?;
    } else {
        autolaunch
            .disable()
            .map_err(|e| format!("关闭自启失败: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归:旧版 settings.json 只有 10 个阈值字段,反序列化后新增的三个
    /// 阈值必须回落到默认值,不能因为缺字段就整体失败回到全默认。
    #[test]
    fn legacy_settings_without_new_thresholds_uses_defaults() {
        let legacy = serde_json::json!({
            "cpuPct": 70.0,
            "cpuSustainS": 8,
            "commitPct": 88.0,
            "pageInput": 400.0,
            "pageSustainS": 4,
            "memSustainS": 4,
            "diskTransferMs": 30.0,
            "diskSustainS": 4,
            "interruptPct": 25.0,
            "cooldownS": 90,
            "closeToTray": false,
            "autostart": true,
            "enableFrameMonitor": false,
            "enableDpcWatch": false
        });
        let s: Settings = serde_json::from_value(legacy).expect("旧设置应能反序列化");
        assert_eq!(s.cpu_pct, 70.0, "已有字段必须原样保留");
        assert_eq!(s.cooldown_s, 90);
        let d = DetectorConfig::default();
        assert_eq!(s.interrupt_sustain_s, d.interrupt_sustain_s);
        assert_eq!(s.stutter_gap_ms, d.stutter_gap_ms);
        assert_eq!(s.dpc_total_threshold, d.dpc_total_threshold);
    }

    /// 三个阈值必须真正流到 DetectorConfig(修复前是硬编码 5 / 100 / 8000,
    /// 设置页怎么改都不生效)。
    #[test]
    fn custom_thresholds_reach_detector_config() {
        let s = Settings {
            interrupt_sustain_s: 9,
            stutter_gap_ms: 55.5,
            dpc_total_threshold: 12_345.0,
            ..Settings::default()
        };
        let c = s.to_detector_config();
        assert_eq!(c.interrupt_sustain_s, 9);
        assert_eq!(c.stutter_gap_ms, 55.5);
        assert_eq!(c.dpc_total_threshold, 12_345.0);
    }

    /// 旧版 settings.json 没有 notifyLag 字段时必须默认开启通知,
    /// 不能因为多了一个字段就让整份设置回落默认(用户调过的阈值会丢)。
    #[test]
    fn legacy_settings_without_notify_lag_defaults_to_on() {
        let legacy = serde_json::json!({
            "cpuPct": 70.0,
            "closeToTray": false
        });
        let s: Settings = serde_json::from_value(legacy).expect("旧设置应能反序列化");
        assert!(s.notify_lag, "缺字段时应默认开启通知");
        assert_eq!(s.cpu_pct, 70.0, "已有字段必须原样保留");
    }

    /// 旧版 settings.json 没有 retainRows 字段时必须回落到默认保留条数,
    /// 而不是整份设置被丢弃回到全默认(那样用户调过的阈值会丢)。
    #[test]
    fn legacy_settings_without_retain_rows_uses_default() {
        let legacy = serde_json::json!({
            "cpuPct": 70.0,
            "cpuSustainS": 8,
            "commitPct": 88.0,
            "pageInput": 400.0,
            "pageSustainS": 4,
            "memSustainS": 4,
            "diskTransferMs": 30.0,
            "diskSustainS": 4,
            "interruptPct": 25.0,
            "interruptSustainS": 9,
            "stutterGapMs": 55.5,
            "dpcTotalThreshold": 12345.0,
            "cooldownS": 90,
            "closeToTray": false,
            "autostart": true,
            "enableFrameMonitor": false,
            "enableDpcWatch": false
        });
        let s: Settings = serde_json::from_value(legacy).expect("旧设置应能反序列化");
        assert_eq!(s.cpu_pct, 70.0, "已有字段必须原样保留");
        assert_eq!(
            s.retain_rows,
            crate::store::db::RETAIN_ROWS,
            "缺 retainRows 字段应回落默认保留条数"
        );
    }

    /// 保留条数必须原样透传到设置对象,不能被夹断或改写(0 的夹取在 Db 里做)。
    #[test]
    fn custom_retain_rows_roundtrip_via_json() {
        let s = Settings {
            retain_rows: 2000,
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).expect("序列化应成功");
        assert!(json.contains("\"retainRows\":2000"), "字段名须为 camelCase");
        let back: Settings = serde_json::from_str(&json).expect("反序列化应成功");
        assert_eq!(back.retain_rows, 2000);
    }

    /// 手改成 0 / 负数 / NaN 时不能让判定恒真,回落默认值。
    #[test]
    fn degenerate_thresholds_fall_back_to_defaults() {
        let d = DetectorConfig::default();
        for bad in [0.0, -5.0, f64::NAN] {
            let s = Settings {
                stutter_gap_ms: bad,
                dpc_total_threshold: bad,
                interrupt_sustain_s: 0,
                ..Settings::default()
            };
            let c = s.to_detector_config();
            assert_eq!(c.stutter_gap_ms, d.stutter_gap_ms, "stutter_gap_ms={bad}");
            assert_eq!(c.dpc_total_threshold, d.dpc_total_threshold, "dpc={bad}");
            assert!(c.interrupt_sustain_s >= 1, "持续秒数至少 1");
        }
    }
}
