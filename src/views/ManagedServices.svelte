<script lang="ts">
  // Servicios propios (06): una lista por servicio (MySQL, PostgreSQL, Redis y
  // Mailpit) con las versiones instaladas y las del catálogo, como la vista PHP.
  // Instalar, actualizar a un parche, encender o apagar (una rama a la vez),
  // arranque al iniciar sesión, desinstalar y copiar los datos de Homebrew.
  // Después, las herramientas (phpMyAdmin y pgAdmin). El estado sale del snapshot.
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import Switch from "../ui/Switch.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Button from "../ui/Button.svelte";
  import Badge from "../ui/Badge.svelte";
  import StatusDot, { type DotState } from "../ui/StatusDot.svelte";
  import ProgressRing from "../ui/ProgressRing.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import { toast } from "../ui/toasts.svelte";
  import {
    errorMessage,
    openServiceLog,
    openUrl,
    revealInFinder,
    serviceAutostart,
    serviceCatalog,
    serviceImport,
    serviceInstall,
    servicePower,
    serviceUninstall,
    serviceUpdate,
    toolInstall,
    toolOpen,
    toolPower,
    toolUninstall,
  } from "../lib/ipc";
  import { below, busyFor, popupMenu, problemFor, tildePath, type MenuEntry } from "../lib/stores/app.svelte";
  import { go } from "../lib/router.svelte";
  import { mergeManagedVersions } from "../lib/managed-services";
  import { fmt, t } from "../lib/i18n/index.svelte";
  import type { BranchStatus } from "../lib/bindings/BranchStatus";
  import type { InstallProgress } from "../lib/bindings/InstallProgress";
  import type { ManagedService } from "../lib/bindings/ManagedService";
  import type { ServicesCatalog } from "../lib/bindings/ServicesCatalog";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { ToolStatus } from "../lib/bindings/ToolStatus";

  let { snapshot }: { snapshot: Snapshot } = $props();

  let catalog = $state<ServicesCatalog | null>(null);
  let catalogLoading = $state(true);
  /** Descargas pedidas desde esta ventana: versión → fracción hecha (o sin saber). */
  let downloading: Record<string, number | undefined> = $state({});

  async function loadCatalog() {
    catalogLoading = true;
    try {
      catalog = await serviceCatalog();
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

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  const label = (s: ManagedService, version: string) => `${s.name} ${version}`;

  function track(version: string) {
    downloading[version] = undefined;
    return (e: InstallProgress) => {
      if (e.bytes && e.bytes[1] > 0) downloading[version] = e.bytes[0] / e.bytes[1];
    };
  }

  async function install(s: ManagedService, version: string) {
    try {
      await serviceInstall(s.service, version, track(version));
      toast(t("services.managed.install.done", { name: label(s, version) }), { tone: "success" });
    } catch (e) {
      fail(e);
    } finally {
      delete downloading[version];
    }
  }

  async function update(s: ManagedService, from: string, to: string) {
    try {
      const report = await serviceUpdate(s.service, from, to, track(to));
      const key = report.restarted ? "services.managed.update.doneRestarted" : "services.managed.update.done";
      toast(t(key, { name: label(s, from), next: to }), { tone: "success" });
    } catch (e) {
      fail(e);
    } finally {
      delete downloading[to];
    }
  }

  async function power(s: ManagedService, b: BranchStatus, on: boolean) {
    try {
      const report = await servicePower(s.service, b.branch, on);
      if (!on) return;
      const name = label(s, b.branch);
      toast(
        report.stopped.length > 0
          ? t("services.managed.power.switched", { name, list: fmt.list(report.stopped) })
          : t("services.managed.power.started", { name }),
        { tone: "success" },
      );
    } catch (e) {
      fail(e);
    }
  }

  async function uninstall(s: ManagedService, version: string, branch: string) {
    try {
      await serviceUninstall(s.service, version);
      const dir = s.branches.find((b) => b.branch === branch)?.data_dir ?? "";
      toast(t("services.managed.uninstall.done", { name: label(s, version), dir: tildePath(dir) }), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  async function importData(s: ManagedService, branch: string) {
    try {
      const report = await serviceImport(s.service, branch);
      const name = s.name;
      toast(
        report.backup
          ? t("services.managed.import.doneWithBackup", { name, branch, backup: tildePath(report.backup) })
          : t("services.managed.import.done", { name, branch }),
        { tone: "success" },
      );
    } catch (e) {
      fail(e);
    }
  }

  function menu(s: ManagedService, b: BranchStatus, version: string, at?: { x: number; y: number }) {
    const entries: MenuEntry[] = [
      {
        text: t("services.managed.menu.startAtLogin"),
        checked: b.autostart,
        action: () => void serviceAutostart(s.service, b.branch, !b.autostart).catch(fail),
      },
      "separator",
      { text: t("services.menu.viewLogs"), action: () => void openServiceLog(b.label).catch(fail) },
      {
        text: t("services.managed.menu.revealData"),
        enabled: b.data_ready,
        action: () => void revealInFinder(b.data_dir).catch(fail),
      },
      "separator",
      {
        text: t("services.managed.uninstall.action"),
        enabled: b.state !== "running",
        action: () => void uninstall(s, version, b.branch),
      },
    ];
    void popupMenu(entries, at).catch(fail);
  }

  function dot(b: BranchStatus): DotState {
    if (busyFor(snapshot, b.label)) return "busy";
    return b.state;
  }

  function subtitle(s: ManagedService, b: BranchStatus, lts: boolean): string {
    const busy = busyFor(snapshot, b.label);
    if (busy) return busy.label;
    // El motivo completo va en la nota de debajo.
    if (b.state === "error") return t("services.managed.error");
    const parts = [
      lts ? t("services.managed.lts") : null,
      b.state === "running"
        ? t("services.managed.running", { port: String(s.port) })
        : b.data_ready
          ? t("services.managed.stopped")
          : t("services.managed.firstStart"),
      b.autostart ? t("services.managed.atLogin") : null,
    ];
    return parts.filter(Boolean).join(" · ");
  }

  const isLts = (version: string) => releases.find((r) => r.version === version)?.lts ?? false;

  function footer(s: ManagedService): string {
    const ports = s.web_port
      ? t("services.managed.ports.web", { port: String(s.port), web: String(s.web_port) })
      : t("services.managed.ports.one", { port: String(s.port) });
    return catalog?.offline
      ? t("services.managed.footerOffline", { ports })
      : t("services.managed.footer", { ports, origin: t(`services.managed.origin.${s.service}`) });
  }

  // ── Herramientas ──

  const toolKey = (tool: ToolStatus) => `tool:${tool.tool}`;
  const latestTool = (tool: ToolStatus) => catalog?.tools.find((r) => r.tool === tool.tool) ?? null;

  async function installTool(tool: ToolStatus) {
    const key = toolKey(tool);
    downloading[key] = undefined;
    try {
      const version = await toolInstall(tool.tool, (e) => {
        if (e.bytes && e.bytes[1] > 0) downloading[key] = e.bytes[0] / e.bytes[1];
      });
      toast(
        tool.version
          ? t("services.tools.update.done", { name: tool.name, version })
          : t("services.tools.install.done", { name: tool.name, version }),
        { tone: "success" },
      );
    } catch (e) {
      fail(e);
    } finally {
      delete downloading[key];
    }
  }

  async function openTool(tool: ToolStatus) {
    try {
      await toolOpen(tool.tool);
    } catch (e) {
      fail(e);
    }
  }

  async function uninstallTool(tool: ToolStatus) {
    try {
      await toolUninstall(tool.tool);
      toast(t("services.tools.uninstall.done", { name: tool.name }), { tone: "success" });
    } catch (e) {
      fail(e);
    }
  }

  function toolMenu(tool: ToolStatus, at?: { x: number; y: number }) {
    const entries: MenuEntry[] = [];
    if (tool.tool === "phpmyadmin") {
      entries.push(
        {
          text: t("services.tools.menu.stopServer"),
          enabled: tool.running,
          action: () => void toolPower(tool.tool, false).catch(fail),
        },
        "separator",
      );
    }
    entries.push(
      {
        text: t("services.tools.menu.reveal"),
        enabled: !!tool.path,
        action: () => tool.path && void revealInFinder(tool.path).catch(fail),
      },
      "separator",
      { text: t("services.tools.uninstall.action"), action: () => void uninstallTool(tool) },
    );
    void popupMenu(entries, at).catch(fail);
  }

  function toolSubtitle(tool: ToolStatus): string {
    const busy = busyFor(snapshot, tool.target);
    if (busy) return busy.label;
    const problem = problemFor(snapshot, tool.target);
    if (problem) return problem.message;
    if (tool.external) return t("services.tools.external", { path: tildePath(tool.external) });
    if (!tool.version) {
      const release = latestTool(tool);
      if (!release) return catalogLoading ? "" : t("services.tools.unavailable");
      return [
        t("services.tools.notInstalled", { size: fmt.bytes(release.size) }),
        release.php ? t("services.tools.needsPhp", { range: release.php }) : null,
      ]
        .filter(Boolean)
        .join(" · ");
    }
    if (tool.tool === "pgadmin") return t("services.tools.app", { path: tildePath(tool.path ?? "") });
    if (!tool.php) return t("services.tools.needsPhp", { range: tool.php_range ?? "?" });
    return tool.running
      ? t("services.tools.running", { url: tool.url ?? "", php: tool.php })
      : t("services.tools.stopped", { php: tool.php });
  }

  function toolDot(tool: ToolStatus): DotState {
    if (busyFor(snapshot, tool.target)) return "busy";
    if (tool.blocked || problemFor(snapshot, tool.target)) return "error";
    return tool.running || (tool.tool === "pgadmin" && (tool.version || tool.external)) ? "running" : "stopped";
  }
</script>

{#each snapshot.managed as s (s.service)}
  {@const entries = mergeManagedVersions(s, releases)}
  {@const offer = s.import}
  <Group title={s.name} footer={footer(s)}>
    {#each entries as e (e.key)}
      {#if e.kind === "installed" && e.branch}
        {@const b = e.branch}
        {@const name = label(s, e.version.version)}
        {@const busy = busyFor(snapshot, b.label)}
        <Row title={name} subtitle={subtitle(s, b, isLts(e.version.version))}>
          {#snippet leading()}
            <StatusDot state={dot(b)} label={busy?.label ?? subtitle(s, b, false)} />
          {/snippet}
          {#snippet trailing()}
            {#if busy}<ProgressRing label={busy.label} />{/if}
            {#if e.update}
              {@const next = e.update.version}
              {#if next in downloading}
                <ProgressRing value={downloading[next]} label={t("services.managed.update.busy", { name, next })} />
              {:else}
                <Button variant="primary" disabled={!!busy} onclick={() => void update(s, e.version.version, next)}>
                  {t("services.managed.update.action", { next })}<span class="visually-hidden"> ({name})</span>
                </Button>
              {/if}
            {/if}
            {#if s.web_port && b.state === "running"}
              {@const url = `http://127.0.0.1:${s.web_port}`}
              <IconButton
                icon="open"
                label={t("services.managed.openWeb", { name: s.name })}
                onclick={() => void openUrl(url).catch(fail)}
              />
            {/if}
            <Switch
              checked={b.state === "running"}
              label={b.state === "running"
                ? t("services.managed.toggleStop", { name })
                : t("services.managed.toggleStart", { name })}
              disabled={!!busy}
              onchange={(on) => void power(s, b, on)}
            />
            <IconButton
              icon="more"
              label={t("services.managed.more", { name })}
              disabled={!!busy}
              onclick={(event) => menu(s, b, e.version.version, below(event))}
            />
          {/snippet}
        </Row>
        {#if b.state === "error" && b.error && !busy}
          <p class="note" role="listitem">{b.error}</p>
        {/if}
      {:else if e.kind === "installed"}
        {@const name = label(s, e.version.version)}
        <Row
          title={name}
          subtitle={t("services.managed.notUsed", { branch: e.version.branch, version: e.usedInstead ?? "" })}
        >
          {#snippet trailing()}
            <Button onclick={() => void uninstall(s, e.version.version, e.version.branch)}>
              {t("services.managed.uninstall.action")}<span class="visually-hidden"> {name}</span>
            </Button>
          {/snippet}
        </Row>
      {:else}
        {@const r = e.release}
        {@const name = label(s, r.version)}
        <Row
          title={name}
          subtitle={[r.lts ? t("services.managed.lts") : null, t("services.managed.notInstalled", { size: fmt.bytes(r.size) })]
            .filter(Boolean)
            .join(" · ")}
        >
          {#snippet leading()}<StatusDot state="stopped" label={t("services.managed.stopped")} />{/snippet}
          {#snippet trailing()}
            {#if r.version in downloading}
              <ProgressRing value={downloading[r.version]} label={t("services.managed.install.busy", { name })} />
            {:else}
              <Button onclick={() => void install(s, r.version)}>
                {t("services.managed.install.action")}<span class="visually-hidden"> {name}</span>
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
        <Row title={t("services.managed.catalog.errorTitle")} subtitle={t("services.managed.catalog.errorMessage")}>
          {#snippet trailing()}
            <Button onclick={() => void loadCatalog()}>{t("common.retry")}</Button>
          {/snippet}
        </Row>
      {/if}
    {/if}

    {#if offer}
      {@const target = s.branches.find((b) => b.branch === offer.branch)}
      {@const busy = target ? busyFor(snapshot, target.label) : undefined}
      {@const name = s.name}
      {@const hint = offer.manual
        ? t("services.managed.import.manual")
        : offer.blocked
          ? t("services.managed.import.blocked", { source: offer.source })
          : (!target
            ? t("services.managed.import.needsBranch", { name, branch: offer.branch })
            : target.state === "running"
              ? t("services.managed.import.stopFirst", { name, branch: offer.branch })
            : offer.replaces
              ? t("services.managed.import.replaces", { name, branch: offer.branch, source: offer.source })
              : t("services.managed.import.ready", { source: offer.source }))}
      <Row
        title={offer.manual
          ? t("services.managed.import.titleNoSize", { name, version: offer.version })
          : t("services.managed.import.title", { name, version: offer.version, size: fmt.bytes(offer.size) })}
        subtitle={busy?.label ?? hint}
      >
        {#snippet leading()}<Badge tone="orange" label="Homebrew" />{/snippet}
        {#snippet trailing()}
          {#if target && !offer.manual}
            <Button
              busy={!!busy}
              disabled={!!offer.blocked || target.state === "running"}
              onclick={() => void importData(s, offer.branch)}
            >
              {t("services.managed.import.action", { name, branch: offer.branch })}
            </Button>
          {/if}
        {/snippet}
      </Row>
      {#if offer.manual ?? offer.blocked}
        <p class="note" role="listitem">{offer.manual ?? offer.blocked}</p>
      {/if}
    {/if}
  </Group>
{/each}

{#if snapshot.managed_tools.length > 0}
  <Group title={t("services.tools.title")} footer={t("services.tools.footer")}>
    {#each snapshot.managed_tools as tool (tool.tool)}
      {@const busy = busyFor(snapshot, tool.target)}
      {@const latest = latestTool(tool)}
      {@const key = toolKey(tool)}
      {@const title = tool.version ? `${tool.name} ${tool.version}` : tool.name}
      <Row {title} subtitle={toolSubtitle(tool)}>
        {#snippet leading()}
          <StatusDot state={toolDot(tool)} label={busy?.label ?? toolSubtitle(tool)} />
        {/snippet}
        {#snippet trailing()}
          {#if busy}<ProgressRing label={busy.label} />{/if}
          {#if key in downloading}
            <ProgressRing
              value={downloading[key]}
              label={tool.version
                ? t("services.tools.update.busy", { name: tool.name, next: latest?.version ?? "" })
                : t("services.tools.install.busy", { name: tool.name })}
            />
          {:else if !tool.version && !tool.external && latest}
            <Button onclick={() => void installTool(tool)}>
              {t("services.tools.install.action")}<span class="visually-hidden"> {tool.name}</span>
            </Button>
          {:else if tool.version && latest && latest.version !== tool.version}
            <Button variant="primary" disabled={!!busy} onclick={() => void installTool(tool)}>
              {t("services.tools.update.action", { next: latest.version })}<span class="visually-hidden"> ({title})</span>
            </Button>
          {/if}
          {#if tool.blocked}
            <Button onclick={() => go("#/php")}>{t("services.tools.goToPhp")}</Button>
          {:else if tool.version || tool.external}
            <Button disabled={!!busy} onclick={() => void openTool(tool)}>
              {t("services.tools.open")}<span class="visually-hidden"> {tool.name}</span>
            </Button>
          {/if}
          {#if tool.version}
            <IconButton
              icon="more"
              label={t("services.tools.more", { name: tool.name })}
              disabled={!!busy}
              onclick={(event) => toolMenu(tool, below(event))}
            />
          {/if}
        {/snippet}
      </Row>
      {#if tool.warning}
        <p class="note" role="listitem">{tool.warning}</p>
      {/if}
    {/each}
  </Group>
{/if}

<style>
  /* Un texto largo que no cabe en el subtítulo de la fila de encima. */
  .note {
    margin: 0;
    padding: 0 var(--s-3) var(--s-3) calc(var(--s-3) + 16px + var(--s-3));
    font: var(--t-callout);
    color: var(--label-2);
    user-select: text;
    -webkit-user-select: text;
    overflow-wrap: anywhere;
  }
</style>
