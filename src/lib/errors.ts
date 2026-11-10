// 错误对象的展示文本。
//
// Tauri 命令失败一般 reject 一个字符串,而渲染期异常是 Error 实例;
// 两者都要能塞进一行提示里,且不能让整段堆栈盖满屏幕。

/** 兜底文案的最大长度 */
export const MAX_ERROR_TEXT = 240;

export function errorText(err: unknown, max = MAX_ERROR_TEXT): string {
  const raw = err instanceof Error ? err.message || err.name : String(err ?? "");
  const text = raw.trim() || "未知错误";
  return text.length > max ? text.slice(0, max) + "…" : text;
}
