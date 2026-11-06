<script lang="ts">
  import { t } from "../lib/i18n/index.svelte";
  // `value` va de 0 a 1. Sin `value`, el anillo es indeterminado y gira.
  let {
    size = 16,
    value,
    label,
  }: { size?: 12 | 16 | 20; value?: number; label?: string } = $props();

  // Radio y circunferencia en unidades del viewBox (16 × 16).
  const R = 6.25;
  const C = 2 * Math.PI * R;

  const indeterminate = $derived(value === undefined || Number.isNaN(value));
  const fraction = $derived(indeterminate ? 0.28 : Math.min(1, Math.max(0, value ?? 0)));
  const percent = $derived(Math.round(fraction * 100));
</script>

<span
  class="ring"
  class:indeterminate
  style:--size="{size}px"
  role="progressbar"
  aria-label={label ?? t("ui.progress.label")}
  aria-valuemin={indeterminate ? undefined : 0}
  aria-valuemax={indeterminate ? undefined : 100}
  aria-valuenow={indeterminate ? undefined : percent}
>
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <circle class="track" cx="8" cy="8" r={R} />
    <circle
      class="arc"
      cx="8"
      cy="8"
      r={R}
      stroke-dasharray="{C * fraction} {C}"
      transform="rotate(-90 8 8)"
    />
  </svg>
</span>

<style>
  .ring {
    display: inline-flex;
    width: var(--size);
    height: var(--size);
    flex-shrink: 0;
  }

  svg {
    width: 100%;
    height: 100%;
  }

  circle {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
  }

  .track {
    opacity: 0.2;
  }

  .arc {
    transition: stroke-dasharray var(--d-2) var(--ease);
  }

  .indeterminate svg {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* Con reducir movimiento no gira: el arco fijo sigue diciendo «en curso». */
  @media (prefers-reduced-motion: reduce) {
    .indeterminate svg {
      animation: none;
    }
  }
</style>
