<script lang="ts">
  import { onMount } from "svelte";

  import { invoke } from "@tauri-apps/api/core";

  interface Settings {
    cpuPct: number;
    cpuSustainS: number;
    commitPct: number;
    pageInput: number;
    diskTransferMs: number;
    diskSustainS: number;
    interruptPct: number;
    cooldownS: number;
    closeToTray: boolean;
    autostart: boolean;
  }

  let s = $state<Settings | null>(null);
  let saving = $state(false);
  let saved = $state(false);
  let errMsg = $state("");

  onMount(async () => {
    try {
      s = await invoke<Settings>("get_settings");
    } catch (e) {
      errMsg = String(e);
    }
  });

  async function save() {
    if (!s) return;
    saving = true;
    saved = false;
    errMsg = "";
    try {
      await invoke("set_settings", { settings: s });
      saved = true;
      setTimeout(() => (saved = false), 2000);
    } catch (e) {
      errMsg = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="panel">
  <div class="head">设置</div>
  {#if errMsg}
    <div class="err">{errMsg}</div>
  {/if}
  {#if s}
    <div class="group">
      <div class="gtitle">卡顿检测阈值(保存后立即生效)</div>
      <label>
        <span>CPU 占用阈值 %</span>
        <input type="number" min="50" max="100" bind:value={s.cpuPct} />
      </label>
      <label>
        <span>持续秒数</span>
        <input type="number" min="5" max="60" bind:value={s.cpuSustainS} />
      </label>
      <label>
        <span>提交内存占比阈值 %</span>
        <input type="number" min="50" max="100" bind:value={s.commitPct} />
      </label>
      <label>
        <span>硬缺页阈值 页/秒</span>
        <input type="number" min="100" max="5000" bind:value={s.pageInput} />
      </label>
      <label>
        <span>磁盘延迟阈值 ms</span>
        <input type="number" min="5" max="500" bind:value={s.diskTransferMs} />
      </label>
      <label>
        <span>磁盘持续秒数</span>
        <input type="number" min="3" max="60" bind:value={s.diskSustainS} />
      </label>
      <label>
        <span>中断时间阈值 %</span>
        <input type="number" min="5" max="90" bind:value={s.interruptPct} />
      </label>
      <label>
        <span>同类事件冷却 秒</span>
        <input type="number" min="10" max="600" bind:value={s.cooldownS} />
      </label>
    </div>

    <div class="group">
      <div class="gtitle">行为</div>
      <label class="check">
        <input type="checkbox" bind:checked={s.closeToTray} />
        <span>关闭窗口时隐藏到托盘,后台继续监测</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.autostart} />
        <span>开机自动启动</span>
      </label>
    </div>

    <div class="actions">
      <button class="save" disabled={saving} onclick={save}>
        {saving ? "保存中…" : "保存"}
      </button>
      {#if saved}<span class="ok">已保存 ✓</span>{/if}
    </div>
  {:else}
    <div class="loading">加载中…</div>
  {/if}
</div>

<style>
  .panel {
    flex: 1;
    background: var(--panel, #131926);
    border: 1px solid var(--border, #232a3a);
    border-radius: 10px;
    padding: 14px 18px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head {
    font-size: 14px;
    font-weight: 600;
  }
  .group {
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 520px;
  }
  .gtitle {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-dim, #8b94a7);
  }
  label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 13px;
  }
  label.check {
    justify-content: flex-start;
    gap: 10px;
    cursor: pointer;
  }
  input[type="number"] {
    width: 110px;
    background: var(--bg, #0d1220);
    border: 1px solid var(--border, #232a3a);
    color: var(--text, #e8ecf4);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 13px;
    outline: none;
  }
  input[type="number"]:focus {
    border-color: #3b6cd4;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .save {
    background: #3b6cd4;
    border: none;
    color: #fff;
    border-radius: 8px;
    padding: 7px 22px;
    font-size: 13px;
    cursor: pointer;
  }
  .save:hover {
    background: #4a7be0;
  }
  .save:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .ok {
    color: var(--ok, #3dd68c);
    font-size: 12.5px;
  }
  .err {
    color: var(--err, #e5484d);
    font-size: 12.5px;
  }
  .loading {
    color: var(--text-dim, #8b94a7);
    font-size: 13px;
  }
</style>
