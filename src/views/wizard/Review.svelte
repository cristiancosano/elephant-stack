<script lang="ts">
  // Revisión (04 §4.11 paso 3): el plan entero antes de ejecutar nada.
  import type { Step } from "../../lib/bindings/Step";
  import { formatBytes } from "../../lib/format";
  import { wizard } from "../../lib/stores/wizard.svelte";
  import { choiceLabel } from "../../lib/wizard/machine";
  import {
    isEmptyPlan,
    optionalNote,
    optionalSteps,
    reviewSections,
    stepBytes,
    totalsLine,
    visibleNotices,
  } from "../../lib/wizard/review";
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import Group from "../../ui/Group.svelte";
  import Icon from "../../ui/Icon.svelte";
  import Row from "../../ui/Row.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";
  import PathNotice from "./PathNotice.svelte";

  const s = $derived(wizard.s);
  const plan = $derived(s.plan);
  const label = $derived(choiceLabel(s, wizard.cards ?? []) ?? "");
  const sections = $derived(plan ? reviewSections(plan) : []);
  const optionals = $derived(plan ? optionalSteps(plan) : []);
  const accepted = $derived(new Set(s.accepted));
  const hasCommands = $derived(plan?.steps.some((x) => x.command) ?? false);

  let showCommands = $state(false);

  function subtitle(step: Step, extra: string | null = null): string | undefined {
    const parts: string[] = [];
    if (extra) parts.push(extra);
    const bytes = stepBytes(step);
    if (bytes > 0) parts.push(formatBytes(bytes));
    if (showCommands && step.command) parts.push(step.command);
    return parts.length ? parts.join(" · ") : undefined;
  }
</script>

<section class="review">
  <header class="head">
    <h1 class="title">Revisa los cambios</h1>
    <p class="lead">
      <span class="choice">{label}</span>
      {#if plan}<span class="num"> · {totalsLine(plan)}</span>{/if}
    </p>
  </header>

  {#if s.stale}
    <p class="banner" role="alert">
      <Icon name="warning" />
      <span>Tu Mac ha cambiado desde la revisión anterior y no se ha aplicado nada. Este es el plan actualizado.</span>
    </p>
  {/if}

  {#if wizard.planError}
    <EmptyState
      icon="warning"
      title="No se ha podido calcular el plan"
      message={wizard.planError}
      action={{ label: "Reintentar", onclick: () => wizard.retryPlan() }}
    />
  {:else if !plan}
    <div class="loading" aria-busy="true">
      <p class="visually-hidden">Calculando el plan…</p>
      <Skeleton lines={6} />
    </div>
  {:else}
    {#if isEmptyPlan(plan)}
      <div class="empty">
        <span class="done" aria-hidden="true"><Icon name="check" size={20} /></span>
        <p class="empty-title">Tu Mac ya está como pide <em>{label}</em></p>
        <p class="empty-text">No hay nada que instalar ni que cambiar. Pulsa «Hecho» para terminar.</p>
      </div>
    {/if}

    {#each sections as section (section.id)}
      <Group title={section.title} footer={section.footer ?? undefined}>
        {#each section.steps as step (step.key)}
          <Row title={step.title} subtitle={subtitle(step)} />
        {/each}
      </Group>
    {/each}

    {#if optionals.length}
      <Group title="Cambios opcionales" footer="Vienen desmarcados: solo se hacen si los marcas.">
        {#each optionals as step (step.key)}
          <Row title={step.title} subtitle={subtitle(step, optionalNote(step))}>
            {#snippet trailing()}
              <input
                type="checkbox"
                class="check"
                aria-label={step.title}
                checked={accepted.has(step.key)}
                onchange={(e) => wizard.toggle(step.key, e.currentTarget.checked)}
              />
            {/snippet}
          </Row>
        {/each}
      </Group>
    {/if}

    {#if plan.satisfied.length}
      <div class="muted">
        <Group title="Ya lo tienes">
          {#each plan.satisfied as item (item)}
            <Row title={item}>
              {#snippet leading()}<span class="satisfied"><Icon name="check" /></span>{/snippet}
            </Row>
          {/each}
        </Group>
      </div>
    {/if}

    {#if plan.conflicts.length}
      <Group title="No se hará">
        {#each plan.conflicts as c (c.key)}
          <Row title={c.reason}>
            {#snippet leading()}<span class="conflict"><Icon name="warning" /></span>{/snippet}
          </Row>
        {/each}
      </Group>
    {/if}

    {@const notices = visibleNotices(plan, wizard.shell !== null)}
    {#if notices.length}
      <Group title="Avisos">
        {#each notices as notice (notice)}
          <Row title={notice}>
            {#snippet leading()}<span class="notice"><Icon name="info" /></span>{/snippet}
          </Row>
        {/each}
      </Group>
    {/if}

    <PathNotice />

    {#if hasCommands}
      <div class="commands">
        <Button variant="plain" onclick={() => (showCommands = !showCommands)}>
          {showCommands ? "Ocultar comandos" : "Ver comandos"}
        </Button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .review {
    display: flex;
    flex-direction: column;
    gap: var(--group-gap);
    padding-top: var(--s-4);
  }

  /* El espacio entre bloques lo da el gap: se anula el margen que Group pone
     entre dos grupos seguidos (doble clase para ganar en especificidad). */
  .review :global(.group.group) {
    margin-top: 0;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .title {
    font: var(--t-hero);
  }

  .lead {
    color: var(--label-2);
  }

  .choice {
    font: var(--t-headline);
    color: var(--label);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-3);
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--orange) 14%, transparent);
  }

  .banner :global(svg) {
    color: var(--orange);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-6) var(--s-4);
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
    text-align: center;
  }

  .done {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--r-pill);
    background: var(--green);
    color: var(--on-hero);
  }

  .empty-title {
    font: var(--t-title);
  }

  .empty-title em {
    font-style: normal;
    color: var(--accent);
  }

  .empty-text {
    color: var(--label-2);
  }

  .check {
    margin: 0;
    accent-color: var(--accent);
  }

  .satisfied {
    display: inline-flex;
    color: var(--green);
  }

  .conflict {
    display: inline-flex;
    color: var(--orange);
  }

  .notice {
    display: inline-flex;
    color: var(--label-2);
  }

  /* «Ya lo tienes» va en gris (04 §4.11). */
  .muted :global(.row .title) {
    color: var(--label-2);
  }

  .commands {
    margin-left: calc(-1 * var(--s-3));
  }
</style>
