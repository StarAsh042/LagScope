<script lang="ts">
  import { onMount } from "svelte";

  import Gauge from "$lib/components/Gauge.svelte";
  import ProcessTable from "$lib/components/ProcessTable.svelte";
  import RollingChart from "$lib/components/RollingChart.svelte";
  import {
    fmtBytes,
    fmtBytesPerSec,
    metrics,
    startMetrics,
  } from "$lib/stores/metrics.svelte";
  import type { MetricSample } from "$lib/types";

  onMount(() => {
    void startMetrics();
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
</script>

<main>
  <header>
    <h1>LagScope <span class="zh">卡顿透视</span></h1>
    <span class="status" class:on={metrics.connected}>
      {metrics.connected ? "● 采集中" : "○ 连接中…"}
    </span>
  </header>

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
  </section>

  <section class="charts">
    <RollingChart
      title="CPU(% / 每核)"
      data={cpuData}
      yMax={100}
      unit="%"
      series={[
        { label: "总", color: "#3b6cd4" },
        { label: "核", color: "#3b6cd466" },
      ]}
    />
    <RollingChart
      title="内存 / 提交内存(%)"
      data={memData}
      yMax={100}
      unit="%"
      series={[
        { label: "物理", color: "#3dd68c" },
        { label: "提交", color: "#f5a623" },
      ]}
    />
    <RollingChart
      title="磁盘(MB/s / 活跃%)"
      data={diskData}
      unit="MB/s"
      series={[
        { label: "读 MB/s", color: "#9a6cd4" },
        { label: "写 MB/s", color: "#d46cb4" },
        { label: "活跃 %", color: "#f5a623" },
      ]}
    />
    <RollingChart
      title="网络(KB/s)"
      data={netData}
      unit="KB/s"
      series={[
        { label: "下行", color: "#3dd68c" },
        { label: "上行", color: "#3b6cd4" },
      ]}
    />
    {#if metrics.latest?.gpuUtilPct != null || metrics.latest?.gpuMemMb != null}
      <RollingChart
        title="GPU(3D 引擎 % / 显存 MB)"
        data={gpuData}
        unit=""
        series={[
          { label: "3D %", color: "#f5a623" },
          { label: "显存 MB", color: "#8b94a7" },
        ]}
      />
    {/if}
    <RollingChart
      title="DPC / 硬缺页(次·页/秒)"
      data={chartData((s) => [s.dpcPerSec, s.pageInputPerSec])}
      unit="/s"
      series={[
        { label: "DPC/s", color: "#e5484d" },
        { label: "硬缺页/s", color: "#f5a623" },
      ]}
    />
  </section>

  <ProcessTable processes={metrics.processes} />

  <footer>
    <span>
      {metrics.latest?.temperatureC != null
        ? `温度 ${metrics.latest.temperatureC.toFixed(0)}℃  ·  `
        : ""}
      中断 {metrics.latest ? metrics.latest.interruptPct.toFixed(1) : "--"}%  ·
      ·  磁盘延迟
      {metrics.latest
        ? (metrics.latest.diskAvgSecTransfer * 1000).toFixed(1)
        : "--"}
      ms  ·  队列 {metrics.latest?.diskQueueLength.toFixed(2) ?? "--"}
    </span>
    <span class="io-note">进程 I/O 含网络流量口径(sysinfo)</span>
  </footer>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 12px 16px;
    gap: 12px;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
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
  .status {
    font-size: 12px;
    color: var(--text-dim);
  }
  .status.on {
    color: var(--ok);
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
  .io-note {
    opacity: 0.7;
  }
</style>
