<script lang="ts">
  // Servicios propios (06): una lista por servicio con las versiones instaladas
  // y las del catálogo, como la vista PHP. Instalar, actualizar a un parche,
  // encender o apagar (una rama a la vez), arranque al iniciar sesión,
  // desinstalar y copiar los datos de Homebrew. El estado sale del snapshot.
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
    revealInFinder,
    serviceAutostart,
    serviceCatalog,
    serviceImport,
    serviceInstall,
    servicePower,
    serviceUninstall,
    serviceUpdate,
  } from "../lib/ipc";
  import { below, busyFor, popupMenu, tildePath, type MenuEntry } from "../lib/stores/app.svelte";
  import { mergeManagedVersions } from "../lib/managed-services";
  import { fmt, t } from "../lib/i18n/index.svelte";
  import type { BranchStatus } from "../lib/bindings/BranchStatus";
  import type { InstallProgress } from "../lib/bindings/InstallProgress";
  import type { ManagedService } from "../lib/bindings/ManagedService";
  import type { ServicesCatalog } from "../lib/bindings/ServicesCatalog";
  import type { Snapshot } from "../lib/bindings/Snapshot";

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
      toast(
        report.backup
          ? t("services.managed.import.doneWithBackup", { branch, backup: tildePath(report.backup) })
          : t("services.managed.import.done", { branch }),
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
    if (b.state === "error" && b.error) return b.error;
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
</script>

{#each snapshot.managed as s (s.service)}
  {@const entries = mergeManagedVersions(s, releases)}
  {@const offer = s.import}
  <Group
    title={s.name}
    footer={catalog?.offline
      ? t("services.managed.footerOffline", { port: String(s.port) })
      : t("services.managed.footer", { port: String(s.port) })}
  >
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
      {@const hint = offer.blocked
        ?? (!target
          ? t("services.managed.import.needsBranch", { branch: offer.branch })
          : target.state === "running"
            ? t("services.managed.import.stopFirst", { branch: offer.branch })
          : offer.replaces
            ? t("services.managed.import.replaces", { branch: offer.branch, source: offer.source })
            : t("services.managed.import.ready", { source: offer.source }))}
      <Row
        title={t("services.managed.import.title", { version: offer.version, size: fmt.bytes(offer.size) })}
        subtitle={busy?.label ?? hint}
      >
        {#snippet leading()}<Badge tone="orange" label="Homebrew" />{/snippet}
        {#snippet trailing()}
          {#if target}
            <Button
              busy={!!busy}
              disabled={!!offer.blocked || target.state === "running"}
              onclick={() => void importData(s, offer.branch)}
            >
              {t("services.managed.import.action", { branch: offer.branch })}
            </Button>
          {/if}
        {/snippet}
      </Row>
    {/if}
  </Group>
{/each}
