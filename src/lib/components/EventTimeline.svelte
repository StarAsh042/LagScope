<script lang="ts">
  import {
    exportEvents,
    fmtTs,
    kindMeta,
    lagEvents,
    revealLastExport,
    select,
  } from "$lib/stores/lagEvents.svelte";
  import type { LagEvent } from "$lib/types";

  function summary(ev: LagEvent): string {
    return ev.reasons.map((r) => kindMeta(r.kind).label).join(" · ");
  }

  const canExport = $derived(lagEvents.list.length > 0 && !lagEvents.exporting);
</script>

<div class="timeline">
  <div class="head">
    <span>时间线({lagEvents.list.length})</span>
    <span class="tools">
      <button
        class="mini"
        disabled={!canExport}
        onclick={() => exportEvents("csv")}
        title="导出为 CSV(Excel 可直接打开)"
      >
        CSV
      </button>
      <button
        class="mini"
        disabled={!canExport}
        onclick={() => exportEvents("json")}
        title="导出为 JSON(含完整计数器快照)"
      >
        JSON
      </button>
    </span>
  </div>
  {#if lagEvents.exporting}
    <div class="note">正在导出…</div>
  {:else if lagEvents.exportError}
    <div class="note err">{lagEvents.exportError}</div>
  {:else if lagEvents.exportNote}
    <div class="note ok">
      {lagEvents.exportNote}
      <button class="link" onclick={() => revealLastExport()}>在文件夹中显示</button>
    </div>
  {/if}
  {#if lagEvents.list.length === 0}
    <div class="empty">暂无卡顿事件 —— 引擎正在后台持续监测</div>
  {:else}
    <div class="scroll">
      {#each lagEvents.list as ev (ev.id ?? ev.tsMs)}
        <button
          class="item"
          class:active={lagEvents.selected?.id === ev.id &&
            lagEvents.selected?.tsMs === ev.tsMs}
          onclick={() => select(ev)}
        >
          <div class="row1">
            <span class="ts">{fmtTs(ev.tsMs)}</span>
            {#if ev.id != null && ev.id < 0}
              <span class="fresh">新</span>
            {/if}
          </div>
          <div class="row2">
            {#each ev.reasons as r (r.kind + r.value)}
              <span class="tag" style="background: {kindMeta(r.kind).color}22;
                        color: {kindMeta(r.kind).color};
                        border-color: {kindMeta(r.kind).color}55">
                {kindMeta(r.kind).label}
              </span>
            {/each}
          </div>
          <div class="row3">{summary(ev)}</div>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .timeline {
    background: var(--panel, #131926);
    border: 1px solid var(--border, #232a3a);
    border-radius: 10px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    width: 320px;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim, #8b94a7);
    margin-bottom: 8px;
  }
  .tools {
    display: flex;
    gap: 4px;
  }
  .mini {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 5px;
    border: 1px solid var(--border, #232a3a);
    background: var(--bg, #0d1220);
    color: var(--text-dim, #8b94a7);
    cursor: pointer;
    font-family: inherit;
  }
  .mini:hover:not(:disabled) {
    border-color: #3b6cd4;
    color: var(--text, #e8ecf4);
  }
  .mini:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .note {
    font-size: 11.5px;
    color: var(--text-dim, #8b94a7);
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    border-radius: 6px;
    padding: 6px 8px;
    margin-bottom: 8px;
    word-break: break-all;
  }
  .note.ok {
    border-color: #1f4a33;
    color: #3dd68c;
  }
  .note.err {
    border-color: #5a2126;
    color: #e5484d;
  }
  .link {
    background: none;
    border: none;
    padding: 0 0 0 6px;
    color: #3b6cd4;
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    text-decoration: underline;
  }
  .empty {
    color: var(--text-dim, #8b94a7);
    font-size: 12.5px;
    padding: 24px 12px;
    text-align: center;
  }
  .scroll {
    overflow: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .item {
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    border-radius: 8px;
    padding: 8px 10px;
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .item:hover {
    border-color: #3b6cd4;
  }
  .item.active {
    border-color: #3b6cd4;
    background: #16203a;
  }
  .row1 {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .ts {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text, #e8ecf4);
  }
  .fresh {
    font-size: 10px;
    background: #e5484d;
    color: #fff;
    border-radius: 4px;
    padding: 1px 5px;
  }
  .row2 {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 5px;
  }
  .tag {
    font-size: 10.5px;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 1px 6px;
  }
  .row3 {
    margin-top: 4px;
    font-size: 11.5px;
    color: var(--text-dim, #8b94a7);
  }
</style>
