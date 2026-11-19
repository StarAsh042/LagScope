<script lang="ts">
  // 每磁盘卷 / 每网卡细分:定位具体是哪块盘或哪张网卡在产生流量。
  // 数据由 metrics store 每 2s 从后端 get_io_breakdown 拉取,不进入 1Hz 图表推送。
  import Icon from "$lib/components/Icon.svelte";
  import type { IoBreakdown } from "$lib/types";
  import { fmtBytesPerSec } from "$lib/stores/metrics.svelte";

  let { io }: { io: IoBreakdown } = $props();

  const volumes = $derived(
    [...io.volumes].sort((a, b) => b.readBps + b.writeBps - (a.readBps + a.writeBps))
  );
  const nics = $derived(
    [...io.nics].sort((a, b) => b.rxBps + b.txBps - (a.rxBps + a.txBps))
  );
  const maxVol = $derived(Math.max(1, ...volumes.map((v) => v.readBps + v.writeBps)));
  const maxNic = $derived(Math.max(1, ...nics.map((n) => n.rxBps + n.txBps)));
  const pct = (v: number, max: number) => Math.max(2, (v / max) * 100);
</script>

<section class="io-breakdown">
  <div class="col">
    <h3>磁盘卷</h3>
    {#if volumes.length === 0}
      <p class="empty">暂无卷级计数</p>
    {:else}
      {#each volumes as v (v.mount)}
        <div class="row">
          <div class="head">
            <span class="name" title={v.mount}>{v.mount}</span>
            <span class="rate"><Icon name="down" size={12} />{fmtBytesPerSec(v.readBps)} <Icon name="up" size={12} />{fmtBytesPerSec(v.writeBps)}</span>
          </div>
          <div class="bar">
            <span class="read" style="width:{pct(v.readBps, maxVol)}%"></span>
            <span class="write" style="width:{pct(v.writeBps, maxVol)}%"></span>
          </div>
        </div>
      {/each}
    {/if}
  </div>
  <div class="col">
    <h3>网卡</h3>
    {#if nics.length === 0}
      <p class="empty">暂无网卡数据</p>
    {:else}
      {#each nics as n (n.name)}
        <div class="row">
          <div class="head">
            <span class="name" title={n.name}>{n.name}</span>
            <span class="rate"><Icon name="down" size={12} />{fmtBytesPerSec(n.rxBps)} <Icon name="up" size={12} />{fmtBytesPerSec(n.txBps)}</span>
          </div>
          <div class="bar">
            <span class="rx" style="width:{pct(n.rxBps, maxNic)}%"></span>
            <span class="tx" style="width:{pct(n.txBps, maxNic)}%"></span>
          </div>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .io-breakdown {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-16);
    padding: var(--space-10) var(--space-12);
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .col {
    min-width: 0;
  }
  h3 {
    margin: 0 0 var(--space-8);
    font-size: var(--font-sm);
    font-weight: var(--weight-medium);
    color: var(--text-dim);
  }
  .empty {
    margin: 0;
    font-size: var(--font-xs);
    color: var(--placeholder);
  }
  .row + .row {
    margin-top: var(--space-6);
  }
  .head {
    display: flex;
    justify-content: space-between;
    gap: var(--space-8);
    font-size: var(--font-xs);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .rate {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: none;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .bar {
    display: flex;
    height: 5px;
    margin-top: var(--space-4);
    background: var(--track-deep);
    border-radius: var(--radius-xs);
    overflow: hidden;
  }
  .bar span {
    height: 100%;
  }
  .read { background: var(--chart-disk-read); }
  .write { background: var(--chart-disk-write); }
  .rx { background: var(--chart-net-rx); }
  .tx { background: var(--chart-net-tx); }
</style>
