//! 管理员权限检测与提权重启。
//!
//! DPC 内核 ETW 会话与 HKLM 启动项写入都需要管理员权限。普通权限下这两处只会
//! 静默失败:用户看到的是「打开了但没生效」,而真正的原因只留在控制台里。
//! 这里把「当前有没有权限」与「怎么拿到权限」收敛到一处,前端只调用这两个能力。

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::HRESULT;

/// 提权重启时传给新进程的参数,后面跟父进程 pid
const RELAUNCH_WAIT_FLAG: &str = "--relaunch-wait-pid";
/// 等父进程退出的上限;正常退出在几十毫秒量级,留足余量防卡死
const RELAUNCH_WAIT_MS: u32 = 10_000;

/// 当前进程是否以管理员权限运行。
///
/// 直接查进程令牌的 TokenElevation,而不是「试着做一次需要权限的操作再看结果」:
/// 后者会留下副作用(比如真建了一个 ETW 会话又要去清理)。
pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        // 查询失败一律按「没有权限」处理:相关功能后面会自己再失败一次并给出原因,
        // 多报一次提权引导比漏报好
        queried && elevation.TokenIsElevated != 0
    }
}

/// 提权重启的结果。前端据此区分「用户点了否」与「真的失败了」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RelaunchOutcome {
    /// 新进程已创建,当前进程应随即退出
    Started,
    /// 用户在 UAC 对话框上点了「否」
    Cancelled,
}

/// 以管理员身份重启自身。
///
/// 返回 Started 时调用方负责退出当前进程。
pub fn relaunch_as_admin() -> Result<RelaunchOutcome, String> {
    use windows::core::PCWSTR;

    let exe = std::env::current_exe().map_err(|e| format!("取不到自身路径: {e}"))?;
    let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    // 把自己 pid 传给新进程:新进程要等我们真正退出,否则单实例插件会把
    // 「父进程还活着」当成「已经有一个实例在跑」,转发完参数就自己退出了。
    let params = format!("{RELAUNCH_WAIT_FLAG} {}", std::process::id());

    let verb_w = wide(OsStr::new("runas"));
    let exe_w = wide(exe.as_os_str());
    let params_w = wide(OsStr::new(params.as_str()));
    let dir_w = wide(dir.as_os_str());

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR::from_raw(verb_w.as_ptr()),
        lpFile: PCWSTR::from_raw(exe_w.as_ptr()),
        lpParameters: PCWSTR::from_raw(params_w.as_ptr()),
        lpDirectory: PCWSTR::from_raw(dir_w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => {
            // 只要确认它起来了;进程句柄不关会随本进程退出一起回收
            let _ = unsafe { CloseHandle(info.hProcess) };
            Ok(RelaunchOutcome::Started)
        }
        // UAC 被拒是正常操作,不该弹成红框
        Err(e) if is_uac_cancelled(&e) => Ok(RelaunchOutcome::Cancelled),
        Err(e) => Err(format!("提权重启失败: {e}")),
    }
}

/// UAC 被取消的判定。
///
/// ShellExecuteExW 失败时 windows crate 把 GetLastError 包成 HRESULT(例如
/// ERROR_CANCELLED=1223 → 0x800704C7),直接拿 1223 比较永远不会成立。
fn is_uac_cancelled(err: &windows::core::Error) -> bool {
    err.code() == HRESULT::from_win32(ERROR_CANCELLED.0)
}

/// 若本进程是「提权重启」拉起来的,等父进程完全退出后再继续。
///
/// 必须在注册 single-instance 插件之前调用:父进程还活着时它持有命名互斥体,
/// 此时新进程会被当成第二个实例,转发完参数就退出,用户看到的是「点了重启然后没了」。
pub fn wait_for_relaunch_target() {
    let Some(pid) = parse_wait_pid(std::env::args()) else {
        return;
    };
    if pid == 0 || pid == std::process::id() {
        return;
    }
    unsafe {
        // 拿不到句柄说明父进程已经没了,直接继续
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return;
        };
        let _ = WaitForSingleObject(handle, RELAUNCH_WAIT_MS);
        let _ = CloseHandle(handle);
    }
}

/// 从命令行里取出父进程 pid。
fn parse_wait_pid<I: Iterator<Item = String>>(mut args: I) -> Option<u32> {
    while let Some(arg) = args.next() {
        if arg == RELAUNCH_WAIT_FLAG {
            return args.next().and_then(|v| v.parse().ok());
        }
    }
    None
}

/// UTF-16 编码并补 NUL。Windows API 不接受缺尾零的字符串。
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_pid_after_the_flag() {
        let args = args_of(&["lagscope.exe", RELAUNCH_WAIT_FLAG, "4321"]);
        assert_eq!(parse_wait_pid(args.into_iter()), Some(4321));
    }

    #[test]
    fn ignores_normal_launch() {
        let args = args_of(&["lagscope.exe"]);
        assert_eq!(parse_wait_pid(args.into_iter()), None);
    }

    #[test]
    fn missing_or_broken_pid_is_ignored() {
        let tail = args_of(&["lagscope.exe", RELAUNCH_WAIT_FLAG]);
        assert_eq!(parse_wait_pid(tail.into_iter()), None);
        let broken = args_of(&["lagscope.exe", RELAUNCH_WAIT_FLAG, "abc"]);
        assert_eq!(parse_wait_pid(broken.into_iter()), None);
    }

    #[test]
    fn elevation_probe_does_not_panic() {
        // 只验证 FFI 调用路径可用;具体真假取决于跑测试的终端是不是管理员
        let _ = is_elevated();
    }

    #[test]
    fn recognises_uac_cancel_hrresult() {
        let cancelled = windows::core::Error::from_hresult(HRESULT::from_win32(ERROR_CANCELLED.0));
        assert!(is_uac_cancelled(&cancelled));
        // 拒绝访问(0x80070005)不是「用户点了否」,不能当成正常取消
        let denied = windows::core::Error::from_hresult(HRESULT::from_win32(5));
        assert!(!is_uac_cancelled(&denied));
    }
}
