//! 指标采集层:后台线程 1Hz 轮询 sysinfo + PDH,
//! 产出 [`MetricSample`](实时曲线)与 [`ProcessInfo`](进程表)缓存。

pub mod dpc_etw;
pub mod hung_window;
pub mod pdh;
pub mod presentmon;

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

use tauri::{AppHandle, Emitter};

use crate::engine::LagDetector;
use crate::logging;
use crate::state::AppState;

pub const COLLECT_INTERVAL: Duration = Duration::from_millis(1000);
pub const EVENT_METRICS: &str = "metrics:sample";
pub const EVENT_LAG: &str = "lag:event";
pub const EVENT_GAME: &str = "game:sample";
pub const EVENT_DPC: &str = "dpc:sample";

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
    /// 内存换页(从磁盘读页)速率 页/秒
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

/// 单块磁盘卷(逻辑磁盘)的读写速率 B/s
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeIo {
    pub mount: String,
    pub read_bps: u64,
    pub write_bps: u64,
}

/// 单个网卡的收发速率 B/s
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NicIo {
    pub name: String,
    pub rx_bps: u64,
    pub tx_bps: u64,
}

/// I/O 细分快照(每 1Hz 更新,单独命令拉取,不进入 metrics:sample 推送)
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IoBreakdown {
    pub volumes: Vec<VolumeIo>,
    pub nics: Vec<NicIo>,
}

/// 把累计计数的差分换算为 B/s,并在计数器回绕/重置时归零。
/// 公开仅供测试使用,采集线程每秒调用。
pub fn rate_per_sec(cur: u64, prev: u64) -> u64 {
    cur.saturating_sub(prev)
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
    disk_instance_read: Option<pdh::Counter>,
    disk_instance_write: Option<pdh::Counter>,
    dpc_per_sec: Option<pdh::Counter>,
    interrupt_pct: Option<pdh::Counter>,
    gpu_util: Option<pdh::Counter>,
    gpu_mem: Option<pdh::Counter>,
    thermal: Option<pdh::Counter>,
}

impl<'a> PdhSet<'a> {
    fn create(q: &'a pdh::PdhQuery) -> Self {
        fn add(q: &pdh::PdhQuery, path: &str) -> Option<pdh::Counter> {
            match q.add(path) {
                Ok(c) => Some(c),
                Err(e) => {
                    logging::warn("pdh", format!("跳过计数器 {path}: {e}"));
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
            disk_instance_read: add(q, r"\LogicalDisk(*)\Disk Read Bytes/sec"),
            disk_instance_write: add(q, r"\LogicalDisk(*)\Disk Write Bytes/sec"),
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
            thermal: add(q, r"\Thermal Zone Information(*)\Temperature"),
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
            logging::warn("pdh", format!("打开查询失败,系统级计数器不可用: {e}"));
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
    let mut prev_nic: HashMap<String, (u64, u64)> = HashMap::new();
    let mut detector = LagDetector::new();
    // 挂起窗口检测在独立线程(检测调用最坏阻塞 2s/窗口,不能占用采样线程)
    let hung_watch = hung_window::HungWatch::start();
    // ETW DPC 归因由设置层启停(state.dpc_watch),这里只读快照
    thread::sleep(COLLECT_INTERVAL);

    let mut tick: u64 = 0;
    let mut cur_nic: HashMap<String, (u64, u64)> = HashMap::with_capacity(networks.len());
    loop {
        let t0 = Instant::now();

        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        // 网络:累计值差分(Networks Deref 到 HashMap,迭代器产出 (接口名, NetworkData))
        networks.refresh(true);
        let mut net_rx_total = 0u64;
        let mut net_tx_total = 0u64;
        cur_nic.clear();
        for (name, data) in networks.iter() {
            let rx = data.received();
            let tx = data.transmitted();
            net_rx_total += rx;
            net_tx_total += tx;
            cur_nic.insert(name.to_string_lossy().into_owned(), (rx, tx));
        }
        let nics: Vec<NicIo> = cur_nic
            .iter()
            .filter_map(|(name, (rx, tx))| {
                let (pr, pt) = prev_nic.get(name).copied().unwrap_or((*rx, *tx));
                let (rx_bps, tx_bps) = (rate_per_sec(*rx, pr), rate_per_sec(*tx, pt));
                (rx_bps > 0 || tx_bps > 0).then(|| NicIo {
                    name: name.clone(),
                    rx_bps,
                    tx_bps,
                })
            })
            .collect();
        prev_nic = cur_nic.clone();
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
                    (du.total_read_bytes - pr, du.total_written_bytes - pw)
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

        // 每网卡差分结果写入缓存,由前端按需拉取
        *state.nics.lock().unwrap() = nics;

        // 每卷读写速率从 PDH 通配符计数器读取(_Total 实例不计入细分)
        if let Some(cs) = pdh_set.as_ref() {
            let read_rates: HashMap<String, u64> = cs.disk_instance_read.as_ref()
                .map(|c| cs.q.read_instances(c).into_iter().map(|(k, v)| (k, v.max(0.0) as u64)).collect())
                .unwrap_or_default();
            let write_rates: HashMap<String, u64> = cs.disk_instance_write.as_ref()
                .map(|c| cs.q.read_instances(c).into_iter().map(|(k, v)| (k, v.max(0.0) as u64)).collect())
                .unwrap_or_default();
            let volumes: Vec<VolumeIo> = read_rates
                .keys()
                .chain(write_rates.keys())
                .filter(|k| k.as_str() != "_Total")
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(|k| VolumeIo {
                    mount: k.clone(),
                    read_bps: read_rates.get(k).copied().unwrap_or(0),
                    write_bps: write_rates.get(k).copied().unwrap_or(0),
                })
                .collect();
            *state.volumes.lock().unwrap() = volumes;
        }

        // 缓存与检测引擎共用同一份数据:每 tick 只有两次 Arc 引用计数自增,
        // 不再各自深拷贝 300 个进程快照
        let snapshot = Arc::new(processes);
        *state.processes.lock().unwrap() = Arc::clone(&snapshot);
        let sample = Arc::new(sample);
        *state.last_sample.lock().unwrap() = Some(Arc::clone(&sample));
        let _ = app.emit(EVENT_METRICS, sample.as_ref());

        // 卡顿检测引擎:命中阈值 → 快照入库 + 推送前端(阈值可被设置页热更新)
        let cfg = state.config.lock().unwrap().clone();
        let notify_lag = *state.notify_lag.lock().unwrap();
        // 新鲜度守卫:监测器意外死亡后快照会冻结,3 秒内的数据才有效
        let now = sample.ts_ms;
        let game = {
            let fm = state.frame_monitor.lock().unwrap();
            fm.as_ref()
                .and_then(|w| w.snapshot())
                .filter(|g| now.saturating_sub(g.ts_ms) < 3000)
        };
        if let Some(g) = &game {
            let _ = app.emit(EVENT_GAME, g);
        }
        let dpc = {
            let w = state.dpc_watch.lock().unwrap();
            w.as_ref()
                .and_then(|w| w.snapshot())
                .filter(|d| now.saturating_sub(d.ts_ms) < 3000)
        };
        if let Some(d) = &dpc {
            let _ = app.emit(EVENT_DPC, d);
        }
        if let Some(event) = detector.feed(
            sample.as_ref(),
            snapshot.as_slice(),
            &hung_watch.snapshot(),
            game.as_ref(),
            dpc.as_ref(),
            &cfg,
        ) {
            let mut ev = event;
            match state.db.insert_event(&ev) {
                Ok(id) => {
                    ev.id = Some(id);
                    logging::debug(
                        "lag",
                        format!(
                            "触发: {:?}",
                            ev.reasons.iter().map(|r| &r.kind).collect::<Vec<_>>()
                        ),
                    );
                    // 系统通知:窗口在前台时不打扰,失败也不影响入库与界面
                    crate::notify::notify_event(&app, notify_lag, &ev);
                    let _ = app.emit(EVENT_LAG, &ev);
                }
                Err(e) => {
                    logging::error("db", format!("事件入库失败: {e}"));
                }
            }
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
    sample.disk_avg_sec_transfer = cs
        .disk_sec_transfer
        .and_then(|c| cs.q.read(&c))
        .unwrap_or(0.0);
    sample.disk_queue_length = cs.disk_queue.and_then(|c| cs.q.read(&c)).unwrap_or(0.0);
    // % Idle Time 反转为活跃占比
    sample.disk_active_pct = (100.0
        - cs.disk_idle_pct
            .and_then(|c| cs.q.read(&c))
            .unwrap_or(100.0))
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

    // 热区温度:PDH cooked 值为开尔文(实测 329K≈55.9℃,与 WMI MSAcpi 一致),取最热区
    if let Some(c) = &cs.thermal {
        let max_k =
            cs.q.read_instances(c)
                .iter()
                .map(|(_, v)| *v)
                .fold(0.0_f64, f64::max);
        if max_k > 100.0 {
            sample.temperature_c = Some((max_k - 273.15) as f32);
        }
    }

    sample
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_per_sec_handles_normal_and_reset() {
        assert_eq!(rate_per_sec(1_000, 400), 600);
        assert_eq!(rate_per_sec(400, 1_000), 0, "计数器回绕应归零而非下溢");
        assert_eq!(rate_per_sec(500, 500), 0);
    }

    /// 无 PDH 计数器时,系统级指标仍从 sysinfo 正确填充,PDH 字段保持默认。
    #[test]
    fn build_sample_without_counters_uses_sysinfo() {
        let mut sys = System::new().expect("创建 System 失败");
        sys.refresh_all();
        let sample = build_sample(&sys, None, 123, 456);

        assert!(sample.ts_ms > 0, "时间戳应为 epoch 毫秒");
        assert!(sample.cpu_total >= 0.0 && sample.cpu_total <= 100.0);
        assert_eq!(sample.cpu_per_core.len(), sys.cpus().len());
        assert_eq!(sample.net_rx_bps, 123);
        assert_eq!(sample.net_tx_bps, 456);
        assert_eq!(sample.mem_total, sys.total_memory());
        // PDH 相关字段在无计数器时应保持默认,不应出现幽灵数值
        assert_eq!(sample.mem_commit_used, 0);
        assert_eq!(sample.page_input_per_sec, 0.0);
        assert_eq!(sample.disk_read_bps, 0);
        assert!(sample.gpu_util_pct.is_none());
        assert!(sample.temperature_c.is_none());
    }

    #[test]
    fn io_breakdown_serializes_camel_case() {
        let v = VolumeIo { mount: "C:".into(), read_bps: 1, write_bps: 2 };
        let n = NicIo { name: "以太网".into(), rx_bps: 3, tx_bps: 4 };
        let b = IoBreakdown { volumes: vec![v], nics: vec![n] };
        let json = serde_json::to_value(&b).unwrap();
        assert_eq!(json["volumes"][0]["readBps"], 1);
        assert_eq!(json["nics"][0]["txBps"], 4);
    }
}
