<script lang="ts">
  import type { Snippet } from "svelte";
  import ProgressRing from "./ProgressRing.svelte";

  let {
    variant = "secondary",
    size = "regular",
    busy = false,
    disabled = false,
    onclick,
    children,
  }: {
    variant?: "primary" | "secondary" | "plain" | "destructive";
    size?: "regular" | "large";
    busy?: boolean;
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button
  type="button"
  class="button {variant} {size}"
  class:busy
  disabled={disabled || busy}
  aria-busy={busy || undefined}
  {onclick}
>
  <!-- El texto se oculta pero sigue ocupando su sitio: el botón no cambia de ancho. -->
  <span class="content">{@render children()}</span>
  {#if busy}
    <span class="spinner"><ProgressRing size={12} /></span>
  {/if}
</button>

<style>
  .button {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    height: var(--control-height);
    padding: 0 var(--s-3);
    border: none;
    border-radius: var(--r-sm);
    font: var(--t-body);
    white-space: nowrap;
    transition:
      background-color var(--d-1) var(--ease),
      opacity var(--d-1) var(--ease);
  }

  .large {
    height: var(--control-height-large);
    padding: 0 var(--s-4);
  }

  .content {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
  }

  .busy .content {
    visibility: hidden;
  }

  .spinner {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .primary {
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: var(--shadow-1);
  }

  .primary:active:not(:disabled) {
    filter: brightness(0.9);
  }

  .secondary {
    background: var(--raised);
    color: var(--label);
    box-shadow: var(--shadow-1);
  }

  .secondary:active:not(:disabled) {
    background: var(--fill-3);
  }

  .plain {
    background: transparent;
    color: var(--accent);
  }

  .plain:active:not(:disabled) {
    background: var(--fill-2);
  }

  /* Rojo sólido: con texto rojo sobre el gris de los botones, el contraste en
     oscuro no llega a AA. */
  .destructive {
    background: var(--red);
    color: var(--on-hero);
    box-shadow: var(--shadow-1);
  }

  .destructive:active:not(:disabled) {
    filter: brightness(0.9);
  }

  .button:disabled {
    opacity: 0.5;
  }

  /* Mientras trabaja no se atenúa: el anillo ya dice que está ocupado. */
  .button.busy:disabled {
    opacity: 1;
  }
</style>
