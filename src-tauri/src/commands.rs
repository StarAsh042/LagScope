//! Tauri 命令层:前端 invoke 的入口。

use sysinfo::Pid;

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager, State};

use crate::collectors::hung_window;
use crate::collectors::presentmon::PresentMonWatch;
use crate::collectors::{DiskInfo, MetricSample, ProcessInfo};
use crate::engine::{LagEvent, LagReason};
use crate::state::AppState;

/// 启动/停止 PresentMon 帧监测(设置开关与启动流程共用)
pub fn set_frame_monitor(app: &AppHandle, state: &std::sync::Arc<AppState>, enable: bool) {
    let mut fm = state.frame_monitor.lock().unwrap();
    if enable {
        if fm.is_none() {
            match app.path().resolve("extras/PresentMon.exe", BaseDirectory::Resource) {
                Ok(p) if p.exists() => {
                    *fm = Some(PresentMonWatch::start(&p));
                }
                Ok(p) => eprintln!("[presentmon] 未找到 PresentMon.exe: {}", p.display()),
                Err(e) => eprintln!("[presentmon] 解析资源路径失败: {e}"),
            }
        }
    } else {
        // Drop 会终止子进程
        *fm = None;
    }
}

/// 最新进程快照(1Hz 由采集线程刷新,进程表每 2s 拉取)
#[tauri::command]
pub fn get_processes(state: State<'_, std::sync::Arc<AppState>>) -> Vec<ProcessInfo> {
    state.processes.lock().unwrap().clone()
}

/// 最新系统采样
#[tauri::command]
pub fn get_latest_sample(state: State<'_, std::sync::Arc<AppState>>) -> Option<MetricSample> {
    state.last_sample.lock().unwrap().clone()
}

/// 磁盘静态容量
#[tauri::command]
pub fn get_disks(state: State<'_, std::sync::Arc<AppState>>) -> Vec<DiskInfo> {
    state.disks.lock().unwrap().clone()
}

/// 结束进程(对应任务管理器的"结束任务")
#[tauri::command]
pub fn kill_process(state: State<'_, std::sync::Arc<AppState>>, pid: u32) -> Result<bool, String> {
    let mut sys = state.sys.lock().unwrap();
    // 先补一次进程列表,保证目标进程可见
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    match sys.process(Pid::from_u32(pid)) {
        Some(proc) => Ok(proc.kill()),
        None => Err(format!("进程 {pid} 不存在或已退出")),
    }
}

/// 最近的卡顿事件列表(新→旧)
#[tauri::command]
pub fn list_lag_events(
    state: State<'_, std::sync::Arc<AppState>>,
    limit: Option<u32>,
) -> Result<Vec<LagEvent>, String> {
    state.db.list_events(limit.unwrap_or(100))
}

/// 立即诊断:抓取当前状态生成一次事件并推送前端(命令与托盘菜单共用)
pub fn run_diagnose(
    app: &tauri::AppHandle,
    state: &std::sync::Arc<AppState>,
) -> Result<LagEvent, String> {
    let counters = state
        .last_sample
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "暂无采样数据".to_string())?;
    let processes = state.processes.lock().unwrap().clone();
    let hung = hung_window::detect_hung_windows();
    let game = state
        .frame_monitor
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|w| w.snapshot());
    // 手动诊断只如实记录当前状态;若没有任何自动阈值命中,标记为 manual
    let reasons = vec![LagReason {
        kind: "manual".into(),
        detail: "手动触发诊断快照".into(),
        value: 0.0,
    }];
    let mut event = LagEvent::build(counters.ts_ms, reasons, counters, &processes, hung, game);
    let id = state.db.insert_event(&event)?;
    event.id = Some(id);
    let _ = tauri::Emitter::emit(app, crate::collectors::EVENT_LAG, &event);
    Ok(event)
}

/// 立即诊断:抓取当前状态生成一次事件(不入库时也返回给前端展示)
#[tauri::command]
pub fn diagnose_now(
    app: tauri::AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
) -> Result<LagEvent, String> {
    run_diagnose(&app, &state)
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    use sysinfo::Pid;

    /// 用一个短命子进程验证 refresh_processes(Some)+kill 的 sysinfo 通路
    #[test]
    fn kill_path_smoke() {
        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 > nul"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sacrificial process");
        let pid = child.id();

        let mut sys = sysinfo::System::new().expect("System::new");
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
        let killed = sys
            .process(Pid::from_u32(pid))
            .map(|p| p.kill())
            .expect("target process visible");
        assert!(killed, "kill() 返回 false");
        let _ = child.wait();
    }
}
