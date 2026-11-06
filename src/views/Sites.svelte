<script lang="ts">
  // Sitios (C17): lo que descubre elephant-logs, incluidos los sitios de Herd, en
  // solo lectura. Elephant no escribe en Herd: no hay «Dejar de servir» hasta que
  // exista una decisión de sitios.
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Select from "../ui/Select.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import { toast } from "../ui/toasts.svelte";
  import { errorMessage, revealInFinder, setProjectPhp, siteAction } from "../lib/ipc";
  import {
    below,
    middleTruncate,
    phpOptions,
    popupMenu,
    siteUrl,
    tildePath,
    versionForProject,
  } from "../lib/stores/app.svelte";
  import { go } from "../lib/router.svelte";
  import type { Project } from "../lib/bindings/Project";
  import type { SiteAction } from "../lib/bindings/SiteAction";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";

  let { snapshot, route: _route }: { snapshot: Snapshot | null; route: Route } = $props();

  /** Caracteres de la ruta que caben en el subtítulo antes de recortarla por el centro. */
  const PATH_CHARS = 56;

  const projects = $derived(snapshot?.projects ?? []);
  const served = $derived(projects.filter((p) => p.hosts.length > 0));
  const others = $derived(projects.filter((p) => p.hosts.length === 0));
  const versions = $derived(snapshot?.php.versions ?? []);
  const options = $derived(phpOptions(versions));

  const KIND: Record<Project["kind"], string> = { laravel: "Laravel", php: "PHP", other: "Otro" };

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  async function act(p: Project, action: SiteAction) {
    try {
      await siteAction(p.id, action);
      if (action === "copy_url") toast(`Copiado: ${siteUrl(p)}`, { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function menu(p: Project, at?: { x: number; y: number }) {
    const web = p.hosts.length > 0;
    void popupMenu(
      [
        { text: "Abrir en el navegador", enabled: web, action: () => void act(p, "browser") },
        { text: "Abrir en el editor", action: () => void act(p, "editor") },
        { text: "Abrir terminal", action: () => void act(p, "terminal") },
        { text: "Mostrar en Finder", action: () => void revealInFinder(p.root).catch(fail) },
        { text: "Copiar URL", enabled: web, action: () => void act(p, "copy_url") },
        "separator",
        { text: "Ver logs", action: () => go(`#/logs?project=${encodeURIComponent(p.id)}`) },
      ],
      at,
    ).catch(fail);
  }

  function phpChoice(p: Project) {
    const matched = versionForProject(versions, p.php_version);
    const current = matched
      ? []
      : [{ value: "", label: p.php_version ? `${p.php_version} · no instalada` : "PHP por defecto" }];
    return { value: matched?.key ?? "", options: [...current, ...options] };
  }

  async function setPhp(p: Project, key: string) {
    const v = versions.find((candidate) => candidate.key === key);
    if (!v) return;
    try {
      await setProjectPhp(p.root, key);
      toast(`${p.name} usará PHP ${v.id} (.php-version).`, { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function subtitle(p: Project): string {
    const path = middleTruncate(tildePath(p.root), PATH_CHARS);
    return p.hosts.length > 0 ? `${KIND[p.kind]} · ${path}` : path;
  }
</script>

{#snippet projectRow(p: Project)}
  {@const choice = phpChoice(p)}
  <Row title={p.hosts[0] ?? p.name} subtitle={subtitle(p)}>
    {#snippet leading()}<Icon name={p.hosts.length > 0 ? "globe" : "folder"} />{/snippet}
    {#snippet trailing()}
      {#if versions.length > 0}
        <Select
          options={choice.options}
          value={choice.value}
          label={`PHP de ${p.name}`}
          onchange={(key) => void setPhp(p, key)}
        />
      {/if}
      {#if p.hosts.length > 0}
        <IconButton icon="open" label={`Abrir ${p.hosts[0]} en el navegador`} onclick={() => void act(p, "browser")} />
      {/if}
      <IconButton icon="more" label={`Más opciones de ${p.name}`} onclick={(event) => menu(p, below(event))} />
    {/snippet}
  </Row>
{/snippet}

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>Sitios</h1>
  </header>

  <div class="scroll">
    <div class="content">
      {#if !snapshot}
        <Skeleton lines={4} />
      {:else if projects.length === 0}
        <EmptyState
          icon="sitios"
          title="Tus sitios los sirve Herd por ahora"
          message="Cuando Herd sirva un proyecto o añadas una carpeta en Ajustes ▸ Logs, aparecerá aquí."
          action={{ label: "Configurar con el asistente", onclick: () => go("#/asistente") }}
        />
      {:else}
        {#if served.length > 0}
          <Group title="Sitios" footer="Los sirve Herd. Elephant Stack no cambia su configuración.">
            {#each served as p (p.id)}
              {@render projectRow(p)}
            {/each}
          </Group>
        {/if}
        {#if others.length > 0}
          <Group title="Otros proyectos">
            {#each others as p (p.id)}
              {@render projectRow(p)}
            {/each}
          </Group>
        {/if}
      {/if}
    </div>
  </div>
</div>
