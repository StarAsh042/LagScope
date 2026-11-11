<script lang="ts">
  import {
    deleteEvent,
    fmtTs,
    kindMeta,
    lagEvents,
  } from "$lib/stores/lagEvents.svelte";
  import { fmtBytes, fmtBytesPerSec } from "$lib/stores/metrics.svelte";
  import StatusNote from "$lib/components/StatusNote.svelte";
  import type { LagEvent, TopProc } from "$lib/types";

  const ev = $derived(lagEvents.selected);

  /** 只有已入库的记录才删得掉;实时推送里 id 为负的旧式事件没有库行可删 */
  const deletable = $derived(ev != null && ev.id != null && ev.id > 0);
  const deleting = $derived(ev?.id != null && lagEvents.deleting === ev.id);

  let confirming = $state(false);

  // 换一条事件就把确认态收掉,避免确认框跟着跑到另一条记录上
  $effect(() => {
    void ev?.id;
    void ev?.tsMs;
    confirming = false;
  });

  async function confirmDelete() {
    if (ev == null) return;
    if (await deleteEvent(ev)) confirming = false;
  }

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
      <span class="head-right">
        <span class="ts">{fmtTs(ev.tsMs)}</span>
        {#if confirming}
          <span class="ask">删除这条记录?</span>
          <button class="act danger" onclick={confirmDelete} disabled={deleting}>
            {deleting ? "删除中…" : "删除"}
          </button>
          <button
            class="act"
            onclick={() => (confirming = false)}
            disabled={deleting}
          >
            取消
          </button>
        {:else}
          <button
            class="act"
            onclick={() => (confirming = true)}
            disabled={!deletable}
            title={deletable
              ? "从事件库中删除这条记录"
              : "这条事件没有入库记录,无法删除"}
          >
            删除
          </button>
        {/if}
      </span>
    </div>

    {#if lagEvents.loadError}
      <StatusNote kind="error" message={lagEvents.loadError} />
    {/if}

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

    {#if ev.dpcTop}
      <div class="section">
        <div class="stitle">DPC 驱动分布(共 {ev.dpcTop.total} 次/秒)</div>
        {#each ev.dpcTop.drivers as [name, count], i (name)}
          <div class="line">
            {i + 1}. {name} — {count} 次
            ({((count / ev.dpcTop.total) * 100).toFixed(0)}%)
          </div>
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
  <div class="detail">
    <StatusNote kind="empty" message="从左侧选择一个卡顿事件查看归因详情" fill />
  </div>
{/if}

<style>
  .detail {
    flex: 1;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-2xl);
    padding: 12px 14px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-12);
    min-width: 0;
    min-height: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-8);
  }
  .title {
    font-size: var(--font-lg);
    font-weight: var(--weight-bold);
  }
  .head-right {
    display: flex;
    align-items: center;
    gap: var(--space-6);
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .ts {
    font-size: var(--font-sm);
    color: var(--text-dim);
  }
  .ask {
    font-size: var(--font-sm);
    color: var(--text-dim);
  }
  .act {
    font-family: inherit;
    font-size: var(--font-xs);
    padding: 2px 8px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-dim);
    cursor: pointer;
  }
  .act:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text);
  }
  .act:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .act.danger {
    border-color: var(--err-border);
    color: var(--err);
  }
  .act.danger:hover:not(:disabled) {
    background: var(--err-bg-strong);
    border-color: var(--err-border-strong);
    color: var(--err-strong);
  }
  .reasons {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .reason {
    background: var(--bg);
    border: 1px solid var(--border);
    border-left: 3px solid var(--neutral);
    border-radius: var(--radius-lg);
    padding: 7px 10px;
    display: flex;
    gap: var(--space-10);
    align-items: baseline;
  }
  .kind {
    font-size: var(--font-sm);
    font-weight: var(--weight-bold);
    white-space: nowrap;
  }
  .d {
    font-size: var(--font-md);
    color: var(--text);
  }
  .counters {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-6) var(--space-16);
    font-size: var(--font-sm);
    color: var(--text-dim);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 8px 10px;
  }
  .tops {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-10);
  }
  .section {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 8px 10px;
  }
  .stitle {
    font-size: var(--font-sm);
    font-weight: var(--weight-bold);
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  .line {
    font-size: var(--font-sm);
    padding: 2px 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hung {
    font-size: var(--font-sm);
    padding: 2px 0;
    color: var(--warn);
  }
  .none {
    font-size: var(--font-sm);
    color: var(--text-dim);
  }
</style>
