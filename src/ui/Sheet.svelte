<script lang="ts">
  import type { Snippet } from "svelte";

  // Hoja modal que baja desde la barra superior, como las de macOS.
  // Usa <dialog> con showModal(): el navegador atrapa el foco, deja el resto inerte
  // y devuelve el foco al cerrar. Esc llega como `cancel` y lo decide `onclose`.
  let {
    open,
    title,
    onclose,
    children,
  }: { open: boolean; title: string; onclose: () => void; children: Snippet } = $props();

  let dialog: HTMLDialogElement | undefined = $state();
  const titleId = $props.id();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });

  function oncancel(event: Event) {
    event.preventDefault();
    onclose();
  }
</script>

<dialog bind:this={dialog} class="sheet" aria-labelledby={titleId} {oncancel}>
  {#if open}
    <h2 class="title" id={titleId}>{title}</h2>
    <div class="body">{@render children()}</div>
  {/if}
</dialog>

<style>
  .sheet {
    width: min(480px, calc(100vw - 2 * var(--s-6)));
    max-height: calc(100vh - var(--toolbar-height) - var(--s-6));
    margin: var(--toolbar-height) auto auto;
    padding: var(--s-5) var(--s-6) var(--s-6);
    overflow: auto;
    border: none;
    border-radius: 0 0 var(--r-lg) var(--r-lg);
    background: var(--group);
    color: var(--label);
    box-shadow: var(--shadow-pop);
  }

  .sheet[open] {
    animation: drop var(--d-3) var(--ease);
  }

  .sheet::backdrop {
    background: var(--backdrop);
    animation: fade var(--d-3) var(--ease);
  }

  .title {
    margin-bottom: var(--s-3);
    font: var(--t-headline);
  }

  .body {
    font: var(--t-body);
  }

  @keyframes drop {
    from {
      transform: translateY(-100%);
    }
    to {
      transform: translateY(0);
    }
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
