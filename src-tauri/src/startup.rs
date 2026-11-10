//! 启动项扫描与管理:
//! - 注册表 Run 键(HKCU / HKLM / HKLM WOW6432Node)
//! - 启动文件夹(当前用户 / 所有用户)
//! - 启用状态来自 Explorer 的 StartupApproved 键(与任务管理器同机制):
//!   值存在且首字节为奇数(如 0x03)= 已禁用;值不存在或首字节偶数 = 启用
//! - 禁用 = 写 StartupApproved 值;启用 = 删除该值;目标文件缺失 = 残留项提示

use serde::{Deserialize, Serialize};
use winreg::enums::*;
use winreg::{HKEY, RegKey, RegValue};

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_SUBKEY_WOW: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED_SUBKEY_WOW: &str =
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// 启动项来源标识(禁用/启用操作需要原样传回)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupSource {
    /// HKCU Run
    UserRun,
    /// HKLM Run
    MachineRun,
    /// HKLM WOW6432Node Run
    MachineRunWow,
    /// 用户启动文件夹
    UserFolder,
    /// 所有用户启动文件夹
    CommonFolder,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// 显示名(注册表值名 / 文件名)
    pub name: String,
    pub source: StartupSource,
    /// 来源位置的人类可读描述
    pub source_label: String,
    /// 完整命令行(文件夹项为文件路径)
    pub command: String,
    /// 解析出的目标路径
    pub target: Option<String>,
    /// 目标文件是否存在
    pub target_exists: bool,
    /// 目标文件大小字节
    pub target_size: Option<u64>,
    /// 任务管理器口径的启用状态
    pub enabled: bool,
    /// 简要建议
    pub advice: String,
}

/// (根键, Run 子键, StartupApproved 子键, 显示标签)
fn source_meta(source: StartupSource) -> (HKEY, &'static str, &'static str, &'static str) {
    match source {
        StartupSource::UserRun => (
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            APPROVED_SUBKEY,
            "当前用户注册表 Run",
        ),
        StartupSource::MachineRun => (
            HKEY_LOCAL_MACHINE,
            RUN_SUBKEY,
            APPROVED_SUBKEY,
            "系统注册表 Run(修改需管理员权限)",
        ),
        StartupSource::MachineRunWow => (
            HKEY_LOCAL_MACHINE,
            RUN_SUBKEY_WOW,
            APPROVED_SUBKEY_WOW,
            "系统注册表 Run WOW64(修改需管理员权限)",
        ),
        StartupSource::UserFolder | StartupSource::CommonFolder => {
            unreachable!("文件夹来源不走注册表路径")
        }
    }
}

/// 读取 StartupApproved 判定启用状态;值不存在视为启用
fn is_disabled(root: HKEY, approved_subkey: &str, name: &str) -> bool {
    let hk = RegKey::predef(root);
    hk.open_subkey_with_flags(approved_subkey, KEY_READ)
        .and_then(|k| k.get_raw_value(name))
        .map(|v| v.bytes.first().is_some_and(|b| b & 1 == 1))
        .unwrap_or(false)
}

/// 从命令行提取目标 exe 路径:去引号、剥离参数、展开 %VAR%
fn extract_target(command: &str) -> Option<String> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return None;
    }
    let first = if let Some(rest) = cmd.strip_prefix('"') {
        match rest.find('"') {
            Some(i) => &rest[..i],
            None => rest.split_whitespace().next().unwrap_or(rest),
        }
    } else {
        cmd.split_whitespace().next().unwrap_or(cmd)
    };
    let p = expand_env(first).trim().to_string();
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let var = &after[..end];
                out.push_str(&std::env::var(var).unwrap_or(format!("%{var}%")));
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn scan_registry_source(source: StartupSource, out: &mut Vec<StartupItem>) {
    let (root, run_subkey, approved_subkey, label) = source_meta(source);
    let hk = RegKey::predef(root);
    let Ok(run_key) = hk.open_subkey_with_flags(run_subkey, KEY_READ) else {
        return;
    };
    for name in run_key.enum_values().flatten().map(|(n, _)| n) {
        let Ok(command) = run_key.get_value::<String, _>(&name) else {
            continue;
        };
        let target = extract_target(&command);
        let (target_exists, target_size) = target
            .as_ref()
            .map(|t| match std::fs::metadata(std::path::Path::new(t)) {
                Ok(m) if m.is_file() => (true, Some(m.len())),
                _ => (false, None),
            })
            .unwrap_or((false, None));
        let disabled = is_disabled(root, approved_subkey, &name);
        let advice = if disabled {
            "已禁用(不影响开机)".to_string()
        } else if !target_exists {
            "目标文件缺失,疑似残留项,可清理".to_string()
        } else {
            String::new()
        };
        out.push(StartupItem {
            name,
            source,
            source_label: label.to_string(),
            command,
            target,
            target_exists,
            target_size,
            enabled: !disabled,
            advice,
        });
    }
}

fn startup_folder_paths() -> Vec<(std::path::PathBuf, StartupSource, &'static str)> {
    let mut v = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        v.push((
            std::path::PathBuf::from(appdata)
                .join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            StartupSource::UserFolder,
            "当前用户启动文件夹",
        ));
    }
    if let Some(pdata) = std::env::var_os("PROGRAMDATA") {
        v.push((
            std::path::PathBuf::from(pdata)
                .join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
            StartupSource::CommonFolder,
            "所有用户启动文件夹",
        ));
    }
    v
}

/// 扫描全部启动项(注册表 + 启动文件夹)
pub fn list_startup() -> Vec<StartupItem> {
    let mut items = Vec::new();
    scan_registry_source(StartupSource::UserRun, &mut items);
    scan_registry_source(StartupSource::MachineRun, &mut items);
    scan_registry_source(StartupSource::MachineRunWow, &mut items);

    for (dir, source, label) in startup_folder_paths() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let target_size = std::fs::metadata(&path).ok().map(|m| m.len());
            items.push(StartupItem {
                name,
                source,
                source_label: label.to_string(),
                command: path.to_string_lossy().into_owned(),
                target: Some(path.to_string_lossy().into_owned()),
                target_exists: true,
                target_size,
                enabled: true,
                advice: "文件夹快捷方式:如需禁用请删除该文件".to_string(),
            });
        }
    }
    items.sort_by_key(|a| a.name.to_lowercase());
    items
}

/// 禁用/启用注册表启动项(写/删 StartupApproved,与任务管理器同机制)。
///
/// 系统级(HKLM / HKLM WOW64)项目写 StartupApproved 需要管理员权限,
/// 无权限时返回带 `需要管理员` 字样的错误,前端据此提示"以管理员身份重启"。
pub fn set_startup_enabled_impl(
    source: StartupSource,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    if matches!(source, StartupSource::UserFolder | StartupSource::CommonFolder) {
        return Err("启动文件夹项请在文件管理器中删除对应快捷方式".into());
    }
    let (root, _run_subkey, approved_subkey, label) = source_meta(source);
    let hk = RegKey::predef(root);
    let approved = hk
        .open_subkey_with_flags(approved_subkey, KEY_SET_VALUE)
        .or_else(|_| hk.create_subkey(approved_subkey).map(|(k, _)| k))
        .map_err(|e| format!("打开 {label} 的 StartupApproved 失败: {e}(需要管理员权限)"))?;

    if enabled {
        match approved.delete_value(name) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // 本就是启用态
            Err(e) => Err(format!("启用失败: {e}")),
        }
    } else {
        // 12 字节:首字节 0x03(禁用)+ 8 字节 FILETIME + 3 字节填充,任务管理器同格式
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Windows FILETIME = 100ns 自 1601-01-01
        let filetime = (now + 11_644_473_600) * 10_000_000;
        let mut bytes = vec![0x03u8, 0, 0, 0];
        bytes.extend_from_slice(&filetime.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
        approved
            .set_raw_value(
                name,
                &RegValue {
                    bytes: bytes.into(),
                    vtype: REG_BINARY,
                },
            )
            .map_err(|e| format!("禁用失败: {e}"))
    }
}

/// 扫描全部启动项(注册表 + 启动文件夹)
#[tauri::command]
pub fn get_startup_items() -> Vec<StartupItem> {
    list_startup()
}

/// 禁用/启用启动项
#[tauri::command]
pub fn set_startup_enabled(
    source: StartupSource,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    set_startup_enabled_impl(source, &name, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在 HKCU 测试键上验证 写禁用 → 状态读取 → 启用 的完整回路
    #[test]
    fn startup_disable_enable_roundtrip() {
        const RUN: &str = r"Software\LagScopeTest\Run";
        const APPROVED: &str = r"Software\LagScopeTest\Approved";
        let hk = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hk.create_subkey(RUN).expect("create run test key");
        run_key
            .set_value("LagScopeTestItem", &r"C:\Windows\System32\notepad.exe")
            .expect("set test value");

        // 模拟 scan:状态读取
        assert!(!is_disabled(HKEY_CURRENT_USER, APPROVED, "LagScopeTestItem"));

        // 禁用(直接写 approved)
        let (approved_key, _) = hk.create_subkey(APPROVED).expect("create approved");
        let bytes = vec![0x03u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0];
        approved_key
            .set_raw_value(
                "LagScopeTestItem",
                &RegValue {
                    bytes: bytes.into(),
                    vtype: REG_BINARY,
                },
            )
            .expect("set approved");
        assert!(is_disabled(HKEY_CURRENT_USER, APPROVED, "LagScopeTestItem"));

        // 启用 = 删除
        approved_key
            .delete_value("LagScopeTestItem")
            .expect("delete approved");
        assert!(!is_disabled(HKEY_CURRENT_USER, APPROVED, "LagScopeTestItem"));

        // 清理测试键
        hk.delete_subkey_all(r"Software\LagScopeTest").ok();
    }

    #[test]
    fn extract_target_parses_variants() {
        assert_eq!(
            extract_target(r#""C:\Program Files\App\app.exe" -min"#).as_deref(),
            Some(r"C:\Program Files\App\app.exe")
        );
        assert_eq!(
            extract_target(r"C:\Windows\System32\notepad.exe").as_deref(),
            Some(r"C:\Windows\System32\notepad.exe")
        );
        assert_eq!(extract_target(""), None);
    }
}
