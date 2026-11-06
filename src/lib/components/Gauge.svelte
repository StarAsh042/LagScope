<script lang="ts">
  // 环形仪表:0-100%,或传入 rawText 显示自定义数值(如 FPS)
  let {
    label,
    value, // 0-100 或 null
    sub = "", // 副标题
    rawText = undefined, // 自定义中央文本(如 "60")
  }: {
    label: string;
    value: number | null;
    sub?: string;
    rawText?: string;
  } = $props();

  const R = 34;
  const CIRC = 2 * Math.PI * R;

  const pct = $derived(value == null ? 0 : Math.min(100, Math.max(0, value)));
  const color = $derived(
    pct >= 85 ? "#e5484d" : pct >= 60 ? "#f5a623" : "#3dd68c"
  );
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
    gap: 4px;
    min-width: 96px;
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
    stroke: var(--track, #232a3a);
  }
  .bar {
    transition: stroke-dasharray 0.6s ease, stroke 0.4s ease;
  }
  text {
    fill: var(--text, #e8ecf4);
    text-anchor: middle;
    dominant-baseline: middle;
  }
  .num {
    font-size: 18px;
    font-weight: 600;
  }
  .unit {
    font-size: 10px;
    fill: var(--text-dim, #8b94a7);
  }
  .meta {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .label {
    font-size: 12px;
    color: var(--text, #e8ecf4);
    font-weight: 500;
  }
  .sub {
    font-size: 11px;
    color: var(--text-dim, #8b94a7);
    white-space: nowrap;
  }
</style>
