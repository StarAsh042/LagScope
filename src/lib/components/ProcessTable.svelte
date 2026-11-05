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

  async function kill(p: ProcessInfo) {
    if (!confirm(`确定结束进程 ${p.name} (PID ${p.pid}) 吗?`)) return;
    errMsg = "";
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
</script>

<div class="panel">
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
                onclick={() => kill(p)}
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
    background: var(--panel, #131926);
    border: 1px solid var(--border, #232a3a);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
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
    color: var(--text-dim, #8b94a7);
  }
  input {
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    color: var(--text, #e8ecf4);
    border-radius: 6px;
    padding: 5px 10px;
    font-size: 12px;
    width: 220px;
    outline: none;
  }
  input:focus {
    border-color: #3b6cd4;
  }
  .err {
    color: #e5484d;
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
    background: var(--panel, #131926);
    color: var(--text-dim, #8b94a7);
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border, #232a3a);
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
    border-bottom: 1px solid #1a2130;
    color: var(--text, #e8ecf4);
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
    background: #1a2130;
  }
  .hot {
    color: #f5a623;
    font-weight: 600;
  }
  .badge {
    display: inline-block;
    background: #3a2b12;
    color: #f5a623;
    border: 1px solid #6b4d16;
    border-radius: 4px;
    font-size: 10px;
    padding: 1px 5px;
    margin-right: 6px;
    vertical-align: middle;
  }
  .kill {
    background: transparent;
    border: 1px solid #593033;
    color: #e5484d;
    border-radius: 5px;
    font-size: 11px;
    padding: 2px 8px;
    cursor: pointer;
  }
  .kill:hover {
    background: #2a1a1d;
  }
  .kill:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
