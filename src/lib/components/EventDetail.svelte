<script lang="ts">
  import { fmtTs, kindMeta, lagEvents } from "$lib/stores/lagEvents.svelte";
  import { fmtBytes, fmtBytesPerSec } from "$lib/stores/metrics.svelte";
  import type { LagEvent, TopProc } from "$lib/types";

  const ev = $derived(lagEvents.selected);

  function topList(list: TopProc[], unit: (v: number) => string) {
    return list.map(
      (t) => `${t.name} (${t.pid}) — ${unit(t.value)}`
    );
  }
</script>

{#if ev}
  <div class="detail">
    <div class="head">
      <span class="title">事件详情 {ev.id != null && ev.id > 0 ? `#${ev.id}` : ""}</span>
      <span class="ts">{fmtTs(ev.tsMs)}</span>
    </div>

    <div class="reasons">
      {#each ev.reasons as r (r.kind + r.value)}
        <div class="reason" style="border-left-color: {kindMeta(r.kind).color}">
          <span class="kind" style="color: {kindMeta(r.kind).color}">
            {kindMeta(r.kind).label}
          </span>
          <span class="d">{r.detail}</span>
        </div>
      {/each}
    </div>

    <div class="counters">
      <span>CPU {ev.counters.cpuTotal.toFixed(0)}%</span>
      <span>
        内存 {fmtBytes(ev.counters.memUsed)} / {fmtBytes(ev.counters.memTotal)}
      </span>
      <span>
        提交
        {ev.counters.memCommitLimit
          ? ((ev.counters.memCommitUsed / ev.counters.memCommitLimit) * 100).toFixed(0)
          : "--"}
        %
      </span>
      <span>硬缺页 {ev.counters.pageInputPerSec.toFixed(0)} 页/s</span>
      <span>
        磁盘延迟 {(ev.counters.diskAvgSecTransfer * 1000).toFixed(1)} ms
      </span>
      <span>磁盘活跃 {ev.counters.diskActivePct.toFixed(0)}%</span>
      <span>DPC {ev.counters.dpcPerSec.toFixed(0)}/s</span>
      <span>中断 {ev.counters.interruptPct.toFixed(1)}%</span>
      <span>
        网络 ↓{fmtBytesPerSec(ev.counters.netRxBps)} ↑{fmtBytesPerSec(ev.counters.netTxBps)}
      </span>
      {#if ev.counters.gpuUtilPct != null}
        <span>GPU {ev.counters.gpuUtilPct.toFixed(0)}%</span>
      {/if}
    </div>

    {#if ev.hungWindows.length}
      <div class="section">
        <div class="stitle">无响应窗口</div>
        {#each ev.hungWindows as h (h)}
          <div class="hung">{h}</div>
        {/each}
      </div>
    {/if}

    {#if ev.game}
      <div class="section">
        <div class="stitle">帧统计({ev.game.app})</div>
        <div class="line">
          FPS {ev.game.fps} · 平均帧间隔 {ev.game.avgGapMs.toFixed(1)} ms · 最大
          {ev.game.maxGapMs.toFixed(1)} ms · 卡帧 {ev.game.stutters} 次
        </div>
      </div>
    {/if}

    <div class="tops">
      <div class="section">
        <div class="stitle">CPU 占用 Top5</div>
        {#each topList(ev.topCpu, (v) => `${v.toFixed(1)}%`) as line, i (i)}
          <div class="line">{line}</div>
        {:else}
          <div class="none">无数据</div>
        {/each}
      </div>
      <div class="section">
        <div class="stitle">内存占用 Top5</div>
        {#each topList(ev.topMem, (v) => `${v.toFixed(0)} MB`) as line, i (i)}
          <div class="line">{line}</div>
        {:else}
          <div class="none">无数据</div>
        {/each}
      </div>
      <div class="section">
        <div class="stitle">I/O 速率 Top5(含网络)</div>
        {#each topList(ev.topDisk, (v) => fmtBytesPerSec(v)) as line, i (i)}
          <div class="line">{line}</div>
        {:else}
          <div class="none">无数据</div>
        {/each}
      </div>
    </div>
  </div>
{:else}
  <div class="detail empty">
    从左侧选择一个卡顿事件查看归因详情
  </div>
{/if}

<style>
  .detail {
    flex: 1;
    background: var(--panel, #131926);
    border: 1px solid var(--border, #232a3a);
    border-radius: 10px;
    padding: 12px 14px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    min-height: 0;
  }
  .detail.empty {
    align-items: center;
    justify-content: center;
    color: var(--text-dim, #8b94a7);
    font-size: 13px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .title {
    font-size: 14px;
    font-weight: 600;
  }
  .ts {
    font-size: 12.5px;
    color: var(--text-dim, #8b94a7);
  }
  .reasons {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .reason {
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    border-left: 3px solid #888;
    border-radius: 6px;
    padding: 7px 10px;
    display: flex;
    gap: 10px;
    align-items: baseline;
  }
  .kind {
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
  }
  .d {
    font-size: 13px;
    color: var(--text, #e8ecf4);
  }
  .counters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    font-size: 12px;
    color: var(--text-dim, #8b94a7);
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    border-radius: 6px;
    padding: 8px 10px;
  }
  .tops {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
  }
  .section {
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    border-radius: 6px;
    padding: 8px 10px;
  }
  .stitle {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim, #8b94a7);
    margin-bottom: 6px;
  }
  .line {
    font-size: 12.5px;
    padding: 2px 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hung {
    font-size: 12.5px;
    padding: 2px 0;
    color: #f5a623;
  }
  .none {
    font-size: 12px;
    color: var(--text-dim, #8b94a7);
  }
</style>
