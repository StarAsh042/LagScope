<script lang="ts">
  // 渲染期异常的兜底:面板炸了只黑这一块,窗口其余部分照常可用。
  import { untrack } from "svelte";

  import { errorText } from "$lib/errors";
  import { notify } from "$lib/toast.svelte";

  let { label = "此区域", children } = $props();

  /** 兜底界面展示的错误文本;onerror 收到的是原始异常,这里统一格式化 */
  let detail = $state("");

  /**
   * failed 片段的 error/reset 在编译后是取值函数(模板里 {error.message} 会编成 error().message),
   * 但 Svelte 的类型声明把它们写成 unknown / () => void。这里两种形态都接住,
   * 否则类型检查报「'error' is of type 'unknown'」,而直接当对象用运行时又拿不到值。
   */
  function unwrapError(value: unknown): unknown {
    return typeof value === "function" ? (value as () => unknown)() : value;
  }

  function handleError(err: unknown) {
    // untrack:同一个异常可能触发多次渲染,提示只报一次由 notify 去重
    untrack(() => {
      detail = errorText(err);
      notify(label + "出错了: " + detail, "error");
    });
  }
</script>

<svelte:boundary onerror={handleError}>
  {@render children?.()}

  {#snippet failed(error, reset)}
    <!-- error 与 reset 在片段里是取值函数,必须写成 error() / reset() -->
    <div class="fallback" role="alert">
      <div class="title">{label}出错</div>
      <div class="detail">{detail || errorText(unwrapError(error))}</div>
      <div class="acts">
        <button onclick={reset}>重试</button>
        <button class="ghost" onclick={() => location.reload()}>重新加载</button>
      </div>
    </div>
  {/snippet}
</svelte:boundary>

<style>
  .fallback {
    margin: 12px 0;
    padding: 14px 16px;
    border: 1px solid var(--err-border-muted);
    border-radius: var(--radius-2xl);
    background: var(--err-bg-deep);
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: var(--err);
  }
  .detail {
    font-size: 12px;
    color: var(--text-dim);
    overflow-wrap: anywhere;
  }
  .acts {
    display: flex;
    gap: var(--space-8);
  }
  button {
    font-size: 12px;
    padding: 4px 12px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--warn-border-soft);
    background: var(--warn-bg-soft);
    color: var(--warn-strong);
    cursor: pointer;
  }
  button:hover {
    background: var(--warn-bg-hover);
  }
  button.ghost {
    border-color: var(--border);
    background: transparent;
    color: var(--text-dim);
  }
  button.ghost:hover {
    color: var(--text);
  }
</style>
