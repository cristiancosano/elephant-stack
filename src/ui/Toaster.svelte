<script lang="ts">
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { prefersReducedMotion } from "svelte/motion";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import { dismiss, hold, release, toasts, type ToastTone } from "./toasts.svelte";

  // Un solo Toaster en la app (App.svelte). Los avisos se piden con `toast()`.
  const ICONS: Record<ToastTone, IconName> = { info: "info", success: "check", error: "warning" };

  const motion = $derived({
    y: prefersReducedMotion.current ? 0 : 8,
    duration: 200,
    easing: cubicOut,
  });
</script>

<div class="toaster">
  {#each toasts as t (t.id)}
    <div
      class="toast {t.tone}"
      role={t.tone === "error" ? "alert" : "status"}
      transition:fly={motion}
      onpointerenter={() => hold(t.id)}
      onpointerleave={() => release(t.id)}
      onfocusin={() => hold(t.id)}
      onfocusout={() => release(t.id)}
    >
      <span class="glyph"><Icon name={ICONS[t.tone]} /></span>
      <span class="message">{t.message}</span>
      {#if t.action}
        <button
          type="button"
          class="action"
          onclick={() => {
            t.action?.onclick();
            dismiss(t.id);
          }}>{t.action.label}</button
        >
      {/if}
      {#if t.tone === "error"}
        <button type="button" class="close" aria-label="Cerrar aviso" title="Cerrar aviso" onclick={() => dismiss(t.id)}>
          <Icon name="x" />
        </button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed;
    z-index: 100;
    left: 50%;
    bottom: var(--s-6);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-2);
    transform: translateX(-50%);
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    max-width: min(480px, calc(100vw - 2 * var(--s-6)));
    min-height: 32px;
    padding: var(--s-1) var(--s-2) var(--s-1) var(--s-3);
    border-radius: var(--r-md);
    background: var(--hud);
    color: var(--on-hud);
    box-shadow: var(--shadow-pop);
    -webkit-backdrop-filter: blur(20px) saturate(1.6);
    backdrop-filter: blur(20px) saturate(1.6);
    font: var(--t-body);
    pointer-events: auto;
  }

  .glyph {
    display: inline-flex;
  }

  .success .glyph {
    color: var(--green);
  }

  .error .glyph {
    color: var(--orange);
  }

  .message {
    padding: var(--s-1) 0;
  }

  .action,
  .close {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    height: var(--control-height);
    padding: 0 var(--s-2);
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--on-hud);
    font: var(--t-headline);
  }

  .close {
    padding: 0 var(--s-1);
    opacity: 0.7;
  }

  .action:active,
  .close:active {
    background: color-mix(in srgb, var(--on-hud) 14%, transparent);
  }
</style>
