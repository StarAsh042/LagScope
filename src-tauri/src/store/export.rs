//! 事件导出:把事件库里的 LagEvent 序列化为 CSV / JSON 并落盘。
//!
//! 这里只做纯字符串转换与文件写入,不碰 Tauri 路径 —— 目录选择留在命令层,
//! 这样格式化逻辑可以脱离 Tauri 运行时直接单测。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::lag_detector::{LagEvent, TopProc};

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
}

impl ExportFormat {
    /// 解析前端传来的格式名(大小写不敏感)
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            other => Err(format!("不支持的导出格式: {other}(可用: csv / json)")),
        }
    }

    /// 文件扩展名
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

/// CSV 表头。列顺序与 to_csv 每行写入的字段严格一一对应。
pub const CSV_HEADER: [&str; 32] = [
    "id",
    "时间",
    "tsMs",
    "原因",
    "归因详情",
    "归因值",
    "CPU%",
    "内存%",
    "提交内存%",
    "硬缺页/s",
    "磁盘活跃%",
    "磁盘传输耗时ms",
    "磁盘队列",
    "磁盘读B/s",
    "磁盘写B/s",
    "网络下行B/s",
    "网络上行B/s",
    "DPC/s",
    "中断%",
    "GPU%",
    "显存MB",
    "温度℃",
    "TopCPU",
    "Top内存",
    "Top磁盘",
    "挂起窗口",
    "帧率FPS",
    "最大帧间隔ms",
    "卡帧数",
    "渲染进程",
    "DPC总数",
    "DPC驱动",
];

/// 逐字段拼一行 CSV:自动处理分隔符与转义。
struct Row {
    buf: String,
    empty: bool,
}

impl Row {
    fn new() -> Self {
        Self {
            buf: String::with_capacity(320),
            empty: true,
        }
    }

    /// 写入一个字段(含逗号/引号/换行时加引号并把内部引号翻倍)
    fn raw(&mut self, v: &str) -> &mut Self {
        if !self.empty {
            self.buf.push(',');
        }
        if v.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
            self.buf.push('"');
            for c in v.chars() {
                if c == '"' {
                    self.buf.push('"');
                }
                self.buf.push(c);
            }
            self.buf.push('"');
        } else {
            self.buf.push_str(v);
        }
        self.empty = false;
        self
    }

    /// 写入定小数位的数值
    fn num(&mut self, v: f64, decimals: usize) -> &mut Self {
        let text = format!("{v:.decimals$}");
        self.raw(&text)
    }

    /// 写入可缺省数值:缺失时留空单元格
    fn opt(&mut self, v: Option<f64>, decimals: usize) -> &mut Self {
        match v {
            Some(x) => self.num(x, decimals),
            None => self.raw(""),
        }
    }

    fn finish(mut self) -> String {
        self.buf.push('\n');
        self.buf
    }
}

/// Top 进程列表压成一格:chrome.exe (1234) 42.0%
fn top_cell(list: &[TopProc], show: impl Fn(f64) -> String) -> String {
    list.iter()
        .map(|p| format!("{} ({}) {}", p.name, p.pid, show(p.value)))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn fmt_bps(v: f64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if v >= MIB {
        format!("{:.1} MB/s", v / MIB)
    } else {
        format!("{:.0} KB/s", v / 1024.0)
    }
}

/// 事件 → CSV 文本(带 UTF-8 BOM,Excel 双击即可正确显示中文)
pub fn to_csv(events: &[LagEvent]) -> String {
    let mut out = String::with_capacity(256 + events.len() * 400);
    out.push('\u{feff}');
    out.push_str(&CSV_HEADER.join(","));
    out.push('\n');
    for ev in events {
        let c = &ev.counters;
        let mut r = Row::new();
        r.raw(&ev.id.map(|i| i.to_string()).unwrap_or_default());
        r.raw(&local_datetime(ev.ts_ms));
        r.raw(&ev.ts_ms.to_string());
        r.raw(&joined(&ev.reasons, |x| x.kind.clone()));
        r.raw(&joined(&ev.reasons, |x| x.detail.clone()));
        r.raw(&joined(&ev.reasons, |x| format!("{:.1}", x.value)));
        r.num(c.cpu_total as f64, 1);
        r.num(pct(c.mem_used, c.mem_total), 1);
        r.num(pct(c.mem_commit_used, c.mem_commit_limit), 1);
        r.num(c.page_input_per_sec, 0);
        r.num(c.disk_active_pct, 1);
        r.num(c.disk_avg_sec_transfer * 1000.0, 2);
        r.num(c.disk_queue_length, 2);
        r.raw(&c.disk_read_bps.to_string());
        r.raw(&c.disk_write_bps.to_string());
        r.raw(&c.net_rx_bps.to_string());
        r.raw(&c.net_tx_bps.to_string());
        r.num(c.dpc_per_sec, 1);
        r.num(c.interrupt_pct, 1);
        r.opt(c.gpu_util_pct, 1);
        r.opt(c.gpu_mem_mb, 0);
        r.opt(c.temperature_c.map(|t| t as f64), 1);
        r.raw(&top_cell(&ev.top_cpu, |v| format!("{v:.1}%")));
        r.raw(&top_cell(&ev.top_mem, |v| format!("{v:.0} MB")));
        r.raw(&top_cell(&ev.top_disk, fmt_bps));
        r.raw(&ev.hung_windows.join(" | "));
        r.opt(ev.game.as_ref().map(|g| g.fps as f64), 0);
        r.opt(ev.game.as_ref().map(|g| g.max_gap_ms), 1);
        r.opt(ev.game.as_ref().map(|g| g.stutters as f64), 0);
        r.raw(ev.game.as_ref().map(|g| g.app.as_str()).unwrap_or(""));
        r.opt(ev.dpc_top.as_ref().map(|d| d.total as f64), 0);
        r.raw(
            &ev.dpc_top
                .as_ref()
                .map(|d| joined(&d.drivers, |(name, n)| format!("{name} {n}")))
                .unwrap_or_default(),
        );
        out.push_str(&r.finish());
    }
    out
}

fn joined<T>(items: &[T], f: impl Fn(&T) -> String) -> String {
    items.iter().map(f).collect::<Vec<_>>().join(" | ")
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

/// 导出信封:带上来源与条数,便于离线分析时确认数据出处
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    app: &'static str,
    version: &'static str,
    exported_at_ms: u64,
    exported_at_local: String,
    count: usize,
    events: &'a [LagEvent],
}

/// 事件 → 带信封的 JSON 文本(缩进两格,方便人读也方便 jq)
pub fn to_json(events: &[LagEvent]) -> Result<String, String> {
    let now = now_ms();
    let envelope = Envelope {
        app: "LagScope",
        version: env!("CARGO_PKG_VERSION"),
        exported_at_ms: now,
        exported_at_local: local_datetime(now),
        count: events.len(),
        events,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("序列化事件失败: {e}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 毫秒时间戳 → 本地时区的 SYSTEMTIME(失败返回 None)
fn local_systemtime(ts_ms: u64) -> Option<windows::Win32::Foundation::SYSTEMTIME> {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    // FILETIME 从 1601-01-01 起算、单位 100ns;与 Unix 纪元相差 11644473600 秒
    let ticks = (ts_ms as u128 + 11_644_473_600_000) * 10_000;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: 两个调用只读写本栈上的 FILETIME / SYSTEMTIME,不持有或释放任何句柄。
    unsafe {
        FileTimeToSystemTime(&ft, &mut utc).ok()?;
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            local = utc; // 拿不到本地时区时退回 UTC,不让整份导出失败
        }
    }
    Some(local)
}

/// 毫秒时间戳 → YYYY-MM-DD HH:MM:SS(本地时区)
pub fn local_datetime(ts_ms: u64) -> String {
    match local_systemtime(ts_ms) {
        Some(t) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        ),
        None => ts_ms.to_string(),
    }
}

/// 毫秒时间戳 → YYYYMMDD-HHMMSS(本地时区,用于文件名)
pub fn file_stamp(ts_ms: u64) -> String {
    match local_systemtime(ts_ms) {
        Some(t) => format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        ),
        None => ts_ms.to_string(),
    }
}

/// 把事件写进 dir,返回最终文件路径。
///
/// 目录不存在时自动创建;文件名带本地时间戳,重名时追加 -2、-3 后缀,
/// 不覆盖任何已有导出。
pub fn write_to(dir: &Path, events: &[LagEvent], fmt: ExportFormat) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("创建导出目录 {} 失败: {e}", dir.display()))?;
    let stem = format!("lagscope-events-{}", file_stamp(now_ms()));
    let body = match fmt {
        ExportFormat::Csv => to_csv(events),
        ExportFormat::Json => to_json(events)?,
    };
    let path = unique_path(dir, &stem, fmt.extension());
    std::fs::write(&path, body).map_err(|e| format!("写入 {} 失败: {e}", path.display()))?;
    Ok(path)
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem}-{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-overflow.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::MetricSample;

    fn proc(pid: u32, name: &str, value: f64) -> TopProc {
        TopProc {
            pid,
            name: name.to_string(),
            value,
        }
    }

    fn event(id: i64, detail: &str) -> LagEvent {
        LagEvent {
            id: Some(id),
            ts_ms: 1_700_000_000_000,
            reasons: vec![crate::engine::LagReason {
                kind: "cpu".into(),
                detail: detail.to_string(),
                value: 91.5,
            }],
            counters: MetricSample {
                ts_ms: 1_700_000_000_000,
                cpu_total: 91.5,
                mem_used: 9,
                mem_total: 10,
                mem_commit_used: 1,
                mem_commit_limit: 4,
                page_input_per_sec: 812.0,
                disk_active_pct: 63.5,
                disk_avg_sec_transfer: 0.012,
                disk_queue_length: 2.5,
                disk_read_bps: 4096,
                disk_write_bps: 1024,
                net_rx_bps: 512,
                net_tx_bps: 256,
                dpc_per_sec: 1234.0,
                interrupt_pct: 7.5,
                ..MetricSample::default()
            },
            top_cpu: vec![proc(100, "chrome.exe", 42.0)],
            top_mem: vec![proc(100, "chrome.exe", 2048.0)],
            top_disk: vec![proc(200, "svchost.exe", 3.0 * 1024.0 * 1024.0)],
            hung_windows: vec![],
            game: None,
            dpc_top: None,
        }
    }

    /// 最小 CSV 记录解析器:只为验证转义是否正确
    fn parse_record(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if in_quotes => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        cur.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                '"' => in_quotes = true,
                ',' if !in_quotes => out.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            }
        }
        out.push(cur);
        out
    }

    /// 跳过表头,取第一行数据(保留字段内部换行)
    fn first_record(csv: &str) -> String {
        let body = csv.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
        body.trim_end_matches('\n').to_string()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("export-tests")
            .join(tag);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn format_parsing_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(ExportFormat::from_name(" CSV ").unwrap(), ExportFormat::Csv);
        assert_eq!(ExportFormat::from_name("Json").unwrap(), ExportFormat::Json);
        assert!(ExportFormat::from_name("xml").is_err());
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
    }

    #[test]
    fn csv_has_bom_header_and_one_row_per_event() {
        let events = vec![event(1, "CPU 持续 91.5%"), event(2, "内存压力")];
        let csv = to_csv(&events);
        assert!(csv.starts_with('\u{feff}'), "缺少 UTF-8 BOM");
        let lines: Vec<&str> = csv.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 3, "应为表头 + 2 行数据");
        let head = parse_record(lines[0].trim_start_matches('\u{feff}'));
        assert_eq!(head, CSV_HEADER.to_vec());
        for line in &lines[1..] {
            assert_eq!(parse_record(line).len(), CSV_HEADER.len());
        }
    }

    #[test]
    fn csv_escapes_commas_quotes_and_newlines() {
        let nasty = "磁盘\",延迟,[高]\n等待";
        let csv = to_csv(&[event(7, nasty)]);
        let cells = parse_record(&first_record(&csv));
        assert_eq!(cells.len(), CSV_HEADER.len());
        assert_eq!(cells[4], nasty);
        assert_eq!(cells[0], "7");
        assert_eq!(cells[6], "91.5");
        assert_eq!(cells[7], "90.0");
        assert_eq!(cells[11], "12.00");
        assert_eq!(cells[22], "chrome.exe (100) 42.0%");
        assert_eq!(cells[24], "svchost.exe (200) 3.0 MB/s");
    }

    #[test]
    fn csv_leaves_missing_optional_metrics_empty() {
        let csv = to_csv(&[event(1, "x")]);
        let cells = parse_record(&first_record(&csv));
        // GPU% / 显存 / 温度 / 帧率 / 最大帧间隔 / 卡帧数 / 渲染进程 / DPC 总数
        for idx in [19, 20, 21, 26, 27, 28, 29, 30] {
            assert_eq!(cells[idx], "", "第 {idx} 列应为空: {}", CSV_HEADER[idx]);
        }
        // DPC 驱动列同理为空,但表头仍占位
        assert_eq!(cells[31], "");
    }

    #[test]
    fn json_envelope_counts_events_and_keeps_camel_case_fields() {
        let events = vec![event(1, "a"), event(2, "b")];
        let text = to_json(&events).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["app"], "LagScope");
        assert_eq!(v["count"], 2);
        assert_eq!(v["events"].as_array().unwrap().len(), 2);
        assert_eq!(v["events"][0]["tsMs"], 1_700_000_000_000u64);
        assert_eq!(v["events"][0]["topCpu"][0]["name"], "chrome.exe");
    }

    #[test]
    fn timestamps_render_in_a_stable_shape() {
        let text = local_datetime(1_700_000_000_000);
        assert_eq!(text.len(), 19, "应形如 2023-11-15 06:13:20,实际 {text}");
        assert_eq!(text.as_bytes()[4], b'-');
        assert_eq!(text.as_bytes()[10], b' ');
        let stamp = file_stamp(1_700_000_000_000);
        assert_eq!(stamp.len(), 15, "文件名时间戳应形如 20231115-061320");
        assert_eq!(stamp.as_bytes()[8], b'-');
    }

    #[test]
    fn write_to_creates_directory_and_never_overwrites() {
        let dir = temp_dir("write-to");
        let events = vec![event(1, "x")];
        let first = write_to(&dir, &events, ExportFormat::Csv).unwrap();
        assert!(first.exists(), "首个导出文件应存在");
        assert_eq!(first.extension().unwrap(), "csv");
        let name = first.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("lagscope-events-"),
            "文件名应带时间戳: {name}"
        );
        let body = std::fs::read_to_string(&first).unwrap();
        assert!(body.starts_with('\u{feff}'));
        assert_eq!(body.lines().count(), 2, "表头 + 1 行数据");

        let second = write_to(&dir, &events, ExportFormat::Csv).unwrap();
        assert_ne!(first, second, "重名时必须另起文件名,不能覆盖");
        assert!(first.exists() && second.exists());

        let json = write_to(&dir, &events, ExportFormat::Json).unwrap();
        assert_eq!(json.extension().unwrap(), "json");
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(parsed["count"], 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
