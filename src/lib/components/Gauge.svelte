<script lang="ts">
  import { levelColor } from "$lib/theme";

  // 环形仪表:0-100%,或传入 rawText 显示自定义数值(如 FPS)
  let {
    label,
    value, // 0-100 或 null
    sub = "", // 副标题
    rawText = undefined, // 自定义中央文本(如 "60")
    colorOverride = undefined, // 覆盖弧线颜色(如温度分级)
  }: {
    label: string;
    value: number | null;
    sub?: string;
    rawText?: string;
    colorOverride?: string;
  } = $props();

  const R = 34;
  const CIRC = 2 * Math.PI * R;

  const pct = $derived(value == null ? 0 : Math.min(100, Math.max(0, value)));
  const color = $derived(colorOverride ?? levelColor(pct));
</script>

<div class="gauge">
  <svg viewBox="0 0 90 90">
    <!-- 只旋转圆环(进度弧从顶部起),文字保持水平 -->
    <g transform="rotate(-90 45 45)">
      <circle class="track" cx="45" cy="45" r={R} />
      <circle
        class="bar"
        cx="45"
        cy="45"
        r={R}
        stroke={color}
        stroke-dasharray="{(pct / 100) * CIRC} {CIRC}"
      />
    </g>
    <text x="45" y="43" class="num">{rawText ?? (value == null ? "--" : Math.round(pct))}</text>
    <text x="45" y="56" class="unit">{rawText != null || value == null ? "" : "%"}</text>
  </svg>
  <div class="meta">
    <span class="label">{label}</span>
    <span class="sub">{sub}</span>
  </div>
</div>

<style>
  .gauge {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-4);
    /* 固定宽度:副标题文字变化时不会引起整排表盘左右抖动 */
    width: 118px;
    flex: none;
  }
  svg {
    width: 84px;
    height: 84px;
  }
  circle {
    fill: none;
    stroke-width: 8;
    stroke-linecap: round;
  }
  .track {
    stroke: var(--track);
  }
  .bar {
    transition: stroke-dasharray 0.6s ease, stroke 0.4s ease;
  }
  text {
    fill: var(--text);
    text-anchor: middle;
    dominant-baseline: middle;
  }
  .num {
    font-size: var(--font-xl);
    font-weight: var(--weight-bold);
  }
  .unit {
    font-size: var(--font-2xs);
    fill: var(--text-dim);
  }
  .meta {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .label {
    font-size: var(--font-sm);
    color: var(--text);
    font-weight: var(--weight-medium);
  }
  .sub {
    font-size: var(--font-xs);
    color: var(--text-dim);
    white-space: nowrap;
    max-width: 112px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
