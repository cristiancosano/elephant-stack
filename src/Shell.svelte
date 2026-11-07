<script lang="ts">
  // Barra lateral más la vista de la ruta. Define también el armazón que comparten
  // todas las vistas (clases globales `.view`, `.toolbar`, `.scroll` y `.content`),
  // para que la barra superior respete el hueco de los botones de ventana.
  import { onMount, untrack } from "svelte";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { prefersReducedMotion } from "svelte/motion";
  import Sidebar from "./Sidebar.svelte";
  import IconButton from "./ui/IconButton.svelte";
  import { toast } from "./ui/toasts.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import Services from "./views/Services.svelte";
  import Php from "./views/Php.svelte";
  import Settings from "./views/Settings.svelte";
  import { clearProblems, errorMessage, refresh } from "./lib/ipc";
  import { problemsClearedBy } from "./lib/stores/app.svelte";
  import { onUiCommand } from "./lib/events";
  import type { Route } from "./lib/route";
  import type { Snapshot } from "./lib/bindings/Snapshot";
  import { t } from "./lib/i18n/index.svelte";

  let {
    snapshot,
    route,
    error,
  }: { snapshot: Snapshot | null; route: Route; error: string | null } = $props();

  const COLLAPSED_KEY = "elephant-stack.sidebar-collapsed";

  // Logs va en su propio chunk (01 §4.2). Sitios tampoco entra en el presupuesto
  // del chunk inicial (01 §11), y crecerá con la gestión de sitios.
  let logs: Promise<typeof import("./views/logs/Logs.svelte")> | null = null;
  let sites: Promise<typeof import("./views/Sites.svelte")> | null = null;
  // Node lleva su área de textos diferida (06 fase 3).
  let node: Promise<typeof import("./views/Node.svelte")> | null = null;
  const loadNode = () => (node ??= import("./views/Node.svelte"));
  const loadLogs = () => (logs ??= import("./views/logs/Logs.svelte"));
  const loadSites = () => (sites ??= import("./views/Sites.svelte"));

  function readCollapsed(): boolean {
    try {
      return localStorage.getItem(COLLAPSED_KEY) === "1";
    } catch {
      return false;
    }
  }

  let collapsed = $state(readCollapsed());

  function toggleSidebar() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {
      // Se pierde al cerrar la ventana; no pasa nada.
    }
  }

  async function forceRefresh() {
    try {
      await refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "error" });
    }
  }

  onMount(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    // `find`, `live`, `clear-view` y `toggle-inspector` son de la vista de Logs.
    void onUiCommand((command) => {
      if (command === "toggle-sidebar") toggleSidebar();
      else if (command === "refresh") void forceRefresh();
    }).then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  // Los problemas `action_failed` de una vista se dan por vistos al abrirla (01 §9.4)
  // y se quitan al salir de ella: así «Ver problema» todavía lo enseña. Uno que
  // aparece con la vista abierta se queda hasta la siguiente visita o hasta que la
  // siguiente operación sobre su objetivo salga bien.
  const hasSnapshot = $derived(snapshot !== null);
  let seen: { view: string; targets: string[] } | null = null;
  $effect(() => {
    const view = route.view;
    if (!hasSnapshot) return;
    const current = untrack(() => snapshot);
    if (!current) return;
    if (seen && seen.view !== view) {
      for (const target of seen.targets) {
        clearProblems(target).catch(() => {
          // Sin el comando, el problema se queda hasta la siguiente operación.
        });
      }
    }
    if (!seen || seen.view !== view) seen = { view, targets: problemsClearedBy(current, view) };
  });

  // Un fallo de una acción (por ejemplo, desde el menú de la barra) se avisa también
  // con un toast si la ventana está abierta (01 §9.4). Los que ya estaban al abrirla,
  // no: se ven en el Panel y en su vista.
  let knownProblems: Set<string> | null = null;
  $effect(() => {
    const problems = snapshot?.problems;
    if (!problems) return;
    const ids = new Set(problems.map((p) => p.id));
    if (knownProblems) {
      for (const p of problems) {
        if (p.kind === "action_failed" && !knownProblems.has(p.id)) toast(p.message, { tone: "error" });
      }
    }
    knownProblems = ids;
  });

  // Un error al arrancar se cuenta una vez; los siguientes snapshots lo borran.
  $effect(() => {
    if (error) toast(t("shell.snapshotFailed", { error }), { tone: "error" });
  });

  const enter = $derived({
    y: prefersReducedMotion.current ? 0 : 4,
    duration: 120,
    easing: cubicOut,
  });
</script>

<div class="shell" class:collapsed>
  {#if !collapsed}
    <Sidebar {snapshot} {route} ontoggle={toggleSidebar} />
  {:else}
    <div class="reopen">
      <IconButton icon="sidebar" label={t("shell.sidebar.show")} onclick={toggleSidebar} />
    </div>
  {/if}

  <main class="main">
    {#key route.view}
      <div class="pane" in:fly={enter}>
        {#if route.view === "panel"}
          <Dashboard {snapshot} {route} />
        {:else if route.view === "sitios"}
          {#await loadSites() then { default: Sites }}
            <Sites {snapshot} {route} />
          {/await}
        {:else if route.view === "servicios"}
          <Services {snapshot} {route} />
        {:else if route.view === "php"}
          <Php {snapshot} {route} />
        {:else if route.view === "node"}
          {#await loadNode() then { default: Node }}
            <Node {snapshot} {route} />
          {/await}
        {:else if route.view === "logs"}
          {#await loadLogs() then { default: Logs }}
            <Logs {snapshot} {route} />
          {/await}
        {:else if route.view === "ajustes"}
          <Settings {snapshot} {route} />
        {/if}
      </div>
    {/key}
  </main>
</div>

<style>
  .shell {
    position: relative;
    display: flex;
    height: 100vh;
    /* Lo que queda a la izquierda del título de la vista. */
    --toolbar-inset: var(--content-padding);
  }

  .shell.collapsed {
    /* Botones de ventana + el botón para volver a mostrar la barra lateral. */
    --toolbar-inset: calc(var(--traffic-lights-gap) + 28px + var(--s-3));
  }

  .reopen {
    position: absolute;
    z-index: 1;
    top: calc((var(--toolbar-height) - var(--control-height)) / 2);
    left: var(--traffic-lights-gap);
  }

  /* El contenido es opaco: la vibrancy solo se ve en la barra lateral (01 §5.2). */
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
    background: var(--window);
    box-shadow: -0.5px 0 0 var(--separator);
  }

  .pane {
    position: absolute;
    inset: 0;
  }

  /* ── Armazón común de las vistas ── */

  .main :global(.view) {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .main :global(.toolbar) {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--s-2);
    height: var(--toolbar-height);
    padding: 0 var(--s-4) 0 var(--toolbar-inset);
  }

  .main :global(.toolbar h1) {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font: var(--t-title);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .main :global(.scroll) {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  /* Solo el contenido de la vista: `.content` también es el nombre de clases
     internas de componentes (Button, Inspector). */
  .main :global(.scroll > .content) {
    max-width: calc(var(--content-max) + 2 * var(--content-padding));
    margin: 0 auto;
    padding: var(--s-2) var(--content-padding) var(--content-padding);
  }

  /* Separación entre bloques: la misma que Group pone entre dos grupos seguidos. */
  .main :global(.scroll > .content > * + *) {
    margin-top: var(--group-gap);
  }
</style>
