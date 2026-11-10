// 指标中枢:订阅 Rust 1Hz 事件 → 滚动历史;进程表 2s 轮询。
// Svelte 5 runes 模块(.svelte.ts)。

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import type { DiskInfo, GameSample, MetricSample, ProcessInfo } from "$lib/types";

/** 历史窗口:120 秒(1Hz) */
export const HISTORY_LEN = 120;

export const metrics = $state({
  latest: null as MetricSample | null,
  history: [] as MetricSample[],
  processes: [] as ProcessInfo[],
  disks: [] as DiskInfo[],
  game: null as GameSample | null,
  gameHistory: [] as GameSample[],
  connected: false,
});

let started = false;
/** 本模块创建的定时器;stopMetrics() 负责全部撤掉 */
const timers: ReturnType<typeof setInterval>[] = [];
/** Tauri 事件监听的注销函数,同样交给 stopMetrics() */
const unlisteners: (() => void)[] = [];

export async function startMetrics() {
  if (started) return;
  started = true;

  // 订阅采集线程的 1Hz 推送
  unlisteners.push(await listen<MetricSample>("metrics:sample", (e) => {
    metrics.latest = e.payload;
    metrics.connected = true;
    metrics.history.push(e.payload);
    if (metrics.history.length > HISTORY_LEN) {
      metrics.history.splice(0, metrics.history.length - HISTORY_LEN);
    }
  }));

  // 帧监测(PresentMon):有渲染活动时 1Hz
  unlisteners.push(await listen<GameSample>("game:sample", (e) => {
    metrics.game = e.payload;
    metrics.gameHistory.push(e.payload);
    if (metrics.gameHistory.length > HISTORY_LEN) {
      metrics.gameHistory.splice(0, metrics.gameHistory.length - HISTORY_LEN);
    }
  }));

  // 陈旧数据守卫:数据流断掉(如监测器崩溃)后清空显示,避免幽灵数值
  timers.push(
    setInterval(() => {
      const now = Date.now();
      if (metrics.game && now - metrics.game.tsMs > 5000) {
        metrics.game = null;
      }
    }, 3000)
  );

  // 启动时先补一次最新值(避免图表空白 1 秒)
  const seed = await invoke<MetricSample | null>("get_latest_sample");
  if (seed && metrics.history.length === 0) {
    metrics.latest = seed;
    metrics.history.push(seed);
  }

  // 进程表轮询(2s)
  const pollProcesses = async () => {
    try {
      metrics.processes = await invoke<ProcessInfo[]>("get_processes");
    } catch (err) {
      console.error("get_processes 失败", err);
    }
  };
  await pollProcesses();
  timers.push(setInterval(pollProcesses, 2000));

  // 磁盘容量(30s 足够)
  const pollDisks = async () => {
    try {
      metrics.disks = await invoke<DiskInfo[]>("get_disks");
    } catch (err) {
      console.error("get_disks 失败", err);
    }
  };
  await pollDisks();
  timers.push(setInterval(pollDisks, 30000));
}

/**
 * 撤掉全部轮询定时器并允许重新 start。
 *
 * 事件监听(`listen`)由 Tauri 的 unlisten 管理,这里只管定时器;
 * 前端路由切换时(仪表盘 ⇄ 迷你窗)必须调用,否则每进一次多一套轮询。
 */
export function stopMetrics() {
  while (timers.length) {
    clearInterval(timers.pop()!);
  }
  while (unlisteners.length) {
    unlisteners.pop()!();
  }
  started = false;
}

// ---- 格式化工具 ----

export function fmtBytesPerSec(bps: number): string {
  if (bps >= 1024 * 1024 * 1024) return (bps / 1024 / 1024 / 1024).toFixed(2) + " GB/s";
  if (bps >= 1024 * 1024) return (bps / 1024 / 1024).toFixed(1) + " MB/s";
  if (bps >= 1024) return (bps / 1024).toFixed(1) + " KB/s";
  return Math.round(bps) + " B/s";
}

export function fmtBytes(bytes: number): string {
  if (bytes >= 1024 ** 4) return (bytes / 1024 ** 4).toFixed(2) + " TB";
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(0) + " MB";
  return Math.round(bytes / 1024) + " KB";
}

export function fmtPct(v: number | null | undefined, digits = 1): string {
  return v == null ? "--" : v.toFixed(digits) + "%";
}
