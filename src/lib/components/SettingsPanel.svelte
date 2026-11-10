<script lang="ts">
  import { onMount } from "svelte";

  import { invoke } from "@tauri-apps/api/core";

  interface MonitorStatus {
    frameActive: boolean;
    dpcActive: boolean;
    frameEnabled: boolean;
    dpcEnabled: boolean;
  }

  let monitor = $state<MonitorStatus | null>(null);

  interface Settings {
    cpuPct: number;
    cpuSustainS: number;
    commitPct: number;
    pageInput: number;
    pageSustainS: number;
    memSustainS: number;
    diskTransferMs: number;
    diskSustainS: number;
    interruptPct: number;
    interruptSustainS: number;
    stutterGapMs: number;
    dpcTotalThreshold: number;
    cooldownS: number;
    retainRows: number;
    closeToTray: boolean;
    autostart: boolean;
    enableFrameMonitor: boolean;
    enableDpcWatch: boolean;
  }

  let s = $state<Settings | null>(null);
  let saving = $state(false);
  let saved = $state(false);
  let errMsg = $state("");

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let savedTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => {
    // 轮询监测器实际运行状态(5s)
    const poll = async () => {
      try {
        monitor = await invoke<MonitorStatus>("get_monitor_status");
      } catch {
        /* 忽略瞬时失败 */
      }
    };

    void (async () => {
      try {
        s = await invoke<Settings>("get_settings");
      } catch (e) {
        errMsg = String(e);
      }
      if (!pollTimer) {
        await poll();
        pollTimer = setInterval(poll, 5000);
      }
    })();

    // 面板按 tab 挂载/卸载;不撤定时器的话每切一次就多一套轮询
    return () => {
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
      if (savedTimer) {
        clearTimeout(savedTimer);
        savedTimer = null;
      }
    };
  });

  async function save() {
    if (!s) return;
    saving = true;
    saved = false;
    errMsg = "";
    try {
      await invoke("set_settings", { settings: s });
      saved = true;
      if (savedTimer) clearTimeout(savedTimer);
      savedTimer = setTimeout(() => {
        saved = false;
        savedTimer = null;
      }, 2000);
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
        <span>硬缺页持续秒数</span>
        <input type="number" min="2" max="60" bind:value={s.pageSustainS} />
      </label>
      <label>
        <span>内存占用持续秒数</span>
        <input type="number" min="2" max="60" bind:value={s.memSustainS} />
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
        <span>中断持续秒数</span>
        <input type="number" min="2" max="60" bind:value={s.interruptSustainS} />
      </label>
      <label>
        <span>帧间隔尖峰 ms</span>
        <input type="number" min="20" max="1000" bind:value={s.stutterGapMs} />
      </label>
      <label>
        <span>DPC 每秒阈值</span>
        <input
          type="number"
          min="1000"
          max="100000"
          step="500"
          bind:value={s.dpcTotalThreshold}
        />
      </label>
      <label>
        <span>同类事件冷却 秒</span>
        <input type="number" min="10" max="600" bind:value={s.cooldownS} />
      </label>
      <label>
        <span>事件库保留条数</span>
        <input
          type="number"
          min="50"
          max="20000"
          step="50"
          bind:value={s.retainRows}
        />
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
      <label class="check">
        <input type="checkbox" bind:checked={s.enableFrameMonitor} />
        <span>游戏帧监测(PresentMon,检测帧率骤降与卡帧)</span>
        {#if monitor?.frameEnabled}
          <span class="state-badge" class:on={monitor.frameActive}>
            {monitor.frameActive ? "● 运行中" : "○ 已停止"}
          </span>
        {/if}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.enableDpcWatch} />
        <span>DPC 驱动归因(ETW 内核跟踪,需管理员权限)</span>
        {#if monitor?.dpcEnabled}
          <span class="state-badge" class:on={monitor.dpcActive}>
            {monitor.dpcActive ? "● 运行中" : "○ 已停止(需管理员)"}
          </span>
        {/if}
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
    flex: 1 0 480px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-2xl);
    padding: 14px 18px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-14);
  }
  .head {
    font-size: 14px;
    font-weight: 600;
  }
  .group {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: 520px;
  }
  .gtitle {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-dim);
  }
  label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 13px;
  }
  label.check {
    justify-content: flex-start;
    gap: var(--space-10);
    cursor: pointer;
  }
  input[type="number"] {
    width: 110px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: var(--radius-lg);
    padding: 5px 8px;
    font-size: 13px;
    outline: none;
  }
  input[type="number"]:focus {
    border-color: var(--accent);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-12);
  }
  .save {
    background: var(--accent);
    border: none;
    color: var(--on-accent);
    border-radius: var(--radius-xl);
    padding: 7px 22px;
    font-size: 13px;
    cursor: pointer;
  }
  .save:hover {
    background: var(--accent-hover);
  }
  .save:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .ok {
    color: var(--ok);
    font-size: 12.5px;
  }
  .err {
    color: var(--err);
    font-size: 12.5px;
  }
  .state-badge {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-dim);
  }
  .state-badge.on {
    color: var(--ok);
  }
  .loading {
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
