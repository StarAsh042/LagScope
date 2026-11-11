//! 卡顿事件 → 系统通知(Windows Toast)。
//!
//! 只在用户没在看主窗口时打扰:窗口可见且在前台时,界面上已经有红色进度条
//! 与时间线条目,再弹一条系统通知是重复打扰。

use tauri::Manager;

use crate::engine::LagEvent;

/// 通知正文最多列出几条归因,其余折叠成「等 N 项」
pub const MAX_BODY_REASONS: usize = 3;

/// 是否该弹通知
///
/// - 用户在设置里关掉 → 不弹
/// - 主窗口可见且在前台 → 不弹(界面上已经能看到)
/// - 窗口隐藏到托盘 / 最小化 / 被其他窗口盖住 → 弹
pub fn should_notify(enabled: bool, window_visible: bool, window_focused: bool) -> bool {
    enabled && !(window_visible && window_focused)
}

/// 组装通知的标题与正文
///
/// 正文只列前 MAX_BODY_REASONS 条归因:Windows 的 Toast 在通知中心里
/// 最多显示两行,把九条原因全塞进去只会被截断成半句话。
pub fn compose(event: &LagEvent) -> (String, String) {
    let title = "检测到卡顿".to_string();
    let mut parts: Vec<String> = event
        .reasons
        .iter()
        .take(MAX_BODY_REASONS)
        .map(|r| r.detail.clone())
        .collect();
    if parts.is_empty() {
        // 理论上不会走到:检测器只在有归因时才构造事件
        parts.push("系统负载异常".to_string());
    }
    let mut body = parts.join(" · ");
    if event.reasons.len() > MAX_BODY_REASONS {
        body.push_str(&format!(" 等 {} 项", event.reasons.len()));
    }
    (title, body)
}

/// 弹一条系统通知
///
/// 失败只影响这一条提示,采集与入库照常进行 —— 通知不是关键路径。
pub fn notify_event(app: &tauri::AppHandle, enabled: bool, event: &LagEvent) {
    if !enabled {
        return;
    }
    let window = app.get_webview_window("main");
    let visible = window
        .as_ref()
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    let focused = window
        .as_ref()
        .map(|w| w.is_focused().unwrap_or(false))
        .unwrap_or(false);
    if !should_notify(enabled, visible, focused) {
        return;
    }

    use tauri_plugin_notification::NotificationExt;
    let (title, body) = compose(event);
    if let Err(_e) = app.notification().builder().title(title).body(body).show() {
        #[cfg(debug_assertions)]
        eprintln!("[notify] 发送通知失败: {_e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::MetricSample;
    use crate::engine::LagReason;

    fn event(reasons: Vec<(&str, &str)>) -> LagEvent {
        LagEvent {
            id: Some(1),
            ts_ms: 1_793_000_000_000,
            reasons: reasons
                .into_iter()
                .map(|(kind, detail)| LagReason {
                    kind: kind.to_string(),
                    detail: detail.to_string(),
                    value: 1.0,
                })
                .collect(),
            counters: MetricSample::default(),
            top_cpu: Vec::new(),
            top_mem: Vec::new(),
            top_disk: Vec::new(),
            hung_windows: Vec::new(),
            game: None,
            dpc_top: None,
        }
    }

    #[test]
    fn single_reason_goes_to_body_verbatim() {
        let (title, body) = compose(&event(vec![("cpu", "CPU 总占用 95% 持续 10 秒")]));
        assert_eq!(title, "检测到卡顿");
        assert_eq!(body, "CPU 总占用 95% 持续 10 秒");
        assert!(!body.contains("等"), "只有一条时不该出现折叠提示");
    }

    #[test]
    fn reasons_beyond_the_cap_are_folded_into_a_count() {
        let (_, body) = compose(&event(vec![
            ("cpu", "原因一"),
            ("mem_pressure", "原因二"),
            ("page_faults", "原因三"),
            ("disk_latency", "原因四"),
            ("interrupt", "原因五"),
        ]));
        assert!(body.starts_with("原因一 · 原因二 · 原因三"), "正文: {body}");
        assert!(body.ends_with("等 5 项"), "正文: {body}");
        assert!(!body.contains("原因四"), "超出上限的归因不该进正文");
    }

    #[test]
    fn exactly_at_the_cap_has_no_fold_hint() {
        let (_, body) = compose(&event(vec![
            ("cpu", "甲"),
            ("mem_pressure", "乙"),
            ("page_faults", "丙"),
        ]));
        assert_eq!(body, "甲 · 乙 · 丙");
    }

    #[test]
    fn empty_reason_list_still_produces_a_readable_body() {
        let (_, body) = compose(&event(Vec::new()));
        assert_eq!(body, "系统负载异常");
    }

    #[test]
    fn enabled_flag_alone_is_not_enough() {
        // 用户没关通知,但正盯着主窗口 —— 不弹
        assert!(!should_notify(true, true, true));
    }

    #[test]
    fn notifies_when_window_is_hidden_or_unfocused() {
        assert!(should_notify(true, false, false), "隐藏到托盘时该弹");
        assert!(should_notify(true, false, true), "不可见即视为没在看");
        assert!(should_notify(true, true, false), "被其他窗口盖住时该弹");
    }

    #[test]
    fn disabled_setting_suppresses_every_notification() {
        assert!(!should_notify(false, false, false));
        assert!(!should_notify(false, true, true));
    }
}
