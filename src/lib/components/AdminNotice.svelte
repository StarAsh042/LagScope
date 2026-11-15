<script lang="ts">
  /**
   * 管理员权限的统一入口。
   *
   * 设置页与启动项面板都有「这事需要管理员」的地方:各自写一遍提示,文案和
   * 重启用词就会漂移。统一到这里之后,「以管理员身份重启」在全应用只有一处实现。
   */
  import {
    ADMIN_ELEVATED_NOTE,
    ADMIN_HINT,
    ADMIN_LIMITED_NOTE,
    relaunchAsAdmin,
  } from "$lib/admin";
  import { errorText } from "$lib/errors";

  let {
    elevated,
    reason = "",
  }: {
    /** null 表示还在读取权限状态 */
    elevated: boolean | null;
    /** 具体哪一项因为权限失败了,如内核会话创建失败的原因 */
    reason?: string;
  } = $props();

  let relaunching = $state(false);
  let launchErr = $state("");

  async function relaunch() {
    relaunching = true;
    launchErr = "";
    try {
      // 成功时 Rust 侧会先拉起提权的新进程再退出自己,这里通常收不到回包;
      // 能拿到 false 说明用户在 UAC 弹窗上点了「否」。
      const started = await relaunchAsAdmin();
      if (!started) launchErr = "已取消提权,权限未变更。";
    } catch (e) {
      launchErr = errorText(e);
    } finally {
      relaunching = false;
    }
  }
</script>

<div class="admin">
  {#if elevated === null}
    <span class="state">读取运行权限…</span>
  {:else}
    <span class="state" class:on={elevated}>
      {elevated ? "● 已以管理员身份运行" : "○ 以普通权限运行"}
    </span>
    <span class="note">{elevated ? ADMIN_ELEVATED_NOTE : ADMIN_LIMITED_NOTE}</span>
    {#if reason}
      <span class="reason">{reason}</span>
    {/if}
    {#if !elevated}
      <span class="hint">{ADMIN_HINT}</span>
      <button class="relaunch" disabled={relaunching} onclick={relaunch}>
        {relaunching ? "正在提权…" : "以管理员身份重启"}
      </button>
    {/if}
  {/if}
  {#if launchErr}
    <span class="err">{launchErr}</span>
  {/if}
</div>

<style>
  .admin {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    font-size: var(--font-sm);
    color: var(--text-dim);
  }
  .state.on {
    color: var(--ok);
  }
  .note {
    color: var(--text-dim);
  }
  .reason {
    /* 具体失败原因比通用说明更值得被看见,但仍属引导而非报错 */
    color: var(--warn);
  }
  .hint {
    color: var(--text-dim);
    font-size: var(--font-xs);
  }
  .err {
    color: var(--err);
    font-size: var(--font-xs);
  }
  .relaunch {
    align-self: flex-start;
    background: var(--accent);
    border: none;
    color: var(--on-accent);
    border-radius: var(--radius-lg);
    padding: 5px 14px;
    font-size: var(--font-sm);
    cursor: pointer;
  }
  .relaunch:hover {
    background: var(--accent-hover);
  }
  .relaunch:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
