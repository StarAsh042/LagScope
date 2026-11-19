<script lang="ts">
  /**
   * 运行日志查看页:读后端滚动日志(lagscope.log + 3 个滚动备份),提供级别/关键词筛选、
   * 自动刷新、清空与在资源管理器中打开。日志写入是旁路,读取失败不影响其他页面。
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { LogLine } from "$lib/types";
  import { errorText } from "$lib/errors";
  import StatusNote from "$lib/components/StatusNote.svelte";

  let lines = $state<LogLine[]>([]);
  let loading = $state(true);
  let err = $state("");
  let query = $state("");
  let level = $state<"ALL" | "ERROR" | "WARN" | "INFO" | "DEBUG">("ALL");
  let autoRefresh = $state(true);
  let confirming = $state(false);
  let notice = $state("");

  const levels = ["ALL", "ERROR", "WARN", "INFO", "DEBUG"] as const;

  async function refresh() {
    try {
      const data = await invoke<LogLine[]>("get_logs", { limit: 300 });
      lines = data;
      err = "";
    } catch (e) {
      err = errorText(e);
    } finally {
      loading = false;
    }
  }

  function fmtTime(ts: number): string {
    const d = new Date(ts);
    const p = (n: number, w = 2) => String(n).padStart(w, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
  }

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return lines.filter((l) => {
      if (level !== "ALL" && l.level !== level) return false;
      if (!q) return true;
      return (
        l.message.toLowerCase().includes(q) ||
        l.target.toLowerCase().includes(q) ||
        l.level.toLowerCase().includes(q)
      );
    });
  });

  async function clear() {
    confirming = false;
    try {
      await invoke("clear_logs");
      notice = "日志已清空";
      await refresh();
    } catch (e) {
      err = errorText(e);
    }
  }

  async function openDir() {
    try {
      await invoke("open_log_dir");
    } catch (e) {
      err = errorText(e);
    }
  }

  onMount(() => {
    void refresh();
    const timer = setInterval(() => {
      if (autoRefresh) void refresh();
    }, 3000);
    return () => clearInterval(timer);
  });
</script>

<section class="log-viewer">
  <div class="toolbar">
    <div class="filters">
      <label>
        级别
        <select bind:value={level}>
          {#each levels as lv}
            <option value={lv}>{lv === "ALL" ? "全部" : lv}</option>
          {/each}
        </select>
      </label>
      <label class="search">
        关键词
        <input type="text" bind:value={query} placeholder="搜索消息 / 模块" />
      </label>
      <label class="auto">
        <input type="checkbox" bind:checked={autoRefresh} />
        自动刷新
      </label>
    </div>
    <div class="actions">
      <button class="ghost" onclick={refresh} disabled={loading}>刷新</button>
      {#if confirming}
        <button class="danger" onclick={clear}>确认清空</button>
        <button class="ghost" onclick={() => (confirming = false)}>取消</button>
      {:else}
        <button class="ghost" onclick={() => (confirming = true)}>清空</button>
      {/if}
      <button class="ghost" onclick={openDir}>打开日志目录</button>
    </div>
  </div>

  {#if notice}
    <StatusNote kind="success" message={notice} />
  {/if}
  {#if err}
    <StatusNote kind="error" message="读取日志失败" hint={err} />
  {/if}

  <div class="list" role="log">
    {#if loading && lines.length === 0}
      <StatusNote kind="loading" message="正在读取日志…" fill />
    {:else if filtered.length === 0}
      <StatusNote
        kind="empty"
        message="没有日志记录"
        hint="卡顿触发、采集异常与诊断信息会写到这里"
        fill
      />
    {:else}
      {#each filtered as l}
        <div class="row lv-{l.level.toLowerCase()}">
          <span class="ts">{fmtTime(l.tsMs)}</span>
          <span class="lv">{l.level}</span>
          <span class="target">[{l.target}]</span>
          <span class="msg">{l.message}</span>
        </div>
      {/each}
    {/if}
  </div>

  <footer>
    <span class="stats">共 {filtered.length} / {lines.length} 行 · 保留最近 300 行,滚动文件 5MB × 3</span>
  </footer>
</section>

<style>
  .log-viewer {
    display: flex;
    flex-direction: column;
    gap: var(--space-12);
    flex: 1;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-12);
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-12);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    font-size: var(--font-xs);
    color: var(--text-dim);
  }
  select,
  input[type="text"] {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text);
    font-size: var(--font-sm);
    padding: 5px 8px;
  }
  input[type="text"] {
    min-width: 200px;
  }
  .auto {
    flex-direction: row;
    align-items: center;
    gap: var(--space-6);
    padding-bottom: 6px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-8);
  }
  button {
    background: var(--panel-alt);
    border: 1px solid var(--border);
    color: var(--text-dim);
    border-radius: var(--radius-xl);
    padding: 5px 12px;
    font-size: var(--font-sm);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  button.danger {
    color: var(--err);
    border-color: var(--err-border);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-family: "Cascadia Mono", Consolas, monospace;
    font-size: var(--font-xs);
    line-height: 1.5;
  }
  .row {
    display: flex;
    gap: var(--space-8);
    padding: 2px 8px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    border-bottom: 1px solid var(--track-deep);
  }
  .row:hover {
    background: var(--panel);
  }
  .ts {
    color: var(--text-dim);
    flex: none;
  }
  .lv {
    flex: none;
    width: 42px;
    color: var(--text-dim);
  }
  .lv-error .lv {
    color: var(--err);
  }
  .lv-warn .lv {
    color: var(--warn);
  }
  .lv-info .lv {
    color: var(--accent-strong);
  }
  .target {
    flex: none;
    color: var(--accent-strong);
  }
  .msg {
    color: var(--text);
  }
  footer {
    font-size: var(--font-xs);
    color: var(--text-dim);
  }
</style>
