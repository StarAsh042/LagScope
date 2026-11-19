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
  import StatusNote from "$lib/components/StatusNote.svelte";

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

  {#if lagEvents.exporting || lagEvents.exportError || lagEvents.exportNote}
    <div class="notes">
      {#if lagEvents.exporting}
        <StatusNote kind="loading" message="正在导出…" />
      {:else if lagEvents.exportError}
        <StatusNote kind="error" message={lagEvents.exportError} />
      {:else}
        <StatusNote
          kind="success"
          message={lagEvents.exportNote}
          actionLabel="在文件夹中显示"
          onAction={() => revealLastExport()}
        />
      {/if}
    </div>
  {/if}

  {#if lagEvents.list.length === 0}
    <StatusNote kind="empty" message="暂无卡顿事件 —— 引擎正在后台持续监测" />
  {:else if visible.length === 0}
    <StatusNote
      kind="empty"
      message="没有符合条件的事件"
      actionLabel="清除筛选"
      onAction={clearFilter}
    />
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
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-2xl);
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
    gap: var(--space-8);
    font-size: var(--font-md);
    font-weight: var(--weight-bold);
    color: var(--text-dim);
    margin-bottom: 8px;
  }
  .tools {
    display: flex;
    gap: var(--space-4);
  }
  .mini {
    font-size: var(--font-xs);
    font-weight: var(--weight-bold);
    padding: 2px 7px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-dim);
    cursor: pointer;
    font-family: inherit;
  }
  .mini:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--text);
  }
  .mini:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .filters {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
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
    font-size: var(--font-sm);
    padding: 5px 22px 5px 8px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
  }
  .search input::placeholder {
    color: var(--placeholder);
  }
  .search input:focus {
    border-color: var(--accent);
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
    color: var(--text-dim);
    font-size: var(--font-lg);
    line-height: 1;
    padding: 2px 4px;
    cursor: pointer;
  }
  .clear:hover {
    color: var(--text);
  }
  .range,
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    align-items: center;
  }
  .chip {
    font-size: var(--font-xs);
    font-family: inherit;
    padding: 2px 7px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-dim);
    cursor: pointer;
  }
  .chip:hover {
    border-color: var(--accent);
    color: var(--text);
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .chip.kind.on {
    border-color: var(--k, var(--accent));
    color: var(--k, var(--accent));
    background: color-mix(in srgb, var(--k, var(--accent)) 16%, transparent);
  }
  .chip .n {
    opacity: 0.6;
    margin-left: 3px;
  }
  .clear-all {
    font-size: var(--font-xs);
  }
  .notes {
    margin-bottom: 8px;
  }
  .link {
    background: none;
    border: none;
    padding: 0 0 0 6px;
    color: var(--accent);
    font: inherit;
    font-size: var(--font-xs);
    cursor: pointer;
    text-decoration: underline;
  }
  .scroll {
    overflow: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }
  .item {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 8px 10px;
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .item:hover {
    border-color: var(--accent);
  }
  .item.active {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .row1 {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .ts {
    font-size: var(--font-sm);
    font-weight: var(--weight-bold);
    color: var(--text);
  }
  .fresh {
    font-size: var(--font-2xs);
    background: var(--err);
    color: var(--on-accent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
  }
  .row2 {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    margin-top: 5px;
  }
  .tag {
    font-size: var(--font-2xs);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    padding: 1px 6px;
  }
  .row3 {
    margin-top: 4px;
    font-size: var(--font-xs);
    color: var(--text-dim);
  }
</style>
