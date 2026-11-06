<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "../lib/i18n/index.svelte";
  import {
    clampInspectorWidth,
    dragInspectorWidth,
    INSPECTOR_DEFAULT,
    INSPECTOR_MAX,
    INSPECTOR_MIN,
    INSPECTOR_STEP,
  } from "./inspector";

  // Panel derecho plegable. El ancho lo guarda quien lo usa: aquí solo se avisa
  // con `onresize` mientras se arrastra el borde o se mueve con las flechas.
  let {
    open,
    width = INSPECTOR_DEFAULT,
    onresize,
    label,
    children,
  }: {
    open: boolean;
    width?: number;
    onresize?: (width: number) => void;
    label?: string;
    children: Snippet;
  } = $props();

  let dragging = $state(false);
  let dragWidth = $state(INSPECTOR_DEFAULT);
  let start = { x: 0, width: INSPECTOR_DEFAULT };

  const current = $derived(dragging ? dragWidth : clampInspectorWidth(width));

  function onpointerdown(event: PointerEvent & { currentTarget: HTMLElement }) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    start = { x: event.clientX, width: current };
    dragWidth = current;
    dragging = true;
  }

  function onpointermove(event: PointerEvent) {
    if (!dragging) return;
    const next = dragInspectorWidth(start.width, start.x, event.clientX);
    if (next === dragWidth) return;
    dragWidth = next;
    onresize?.(next);
  }

  function onpointerup(event: PointerEvent & { currentTarget: HTMLElement }) {
    if (!dragging) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    dragging = false;
  }

  function onkeydown(event: KeyboardEvent) {
    const step = event.shiftKey ? INSPECTOR_STEP * 3 : INSPECTOR_STEP;
    let next: number;
    if (event.key === "ArrowLeft") next = current + step;
    else if (event.key === "ArrowRight") next = current - step;
    else if (event.key === "Home") next = INSPECTOR_MAX;
    else if (event.key === "End") next = INSPECTOR_MIN;
    else return;
    event.preventDefault();
    onresize?.(clampInspectorWidth(next));
  }
</script>

<aside
  class="inspector"
  class:open
  class:dragging
  style:--width="{current}px"
  aria-label={label ?? t("ui.inspector.label")}
  aria-hidden={!open}
  inert={!open}
>
  <!-- Un separador con foco es un control válido en WAI-ARIA (window splitter),
       pero Svelte lo trata como no interactivo. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="handle"
    role="separator"
    aria-orientation="vertical"
    aria-label={t("ui.inspector.resize")}
    aria-valuemin={INSPECTOR_MIN}
    aria-valuemax={INSPECTOR_MAX}
    aria-valuenow={current}
    tabindex={open ? 0 : -1}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    {onkeydown}
  ></div>
  <div class="content">{@render children()}</div>
</aside>

<style>
  .inspector {
    position: relative;
    flex-shrink: 0;
    width: 0;
    height: 100%;
    overflow: hidden;
    background: var(--window);
    box-shadow: -0.5px 0 0 var(--separator);
    transition: width var(--d-2) var(--ease);
  }

  .inspector.open {
    width: var(--width);
  }

  .inspector.dragging {
    transition: none;
  }

  .content {
    width: var(--width);
    height: 100%;
    overflow: auto;
  }

  .handle {
    position: absolute;
    z-index: 1;
    top: 0;
    bottom: 0;
    left: 0;
    width: 6px;
    cursor: col-resize;
  }

  .handle:focus-visible {
    outline: none;
    background: var(--focus-ring);
  }
</style>
