// 卡顿事件:加载历史 + 实时监听 + 立即诊断
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { token } from "$lib/theme";
import type { LagEvent } from "$lib/types";
import {
  RANGE_OPTIONS,
  type RangeKey,
  toggleKind,
} from "$lib/timeline-filter";

/** 导出落盘结果 */
export interface ExportResult {
  path: string;
  count: number;
  format: string;
}

export const lagEvents = $state({
  list: [] as LagEvent[],
  selected: null as LagEvent | null,
  diagnosing: false,
  loadError: "",
  exporting: false,
  exportNote: "",
  exportError: "",
  lastExport: "",
  /** 正在删除的事件 id,用于禁用按钮 */
  deleting: null as number | null,
});

/** 时间线筛选条件;跟着 tab 切换保留,所以放在 store 而不是组件里 */
export const timeline = $state({
  query: "",
  kinds: [] as string[],
  range: "all" as RangeKey,
});

/** 时间范围的中文标签,供组件渲染当前选择 */
export function rangeLabel(range: RangeKey): string {
  return RANGE_OPTIONS.find((o) => o.key === range)?.label ?? "全部";
}

export function toggleKindFilter(kind: string) {
  timeline.kinds = toggleKind(timeline.kinds, kind);
}

export function clearFilter() {
  timeline.query = "";
  timeline.kinds = [];
  timeline.range = "all";
}

let started = false;
/** Tauri 事件监听的注销函数 */
const unlisteners: (() => void)[] = [];

export async function startLagEvents() {
  if (started) return;
  started = true;

  unlisteners.push(await listen<LagEvent>("lag:event", (e) => {
    const ev = e.payload;
    ev.id = ev.id ?? -1;
    // 实时事件置顶;选中态保持
    lagEvents.list.unshift(ev);
    if (lagEvents.list.length > 200) lagEvents.list.length = 200;
    if (!lagEvents.selected) lagEvents.selected = ev;
  }));

  await refresh();
}

/** 撤掉事件监听,允许重新 start(路由切换时调用) */
export function stopLagEvents() {
  while (unlisteners.length) {
    unlisteners.pop()!();
  }
  started = false;
}

export async function refresh() {
  try {
    const list = await invoke<LagEvent[]>("list_lag_events", { limit: 100 });
    lagEvents.list = list;
    lagEvents.loadError = "";
    if (!lagEvents.selected && list.length) lagEvents.selected = list[0];
  } catch (err) {
    lagEvents.loadError = String(err);
  }
}

export async function diagnoseNow() {
  lagEvents.diagnosing = true;
  try {
    const ev = await invoke<LagEvent>("diagnose_now");
    ev.id = ev.id ?? -1;
    lagEvents.list.unshift(ev);
    lagEvents.selected = ev;
  } catch (err) {
    lagEvents.loadError = String(err);
  } finally {
    lagEvents.diagnosing = false;
  }
}

/** 导出最近事件为 CSV / JSON,成功后可选直接定位到文件 */
export async function exportEvents(format: "csv" | "json", reveal = true) {
  lagEvents.exporting = true;
  lagEvents.exportNote = "";
  lagEvents.exportError = "";
  try {
    const r = await invoke<ExportResult>("export_lag_events", { format });
    lagEvents.lastExport = r.path;
    lagEvents.exportNote = "已导出 " + r.count + " 条事件到 " + r.path;
    if (reveal) await revealItemInDir(r.path);
  } catch (err) {
    lagEvents.exportError = String(err);
  } finally {
    lagEvents.exporting = false;
  }
}

/** 删除一条已入库的事件;未入库的实时事件(id <= 0)不处理 */
export async function deleteEvent(ev: LagEvent): Promise<boolean> {
  const id = ev.id;
  if (id == null || id <= 0) {
    lagEvents.loadError = "这条事件还没有入库,无法删除";
    return false;
  }
  lagEvents.deleting = id;
  lagEvents.loadError = "";
  try {
    await invoke<number>("delete_lag_events", { ids: [id] });
    lagEvents.list = lagEvents.list.filter((x) => x.id !== id);
    if (lagEvents.selected?.id === id) {
      lagEvents.selected = lagEvents.list[0] ?? null;
    }
    return true;
  } catch (err) {
    lagEvents.loadError = String(err);
    return false;
  } finally {
    lagEvents.deleting = null;
  }
}

/** 在资源管理器中定位上一次导出的文件 */
export async function revealLastExport() {
  lagEvents.exportError = "";
  if (!lagEvents.lastExport) return;
  try {
    await revealItemInDir(lagEvents.lastExport);
  } catch (err) {
    lagEvents.exportError = String(err);
  }
}

export function select(ev: LagEvent) {
  lagEvents.selected = ev;
}

/** 原因类型的展示名与配色:颜色令牌定义在 app.css 的 :root,这里只存令牌名与兜底值 */
export const KIND_META: Record<string, { label: string; token: string; fallback: string }> = {
  cpu: { label: "CPU 满载", token: "--kind-cpu", fallback: "#e5484d" },
  mem_pressure: { label: "内存压力", token: "--kind-mem", fallback: "#f5a623" },
  page_faults: { label: "硬缺页", token: "--kind-page-fault", fallback: "#e28c3a" },
  disk_latency: { label: "磁盘延迟", token: "--kind-disk", fallback: "#d46cb4" },
  interrupt: { label: "中断高载", token: "--kind-interrupt", fallback: "#9a6cd4" },
  hung_window: { label: "窗口假死", token: "--kind-hung", fallback: "#3b6cd4" },
  frame_stutter: { label: "游戏卡帧", token: "--kind-frame", fallback: "#3dd68c" },
  dpc_driver: { label: "DPC 驱动风暴", token: "--kind-dpc", fallback: "#e28c3a" },
  manual: { label: "手动诊断", token: "--kind-manual", fallback: "#8b94a7" },
};

/** 令牌在调用时才解析:模块初始化阶段样式表可能还没进 DOM */
export function kindMeta(kind: string): { label: string; color: string } {
  const meta = KIND_META[kind];
  if (!meta) return { label: kind, color: token("--kind-manual", "#8b94a7") };
  return { label: meta.label, color: token(meta.token, meta.fallback) };
}

export function fmtTs(tsMs: number): string {
  const d = new Date(tsMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return (
    d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()) + " " +
    p(d.getHours()) + ":" + p(d.getMinutes()) + ":" + p(d.getSeconds())
  );
}
