//! 应用共享状态:采集线程写入,命令层读取。

use std::sync::Mutex;

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
    pub processes: Mutex<Vec<ProcessInfo>>,
    /// 最新系统采样(1Hz 更新)
    pub last_sample: Mutex<Option<MetricSample>>,
    /// 磁盘静态容量(30s 更新)
    pub disks: Mutex<Vec<DiskInfo>>,
    /// 卡顿事件库
    pub db: Db,
    /// 检测阈值(设置页热更新)
    pub config: Mutex<DetectorConfig>,
    /// 关窗是否隐藏到托盘
    pub close_to_tray: Mutex<bool>,
    /// PresentMon 帧监测(Some = 运行中)
    pub frame_monitor: Mutex<Option<PresentMonWatch>>,
    /// ETW DPC 归因(需管理员;失败为 None)
    pub dpc_watch: Mutex<Option<DpcWatch>>,
}

impl AppState {
    pub fn new(db: Db, config: DetectorConfig, close_to_tray: bool) -> Self {
        Self {
            // System::new 失败意味着连基本内存信息都无法初始化,直接 panic
            sys: Mutex::new(System::new().expect("sysinfo System::new 失败")),
            processes: Mutex::new(Vec::new()),
            last_sample: Mutex::new(None),
            disks: Mutex::new(Vec::new()),
            db,
            config: Mutex::new(config),
            close_to_tray: Mutex::new(close_to_tray),
            frame_monitor: Mutex::new(None),
            dpc_watch: Mutex::new(None),
        }
    }
}
