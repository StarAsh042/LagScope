<script lang="ts">
  // uPlot 滚动时间序列图:数据为 (tsMs, y1..yn) 元组数组
  import { untrack } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import { token } from "$lib/theme";

  let {
    title,
    data, // [tsMs, v1, v2, ...][]
    series, // { label, color, scale } 列表
    yMax = null, // 固定 y 上限(如 100);null 为自适应
    unit = "",
  }: {
    title: string;
    data: number[][];
    series: { label: string; color: string; scale?: string }[];
    yMax?: number | null;
    unit?: string;
  } = $props();

  const HEIGHT = 168;

  let el: HTMLDivElement;
  let chart: uPlot | null = null;
  // 必须是响应式状态:ResizeObserver 回调里改它,若只是普通 let,
  // 下面负责重建图表的 $effect 永远不会被重新触发,窗口拉伸后图表
  // 会一直停在初始的 300px 宽度。
  let width = $state(300);

  function timeAxisValues(
    _u: uPlot,
    ticks: number[]
  ): (string | number | null)[] {
    return ticks.map((t) => {
      const d = new Date(t * 1000);
      return (
        String(d.getHours()).padStart(2, "0") +
        ":" +
        String(d.getMinutes()).padStart(2, "0") +
        ":" +
        String(d.getSeconds()).padStart(2, "0")
      );
    });
  }

  function opts(): uPlot.Options {
    const axes: uPlot.Axis[] = [
      {
        stroke: token("--chart-axis", "#5a647a"),
        grid: { stroke: token("--chart-grid", "#1d2434") },
        ticks: { stroke: token("--chart-grid", "#1d2434") },
        values: timeAxisValues,
      },
      {
        stroke: token("--chart-axis", "#5a647a"),
        grid: { stroke: token("--chart-grid", "#1d2434") },
        ticks: { stroke: token("--chart-grid", "#1d2434") },
        label: unit || undefined,
      },
    ];
    return {
      width,
      height: HEIGHT,
      // live: true 才会随光标刷新图例里的数值(uPlot 默认给 .u-value 加了
      // 仅 live 时可见的样式;false 时数值列会被 CSS 直接隐藏)。
      legend: { show: true, live: true },
      // show: true 才会绑定鼠标事件 —— 悬浮读数、拖拽框选缩放、
      // 双击还原时间轴全部依赖它;关掉则一个事件都不注册。
      cursor: { show: true, x: true, y: false },
      scales: yMax != null ? { y: { range: [0, yMax] } } : undefined,
      axes,
      series: [
        {},
        ...series.map((s) => ({
          label: s.label,
          stroke: s.color,
          width: 1.5,
          scale: s.scale ?? "y",
          points: { show: false },
        })),
      ],
    };
  }

  // 只在挂载时建一次图表,避免每次宽度变化都销毁重建(会丢动画与光标状态)。
  // 用 untrack 读 data/配置:否则每秒一条新采样都会把建图表这段再跑一遍
  // (虽然 chart 非空会提前返回,但白白唤醒一次 effect)。
  $effect(() => {
    if (!el || chart) return;
    const initial = untrack(() => ({ o: opts(), d: data as unknown as uPlot.AlignedData }));
    chart = new uPlot(initial.o, initial.d, el);
    const ro = new ResizeObserver((entries) => {
      const w = Math.max(200, Math.floor(entries[0].contentRect.width));
      if (w !== width) width = w;
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      chart?.destroy();
      chart = null;
    };
  });

  // 宽度变化:交给 uPlot 自己 resize,不重建实例
  $effect(() => {
    if (chart && width > 0) chart.setSize({ width, height: HEIGHT });
  });

  // 数据变化:每来一条新采样都要重绘,但不能把用户拖出来的时间范围顶掉。
  // setData(data, false) 只换数据不动 x 轴,可是它同时会跳过内部的 commit(),
  // 画面不会刷新,所以要再补一次 redraw(false) —— 那条路径只走 commit(),
  // 不碰 setScale。
  $effect(() => {
    if (chart && data.length) {
      chart.setData(data as unknown as uPlot.AlignedData, false);
      chart.redraw(false);
    }
  });
</script>

<div class="chart-card">
  <div class="title">{title}</div>
  <div class="plot" bind:this={el}></div>
</div>

<style>
  .chart-card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-2xl);
    padding: 10px 12px 6px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }
  .title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
  }
  .plot {
    width: 100%;
  }

  /* uPlot 自带样式是按浅色主题写的,这里覆盖成深色面板上的观感 */
  .plot :global(.u-legend) {
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-dim);
    margin: 0;
    text-align: left;
  }
  .plot :global(.u-legend .u-marker) {
    width: 8px;
    height: 8px;
    margin-right: 4px;
  }
  .plot :global(.u-legend .u-series > *) {
    padding: 0 8px 0 0;
  }
  .plot :global(.u-legend .u-value) {
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .plot :global(.u-cursor-x) {
    border-right: 1px dashed rgba(59, 108, 212, 0.85);
  }
  .plot :global(.u-select) {
    background: rgba(59, 108, 212, 0.16);
    border-left: 1px solid rgba(59, 108, 212, 0.5);
    border-right: 1px solid rgba(59, 108, 212, 0.5);
  }
  .plot :global(.u-cursor-pt) {
    border-width: 0;
  }
</style>
