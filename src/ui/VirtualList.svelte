<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import { canLeaveStart, isAtBottom, isNearStart, maxScrollTop, scrollTopForIndex, visibleWindow } from "../lib/virtual";

  // Lista virtual de filas de altura fija (01 §7.5). Pinta solo lo visible más 20
  // filas de margen, cada una con `transform: translateY` dentro de un contenedor
  // del alto total. Los datos y la selección son de quien la usa: `row(i)` pinta la
  // fila i (con sus `role="gridcell"`).
  //
  // Seguir la cola: con `follow`, cada fila nueva lleva el scroll abajo. Al subir,
  // avisa con `onfollowchange(false)`; al volver abajo del todo, con `true`.
  //
  // El foco se queda en la lista; `activeIndex` es la fila seleccionada y se expone
  // con `aria-activedescendant`, para que VoiceOver la lea al moverla con ↑ y ↓.
  // Quien la usa la lleva a la vista (`scrollToIndex`), así que está pintada.
  let {
    count,
    rowHeight,
    row,
    follow = false,
    onnearstart,
    onfollowchange,
    label,
    activeIndex = -1,
  }: {
    count: number;
    rowHeight: number;
    row: Snippet<[number]>;
    follow?: boolean;
    /** Al acercarse al principio (para pedir más historial). Una vez por acercamiento. */
    onnearstart?: () => void;
    onfollowchange?: (follow: boolean) => void;
    label: string;
    /** Fila seleccionada, o -1. */
    activeIndex?: number;
  } = $props();

  const uid = $props.id();
  const rowId = (i: number) => `${uid}-fila-${i}`;

  let viewport: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let wasNearStart = true;

  const win = $derived(visibleWindow({ scrollTop, viewportHeight, rowHeight, count }));
  const indices = $derived(Array.from({ length: win.last - win.first }, (_, k) => win.first + k));
  const activeId = $derived(activeIndex >= 0 && activeIndex >= win.first && activeIndex < win.last ? rowId(activeIndex) : undefined);

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

  // Sin barra de desplazamiento (o con tan poca que nunca se sale de la zona del
  // principio) no llega la transición de `onscroll`: se pide sin esperarla. Se
  // vuelve a mirar cada vez que cambian las filas o el alto; quien escucha evita
  // pedir dos veces.
  $effect(() => {
    if (count === 0 || viewportHeight === 0 || canLeaveStart(count, rowHeight, viewportHeight)) return;
    untrack(() => onnearstart?.());
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
  aria-activedescendant={activeId}
  tabindex="0"
  {onscroll}
>
  <div class="spacer" style:height="{win.total}px">
    {#each indices as i (i)}
      <div
        class="row"
        role="row"
        id={rowId(i)}
        aria-rowindex={i + 1}
        aria-selected={activeIndex >= 0 ? i === activeIndex : undefined}
        style:height="{rowHeight}px" style:transform="translateY({i * rowHeight}px)">
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
