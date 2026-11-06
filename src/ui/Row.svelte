<script lang="ts">
  import type { Snippet } from "svelte";

  // Fila de un Group. Con `onactivate`, la zona del título es un botón que ocupa
  // toda la fila; los controles de `trailing` quedan por encima y siguen siendo
  // independientes (no hay elementos interactivos anidados).
  let {
    title,
    subtitle,
    leading,
    trailing,
    selected = false,
    onactivate,
  }: {
    title: string;
    subtitle?: string;
    leading?: Snippet;
    trailing?: Snippet;
    selected?: boolean;
    onactivate?: () => void;
  } = $props();
</script>

<div class="row" class:with-subtitle={!!subtitle} class:selected role="listitem">
  {#if onactivate}
    <button
      type="button"
      class="hit"
      aria-current={selected ? "true" : undefined}
      aria-label={subtitle ? `${title}, ${subtitle}` : title}
      onclick={onactivate}
    ></button>
  {/if}
  {#if leading}<span class="leading">{@render leading()}</span>{/if}
  <span class="text">
    <span class="title">{title}</span>
    {#if subtitle}<span class="subtitle">{subtitle}</span>{/if}
  </span>
  {#if trailing}<span class="trailing">{@render trailing()}</span>{/if}
</div>

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--s-3);
    min-height: var(--row-height);
    padding: 0 var(--s-3);
  }

  .row.with-subtitle {
    min-height: var(--row-height-subtitle);
  }

  /* Separador de 0,5 px sangrado hasta el texto, como en Ajustes del Sistema. */
  .row + :global(.row)::before {
    content: "";
    position: absolute;
    top: 0;
    left: var(--row-inset, var(--s-3));
    right: 0;
    height: 0.5px;
    background: var(--separator);
  }

  .row:has(.leading) + :global(.row:has(.leading))::before {
    --row-inset: calc(var(--s-3) + 16px + var(--s-3));
  }

  .row.selected {
    background: var(--accent-soft);
  }

  :global(:root[data-window-inactive]) .row.selected {
    background: var(--sidebar-selected);
  }

  .hit {
    position: absolute;
    inset: 0;
    width: 100%;
    padding: 0;
    border: none;
    background: transparent;
  }

  .hit:focus-visible {
    outline-offset: -3px;
    border-radius: var(--r-md);
  }

  .leading {
    position: relative;
    display: inline-flex;
    align-items: center;
    pointer-events: none;
    color: var(--label-2);
  }

  .text {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    gap: 2px;
    padding: var(--s-2) 0;
    pointer-events: none;
  }

  .title {
    overflow: hidden;
    font: var(--t-headline);
    color: var(--label);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subtitle {
    overflow: hidden;
    font: var(--t-callout);
    color: var(--label-2);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trailing {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
    flex-shrink: 0;
  }
</style>
