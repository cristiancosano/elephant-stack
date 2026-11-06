<script lang="ts">
  // Asistente a ventana completa (01 §7.8, C9): banda con el degradado de marca,
  // indicador de puntos, la pantalla actual y la barra de botones abajo.
  // Textos del área diferida «wizard» (docs/i18n.md): viajan con esta vista.
  import "../../lib/i18n/locales/wizard";
  import { untrack } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { fade, fly } from "svelte/transition";

  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import type { Route } from "../../lib/route";
  import { entryOf, wizard } from "../../lib/stores/wizard.svelte";
  import { canGoBack, primary, screens } from "../../lib/wizard/machine";
  import Button from "../../ui/Button.svelte";
  import Apply from "./Apply.svelte";
  import Customize from "./Customize.svelte";
  import Review from "./Review.svelte";
  import Scenario from "./Scenario.svelte";
  import Welcome from "./Welcome.svelte";

  let { snapshot, route }: { snapshot: Snapshot | null; route: Route } = $props();

  const entry = $derived(entryOf(route.sub));
  $effect(() => {
    const e = entry;
    untrack(() => void wizard.open(e));
  });

  const s = $derived(wizard.s);
  const main = $derived(primary(s));
  const path = $derived(screens(s));
  const step = $derived(Math.max(0, path.indexOf(s.screen)));
  /** Primera ejecución: no hay a dónde volver salvo «No cambiar nada». */
  const firstRun = $derived(snapshot?.setup_needed ?? true);
  const showCancel = $derived(!firstRun && s.screen !== "welcome" && s.screen !== "apply" && !wizard.running);

  const SCREEN_NAMES = {
    welcome: "Bienvenida",
    scenario: "Escenario",
    customize: "Personalizar",
    review: "Revisar",
    apply: "Aplicar",
  } as const;

  const reducedMotion =
    typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  function enter(node: Element) {
    // Con reducir movimiento, solo fundido (01 §7.8).
    if (reducedMotion) return fade(node, { duration: 120 });
    return fly(node, { x: 24 * s.direction, duration: 340, easing: cubicOut, opacity: 0 });
  }

  function runPrimary() {
    if (!main.enabled) return;
    if (main.action === "start") wizard.start();
    else if (main.action === "review") wizard.review();
    else if (main.action === "apply") void wizard.apply();
  }

  // ↵ continúa, salvo si el foco está en un control que ya usa Intro.
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.defaultPrevented) return;
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || event.isComposing) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a, input, select, textarea, [role='radio'], dialog")) return;
    event.preventDefault();
    runPrimary();
  }
</script>

<svelte:window {onkeydown} />

<div class="wizard">
  <div class="hero" aria-hidden="true"></div>

  <header class="top" data-tauri-drag-region>
    <ol class="dots" aria-label="Paso {step + 1} de {path.length}: {SCREEN_NAMES[s.screen]}">
      {#each path as screen, i (screen)}
        <li class="dot" class:current={i === step} class:done={i < step} aria-hidden="true"></li>
      {/each}
    </ol>
  </header>

  <main class="stage">
    {#key s.screen}
      <div class="screen" in:enter>
        {#if s.screen === "welcome"}
          <Welcome />
        {:else if s.screen === "scenario"}
          <Scenario />
        {:else if s.screen === "customize"}
          <Customize {snapshot} />
        {:else if s.screen === "review"}
          <Review />
        {:else}
          <Apply />
        {/if}
      </div>
    {/key}
  </main>

  {#if s.screen !== "apply"}
    <footer class="bar">
      <div class="left">
        {#if s.screen === "welcome"}
          <Button variant="plain" busy={wizard.busy === "skip"} onclick={() => void wizard.skip()}>
            Ya tengo mi entorno: no cambiar nada
          </Button>
        {:else if showCancel}
          <Button variant="plain" onclick={() => wizard.close()}>Cancelar</Button>
        {/if}
      </div>
      <div class="right">
        {#if canGoBack(s)}
          <Button onclick={() => wizard.back()}>Atrás</Button>
        {/if}
        <Button variant="primary" disabled={!main.enabled} busy={wizard.busy === "profile"} onclick={runPrimary}>
          {main.label}
        </Button>
      </div>
    </footer>
  {/if}
</div>

<style>
  .wizard {
    position: relative;
    display: grid;
    grid-template-rows: var(--toolbar-height) 1fr auto;
    height: 100vh;
    overflow: hidden;
    background: var(--window);
    color: var(--label);
  }

  /* Banda de 200 px con el degradado de marca que se desvanece hacia abajo. */
  .hero {
    position: absolute;
    inset: 0 0 auto;
    height: 200px;
    background: var(--hero);
    /* Atenuada: el texto de encima usa los colores normales, no --on-hero. */
    opacity: 0.32;
    -webkit-mask-image: linear-gradient(to bottom, #000 0%, transparent 100%);
    mask-image: linear-gradient(to bottom, #000 0%, transparent 100%);
    pointer-events: none;
  }

  .top {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dots {
    display: flex;
    gap: var(--s-2);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--r-pill);
    background: var(--fill-3);
    transition:
      width var(--d-2) var(--ease),
      background-color var(--d-2) var(--ease);
  }

  .dot.done {
    background: var(--label-3);
  }

  .dot.current {
    width: 18px;
    background: var(--accent);
  }

  .stage {
    position: relative;
    min-height: 0;
    overflow: auto;
  }

  .screen {
    max-width: var(--content-max);
    margin: 0 auto;
    padding: var(--s-4) var(--content-padding) var(--s-8);
  }

  .bar {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    padding: var(--s-3) var(--content-padding);
    border-top: 0.5px solid var(--separator);
    background: var(--window);
  }

  .left,
  .right {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }
</style>
