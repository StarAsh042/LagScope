//! 指标采集层:后台线程 1Hz 轮询 sysinfo + PDH,
//! 产出 [`MetricSample`](实时曲线)与 [`ProcessInfo`](进程表)缓存。

pub mod hung_window;
pub mod pdh;

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

use tauri::{AppHandle, Emitter};

use crate::engine::{DetectorConfig, LagDetector};
use crate::state::AppState;

pub const COLLECT_INTERVAL: Duration = Duration::from_millis(1000);
pub const EVENT_METRICS: &str = "metrics:sample";
pub const EVENT_LAG: &str = "lag:event";

/// 一次采样:前端 1Hz 接收,同时是卡顿检测引擎的输入
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSample {
    pub ts_ms: u64,
    /// 总 CPU 使用率 0-100
    pub cpu_total: f32,
    /// 每核使用率 0-100
    pub cpu_per_core: Vec<f32>,
    /// 平均主频 MHz
    pub cpu_freq_mhz: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    /// 提交内存(虚拟内存承诺)
    pub mem_commit_used: u64,
    pub mem_commit_limit: u64,
    /// 硬缺页(从磁盘读页)速率 页/秒
    pub page_input_per_sec: f64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    /// 磁盘活跃时间占比 0-100
    pub disk_active_pct: f64,
    /// 平均单次传输耗时 秒
    pub disk_avg_sec_transfer: f64,
    /// 平均磁盘队列长度
    pub disk_queue_length: f64,
    pub disk_total: u64,
    pub disk_free: u64,
    pub net_rx_bps: u64,
    pub net_tx_bps: u64,
    /// GPU 3D 引擎总占用 0-100(无 GPU 计数器时为 None)
    pub gpu_util_pct: Option<f64>,
    /// GPU 专用显存占用 MB(无 GPU 计数器时为 None)
    pub gpu_mem_mb: Option<f64>,
    /// DPC 速率 次/秒
    pub dpc_per_sec: f64,
    /// 中断时间占比 0-100
    pub interrupt_pct: f64,
    /// 传感器最高温度 ℃(取不到时为 None)
    pub temperature_c: Option<f32>,
}

/// 进程快照(供进程表命令读取)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub mem_mb: f64,
    /// ⚠️ sysinfo disk_usage 含全部 I/O(含网络),非纯磁盘,UI 需注明口径
    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub exe_path: Option<String>,
}

/// 物理磁盘静态信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub free: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 启动采集线程(应用退出时随进程结束)
pub fn start(app: AppHandle, state: std::sync::Arc<AppState>) {
    thread::Builder::new()
        .name("lag-monitor-collector".into())
        .spawn(move || run_loop(app, state))
        .expect("spawn collector thread");
}

/// PDH 计数器集合。任一添加失败仅记录并置 None,不影响其余指标。
struct PdhSet<'a> {
    q: &'a pdh::PdhQuery,
    commit_used: Option<pdh::Counter>,
    commit_limit: Option<pdh::Counter>,
    page_input: Option<pdh::Counter>,
    disk_read: Option<pdh::Counter>,
    disk_write: Option<pdh::Counter>,
    disk_sec_transfer: Option<pdh::Counter>,
    disk_queue: Option<pdh::Counter>,
    disk_idle_pct: Option<pdh::Counter>,
    dpc_per_sec: Option<pdh::Counter>,
    interrupt_pct: Option<pdh::Counter>,
    gpu_util: Option<pdh::Counter>,
    gpu_mem: Option<pdh::Counter>,
}

impl<'a> PdhSet<'a> {
    fn create(q: &'a pdh::PdhQuery) -> Self {
        fn add(q: &pdh::PdhQuery, path: &str) -> Option<pdh::Counter> {
            match q.add(path) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("[pdh] 跳过计数器 {path}: {e}");
                    None
                }
            }
        }
        /// 新版 Windows 将 `DPCs/sec` 改名为 `DPCs Queued/sec`,按优先级尝试
        fn add_first(q: &pdh::PdhQuery, paths: &[&str]) -> Option<pdh::Counter> {
            paths.iter().find_map(|p| add(q, p))
        }
        PdhSet {
            q,
            commit_used: add(q, r"\Memory\Committed Bytes"),
            commit_limit: add(q, r"\Memory\Commit Limit"),
            page_input: add(q, r"\Memory\Pages Input/sec"),
            disk_read: add(q, r"\PhysicalDisk(_Total)\Disk Read Bytes/sec"),
            disk_write: add(q, r"\PhysicalDisk(_Total)\Disk Write Bytes/sec"),
            disk_sec_transfer: add(q, r"\PhysicalDisk(_Total)\Avg. Disk sec/Transfer"),
            disk_queue: add(q, r"\PhysicalDisk(_Total)\Avg. Disk Queue Length"),
            disk_idle_pct: add(q, r"\PhysicalDisk(_Total)\% Idle Time"),
            dpc_per_sec: add_first(
                q,
                &[
                    r"\Processor(_Total)\DPCs Queued/sec",
                    r"\Processor(_Total)\DPCs/sec",
                ],
            ),
            interrupt_pct: add(q, r"\Processor(_Total)\% Interrupt Time"),
            gpu_util: add(q, r"\GPU Engine(*engtype_3D)\Utilization Percentage"),
            gpu_mem: add(q, r"\GPU Adapter Memory(*)\Dedicated Usage"),
        }
    }
}

fn run_loop(app: AppHandle, state: std::sync::Arc<AppState>) {
    // sysinfo 0.40 构造函数返回 Result,失败即采集线程不可用,直接终止线程
    let mut sys = System::new().expect("sysinfo System::new 失败");
    let mut networks = Networks::new_with_refreshed_list().expect("sysinfo Networks 初始化失败");
    let mut disks = Disks::new_with_refreshed_list().expect("sysinfo Disks 初始化失败");

    // PDH:打开失败或个别计数器缺失不影响其余采集
    let pdh_query = match pdh::PdhQuery::new() {
        Ok(q) => Some(q),
        Err(e) => {
            eprintln!("[pdh] 打开查询失败,系统级计数器不可用: {e}");
            None
        }
    };
    let pdh_set: Option<PdhSet<'_>> = pdh_query.as_ref().map(PdhSet::create);

    // 预热:sysinfo CPU 与 PDH 速率计数器都需要两次采样才有效
    sys.refresh_cpu_usage();
    if let Some(q) = &pdh_query {
        let _ = q.collect();
    }
    let mut prev_io: HashMap<u32, (u64, u64)> = HashMap::new();
    let mut prev_net: (u64, u64) = (0, 0);
    let mut detector = LagDetector::new(DetectorConfig::default());
    // 挂起窗口检测在独立线程(检测调用最坏阻塞 2s/窗口,不能占用采样线程)
    let hung_watch = hung_window::HungWatch::start();
    thread::sleep(COLLECT_INTERVAL);

    let mut tick: u64 = 0;
    loop {
        let t0 = Instant::now();

        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        // 网络:累计值差分(Networks Deref 到 HashMap,迭代器产出 (接口名, NetworkData))
        networks.refresh(true);
        let mut net_rx_total = 0u64;
        let mut net_tx_total = 0u64;
        for data in networks.values() {
            net_rx_total += data.received();
            net_tx_total += data.transmitted();
        }
        let (net_rx_bps, net_tx_bps) = (
            net_rx_total.saturating_sub(prev_net.0),
            net_tx_total.saturating_sub(prev_net.1),
        );
        prev_net = (net_rx_total, net_tx_total);

        // 进程信息 + I/O 差分
        let mut processes: Vec<ProcessInfo> = Vec::with_capacity(sys.processes().len());
        let mut cur_io: HashMap<u32, (u64, u64)> = HashMap::with_capacity(sys.processes().len());
        for (pid, proc) in sys.processes() {
            let du = proc.disk_usage();
            let pid32 = pid.as_u32();
            let (read_rate, write_rate) = match prev_io.get(&pid32) {
                Some((pr, pw)) if du.total_read_bytes >= *pr && du.total_written_bytes >= *pw => {
                    (
                        du.total_read_bytes - pr,
                        du.total_written_bytes - pw,
                    )
                }
                _ => (0, 0),
            };
            cur_io.insert(pid32, (du.total_read_bytes, du.total_written_bytes));
            processes.push(ProcessInfo {
                pid: pid32,
                name: proc.name().to_string_lossy().into_owned(),
                cpu_pct: proc.cpu_usage(),
                mem_mb: proc.memory() as f64 / 1024.0 / 1024.0,
                disk_read_bps: read_rate as f64,
                disk_write_bps: write_rate as f64,
                exe_path: proc.exe().and_then(|p| p.to_str()).map(String::from),
            });
        }
        prev_io = cur_io;

        // 磁盘静态容量:每 30 秒刷新一次(Disks 通过 Deref 到 Vec<Disk> 迭代)
        if tick.is_multiple_of(30) {
            disks.refresh(true);
            let infos: Vec<DiskInfo> = disks
                .iter()
                .map(|d| DiskInfo {
                    mount: d.mount_point().to_string_lossy().into_owned(),
                    total: d.total_space(),
                    free: d.available_space(),
                })
                .collect();
            *state.disks.lock().unwrap() = infos;
        }

        // PDH 采样并组装系统级指标
        if let Some(q) = &pdh_query {
            let _ = q.collect();
        }
        let sample = build_sample(&sys, pdh_set.as_ref(), net_rx_bps, net_tx_bps);

        *state.last_sample.lock().unwrap() = Some(sample.clone());
        *state.processes.lock().unwrap() = processes.clone();
        let _ = app.emit(EVENT_METRICS, &sample);

        // 卡顿检测引擎:命中阈值 → 快照入库 + 推送前端
        if let Some(event) = detector.feed(sample, &processes, &hung_watch.snapshot()) {
            let mut ev = event;
            match state.db.insert_event(&ev) {
                Ok(id) => ev.id = Some(id),
                Err(e) => eprintln!("[db] 事件入库失败: {e}"),
            }
            eprintln!("[lag] 触发: {:?}", ev.reasons.iter().map(|r| &r.kind).collect::<Vec<_>>());
            let _ = app.emit(EVENT_LAG, &ev);
        }

        tick += 1;
        let elapsed = t0.elapsed();
        if let Some(rest) = COLLECT_INTERVAL.checked_sub(elapsed) {
            thread::sleep(rest);
        }
    }
}

fn build_sample(
    sys: &System,
    counters: Option<&PdhSet<'_>>,
    net_rx_bps: u64,
    net_tx_bps: u64,
) -> MetricSample {
    let cpus = sys.cpus();
    let cpu_total = cpus.iter().map(|c| c.usage()).sum::<f32>() / cpus.len().max(1) as f32;
    let cpu_per_core: Vec<f32> = cpus.iter().map(|c| c.usage()).collect();
    let cpu_freq_mhz = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().map(|c| c.frequency() as f64).sum::<f64>() / cpus.len() as f64
    };

    let mut sample = MetricSample {
        ts_ms: now_ms(),
        cpu_total,
        cpu_per_core,
        cpu_freq_mhz,
        mem_used: sys.used_memory(),
        mem_total: sys.total_memory(),
        mem_commit_used: 0,
        mem_commit_limit: 0,
        page_input_per_sec: 0.0,
        disk_read_bps: 0,
        disk_write_bps: 0,
        disk_active_pct: 0.0,
        disk_avg_sec_transfer: 0.0,
        disk_queue_length: 0.0,
        disk_total: 0,
        disk_free: 0,
        net_rx_bps,
        net_tx_bps,
        gpu_util_pct: None,
        gpu_mem_mb: None,
        dpc_per_sec: 0.0,
        interrupt_pct: 0.0,
        temperature_c: None,
    };

    let Some(cs) = counters else {
        return sample;
    };

    sample.mem_commit_used = cs.commit_used.and_then(|c| cs.q.read(&c)).unwrap_or(0.0) as u64;
    sample.mem_commit_limit = cs.commit_limit.and_then(|c| cs.q.read(&c)).unwrap_or(0.0) as u64;
    sample.page_input_per_sec = cs.page_input.and_then(|c| cs.q.read(&c)).unwrap_or(0.0);

    sample.disk_read_bps = cs.disk_read.and_then(|c| cs.q.read(&c)).unwrap_or(0.0) as u64;
    sample.disk_write_bps = cs.disk_write.and_then(|c| cs.q.read(&c)).unwrap_or(0.0) as u64;
    sample.disk_avg_sec_transfer = cs.disk_sec_transfer.and_then(|c| cs.q.read(&c)).unwrap_or(0.0);
    sample.disk_queue_length = cs.disk_queue.and_then(|c| cs.q.read(&c)).unwrap_or(0.0);
    // % Idle Time 反转为活跃占比
    sample.disk_active_pct = (100.0 - cs.disk_idle_pct.and_then(|c| cs.q.read(&c)).unwrap_or(100.0))
        .clamp(0.0, 100.0);

    sample.dpc_per_sec = cs.dpc_per_sec.and_then(|c| cs.q.read(&c)).unwrap_or(0.0);
    sample.interrupt_pct = cs.interrupt_pct.and_then(|c| cs.q.read(&c)).unwrap_or(0.0);

    // GPU:全部 3D 引擎实例求和并封顶 100;显存为各适配器专用显存之和
    if let Some(c) = &cs.gpu_util {
        let sum: f64 = cs.q.read_instances(c).iter().map(|(_, v)| *v).sum();
        sample.gpu_util_pct = Some(sum.clamp(0.0, 100.0));
    }
    if let Some(c) = &cs.gpu_mem {
        let sum: f64 = cs.q.read_instances(c).iter().map(|(_, v)| *v).sum();
        sample.gpu_mem_mb = Some(sum / 1024.0 / 1024.0);
    }

    sample
}
