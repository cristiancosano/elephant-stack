<script lang="ts">
  // Personalizar (04 §4.11 y C10): servicio a servicio con el control de tres
  // estados, ramas de PHP, versión por defecto y los interruptores, con el total
  // de RAM y descarga en directo (lo calcula Rust con el mismo plan de la revisión).
  import * as ipc from "../../lib/ipc";
  import type { Plan } from "../../lib/bindings/Plan";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { formatBytes, plural } from "../../lib/format";
  import { wizard } from "../../lib/stores/wizard.svelte";
  import {
    SERVICE_WANT_OPTIONS,
    TOGGLES,
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
    return c?.installed ? (c.version ?? "instalado") : null;
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
    <h1 class="title">{choiceLabel(s, wizard.cards ?? []) ?? "Personalizado"}</h1>
    <p class="lead">
      {#if draft?.basedOn}
        Parte de lo que pide el escenario. Lo que cambies aquí lo convierte en Personalizado.
      {:else}
        Parte de lo que ya hay en tu Mac: si no tocas nada, el plan sale vacío.
      {/if}
    </p>
  </header>

  {#if profile}
    <Group
      title="Servicios"
      footer="«Instalado y apagado» no consume nada hasta que lo enciendes. El asistente nunca desinstala ni apaga nada."
    >
      {#each rows as row (row.id)}
        {@const label = serviceLabel(row.id)}
        {@const version = installedVersion(row.id)}
        <Row
          title={label.name}
          subtitle={[label.purpose, version ? `Instalado: ${version}` : null].filter(Boolean).join(" · ") || undefined}
        >
          {#snippet trailing()}
            <SegmentedControl
              label="Qué hacer con {label.name}"
              options={SERVICE_WANT_OPTIONS}
              value={row.want}
              onchange={(want) => wizard.edit(setService(profile, row.id, want))}
            />
          {/snippet}
        </Row>
      {/each}
    </Group>

    <Group
      title="PHP"
      footer={offline
        ? "Sin conexión: solo se pueden elegir las versiones que ya tienes."
        : "Las versiones de Elephant se descargan de php-builds y traen las extensiones de Laravel."}
    >
      {#if branches.length === 0}
        <Row title="No hay versiones disponibles" subtitle="Sin conexión ni versiones propias instaladas." />
      {/if}
      {#each branches as b (b.branch)}
        {@const on = checked.has(b.branch)}
        <Row
          title="PHP {b.branch}"
          subtitle={b.installed
            ? `${b.installed} ya instalada`
            : b.latest
              ? `Se descargará ${b.latest}${b.size ? ` · ${formatBytes(b.size)}` : ""}`
              : undefined}
        >
          {#snippet trailing()}
            <input
              type="checkbox"
              class="check"
              aria-label="Usar PHP {b.branch}"
              checked={on}
              onchange={(e) => wizard.edit(setBranch(profile, branches, b.branch, e.currentTarget.checked))}
            />
          {/snippet}
        </Row>
      {/each}
    </Group>

    {#if defaults.length > 1}
      <Group title="Versión por defecto" footer="La que usa la terminal fuera de un proyecto con .php-version.">
        <!-- Radios nativos con el mismo `name`: las flechas ya se mueven entre ellos. -->
        {#each defaults as option (option.value ?? "auto")}
          <Row title={option.label}>
            {#snippet trailing()}
              <input
                type="radio"
                class="check"
                name="default-php"
                aria-label="Por defecto: {option.label}"
                checked={profile.default_php === option.value}
                onchange={() => wizard.edit(setDefaultPhp(profile, option.value))}
              />
            {/snippet}
          </Row>
        {/each}
      </Group>
    {/if}

    <Group title="Herramientas">
      {#each TOGGLES as t (t.key)}
        <Row title={t.title} subtitle={t.subtitle}>
          {#snippet trailing()}
            <Switch
              label={t.title}
              checked={profile[t.key]}
              onchange={(v) => wizard.edit(setToggle(profile, t.key, v))}
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
    <span>{live.totals.download_bytes > 0 ? `${formatBytes(live.totals.download_bytes)} de descarga` : "Sin descargas"}</span>
    <span>{live.steps.length === 0 ? "Sin cambios" : plural(live.steps.length, "paso")}</span>
  {:else}
    <span>Calculando…</span>
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
