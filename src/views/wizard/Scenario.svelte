<script lang="ts">
  // Elección de escenario (04 §4.11 paso 2, 01 §7.8 y C10): tres tarjetas de
  // selección única y, debajo, «Personalizado».
  import type { Database } from "../../lib/bindings/Database";
  import type { ScenarioCard } from "../../lib/bindings/ScenarioCard";
  import type { ScenarioId } from "../../lib/bindings/ScenarioId";
  import { formatBytes, plural } from "../../lib/format";
  import { wizard } from "../../lib/stores/wizard.svelte";
  import { choiceLabel, type Preset } from "../../lib/wizard/machine";
  import { DATABASE_NAMES, idleRam } from "../../lib/wizard/scenario";
  import Badge from "../../ui/Badge.svelte";
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import Group from "../../ui/Group.svelte";
  import Icon from "../../ui/Icon.svelte";
  import type { IconName } from "../../ui/icons";
  import Row from "../../ui/Row.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";

  const ICONS: Record<ScenarioId, IconName> = {
    laravel: "sitios",
    laravel_services: "database",
    terminal: "terminal",
    custom: "ajustes",
  };

  const s = $derived(wizard.s);
  const cards = $derived((wizard.cards ?? []).filter((c): c is ScenarioCard & { id: Preset } => c.id !== "custom"));
  /** La tarjeta marcada. Con un «Personalizado» elegido, ninguna. */
  const selected = $derived(s.scenario);
  const customLabel = $derived(s.scenario === "custom" ? choiceLabel(s, wizard.cards ?? []) : null);

  let radios: HTMLButtonElement[] = $state([]);

  function focusable(i: number): boolean {
    const index = cards.findIndex((c) => c.id === selected);
    return index === -1 ? i === 0 : index === i;
  }

  function onkeydown(event: KeyboardEvent, i: number) {
    const n = cards.length;
    let next: number;
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        next = (i + 1) % n;
        break;
      case "ArrowLeft":
      case "ArrowUp":
        next = (i - 1 + n) % n;
        break;
      case "Enter":
        // Elegir y continuar, como Intro en un diálogo de macOS.
        event.preventDefault();
        wizard.select(cards[i].id);
        wizard.review();
        return;
      default:
        return;
    }
    event.preventDefault();
    wizard.select(cards[next].id);
    radios[next]?.focus();
  }

  const dbName = (d: ScenarioCard["databases"][number]) => d.label || DATABASE_NAMES[d.database];

  function databaseOptions(card: ScenarioCard): { value: Database; label: string }[] {
    return card.databases.map((d) => ({ value: d.database, label: dbName(d) }));
  }

  /** El coste de cada opción, bajo el control (en la tarjeta no cabe dentro). */
  function databaseCosts(card: ScenarioCard): string {
    return card.databases.map((d) => `${dbName(d)} ≈ ${d.idle_ram_mb} MB`).join(" · ");
  }
</script>

<section class="scenario">
  <header class="head">
    <h1 class="title">¿Cómo trabajas?</h1>
    <p class="lead">Elige un punto de partida. Podrás revisar cada cambio antes de aplicarlo, y nada se desinstala.</p>
  </header>

  {#if wizard.cards === null && !wizard.cardsError}
    <div class="cards" aria-busy="true">
      {#each { length: 3 }, i (i)}
        <div class="card"><Skeleton lines={5} /></div>
      {/each}
    </div>
  {:else if cards.length === 0}
    <EmptyState
      icon="warning"
      title="No se han podido preparar los escenarios"
      message={wizard.cardsError ?? "Puedes elegir los componentes uno a uno en Personalizado."}
      action={{ label: "Reintentar", onclick: () => wizard.retryCards() }}
    />
  {:else}
    <div class="cards" role="radiogroup" aria-label="Escenario">
      {#each cards as card, i (card.id)}
        {@const isSelected = selected === card.id}
        <div class="card" class:selected={isSelected}>
          <button
            bind:this={radios[i]}
            type="button"
            class="pick"
            role="radio"
            aria-checked={isSelected}
            tabindex={focusable(i) ? 0 : -1}
            onclick={() => wizard.select(card.id)}
            onkeydown={(e) => onkeydown(e, i)}
          >
            <span class="icon" aria-hidden="true"><Icon name={ICONS[card.id]} size={20} /></span>
            <span class="check" aria-hidden="true"><Icon name="check" /></span>
            <span class="name">{card.name}</span>
            {#if card.recommended}<span class="badge"><Badge tone="accent" label="Recomendado" /></span>{/if}
            <span class="phrase">{card.phrase}</span>
            {#if card.components.length}
              <span class="components">{card.components.join(" · ")}</span>
            {/if}
            <span class="figures num">
              <span>{idleRam(card.idle_ram_mb)}</span>
              <span>{card.download_bytes > 0 ? `${formatBytes(card.download_bytes)} de descarga` : "Sin descargas"}</span>
              <span>{card.steps === 0 ? "Ya lo tienes todo" : plural(card.steps, "paso")}</span>
            </span>
          </button>

          {#if card.databases.length > 1}
            <div class="database">
              <SegmentedControl
                label="Base de datos de {card.name}"
                options={databaseOptions(card)}
                value={wizard.database}
                onchange={(d) => wizard.setDatabase(d)}
              />
              <span class="costs num">Encendida: {databaseCosts(card)}</span>
            </div>
          {/if}

          <div class="adjust">
            <Button variant="plain" disabled={wizard.busy !== null} onclick={() => void wizard.customize(card.id)}>
              Ajustar…
            </Button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <div class="custom">
    <Group footer="Elige servicio a servicio. Empieza con lo que ya tienes: si no tocas nada, no cambia nada.">
      <Row
        title={customLabel ?? "Personalizado"}
        subtitle={customLabel ? "Elegido" : undefined}
        selected={s.scenario === "custom"}
        onactivate={() => void wizard.customize(null)}
      >
        {#snippet leading()}<Icon name="ajustes" />{/snippet}
        {#snippet trailing()}<Icon name="chevron-right" />{/snippet}
      </Row>
    </Group>
  </div>
</section>

<style>
  .scenario {
    display: flex;
    flex-direction: column;
    gap: var(--s-5);
    padding-top: var(--s-4);
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

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: var(--s-3);
  }

  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-4);
    border-radius: var(--r-lg);
    background: var(--group);
    box-shadow: var(--shadow-1);
    /* El borde de 2 px se reserva siempre: elegir una tarjeta no mueve nada. */
    outline: 2px solid transparent;
    outline-offset: -2px;
    transition: outline-color var(--d-2) var(--ease);
  }

  .card.selected {
    outline-color: var(--accent);
  }

  .pick {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--s-1) var(--s-2);
    padding: 0;
    border: none;
    background: none;
    text-align: left;
  }

  .pick:focus-visible {
    border-radius: var(--r-sm);
  }

  .icon {
    display: inline-flex;
    width: 32px;
    height: 32px;
    align-items: center;
    justify-content: center;
    border-radius: var(--r-md);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .check {
    grid-column: 3;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: var(--r-pill);
    background: var(--accent);
    color: var(--accent-text);
    transform: scale(0);
    transition: transform var(--d-2) var(--ease);
  }

  .selected .check {
    transform: scale(1);
  }

  .name {
    grid-column: 1 / -1;
    margin-top: var(--s-2);
    font: var(--t-headline);
  }

  .badge {
    grid-column: 1 / -1;
  }

  .phrase {
    grid-column: 1 / -1;
    color: var(--label);
  }

  .components {
    grid-column: 1 / -1;
    font: var(--t-callout);
    color: var(--label-2);
  }

  .figures {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: var(--s-2);
    font: var(--t-callout);
    color: var(--label-2);
  }

  .database {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--s-1);
  }

  .costs {
    font: var(--t-caption);
    color: var(--label-2);
  }

  .adjust {
    margin-top: auto;
    margin-left: calc(-1 * var(--s-3));
  }
</style>
