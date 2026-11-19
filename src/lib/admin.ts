// 管理员权限的共用入口。
//
// DPC 内核跟踪(ETW)与系统级(HKLM)启动项都要管理员权限,而普通权限下
// 这两处只会静默失败 —— 用户看到的是「开了但没生效」。与其让人自己去找
// exe 右键,不如在这里提供同一个「以管理员身份重启」入口。

import { invoke } from "@tauri-apps/api/core";

/** 与 src-tauri/src/admin.rs 的 RelaunchOutcome 对应 */
export type RelaunchOutcome = "started" | "cancelled";

/** 提权后哪些功能才完整可用 */
export const ADMIN_ELEVATED_NOTE = "DPC 驱动归因与系统级(HKLM)启动项均可正常使用。";

/** 普通权限下缺了什么 */
export const ADMIN_LIMITED_NOTE =
  "DPC 驱动归因与系统级(HKLM)启动项需要管理员权限,当前为普通权限,这两项只能降级运行。";

/** 怎么提权 */
export const ADMIN_HINT =
  "点「以管理员身份重启」会弹 UAC 确认;也可以关掉本程序后右键 exe 选「以管理员身份运行」。";

/**
 * 以管理员身份重启自身,返回是否真的拉起了新进程。
 *
 * 成功时 Rust 侧先启动提权的新进程、再退出自己,所以这里一般收不到回包;
 * 能正常返回 false 的只有用户在 UAC 弹窗上点了「否」。
 */
export async function relaunchAsAdmin(): Promise<boolean> {
  const outcome = await invoke<RelaunchOutcome>("relaunch_as_admin");
  return outcome === "started";
}
