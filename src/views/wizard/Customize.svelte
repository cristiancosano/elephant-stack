<script lang="ts">
  // Personalizar (04 §4.11 y C10): servicio a servicio con el control de tres
  // estados, ramas de PHP, versión por defecto y los interruptores, con el total
  // de RAM y descarga en directo (lo calcula Rust con el mismo plan de la revisión).
  import * as ipc from "../../lib/ipc";
  import type { Plan } from "../../lib/bindings/Plan";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { formatBytes } from "../../lib/format";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/stores/wizard.svelte";
  import {
    serviceWantOptions,
    toggles,
    checkedBranches,
    defaultPhpOptions,
    phpBranches,
    serviceLabel,
    serviceRows,
    setBranch,
    setDefaultPhp,
    setService,
    setToggle,
  } from "../../lib/wizard/customize";
  import { choiceLabel, currentInput, inputKey } from "../../lib/wizard/machine";
  import { idleRam } from "../../lib/wizard/scenario";
  import Group from "../../ui/Group.svelte";
  import Row from "../../ui/Row.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Switch from "../../ui/Switch.svelte";

  let { snapshot }: { snapshot: Snapshot | null } = $props();

  const s = $derived(wizard.s);
  const draft = $derived(s.draft);
  const profile = $derived(draft?.profile ?? null);
  const branches = $derived(phpBranches(wizard.catalog, wizard.detection));
  const checked = $derived(profile ? new Set(checkedBranches(profile, branches)) : new Set<string>());
  const defaults = $derived(profile ? defaultPhpOptions(profile, branches) : []);
  const rows = $derived(profile ? serviceRows(profile) : []);
  const offline = $derived(wizard.catalog?.offline ?? false);

  /** Versión de cada servicio instalado, para el subtítulo. */
  function installedVersion(id: string): string | null {
    const c = snapshot?.components.find((x) => x.id === id);
    return c?.installed ? (c.version ?? t("wizard.customize.services.installedUnknown")) : null;
  }

  // Total en directo: se pide a Rust 300 ms después del último cambio.
  let live: Plan | null = $state.raw(null);
  let liveError: string | null = $state(null);
  let liveKey = "";
  $effect(() => {
    const input = currentInput(s);
    if (!input) return;
    const key = inputKey(input);
    if (key === liveKey) return;
    const timer = setTimeout(() => {
      liveKey = key;
      ipc
        .setupPreview(input)
        .then((p) => {
          if (liveKey === key) {
            live = p;
            liveError = null;
          }
        })
        .catch((e: unknown) => {
          if (liveKey === key) liveError = ipc.errorMessage(e);
        });
    }, 300);
    return () => clearTimeout(timer);
  });
</script>

<section class="customize">
  <header class="head">
    <h1 class="title">{choiceLabel(s, wizard.cards ?? []) ?? t("wizard.scenarios.custom")}</h1>
    <p class="lead">
      {#if draft?.basedOn}
        {t("wizard.customize.intro.fromScenario")}
      {:else}
        {t("wizard.customize.intro.fromMac")}
      {/if}
    </p>
  </header>

  {#if profile}
    <Group
      title={t("wizard.customize.services.title")}
      footer={t("wizard.customize.services.footer")}
    >
      {#each rows as row (row.id)}
        {@const label = serviceLabel(row.id)}
        {@const version = installedVersion(row.id)}
        <Row
          title={label.name}
          subtitle={[label.purpose, version ? t("wizard.customize.services.installed", { version }) : null]
            .filter(Boolean)
            .join(" · ") || undefined}
        >
          {#snippet trailing()}
            <SegmentedControl
              label={t("wizard.customize.services.actionLabel", { name: label.name })}
              options={serviceWantOptions()}
              value={row.want}
              onchange={(want) => wizard.edit(setService(profile, row.id, want))}
            />
          {/snippet}
        </Row>
      {/each}
    </Group>

    <Group
      title={t("wizard.customize.php.title")}
      footer={offline ? t("wizard.customize.php.offline") : t("wizard.customize.php.footer")}
    >
      {#if branches.length === 0}
        <Row title={t("wizard.customize.php.emptyTitle")} subtitle={t("wizard.customize.php.emptyMessage")} />
      {/if}
      {#each branches as b (b.branch)}
        {@const on = checked.has(b.branch)}
        <Row
          title={t("wizard.customize.php.branch", { branch: b.branch })}
          subtitle={b.installed
            ? t("wizard.customize.php.installed", { version: b.installed })
            : b.latest
              ? `${t("wizard.customize.php.willDownload", { version: b.latest })}${b.size ? ` · ${formatBytes(b.size)}` : ""}`
              : undefined}
        >
          {#snippet trailing()}
            <input
              type="checkbox"
              class="check"
              aria-label={t("wizard.customize.php.use", { branch: b.branch })}
              checked={on}
              onchange={(e) => wizard.edit(setBranch(profile, branches, b.branch, e.currentTarget.checked))}
            />
          {/snippet}
        </Row>
      {/each}
    </Group>

    {#if defaults.length > 1}
      <Group title={t("wizard.customize.default.title")} footer={t("wizard.customize.default.footer")}>
        <!-- Radios nativos con el mismo `name`: las flechas ya se mueven entre ellos. -->
        {#each defaults as option (option.value ?? "auto")}
          <Row title={option.label}>
            {#snippet trailing()}
              <input
                type="radio"
                class="check"
                name="default-php"
                aria-label={t("wizard.customize.default.label", { label: option.label })}
                checked={profile.default_php === option.value}
                onchange={() => wizard.edit(setDefaultPhp(profile, option.value))}
              />
            {/snippet}
          </Row>
        {/each}
      </Group>
    {/if}

    <Group title={t("wizard.customize.tools.title")}>
      {#each toggles() as toggle (toggle.key)}
        <Row title={toggle.title} subtitle={toggle.subtitle}>
          {#snippet trailing()}
            <Switch
              label={toggle.title}
              checked={profile[toggle.key]}
              onchange={(v) => wizard.edit(setToggle(profile, toggle.key, v))}
            />
          {/snippet}
        </Row>
      {/each}
    </Group>
  {/if}
</section>

<div class="summary num" aria-live="polite">
  {#if liveError}
    <span class="error">{liveError}</span>
  {:else if live}
    <span>{idleRam(live.totals.idle_ram_mb)}</span>
    <span
      >{live.totals.download_bytes > 0
        ? t("wizard.scenario.download", { size: formatBytes(live.totals.download_bytes) })
        : t("wizard.scenario.noDownload")}</span
    >
    <span
      >{live.steps.length === 0
        ? t("wizard.customize.noChanges")
        : t("wizard.scenario.steps", { count: live.steps.length })}</span
    >
  {:else}
    <span>{t("wizard.customize.calculating")}</span>
  {/if}
</div>

<style>
  .customize {
    display: flex;
    flex-direction: column;
    padding-top: var(--s-4);
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    margin-bottom: var(--s-5);
  }

  .title {
    font: var(--t-hero);
  }

  .lead {
    color: var(--label-2);
  }

  .check {
    margin: 0;
    accent-color: var(--accent);
  }

  /* Resumen fijo abajo, siempre a la vista mientras se edita. */
  .summary {
    position: sticky;
    bottom: 0;
    display: flex;
    justify-content: center;
    gap: var(--s-4);
    margin: var(--s-5) calc(-1 * var(--content-padding)) calc(-1 * var(--s-8));
    padding: var(--s-2) var(--content-padding);
    border-top: 0.5px solid var(--separator);
    background: color-mix(in srgb, var(--window) 92%, transparent);
    -webkit-backdrop-filter: blur(12px);
    backdrop-filter: blur(12px);
    font: var(--t-callout);
    color: var(--label-2);
  }

  .error {
    color: var(--red);
  }
</style>
