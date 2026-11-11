<script lang="ts">
  // 全局提示的展示层:右下角堆叠,点击即关,到点自动消失。
  import { dismiss, toasts } from "$lib/toast.svelte";
</script>

{#if toasts.list.length}
  <div class="toasts" role="status" aria-live="polite">
    {#each toasts.list as t (t.id)}
      <div class="toast" class:error={t.level === "error"} class:warn={t.level === "warn"}>
        <span class="msg">{t.message}</span>
        <button class="close" title="关闭" aria-label="关闭" onclick={() => dismiss(t.id)}>
          ×
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  /* 固定定位而不是跟随文档流:主窗口内容可滚动,提示要始终看得见 */
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
    max-width: 420px;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--space-8);
    padding: 9px 10px 9px 12px;
    border-radius: var(--radius-xl);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    background: var(--panel);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
    pointer-events: auto;
    font-size: var(--font-sm);
    line-height: 1.5;
  }
  .toast.warn {
    border-left-color: var(--warn);
  }
  .toast.error {
    border-left-color: var(--err);
  }
  .msg {
    flex: 1;
    min-width: 0;
    /* 长路径/错误串不撑破面板 */
    overflow-wrap: anywhere;
  }
  .close {
    flex: none;
    border: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: var(--font-lg);
    line-height: 1;
    padding: 0 2px;
    cursor: pointer;
  }
  .close:hover {
    color: var(--text);
  }
</style>
