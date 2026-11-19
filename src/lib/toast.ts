// 全局提示的纯逻辑:队列增删与去重规则,不依赖 Svelte 与 Tauri,便于单测。

export type ToastLevel = "info" | "warn" | "error";

export interface Toast {
  id: number;
  level: ToastLevel;
  message: string;
}

/** 同屏最多几条:再多会盖住界面,而被挤掉的旧提示早已过时 */
export const MAX_TOASTS = 3;

/** 各等级停留时长:错误留足阅读时间,信息只作确认 */
export const TOAST_TTL_MS: Record<ToastLevel, number> = {
  info: 2500,
  warn: 5000,
  error: 8000,
};

/**
 * 追加一条提示,超出上限时丢掉最旧的。
 *
 * 这里不做去重:去重要求保留原条目(否则每次轮询失败都重新挂一条、界面闪一下),
 * 由调用方先用 findDuplicate 判断。
 */
export function appendToast(
  list: readonly Toast[],
  next: Toast,
  max = MAX_TOASTS,
): Toast[] {
  const out = [...list, next];
  return out.length > max ? out.slice(out.length - max) : out;
}

/** 找出与 next 同文案、同等级的现有条目 */
export function findDuplicate(
  list: readonly Toast[],
  next: Pick<Toast, "message" | "level">,
): Toast | undefined {
  return list.find((t) => t.level === next.level && t.message === next.message);
}

export function removeToast(list: readonly Toast[], id: number): Toast[] {
  return list.filter((t) => t.id !== id);
}
