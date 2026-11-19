// 全局提示队列:跨路由共享,主窗口的任何一条命令失败都报到同一处。
import {
  appendToast,
  findDuplicate,
  removeToast,
  TOAST_TTL_MS,
  type Toast,
  type ToastLevel,
} from "$lib/toast";

export const toasts = $state({ list: [] as Toast[] });

let seq = 0;
/** 每条提示的自动消失定时器;手动关闭或续命时一并撤掉 */
const timers = new Map<number, ReturnType<typeof setTimeout>>();

function clearTimer(id: number) {
  const timer = timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    timers.delete(id);
  }
}

function schedule(id: number, level: ToastLevel) {
  clearTimer(id);
  timers.set(
    id,
    setTimeout(() => dismiss(id), TOAST_TTL_MS[level]),
  );
}

/**
 * 弹一条提示,返回它的 id。
 *
 * 同一句话重复上报(例如 2s 一次、持续失败的轮询)不会堆成瀑布:
 * 复用已存在的那条并续命 —— 既保留用户正在看的提示,也不会因重新挂载而闪烁。
 */
export function notify(message: unknown, level: ToastLevel = "error"): number {
  const text = String(message);
  const dup = findDuplicate(toasts.list, { message: text, level });
  if (dup) {
    schedule(dup.id, level);
    return dup.id;
  }

  const id = ++seq;
  toasts.list = appendToast(toasts.list, { id, level, message: text });
  schedule(id, level);
  return id;
}

export function dismiss(id: number) {
  clearTimer(id);
  toasts.list = removeToast(toasts.list, id);
}
