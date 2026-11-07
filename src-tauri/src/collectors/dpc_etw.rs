//! ETW 内核 DPC 按驱动归因:
//! 内核 ETW 会话(PerfInfo,DPC 子类型)捕获每次 DPC 例程调用,
//! 例程内核地址落入 [`EnumDeviceDrivers`] 模块表的哪个模块即归因到该驱动,
//! 按秒聚合成 per-driver 计数。
//!
//! 需要管理员权限;失败时 available=false,上层优雅降级。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferrisetw::parser::Parser;
use ferrisetw::provider::kernel_providers::DPC_PROVIDER;
use ferrisetw::provider::Provider;
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::KernelTrace;
use crate::engine::DpcSample;

/// 内核模块表:按基址排序,用于例程地址 → 驱动名
struct ModuleTable {
    modules: Vec<(u64, u64, String)>, // (基址, 大小, 名)
}

impl ModuleTable {
    fn refresh() -> Self {
        let mut mods: Vec<(u64, u64, String)> = Vec::new();
        unsafe {
            // NtQuerySystemInformation(SystemModuleInformation):psapi 对驱动模块
            // 取不到名字,此路径稳定。实测布局:buf 头 8 字节(Count 4 + 填充 4),
            // entry stride 296:+16..24 ImageBase,+24..28 ImageSize,
            // +34..36 NameOffset,+40 起为完整路径文本
            type NtQuerySystemInformationFn =
                unsafe extern "system" fn(u32, *mut core::ffi::c_void, u32, *mut u32) -> i32;
            let ntdll = windows::Win32::System::LibraryLoader::GetModuleHandleW(
                windows::core::w!("ntdll.dll"),
            )
            .expect("加载 ntdll");
            let proc_addr = windows::Win32::System::LibraryLoader::GetProcAddress(
                ntdll,
                windows::core::s!("NtQuerySystemInformation"),
            )
            .expect("NtQuerySystemInformation 不存在");
            let nt_query: NtQuerySystemInformationFn = std::mem::transmute(proc_addr);

            const SYSTEM_MODULE_INFORMATION: u32 = 11;
            const ENTRY_HDR: usize = 8;
            const STRIDE: usize = 296;
            let mut len = 0u32;
            let _ = nt_query(SYSTEM_MODULE_INFORMATION, std::ptr::null_mut(), 0, &mut len);
            if len == 0 {
                return Self { modules: mods };
            }
            let mut buf = vec![0u8; len as usize];
            let status = nt_query(
                SYSTEM_MODULE_INFORMATION,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                len,
                &mut len,
            );
            if status < 0 {
                return Self { modules: mods };
            }

            let count = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
            let usable = (buf.len() - ENTRY_HDR) / STRIDE;
            let count = count.min(usable);
            for i in 0..count {
                let off = ENTRY_HDR + i * STRIDE;
                let image_base =
                    u64::from_le_bytes(buf[off + 16..off + 24].try_into().unwrap());
                // 完整路径文本(\SystemRoot\...),以 NUL 截断
                let path_buf = &buf[off + 40..off + STRIDE];
                let path_end = path_buf
                    .iter()
                    .position(|b| *b == 0)
                    .unwrap_or(path_buf.len());
                let path = String::from_utf8_lossy(&path_buf[..path_end]);
                // 文件名取最后一个反斜杠之后
                let name = path
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&path)
                    .to_string();
                if name.is_empty() || image_base == 0 {
                    continue;
                }
                mods.push((image_base, 0, name));
            }
        }
        mods.sort_by_key(|(base, _, _)| *base);
        Self { modules: mods }
    }

    /// 地址 → 驱动名(按排序基址二分;命中最后一个 ≤ 地址的模块)
    fn lookup(&self, addr: u64) -> Option<&str> {
        let idx = self
            .modules
            .partition_point(|(base, _, _)| *base <= addr)
            .checked_sub(1)?;
        Some(self.modules[idx].2.as_str())
    }

    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

/// 停止同名残留内核会话(进程被强杀时 Drop 不会执行,会话会泄漏)
fn stop_stale_session() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("logman")
        .args(["stop", "LagScopeDpc", "-ets"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub struct DpcWatch {
    latest: Arc<Mutex<Option<DpcSample>>>,
    available: Arc<AtomicBool>,
    /// 置位后会话线程停止 trace 并退出(设置开关用)
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl DpcWatch {
    /// 启动 ETW 会话与解析线程。非管理员时 KernelTrace 创建失败 → available=false。
    pub fn start() -> Self {
        // 强杀进程会泄漏命名内核会话,启动前先停掉同名残留
        stop_stale_session();
        let watch = Self {
            latest: Arc::new(Mutex::new(None)),
            available: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
        };

        let (tx, rx) = std::sync::mpsc::channel::<u64>();
        let latest = watch.latest.clone();
        let available = watch.available.clone();
        let stop_session = watch.stop.clone();
        let stop_mt = watch.stop.clone();

        let module_table = Arc::new(Mutex::new(ModuleTable::refresh()));
        // 模块表 30s 刷新(随停止标志退出)
        {
            let mt = module_table.clone();
            thread::Builder::new()
                .name("dpc-module-table".into())
                .spawn(move || loop {
                    thread::sleep(Duration::from_secs(30));
                    if stop_mt.load(Ordering::Relaxed) {
                        return;
                    }
                    *mt.lock().unwrap_or_else(|e| e.into_inner()) = ModuleTable::refresh();
                })
                .ok();
        }

        // 计数聚合线程:接收例程地址,按秒归并
        thread::Builder::new()
            .name("dpc-aggregator".into())
            .spawn(move || {
                let mut cur_sec = now_ms() / 1000;
                let mut counts: HashMap<String, u32> = HashMap::new();
                let mut total = 0u32;
                for addr in rx {
                    let sec = now_ms() / 1000;
                    if sec != cur_sec {
                        flush(&mut counts, &mut total, cur_sec, &latest, &module_table);
                        cur_sec = sec;
                    }
                    total += 1;
                    if let Some(name) = {
                        let mt = module_table.lock().unwrap_or_else(|e| e.into_inner());
                        mt.lookup(addr).map(|s| s.to_string())
                    } {
                        *counts.entry(name).or_insert(0) += 1;
                    }
                }
            })
            .ok();

        // ETW 会话线程
        let handle = thread::Builder::new()
            .name("dpc-etw".into())
            .spawn(move || {
                let callback = move |record: &ferrisetw::EventRecord,
                                     locator: &SchemaLocator| {
                    if record.opcode() != 68 {
                        return; // DPC 子类型(PerfInfo opcode 68)
                    }
                    if let Ok(schema) = locator.event_schema(record) {
                        let parser = Parser::create(record, &schema);
                        if let Ok(routine) = parser.try_parse::<u64>("Routine") {
                            let _ = tx.send(routine);
                        }
                    }
                };
                let provider = Provider::kernel(&DPC_PROVIDER)
                    .add_callback(callback)
                    .build();
                match KernelTrace::new()
                    .named(String::from("LagScopeDpc"))
                    .enable(provider)
                    .start_and_process()
                {
                    Ok(trace) => {
                        available.store(true, Ordering::Relaxed);
                        // 会话线程持有 trace,停止标志置位后结束会话
                        while !stop_session.load(Ordering::Relaxed) {
                            thread::sleep(Duration::from_millis(500));
                        }
                        let _ = trace.stop();
                    }
                    Err(e) => {
                        eprintln!("[dpc-etw] 内核会话创建失败(需管理员): {e:?}");
                    }
                }
            })
            .ok();

        // 存会话线程句柄供 Drop 等待
        if let Ok(mut h) = watch.handle.lock() {
            *h = handle;
        }

        watch
    }

    pub fn snapshot(&self) -> Option<DpcSample> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }
}

impl Drop for DpcWatch {
    fn drop(&mut self) {
        // 停止标志 → 会话线程 trace.stop() 并退出
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut h) = self.handle.lock()
            && let Some(h) = h.take() {
                let _ = h.join();
            }
    }
}

fn flush(
    counts: &mut HashMap<String, u32>,
    total: &mut u32,
    sec: u64,
    latest: &Arc<Mutex<Option<DpcSample>>>,
    module_table: &Arc<Mutex<ModuleTable>>,
) {
    if *total == 0 {
        return;
    }
    let mut drivers: Vec<(String, u32)> =
        counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    drivers.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    drivers.truncate(5);
    let sample = DpcSample {
        ts_ms: sec * 1000,
        total: *total,
        drivers,
    };
    let _ = module_table; // 查找已在回调线程完成
    *latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(sample);
    counts.clear();
    *total = 0;
}

/// 供测试:模块表查找逻辑
#[cfg(test)]
mod tests {
    use super::*;

    /// 真实内核会话验证:需管理员,默认 ignore(`cargo test -- --ignored` 运行)
    #[test]
    #[ignore]
    fn dpc_watch_real_attribution() {
        let w = DpcWatch::start();
        // 等两个聚合窗口(会话启动 + 数据流入)
        std::thread::sleep(Duration::from_secs(4));
        let snap = w
            .snapshot()
            .expect("管理员下 DPC 快照应存在(内核会话+数据流)");
        assert!(snap.total > 0, "应有 DPC 事件: total={}", snap.total);
        assert!(
            !snap.drivers.is_empty(),
            "应归因到至少一个驱动: {:?}",
            snap.drivers
        );
        println!(
            "dpc total={} top={:?}",
            snap.total,
            snap.drivers.iter().take(3).collect::<Vec<_>>()
        );
        // 系统核心驱动 ntoskrnl 应该出现在归因里
        assert!(
            snap.drivers.iter().any(|(n, _)| n.contains("ntoskrnl")
                || n.contains("ndis")
                || n.contains("ACPI")
                || n.contains("Wdf")),
            "应有常见内核驱动: {:?}",
            snap.drivers
        );
    }

    #[test]
    fn module_table_lookup() {
        let table = ModuleTable {
            modules: vec![
                (0x1000, 0, "ntoskrnl.exe".into()),
                (0x5000, 0, "nvlddmkm.sys".into()),
                (0x9000, 0, "dxgkrnl.sys".into()),
            ],
        };
        assert_eq!(table.lookup(0x1000), Some("ntoskrnl.exe"));
        assert_eq!(table.lookup(0x4FFF), Some("ntoskrnl.exe"));
        assert_eq!(table.lookup(0x5000), Some("nvlddmkm.sys"));
        assert_eq!(table.lookup(0x8ABC), Some("nvlddmkm.sys"));
        assert_eq!(table.lookup(0x9000), Some("dxgkrnl.sys"));
        assert_eq!(table.lookup(0x0FFF), None); // 低于首个基址
    }
}
