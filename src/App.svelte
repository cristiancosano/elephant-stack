<script lang="ts">
  // Raíz de la ventana: conecta el snapshot de Rust, la ruta y el sondeo de uso,
  // y decide entre el asistente (a ventana completa) y el Shell con barra lateral.
  import { onMount } from "svelte";
  import Shell from "./Shell.svelte";
  import Toaster from "./ui/Toaster.svelte";
  import { app, connect, runningPids } from "./lib/stores/app.svelte";
  import { startUsage, type Poller } from "./lib/stores/usage.svelte";
  import { go, router } from "./lib/router.svelte";
  import { serialize } from "./lib/route";
  import { takePendingRoute } from "./lib/ipc";
  import { onAppearanceChanged, onNavGo, type UnlistenFn } from "./lib/events";

  const LAST_ROUTE_KEY = "elephant-stack.last-route";

  // El asistente se carga aparte: no entra en el chunk inicial.
  let wizard: Promise<typeof import("./views/wizard/Wizard.svelte")> | null = null;
  const loadWizard = () => (wizard ??= import("./views/wizard/Wizard.svelte"));

  /** Ya se sabe si hay una ruta pedida desde Rust; antes no se decide nada. */
  let started = $state(false);
  let pendingRoute: string | null = null;
  let setupChecked = false;
  let poller: Poller | null = null;

  function readLastRoute(): string | null {
    try {
      return localStorage.getItem(LAST_ROUTE_KEY);
    } catch {
      return null;
    }
  }

  function saveLastRoute(hash: string) {
    try {
      localStorage.setItem(LAST_ROUTE_KEY, hash);
    } catch {
      // Sin almacenamiento, se abre en el Panel la próxima vez.
    }
  }

  function setAccent(accent: string, accentText: string) {
    const root = document.documentElement.style;
    root.setProperty("--system-accent", accent);
    root.setProperty("--system-accent-text", accentText);
  }

  onMount(() => {
    const cleanups: (UnlistenFn | (() => void))[] = [];
    let disposed = false;
    const keep = (fn: UnlistenFn | (() => void)) => (disposed ? fn() : cleanups.push(fn));

    void (async () => {
      // Primero los eventos, para no perder un `nav:go` mientras se monta.
      const listeners = await Promise.allSettled([
        connect(),
        onNavGo((route) => go(route)),
        onAppearanceChanged((a) => setAccent(a.accent, a.accent_text)),
      ]);
      for (const result of listeners) if (result.status === "fulfilled") keep(result.value);

      try {
        pendingRoute = await takePendingRoute();
      } catch {
        pendingRoute = null;
      }
      if (pendingRoute) {
        go(pendingRoute);
      } else if (!window.location.hash) {
        const last = readLastRoute();
        if (last) go(last);
      }
      started = true;
    })();

    const usage = startUsage(() => (app.snapshot ? runningPids(app.snapshot) : []));
    poller = usage.poller;
    keep(usage.stop);

    return () => {
      disposed = true;
      for (const fn of cleanups) fn();
    };
  });

  // Primera ejecución (C8): sin `setup.json` se abre el asistente. Una ruta pedida
  // expresamente (desde el menú de la barra) manda sobre esto: el usuario ha pedido
  // otra cosa, y el asistente sigue a un clic en Ajustes ▸ General.
  $effect(() => {
    const snapshot = app.snapshot;
    if (!started || !snapshot || setupChecked) return;
    setupChecked = true;
    if (snapshot.setup_needed && !pendingRoute) go("#/asistente");
  });

  // Recuerda la última vista para la próxima vez que se abra la ventana. El
  // asistente no: volver a él tiene que ser una decisión.
  $effect(() => {
    const route = router.route;
    if (started && route.view !== "asistente") saveLastRoute(serialize(route));
  });

  // Un servicio que arranca o se para cambia los PIDs: se mide sin esperar 5 s.
  const pidsKey = $derived(app.snapshot ? runningPids(app.snapshot).join(",") : "");
  $effect(() => {
    void pidsKey;
    void poller?.poll();
  });
</script>

{#if router.route.view === "asistente"}
  {#await loadWizard() then { default: Wizard }}
    <Wizard snapshot={app.snapshot} route={router.route} />
  {/await}
{:else}
  <Shell snapshot={app.snapshot} route={router.route} error={app.error} />
{/if}

<Toaster />
