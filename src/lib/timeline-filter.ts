// 时间线搜索与筛选:纯函数,不依赖 Tauri 运行时,可单独验证
import type { LagEvent } from "$lib/types";

/** 时间范围档位 */
export type RangeKey = "all" | "1h" | "24h" | "7d";

export interface RangeOption {
  key: RangeKey;
  label: string;
  /** 回看时长(毫秒);0 表示不限 */
  spanMs: number;
}

export const RANGE_OPTIONS: readonly RangeOption[] = [
  { key: "all", label: "全部", spanMs: 0 },
  { key: "1h", label: "1 小时", spanMs: 3_600_000 },
  { key: "24h", label: "24 小时", spanMs: 86_400_000 },
  { key: "7d", label: "7 天", spanMs: 604_800_000 },
];

export interface TimelineFilter {
  /** 原始搜索串,空白分隔的多个词按 AND 组合 */
  query: string;
  /** 选中的原因类型;空数组表示不限 */
  kinds: readonly string[];
  range: RangeKey;
}

export const EMPTY_FILTER: TimelineFilter = { query: "", kinds: [], range: "all" };

/** 是否有任何筛选生效(用于显示"清除"按钮与结果计数) */
export function isFilterActive(f: TimelineFilter): boolean {
  return f.query.trim().length > 0 || f.kinds.length > 0 || f.range !== "all";
}

/** 搜索词切分:小写、按空白分组、丢弃空串 */
export function queryTerms(query: string): string[] {
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter((t) => t.length > 0);
}

/** 所有词都要出现在文本里(AND),大小写不敏感;空词表视为不过滤 */
export function matchesQuery(text: string, terms: readonly string[]): boolean {
  if (terms.length === 0) return true;
  const lower = text.toLowerCase();
  return terms.every((t) => lower.includes(t));
}

/** 时间范围:nowMs 之前 spanMs 内;未来时间戳(时钟回拨)一律保留 */
export function matchesRange(
  tsMs: number,
  range: RangeKey,
  nowMs: number,
): boolean {
  const opt = RANGE_OPTIONS.find((o) => o.key === range);
  if (!opt || opt.spanMs === 0) return true;
  return nowMs - tsMs <= opt.spanMs;
}

/** 事件的可搜索文本:时间、原因(中英文)、归因详情、Top 进程、挂起窗口、游戏与 DPC 驱动 */
export function eventSearchText(
  ev: LagEvent,
  kindLabel: (kind: string) => string,
  fmtTs: (tsMs: number) => string,
): string {
  const parts: string[] = [fmtTs(ev.tsMs)];
  for (const r of ev.reasons) {
    parts.push(r.kind, kindLabel(r.kind), r.detail);
  }
  for (const list of [ev.topCpu, ev.topMem, ev.topDisk]) {
    for (const p of list) {
      parts.push(p.name, String(p.pid));
    }
  }
  parts.push(...ev.hungWindows);
  if (ev.game) {
    parts.push(ev.game.app, String(ev.game.pid));
  }
  if (ev.dpcTop) {
    for (const [name] of ev.dpcTop.drivers) {
      parts.push(name);
    }
  }
  return parts.join(" ");
}

export interface FilterContext {
  nowMs: number;
  kindLabel: (kind: string) => string;
  fmtTs: (tsMs: number) => string;
}

/** 依次应用时间范围、原因类型、关键词三道筛选,保持原有顺序 */
export function filterEvents(
  events: readonly LagEvent[],
  filter: TimelineFilter,
  ctx: FilterContext,
): LagEvent[] {
  const terms = queryTerms(filter.query);
  const kinds = filter.kinds;
  const out: LagEvent[] = [];
  for (const ev of events) {
    if (!matchesRange(ev.tsMs, filter.range, ctx.nowMs)) continue;
    if (kinds.length > 0 && !ev.reasons.some((r) => kinds.includes(r.kind))) {
      continue;
    }
    if (terms.length > 0) {
      const text = eventSearchText(ev, ctx.kindLabel, ctx.fmtTs);
      if (!matchesQuery(text, terms)) continue;
    }
    out.push(ev);
  }
  return out;
}

export interface KindCount {
  kind: string;
  label: string;
  count: number;
}

/**
 * 各原因类型的可选条数。
 * 只应用时间范围与关键词,不应用类型筛选 —— 否则选中一个类型后其余类型的
 * 计数会全部归零,用户无法再切回来。
 */
export function kindCounts(
  events: readonly LagEvent[],
  filter: TimelineFilter,
  ctx: FilterContext,
): KindCount[] {
  const terms = queryTerms(filter.query);
  const seen = new Map<string, number>();
  for (const ev of events) {
    if (!matchesRange(ev.tsMs, filter.range, ctx.nowMs)) continue;
    if (terms.length > 0) {
      const text = eventSearchText(ev, ctx.kindLabel, ctx.fmtTs);
      if (!matchesQuery(text, terms)) continue;
    }
    const kinds = new Set(ev.reasons.map((r) => r.kind));
    for (const k of kinds) {
      seen.set(k, (seen.get(k) ?? 0) + 1);
    }
  }
  const list: KindCount[] = [];
  for (const [kind, count] of seen) {
    list.push({ kind, label: ctx.kindLabel(kind), count });
  }
  // 数量降序,同数量按类型名升序,保证渲染顺序稳定
  list.sort((a, b) => (b.count - a.count) || a.kind.localeCompare(b.kind));
  return list;
}

/** 切换某个原因类型的选中状态 */
export function toggleKind(
  kinds: readonly string[],
  kind: string,
): string[] {
  return kinds.includes(kind)
    ? kinds.filter((k) => k !== kind)
    : [...kinds, kind];
}
