<script lang="ts">
  // Barra lateral (01 §7.1): transparente sobre el material `sidebar` de la ventana.
  // Los atajos (⌘1…⌘5, ⌘,) están en el menú Visualización, no se pintan aquí.
  import Icon from "./ui/Icon.svelte";
  import IconButton from "./ui/IconButton.svelte";
  import type { IconName } from "./ui/icons";
  import type { Route, View } from "./lib/route";
  import type { Snapshot } from "./lib/bindings/Snapshot";
  import { phpBranch, runningPids, services, sites } from "./lib/stores/app.svelte";
  import { totalMemoryKb, usage } from "./lib/stores/usage.svelte";
  import { formatMemory, formatNumber, plural } from "./lib/format";

  let {
    snapshot,
    route,
    ontoggle,
  }: { snapshot: Snapshot | null; route: Route; ontoggle: () => void } = $props();

  interface Item {
    view: View;
    href: string;
    label: string;
    icon: IconName;
    /** Texto a la derecha («3/5», «8.4»). */
    count?: string;
    /** Lo que lee VoiceOver en lugar de `count`. */
    countLabel?: string;
    /** Punto rojo de errores sin ver. */
    alert?: boolean;
  }

  const running = $derived(snapshot ? runningPids(snapshot) : []);
  const runningCount = $derived(
    snapshot ? services(snapshot).filter((c) => c.service?.running).length : 0,
  );
  const memoryKb = $derived(totalMemoryKb(running, usage.byPid));

  const summary = $derived.by(() => {
    if (!snapshot) return "";
    if (runningCount === 0) return "Nada en marcha";
    const base = `${formatNumber(runningCount)} en marcha`;
    return memoryKb > 0 ? `${base} · ${formatMemory(memoryKb)}` : base;
  });

  const main: Item[] = $derived.by(() => {
    const s = snapshot;
    const siteCount = s ? sites(s).length : 0;
    const installed = s ? services(s).length : 0;
    const php = s ? phpBranch(s) : null;
    const unseen = s?.logs.unseen_errors ?? 0;
    return [
      { view: "panel", href: "#/panel", label: "Panel", icon: "panel" },
      {
        view: "sitios",
        href: "#/sitios",
        label: "Sitios",
        icon: "sitios",
        count: siteCount > 0 ? formatNumber(siteCount) : undefined,
        countLabel: plural(siteCount, "sitio"),
      },
      {
        view: "servicios",
        href: "#/servicios",
        label: "Servicios",
        icon: "servicios",
        count: installed > 0 ? `${runningCount}/${installed}` : undefined,
        countLabel: `${runningCount} de ${plural(installed, "servicio")} en marcha`,
      },
      {
        view: "php",
        href: "#/php",
        label: "PHP",
        icon: "php",
        count: php ?? undefined,
        countLabel: php ? `PHP ${php} por defecto` : undefined,
      },
      {
        view: "logs",
        href: "#/logs",
        label: "Logs",
        icon: "logs",
        count: unseen > 0 ? formatNumber(unseen) : undefined,
        countLabel: plural(unseen, "error sin ver", "errores sin ver"),
        alert: unseen > 0,
      },
    ];
  });

  const settings: Item = { view: "ajustes", href: "#/ajustes/general", label: "Ajustes", icon: "ajustes" };

  const agents = $derived.by(() => {
    const mcp = snapshot?.mcp;
    if (!mcp?.linked) return null;
    const mode = { read_only: "solo lectura", confirm: "activo", trusted: "sin confirmación" }[mcp.mode];
    return `Agentes · ${mode}`;
  });

  let nav: HTMLElement | undefined = $state();

  // ↑ y ↓ mueven el foco entre los items, como en una lista de macOS.
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const links = Array.from(nav?.querySelectorAll<HTMLAnchorElement>("a.item") ?? []);
    const index = links.indexOf(document.activeElement as HTMLAnchorElement);
    if (index === -1) return;
    event.preventDefault();
    const next = event.key === "ArrowDown" ? index + 1 : index - 1;
    links[(next + links.length) % links.length]?.focus();
  }
</script>

{#snippet item(it: Item)}
  <a class="item" href={it.href} aria-current={route.view === it.view ? "page" : undefined}>
    <span class="icon"><Icon name={it.icon} /></span>
    <span class="label">{it.label}</span>
    {#if it.count}
      <span class="count num" class:alert={it.alert} aria-hidden="true">
        {#if it.alert}<span class="dot"></span>{/if}{it.count}
      </span>
      {#if it.countLabel}<span class="visually-hidden">, {it.countLabel}</span>{/if}
    {/if}
  </a>
{/snippet}

<aside class="sidebar">
  <div class="top" data-tauri-drag-region>
    <IconButton icon="sidebar" label="Ocultar la barra lateral" onclick={ontoggle} />
  </div>

  <header class="header" data-tauri-drag-region>
    <p class="title" data-tauri-drag-region>Elephant Stack</p>
    <p class="summary num" data-tauri-drag-region aria-live="polite">{summary}</p>
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav aria-label="Secciones" bind:this={nav} {onkeydown}>
    <ul class="list">
      {#each main as it (it.view)}
        <li>{@render item(it)}</li>
      {/each}
    </ul>
    <ul class="list">
      <li>{@render item(settings)}</li>
    </ul>
  </nav>

  {#if agents}
    <footer class="footer">
      <a class="agents" href="#/ajustes/agentes">
        <Icon name="agent" />
        <span>{agents}</span>
      </a>
    </footer>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: var(--sidebar-width);
    height: 100%;
    background: var(--sidebar-bg);
  }

  /* A la altura de los botones de ventana (trafficLightPosition y = 20). */
  .top {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    height: var(--toolbar-height);
    padding: 0 var(--s-2) 0 var(--traffic-lights-gap);
    flex-shrink: 0;
  }

  .header {
    padding: 0 var(--s-5) var(--s-3);
  }

  .title {
    font: var(--t-headline);
    color: var(--label);
  }

  .summary {
    min-height: 15px;
    margin-top: 2px;
    font: var(--t-callout);
    color: var(--label-2);
  }

  nav {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--s-4);
    min-height: 0;
    overflow-y: auto;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    height: 28px;
    margin: 0 var(--s-2);
    padding: 0 var(--s-2);
    border-radius: var(--r-sm);
    color: var(--label);
  }

  .item:focus-visible {
    outline-offset: -2px;
  }

  .icon {
    display: inline-flex;
    color: var(--label-2);
  }

  .item[aria-current="page"] .icon {
    color: var(--accent);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    font: var(--t-callout);
    color: var(--label-2);
  }

  .count.alert {
    color: var(--red);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--r-pill);
    background: var(--red);
  }

  .item[aria-current="page"] {
    background: var(--accent-soft);
  }

  /* Ventana inactiva: selección gris e iconos sin acento, como AppKit. */
  :global(:root[data-window-inactive]) .item[aria-current="page"] {
    background: var(--sidebar-selected);
  }

  :global(:root[data-window-inactive]) .item[aria-current="page"] .icon {
    color: var(--label-2);
  }

  .footer {
    padding: var(--s-3) var(--s-2);
    border-top: 0.5px solid var(--separator);
  }

  .agents {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-1) var(--s-2);
    border-radius: var(--r-sm);
    font: var(--t-callout);
    color: var(--label-2);
  }
</style>
