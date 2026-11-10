<script lang="ts">
  import { onMount } from "svelte";

  import { invoke } from "@tauri-apps/api/core";

  import ErrorBoundary from "$lib/components/ErrorBoundary.svelte";
  import EventDetail from "$lib/components/EventDetail.svelte";
  import EventTimeline from "$lib/components/EventTimeline.svelte";
  import Gauge from "$lib/components/Gauge.svelte";
  import ProcessTable from "$lib/components/ProcessTable.svelte";
  import RollingChart from "$lib/components/RollingChart.svelte";
  import SettingsPanel from "$lib/components/SettingsPanel.svelte";
  import StartupPanel from "$lib/components/StartupPanel.svelte";
  import {
    fmtBytes,
    fmtBytesPerSec,
    metrics,
    startMetrics,
    stopMetrics,
  } from "$lib/stores/metrics.svelte";
  import {
    diagnoseNow,
    lagEvents,
    startLagEvents,
    stopLagEvents,
  } from "$lib/stores/lagEvents.svelte";
  import { isSettingsShortcut, settingsHint } from "$lib/shortcuts";
  import { levelColor, token } from "$lib/theme";
  import type { MetricSample } from "$lib/types";

  let tab = $state<"dashboard" | "events" | "startup" | "settings">("dashboard");

  /** 兜底界面里显示出错的是哪一块,避免用户以为整个程序挂了 */
  const tabLabel = $derived(
    tab === "dashboard"
      ? "仪表盘"
      : tab === "events"
        ? "卡顿事件"
        : tab === "startup"
          ? "启动项"
          : "设置"
  );

  // macOS 上主修饰键是 Cmd,提示文案跟着平台走
  const settingsTitle = $derived(
    settingsHint(
      typeof navigator !== "undefined" &&
        /Mac|iPhone|iPad/.test(navigator.platform)
    )
  );

  /** Ctrl+, / Cmd+, 直接跳到设置页 */
  function onKeydown(e: KeyboardEvent) {
    if (isSettingsShortcut(e)) {
      e.preventDefault();
      tab = "settings";
    }
  }

  onMount(() => {
    void startMetrics();
    void startLagEvents();

    // 主窗口与迷你窗是两个路由页面,来回切换会重新挂载本组件;
    // 不撤掉轮询,每切一次就多一套 2s/30s 定时器与事件监听。
    return () => {
      stopMetrics();
      stopLagEvents();
    };
  });

  // uPlot AlignedData 为列主序:[xs, ys1, ys2, ...]
  function toColumns(rows: number[][]): number[][] {
    const nCols = rows[0]?.length ?? 1;
    const out: number[][] = Array.from({ length: nCols }, () => []);
    for (const r of rows) {
      for (let c = 0; c < nCols; c++) out[c].push(r[c]);
    }
    return out;
  }

  // 图表数据派生:history → [xs, ys...] 列主序
  function chartData(pick: (s: MetricSample) => number[]): number[][] {
    return toColumns(metrics.history.map((s) => [s.tsMs / 1000, ...pick(s)]));
  }

  const cpuData = $derived(
    chartData((s) => [s.cpuTotal, ...s.cpuPerCore])
  );
  const memData = $derived(
    chartData((s) => {
      const usedPct = s.memTotal ? (s.memUsed / s.memTotal) * 100 : 0;
      const commitPct = s.memCommitLimit
        ? (s.memCommitUsed / s.memCommitLimit) * 100
        : 0;
      return [usedPct, commitPct];
    })
  );
  const diskData = $derived(
    chartData((s) => [
      s.diskReadBps / 1024 / 1024,
      s.diskWriteBps / 1024 / 1024,
      s.diskActivePct,
    ])
  );
  const netData = $derived(
    chartData((s) => [s.netRxBps / 1024, s.netTxBps / 1024])
  );
  const gpuData = $derived(
    chartData((s) => [s.gpuUtilPct ?? 0, s.gpuMemMb ?? 0])
  );

  const memSub = $derived(
    metrics.latest
      ? `${fmtBytes(metrics.latest.memUsed)} / ${fmtBytes(metrics.latest.memTotal)}`
      : "--"
  );
  const diskSub = $derived(
    metrics.latest ? `↓${fmtBytesPerSec(metrics.latest.diskReadBps)} ↑${fmtBytesPerSec(metrics.latest.diskWriteBps)}` : "--"
  );
  const netSub = $derived(
    metrics.latest ? `↓${fmtBytesPerSec(metrics.latest.netRxBps)} ↑${fmtBytesPerSec(metrics.latest.netTxBps)}` : "--"
  );
  const gpuSub = $derived(
    metrics.latest?.gpuMemMb != null ? `显存 ${metrics.latest.gpuMemMb.toFixed(0)} MB` : "--"
  );
  const cpuSub = $derived(
    metrics.latest && metrics.latest.cpuFreqMhz > 0
      ? `${metrics.latest.cpuFreqMhz.toFixed(0)} MHz`
      : ""
  );

  // 帧监测(PresentMon):渲染大户的每秒帧统计
  const gameSub = $derived(
    metrics.game
      ? `${metrics.game.app} · 卡帧 ${metrics.game.stutters}`
      : "无渲染活动"
  );
  const gameData = $derived(
    toColumns(
      metrics.gameHistory.map((g) => [g.tsMs / 1000, g.fps, g.maxGapMs])
    )
  );

  // 温度表盘:分级变色(≥75 红 / ≥60 黄 / 其余绿)
  const temp = $derived(metrics.latest?.temperatureC ?? null);
  const tempRaw = $derived(temp == null ? undefined : `${Math.round(temp)}°`);
  const tempColor = $derived(
    temp == null ? undefined : levelColor(temp, 75, 60)
  );

  // 底栏指标:先拼成数组再统一插分隔符。此前是手写 "·" 字面量,断行处多打了
  // 一个,显示成 "中断 0.0%  ·  ·  磁盘延迟"。
  const footerStats = $derived.by(() => {
    const s = metrics.latest;
    const items: string[] = [];
    if (s?.temperatureC != null) items.push(`温度 ${s.temperatureC.toFixed(0)}℃`);
    items.push(`中断 ${s ? s.interruptPct.toFixed(1) : "--"}%`);
    items.push(`磁盘延迟 ${s ? (s.diskAvgSecTransfer * 1000).toFixed(1) : "--"} ms`);
    items.push(`队列 ${s?.diskQueueLength.toFixed(2) ?? "--"}`);
    return items;
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <header>
    <div class="brand">
      <h1>LagScope <span class="zh">卡顿透视</span></h1>
      <nav>
        <button
          class:active={tab === "dashboard"}
          onclick={() => (tab = "dashboard")}
        >
          仪表盘
        </button>
        <button class:active={tab === "events"} onclick={() => (tab = "events")}>
          卡顿事件
          {#if lagEvents.list.length}
            <span class="badge">{lagEvents.list.length}</span>
          {/if}
        </button>
        <button
          class:active={tab === "startup"}
          onclick={() => (tab = "startup")}
        >
          启动项
        </button>
        <button
          class:active={tab === "settings"}
          onclick={() => (tab = "settings")}
          title={settingsTitle}
        >
          设置
        </button>
      </nav>
    </div>
    <div class="right">
      {#if tab === "events"}
        <button class="diag" disabled={lagEvents.diagnosing} onclick={diagnoseNow}>
          {lagEvents.diagnosing ? "诊断中…" : "立即诊断"}
        </button>
      {/if}
      <button class="mini-toggle" onclick={() => invoke("toggle_mini")}>
        悬浮窗
      </button>
      <span class="status" class:on={metrics.connected}>
        {metrics.connected ? "● 采集中" : "○ 连接中…"}
      </span>
    </div>
  </header>

  <!-- key:切 tab 时重建边界,否则某一页的异常会一直锁着后续页面 -->
  {#key tab}
  <ErrorBoundary label={tabLabel}>
  {#if tab === "dashboard"}
    <section class="gauges">
    <Gauge label="CPU" value={metrics.latest?.cpuTotal ?? null} sub={cpuSub} />
    <Gauge
      label="内存"
      value={metrics.latest && metrics.latest.memTotal
        ? (metrics.latest.memUsed / metrics.latest.memTotal) * 100
        : null}
      sub={memSub}
    />
    <Gauge
      label="磁盘"
      value={metrics.latest?.diskActivePct ?? null}
      sub={diskSub}
    />
    <Gauge
      label="GPU"
      value={metrics.latest?.gpuUtilPct ?? null}
      sub={gpuSub}
    />
    <Gauge
      label="提交内存"
      value={metrics.latest && metrics.latest.memCommitLimit
        ? (metrics.latest.memCommitUsed / metrics.latest.memCommitLimit) * 100
        : null}
      sub={metrics.latest
        ? `${fmtBytes(metrics.latest.memCommitUsed)} / ${fmtBytes(metrics.latest.memCommitLimit)}`
        : "--"}
    />
    <Gauge
      label="网络"
      value={null}
      sub={netSub}
    />
    <Gauge
      label="帧率"
      value={null}
      rawText={metrics.game ? String(metrics.game.fps) : "--"}
      sub={gameSub}
    />
    <Gauge
      label="温度"
      value={null}
      rawText={tempRaw}
      colorOverride={tempColor}
      sub={temp == null ? "热区不可用" : "最热 ACPI 热区"}
    />
  </section>

  <section class="charts">
    <RollingChart
      title="CPU(% / 每核)"
      data={cpuData}
      yMax={100}
      unit="%"
      series={[
        { label: "总", color: token("--chart-cpu", "#3b6cd4") },
        { label: "核", color: token("--chart-cpu-soft", "#3b6cd466") },
      ]}
    />
    <RollingChart
      title="内存 / 提交内存(%)"
      data={memData}
      yMax={100}
      unit="%"
      series={[
        { label: "物理", color: token("--chart-mem-used", "#3dd68c") },
        { label: "提交", color: token("--chart-mem-commit", "#f5a623") },
      ]}
    />
    <RollingChart
      title="磁盘(MB/s / 活跃%)"
      data={diskData}
      unit="MB/s"
      series={[
        { label: "读 MB/s", color: token("--chart-disk-read", "#9a6cd4") },
        { label: "写 MB/s", color: token("--chart-disk-write", "#d46cb4") },
        { label: "活跃 %", color: token("--chart-disk-active", "#f5a623") },
      ]}
    />
    <RollingChart
      title="网络(KB/s)"
      data={netData}
      unit="KB/s"
      series={[
        { label: "下行", color: token("--chart-net-rx", "#3dd68c") },
        { label: "上行", color: token("--chart-net-tx", "#3b6cd4") },
      ]}
    />
    {#if metrics.latest?.gpuUtilPct != null || metrics.latest?.gpuMemMb != null}
      <RollingChart
        title="GPU(3D 引擎 % / 显存 MB)"
        data={gpuData}
        unit=""
        series={[
          { label: "3D %", color: token("--chart-gpu-util", "#f5a623") },
          { label: "显存 MB", color: token("--chart-gpu-mem", "#8b94a7") },
        ]}
      />
    {/if}
    <RollingChart
      title="DPC / 硬缺页(次·页/秒)"
      data={chartData((s) => [s.dpcPerSec, s.pageInputPerSec])}
      unit="/s"
      series={[
        { label: "DPC/s", color: token("--chart-dpc", "#e5484d") },
        { label: "硬缺页/s", color: token("--chart-page-fault", "#f5a623") },
      ]}
    />
    {#if metrics.gameHistory.length}
      <RollingChart
        title="帧率 / 最大帧间隔(渲染大户)"
        data={gameData}
        unit="FPS"
        series={[
          { label: "FPS", color: token("--chart-fps", "#3dd68c") },
          { label: "最大间隔 ms", color: token("--chart-frame-gap", "#e5484d") },
        ]}
      />
    {/if}
  </section>

  <ProcessTable processes={metrics.processes} />

  <footer>
    <span class="stats">
      {#each footerStats as stat, i}
        {#if i > 0}<span class="sep">·</span>{/if}
        <span>{stat}</span>
      {/each}
    </span>
    <span class="io-note">进程 I/O 含网络流量口径(sysinfo)</span>
  </footer>
  {:else if tab === "events"}
    <div class="events-view">
      <EventTimeline />
      <EventDetail />
    </div>
  {:else if tab === "startup"}
    <StartupPanel />
  {:else}
    <SettingsPanel />
  {/if}
  </ErrorBoundary>
  {/key}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 12px 16px;
    gap: 12px;
    /* 内容超高时可纵向滚动,避免底部被裁切 */
    overflow-y: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  h1 {
    font-size: 18px;
    margin: 0;
    letter-spacing: 0.5px;
  }
  h1 .zh {
    font-size: 14px;
    color: var(--text-dim);
    font-weight: 400;
    margin-left: 6px;
  }
  nav {
    display: flex;
    gap: 6px;
  }
  nav button {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-dim);
    border-radius: 8px;
    padding: 5px 14px;
    font-size: 13px;
    cursor: pointer;
  }
  nav button:hover {
    color: var(--text);
    background: var(--panel-alt);
  }
  nav button.active {
    color: var(--text);
    background: var(--panel-alt);
    border-color: var(--border);
  }
  .badge {
    display: inline-block;
    margin-left: 6px;
    background: var(--err);
    color: var(--on-accent);
    font-size: 10px;
    border-radius: 8px;
    padding: 1px 6px;
    vertical-align: middle;
  }
  .right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .diag {
    background: var(--accent);
    border: none;
    color: var(--on-accent);
    border-radius: 8px;
    padding: 6px 16px;
    font-size: 13px;
    cursor: pointer;
  }
  .diag:hover {
    background: var(--accent-hover);
  }
  .diag:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .mini-toggle {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-dim);
    border-radius: 8px;
    padding: 5px 12px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .mini-toggle:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .status {
    font-size: 12px;
    color: var(--text-dim);
  }
  .status.on {
    color: var(--ok);
  }
  .events-view {
    display: flex;
    gap: 12px;
    /* 滚动容器内保证最小高度,内部各自滚动 */
    flex: 1 0 480px;
    min-height: 0;
  }
  .gauges {
    display: flex;
    gap: 8px;
    justify-content: space-around;
    flex-wrap: wrap;
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 10px;
  }
  footer {
    display: flex;
    justify-content: space-between;
    font-size: 11.5px;
    color: var(--text-dim);
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px;
  }
  .stats .sep {
    opacity: 0.45;
  }
  .io-note {
    opacity: 0.7;
    white-space: nowrap;
    margin-left: 12px;
  }
</style>
