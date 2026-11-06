<script lang="ts">
  // Ajustes ▸ Escenario (04 §4.8): el escenario guardado y, solo al abrir la
  // pestaña, si el Mac sigue como pide («Al día / N diferencias»).
  import { onMount } from "svelte";

  import * as ipc from "../../lib/ipc";
  import type { Plan } from "../../lib/bindings/Plan";
  import type { SetupStatus } from "../../lib/bindings/SetupStatus";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { formatShortDate } from "../../lib/format";
  import { t } from "../../lib/i18n/index.svelte";
  import { go } from "../../lib/router.svelte";
  import { DATABASE_NAMES, driftOf, inputFromSetupFile, scenarioName } from "../../lib/wizard/scenario";
  import Badge from "../../ui/Badge.svelte";
  import Button from "../../ui/Button.svelte";
  import Group from "../../ui/Group.svelte";
  import Icon from "../../ui/Icon.svelte";
  import ProgressRing from "../../ui/ProgressRing.svelte";
  import Row from "../../ui/Row.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";

  // Recibe { snapshot } como el resto de pestañas; aquí no hace falta.
  let { snapshot: _snapshot }: { snapshot: Snapshot | null } = $props();

  let status = $state.raw<SetupStatus | null>(null);
  let statusError: string | null = $state(null);
  let plan = $state.raw<Plan | null>(null);
  let planError: string | null = $state(null);
  let checking = $state(false);

  const file = $derived(status?.state ?? null);
  const drift = $derived(plan ? driftOf(plan) : null);

  // Solo se calcula al abrir la pestaña: detectar llama a brew y no es gratis.
  onMount(() => {
    void load();
  });

  async function load() {
    try {
      status = await ipc.setupStatus();
    } catch (e) {
      statusError = ipc.errorMessage(e);
      return;
    }
    const input = inputFromSetupFile(status.state);
    if (!input) return;
    checking = true;
    try {
      plan = await ipc.setupPreview(input);
    } catch (e) {
      planError = ipc.errorMessage(e);
    } finally {
      checking = false;
    }
  }

  function subtitle(): string | undefined {
    if (!file) return undefined;
    const parts: string[] = [];
    if (file.scenario === "laravel_services" && file.database) parts.push(DATABASE_NAMES[file.database]);
    if (file.completed_at) {
      const when = Date.parse(file.completed_at);
      if (!Number.isNaN(when)) parts.push(t("settings.scenario.applied", { date: formatShortDate(when) }));
    }
    return parts.length ? parts.join(" · ") : undefined;
  }
</script>

{#if statusError}
  <Group title={t("settings.scenario.title")}>
    <Row title={t("settings.scenario.readError")} subtitle={statusError} />
  </Group>
{:else if !status}
  <Skeleton lines={3} />
{:else if !file?.scenario}
  <Group
    title={t("settings.scenario.title")}
    footer={file ? t("settings.scenario.none.keep") : t("settings.scenario.none.never")}
  >
    <Row title={t("settings.scenario.empty")}>
      {#snippet leading()}<Icon name="sparkles" />{/snippet}
      {#snippet trailing()}
        <Button variant="primary" onclick={() => go("#/asistente/escenario")}>{t("settings.scenario.choose")}</Button>
      {/snippet}
    </Row>
  </Group>
{:else}
  <Group title={t("settings.scenario.title")} footer={t("settings.scenario.footer")}>
    <Row title={scenarioName(file.scenario)} subtitle={subtitle()}>
      {#snippet leading()}<Icon name="sparkles" />{/snippet}
      {#snippet trailing()}
        <span class="state" aria-live="polite">
          {#if checking}
            <ProgressRing size={12} label={t("settings.scenario.checking")} />
            <span class="muted">{t("settings.scenario.checkingEllipsis")}</span>
          {:else if drift}
            <Badge tone={drift.kind === "up_to_date" ? "green" : "orange"} label={drift.label} />
          {:else if planError}
            <span class="muted">{t("settings.scenario.unavailable")}</span>
          {/if}
        </span>
      {/snippet}
    </Row>
    {#if status.news}
      <Row title={t("settings.scenario.updates.title")} subtitle={t("settings.scenario.updates.subtitle")}>
        {#snippet leading()}<Icon name="info" />{/snippet}
      </Row>
    {/if}
  </Group>

  {#if plan && plan.steps.length}
    <Group title={t("settings.scenario.diff")}>
      {#each plan.steps as step (step.key)}
        <Row title={step.title} subtitle={step.optional ? t("settings.scenario.optional") : undefined} />
      {/each}
    </Group>
  {/if}

  {#if planError}
    <p class="error">{t("settings.scenario.checkFailed", { error: planError })}</p>
  {/if}

  <div class="actions">
    <Button onclick={() => go("#/asistente/escenario")}>{t("settings.scenario.change")}</Button>
    <Button variant="primary" onclick={() => go("#/asistente/revisar")}>{t("settings.scenario.reviewApply")}</Button>
  </div>
{/if}

<style>
  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
  }

  .muted {
    font: var(--t-callout);
    color: var(--label-2);
  }

  .error {
    margin-top: var(--s-3);
    font: var(--t-callout);
    color: var(--label-2);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
    margin-top: var(--group-gap);
  }
</style>
