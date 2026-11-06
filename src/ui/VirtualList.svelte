<script lang="ts">
  import type { Snippet } from "svelte";
  import { isAtBottom, isNearStart, maxScrollTop, scrollTopForIndex, visibleWindow } from "../lib/virtual";

  // Lista virtual de filas de altura fija (01 §7.5). Pinta solo lo visible más 20
  // filas de margen, cada una con `transform: translateY` dentro de un contenedor
  // del alto total. Los datos y la selección son de quien la usa: `row(i)` pinta la
  // fila i (con sus `role="gridcell"`).
  //
  // Seguir la cola: con `follow`, cada fila nueva lleva el scroll abajo. Al subir,
  // avisa con `onfollowchange(false)`; al volver abajo del todo, con `true`.
  let {
    count,
    rowHeight,
    row,
    follow = false,
    onnearstart,
    onfollowchange,
    label,
  }: {
    count: number;
    rowHeight: number;
    row: Snippet<[number]>;
    follow?: boolean;
    /** Al acercarse al principio (para pedir más historial). Una vez por acercamiento. */
    onnearstart?: () => void;
    onfollowchange?: (follow: boolean) => void;
    label: string;
  } = $props();

  let viewport: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let wasNearStart = true;

  const win = $derived(visibleWindow({ scrollTop, viewportHeight, rowHeight, count }));
  const indices = $derived(Array.from({ length: win.last - win.first }, (_, k) => win.first + k));

  function setScroll(top: number) {
    if (!viewport) return;
    const clamped = Math.min(Math.max(0, top), maxScrollTop(count, rowHeight, viewportHeight));
    viewport.scrollTop = clamped;
    scrollTop = clamped;
  }

  // Con `follow`, el final se mantiene a la vista cuando llegan filas o cambia el alto.
  $effect(() => {
    if (!follow) return;
    void count;
    void viewportHeight;
    setScroll(maxScrollTop(count, rowHeight, viewportHeight));
  });

  function onscroll() {
    if (!viewport) return;
    scrollTop = viewport.scrollTop;

    const atBottom = isAtBottom(scrollTop, viewportHeight, viewport.scrollHeight);
    if (follow && !atBottom) onfollowchange?.(false);
    else if (!follow && atBottom && count > 0) onfollowchange?.(true);

    const nearStart = isNearStart(scrollTop, rowHeight);
    if (nearStart && !wasNearStart) onnearstart?.();
    wasNearStart = nearStart;
  }

  /** Lleva la fila `index` a la vista sin mover el scroll si ya se ve. */
  export function scrollToIndex(index: number): void {
    setScroll(scrollTopForIndex(index, rowHeight, viewportHeight, scrollTop));
  }

  export function scrollToEnd(): void {
    setScroll(maxScrollTop(count, rowHeight, viewportHeight));
  }

  /**
   * Compensa filas añadidas (positivo) o quitadas (negativo) **antes** de lo
   * visible, para que lo que se está mirando no se mueva: al cargar historial
   * arriba o al descartar las más antiguas del anillo.
   */
  export function shiftBy(rows: number): void {
    if (!viewport || rows === 0) return;
    const top = scrollTop + rows * rowHeight;
    // El nuevo alto total ya está en el DOM cuando quien llama ha cambiado `count`
    // en el mismo tick; se espera al siguiente frame para no recortar el scroll.
    requestAnimationFrame(() => setScroll(top));
    wasNearStart = isNearStart(top, rowHeight);
  }
</script>

<div
  bind:this={viewport}
  bind:clientHeight={viewportHeight}
  class="viewport"
  role="grid"
  aria-label={label}
  aria-rowcount={count}
  tabindex="0"
  {onscroll}
>
  <div class="spacer" style:height="{win.total}px">
    {#each indices as i (i)}
      <div class="row" role="row" aria-rowindex={i + 1} style:height="{rowHeight}px" style:transform="translateY({i * rowHeight}px)">
        {@render row(i)}
      </div>
    {/each}
  </div>
</div>

<style>
  .viewport {
    position: relative;
    height: 100%;
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    contain: strict;
  }

  .spacer {
    position: relative;
    width: 100%;
  }

  .row {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    overflow: hidden;
    will-change: transform;
  }

  .viewport:focus-visible {
    outline-offset: -3px;
  }
</style>
