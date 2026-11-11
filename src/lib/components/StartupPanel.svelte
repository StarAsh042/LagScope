<script lang="ts">
  import { onMount } from "svelte";

  import { invoke } from "@tauri-apps/api/core";

  import type { StartupItem } from "$lib/types";
  import StatusNote from "$lib/components/StatusNote.svelte";

  let items = $state<StartupItem[]>([]);
  let loading = $state(true);
  let errMsg = $state("");
  let busy = $state("");
  let search = $state("");

  /** 同一名字可能同时存在于 HKCU 与 HKLM,忙碌标记必须带上来源 */
  const key = (it: StartupItem) => it.source + "\u0000" + it.name;

  /** 系统级项目写 StartupApproved 需要管理员权限 */
  const needsAdmin = (it: StartupItem) =>
    it.source === "machineRun" || it.source === "machineRunWow";

  /** 启动文件夹项目没有启用/禁用开关,只能删快捷方式 */
  const isFolder = (it: StartupItem) =>
    it.source === "userFolder" || it.source === "commonFolder";

  const adminHint = $derived(
    errMsg.includes("需要管理员权限")
      ? "关闭 LagScope,右键「以管理员身份运行」后再试。"
      : ""
  );

  async function load() {
    loading = true;
    errMsg = "";
    try {
      items = await invoke<StartupItem[]>("get_startup_items");
    } catch (e) {
      errMsg = String(e);
    } finally {
      loading = false;
    }
  }
  onMount(load);

  const view = $derived.by(() => {
    const kw = search.trim().toLowerCase();
    return kw
      ? items.filter(
          (i) =>
            i.name.toLowerCase().includes(kw) ||
            i.command.toLowerCase().includes(kw)
        )
      : items;
  });

  const leftovers = $derived(
    items.filter((i) => i.enabled && !i.targetExists).length
  );
  const disabledCount = $derived(items.filter((i) => !i.enabled).length);

  function fmtSize(bytes: number | null): string {
    if (bytes == null) return "--";
    if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(1) + " MB";
    if (bytes >= 1024) return (bytes / 1024).toFixed(0) + " KB";
    return bytes + " B";
  }

  async function toggle(item: StartupItem) {
    busy = key(item);
    errMsg = "";
    try {
      await invoke("set_startup_enabled", {
        source: item.source,
        name: item.name,
        enabled: !item.enabled,
      });
      await load();
    } catch (e) {
      errMsg = String(e);
    } finally {
      busy = "";
    }
  }
</script>

<div class="panel">
  <div class="head">
    <span>启动项({items.length})</span>
    <span class="stats">
      残留 {leftovers} · 已禁用 {disabledCount}
      <button class="refresh" onclick={load}>刷新</button>
    </span>
  </div>
  <input class="search" type="search" placeholder="搜索名称或命令…" bind:value={search} />
  {#if errMsg}
    <StatusNote kind="error" message={errMsg} hint={adminHint} />
  {/if}
  {#if loading}
    <StatusNote kind="loading" message="扫描中…" />
  {:else}
    <div class="scroll">
      <table>
        <thead>
          <tr>
            <th>名称</th>
            <th>来源</th>
            <th>命令 / 目标</th>
            <th class="num">大小</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {#each view as it (it.source + it.name)}
            <tr>
              <td>
                {it.name}
                {#if it.enabled && !it.targetExists}
                  <span class="badge warn">残留</span>
                {/if}
              </td>
              <td class="dim">{it.sourceLabel}</td>
              <td class="cmd" title={it.command}>
                {it.target ?? it.command}
                {#if !it.targetExists}<span class="missing">(缺失)</span>{/if}
              </td>
              <td class="num">{fmtSize(it.targetSize)}</td>
              <td>
                {#if it.enabled}
                  <span class="state on">启用</span>
                {:else}
                  <span class="state off">已禁用</span>
                {/if}
              </td>
              <td>
                {#if isFolder(it)}
                  <span class="dim" title={it.advice}>{it.advice || "—"}</span>
                {:else}
                  <button
                    class="act"
                    class:admin={needsAdmin(it)}
                    disabled={busy === key(it)}
                    title={needsAdmin(it) ? "系统级项目,修改需要管理员权限" : undefined}
                    onclick={() => toggle(it)}
                  >
                    {busy === key(it) ? "…" : it.enabled ? "禁用" : "启用"}
                  </button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
  <div class="note">
    禁用/启用通过 Explorer StartupApproved 键实现,与任务管理器"启动应用"一致。
    系统级(HKLM)项目需要以管理员身份运行本程序才能修改;启动文件夹项目请删除对应快捷方式。
  </div>
</div>

<style>
  .panel {
    flex: 1 0 480px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-2xl);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    min-height: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: var(--font-md);
    font-weight: var(--weight-bold);
  }
  .stats {
    font-size: var(--font-sm);
    font-weight: var(--weight-normal);
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: var(--space-8);
  }
  .refresh {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-dim);
    border-radius: var(--radius-lg);
    font-size: var(--font-xs);
    padding: 3px 10px;
    cursor: pointer;
  }
  .refresh:hover {
    color: var(--text);
  }
  .search {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: var(--radius-lg);
    padding: 5px 10px;
    font-size: var(--font-sm);
    width: 260px;
  }
  .search:focus {
    border-color: var(--accent);
  }
  .scroll {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-sm);
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    color: var(--text-dim);
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    font-weight: var(--weight-medium);
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
  td.cmd {
    max-width: 380px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-dim);
  }
  tr:hover td {
    background: var(--panel-alt);
  }
  .dim {
    color: var(--text-dim);
  }
  .missing {
    color: var(--err);
  }
  .badge.warn {
    display: inline-block;
    background: var(--warn-bg);
    color: var(--warn);
    border: 1px solid var(--warn-border);
    border-radius: var(--radius-sm);
    font-size: var(--font-2xs);
    padding: 1px 5px;
    margin-left: 6px;
  }
  .state.on {
    color: var(--ok);
  }
  .state.off {
    color: var(--text-dim);
  }
  .act {
    background: transparent;
    border: 1px solid var(--accent-border);
    color: var(--accent-strong);
    border-radius: var(--radius-md);
    font-size: var(--font-xs);
    padding: 2px 10px;
    cursor: pointer;
  }
  .act:hover {
    background: var(--accent-soft);
  }
  .act.admin {
    border-color: var(--warn-border-soft);
    color: var(--warn-strong);
  }
  .act.admin:hover {
    background: var(--warn-bg-soft);
  }
  .act:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .note {
    font-size: var(--font-xs);
    color: var(--text-dim);
    border-top: 1px solid var(--border);
    padding-top: 6px;
  }
</style>
