<script lang="ts">
  /**
   * 空态 / 加载态 / 错误态 / 成功提示的统一呈现。
   *
   * 这几种状态原本在各组件里各写一套 .empty / .loading / .err / .note,结果同一个
   * "出错了"在事件详情里是有底色的框,在进程表里是一行红字。统一到这里之后,错误与
   * 成功一律带底框(它们在密集界面里更容易被看见),空态与加载态一律居中灰字。
   */
  type Kind = "empty" | "loading" | "error" | "success";

  let {
    kind,
    message,
    hint = "",
    actionLabel = "",
    onAction,
    fill = false,
  }: {
    kind: Kind;
    /** 主文案,一句话说清当前状态 */
    message: string;
    /** 补充说明,如"需要管理员权限"的操作建议 */
    hint?: string;
    /** 可选操作按钮的文案;必须同时给 onAction 才会渲染 */
    actionLabel?: string;
    onAction?: () => void;
    /** 撑满父容器并居中(用于整块区域被状态占据的场景) */
    fill?: boolean;
  } = $props();
</script>

<div
  class="status kind-{kind}"
  class:fill
  role={kind === "error" ? "alert" : undefined}
>
  <span class="msg">{message}</span>
  {#if hint}
    <span class="hint">{hint}</span>
  {/if}
  {#if actionLabel && onAction}
    <button class="link" onclick={onAction}>{actionLabel}</button>
  {/if}
</div>

<style>
  .status {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    color: var(--text-dim);
    font-size: var(--font-sm);
  }
  .status.fill {
    flex: 1;
    align-items: center;
    justify-content: center;
    padding: var(--space-12);
    text-align: center;
  }
  .kind-empty {
    padding: 24px 12px;
    text-align: center;
  }
  .kind-loading {
    padding: var(--space-16) 0;
    text-align: center;
  }
  .kind-error,
  .kind-success {
    align-items: flex-start;
    padding: 6px 9px;
    border-radius: var(--radius-lg);
    /* 导出路径、异常文本可能很长且不带空格,靠 anywhere 让它在框内折行 */
    overflow-wrap: anywhere;
  }
  .kind-error {
    color: var(--err);
    background: var(--err-bg);
    border: 1px solid var(--err-border);
  }
  .kind-success {
    color: var(--ok);
    background: var(--bg);
    border: 1px solid var(--ok-border);
  }
  .hint {
    color: var(--warn);
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font: inherit;
    font-size: var(--font-xs);
    cursor: pointer;
    text-decoration: underline;
  }
</style>
