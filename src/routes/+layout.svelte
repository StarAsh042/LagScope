<script lang="ts">
  import "$lib/app.css";
  import { invoke } from "@tauri-apps/api/core";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { Menu } from "@tauri-apps/api/menu";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  import Toasts from "$lib/components/Toasts.svelte";

  let { children } = $props();

  // 悬浮窗用 Tauri 原生菜单替代 Chromium 默认网页菜单；主窗口不需要右键菜单。
  // 输入框内保留系统文本菜单，方便复制粘贴。
  async function onContextMenu(e: MouseEvent) {
    const win = getCurrentWindow();
    if (win.label !== "mini") {
      // 主窗口不需要右键菜单，连 Chromium 默认网页菜单也一并抑制。
      e.preventDefault();
      return;
    }
    const target = e.target instanceof HTMLElement ? e.target : null;
    if (target?.closest('input, textarea, [contenteditable="true"]')) return;
    e.preventDefault();
    try {
      const menu = await Menu.new({
        items: [
          { id: "minimize", text: "最小化", action: () => void win.minimize() },
          { item: "Separator" },
          {
            id: "mini",
            text: "显示 / 隐藏悬浮窗",
            action: () => void invoke("toggle_mini"),
          },
          { item: "Separator" },
          { id: "close", text: "关闭窗口", action: () => void win.close() },
        ],
      });
      await menu.popup(new LogicalPosition(e.clientX, e.clientY));
    } catch (err) {
      console.error("打开右键菜单失败", err);
    }
  }
</script>

<svelte:window oncontextmenu={onContextMenu} />

{@render children?.()}

<!-- 全局提示挂在布局层：主窗口与迷你窗共用一份队列，切换页面不会丢 -->
<Toasts />
