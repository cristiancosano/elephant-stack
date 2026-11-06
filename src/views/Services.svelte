<script lang="ts">
  // Servicios (01 §7.3): servicios de Homebrew, herramientas y lo que falta por
  // instalar. El estado sale siempre del snapshot; aquí solo se piden acciones.
  import Group from "../ui/Group.svelte";
  import Row from "../ui/Row.svelte";
  import Switch from "../ui/Switch.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Button from "../ui/Button.svelte";
  import StatusDot, { type DotState } from "../ui/StatusDot.svelte";
  import ProgressRing from "../ui/ProgressRing.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import { toast } from "../ui/toasts.svelte";
  import {
    errorMessage,
    installComponent,
    openServiceLog,
    refresh,
    revealInFinder,
    serviceOperation,
    servicesBulk,
  } from "../lib/ipc";
  import {
    below,
    brewBusy,
    busyFor,
    installedText,
    jobForComponent,
    jobText,
    missing,
    popupMenu,
    problemFor,
    serviceState,
    serviceStatusText,
    services,
    tools,
    type MenuEntry,
  } from "../lib/stores/app.svelte";
  import { usage } from "../lib/stores/usage.svelte";
  import { plural } from "../lib/format";
  import type { Component } from "../lib/bindings/Component";
  import type { Operation } from "../lib/bindings/Operation";
  import type { Service } from "../lib/bindings/Service";
  import type { Snapshot } from "../lib/bindings/Snapshot";
  import type { Route } from "../lib/route";

  let { snapshot, route: _route }: { snapshot: Snapshot | null; route: Route } = $props();

  /** launchd puede tardar en reflejar un arranque: se vuelve a mirar pasado este tiempo. */
  const STATUS_RECHECK_MS = 2000;

  const serviceList = $derived(snapshot ? services(snapshot) : []);
  const toolList = $derived(snapshot ? tools(snapshot) : []);
  const missingList = $derived(snapshot ? missing(snapshot) : []);
  const locked = $derived(snapshot ? brewBusy(snapshot) : false);
  const anyRunning = $derived(serviceList.some((c) => c.service?.running));
  /** Tareas de agentes que no corresponden a ninguna fila (crear un proyecto…). */
  const otherJobs = $derived(
    snapshot
      ? snapshot.jobs.filter(
          (job) => !snapshot.components.some((c) => jobForComponent(snapshot, c.id) === job),
        )
      : [],
  );

  let showMissing = $state(false);
  /** Instalaciones pedidas desde esta ventana: id → última línea de brew. */
  let installing: Record<string, string> = $state({});

  function fail(e: unknown) {
    toast(errorMessage(e), { tone: "error" });
  }

  async function operate(name: string, operation: Operation) {
    try {
      await serviceOperation(name, operation);
      setTimeout(() => void refresh().catch(() => {}), STATUS_RECHECK_MS);
    } catch (e) {
      fail(e);
    }
  }

  async function bulk(operation: Operation) {
    try {
      await servicesBulk(operation);
    } catch (e) {
      fail(e);
    }
  }

  async function install(id: string) {
    installing[id] = "";
    try {
      await installComponent(id, (line) => {
        if (id in installing) installing[id] = line;
      });
      toast(`${id} se ha instalado.`, { tone: "success" });
    } catch (e) {
      fail(e);
    } finally {
      delete installing[id];
    }
  }

  async function openLog(name: string) {
    try {
      await openServiceLog(name);
    } catch (e) {
      fail(e);
    }
  }

  function serviceMenu(c: Component, service: Service, at?: { x: number; y: number }) {
    const use = service.pid ? usage.byPid.get(service.pid) : undefined;
    const entries: MenuEntry[] = [
      {
        text: service.running ? "Detener" : "Arrancar",
        action: () => void operate(c.id, service.running ? "turn_off" : "turn_on"),
      },
      { text: "Reiniciar", enabled: service.running, action: () => void operate(c.id, "restart") },
      {
        text: "Arrancar al iniciar sesión",
        checked: service.registered,
        action: () => void operate(c.id, service.registered ? "disable_autostart" : "enable_autostart"),
      },
      "separator",
      { text: "Ver logs", action: () => void openLog(c.id) },
      {
        text: "Mostrar en Finder",
        enabled: !!c.path,
        action: () => c.path && void revealInFinder(c.path).catch(fail),
      },
    ];
    const details = [
      c.version ? `Versión ${c.version}` : null,
      use ? plural(use.processes, "proceso") : null,
    ].filter(Boolean);
    if (details.length > 0) entries.push("separator", { text: details.join(" · "), enabled: false });
    void popupMenu(entries, at).catch(fail);
  }

  function allMenu(event: MouseEvent) {
    void popupMenu(
      [
        { text: "Reiniciar todos", enabled: anyRunning && !locked, action: () => void bulk("restart") },
        { text: "Detener todos", enabled: anyRunning && !locked, action: () => void bulk("turn_off") },
      ],
      below(event),
    ).catch(fail);
  }

  // Clic derecho en una fila de servicio: el mismo menú que ⋯. Se escucha en el
  // grupo entero para no envolver las filas (los separadores dependen de que sean
  // hermanas).
  let serviceGroup: HTMLElement | undefined = $state();
  function oncontextmenu(event: MouseEvent) {
    const row = (event.target as Element | null)?.closest('[role="listitem"]');
    if (!row || !serviceGroup) return;
    const rows = Array.from(serviceGroup.querySelectorAll('[role="listitem"]'));
    const c = serviceList[rows.indexOf(row)];
    event.preventDefault();
    if (!c?.service || locked) return;
    serviceMenu(c, c.service, { x: event.clientX, y: event.clientY });
  }

  function dotState(c: Component, service: Service): DotState {
    if (snapshot && busyFor(snapshot, c.id)) return "busy";
    if (snapshot && problemFor(snapshot, c.id)) return "error";
    return serviceState(service);
  }

  function subtitle(c: Component, service: Service): string {
    const busy = snapshot ? busyFor(snapshot, c.id) : undefined;
    if (busy) return busy.label;
    const problem = snapshot ? problemFor(snapshot, c.id) : undefined;
    const status = serviceStatusText(service, service.pid ? usage.byPid.get(service.pid) : undefined);
    const extra = installedText(c);
    const head = extra === "Instalado" ? status : `${extra} · ${status}`;
    return problem?.kind === "action_failed" ? `${problem.message} · ${head}` : head;
  }
</script>

<div class="view">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>Servicios</h1>
    <IconButton icon="restart" label="Actualizar" onclick={() => void refresh().catch(fail)} />
    <IconButton icon="more" label="Acciones de todos los servicios" onclick={allMenu} />
  </header>

  <div class="scroll">
    <div class="content">
      {#if !snapshot}
        <Skeleton lines={5} />
      {:else if snapshot.components.length === 0}
        <EmptyState
          icon="servicios"
          title="No se ha encontrado nada"
          message="Elephant Stack usa Homebrew para los servicios. Comprueba que está instalado y pulsa Actualizar."
        />
      {:else}
        {#if otherJobs.length > 0}
          <Group title="Tareas de agentes">
            {#each otherJobs as job (job.id)}
              <Row title={jobText(job)}>
                {#snippet leading()}<Icon name="agent" />{/snippet}
                {#snippet trailing()}<ProgressRing value={job.progress ?? undefined} />{/snippet}
              </Row>
            {/each}
          </Group>
        {/if}

        {#if serviceList.length > 0}
          <div class="group-host" role="presentation" bind:this={serviceGroup} {oncontextmenu}>
            <Group title="Servicios">
              {#each serviceList as c (c.id)}
                {@const service = c.service}
                {#if service}
                  {@const busy = busyFor(snapshot, c.id)}
                  <Row title={c.id} subtitle={subtitle(c, service)}>
                    {#snippet leading()}
                      <StatusDot state={dotState(c, service)} label={busy?.label ?? serviceStatusText(service)} />
                    {/snippet}
                    {#snippet trailing()}
                      {#if busy}<ProgressRing label={busy.label} />{/if}
                      <Switch
                        checked={service.running}
                        label={`${service.running ? "Detener" : "Arrancar"} ${c.id}`}
                        disabled={locked}
                        onchange={(on) => void operate(c.id, on ? "turn_on" : "turn_off")}
                      />
                      <IconButton
                        icon="more"
                        label={`Más opciones de ${c.id}`}
                        disabled={locked}
                        onclick={(event) => serviceMenu(c, service, below(event))}
                      />
                    {/snippet}
                  </Row>
                {:else}
                  <Row title={c.id} subtitle={installedText(c)} />
                {/if}
              {/each}
            </Group>
          </div>
        {/if}

        {#if toolList.length > 0}
          <Group title="Herramientas">
            {#each toolList as c (c.id)}
              <Row title={c.id} subtitle={installedText(c)}>
                {#snippet trailing()}
                  {#if c.path}
                    {@const path = c.path}
                    <IconButton
                      icon="folder"
                      label={`Mostrar ${c.id} en Finder (${path})`}
                      onclick={() => void revealInFinder(path).catch(fail)}
                    />
                  {/if}
                {/snippet}
              </Row>
            {/each}
          </Group>
        {/if}

        {#if missingList.length > 0}
          <section class="missing">
            <button
              type="button"
              class="disclosure"
              aria-expanded={showMissing}
              onclick={() => (showMissing = !showMissing)}
            >
              <Icon name={showMissing ? "chevron-down" : "chevron-right"} />
              Sin instalar ({missingList.length})
            </button>
            {#if showMissing}
              <Group>
                {#each missingList as c (c.id)}
                  {@const job = jobForComponent(snapshot, c.id)}
                  {@const mine = c.id in installing}
                  {@const busy = busyFor(snapshot, c.id)}
                  <Row
                    title={c.id}
                    subtitle={job ? jobText(job) : mine ? installing[c.id] || "Instalando…" : c.description}
                  >
                    {#snippet trailing()}
                      {#if job}
                        <ProgressRing value={job.progress ?? undefined} label={jobText(job)} />
                      {:else}
                        <Button busy={mine || !!busy} disabled={locked} onclick={() => void install(c.id)}>
                          Instalar<span class="visually-hidden"> {c.id}</span>
                        </Button>
                      {/if}
                    {/snippet}
                  </Row>
                {/each}
              </Group>
            {/if}
          </section>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .disclosure {
    display: inline-flex;
    align-items: center;
    gap: var(--s-1);
    margin-bottom: var(--s-2);
    padding: var(--s-1) var(--s-2);
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    font: var(--t-headline);
    color: var(--label);
  }
</style>
