<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  import type { ProcessInfo } from "$lib/types";
  import { fmtBytesPerSec } from "$lib/stores/metrics.svelte";

  let { processes }: { processes: ProcessInfo[] } = $props();

  let search = $state("");
  let sortKey = $state<keyof ProcessInfo>("cpuPct");
  let sortAsc = $state(false);
  let killing = $state<number | null>(null);
  let errMsg = $state("");
  // 待确认的结束进程请求。WebView2 里 window.confirm 可能被静默抑制(返回
  // false 而不弹窗),「结束」按钮于是点了没反应;改用页面内确认层,行为不再
  // 依赖宿主对话框。
  let confirming = $state<ProcessInfo | null>(null);
  let cancelBtn = $state<HTMLButtonElement | null>(null);

  type Col = {
    key: keyof ProcessInfo;
    label: string;
    num?: boolean;
    title?: string;
  };
  const cols: Col[] = [
    { key: "name", label: "进程" },
    { key: "pid", label: "PID", num: true },
    { key: "cpuPct", label: "CPU %", num: true },
    { key: "memMb", label: "内存 MB", num: true },
    { key: "diskReadBps", label: "读 B/s", num: true, title: "含全部 I/O(含网络)" },
    { key: "diskWriteBps", label: "写 B/s", num: true, title: "含全部 I/O(含网络)" },
  ];

  const view = $derived.by(() => {
    const kw = search.trim().toLowerCase();
    let list = processes;
    if (kw) list = list.filter((p) => p.name.toLowerCase().includes(kw) || String(p.pid) === kw);
    const sorted = [...list].sort((a, b) => {
      const va = a[sortKey];
      const vb = b[sortKey];
      const cmp =
        typeof va === "number" && typeof vb === "number"
          ? va - vb
          : String(va).localeCompare(String(vb));
      return sortAsc ? cmp : -cmp;
    });
    return sorted.slice(0, 300);
  });

  function setSort(key: keyof ProcessInfo) {
    if (sortKey === key) {
      sortAsc = !sortAsc;
    } else {
      sortKey = key;
      sortAsc = false; // 默认降序(最耗资源的在前)
    }
  }

  const isHeavy = (p: ProcessInfo) => p.cpuPct >= 45 || p.memMb >= 1024;

  function requestKill(p: ProcessInfo) {
    errMsg = "";
    confirming = p;
  }

  function cancelKill() {
    if (killing !== null) return; // 已发请求,等结果,别让用户以为取消了
    confirming = null;
  }

  async function confirmKill() {
    const p = confirming;
    if (!p) return;
    confirming = null;
    killing = p.pid;
    try {
      const ok = await invoke<boolean>("kill_process", { pid: p.pid });
      if (!ok) errMsg = `结束 ${p.name} 失败`;
    } catch (e) {
      errMsg = String(e);
    } finally {
      killing = null;
    }
  }

  // 打开确认层时把焦点送到「取消」,避免回车误触发破坏性操作
  $effect(() => {
    if (confirming && cancelBtn) cancelBtn.focus();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && confirming && killing === null) {
      e.preventDefault();
      confirming = null;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="panel">
  {#if confirming}
    <div class="confirm-backdrop">
      <div
        class="confirm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="kill-title"
        aria-describedby="kill-desc"
      >
        <div class="confirm-title" id="kill-title">结束进程?</div>
        <div class="confirm-body" id="kill-desc">
          <strong>{confirming.name}</strong>
          <span class="dim">PID {confirming.pid}</span>
          <br />
          未保存的工作会丢失,进程无法恢复。
        </div>
        <div class="confirm-actions">
          <button class="act" bind:this={cancelBtn} onclick={cancelKill}>取消</button>
          <button class="act danger" onclick={confirmKill}>结束进程</button>
        </div>
      </div>
    </div>
  {/if}
  <div class="toolbar">
    <span class="title">进程({view.length}{search ? " / 过滤" : ""})</span>
    <input
      type="search"
      placeholder="搜索进程名或 PID…"
      bind:value={search}
    />
  </div>
  {#if errMsg}
    <div class="err">{errMsg}</div>
  {/if}
  <div class="scroll">
    <table>
      <thead>
        <tr>
          {#each cols as c (c.key)}
            <th
              class:num={c.num}
              title={c.title}
              onclick={() => setSort(c.key)}
              role="button"
              tabindex="0"
            >
              {c.label}
              {#if sortKey === c.key}
                <span class="arrow">{sortAsc ? "▲" : "▼"}</span>
              {/if}
            </th>
          {/each}
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each view as p (p.pid)}
          <tr>
            <td class="name">
              {#if isHeavy(p)}<span class="badge" title="CPU ≥45% 或内存 ≥1GB">高耗</span>{/if}
              <span title={p.exePath ?? ""}>{p.name}</span>
            </td>
            <td class="num">{p.pid}</td>
            <td class="num {p.cpuPct >= 45 ? 'hot' : ''}">{p.cpuPct.toFixed(1)}</td>
            <td class="num {p.memMb >= 1024 ? 'hot' : ''}">{p.memMb.toFixed(0)}</td>
            <td class="num">{fmtBytesPerSec(p.diskReadBps)}</td>
            <td class="num">{fmtBytesPerSec(p.diskWriteBps)}</td>
            <td class="num">
              <button
                disabled={killing === p.pid}
                onclick={() => requestKill(p)}
                class="kill"
              >
                {killing === p.pid ? "…" : "结束"}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .panel {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    /* 主界面可滚动:至少 340px 高,空间充足时撑满剩余 */
    flex: 1 0 340px;
    min-height: 340px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
  }
  input {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 6px;
    padding: 5px 10px;
    font-size: 12px;
    width: 220px;
    outline: none;
  }
  input:focus {
    border-color: var(--accent);
  }
  .err {
    color: var(--err);
    font-size: 12px;
    margin-bottom: 6px;
  }
  .scroll {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    color: var(--text-dim);
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    user-select: none;
    font-weight: 500;
    white-space: nowrap;
    z-index: 1;
  }
  th.num {
    text-align: right;
  }
  td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--panel-alt);
    color: var(--text);
    white-space: nowrap;
  }
  td.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  td.name {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  tr:hover td {
    background: var(--panel-alt);
  }
  .hot {
    color: var(--warn);
    font-weight: 600;
  }
  .badge {
    display: inline-block;
    background: var(--warn-bg);
    color: var(--warn);
    border: 1px solid var(--warn-border);
    border-radius: 4px;
    font-size: 10px;
    padding: 1px 5px;
    margin-right: 6px;
    vertical-align: middle;
  }
  .kill {
    background: transparent;
    border: 1px solid var(--err-border-soft);
    color: var(--err);
    border-radius: 5px;
    font-size: 11px;
    padding: 2px 8px;
    cursor: pointer;
  }
  .kill:hover {
    background: var(--err-bg-hover);
  }
  .kill:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* 页面内确认层:不依赖 WebView2 的 window.confirm */
  .confirm-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(6, 9, 16, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }
  .confirm {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 16px 18px;
    width: 340px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  .confirm-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 8px;
  }
  .confirm-body {
    font-size: 12.5px;
    line-height: 1.7;
    color: var(--text);
    margin-bottom: 14px;
  }
  .confirm-body .dim {
    color: var(--text-dim);
    margin-left: 6px;
    font-variant-numeric: tabular-nums;
  }
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .act {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 6px;
    font-size: 12px;
    padding: 4px 14px;
    cursor: pointer;
  }
  .act:hover {
    background: var(--panel-alt);
  }
  .act:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .act.danger {
    border-color: var(--err-border-soft);
    color: var(--err);
  }
  .act.danger:hover {
    background: var(--err-bg-hover);
  }
</style>
