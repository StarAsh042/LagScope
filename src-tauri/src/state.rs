//! 应用共享状态:采集线程写入,命令层读取。

use std::sync::{Arc, Mutex};

use sysinfo::System;

use crate::collectors::dpc_etw::DpcWatch;
use crate::collectors::presentmon::PresentMonWatch;
use crate::collectors::{DiskInfo, MetricSample, ProcessInfo};
use crate::engine::DetectorConfig;
use crate::store::Db;

pub struct AppState {
    /// sysinfo System(采集线程刷新,kill 命令借用)
    pub sys: Mutex<System>,
    /// 最新进程快照缓存(1Hz 更新)
    ///
    /// 用 `Arc` 而不是裸 `Vec`:采集线程每 tick 都要刷新缓存,若原地
    /// `clone()` 一次即为 300 个 `ProcessInfo`(每个含一次 `String` 分配);
    /// 共享 `Arc` 后每 tick 只做一次引用计数自增,深拷贝推迟到真正
    /// 需要跨线程传递的读取方(命令层 / 检测引擎)。
    pub processes: Mutex<Arc<Vec<ProcessInfo>>>,
    /// 最新系统采样(1Hz 更新),同上用 `Arc` 避免每 tick 深拷贝
    pub last_sample: Mutex<Option<Arc<MetricSample>>>,
    /// 磁盘静态容量(30s 更新)
    pub disks: Mutex<Vec<DiskInfo>>,
    /// 卡顿事件库
    pub db: Db,
    /// 检测阈值(设置页热更新)
    pub config: Mutex<DetectorConfig>,
    /// 关窗是否隐藏到托盘
    pub close_to_tray: Mutex<bool>,
    /// 检测到卡顿时是否弹系统通知
    pub notify_lag: Mutex<bool>,
    /// PresentMon 帧监测(Some = 运行中)
    pub frame_monitor: Mutex<Option<PresentMonWatch>>,
    /// ETW DPC 归因(需管理员;失败为 None)
    pub dpc_watch: Mutex<Option<DpcWatch>>,
}

impl AppState {
    pub fn new(db: Db, config: DetectorConfig, close_to_tray: bool, notify_lag: bool) -> Self {
        Self {
            // System::new 失败意味着连基本内存信息都无法初始化,直接 panic
            sys: Mutex::new(System::new().expect("sysinfo System::new 失败")),
            processes: Mutex::new(Arc::new(Vec::new())),
            last_sample: Mutex::new(None),
            disks: Mutex::new(Vec::new()),
            db,
            config: Mutex::new(config),
            close_to_tray: Mutex::new(close_to_tray),
            notify_lag: Mutex::new(notify_lag),
            frame_monitor: Mutex::new(None),
            dpc_watch: Mutex::new(None),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.to_string(),
            cpu_pct: 0.0,
            mem_mb: 0.0,
            disk_read_bps: 0.0,
            disk_write_bps: 0.0,
            exe_path: None,
        }
    }

    /// 采集线程每秒换一份新快照,而命令层/前端请求方持有旧 Arc 时必须
    /// 看到稳定内容 —— 若这里退化成原地改写,读到一半的进程表会出现撕裂。
    #[test]
    fn readers_keep_a_stable_snapshot_across_updates() {
        let cache: Mutex<Arc<Vec<ProcessInfo>>> = Mutex::new(Arc::new(vec![proc(1, "one.exe")]));

        // 读取方拿到快照后立刻放锁
        let reader = cache.lock().unwrap().clone();
        // 采集线程随后换上全新的一代
        *cache.lock().unwrap() = Arc::new(vec![proc(2, "two.exe"), proc(3, "three.exe")]);

        assert_eq!(reader.len(), 1, "旧快照不应被后续写入改动");
        assert_eq!(reader[0].pid, 1);
        assert_eq!(reader[0].name, "one.exe");

        let fresh = cache.lock().unwrap().clone();
        assert_eq!(fresh.len(), 2);
        assert!(!Arc::ptr_eq(&reader, &fresh), "换代后不应仍是同一份数据");
    }

    /// 缓存换代只做引用计数自增:同一份快照被多处持有时不产生深拷贝。
    #[test]
    fn snapshot_sharing_does_not_deep_copy() {
        let snapshot = Arc::new(vec![proc(7, "game.exe")]);
        let cached: Mutex<Option<Arc<Vec<ProcessInfo>>>> = Mutex::new(Some(Arc::clone(&snapshot)));
        let from_cache = cached.lock().unwrap().clone().unwrap();

        assert!(Arc::ptr_eq(&snapshot, &from_cache));
        assert_eq!(Arc::strong_count(&snapshot), 3, "三方共享同一块数据");
    }
}
