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
    /// 磁盘平均延迟阈值 秒
    pub disk_sec_transfer: f64,
    /// 磁盘需持续秒数
    pub disk_sustain_s: u64,
    /// 中断时间占比阈值 %
    pub interrupt_pct: f64,
    /// 中断需持续秒数
    pub interrupt_sustain_s: u64,
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
            disk_sec_transfer: 0.025,
            disk_sustain_s: 5,
            interrupt_pct: 20.0,
            interrupt_sustain_s: 5,
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
}

impl LagEvent {
    /// 用当前状态构造事件(检测引擎与"立即诊断"共用)
    pub fn build(
        ts_ms: u64,
        reasons: Vec<LagReason>,
        counters: MetricSample,
        processes: &[ProcessInfo],
        hung_windows: Vec<String>,
    ) -> Self {
        let mut top_cpu: Vec<TopProc> = processes
            .iter()
            .map(|p| TopProc {
                pid: p.pid,
                name: p.name.clone(),
                value: p.cpu_pct as f64,
            })
            .collect();
        top_cpu.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap());
        top_cpu.truncate(5);

        let mut top_mem: Vec<TopProc> = processes
            .iter()
            .map(|p| TopProc {
                pid: p.pid,
                name: p.name.clone(),
                value: p.mem_mb,
            })
            .collect();
        top_mem.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap());
        top_mem.truncate(5);

        let mut top_disk: Vec<TopProc> = processes
            .iter()
            .map(|p| TopProc {
                pid: p.pid,
                name: p.name.clone(),
                value: p.disk_read_bps + p.disk_write_bps,
            })
            .collect();
        top_disk.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap());
        top_disk.truncate(5);

        LagEvent {
            id: None,
            ts_ms,
            reasons,
            counters,
            top_cpu,
            top_mem,
            top_disk,
            hung_windows,
        }
    }
}

/// 检测器状态机(在采集线程中逐秒 feed)
pub struct LagDetector {
    config: DetectorConfig,
    /// 最近 15 秒样本
    window: VecDeque<MetricSample>,
    /// 各原因上次触发时间
    last_fired: HashMap<String, u64>,
    hung_cache: Vec<String>,
}

const WINDOW_S: u64 = 15;

impl LagDetector {
    pub fn new(config: DetectorConfig) -> Self {
        Self {
            config,
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
        sample: MetricSample,
        processes: &[ProcessInfo],
        hung_windows: &[String],
    ) -> Option<LagEvent> {
        self.window.push_back(sample.clone());
        let cutoff = sample.ts_ms.saturating_sub(WINDOW_S * 1000);
        while self.window.front().map(|s| s.ts_ms < cutoff).unwrap_or(false) {
            self.window.pop_front();
        }
        self.hung_cache = hung_windows.to_vec();

        let cfg = &self.config;
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

        if let Some(v) = self.sustained(cfg.disk_sustain_s, |s| {
            let pct = if s.mem_commit_limit > 0 {
                (s.mem_commit_used as f64 / s.mem_commit_limit as f64) * 100.0
            } else {
                0.0
            };
            (pct >= cfg.commit_pct).then_some(pct)
        }) {
            reasons.push(LagReason {
                kind: "mem_pressure".into(),
                detail: format!("提交内存占用 {v:.0}%(虚拟内存接近上限)"),
                value: v,
            });
        }

        if let Some(v) = self.sustained(cfg.cpu_sustain_s, |s| {
            (s.page_input_per_sec >= cfg.page_input).then_some(s.page_input_per_sec)
        }) {
            reasons.push(LagReason {
                kind: "page_faults".into(),
                detail: format!("硬缺页 {v:.0} 页/秒 持续 {} 秒(内存不足)", cfg.cpu_sustain_s),
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
            sample,
            processes,
            self.hung_cache.clone(),
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
        let mut det = LagDetector::new(DetectorConfig::default());
        let base: u64 = 1_793_000_000_000;
        let mut fired = None;
        for i in 0..14u64 {
            let s = sample(base + i * 1000, 95.0);
            if let Some(ev) = det.feed(s, &[], &[]) {
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
        let mut det = LagDetector::new(DetectorConfig::default());
        let base: u64 = 1_793_000_000_000;
        let mut first = None;
        for i in 0..14u64 {
            if let Some(ev) = det.feed(sample(base + i * 1000, 95.0), &[], &[]) {
                first = Some(ev);
                break;
            }
        }
        assert!(first.is_some());
        // 冷却期内继续满载,不应再触发
        for i in 14..40u64 {
            let ev = det.feed(sample(base + i * 1000, 95.0), &[], &[]);
            assert!(ev.is_none(), "冷却期内不应重复触发: {ev:?}");
        }
    }

    #[test]
    fn moderate_load_no_event() {
        let mut det = LagDetector::new(DetectorConfig::default());
        let base: u64 = 1_793_000_000_000;
        for i in 0..20u64 {
            let ev = det.feed(sample(base + i * 1000, 45.0), &[], &[]);
            assert!(ev.is_none(), "中等负载不应触发: {ev:?}");
        }
    }

    #[test]
    fn slow_tick_still_triggers() {
        // 采集 tick 慢于 1s(如 1.4s)时也应能触发(回归:旧逻辑按样本数判定会永不满足)
        let mut det = LagDetector::new(DetectorConfig::default());
        let base: u64 = 1_793_000_000_000;
        let mut fired = None;
        for i in 0..20u64 {
            if let Some(ev) = det.feed(sample(base + i * 1400, 95.0), &[], &[]) {
                fired = Some(ev);
                break;
            }
        }
        assert!(fired.is_some(), "慢 tick 下 CPU 满载应触发");
        assert!(fired.unwrap().reasons.iter().any(|r| r.kind == "cpu"));
    }
}
