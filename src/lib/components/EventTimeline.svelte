<script lang="ts">
  import { onMount } from "svelte";
  import {
    clearFilter,
    exportEvents,
    fmtTs,
    kindMeta,
    lagEvents,
    rangeLabel,
    revealLastExport,
    select,
    timeline,
    toggleKindFilter,
  } from "$lib/stores/lagEvents.svelte";
  import {
    filterEvents,
    isFilterActive,
    kindCounts,
    RANGE_OPTIONS,
    type RangeKey,
  } from "$lib/timeline-filter";
  import type { LagEvent } from "$lib/types";

  function summary(ev: LagEvent): string {
    return ev.reasons.map((r) => kindMeta(r.kind).label).join(" · ");
  }

  const canExport = $derived(lagEvents.list.length > 0 && !lagEvents.exporting);

  // 时间范围筛选依赖"现在",定时推进一次才能让"1 小时"随着时间自然收缩
  let nowMs = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => {
      nowMs = Date.now();
    }, 30_000);
    return () => clearInterval(timer);
  });

  const ctx = $derived({
    nowMs,
    kindLabel: (kind: string) => kindMeta(kind).label,
    fmtTs,
  });

  const filter = $derived({
    query: timeline.query,
    kinds: timeline.kinds,
    range: timeline.range,
  });

  const visible = $derived(filterEvents(lagEvents.list, filter, ctx));
  const counts = $derived(kindCounts(lagEvents.list, filter, ctx));
  const filtering = $derived(isFilterActive(filter));
</script>

<div class="timeline">
  <div class="head">
    <span>
      时间线({filtering ? `${visible.length}/${lagEvents.list.length}` : lagEvents.list.length})
    </span>
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

  {#if lagEvents.list.length > 0}
    <div class="filters">
      <label class="search">
        <input
          type="search"
          placeholder="搜索进程 / 驱动 / 窗口 / 原因"
          bind:value={timeline.query}
          aria-label="搜索卡顿事件"
        />
        {#if timeline.query}
          <button
            class="clear"
            onclick={() => (timeline.query = "")}
            title="清空搜索"
            aria-label="清空搜索"
          >
            ×
          </button>
        {/if}
      </label>

      <div class="range">
        {#each RANGE_OPTIONS as opt (opt.key)}
          <button
            class="chip"
            class:on={timeline.range === opt.key}
            onclick={() => (timeline.range = opt.key as RangeKey)}
            title="只看最近{opt.label}的事件"
          >
            {opt.label}
          </button>
        {/each}
        {#if filtering}
          <button class="link clear-all" onclick={clearFilter}>清除筛选</button>
        {/if}
      </div>

      {#if counts.length > 1}
        <div class="kinds">
          {#each counts as c (c.kind)}
            <button
              class="chip kind"
              class:on={timeline.kinds.includes(c.kind)}
              style="--k: {kindMeta(c.kind).color}"
              onclick={() => toggleKindFilter(c.kind)}
              title="只保留含「{c.label}」的事件,可多选"
            >
              {c.label}
              <span class="n">{c.count}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

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
  {:else if visible.length === 0}
    <div class="empty">
      没有符合条件的事件
      <button class="link" onclick={clearFilter}>清除筛选</button>
    </div>
  {:else}
    <div class="scroll">
      {#each visible as ev (ev.id ?? ev.tsMs)}
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
  .filters {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 8px;
  }
  .search {
    position: relative;
    display: block;
  }
  .search input {
    width: 100%;
    box-sizing: border-box;
    font: inherit;
    font-size: 12px;
    padding: 5px 22px 5px 8px;
    border-radius: 6px;
    border: 1px solid var(--border, #232a3a);
    background: var(--bg, #0d1220);
    color: var(--text, #e8ecf4);
  }
  .search input::placeholder {
    color: #5b6478;
  }
  .search input:focus {
    outline: none;
    border-color: #3b6cd4;
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }
  .clear {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    border: none;
    background: none;
    color: var(--text-dim, #8b94a7);
    font-size: 14px;
    line-height: 1;
    padding: 2px 4px;
    cursor: pointer;
  }
  .clear:hover {
    color: var(--text, #e8ecf4);
  }
  .range,
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .chip {
    font-size: 11px;
    font-family: inherit;
    padding: 2px 7px;
    border-radius: 5px;
    border: 1px solid var(--border, #232a3a);
    background: var(--bg, #0d1220);
    color: var(--text-dim, #8b94a7);
    cursor: pointer;
  }
  .chip:hover {
    border-color: #3b6cd4;
    color: var(--text, #e8ecf4);
  }
  .chip.on {
    border-color: #3b6cd4;
    background: #16203a;
    color: var(--text, #e8ecf4);
  }
  .chip.kind.on {
    border-color: var(--k, #3b6cd4);
    color: var(--k, #3b6cd4);
    background: color-mix(in srgb, var(--k, #3b6cd4) 16%, transparent);
  }
  .chip .n {
    opacity: 0.6;
    margin-left: 3px;
  }
  .clear-all {
    font-size: 11px;
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
  .empty .link {
    display: block;
    margin: 6px auto 0;
    padding: 0;
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
