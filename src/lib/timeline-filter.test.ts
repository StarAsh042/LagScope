import { describe, expect, it } from "vitest";
import type { LagEvent, MetricSample } from "$lib/types";
import {
  EMPTY_FILTER,
  eventSearchText,
  filterEvents,
  isFilterActive,
  kindCounts,
  matchesQuery,
  matchesRange,
  queryTerms,
  toggleKind,
} from "$lib/timeline-filter";

const NOW = 1_000_000_000;

function counters(tsMs: number): MetricSample {
  return {
    tsMs, cpuTotal: 0, cpuPerCore: [], cpuFreqMhz: 0, memUsed: 0, memTotal: 0,
    memCommitUsed: 0, memCommitLimit: 0, pageInputPerSec: 0, diskReadBps: 0,
    diskWriteBps: 0, diskActivePct: 0, diskAvgSecTransfer: 0, diskQueueLength: 0,
    diskTotal: 0, diskFree: 0, netRxBps: 0, netTxBps: 0, gpuUtilPct: null,
    gpuMemMb: null, dpcPerSec: 0, interruptPct: 0, temperatureC: null,
  };
}

function ev(id: number, tsMs: number, kinds: string[], extra: Partial<LagEvent> = {}): LagEvent {
  return {
    id, tsMs,
    reasons: kinds.map((kind) => ({ kind, detail: "", value: 1 })),
    counters: counters(tsMs),
    topCpu: [], topMem: [], topDisk: [], hungWindows: [], game: null, dpcTop: null,
    ...extra,
  };
}

const ctx = {
  nowMs: NOW,
  kindLabel: (k: string) => `标签-${k}`,
  fmtTs: (t: number) => `T${t}`,
};

describe("queryTerms / matchesQuery", () => {
  it("切词小写、按空白分组、丢弃空串", () => {
    expect(queryTerms("  Hello  WORLD\tfoo ")).toEqual(["hello", "world", "foo"]);
    expect(queryTerms("   ")).toEqual([]);
  });

  it("多词按 AND 匹配且不区分大小写", () => {
    expect(matchesQuery("Hello World", ["hello", "world"])).toBe(true);
    expect(matchesQuery("Hello", ["hello", "world"])).toBe(false);
    expect(matchesQuery("anything", [])).toBe(true);
  });
});

describe("matchesRange", () => {
  it("all 恒真，未来时间戳保留", () => {
    expect(matchesRange(0, "all", NOW)).toBe(true);
    expect(matchesRange(NOW + 5000, "1h", NOW)).toBe(true);
  });

  it("按回看窗口判定边界", () => {
    expect(matchesRange(NOW - 3_600_000, "1h", NOW)).toBe(true);
    expect(matchesRange(NOW - 3_600_001, "1h", NOW)).toBe(false);
    expect(matchesRange(NOW - 86_400_000, "24h", NOW)).toBe(true);
  });
});

describe("isFilterActive / toggleKind", () => {
  it("空筛选不生效", () => {
    expect(isFilterActive(EMPTY_FILTER)).toBe(false);
    expect(isFilterActive({ query: "  ", kinds: [], range: "all" })).toBe(false);
    expect(isFilterActive({ query: "", kinds: ["cpu"], range: "all" })).toBe(true);
    expect(isFilterActive({ query: "", kinds: [], range: "1h" })).toBe(true);
  });

  it("切换类型选中态且返回新数组", () => {
    const original = ["cpu", "disk"];
    expect(toggleKind(original, "mem_pressure")).toEqual(["cpu", "disk", "mem_pressure"]);
    expect(toggleKind(original, "cpu")).toEqual(["disk"]);
    expect(original).toEqual(["cpu", "disk"]);
  });
});

describe("eventSearchText", () => {
  it("覆盖时间、原因标签、详情、进程、窗口、渲染进程与驱动", () => {
    const text = eventSearchText(
      ev(1, 123, ["cpu"], {
        reasons: [{ kind: "cpu", detail: "满载", value: 100 }],
        topCpu: [{ pid: 42, name: "chrome", value: 90 }],
        hungWindows: ["资源管理器"],
        game: { tsMs: 123, app: "game.exe", pid: 7, fps: 60, avgGapMs: 2, maxGapMs: 5, stutters: 0 },
        dpcTop: { tsMs: 123, total: 10, drivers: [["nvlddmkm.sys", 8]] },
      }),
      ctx.kindLabel,
      ctx.fmtTs,
    );
    for (const needle of ["T123", "cpu", "标签-cpu", "满载", "chrome", "42", "资源管理器", "game.exe", "7", "nvlddmkm.sys"]) {
      expect(text).toContain(needle);
    }
  });
});

describe("filterEvents", () => {
  const events = [
    ev(1, NOW - 10_000, ["cpu"], { topCpu: [{ pid: 1, name: "alpha", value: 1 }] }),
    ev(2, NOW - 20_000, ["disk_latency"]),
    ev(3, NOW - 10 * 3_600_000, ["cpu"], { topCpu: [{ pid: 3, name: "beta", value: 1 }] }),
  ];

  it("空筛选原样返回且保持顺序", () => {
    expect(filterEvents(events, EMPTY_FILTER, ctx).map((e) => e.id)).toEqual([1, 2, 3]);
  });

  it("依次应用范围、类型与关键词三道筛选", () => {
    const f = { query: "alpha", kinds: ["cpu"], range: "1h" as const };
    expect(filterEvents(events, f, ctx).map((e) => e.id)).toEqual([1]);
    expect(filterEvents(events, { ...f, query: "beta" }, ctx)).toEqual([]);
  });
});

describe("kindCounts", () => {
  it("不受类型筛选影响，计数按降序/名称升序稳定排序", () => {
    const events = [
      ev(1, NOW, ["cpu", "disk_latency"]),
      ev(2, NOW, ["cpu"]),
      ev(3, NOW, ["page_faults"]),
    ];
    const counts = kindCounts(events, { query: "", kinds: ["cpu"], range: "all" }, ctx);
    expect(counts).toEqual([
      { kind: "cpu", label: "标签-cpu", count: 2 },
      { kind: "disk_latency", label: "标签-disk_latency", count: 1 },
      { kind: "page_faults", label: "标签-page_faults", count: 1 },
    ]);
  });
});
