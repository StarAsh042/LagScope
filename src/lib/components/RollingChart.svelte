<script lang="ts">
  // uPlot 滚动时间序列图:数据为 (tsMs, y1..yn) 元组数组
  import { untrack } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";

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
        stroke: "#5a647a",
        grid: { stroke: "#1d2434" },
        ticks: { stroke: "#1d2434" },
        values: timeAxisValues,
      },
      {
        stroke: "#5a647a",
        grid: { stroke: "#1d2434" },
        ticks: { stroke: "#1d2434" },
        label: unit || undefined,
      },
    ];
    return {
      width,
      height: 140,
      legend: { show: true, live: false },
      cursor: { show: false },
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
    if (chart && width > 0) chart.setSize({ width, height: 140 });
  });

  // 数据变化:复用同一实例做增量替换,取消不必要的全量重绘
  $effect(() => {
    if (chart && data.length) {
      chart.setData(data as unknown as uPlot.AlignedData);
    }
  });
</script>

<div class="chart-card">
  <div class="title">{title}</div>
  <div class="plot" bind:this={el}></div>
</div>

<style>
  .chart-card {
    background: var(--panel, #131926);
    border: 1px solid var(--border, #232a3a);
    border-radius: 10px;
    padding: 10px 12px 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim, #8b94a7);
  }
  .plot {
    width: 100%;
  }
</style>
