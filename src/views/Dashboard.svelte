<script lang="ts">
  // Panel (01 §7.2): el héroe con el estado del stack y cuatro tarjetas (PHP,
  // servicios, errores recientes y Mailpit), más los sitios cuando los hay.
  import Gauge from "../ui/Gauge.svelte";
  import Button from "../ui/Button.svelte";
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import Select from "../ui/Select.svelte";
  import StatusDot from "../ui/StatusDot.svelte";
  import Switch from "../ui/Switch.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import Icon from "../ui/Icon.svelte";
  import { toast } from "../ui/toasts.svelte";
  import {
    errorMessage,
    logsQuery,
    openUrl,
    refresh,
    serviceOperation,
    setGlobalPhp,
    siteAction,
  } from "../lib/ipc";
  import {
    brewBusy,
    gaugeMaxKb,
    hero,
    phpOptions,
    problemRoute,
    runningPids,
    serviceState,
    serviceStatusText,
    services,
    sites,
    startableServices,
  } from "../lib/stores/app.svelte";
  import { totalMemoryKb, usage } from "../lib/stores/usage.svelte";
  import { go } from "../lib/router.svelte";
  import { formatMemory, formatRelative, plural } from "../lib/format";
  import type { LogEntry } from "../lib/bindings/LogEntry";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";

  let { snapshot, route: _route }: { snapshot: Snapshot | null; route: Route } = $props();

  /** A partir de cuántas versiones el control segmentado no cabe y pasa a ser un menú. */
  const SEGMENTED_MAX = 4;
  const HOUR_MS = 3_600_000;

  const memoryKb = $derived(snapshot ? totalMemoryKb(runningPids(snapshot), usage.byPid) : 0);
  const heroState = $derived(snapshot ? hero(snapshot, memoryKb) : null);
  const locked = $derived(snapshot ? brewBusy(snapshot) : false);
  const phpChoices = $derived(phpOptions(snapshot?.php.versions ?? []));
  const activeKey = $derived(snapshot?.php.active?.key ?? "");
  /** Los que el usuario usa: arrancan al iniciar sesión o están en marcha. */
  const quick = $derived(
    snapshot
      ? services(snapshot)
          .filter((c) => c.service && (c.service.registered || c.service.running))
          .slice(0, 5)
      : [],
  );
  const siteList = $derived(snapshot ? sites(snapshot).slice(0, 5) : []);
  const mailpit = $derived(snapshot?.mailpit ?? null);

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  async function heroAction() {
    if (!snapshot || !heroState?.action) return;
    if (heroState.action.kind === "problem") {
      const problem = snapshot.problems[0];
      go(problem ? problemRoute(snapshot, problem.target) : "#/servicios");
    } else if (heroState.action.kind === "services") {
      go("#/servicios");
    } else {
      // Homebrew hace una cosa cada vez: se arrancan en orden.
      for (const id of startableServices(snapshot)) {
        try {
          await serviceOperation(id, "turn_on");
        } catch (e) {
          fail(e);
          break;
        }
      }
    }
  }

  async function operate(name: string, on: boolean) {
    try {
      await serviceOperation(name, on ? "turn_on" : "turn_off");
      setTimeout(() => void refresh().catch(() => {}), 2000);
    } catch (e) {
      fail(e);
    }
  }

  async function setDefaultPhp(key: string) {
    try {
      await setGlobalPhp(key);
    } catch (e) {
      fail(e);
    }
  }

  // ── Errores recientes ──
  // Se vuelven a pedir solo cuando cambia el resumen de logs del snapshot.
  let errors = $state<LogEntry[]>([]);
  let errorsAvailable = $state(true);
  const logsKey = $derived(
    snapshot
      ? `${snapshot.logs.available}:${snapshot.logs.unseen_errors}:${snapshot.logs.recent.map((r) => r.last_ms).join(",")}`
      : "",
  );

  const logsAvailable = $derived(snapshot?.logs.available ?? false);

  $effect(() => {
    // Solo depende de valores primitivos: un snapshot nuevo sin cambios en los
    // logs no repite la consulta.
    if (!logsKey || !logsAvailable) {
      errors = [];
      errorsAvailable = logsAvailable;
      return;
    }
    let cancelled = false;
    logsQuery({
      projects: [],
      kinds: [],
      min_level: "error",
      text: null,
      status: null,
      since_ms: Date.now() - HOUR_MS,
      until_ms: null,
      before: null,
      limit: 3,
    })
      .then((page) => {
        if (cancelled) return;
        errors = page.entries.slice(0, 3);
        errorsAvailable = true;
      })
      .catch(() => {
        if (cancelled) return;
        errors = [];
        errorsAvailable = false;
      });
    return () => {
      cancelled = true;
    };
  });

  function projectName(id: string | null): string {
    if (!id) return "Sistema";
    return snapshot?.projects.find((p) => p.id === id)?.name ?? "Proyecto";
  }

  function logsHref(entry: LogEntry): string {
    const params = new URLSearchParams({ level: "error", range: "1h" });
    if (entry.project) params.set("project", entry.project);
    return `#/logs?${params}`;
  }
</script>

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>Panel</h1>
    <IconButton icon="restart" label="Actualizar" onclick={() => void refresh().catch(fail)} />
  </header>

  <div class="scroll">
    <div class="content">
      {#if !snapshot || !heroState}
        <div class="hero ok"><Skeleton lines={2} /></div>
      {:else}
        <section class="hero {heroState.tone}" aria-label="Estado">
          <div class="hero-text">
            <h2 class="hero-title" aria-live="polite">{heroState.title}</h2>
            <p class="hero-detail num">{heroState.detail}</p>
            {#if heroState.action}
              <div class="hero-action">
                <Button disabled={heroState.action.kind === "start" && locked} onclick={() => void heroAction()}>
                  {heroState.action.label}
                </Button>
              </div>
            {/if}
          </div>
          <Gauge
            value={memoryKb}
            max={gaugeMaxKb(memoryKb)}
            label={memoryKb > 0 ? formatMemory(memoryKb) : "—"}
            caption="RAM"
          />
        </section>

        <div class="grid">
          <section class="card" aria-labelledby="card-php">
            <h2 class="card-title" id="card-php">PHP por defecto</h2>
            {#if phpChoices.length === 0}
              <p class="muted">No hay ninguna versión instalada.</p>
              <a class="link" href="#/php">Ir a PHP</a>
            {:else if phpChoices.length <= SEGMENTED_MAX}
              <SegmentedControl
                options={phpChoices}
                value={activeKey}
                label="Versión de PHP por defecto"
                onchange={(key) => void setDefaultPhp(key)}
              />
            {:else}
              <Select
                options={phpChoices}
                value={activeKey}
                label="Versión de PHP por defecto"
                onchange={(key) => void setDefaultPhp(key)}
              />
            {/if}
            {#if snapshot.php.active}
              <p class="muted">{snapshot.php.active.id} · {snapshot.php.active.source}</p>
            {/if}
          </section>

          <section class="card" aria-labelledby="card-services">
            <h2 class="card-title" id="card-services">Servicios</h2>
            {#if quick.length === 0}
              <p class="muted">Ningún servicio está en marcha ni arranca al iniciar sesión.</p>
              <a class="link" href="#/servicios">Ver servicios</a>
            {:else}
              <ul class="lines">
                {#each quick as c (c.id)}
                  {#if c.service}
                    {@const service = c.service}
                    <li class="line">
                      <StatusDot state={serviceState(service)} label={serviceStatusText(service)} />
                      <span class="line-text">{c.id}</span>
                      <Switch
                        checked={service.running}
                        label={`${service.running ? "Detener" : "Arrancar"} ${c.id}`}
                        disabled={locked}
                        onchange={(on) => void operate(c.id, on)}
                      />
                    </li>
                  {/if}
                {/each}
              </ul>
            {/if}
          </section>

          <section class="card" aria-labelledby="card-errors">
            <h2 class="card-title" id="card-errors">Errores recientes</h2>
            {#if !errorsAvailable}
              <p class="muted">Los logs todavía no están disponibles.</p>
            {:else if errors.length === 0}
              <div class="calm">
                <Icon name="check" />
                <p class="muted">Sin errores en la última hora</p>
              </div>
            {:else}
              <ul class="lines">
                {#each errors as entry (entry.id)}
                  <li>
                    <a class="error" href={logsHref(entry)}>
                      <span class="error-head">
                        <span>{projectName(entry.project)}</span>
                        <span class="num">{formatRelative(entry.ts_ms)}</span>
                      </span>
                      <span class="error-message">{entry.message}</span>
                    </a>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>

          <section class="card" aria-labelledby="card-mail">
            <h2 class="card-title" id="card-mail">Mailpit</h2>
            {#if !mailpit?.installed}
              <p class="muted">Recoge el correo que envían tus proyectos, sin que salga de tu Mac.</p>
              <a class="link" href="#/servicios">Instalar desde Servicios</a>
            {:else if !mailpit.running}
              <p class="muted">Está detenido.</p>
              <div>
                <Button disabled={locked} onclick={() => void operate("mailpit", true)}>Arrancar Mailpit</Button>
              </div>
            {:else}
              <p class="muted num">
                {mailpit.messages === null ? "En marcha" : plural(mailpit.messages, "correo")}
              </p>
              <div>
                <Button onclick={() => void openUrl(mailpit.url).catch(fail)}>Abrir Mailpit</Button>
              </div>
            {/if}
          </section>
        </div>

        {#if siteList.length > 0}
          <Group title="Sitios">
            {#each siteList as p (p.id)}
              <Row title={p.hosts[0] ?? p.name} subtitle={p.name}>
                {#snippet leading()}<Icon name="globe" />{/snippet}
                {#snippet trailing()}
                  <IconButton
                    icon="open"
                    label={`Abrir ${p.hosts[0]} en el navegador`}
                    onclick={() => void siteAction(p.id, "browser").catch(fail)}
                  />
                {/snippet}
              </Row>
            {/each}
          </Group>
          <a class="link more" href="#/sitios">Ver todos los sitios</a>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: var(--s-6);
    min-height: 168px;
    padding: var(--s-6);
    border-radius: var(--r-lg);
    background: var(--hero);
    color: var(--on-hero);
    --gauge-color: var(--on-hero);
    --gauge-text: var(--on-hero);
    transition: background var(--d-3) var(--ease);
  }

  .hero.warn {
    background: var(--hero-warn);
  }

  .hero.bad {
    background: var(--hero-bad);
  }

  .hero-text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--s-1);
    min-width: 0;
  }

  .hero-title {
    font: var(--t-hero);
    overflow-wrap: anywhere;
  }

  .hero-detail {
    font: var(--t-body);
    opacity: 0.85;
  }

  .hero-action {
    margin-top: var(--s-3);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--s-4);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    min-height: 136px;
    padding: var(--s-4);
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
  }

  .card-title {
    font: var(--t-headline);
  }

  .muted {
    font: var(--t-callout);
    color: var(--label-2);
  }

  .link {
    font: var(--t-callout);
    color: var(--accent);
  }

  .more {
    display: inline-block;
    margin-top: var(--s-2);
    padding: 0 var(--s-3);
  }

  .lines {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .line {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .line-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .calm {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    color: var(--green);
  }

  .error {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--s-1) var(--s-2);
    margin: 0 calc(-1 * var(--s-2));
    border-radius: var(--r-sm);
  }

  .error-head {
    display: flex;
    justify-content: space-between;
    gap: var(--s-2);
    font: var(--t-caption);
    color: var(--red);
  }

  .error-message {
    overflow: hidden;
    font: var(--t-callout);
    color: var(--label);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
