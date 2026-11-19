// 快捷键判定:纯函数,不碰 DOM 也不碰 Tauri API,可以直接用 node 跑测试。

/** 只取判定要用到的字段,KeyboardEvent 天然满足 */
export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/**
 * 设置页快捷键:Ctrl + ","(macOS 上 Cmd + ",")。
 *
 * 带 Alt / Shift 的变体不算:Shift+Ctrl+, 在部分布局里是别的字符,Alt+ 则由系统
 * 与输入法占用,放进来只会抢别人的按键。
 */
export function isSettingsShortcut(e: KeyLike): boolean {
  if (!(e.ctrlKey || e.metaKey)) return false;
  if (e.altKey || e.shiftKey) return false;
  return e.key === ",";
}

/** 关闭浮层的快捷键:Esc */
export function isDismissShortcut(e: KeyLike): boolean {
  return e.key === "Escape";
}

/** 设置按钮的 title,和实际按键保持一致(macOS 显示 Cmd) */
export function settingsHint(isMac: boolean): string {
  return isMac ? "设置(Cmd+,)" : "设置(Ctrl+,)";
}
