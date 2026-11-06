<script lang="ts">
  // PHP (01 §7.4): versiones instaladas con la de por defecto, la versión de cada
  // proyecto (`.php-version`) y la descarga de versiones de php-builds.
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import Button from "../ui/Button.svelte";
  import Badge from "../ui/Badge.svelte";
  import Select from "../ui/Select.svelte";
  import Icon from "../ui/Icon.svelte";
  import ProgressRing from "../ui/ProgressRing.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import { toast } from "../ui/toasts.svelte";
  import {
    errorMessage,
    phpCatalog,
    phpDownload,
    pickFolder,
    refresh,
    setGlobalPhp,
    setProjectPhp,
  } from "../lib/ipc";
  import {
    busyFor,
    PHP_TARGET,
    phpOptions,
    tildePath,
    versionForProject,
  } from "../lib/stores/app.svelte";
  import { formatBytes } from "../lib/format";
  import { mergePhpVersions } from "../lib/php-versions";
  import type { Catalog } from "../lib/bindings/Catalog";
  import type { Project } from "../lib/bindings/Project";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { VersionInfo } from "../lib/bindings/VersionInfo";
  import type { Route } from "../lib/route";

  let { snapshot, route: _route }: { snapshot: Snapshot | null; route: Route } = $props();

  const versions = $derived(snapshot?.php.versions ?? []);
  const activeKey = $derived(snapshot?.php.active?.key ?? null);
  const busy = $derived(snapshot ? busyFor(snapshot, PHP_TARGET) : undefined);
  const options = $derived(phpOptions(versions));
  const projects = $derived(snapshot?.projects ?? []);

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  async function makeDefault(v: VersionInfo) {
    if (v.key === activeKey) return;
    try {
      await setGlobalPhp(v.key);
    } catch (e) {
      fail(e);
    }
  }

  async function useInProject(v: VersionInfo) {
    try {
      const dir = await pickFolder(`Carpeta del proyecto que usará PHP ${v.id}`);
      if (!dir) return;
      await setProjectPhp(dir, v.key);
      const name = dir.split("/").filter(Boolean).pop() ?? dir;
      toast(`${name} usará PHP ${v.id} (.php-version).`, { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  async function setForProject(p: Project, key: string) {
    const v = versions.find((candidate) => candidate.key === key);
    if (!v) return;
    try {
      await setProjectPhp(p.root, key);
      toast(`${p.name} usará PHP ${v.id} (.php-version).`, { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function projectChoice(p: Project) {
    const matched = versionForProject(versions, p.php_version);
    // Sin coincidencia, una opción que solo describe el estado actual y no hace nada.
    const current = matched
      ? []
      : [{ value: "", label: p.php_version ? `${p.php_version} · no instalada` : "Por defecto" }];
    return { value: matched?.key ?? "", options: [...current, ...options] };
  }

  // ── Descargas (php-builds) ──
  // La lista siempre se muestra: si el catálogo no llega, explica por qué y
  // deja reintentar, en vez de ocultar la función sin decir nada.
  let catalog = $state<Catalog | null>(null);
  let catalogLoading = $state(true);
  let downloading: Record<string, number | undefined> = $state({});

  async function loadCatalog() {
    catalogLoading = true;
    try {
      catalog = await phpCatalog();
    } catch {
      catalog = null;
    } finally {
      catalogLoading = false;
    }
  }

  $effect(() => {
    void loadCatalog();
  });

  const releases = $derived(catalog?.php ?? []);
  const entries = $derived(mergePhpVersions(versions, releases));
  const footer = $derived(
    catalog?.offline
      ? "Sin conexión: las versiones no instaladas son las de la última consulta."
      : "La marcada es la versión por defecto, la que se usa fuera de cualquier proyecto con .php-version. Las no instaladas se descargan de php-builds.",
  );

  async function download(version: string) {
    downloading[version] = undefined;
    try {
      const report = await phpDownload(version, (event) => {
        if (event.bytes && event.bytes[1] > 0) downloading[version] = event.bytes[0] / event.bytes[1];
      });
      const failed = report.failed[0];
      if (failed) toast(failed[1], { tone: "error" });
      else toast(`PHP ${version} está listo.`, { tone: "success" });
      void refresh().catch(() => {});
    } catch (e) {
      fail(e);
    } finally {
      delete downloading[version];
    }
  }
</script>

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>PHP</h1>
    {#if busy}<ProgressRing label={busy.label} />{/if}
  </header>

  <div class="scroll">
    <div class="content">
      {#if !snapshot}
        <Skeleton lines={4} />
      {:else}
        {#if entries.length === 0 && !catalogLoading}
          <EmptyState
            icon="php"
            title="No hay ninguna versión de PHP"
            message="No hay versiones instaladas y no se ha podido consultar las disponibles."
            action={{ label: "Reintentar", onclick: () => void loadCatalog() }}
          />
        {:else}
          <Group title="Versiones" footer={footer}>
            {#each entries as e (e.key)}
              {#if e.kind === "installed"}
                {@const v = e.version}
                {@const active = v.key === activeKey}
                <Row
                  title={v.id}
                  subtitle={active ? `${v.source} · Por defecto` : v.source}
                  selected={active}
                  onactivate={() => void makeDefault(v)}
                >
                  {#snippet leading()}
                    <span class="mark" class:on={active}><Icon name="check" /></span>
                  {/snippet}
                  {#snippet trailing()}
                    {#if active}<Badge tone="accent" label="Por defecto" />{/if}
                    <Button onclick={() => void useInProject(v)}>
                      Usar en proyecto…<span class="visually-hidden"> (PHP {v.id})</span>
                    </Button>
                  {/snippet}
                </Row>
              {:else}
                {@const r = e.release}
                <Row title={r.version} subtitle={`No instalada · ${formatBytes(r.size)}`}>
                  {#snippet leading()}
                    <span class="mark"><Icon name="check" /></span>
                  {/snippet}
                  {#snippet trailing()}
                    {#if r.version in downloading}
                      <ProgressRing value={downloading[r.version]} label={`Instalando PHP ${r.version}`} />
                    {:else}
                      <Button onclick={() => void download(r.version)}>
                        Instalar<span class="visually-hidden"> PHP {r.version}</span>
                      </Button>
                    {/if}
                  {/snippet}
                </Row>
              {/if}
            {/each}
            {#if releases.length === 0}
              {#if catalogLoading}
                <Skeleton lines={1} />
              {:else}
                <Row
                  title="No se pudo consultar las versiones disponibles"
                  subtitle="Comprueba la conexión y vuelve a intentarlo en unos minutos."
                >
                  {#snippet trailing()}
                    <Button onclick={() => void loadCatalog()}>Reintentar</Button>
                  {/snippet}
                </Row>
              {/if}
            {/if}
          </Group>
        {/if}

        {#if projects.length > 0}
          <Group title="Proyectos" footer="Elegir una versión escribe .php-version en la carpeta del proyecto.">
            {#each projects as p (p.id)}
              {@const choice = projectChoice(p)}
              <Row title={p.name} subtitle={tildePath(p.root)}>
                {#snippet trailing()}
                  <Select
                    options={choice.options}
                    value={choice.value}
                    label={`PHP de ${p.name}`}
                    onchange={(key) => void setForProject(p, key)}
                  />
                {/snippet}
              </Row>
            {/each}
          </Group>
        {/if}
      {/if}
    </div>
  </div>
</div>


<style>
  .mark {
    display: inline-flex;
    color: transparent;
  }

  .mark.on {
    color: var(--accent);
  }
</style>
