//! 运行日志:滚动文件(5MB × 3),替代 release 下被 cfg(debug_assertions) 关掉的 eprintln。
//!
//! 设计取舍:
//! - 不引第三方日志框架:只有一套固定格式的轻量写入,env_logger 之类反而要额外初始化与过滤配置。
//! - 时间戳存 epoch 毫秒而非本地时间字符串:写入侧不依赖时区/chrono,前端按浏览器本地时区展示。
//! - 每行写完立即 flush:用户打开日志页要能马上看到刚发生的故障,不能等缓冲区满。
//! - 写入失败一律静默:日志是旁路观测,绝不能因为磁盘满就把采集线程带崩。

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const MAX_FILES: usize = 3;
const LOG_NAME: &str = "lagscope.log";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO ",
            Self::Warn => "WARN ",
            Self::Error => "ERROR",
        }
    }
}

/// 前端拿到的单行日志(字段已是 camelCase)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub ts_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

struct Logger {
    dir: PathBuf,
    /// Option 便于滚动时先 close 再 rename(Windows 不允许重命名已打开的文件)
    file: Option<File>,
    len: u64,
}

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn rotated_name(i: usize) -> String {
    format!("lagscope.{i}.log")
}

/// 把当前文件顺移一格并丢弃最旧的一格。调用前必须已关闭当前文件句柄。
fn rotate(dir: &Path) -> std::io::Result<()> {
    let _ = fs::remove_file(dir.join(rotated_name(MAX_FILES)));
    for i in (1..MAX_FILES).rev() {
        let from = dir.join(rotated_name(i));
        if from.exists() {
            let _ = fs::rename(&from, dir.join(rotated_name(i + 1)));
        }
    }
    let current = dir.join(LOG_NAME);
    if current.exists() {
        fs::rename(current, dir.join(rotated_name(1)))?;
    }
    Ok(())
}

/// 在 app_data_dir 下初始化日志。只调用一次;重复调用忽略(单实例插件保证进程唯一)。
pub fn init(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_NAME);
    let mut len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if len >= MAX_BYTES {
        rotate(dir)?;
        len = 0;
    }
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let _ = LOGGER.set(Mutex::new(Logger {
        dir: dir.to_path_buf(),
        file: Some(file),
        len,
    }));
    Ok(())
}

fn write(level: Level, target: &str, message: impl AsRef<str>) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    let Ok(mut logger) = logger.lock() else {
        return;
    };
    let line = format!(
        "[{}] [{}] [{}] {}\n",
        now_ms(),
        level.label(),
        target,
        message.as_ref()
    );
    if logger.len + line.len() as u64 > MAX_BYTES {
        logger.file.take();
        if rotate(&logger.dir).is_ok()
            && let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(logger.dir.join(LOG_NAME))
        {
            logger.file = Some(file);
            logger.len = 0;
        }
    }
    if let Some(file) = logger.file.as_mut()
        && file.write_all(line.as_bytes()).is_ok()
    {
        let _ = file.flush();
        logger.len += line.len() as u64;
    }
}

pub fn debug(target: &str, message: impl AsRef<str>) {
    write(Level::Debug, target, message)
}
pub fn info(target: &str, message: impl AsRef<str>) {
    write(Level::Info, target, message)
}
pub fn warn(target: &str, message: impl AsRef<str>) {
    write(Level::Warn, target, message)
}
pub fn error(target: &str, message: impl AsRef<str>) {
    write(Level::Error, target, message)
}

/// 解析一行日志。消息本身可能含任意字符,只按固定前缀切三段。
fn parse_line(line: &str) -> Option<LogLine> {
    let (ts, rest) = line.strip_prefix('[')?.split_once("] [")?;
    let (level, rest) = rest.split_once("] [")?;
    let (target, message) = rest.split_once("] ")?;
    Some(LogLine {
        ts_ms: ts.trim().parse().ok()?,
        level: level.trim().to_string(),
        target: target.to_string(),
        message: message.trim_end().to_string(),
    })
}

fn read_into(path: PathBuf, out: &mut Vec<LogLine>, keep: usize) {
    let Ok(file) = File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Some(entry) = parse_line(&line) {
            out.push(entry);
            // 单个文件最大 5MB,不能把全部行都读进内存再截断
            if out.len() > keep * 2 {
                out.drain(..out.len() - keep);
            }
        }
    }
}

/// 读取最近的日志(旧 → 新),最多返回 limit 行。
#[tauri::command]
pub fn get_logs(limit: Option<u32>) -> Result<Vec<LogLine>, String> {
    let Some(logger) = LOGGER.get() else {
        return Ok(Vec::new());
    };
    let dir = logger
        .lock()
        .map_err(|_| "日志锁被占用".to_string())?
        .dir
        .clone();
    let keep = limit.unwrap_or(300).min(2000) as usize;
    let mut out = Vec::with_capacity(keep + 8);
    // 编号越大越旧:3 → 2 → 1 → 当前
    for i in (1..=MAX_FILES).rev() {
        read_into(dir.join(rotated_name(i)), &mut out, keep);
    }
    read_into(dir.join(LOG_NAME), &mut out, keep);
    if out.len() > keep {
        out.drain(..out.len() - keep);
    }
    Ok(out)
}

/// 清空日志:删掉所有滚动文件,并把当前文件截断。
#[tauri::command]
pub fn clear_logs() -> Result<(), String> {
    let logger = LOGGER.get().ok_or_else(|| "日志尚未初始化".to_string())?;
    let mut logger = logger.lock().map_err(|_| "日志锁被占用".to_string())?;
    logger.file.take();
    for i in 1..=MAX_FILES {
        let _ = fs::remove_file(logger.dir.join(rotated_name(i)));
    }
    let path = logger.dir.join(LOG_NAME);
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
    {
        Ok(file) => {
            logger.file = Some(file);
            logger.len = 0;
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 在资源管理器中打开日志目录。Windows 桌面应用,直接调 explorer 最省依赖。
#[tauri::command]
pub fn open_log_dir() -> Result<(), String> {
    let logger = LOGGER.get().ok_or_else(|| "日志尚未初始化".to_string())?;
    let dir = logger
        .lock()
        .map_err(|_| "日志锁被占用".to_string())?
        .dir
        .clone();
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[test]
    fn parse_roundtrip() {
        let line = "[1763475907123] [ERROR] [presentmon] 启动失败: boom\n";
        let parsed = parse_line(line).expect("should parse");
        assert_eq!(parsed.ts_ms, 1_763_475_907_123);
        assert_eq!(parsed.level, "ERROR");
        assert_eq!(parsed.target, "presentmon");
        assert_eq!(parsed.message, "启动失败: boom");
    }

    #[test]
    fn parse_ignores_garbage() {
        assert!(parse_line("").is_none());
        assert!(parse_line("not a log line").is_none());
        assert!(parse_line("[x] [INFO] [t] hello").is_none());
    }

    #[test]
    fn level_labels_are_fixed_width() {
        for level in [Level::Debug, Level::Info, Level::Warn, Level::Error] {
            assert_eq!(level.label().len(), 5);
        }
    }

    #[test]
    fn fmt_arguments_are_accepted() {
        fn take(_: impl fmt::Display) {}
        take(format_args!("x={}", 1));
    }
}
