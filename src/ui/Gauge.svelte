<script lang="ts">
  import { Tween, prefersReducedMotion } from "svelte/motion";
  import { cubicOut } from "svelte/easing";

  // Anillo de 120 px. El color del arco es `currentColor` (por defecto el acento);
  // sobre el héroe se pone `color: var(--on-hero)` en el contenedor.
  let {
    value,
    max,
    label,
    caption,
  }: {
    value: number;
    max: number;
    /** Texto central, por ejemplo «412 MB». */
    label: string;
    /** Línea pequeña bajo el texto central, por ejemplo «RAM». */
    caption?: string;
  } = $props();

  const SIZE = 120;
  const STROKE = 10;
  const R = (SIZE - STROKE) / 2;
  const C = 2 * Math.PI * R;

  const fraction = $derived(max > 0 ? Math.min(1, Math.max(0, value / max)) : 0);

  const shown = new Tween(0, { duration: 340, easing: cubicOut });

  $effect(() => {
    void shown.set(fraction, { duration: prefersReducedMotion.current ? 0 : 340 });
  });
</script>

<div
  class="gauge"
  role="meter"
  aria-label={caption ? `${caption}: ${label}` : label}
  aria-valuemin={0}
  aria-valuemax={max}
  aria-valuenow={Math.min(Math.max(0, value), max)}
  aria-valuetext={label}
>
  <svg viewBox="0 0 {SIZE} {SIZE}" aria-hidden="true">
    <circle class="track" cx={SIZE / 2} cy={SIZE / 2} r={R} stroke-width={STROKE} />
    {#if shown.current > 0}
      <circle
        class="arc"
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={R}
        stroke-width={STROKE}
        stroke-dasharray="{C * shown.current} {C}"
        transform="rotate(-90 {SIZE / 2} {SIZE / 2})"
      />
    {/if}
  </svg>
  <span class="center" aria-hidden="true">
    <span class="value num">{label}</span>
    {#if caption}<span class="caption">{caption}</span>{/if}
  </span>
</div>

<style>
  .gauge {
    position: relative;
    display: inline-flex;
    width: 120px;
    height: 120px;
    flex-shrink: 0;
    color: var(--gauge-color, var(--accent));
  }

  svg {
    width: 100%;
    height: 100%;
  }

  circle {
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
  }

  .track {
    opacity: 0.18;
  }

  .center {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    text-align: center;
  }

  .value {
    font: var(--t-title);
    font-variant-numeric: tabular-nums;
    color: var(--gauge-text, var(--label));
  }

  .caption {
    font: var(--t-caption);
    color: var(--gauge-text, var(--label-2));
    opacity: 0.85;
  }
</style>
