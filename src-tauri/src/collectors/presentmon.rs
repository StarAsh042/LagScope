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
    /// PresentMon 是否在使用中:置 true 于启动成功,置 false 于解析线程结束
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
                let available = watch.available.clone();
                let child_slot = watch.child.clone();
                let spawned = thread::Builder::new()
                    .name("presentmon-reader".into())
                    .spawn(move || {
                        parse_stream(stdout, latest, available);
                        // 收尾:回收子进程句柄,避免僵尸进程与句柄泄漏
                        let mut slot = child_slot.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(mut c) = slot.take() {
                            let _ = c.wait();
                        }
                    });
                if spawned.is_err() {
                    // 解析线程起不来 = 输出无人读取,等同于不可用
                    watch.available.store(false, Ordering::Relaxed);
                    if let Some(mut c) = watch.child.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                }
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

    /// PresentMon 是否在跑:启动成功且子进程仍存活。
    ///
    /// 子进程已退出时顺手复位标志并弃掉句柄,避免设置页长期误报「运行中」。
    pub fn is_available(&self) -> bool {
        if !self.available.load(Ordering::Relaxed) {
            return false;
        }
        let mut slot = self.child.lock().unwrap_or_else(|e| e.into_inner());
        match slot.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => {
                    self.available.store(false, Ordering::Relaxed);
                    slot.take();
                    false
                }
            },
            // 句柄已被解析线程收走 = 子进程输出流已结束
            None => {
                self.available.store(false, Ordering::Relaxed);
                false
            }
        }
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
fn parse_stream<R: std::io::Read + Send + 'static>(
    stdout: R,
    latest: Arc<Mutex<Option<GameSample>>>,
    available: Arc<AtomicBool>,
) {
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
    // 流结束(PresentMon 退出):当前窗口不完整,不发布,直接作废并标记不可用
    agg.clear();
    *latest.lock().unwrap_or_else(|e| e.into_inner()) = None;
    available.store(false, Ordering::Relaxed);
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn watch_with(available: bool, child: Option<Child>) -> PresentMonWatch {
        PresentMonWatch {
            latest: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(child)),
            available: Arc::new(AtomicBool::new(available)),
        }
    }

    /// 回归:解析线程读到流结束必须复位 available,不能只留注释让读取方自觉。
    #[test]
    fn stream_end_clears_available() {
        let available = Arc::new(AtomicBool::new(true));
        let latest = Arc::new(Mutex::new(None));
        // 只有表头、没有数据行,读完即结束
        parse_stream(
            Cursor::new(b"Application,ProcessID,Dropped,MsBetweenPresents\n".to_vec()),
            latest.clone(),
            available.clone(),
        );
        assert!(
            !available.load(Ordering::Relaxed),
            "流结束后 available 必须复位为 false"
        );
        assert!(latest.lock().unwrap().is_none(), "流结束应作废未完成的窗口");
    }

    /// 回归:解析线程把上一轮的残留快照清掉,避免前端继续展示已停止的帧数据。
    #[test]
    fn stream_end_drops_stale_sample() {
        let available = Arc::new(AtomicBool::new(true));
        let latest = Arc::new(Mutex::new(Some(GameSample {
            ts_ms: 1_793_000_000_000,
            app: "stale.exe".into(),
            pid: 4242,
            fps: 60,
            avg_gap_ms: 16.7,
            max_gap_ms: 20.0,
            stutters: 0,
        })));
        parse_stream(Cursor::new(Vec::new()), latest.clone(), available.clone());
        assert!(latest.lock().unwrap().is_none(), "残留快照必须清空");
        assert!(!available.load(Ordering::Relaxed));
    }

    /// 回归:available 为 true 但子进程句柄已丢失(解析线程收走)= 不可用。
    #[test]
    fn is_available_false_without_child_handle() {
        let w = watch_with(true, None);
        assert!(!w.is_available(), "没有子进程句柄就不该报「运行中」");
        assert!(
            !w.available.load(Ordering::Relaxed),
            "查询时应顺手把标志复位"
        );
    }

    /// 子进程已退出时 is_available 必须返回 false 并复位标志、弃掉句柄。
    #[test]
    fn is_available_false_after_child_exit() {
        let mut child = Command::new("cmd")
            .args(["/c", "exit", "0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn short-lived process");
        let _ = child.wait(); // 确保它已经退出
        let w = watch_with(true, Some(child));
        assert!(!w.is_available(), "子进程已退出,不应再报「运行中」");
        assert!(
            w.child.lock().unwrap().is_none(),
            "已退出的句柄应被丢弃"
        );
    }

    /// 活着的子进程仍报可用(正向用例,防止把状态锁死成 false)。
    #[test]
    fn is_available_true_for_live_child() {
        let child = Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 > nul"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn long-running process");
        let w = watch_with(true, Some(child));
        assert!(w.is_available(), "存活子进程应报可用");
        // 收尾:kill 并把句柄 wait 掉,避免测试进程泄漏
        if let Some(mut c) = w.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
