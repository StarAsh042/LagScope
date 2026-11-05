//! Tauri 命令层:前端 invoke 的入口。

use sysinfo::Pid;

use tauri::State;

use crate::collectors::{DiskInfo, MetricSample, ProcessInfo};
use crate::state::AppState;

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
