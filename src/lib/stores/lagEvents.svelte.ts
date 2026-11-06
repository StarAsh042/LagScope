// 卡顿事件:加载历史 + 实时监听 + 立即诊断
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import type { LagEvent } from "$lib/types";

export const lagEvents = $state({
  list: [] as LagEvent[],
  selected: null as LagEvent | null,
  diagnosing: false,
  loadError: "",
});

let started = false;

export async function startLagEvents() {
  if (started) return;
  started = true;

  await listen<LagEvent>("lag:event", (e) => {
    const ev = e.payload;
    ev.id = ev.id ?? -1;
    // 实时事件置顶;选中态保持
    lagEvents.list.unshift(ev);
    if (lagEvents.list.length > 200) lagEvents.list.length = 200;
    if (!lagEvents.selected) lagEvents.selected = ev;
  });

  await refresh();
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
  manual: { label: "手动诊断", color: "#8b94a7" },
};

export function kindMeta(kind: string) {
  return KIND_META[kind] ?? { label: kind, color: "#8b94a7" };
}

export function fmtTs(tsMs: number): string {
  const d = new Date(tsMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
