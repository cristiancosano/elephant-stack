<script lang="ts">
  // Sitios (C17): lo que descubre elephant-logs, incluidos los sitios de Herd, en
  // solo lectura. Elephant no escribe en Herd: no hay «Dejar de servir» hasta que
  // exista una decisión de sitios.
  // Textos del área diferida «sites» (docs/i18n.md): viajan con esta vista.
  import "../lib/i18n/locales/sites";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Select from "../ui/Select.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import { toast } from "../ui/toasts.svelte";
  import { errorMessage, projectStartServices, revealInFinder, setProjectPhp, siteAction } from "../lib/ipc";
  import {
    below,
    busyFor,
    middleTruncate,
    phpOptions,
    popupMenu,
    siteUrl,
    tildePath,
    versionForProject,
  } from "../lib/stores/app.svelte";
  import type { MenuEntry } from "../lib/stores/app.svelte";
  import { go } from "../lib/router.svelte";
  import type { Project } from "../lib/bindings/Project";
  import type { SiteAction } from "../lib/bindings/SiteAction";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";
  import { fmt, t } from "../lib/i18n/index.svelte";

  let { snapshot, route: _route }: { snapshot: Snapshot | null; route: Route } = $props();

  /** Caracteres de la ruta que caben en el subtítulo antes de recortarla por el centro. */
  const PATH_CHARS = 56;

  const projects = $derived(snapshot?.projects ?? []);
  const served = $derived(projects.filter((p) => p.hosts.length > 0));
  const others = $derived(projects.filter((p) => p.hosts.length === 0));
  const versions = $derived(snapshot?.php.versions ?? []);
  const options = $derived(phpOptions(versions));

  const kindLabel = (kind: Project["kind"]) => t(`sites.kind.${kind}`);

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  // «Arrancar lo que necesita» (04 §4.10): devuelve los servicios de Homebrew
  // que ha arrancado.
  async function startNeeds(p: Project) {
    try {
      const started = await projectStartServices(p.root);
      toast(
        started.length > 0
          ? t("sites.needs.started", { name: p.name, list: fmt.list(started) })
          : t("sites.needs.nothing", { name: p.name }),
        { tone: "success" },
      );
    } catch (e) {
      fail(e);
    }
  }

  async function act(p: Project, action: SiteAction) {
    try {
      await siteAction(p.id, action);
      if (action === "copy_url") toast(t("sites.copied", { url: siteUrl(p) ?? "" }), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function menu(p: Project, at?: { x: number; y: number }) {
    const web = p.hosts.length > 0;
    void popupMenu(
      [
        { text: t("sites.menu.openBrowser"), enabled: web, action: () => void act(p, "browser") },
        { text: t("sites.menu.openEditor"), action: () => void act(p, "editor") },
        { text: t("sites.menu.openTerminal"), action: () => void act(p, "terminal") },
        { text: t("sites.menu.revealInFinder"), action: () => void revealInFinder(p.root).catch(fail) },
        { text: t("sites.menu.copyUrl"), enabled: web, action: () => void act(p, "copy_url") },
        "separator",
        ...needsEntry(p),
        { text: t("sites.menu.viewLogs"), action: () => go(`#/logs?project=${encodeURIComponent(p.id)}`) },
      ],
      at,
    ).catch(fail);
  }

  function needsEntry(p: Project): MenuEntry[] {
    const busy = snapshot ? busyFor(snapshot, p.id) : undefined;
    return [
      busy
        ? { text: busy.label, enabled: false }
        : { text: t("sites.menu.startNeeds"), action: () => void startNeeds(p) },
    ];
  }

  function phpChoice(p: Project) {
    const matched = versionForProject(versions, p.php_version);
    const current = matched
      ? []
      : [
          {
            value: "",
            label: p.php_version
              ? t("sites.php.notInstalled", { version: p.php_version })
              : t("sites.php.byDefault"),
          },
        ];
    return { value: matched?.key ?? "", options: [...current, ...options] };
  }

  async function setPhp(p: Project, key: string) {
    const v = versions.find((candidate) => candidate.key === key);
    if (!v) return;
    try {
      await setProjectPhp(p.root, key);
      toast(t("sites.php.done", { name: p.name, version: v.id }), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function subtitle(p: Project): string {
    const path = middleTruncate(tildePath(p.root), PATH_CHARS);
    return p.hosts.length > 0 ? `${kindLabel(p.kind)} · ${path}` : path;
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
          label={t("sites.php.selectLabel", { name: p.name })}
          onchange={(key) => void setPhp(p, key)}
        />
      {/if}
      {#if p.hosts.length > 0}
        <IconButton icon="open" label={t("common.openInBrowser", { host: p.hosts[0] })} onclick={() => void act(p, "browser")} />
      {/if}
      <IconButton icon="more" label={t("sites.more", { name: p.name })} onclick={(event) => menu(p, below(event))} />
    {/snippet}
  </Row>
{/snippet}

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>{t("sites.title")}</h1>
  </header>

  <div class="scroll">
    <div class="content">
      {#if !snapshot}
        <Skeleton lines={4} />
      {:else if projects.length === 0}
        <EmptyState
          icon="sitios"
          title={t("sites.empty.title")}
          message={t("sites.empty.message")}
          action={{ label: t("sites.empty.action"), onclick: () => go("#/asistente") }}
        />
      {:else}
        {#if served.length > 0}
          <Group title={t("sites.title")} footer={t("sites.group.footer")}>
            {#each served as p (p.id)}
              {@render projectRow(p)}
            {/each}
          </Group>
        {/if}
        {#if others.length > 0}
          <Group title={t("sites.group.others")}>
            {#each others as p (p.id)}
              {@render projectRow(p)}
            {/each}
          </Group>
        {/if}
      {/if}
    </div>
  </div>
</div>
