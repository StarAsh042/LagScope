<script lang="ts">
  import { onMount } from "svelte";

  import { invoke } from "@tauri-apps/api/core";
  import {
    fmtBytesPerSec,
    metrics,
    startMetrics,
    stopMetrics,
  } from "$lib/stores/metrics.svelte";
  import { isDismissShortcut } from "$lib/shortcuts";
  import { levelColor } from "$lib/theme";

  /** Esc 收起悬浮窗(和其他浮层一致) */
  function onKeydown(e: KeyboardEvent) {
    if (isDismissShortcut(e)) void invoke("hide_mini");
  }

  onMount(() => {
    void startMetrics();
    return () => stopMetrics();
  });

  const rows = $derived.by(() => {
    const s = metrics.latest;
    const g = metrics.game;
    return [
      {
        key: "CPU",
        pct: s ? Math.min(100, s.cpuTotal) : 0,
        text: s ? `${s.cpuTotal.toFixed(0)}%` : "--",
      },
      {
        key: "内存",
        pct: s && s.memTotal ? (s.memUsed / s.memTotal) * 100 : 0,
        text: s && s.memTotal ? `${((s.memUsed / s.memTotal) * 100).toFixed(0)}%` : "--",
      },
      {
        key: "GPU",
        pct: s?.gpuUtilPct ?? 0,
        text: s?.gpuUtilPct != null ? `${s.gpuUtilPct.toFixed(0)}%` : "--",
      },
      {
        key: "磁盘",
        pct: Math.min(100, s?.diskActivePct ?? 0),
        text: s ? `${(s.diskReadBps / 1024 / 1024).toFixed(1)}/${(s.diskWriteBps / 1024 / 1024).toFixed(1)}M` : "--",
      },
      {
        key: "网络",
        pct: 0,
        text: s ? `↓${fmtBytesPerSec(s.netRxBps)}` : "--",
      },
      {
        key: "帧率",
        pct: g ? Math.min(100, g.fps) : 0,
        text: g ? `${g.fps} FPS` : "--",
      },
    ];
  });

  const barColor = (pct: number) =>
    levelColor(pct);
</script>

<svelte:window onkeydown={onKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="mini"
  data-tauri-drag-region
  ondblclick={() => {
    void invoke("show_main");
    void invoke("hide_mini");
  }}
>
  <button class="close" title="隐藏悬浮窗" onclick={() => invoke("hide_mini")}>
    ✕
  </button>
  {#each rows as r (r.key)}
    <div class="row">
      <span class="k">{r.key}</span>
      <span class="bar">
        <span
          class="fill"
          style="width: {r.pct}%; background: {barColor(r.pct)}"
        ></span>
      </span>
      <span class="v">{r.text}</span>
    </div>
  {/each}
</div>

<style>
  .mini {
    position: fixed;
    inset: 0;
    background: var(--bg-glass);
    border: 1px solid var(--border);
    border-radius: var(--radius-2xl);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    user-select: none;
    overflow: hidden;
  }
  .close {
    position: absolute;
    top: 2px;
    right: 4px;
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-size: var(--font-2xs);
    cursor: pointer;
    padding: 2px 5px;
  }
  .close:hover {
    color: var(--text);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-6);
  }
  .k {
    width: 30px;
    font-size: var(--font-2xs);
    color: var(--text-dim);
    flex: none;
  }
  .bar {
    flex: 1;
    height: 6px;
    background: var(--track-deep);
    border-radius: var(--radius-xs);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: var(--radius-xs);
    transition: width 0.4s ease;
  }
  .v {
    width: 74px;
    text-align: right;
    font-size: var(--font-2xs);
    color: var(--text);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    flex: none;
  }
</style>
