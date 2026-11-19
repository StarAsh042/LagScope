import { beforeEach, describe, expect, it, vi } from "vitest";

// vi.mock 会被提升到文件顶部，工厂里只能引用 vi.hoisted 先建立的变量。
const h = vi.hoisted(() => {
  const listeners = new Map<string, (e: { payload: any }) => void>();
  const invokeMock = vi.fn(async (cmd: string) => {
    if (cmd === "get_latest_sample") return null;
    if (cmd === "get_processes" || cmd === "get_disks") return [];
    if (cmd === "get_io_breakdown") return { volumes: [], nics: [] };
    return null;
  });
  return { listeners, invokeMock };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: any }) => void) => {
    h.listeners.set(event, cb);
    return () => { h.listeners.delete(event); };
  }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: h.invokeMock }));
vi.mock("$lib/toast.svelte", () => ({ notify: vi.fn() }));

import {
  HISTORY_LEN,
  fmtBytes,
  fmtBytesPerSec,
  fmtPct,
  metrics,
  startMetrics,
  stopMetrics,
} from "$lib/stores/metrics.svelte";

function sample(tsMs: number) {
  return { tsMs } as any;
}

describe("metrics 格式化工具", () => {
  it("fmtBytesPerSec 按量级选单位", () => {
    expect(fmtBytesPerSec(512)).toBe("512 B/s");
    expect(fmtBytesPerSec(2048)).toBe("2.0 KB/s");
    expect(fmtBytesPerSec(3 * 1024 * 1024)).toBe("3.0 MB/s");
    expect(fmtBytesPerSec(2 * 1024 ** 3)).toBe("2.00 GB/s");
  });

  it("fmtBytes 按量级选单位", () => {
    expect(fmtBytes(512)).toBe("1 KB"); // Math.round(512/1024)=1
    expect(fmtBytes(2 * 1024 ** 2)).toBe("2 MB");
    expect(fmtBytes(3 * 1024 ** 3)).toBe("3.0 GB");
    expect(fmtBytes(2 * 1024 ** 4)).toBe("2.00 TB");
  });

  it("fmtPct 处理空值与精度", () => {
    expect(fmtPct(null)).toBe("--");
    expect(fmtPct(undefined)).toBe("--");
    expect(fmtPct(12.34)).toBe("12.3%");
    expect(fmtPct(12.34, 2)).toBe("12.34%");
  });
});

describe("metrics 状态机", () => {
  beforeEach(async () => {
    stopMetrics();
    h.listeners.clear();
    metrics.history.length = 0;
    metrics.gameHistory.length = 0;
    metrics.latest = null;
    metrics.game = null;
    metrics.connected = false;
    vi.clearAllMocks();
    await startMetrics();
  });

  it("startMetrics 注册两类监听并预热轮询", () => {
    expect(h.listeners.has("metrics:sample")).toBe(true);
    expect(h.listeners.has("game:sample")).toBe(true);
    expect(h.invokeMock).toHaveBeenCalledWith("get_latest_sample");
    expect(h.invokeMock).toHaveBeenCalledWith("get_processes");
    expect(h.invokeMock).toHaveBeenCalledWith("get_io_breakdown");
  });

  it("样本推入历史并保持窗口上限", () => {
    const push = h.listeners.get("metrics:sample")!;
    for (let i = 0; i < HISTORY_LEN + 25; i++) push({ payload: sample(i) });
    expect(metrics.history.length).toBe(HISTORY_LEN);
    expect(metrics.history[0].tsMs).toBe(25);
    expect(metrics.history.at(-1)!.tsMs).toBe(HISTORY_LEN + 24);
    expect(metrics.connected).toBe(true);
  });

  it("游戏样本推入独立历史且窗口上限一致", () => {
    const push = h.listeners.get("game:sample")!;
    for (let i = 0; i < HISTORY_LEN + 5; i++) {
      push({ payload: { tsMs: i, app: "a", pid: 1, fps: 60, avgGapMs: 1, maxGapMs: 2, stutters: 0 } });
    }
    expect(metrics.gameHistory.length).toBe(HISTORY_LEN);
    expect(metrics.gameHistory[0].tsMs).toBe(5);
  });

  it("stopMetrics 撤掉监听并允许重新启动", async () => {
    h.listeners.get("metrics:sample")!({ payload: sample(1) });
    expect(metrics.history.length).toBeGreaterThan(0);
    stopMetrics();
    expect(h.listeners.size).toBe(0);
    await startMetrics();
    expect(h.listeners.has("metrics:sample")).toBe(true);
  });
});
