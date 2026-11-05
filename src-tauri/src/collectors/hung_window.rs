//! 挂起(假死)窗口检测:遍历可见顶层窗口,
//! `SendMessageTimeout(WM_NULL, SMTO_ABORTIFHUNG)` 超时即视为无响应。
//! 与任务管理器"未响应"的判定一致(IsHungAppWindow 语义)。

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW,
    SMTO_ABORTIFHUNG, WM_NULL,
};

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

struct Ctx {
    out: Vec<(u32, String)>,
    self_pid: u32,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL { unsafe {
    let ctx = &mut *(lparam.0 as *mut Ctx);
    // 不可见窗口跳过
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut buf = [0u16; 128];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len == 0 {
        return BOOL(1);
    }
    let title = String::from_utf16_lossy(&buf[..len as usize]);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == ctx.self_pid {
        return BOOL(1);
    }
    // 2 秒内不处理消息 → 挂起
    let mut result_ptr = 0usize;
    let res = SendMessageTimeoutW(
        hwnd,
        WM_NULL,
        WPARAM(0),
        LPARAM(0),
        SMTO_ABORTIFHUNG,
        2000,
        Some(&mut result_ptr),
    );
    if res.0 == 0 {
        ctx.out.push((pid, title));
    }
    BOOL(1)
}}

/// 返回当前挂起窗口列表,如 "无响应 - Chrome (pid 1234)"
pub fn detect_hung_windows() -> Vec<String> {
    let mut ctx = Ctx {
        out: Vec::new(),
        self_pid: std::process::id(),
    };
    unsafe {
        // 回调始终返回 TRUE,不会产生错误
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.out
        .into_iter()
        .map(|(pid, title)| format!("{title} (pid {pid})"))
        .collect()
}

/// 挂起窗口后台监视:独立线程周期检测。
/// 检测可能阻塞(每个窗口最多等 2s),绝不能放采集线程,否则满载时会把 1Hz 采样拖死。
pub struct HungWatch {
    cache: Arc<std::sync::Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

const CHECK_INTERVAL: Duration = Duration::from_secs(5);

impl HungWatch {
    pub fn start() -> Self {
        let cache = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (cache2, stop2) = (cache.clone(), stop.clone());
        let handle = thread::Builder::new()
            .name("hung-watch".into())
            .spawn(move || {
                while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                    let found = detect_hung_windows();
                    if let Ok(mut c) = cache2.lock() {
                        *c = found;
                    }
                    thread::sleep(CHECK_INTERVAL);
                }
            })
            .ok();
        Self {
            cache,
            stop,
            handle,
        }
    }

    /// 当前缓存的挂起窗口列表(读取无阻塞检测)
    pub fn snapshot(&self) -> Vec<String> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for HungWatch {
    fn drop(&mut self) {
        self.stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
