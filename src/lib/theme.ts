/**
 * 主题令牌的 JS 入口。
 *
 * 颜色只在 src/lib/app.css 的 :root 定义一次。CSS 里直接用 var(--x),
 * 但 canvas(uPlot 的轴、网格、曲线)与 SVG 属性拿不到 var(),只能取解析后的字符串,
 * 所以经由这里读同一份值,避免在 JS 里复制一遍十六进制。
 */

const cache = new Map<string, string>();

/** 读 :root 上的令牌;读不到(SSR、样式未加载)时用 fallback 兜底 */
export function token(name: string, fallback: string): string {
  const hit = cache.get(name);
  if (hit != null) return hit;
  if (typeof document === "undefined") return fallback;
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  // 只在真的取到值时入缓存:样式表还没挂上时返回兜底值,不能把它固化下来
  if (!value) return fallback;
  cache.set(name, value);
  return value;
}

/** 占用率分级:>= errAt 红,>= warnAt 黄,其余绿。阈值可由调用方覆盖(温度用 75/60) */
export function levelColor(pct: number, errAt = 85, warnAt = 60): string {
  if (pct >= errAt) return token("--err", "#e5484d");
  if (pct >= warnAt) return token("--warn", "#f5a623");
  return token("--ok", "#3dd68c");
}
