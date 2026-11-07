<script lang="ts">
  // Node (06, fase 3): versiones de Elephant, sin nvm. Una lista única con las
  // instaladas y las de nodejs.org (LTS con soporte y Current; el resto, a
  // petición), la versión por defecto, la de cada proyecto (.nvmrc o
  // .node-version) y si la terminal encuentra antes los selectores de Elephant.
  import "../lib/i18n/locales/node";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Badge from "../ui/Badge.svelte";
  import Select from "../ui/Select.svelte";
  import Icon from "../ui/Icon.svelte";
  import ProgressRing from "../ui/ProgressRing.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import { toast } from "../ui/toasts.svelte";
  import {
    errorMessage,
    nodeCatalog,
    nodeInstall,
    nodeSetGlobal,
    nodeSetProject,
    nodeUninstall,
    nodeUpdate,
    pickFolder,
    revealInFinder,
  } from "../lib/ipc";
  import { below, busyFor, popupMenu, tildePath, type MenuEntry } from "../lib/stores/app.svelte";
  import { copyText } from "../lib/logs/clipboard";
  import { t } from "../lib/i18n/index.svelte";
  import { hiddenReleases, ltsName, mergeNodeVersions, specFor } from "../lib/node-versions";
  import type { InstallProgress } from "../lib/bindings/InstallProgress";
  import type { NodeCatalog } from "../lib/bindings/NodeCatalog";
  import type { NodeChoice } from "../lib/bindings/NodeChoice";
  import type { NodeInstalled } from "../lib/bindings/NodeInstalled";
  import type { NodeRelease } from "../lib/bindings/NodeRelease";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";

  let { snapshot, route: _route }: { snapshot: Snapshot | null; route: Route } = $props();

  const NODE_TARGET = "node";
  const node = $derived(snapshot?.node ?? null);
  const versions = $derived(node?.versions ?? []);
  const active = $derived(node?.active ?? null);
  const busy = $derived(snapshot ? busyFor(snapshot, NODE_TARGET) : undefined);
  const path = $derived(node?.path ?? null);
  const pathOk = $derived(!path || (path.configured && !path.shadowed_by));

  let catalog = $state<NodeCatalog | null>(null);
  let catalogLoading = $state(true);
  let showAll = $state(false);
  /** Descargas pedidas desde esta ventana: versión → fracción hecha (o sin saber). */
  let downloading: Record<string, number | undefined> = $state({});

  async function loadCatalog() {
    catalogLoading = true;
    try {
      catalog = await nodeCatalog();
    } catch {
      catalog = null;
    } finally {
      catalogLoading = false;
    }
  }

  $effect(() => {
    void loadCatalog();
  });

  const releases = $derived(catalog?.releases ?? []);
  const entries = $derived(mergeNodeVersions(versions, releases, showAll));
  const hidden = $derived(hiddenReleases(versions, releases));

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  function track(version: string) {
    downloading[version] = undefined;
    return (e: InstallProgress) => {
      if (e.bytes && e.bytes[1] > 0) downloading[version] = e.bytes[0] / e.bytes[1];
    };
  }

  async function install(r: NodeRelease) {
    try {
      const version = await nodeInstall(r.version, track(r.version));
      toast(t("node.install.done", { version }), { tone: "success" });
    } catch (e) {
      fail(e);
    } finally {
      delete downloading[r.version];
    }
  }

  async function update(v: NodeInstalled, next: NodeRelease) {
    try {
      const report = await nodeUpdate(v.version, track(next.version));
      const moved = report.projects.length;
      toast(
        moved > 0
          ? t("node.update.doneWithProjects", { old: v.version, next: report.new, count: moved })
          : t("node.update.done", { old: v.version, next: report.new }),
        { tone: "success" },
      );
    } catch (e) {
      fail(e);
    } finally {
      delete downloading[next.version];
    }
  }

  async function makeDefault(v: NodeInstalled) {
    if (v.version === active) return;
    const spec = specFor(v.version, versions);
    try {
      await nodeSetGlobal(spec);
      toast(t("node.defaultSet", { spec }), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  async function writeProject(dir: string, name: string, spec: string) {
    const file = await nodeSetProject(dir, spec);
    toast(t("node.useIn.done", { name, spec, file: file.split("/").pop() ?? file }), { tone: "success" });
  }

  async function useInProject(v: NodeInstalled) {
    try {
      const dir = await pickFolder(t("node.useIn.pickFolder", { version: v.version }));
      if (!dir) return;
      const name = dir.split("/").filter(Boolean).pop() ?? dir;
      await writeProject(dir, name, specFor(v.version, versions));
    } catch (e) {
      fail(e);
    }
  }

  async function uninstall(v: NodeInstalled) {
    try {
      await nodeUninstall(v.version);
      toast(t("node.uninstall.done", { version: v.version }), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function menu(v: NodeInstalled, at?: { x: number; y: number }) {
    const entries: MenuEntry[] = [
      { text: t("node.menu.makeDefault"), enabled: v.version !== active, action: () => void makeDefault(v) },
      { text: t("node.menu.reveal"), action: () => void revealInFinder(v.path).catch(fail) },
      "separator",
      { text: t("node.menu.uninstall"), action: () => void uninstall(v) },
    ];
    void popupMenu(entries, at).catch(fail);
  }

  function subtitle(v: NodeInstalled, isDefault: boolean): string {
    return [
      isDefault ? t("node.installedDefault") : null,
      v.lts ? t("node.lts", { name: ltsName(v.lts) }) : null,
      v.npm ? t("node.npm", { version: v.npm }) : null,
      v.globals.length > 0 ? t("node.globals", { list: v.globals.join(", ") }) : null,
    ]
      .filter(Boolean)
      .join(" · ");
  }

  function releaseSubtitle(r: NodeRelease): string {
    return [r.lts ? t("node.lts", { name: ltsName(r.lts) }) : r.featured ? t("node.current") : null, t("node.remote.notInstalled")]
      .filter(Boolean)
      .join(" · ");
  }

  const footer = $derived(catalog?.offline ? t("node.footer.offline") : t("node.footer.default"));

  async function copyLine() {
    if (!path) return;
    try {
      await copyText(path.line);
      toast(t("node.terminal.copied"), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  // ── Proyectos ──
  // `snapshot.node.projects` va en el mismo orden que `snapshot.projects`.
  const projects = $derived(
    (snapshot?.projects ?? []).flatMap((p, i) => {
      const choice = node?.projects[i];
      return choice && choice.dir === p.root ? [{ project: p, choice }] : [];
    }),
  );

  const options = $derived(
    versions.map((v) => ({ value: specFor(v.version, versions), label: v.lts ? `${v.version} (LTS)` : v.version })),
  );

  function projectChoice(choice: NodeChoice) {
    if (choice.reason === "project_file" && choice.version) {
      return { value: specFor(choice.version, versions), options };
    }
    const label =
      choice.reason === "project_file"
        ? t("node.projects.notInstalled", { version: choice.requested ?? "" })
        : choice.version
          ? t("node.projects.byDefault", { version: choice.version })
          : t("node.projects.byDefaultNone");
    return { value: "", options: [{ value: "", label }, ...options] };
  }

  function projectSubtitle(root: string, choice: NodeChoice): string {
    const where = tildePath(root);
    if (choice.reason !== "project_file" || !choice.file || !choice.requested) return where;
    return `${where} · ${t("node.projects.fromFile", { file: choice.file.split("/").pop() ?? "", requested: choice.requested })}`;
  }
</script>

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>{t("node.title")}</h1>
    {#if busy}<ProgressRing label={busy.label} />{/if}
  </header>

  <div class="scroll">
    <div class="content">
      {#if !snapshot}
        <Skeleton lines={4} />
      {:else}
        {#if !pathOk || node?.nvm}
          <Group title={t("node.terminal.title")} footer={!pathOk ? t("node.terminal.footer") : undefined}>
            {#if path && !pathOk}
              {@const who = path.shadowed_by}
              <Row title={who ? t("node.terminal.shadowed.title", { who }) : t("node.terminal.missing.title")}>
                {#snippet leading()}<span class="warn"><Icon name="warning" /></span>{/snippet}
                {#snippet trailing()}
                  <Button onclick={() => void copyLine()}>{t("node.terminal.copy")}</Button>
                {/snippet}
              </Row>
              <p class="note" role="listitem">
                {who ? t("node.terminal.shadowed.message", { who }) : t("node.terminal.missing.message")}
              </p>
              <pre class="code" role="listitem">{path.line}</pre>
            {/if}
            {#if node?.nvm}
              <Row title={t("node.terminal.nvm.title")} subtitle={t("node.terminal.nvm.subtitle", { path: tildePath(node.nvm) })}>
                {#snippet leading()}<span class="info"><Icon name="info" /></span>{/snippet}
              </Row>
              <p class="note" role="listitem">{t("node.terminal.nvm.message")}</p>
            {/if}
          </Group>
        {/if}

        {#if entries.length === 0 && !catalogLoading && releases.length === 0}
          <EmptyState
            icon="node"
            title={t("node.empty.title")}
            message={t("node.empty.message")}
            action={{ label: t("common.retry"), onclick: () => void loadCatalog() }}
          />
        {:else}
          <Group title={t("node.versions.title")} {footer}>
            {#each entries as e (e.key)}
              {#if e.kind === "installed"}
                {@const v = e.version}
                {@const isDefault = v.version === active}
                <Row
                  title={v.version}
                  subtitle={subtitle(v, isDefault)}
                  selected={isDefault}
                  onactivate={() => void makeDefault(v)}
                >
                  {#snippet leading()}
                    <span class="mark" class:on={isDefault}><Icon name="check" /></span>
                  {/snippet}
                  {#snippet trailing()}
                    {#if isDefault}<Badge tone="accent" label={t("node.badge.default")} />{/if}
                    {#if e.update}
                      {@const next = e.update}
                      {#if next.version in downloading}
                        <ProgressRing
                          value={downloading[next.version]}
                          label={t("node.update.busy", { version: v.version, next: next.version })}
                        />
                      {:else}
                        <Button variant="primary" disabled={!!busy} onclick={() => void update(v, next)}>
                          {t("node.update.action", { next: next.version })}<span class="visually-hidden"> (Node {v.version})</span>
                        </Button>
                      {/if}
                    {/if}
                    <Button onclick={() => void useInProject(v)}>
                      {t("node.useIn.action")}<span class="visually-hidden"> (Node {v.version})</span>
                    </Button>
                    <IconButton
                      icon="more"
                      label={t("node.menu.more", { version: v.version })}
                      disabled={!!busy}
                      onclick={(event) => menu(v, below(event))}
                    />
                  {/snippet}
                </Row>
              {:else}
                {@const r = e.release}
                <Row title={r.version} subtitle={releaseSubtitle(r)}>
                  {#snippet leading()}
                    <span class="mark"><Icon name="check" /></span>
                  {/snippet}
                  {#snippet trailing()}
                    {#if r.version in downloading}
                      <ProgressRing value={downloading[r.version]} label={t("node.install.busy", { version: r.version })} />
                    {:else}
                      <Button disabled={!!busy} onclick={() => void install(r)}>
                        {t("node.install.action")}<span class="visually-hidden"> Node {r.version}</span>
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
                <Row title={t("node.catalog.errorTitle")} subtitle={t("node.catalog.errorMessage")}>
                  {#snippet trailing()}
                    <Button onclick={() => void loadCatalog()}>{t("common.retry")}</Button>
                  {/snippet}
                </Row>
              {/if}
            {:else if hidden > 0 || showAll}
              <Row
                title={showAll ? t("node.showAll.less") : t("node.showAll.action", { count: hidden })}
                subtitle={showAll ? undefined : t("node.showAll.hint")}
                onactivate={() => (showAll = !showAll)}
              >
                {#snippet leading()}
                  <span class="mark on"><Icon name={showAll ? "chevron-down" : "chevron-right"} /></span>
                {/snippet}
              </Row>
            {/if}
          </Group>
        {/if}

        {#if projects.length > 0 && versions.length > 0}
          <Group title={t("node.projects.title")} footer={t("node.projects.footer")}>
            {#each projects as { project, choice } (project.id)}
              {@const select = projectChoice(choice)}
              <Row title={project.name} subtitle={projectSubtitle(project.root, choice)}>
                {#snippet trailing()}
                  <Select
                    options={select.options}
                    value={select.value}
                    label={t("node.projects.selectLabel", { name: project.name })}
                    onchange={(spec) => spec && void writeProject(project.root, project.name, spec).catch(fail)}
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

  .warn {
    display: inline-flex;
    color: var(--orange);
  }

  .info {
    display: inline-flex;
    color: var(--label-2);
  }

  .note {
    margin: 0;
    padding: 0 var(--s-3) var(--s-3) calc(var(--s-3) + 16px + var(--s-3));
    font: var(--t-callout);
    color: var(--label-2);
    user-select: text;
    -webkit-user-select: text;
    overflow-wrap: anywhere;
  }

  .code {
    margin: 0 var(--s-3) var(--s-3) calc(var(--s-3) + 16px + var(--s-3));
    padding: var(--s-2) var(--s-3);
    border-radius: var(--r-md);
    background: var(--fill);
    font: var(--t-log);
    font-family: var(--mono);
    color: var(--label);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
