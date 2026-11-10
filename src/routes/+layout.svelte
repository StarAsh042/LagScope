<script lang="ts">
  import "$lib/app.css";

  import Toasts from "$lib/components/Toasts.svelte";

  let { children } = $props();
</script>

{@render children?.()}

<!-- 全局提示挂在布局层:主窗口与迷你窗共用一份队列,切换页面不会丢 -->
<Toasts />
