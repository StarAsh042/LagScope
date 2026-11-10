//! 卡顿检测引擎:滚动窗口 + 阈值判定 + 冷却去重,
//! 触发时生成带归因快照的 [`LagEvent`]。

use std::collections::{HashMap, VecDeque};

use serde::Serialize;

use crate::collectors::{MetricSample, ProcessInfo};

/// 检测阈值(默认值,M6 接入设置页)
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// CPU 总占用阈值 %
    pub cpu_pct: f32,
    /// CPU 需持续秒数
    pub cpu_sustain_s: u64,
    /// 提交内存占比阈值 %
    pub commit_pct: f64,
    /// 硬缺页速率阈值 页/秒
    pub page_input: f64,
    /// 硬缺页需持续秒数(独立于 CPU 阈值,避免改 CPU 设置串扰内存告警)
    pub page_sustain_s: u64,
    /// 提交内存需持续秒数(独立于磁盘阈值)
    pub mem_sustain_s: u64,
    /// 磁盘平均延迟阈值 秒
    pub disk_sec_transfer: f64,
    /// 磁盘需持续秒数
    pub disk_sustain_s: u64,
    /// 中断时间占比阈值 %
    pub interrupt_pct: f64,
    /// 中断需持续秒数
    pub interrupt_sustain_s: u64,
    /// 帧间隔尖峰阈值 ms(超过视为卡帧)
    pub stutter_gap_ms: f64,
    /// DPC 总量阈值 次/秒(超过触发 dpc_driver 归因)
    pub dpc_total_threshold: f64,
    /// 同类原因冷却毫秒
    pub cooldown_ms: u64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            cpu_pct: 85.0,
            cpu_sustain_s: 10,
            commit_pct: 90.0,
            page_input: 500.0,
            page_sustain_s: 5,
            mem_sustain_s: 5,
            disk_sec_transfer: 0.025,
            disk_sustain_s: 5,
            interrupt_pct: 20.0,
            interrupt_sustain_s: 5,
            stutter_gap_ms: 100.0,
            dpc_total_threshold: 8000.0,
            cooldown_ms: 60_000,
        }
    }
}

/// 触发原因
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LagReason {
    /// cpu | mem_pressure | page_faults | disk_latency | interrupt | hung_window | manual
    pub kind: String,
    /// 中文归因描述(展示给用户)
    pub detail: String,
    /// 触发时的量化值
    pub value: f64,
}

/// Top 进程条目(value 语义:cpu=% / mem=MB / disk=B/s)
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopProc {
    pub pid: u32,
    pub name: String,
    pub value: f64,
}

/// 一秒的帧统计(来源 PresentMon,按帧数最多的渲染进程聚合)
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSample {
    /// 统计窗口的起始秒
    pub ts_ms: u64,
    pub app: String,
    pub pid: u32,
    /// 该秒完成的帧数
    pub fps: u32,
    pub avg_gap_ms: f64,
    /// 最大帧间隔(卡顿的直接表现)
    pub max_gap_ms: f64,
    /// 超过卡帧阈值的间隔次数
    pub stutters: u32,
}

/// 一秒的 DPC 驱动分布(来源内核 ETW,可独立触发 dpc_driver 事件)
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DpcSample {
    pub ts_ms: u64,
    /// 该秒 DPC 事件总数
    pub total: u32,
    /// 驱动名 → 计数,降序前 5
    pub drivers: Vec<(String, u32)>,
}

/// 一次卡顿事件:原因 + 全量计数器快照 + 各维度 Top 进程 + 挂起窗口
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LagEvent {
    pub id: Option<i64>,
    pub ts_ms: u64,
    pub reasons: Vec<LagReason>,
    pub counters: MetricSample,
    pub top_cpu: Vec<TopProc>,
    pub top_mem: Vec<TopProc>,
    pub top_disk: Vec<TopProc>,
    pub hung_windows: Vec<String>,
    /// 帧监测开启且当秒有渲染活动时存在
    pub game: Option<GameSample>,
    /// ETW DPC 监测可用时存在
    pub dpc_top: Option<DpcSample>,
}

/// 各维度保留的 Top 进程数
const TOP_N: usize = 5;

/// 把 (pid, name, value) 按 value 降序插入不超过 `TOP_N` 的列表。
///
/// 只有真正挤进前 N 的条目才会复制进程名 —— 这是 `LagEvent::build` 省掉
/// 三次全量 `sort_by` + 三次全量 `name.clone()` 的关键。
fn push_top(list: &mut Vec<TopProc>, pid: u32, name: &str, value: f64) {
    if list.len() == TOP_N && list.last().is_some_and(|last| value.total_cmp(&last.value).is_lt()) {
        return;
    }
    let pos = list
        .iter()
        .position(|x| x.value.total_cmp(&value).is_lt())
        .unwrap_or(list.len());
    list.insert(
        pos,
        TopProc {
            pid,
            name: name.to_string(),
            value,
        },
    );
    if list.len() > TOP_N {
        list.pop();
    }
}

impl LagEvent {
    /// 用当前状态构造事件(检测引擎与"立即诊断"共用)
    pub fn build(
        ts_ms: u64,
        reasons: Vec<LagReason>,
        counters: MetricSample,
        processes: &[ProcessInfo],
        hung_windows: Vec<String>,
        game: Option<GameSample>,
        dpc_top: Option<DpcSample>,
    ) -> Self {
        // 一次遍历同时维护 CPU / 内存 / 磁盘三个 Top5,避免对同一批进程
        // 做三趟全量构造与排序(300 进程时省下约 900 次 String 分配)
        let mut top_cpu: Vec<TopProc> = Vec::with_capacity(TOP_N);
        let mut top_mem: Vec<TopProc> = Vec::with_capacity(TOP_N);
        let mut top_disk: Vec<TopProc> = Vec::with_capacity(TOP_N);
        for p in processes {
            push_top(&mut top_cpu, p.pid, &p.name, p.cpu_pct as f64);
            push_top(&mut top_mem, p.pid, &p.name, p.mem_mb);
            push_top(
                &mut top_disk,
                p.pid,
                &p.name,
                p.disk_read_bps + p.disk_write_bps,
            );
        }

        LagEvent {
            id: None,
            ts_ms,
            reasons,
            counters,
            top_cpu,
            top_mem,
            top_disk,
            hung_windows,
            game,
            dpc_top,
        }
    }
}

/// 检测器状态机(在采集线程中逐秒 feed;阈值由 feed 时传入,支持热更新)
pub struct LagDetector {
    /// 最近 15 秒样本
    window: VecDeque<MetricSample>,
    /// 各原因上次触发时间
    last_fired: HashMap<String, u64>,
    hung_cache: Vec<String>,
}

const WINDOW_S: u64 = 15;

impl Default for LagDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl LagDetector {
    pub fn new() -> Self {
        Self {
            window: VecDeque::new(),
            last_fired: HashMap::new(),
            hung_cache: Vec::new(),
        }
    }

    fn sustained<F>(&self, seconds: u64, pred: F) -> Option<f64>
    where
        F: Fn(&MetricSample) -> Option<f64>,
    {
        // 取窗口内最近 seconds 秒的样本:全部满足,且时间覆盖接近目标
        // (留 2 秒容差,容忍采集 tick 抖动;不能用固定样本数,慢 tick 会永不满足)
        let newest = self.window.back()?;
        let cutoff = newest.ts_ms.saturating_sub(seconds * 1000);
        let in_window: Vec<&MetricSample> = self
            .window
            .iter()
            .filter(|s| s.ts_ms >= cutoff)
            .collect();
        if in_window.len() < 2 {
            return None;
        }
        let coverage = newest.ts_ms.saturating_sub(in_window[0].ts_ms);
        if coverage < seconds.max(1).saturating_sub(2) * 1000 {
            return None;
        }
        let mut last_value = None;
        for s in in_window {
            last_value = Some(pred(s)?);
        }
        last_value
    }

    /// 每秒喂入一次采样;挂起窗口列表由独立线程提供。
    /// 返回 Some(LagEvent) 表示触发卡顿事件
    pub fn feed(
        &mut self,
        sample: &MetricSample,
        processes: &[ProcessInfo],
        hung_windows: &[String],
        game: Option<&GameSample>,
        dpc: Option<&DpcSample>,
        config: &DetectorConfig,
    ) -> Option<LagEvent> {
        self.window.push_back(sample.clone());
        let cutoff = sample.ts_ms.saturating_sub(WINDOW_S * 1000);
        while self.window.front().map(|s| s.ts_ms < cutoff).unwrap_or(false) {
            self.window.pop_front();
        }
        self.hung_cache = hung_windows.to_vec();

        let cfg = config;
        let mut reasons = Vec::new();

        if let Some(v) = self.sustained(cfg.cpu_sustain_s, |s| {
            (s.cpu_total >= cfg.cpu_pct).then_some(s.cpu_total as f64)
        }) {
            reasons.push(LagReason {
                kind: "cpu".into(),
                detail: format!("CPU 总占用 {v:.0}% 持续 {} 秒", cfg.cpu_sustain_s),
                value: v,
            });
        }

        if let Some(v) = self.sustained(cfg.mem_sustain_s, |s| {
            let pct = if s.mem_commit_limit > 0 {
                (s.mem_commit_used as f64 / s.mem_commit_limit as f64) * 100.0
            } else {
                0.0
            };
            (pct >= cfg.commit_pct).then_some(pct)
        }) {
            reasons.push(LagReason {
                kind: "mem_pressure".into(),
                detail: format!(
                    "提交内存占用 {v:.0}% 持续 {} 秒(虚拟内存接近上限)",
                    cfg.mem_sustain_s
                ),
                value: v,
            });
        }

        if let Some(v) = self.sustained(cfg.page_sustain_s, |s| {
            (s.page_input_per_sec >= cfg.page_input).then_some(s.page_input_per_sec)
        }) {
            reasons.push(LagReason {
                kind: "page_faults".into(),
                detail: format!("硬缺页 {v:.0} 页/秒 持续 {} 秒(内存不足)", cfg.page_sustain_s),
                value: v,
            });
        }

        if let Some(v) = self.sustained(cfg.disk_sustain_s, |s| {
            (s.disk_avg_sec_transfer >= cfg.disk_sec_transfer)
                .then_some(s.disk_avg_sec_transfer)
        }) {
            reasons.push(LagReason {
                kind: "disk_latency".into(),
                detail: format!(
                    "磁盘平均延迟 {:.1} ms 持续 {} 秒",
                    v * 1000.0,
                    cfg.disk_sustain_s
                ),
                value: v,
            });
        }

        if let Some(v) = self.sustained(cfg.interrupt_sustain_s, |s| {
            (s.interrupt_pct >= cfg.interrupt_pct).then_some(s.interrupt_pct)
        }) {
            reasons.push(LagReason {
                kind: "interrupt".into(),
                detail: format!("中断时间占比 {v:.0}% 持续 {} 秒(驱动高负载)", cfg.interrupt_sustain_s),
                value: v,
            });
        }

        if !self.hung_cache.is_empty() {
            reasons.push(LagReason {
                kind: "hung_window".into(),
                detail: format!("发现 {} 个无响应窗口", self.hung_cache.len()),
                value: self.hung_cache.len() as f64,
            });
        }

        // 帧卡顿:持续渲染(fps>=45)的进程出现明显帧间隔尖峰。
        // 单次尖峰即触发(卡顿本质是瞬时事件),靠冷却去重。
        if let Some(g) = game
            && g.fps >= 45 && g.max_gap_ms >= cfg.stutter_gap_ms {
                reasons.push(LagReason {
                    kind: "frame_stutter".into(),
                    detail: format!(
                        "{} 帧间隔尖峰 {:.0} ms(fps {},卡帧 {} 次)",
                        g.app, g.max_gap_ms, g.fps, g.stutters
                    ),
                    value: g.max_gap_ms,
                });
            }

        // DPC 风暴:高频 DPC 且可归因到具体驱动(瞬时事件,靠冷却去重)
        if let Some(d) = dpc
            && d.total as f64 >= cfg.dpc_total_threshold {
                let (top_name, top_count) = d
                    .drivers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| ("未知".into(), 0));
                let pct = if d.total > 0 {
                    (top_count as f64 / d.total as f64) * 100.0
                } else {
                    0.0
                };
                reasons.push(LagReason {
                    kind: "dpc_driver".into(),
                    detail: format!(
                        "DPC {}/秒,主要来自 {}(占 {:.0}%)",
                        d.total, top_name, pct
                    ),
                    value: d.total as f64,
                });
            }

        // 冷却:同类原因冷却期内不重复触发
        reasons.retain(|r| {
            self.last_fired
                .get(&r.kind)
                .map(|t| sample.ts_ms.saturating_sub(*t) >= cfg.cooldown_ms)
                .unwrap_or(true)
        });

        if reasons.is_empty() {
            return None;
        }
        for r in &reasons {
            self.last_fired.insert(r.kind.clone(), sample.ts_ms);
        }

        Some(LagEvent::build(
            sample.ts_ms,
            reasons,
            sample.clone(),
            processes,
            self.hung_cache.clone(),
            game.cloned(),
            dpc.cloned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts_ms: u64, cpu: f32) -> MetricSample {
        MetricSample {
            ts_ms,
            cpu_total: cpu,
            cpu_per_core: vec![cpu; 8],
            mem_total: 16 * 1024 * 1024 * 1024,
            mem_used: 4 * 1024 * 1024 * 1024,
            mem_commit_used: 8 * 1024 * 1024 * 1024,
            mem_commit_limit: 22 * 1024 * 1024 * 1024,
            disk_avg_sec_transfer: 0.002,
            interrupt_pct: 2.0,
            page_input_per_sec: 10.0,
            ..Default::default()
        }
    }

    #[test]
    fn cpu_sustained_triggers_event() {
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let mut fired = None;
        for i in 0..14u64 {
            let s = sample(base + i * 1000, 95.0);
            if let Some(ev) = det.feed(&s, &[], &[], None, None, &cfg) {
                fired = Some(ev);
                break;
            }
        }
        let ev = fired.expect("CPU 持续满载应触发事件");
        assert!(
            ev.reasons.iter().any(|r| r.kind == "cpu"),
            "应包含 cpu 原因: {:?}",
            ev.reasons
        );
    }

    #[test]
    fn cooldown_blocks_repeat() {
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let mut first = None;
        for i in 0..14u64 {
            if let Some(ev) = det.feed(&sample(base + i * 1000, 95.0), &[], &[], None, None, &cfg) {
                first = Some(ev);
                break;
            }
        }
        assert!(first.is_some());
        // 冷却期内继续满载,不应再触发
        for i in 14..40u64 {
            let ev = det.feed(&sample(base + i * 1000, 95.0), &[], &[], None, None, &cfg);
            assert!(ev.is_none(), "冷却期内不应重复触发: {ev:?}");
        }
    }

    #[test]
    fn moderate_load_no_event() {
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        for i in 0..20u64 {
            let ev = det.feed(&sample(base + i * 1000, 45.0), &[], &[], None, None, &cfg);
            assert!(ev.is_none(), "中等负载不应触发: {ev:?}");
        }
    }

    #[test]
    fn slow_tick_still_triggers() {
        // 采集 tick 慢于 1s(如 1.4s)时也应能触发(回归:旧逻辑按样本数判定会永不满足)
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let mut fired = None;
        for i in 0..20u64 {
            if let Some(ev) = det.feed(&sample(base + i * 1400, 95.0), &[], &[], None, None, &cfg) {
                fired = Some(ev);
                break;
            }
        }
        assert!(fired.is_some(), "慢 tick 下 CPU 满载应触发");
        assert!(fired.unwrap().reasons.iter().any(|r| r.kind == "cpu"));
    }

    #[test]
    fn dpc_driver_triggers_on_storm() {
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let dpc = DpcSample {
            ts_ms: base,
            total: 12000,
            drivers: vec![
                ("nvlddmkm.sys".into(), 9000),
                ("ndis.sys".into(), 2000),
                ("ACPI.sys".into(), 1000),
            ],
        };
        let ev = det
            .feed(&sample(base, 30.0), &[], &[], None, Some(&dpc), &cfg)
            .expect("DPC 风暴应触发");
        let r = ev
            .reasons
            .iter()
            .find(|r| r.kind == "dpc_driver")
            .expect("应包含 dpc_driver 原因");
        assert!(r.detail.contains("nvlddmkm.sys"), "归因应含驱动名: {}", r.detail);
        assert!(r.detail.contains("75%"), "应含占比: {}", r.detail);
        assert_eq!(ev.dpc_top.as_ref().unwrap().drivers[0].0, "nvlddmkm.sys");
    }

    #[test]
    fn dpc_driver_ignores_below_threshold() {
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        // 正常水平(~1500/s)不触发
        let dpc = DpcSample {
            ts_ms: base,
            total: 1500,
            drivers: vec![("ntoskrnl.exe".into(), 1200)],
        };
        assert!(
            det.feed(&sample(base, 20.0), &[], &[], None, Some(&dpc), &cfg)
                .is_none()
        );
    }

    #[test]
    fn frame_stutter_triggers_on_gap_spike() {
        let cfg = DetectorConfig::default();
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let game = GameSample {
            ts_ms: base,
            app: "game.exe".into(),
            pid: 1,
            fps: 60,
            avg_gap_ms: 16.7,
            max_gap_ms: 320.0,
            stutters: 2,
        };
        let ev = det
            .feed(&sample(base, 30.0), &[], &[], Some(&game), None, &cfg)
            .expect("帧间隔尖峰应触发");
        let r = ev
            .reasons
            .iter()
            .find(|r| r.kind == "frame_stutter")
            .expect("应包含 frame_stutter 原因");
        assert!(r.detail.contains("game.exe"));
        assert_eq!(ev.game.as_ref().unwrap().fps, 60);
    }

    #[test]
    fn frame_stutter_ignores_low_fps_or_small_gap() {
        let cfg = DetectorConfig::default();
        let base: u64 = 1_793_000_000_000;
        // 低 fps(非持续渲染)不触发
        let mut det = LagDetector::new();
        let idle = GameSample {
            ts_ms: base,
            app: "idle.exe".into(),
            pid: 2,
            fps: 5,
            avg_gap_ms: 200.0,
            max_gap_ms: 500.0,
            stutters: 1,
        };
        assert!(det.feed(&sample(base, 20.0), &[], &[], Some(&idle), None, &cfg).is_none());
        // fps 高但间隔正常不触发
        let mut det2 = LagDetector::new();
        let smooth = GameSample {
            ts_ms: base,
            app: "game.exe".into(),
            pid: 3,
            fps: 60,
            avg_gap_ms: 16.7,
            max_gap_ms: 33.0,
            stutters: 0,
        };
        assert!(
            det2
                .feed(&sample(base, 20.0), &[], &[], Some(&smooth), None, &cfg)
                .is_none()
        );
    }

    fn page_sample(ts_ms: u64, page_input: f64) -> MetricSample {
        MetricSample {
            page_input_per_sec: page_input,
            ..sample(ts_ms, 10.0)
        }
    }

    /// 回归:硬缺页的持续时间必须走 page_sustain_s,不能复用 cpu_sustain_s。
    #[test]
    fn page_faults_use_own_sustain_window() {
        let cfg = DetectorConfig {
            page_sustain_s: 3,
            cpu_sustain_s: 60, // CPU 阈值拉到很大,不应影响缺页判定
            page_input: 500.0,
            ..DetectorConfig::default()
        };

        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let mut fired = None;
        for i in 0..8u64 {
            if let Some(ev) = det.feed(&page_sample(base + i * 1000, 900.0), &[], &[], None, None, &cfg) {
                fired = Some(ev);
                break;
            }
        }
        let ev = fired.expect("缺页持续 3 秒即应触发,不应被 cpu_sustain_s=60 拖住");
        let r = ev
            .reasons
            .iter()
            .find(|r| r.kind == "page_faults")
            .expect("应包含 page_faults 原因");
        assert!(
            r.detail.contains("持续 3 秒"),
            "文案应渲染 page_sustain_s,实际: {}",
            r.detail
        );
    }

    /// 回归:内存压力不能借用磁盘阈值。
    #[test]
    fn mem_pressure_uses_own_sustain_window() {
        let cfg = DetectorConfig {
            mem_sustain_s: 3,
            disk_sustain_s: 60, // 磁盘阈值拉大,不应影响内存判定
            commit_pct: 90.0,
            ..DetectorConfig::default()
        };

        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let mut fired = None;
        for i in 0..8u64 {
            let mut s = sample(base + i * 1000, 10.0);
            s.mem_commit_used = 21 * 1024 * 1024 * 1024; // 21/22 GiB ≈ 95%
            s.mem_commit_limit = 22 * 1024 * 1024 * 1024;
            if let Some(ev) = det.feed(&s, &[], &[], None, None, &cfg) {
                fired = Some(ev);
                break;
            }
        }
        let ev = fired.expect("内存超阈持续 3 秒即应触发,不应被 disk_sustain_s=60 拖住");
        let r = ev
            .reasons
            .iter()
            .find(|r| r.kind == "mem_pressure")
            .expect("应包含 mem_pressure 原因");
        assert!(
            r.detail.contains("持续 3 秒"),
            "文案应渲染 mem_sustain_s,实际: {}",
            r.detail
        );
    }

    /// 两个阈值互相独立:改 CPU 持续秒数不影响缺页,改缺页不影响 CPU。
    #[test]
    fn sustain_thresholds_are_independent() {
        let cfg = DetectorConfig {
            page_sustain_s: 2,
            cpu_sustain_s: 30,
            page_input: 500.0,
            cpu_pct: 85.0,
            ..DetectorConfig::default()
        };

        // CPU 高但缺页正常 → 早期不应触发 cpu(30 秒窗口未满)
        let mut det = LagDetector::new();
        let base: u64 = 1_793_000_000_000;
        let mut cpu_fired_early = false;
        for i in 0..6u64 {
            let mut s = sample(base + i * 1000, 99.0);
            s.page_input_per_sec = 10.0;
            if det.feed(&s, &[], &[], None, None, &cfg).is_some() {
                cpu_fired_early = true;
            }
        }
        assert!(!cpu_fired_early, "CPU 30 秒窗口未满,不应因 CPU 触发事件");
    }

    fn proc(pid: u32, cpu: f32, mem: f64, disk: f64) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: format!("p{pid}"),
            cpu_pct: cpu,
            mem_mb: mem,
            disk_read_bps: disk,
            disk_write_bps: 0.0,
            exe_path: None,
        }
    }

    fn build_with(procs: &[ProcessInfo]) -> LagEvent {
        LagEvent::build(
            1,
            vec![],
            sample(1, 0.0),
            procs,
            vec![],
            None,
            None,
        )
    }

    #[test]
    fn top_lists_are_sorted_and_capped() {
        // 8 个进程,CPU / 内存 / 磁盘按不同的顺序排列,确保三条 Top5 各取各的
        let mut procs: Vec<ProcessInfo> = (0..8u32)
            .map(|i| {
                let f = i as f32;
                proc(100 + i, f, (7 - i) as f64, (i % 3) as f64 * 100.0)
            })
            .collect();
        procs[5].disk_read_bps = 999.0;

        let ev = build_with(&procs);

        for (label, list) in [
            ("cpu", &ev.top_cpu),
            ("mem", &ev.top_mem),
            ("disk", &ev.top_disk),
        ] {
            assert_eq!(list.len(), TOP_N, "{label} 应截断到 Top{TOP_N}");
            assert!(
                list.windows(2).all(|w| w[0].value >= w[1].value),
                "{label} 应按降序排列: {list:?}"
            );
        }
        assert_eq!(ev.top_cpu[0].pid, 107);
        assert_eq!(ev.top_mem[0].pid, 100);
        assert_eq!(ev.top_disk[0].pid, 105);
        // 磁盘维度其余全是 0,Top1 之外不能出现 999 以外的更高值
        assert!(ev.top_disk[1..].iter().all(|p| p.value < 999.0));
    }

    #[test]
    fn top_lists_keep_everything_below_the_limit() {
        let procs = vec![proc(1, 10.0, 1.0, 0.0), proc(2, 30.0, 2.0, 0.0)];
        let ev = build_with(&procs);
        assert_eq!(ev.top_cpu.len(), 2, "不足 {TOP_N} 个时不应补齐或截断");
        assert_eq!(ev.top_cpu[0].pid, 2);
        assert_eq!(ev.top_cpu[1].pid, 1);
    }

    #[test]
    fn top_lists_handle_ties_without_dropping_entries() {
        // 同值时按插入顺序稳定保留,且不会因为浮点 NaN 丢条目
        let procs: Vec<ProcessInfo> = (0..7u32).map(|i| proc(i, 50.0, 50.0, 0.0)).collect();
        let ev = build_with(&procs);
        assert_eq!(ev.top_cpu.len(), TOP_N);
        let pids: Vec<u32> = ev.top_cpu.iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![0, 1, 2, 3, 4], "同值时应按插入顺序取前 {TOP_N} 个");
    }

    #[test]
    fn push_top_survives_nan_values() {
        // NaN 参与比较时 total_cmp 仍有全序,不允许 panic 或长度失控
        let mut list: Vec<TopProc> = Vec::with_capacity(TOP_N);
        for i in 0..10u32 {
            let v = if i % 2 == 0 { f64::NAN } else { i as f64 };
            push_top(&mut list, i, "x", v);
        }
        assert_eq!(list.len(), TOP_N);
    }
}
