<script lang="ts">
  // 线性图标集:统一 24×24 视口、1.7px 描边、currentColor 取色。
  // 只做工具界面需要的那一小组,避免引入整套图标库。
  export type IconName =
    | "pulse" | "dashboard" | "events" | "startup" | "logs"
    | "settings" | "close" | "mini" | "down" | "up";

  let {
    name,
    size = 16,
    class: className = "",
  }: { name: IconName; size?: number; class?: string } = $props();

  const PATHS: Record<IconName, string> = {
    // 心电图:品牌记忆点,取自「卡顿透视 / 示波器」的视觉语言
    pulse: "M2 12h4l2.5-7 4 14 2.5-7h7",
    dashboard: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
    events: "M12 3 2 20h20L12 3zM12 9v5M12 17h.01",
    startup: "M6 17l-2 2c0 3 4 4 8 4s8-1 8-4l-2-2M12 3v11M8 6a6 6 0 0 1 8 0",
    logs: "M6 3h9l4 4v14H6zM14 3v5h5M9 12h7M9 16h7",
    settings: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2",
    close: "M5 5l14 14M19 5L5 19",
    mini: "M4 5h16v14H4zM12 11h8v8h-8z",
    down: "M12 4v15M6 13l6 6 6-6",
    up: "M12 20V5M6 11l6-6 6 6",
  };
</script>

<svg
  class={className}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={PATHS[name]} />
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
