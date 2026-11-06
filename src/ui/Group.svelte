<script lang="ts">
  import type { Snippet } from "svelte";

  // Lista agrupada de Ajustes del Sistema: cabecera opcional, filas y pie.
  let {
    title,
    footer,
    children,
  }: { title?: string; footer?: string; children: Snippet } = $props();

  const id = $props.id();
</script>

<section class="group" aria-labelledby={title ? id : undefined}>
  {#if title}<h2 class="title" {id}>{title}</h2>{/if}
  <div class="rows" role="list">
    {@render children()}
  </div>
  {#if footer}<p class="footer">{footer}</p>{/if}
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .group + :global(.group) {
    margin-top: var(--group-gap);
  }

  .title {
    padding: 0 var(--s-3);
    font: var(--t-headline);
    color: var(--label);
  }

  .rows {
    overflow: hidden;
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
  }

  .footer {
    padding: 0 var(--s-3);
    font: var(--t-callout);
    color: var(--label-2);
  }
</style>
