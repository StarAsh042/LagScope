// 卡顿事件:加载历史 + 实时监听 + 立即诊断
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import type { LagEvent } from "$lib/types";

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

/** 原因类型的展示名与配色 */
export const KIND_META: Record<string, { label: string; color: string }> = {
  cpu: { label: "CPU 满载", color: "#e5484d" },
  mem_pressure: { label: "内存压力", color: "#f5a623" },
  page_faults: { label: "硬缺页", color: "#e28c3a" },
  disk_latency: { label: "磁盘延迟", color: "#d46cb4" },
  interrupt: { label: "中断高载", color: "#9a6cd4" },
  hung_window: { label: "窗口假死", color: "#3b6cd4" },
  frame_stutter: { label: "游戏卡帧", color: "#3dd68c" },
  dpc_driver: { label: "DPC 驱动风暴", color: "#e28c3a" },
  manual: { label: "手动诊断", color: "#8b94a7" },
};

export function kindMeta(kind: string) {
  return KIND_META[kind] ?? { label: kind, color: "#8b94a7" };
}

export function fmtTs(tsMs: number): string {
  const d = new Date(tsMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return (
    d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()) + " " +
    p(d.getHours()) + ":" + p(d.getMinutes()) + ":" + p(d.getSeconds())
  );
}
