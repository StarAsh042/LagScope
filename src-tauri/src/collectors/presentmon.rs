//! PresentMon 子进程集成:ETW 抓取每帧呈现事件,输出 v1 CSV 到 stdout,
//! 按秒聚合成 [`GameSample`](以帧数最多的渲染进程为准)。
//!
//! PresentMon 2.3.1+ 采集 ETW 不再要求管理员权限;
//! 子进程随应用退出终止;stdout 逐行解析,无磁盘写入。

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::GameSample;

const STUTTER_GAP_MS: f64 = 100.0;

/// 每秒帧统计缓存 + 子进程句柄
pub struct PresentMonWatch {
    latest: Arc<Mutex<Option<GameSample>>>,
    child: Arc<Mutex<Option<Child>>>,
    /// PresentMon 是否成功跑起来(权限/会话冲突时为 false)
    available: Arc<AtomicBool>,
}

impl PresentMonWatch {
    /// 启动 PresentMon 子进程与 stdout 解析线程
    pub fn start(exe_path: &std::path::Path) -> Self {
        let watch = Self {
            latest: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            available: Arc::new(AtomicBool::new(false)),
        };

        match Command::new(exe_path)
            .args([
                "--output_stdout",
                "--v1_metrics",
                "--no_console_stats",
                "--stop_existing_session",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                let stdout = child.stdout.take().expect("piped stdout");
                watch.available.store(true, Ordering::Relaxed);
                *watch.child.lock().unwrap() = Some(child);

                let latest = watch.latest.clone();
                thread::Builder::new()
                    .name("presentmon-reader".into())
                    .spawn(move || parse_stream(stdout, latest))
                    .ok();
            }
            Err(e) => {
                eprintln!("[presentmon] 启动失败: {e}");
            }
        }
        watch
    }

    /// 最近一秒的帧统计(无渲染活动或未启动时为 None)
    pub fn snapshot(&self) -> Option<GameSample> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }
}

impl Drop for PresentMonWatch {
    fn drop(&mut self) {
        if let Some(child) = self.child.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// 逐行解析 CSV,按挂钟秒切换窗口聚合
fn parse_stream<R: std::io::Read + Send + 'static>(stdout: R, latest: Arc<Mutex<Option<GameSample>>>) {
    let reader = BufReader::new(stdout);
    // v1 CSV 列(部分):Application, ProcessID, ..., Dropped, ..., MsBetweenPresents, ...
    let mut col_app = None;
    let mut col_pid = None;
    let mut col_dropped = None;
    let mut col_gap = None;

    // 当前秒窗口:pid -> (app, gaps)
    let mut cur_sec = unix_sec();
    let mut agg: std::collections::HashMap<u32, (String, Vec<f64>)> =
        std::collections::HashMap::new();

    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.starts_with("Application,") {
            // 表头:解析列索引(大小写不敏感,兼容 1.x/2.x 命名差异)
            let cols: Vec<&str> = line.split(',').collect();
            let find = |name: &str| {
                cols.iter()
                    .position(|c| c.eq_ignore_ascii_case(name))
            };
            col_app = find("Application");
            col_pid = find("ProcessID");
            col_dropped = find("Dropped");
            col_gap = find("msBetweenPresents");
            continue;
        }
        if line.starts_with("PresentMon,") || line.starts_with("Summary,") {
            continue; // 底部统计行
        }
        let (Some(ca), Some(cp), Some(cd), Some(cg)) =
            (col_app, col_pid, col_dropped, col_gap)
        else {
            continue;
        };

        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() <= cg {
            continue;
        }
        let app = fields.get(ca).unwrap_or(&"-").trim().to_string();
        let pid: u32 = fields.get(cp).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let dropped: bool = fields.get(cd).and_then(|v| v.trim().parse().ok()).unwrap_or(false);
        let gap: f64 = fields
            .get(cg)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0.0);

        let sec = unix_sec();
        if sec != cur_sec {
            flush_window(&mut agg, cur_sec, &latest);
            agg.clear();
            cur_sec = sec;
        }
        // 只统计被实际显示的帧(present 数即 fps 口径)
        if !dropped && gap > 0.0 {
            agg.entry(pid).or_insert_with(|| (app, Vec::new())).1.push(gap);
        }
    }
    // 流结束(PresentMon 退出):标记不可用由 available 读取方自行发现
}

fn flush_window(
    agg: &mut std::collections::HashMap<u32, (String, Vec<f64>)>,
    sec: u64,
    latest: &Arc<Mutex<Option<GameSample>>>,
) {
    // 选本秒帧数最多的进程(渲染大户)
    let best = agg
        .iter()
        .max_by_key(|(_, (_, gaps))| gaps.len())
        .map(|(pid, (app, gaps))| (*pid, app.clone(), gaps));
    let Some((pid, app, gaps)) = best else { return };
    if gaps.len() < 5 {
        return; // 过少的帧数(空闲应用偶发刷新)不作为"渲染活动"
    }
    let max_gap = gaps.iter().cloned().fold(0.0_f64, f64::max);
    let avg_gap = gaps.iter().sum::<f64>() / gaps.len() as f64;
    let stutters = gaps.iter().filter(|g| **g >= STUTTER_GAP_MS).count() as u32;
    let sample = GameSample {
        ts_ms: sec * 1000,
        app,
        pid,
        fps: gaps.len() as u32,
        avg_gap_ms: avg_gap,
        max_gap_ms: max_gap,
        stutters,
    };
    *latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(sample);
}

fn unix_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
