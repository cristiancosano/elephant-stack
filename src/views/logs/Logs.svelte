<script lang="ts">
  // Vista Logs (`#/logs`, 01 §7.5 con C1, C2, C19 y C20): barra de filtros, lista virtual
  // de 20 px con un anillo de 10 000 entradas en directo, y el inspector. Los filtros
  // viven en el hash, así que el tray, el Panel o Sitios abren una vista ya filtrada.
  // Textos del área diferida «logs» (docs/i18n.md): viajan con esta vista.
  import "../../lib/i18n/locales/logs";
  import { onDestroy, onMount, untrack } from "svelte";

  import type { LogEntry } from "../../lib/bindings/LogEntry";
  import type { LogGroup } from "../../lib/bindings/LogGroup";
  import type { Snapshot } from "../../lib/bindings/Snapshot";
  import { onUiCommand } from "../../lib/events";
  import { plural, formatBytes, formatNumber } from "../../lib/format";
  import { errorMessage, logsAddRoot, logsClear, logsMarkSeen, openInEditor, pickFolder } from "../../lib/ipc";
  import { copyText } from "../../lib/logs/clipboard";
  import {
    ALL_PROJECTS,
    FILTER_KEYS,
    MODE_OPTIONS,
    NO_PROJECT,
    SYSTEM,
    clearedFilters,
    filtersKey,
    filtersToParams,
    hasFilterParams,
    isFiltered,
    parseFilters,
    resolveProject,
    type Filters as FilterValues,
  } from "../../lib/logs/filters";
  import { unseenLabel } from "../../lib/logs/follow";
  import { entryLine, entryMarkdown } from "../../lib/logs/markdown";
  import { popupMenu, releaseMenu, type MenuEntry } from "../../lib/logs/menu";
  import { canClear, fileKeyOf, isNotAvailable, projectLabel, projectNames } from "../../lib/logs/model";
  import { RING_CAPACITY } from "../../lib/logs/ring";
  import type { Route } from "../../lib/route";
  import { go, setParams } from "../../lib/router.svelte";
  import { createLogsStore } from "../../lib/stores/logs.svelte";
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import Inspector from "../../ui/Inspector.svelte";
  import { INSPECTOR_DEFAULT } from "../../ui/inspector";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Sheet from "../../ui/Sheet.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";
  import { toast } from "../../ui/toasts.svelte";
  import VirtualList from "../../ui/VirtualList.svelte";
  import Detail from "./Detail.svelte";
  import Filters from "./Filters.svelte";
  import GroupRow from "./GroupRow.svelte";
  import Row from "./Row.svelte";

  let { snapshot, route }: { snapshot: Snapshot | null; route: Route } = $props();

  const ROW_HEIGHT = 20;
  const FILTERS_KEY = "elephant-stack.logs.filters";
  const INSPECTOR_KEY = "elephant-stack.logs.inspector";

  const store = createLogsStore();

  // ── Preferencias de la vista (localStorage, 01 §4.3 regla 7) ──────────────

  function readJson<T>(key: string): T | null {
    try {
      const raw = localStorage.getItem(key);
      return raw ? (JSON.parse(raw) as T) : null;
    } catch {
      return null;
    }
  }

  function writeJson(key: string, value: unknown) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      // Sin almacenamiento: se usan los valores por defecto la próxima vez.
    }
  }

  const savedInspector = readJson<{ open?: boolean; width?: number }>(INSPECTOR_KEY);
  let inspectorOpen = $state(savedInspector?.open ?? false);
  let inspectorWidth = $state(typeof savedInspector?.width === "number" ? savedInspector.width : INSPECTOR_DEFAULT);

  $effect(() => writeJson(INSPECTOR_KEY, { open: inspectorOpen, width: inspectorWidth }));

  // Lo que traía el enlace al montar la vista; después manda el hash.
  const initialParams = untrack(() => route.params);

  // Un enlace sin filtros (`#/logs`) recupera los últimos; uno con filtros manda.
  if (!hasFilterParams(initialParams)) {
    const saved = readJson<Record<string, string | null>>(FILTERS_KEY);
    if (saved) {
      const restored: Record<string, string | null> = {};
      for (const key of FILTER_KEYS) {
        const value = saved[key];
        if (typeof value === "string" && value !== "") restored[key] = value;
      }
      if (Object.keys(restored).length > 0) setParams(restored, { replace: true });
    }
  }

  const filters = $derived(parseFilters(route.params));
  const key = $derived(filtersKey(filters));

  $effect(() => writeJson(FILTERS_KEY, filtersToParams(filters)));

  function setFilter(changes: Partial<FilterValues>) {
    setParams(filtersToParams({ ...filters, ...changes }), { replace: true });
  }

  // ── Carga ─────────────────────────────────────────────────────────────────

  // Solo depende de la clave: otros parámetros del hash (como `entry`) no recargan, y lo
  // que `load` lee y escribe del store no debe convertirse en dependencia.
  $effect(() => {
    void key;
    untrack(() => void store.load(filters));
  });

  const projectKey = $derived(snapshot?.projects.map((p) => p.id).join(",") ?? "");
  $effect(() => {
    void projectKey;
    untrack(() => void store.loadProjects());
  });

  // Un enlace puede traer el proyecto por nombre o host: se cambia por su id.
  $effect(() => {
    const value = filters.project;
    if (store.projectsState !== "ready" || value === ALL_PROJECTS || value === NO_PROJECT || value === SYSTEM) return;
    const resolved = resolveProject(value, store.projects);
    if (resolved !== value) setParams({ project: resolved }, { replace: true });
  });

  const names = $derived(
    projectNames(store.projects.length > 0 ? store.projects.map((s) => s.project) : (snapshot?.projects ?? [])),
  );
  const roots = $derived(new Map((store.projects.length > 0 ? store.projects.map((s) => s.project) : (snapshot?.projects ?? [])).map((p) => [p.id, p.root])));

  const unreadable = $derived.by(() => {
    const seen = new Map<string, string>();
    for (const s of store.projects) {
      for (const src of s.sources) {
        if (src.state.kind === "unreadable" && !seen.has(src.source.id)) seen.set(src.source.id, src.state.message);
      }
    }
    return [...seen.values()];
  });

  // ── Selección e inspector ─────────────────────────────────────────────────

  let selected = $state.raw<LogEntry | null>(null);
  let selectedGroup: string | null = $state(null);
  /** Entrada pedida por el enlace (`?entry=<id>`) antes de tenerla en la lista. */
  let pendingEntry = $state<string | null>(initialParams.entry ?? null);
  if (initialParams.entry) inspectorOpen = true;

  const grouped = $derived(filters.mode === "grouped");
  const group: LogGroup | null = $derived(grouped ? (store.groups.find((g) => g.fingerprint === selectedGroup) ?? null) : null);
  const current: LogEntry | null = $derived(
    grouped ? (group?.sample ?? null) : (selected ?? (pendingEntry && store.detail?.id === pendingEntry ? store.detail.data?.entry ?? null : null)),
  );
  const detailId = $derived(grouped ? (group?.sample.id ?? null) : (selected?.id ?? pendingEntry));

  $effect(() => {
    const id = detailId;
    if (!inspectorOpen || !id) return;
    // Al recorrer la lista con las flechas solo se pide la entrada en la que se para.
    const timer = setTimeout(() => void store.showDetail(id), 80);
    return () => clearTimeout(timer);
  });

  function labelFor(entry: LogEntry): string {
    return projectLabel(entry, names);
  }

  function groupProjects(g: LogGroup): string {
    if (g.projects.length === 0) return labelFor(g.sample);
    const first = names.get(g.projects[0]) ?? g.projects[0];
    return g.projects.length === 1 ? first : `${first} y ${g.projects.length - 1} más`;
  }

  function selectIndex(i: number) {
    if (grouped) {
      const g = store.groups[i];
      if (g) selectedGroup = g.fingerprint;
    } else {
      const e = store.ring.at(i);
      if (e) {
        selected = e;
        pendingEntry = null;
      }
    }
  }

  function currentIndex(): number {
    if (grouped) return store.groups.findIndex((g) => g.fingerprint === selectedGroup);
    return selected ? store.ring.indexOf(selected.id) : -1;
  }

  let list: VirtualList | undefined = $state();
  store.onShift = (rows) => list?.shiftBy(rows);
  store.onTruncated = () => toast("Un archivo de log se ha vaciado o rotado. Lo nuevo seguirá apareciendo.");

  const rowCount = $derived(grouped ? store.groups.length : store.count);

  /** La fila seleccionada, para `aria-activedescendant`. Lee `version` para seguirla cuando el anillo cambia. */
  const activeRow = $derived.by(() => {
    if (grouped) return store.groups.findIndex((g) => g.fingerprint === selectedGroup);
    void store.version;
    return selected ? store.ring.indexOf(selected.id) : -1;
  });

  function hasTextSelection(): boolean {
    const s = window.getSelection();
    return !!s && !s.isCollapsed && s.toString().trim() !== "";
  }

  function onListKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (rowCount === 0) return;
      const down = event.key === "ArrowDown";
      const from = currentIndex();
      const next = from < 0 ? (down ? 0 : rowCount - 1) : Math.min(rowCount - 1, Math.max(0, from + (down ? 1 : -1)));
      selectIndex(next);
      list?.scrollToIndex(next);
    } else if (event.key === "Enter") {
      if (current) {
        event.preventDefault();
        inspectorOpen = true;
      }
    } else if (event.key === "Escape") {
      if (inspectorOpen) {
        event.preventDefault();
        inspectorOpen = false;
      }
    } else if (event.metaKey && event.key.toLowerCase() === "c" && current && !hasTextSelection()) {
      event.preventDefault();
      void copyEntry(current);
    }
  }

  // ── Acciones ──────────────────────────────────────────────────────────────

  function showError(e: unknown, unavailable = "Esta acción todavía no está disponible.") {
    const message = errorMessage(e);
    toast(isNotAvailable(message) ? unavailable : message, { tone: "error" });
  }

  async function copyEntry(entry: LogEntry) {
    const d = store.detail?.id === entry.id ? store.detail.data : null;
    const text = d && d.body.trim() !== "" ? d.body : entryLine(entry, labelFor(entry));
    try {
      await copyText(text);
      toast("Entrada copiada.", { tone: "success" });
    } catch (e) {
      showError(e);
    }
  }

  async function copyMarkdown(entry: LogEntry) {
    let markdown: string;
    try {
      markdown = entryMarkdown(await store.fetchDetail(entry.id), labelFor(entry));
    } catch {
      // Sin detalle (motor sin terminar o archivo cambiado): lo que se sabe de la fila.
      markdown = entryMarkdown({ entry, body: "", frames: [], context: null }, labelFor(entry));
    }
    try {
      await copyText(markdown);
      toast("Copiado como Markdown.", { tone: "success" });
    } catch (e) {
      showError(e);
    }
  }

  function openEditor(file: string, line: number | null) {
    openInEditor(file, line).catch((e) => showError(e));
  }

  async function addFolder() {
    const path = await pickFolder("Elige una carpeta con proyectos").catch(() => null);
    if (!path) return;
    try {
      const list = await logsAddRoot(path);
      store.setProjects(list);
      toast("Carpeta añadida.", { tone: "success" });
      void store.load(filters);
    } catch (e) {
      showError(e, "Todavía no se pueden añadir carpetas.");
    }
  }

  let confirmClear = $state.raw<LogEntry | null>(null);
  let clearing = $state(false);

  async function clearFile() {
    const entry = confirmClear;
    const fileKey = entry ? fileKeyOf(entry.id) : null;
    if (!entry || !fileKey) return;
    clearing = true;
    try {
      await logsClear(fileKey);
      toast("Archivo de log vaciado.", { tone: "success" });
      selected = null;
      void store.load(filters);
    } catch (e) {
      showError(e);
    } finally {
      clearing = false;
      confirmClear = null;
    }
  }

  function toggleLive() {
    store.setLive(!store.live);
    if (store.live) list?.scrollToEnd();
  }

  function clearView() {
    store.clearView();
    selected = null;
    selectedGroup = null;
  }

  function reload() {
    void store.load(filters);
    void store.loadProjects();
  }

  function jumpToEnd() {
    store.jumpToEnd();
    list?.scrollToEnd();
  }

  function rowMenu(event: MouseEvent, entry: LogEntry) {
    event.preventDefault();
    selected = entry;
    pendingEntry = null;
    const items: MenuEntry[] = [
      { text: "Copiar", action: () => void copyEntry(entry) },
      { text: "Copiar como Markdown", action: () => void copyMarkdown(entry) },
    ];
    const file = entry.exception?.file;
    if (file) items.push({ text: "Abrir en el editor", action: () => openEditor(file, entry.exception?.line ?? null) });
    items.push("separator", { text: "Mostrar en el inspector", action: () => (inspectorOpen = true) });
    if (entry.project) {
      const project = entry.project;
      items.push({ text: `Solo ${labelFor(entry)}`, action: () => setFilter({ project }) });
    }
    void popupMenu(items, { x: event.clientX, y: event.clientY });
  }

  function moreMenu(event: MouseEvent) {
    const target = event.currentTarget as HTMLElement | null;
    const rect = target?.getBoundingClientRect();
    const entry = current;
    const items: MenuEntry[] = [
      { text: "Añadir carpeta de proyectos…", action: () => void addFolder() },
      { text: "Ajustes de logs…", action: () => go("#/ajustes/logs") },
      "separator",
      { text: "Copiar como Markdown", enabled: entry !== null, action: () => entry && void copyMarkdown(entry) },
      { text: "Vaciar el archivo de log…", enabled: entry !== null && canClear(entry), action: () => (confirmClear = entry) },
    ];
    void popupMenu(items, rect ? { x: rect.left, y: rect.bottom + 2 } : undefined);
  }

  // ── Ciclo de vida ─────────────────────────────────────────────────────────

  let searchFocus = $state(0);
  let now = $state(Date.now());

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    logsMarkSeen().catch(() => {});
    if (initialParams.entry) setParams({ entry: null }, { replace: true });

    onUiCommand((command) => {
      if (command === "find") searchFocus++;
      else if (command === "toggle-inspector") inspectorOpen = !inspectorOpen;
      else if (command === "live") toggleLive();
      else if (command === "clear-view") clearView();
      else if (command === "refresh") reload();
    })
      .then((un) => {
        if (disposed) un();
        else unlisten = un;
      })
      .catch(() => {});

    const onVisibility = () => {
      const visible = document.visibilityState === "visible";
      store.setVisible(visible);
      if (visible) {
        now = Date.now();
        logsMarkSeen().catch(() => {});
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    onVisibility();

    // Solo para el «hace 5 min» de los grupos; no toca el disco.
    const clock = setInterval(() => {
      if (document.visibilityState === "visible") now = Date.now();
    }, 30_000);

    return () => {
      disposed = true;
      unlisten?.();
      document.removeEventListener("visibilitychange", onVisibility);
      clearInterval(clock);
    };
  });

  onDestroy(() => {
    store.dispose();
    releaseMenu();
  });

  // ── Textos ────────────────────────────────────────────────────────────────

  const pill = $derived(grouped ? null : unseenLabel(store.follow));
  const following = $derived(!grouped && store.live && store.follow.following);

  const status = $derived.by(() => {
    const parts: string[] = [];
    if (grouped) parts.push(plural(store.groups.length, "grupo"));
    else {
      parts.push(plural(store.count, "entrada"));
      if (store.count >= RING_CAPACITY) parts.push(`se muestran las ${formatNumber(RING_CAPACITY)} más recientes`);
    }
    if (store.scannedBytes > 0) parts.push(`${formatBytes(store.scannedBytes)} leídos`);
    if (store.dropped > 0) parts.push(`${formatNumber(store.dropped)} descartadas por llegar demasiado rápido`);
    return parts.join(" · ");
  });

  const liveStatus = $derived(
    !store.live ? "En pausa" : store.streamError ? "Sin directo" : store.streaming ? "En vivo" : "Conectando…",
  );

  const empty = $derived.by(() => {
    if (filters.source === "http") {
      return {
        title: "Herd no guarda logs de acceso",
        message: "Herd tiene access_log off. Verás las peticiones cuando Elephant gestione nginx.",
        action: undefined,
      };
    }
    if (store.projectsState === "ready" && store.projects.length === 0 && !isFiltered(filters)) {
      return {
        title: "Todavía no hay proyectos",
        message: "Elephant lee los logs de los sitios de Herd y de las carpetas de proyectos que añadas.",
        action: { label: "Añadir carpeta…", onclick: () => void addFolder() },
      };
    }
    if (isFiltered(filters)) {
      return {
        title: "Nada con estos filtros",
        message: store.live ? "Lo nuevo que coincida aparecerá aquí al instante." : undefined,
        action: { label: "Quitar filtros", onclick: () => setFilter(clearedFilters(filters)) },
      };
    }
    return {
      title: filters.range === "all" ? "Sin entradas" : "Sin entradas en este periodo",
      message: store.live ? "Lo nuevo aparecerá aquí al instante." : undefined,
      action: filters.range === "all" ? undefined : { label: "Ver todo", onclick: () => setFilter({ range: "all" }) },
    };
  });
</script>

<section class="logs">
  <header class="toolbar" data-tauri-drag-region>
    <h1 data-tauri-drag-region>Logs</h1>
    <div class="actions">
      <SegmentedControl options={[...MODE_OPTIONS]} value={filters.mode} label="Presentación" onchange={(mode) => setFilter({ mode })} />
      <button type="button" class="live" aria-pressed={store.live} title="En vivo (⌘L)" onclick={toggleLive}>
        <span class="live-dot" class:on={store.live && store.streaming} aria-hidden="true"></span>
        En vivo
      </button>
      <Button variant="plain" onclick={clearView}>Limpiar</Button>
      <IconButton icon="info" label={inspectorOpen ? "Ocultar el inspector" : "Mostrar el inspector"} onclick={() => (inspectorOpen = !inspectorOpen)} />
      <IconButton icon="more" label="Más acciones" onclick={moreMenu} />
    </div>
  </header>

  <Filters {filters} projects={store.projects} focusToken={searchFocus} onchange={setFilter} />

  {#if unreadable.length > 0}
    <p class="notice" role="status">
      {unreadable.length === 1 ? "Hay una fuente de log que no se puede leer" : `Hay ${unreadable.length} fuentes de log que no se pueden leer`}: {unreadable[0]}
    </p>
  {/if}

  <div class="body">
    <div class="main">
      {#if store.state === "unavailable"}
        <EmptyState icon="logs" title="El visor de logs todavía no está disponible" message="Esta versión de Elephant Stack aún no lee los logs de tus proyectos." />
      {:else if store.state === "error"}
        <EmptyState icon="warning" title="No se han podido leer los logs" message={store.error ?? undefined} action={{ label: "Reintentar", onclick: reload }} />
      {:else if store.state === "loading" && rowCount === 0}
        <div class="skeleton"><Skeleton lines={10} /></div>
      {:else if rowCount === 0 && !grouped && (store.loadingOlder || store.hasOlder)}
        <!-- Rust para al agotar el presupuesto de lectura: no es «sin resultados» si queda historial. -->
        {#if store.olderError}
          <EmptyState icon="warning" title="No se ha podido seguir buscando" message={store.olderError} action={{ label: "Reintentar", onclick: () => void store.loadOlder() }} />
        {:else if store.loadingOlder}
          <EmptyState icon="logs" title="Buscando en el historial…" message={`${formatBytes(store.scannedBytes)} leídos hasta ahora.`} />
        {:else}
          <EmptyState
            icon="logs"
            title="Nada por ahora"
            message={`No hay coincidencias en los ${formatBytes(store.scannedBytes)} más recientes, pero queda historial más antiguo.`}
            action={{ label: "Seguir buscando", onclick: () => void store.loadOlder() }}
          />
        {/if}
      {:else if rowCount === 0}
        <EmptyState icon="logs" title={empty.title} message={empty.message} action={empty.action} />
      {:else}
        {#if store.loadingOlder || store.olderError || (!grouped && store.olderPaused)}
          <div class="older" role="status">
            {#if store.olderError}
              No se ha podido cargar más historial. <Button variant="plain" onclick={() => void store.loadOlder()}>Reintentar</Button>
            {:else if store.loadingOlder}
              Cargando historial…
            {:else}
              Nada más con estos filtros en lo último leído. <Button variant="plain" onclick={() => void store.loadOlder()}>Seguir buscando</Button>
            {/if}
          </div>
        {/if}
        <!-- El teclado llega desde la lista (role="grid", con foco); aquí solo se escucha. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="list" onkeydown={onListKey}>
          {#if grouped}
            <VirtualList
              bind:this={list}
              count={store.groups.length}
              rowHeight={ROW_HEIGHT}
              label="Grupos de entradas"
              activeIndex={activeRow}
            >
              {#snippet row(i)}
                {@const g = store.groups[i]}
                {#if g}
                  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                  <div class="hit" onclick={() => (selectedGroup = g.fingerprint)} ondblclick={() => (inspectorOpen = true)}>
                    <GroupRow group={g} projects={groupProjects(g)} selected={g.fingerprint === selectedGroup} {now} />
                  </div>
                {/if}
              {/snippet}
            </VirtualList>
          {:else}
            <VirtualList
              bind:this={list}
              count={store.count}
              rowHeight={ROW_HEIGHT}
              follow={following}
              onfollowchange={(v) => store.scrolled(v)}
              onnearstart={() => void store.loadOlder()}
              label="Entradas de log"
              activeIndex={activeRow}
            >
              {#snippet row(i)}
                {@const e = store.entryAt(i)}
                {#if e}
                  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                  <div
                    class="hit"
                    onclick={() => {
                      selected = e;
                      pendingEntry = null;
                    }}
                    ondblclick={() => (inspectorOpen = true)}
                    oncontextmenu={(event) => rowMenu(event, e)}
                  >
                    <Row entry={e} project={labelFor(e)} selected={e.id === selected?.id} />
                  </div>
                {/if}
              {/snippet}
            </VirtualList>
          {/if}
        </div>
        {#if pill}
          <button type="button" class="pill" onclick={jumpToEnd}>{pill} ↓</button>
        {/if}
      {/if}

      <!-- Solo el estado del directo es región viva: el recuento cambia con cada lote
           (cada ≤100 ms) y VoiceOver lo leería sin parar. -->
      <footer class="status num">
        <span class="status-text">{status}</span>
        <span class="live-state" role="status">{liveStatus}</span>
      </footer>
    </div>

    <Inspector open={inspectorOpen} width={inspectorWidth} onresize={(w) => (inspectorWidth = w)} label="Detalle de la entrada">
      <div class="inspector-scroll">
        {#key detailId}
          <Detail
            entry={current}
            detail={store.detail && store.detail.id === detailId ? store.detail : null}
            {group}
            project={current ? labelFor(current) : ""}
            root={current?.project ? (roots.get(current.project) ?? null) : null}
            onopen={openEditor}
            oncopy={() => current && void copyEntry(current)}
            onmarkdown={() => current && void copyMarkdown(current)}
            onclear={(entry) => (confirmClear = entry)}
          />
        {/key}
      </div>
    </Inspector>
  </div>
</section>

<Sheet open={confirmClear !== null} title="¿Vaciar el archivo de log?" onclose={() => (confirmClear = null)}>
  <p class="sheet-text">
    Se borrará todo el contenido del archivo de log de <strong>{confirmClear ? labelFor(confirmClear) : ""}</strong> que contiene esta entrada.
    No se puede deshacer.
  </p>
  <div class="sheet-actions">
    <Button onclick={() => (confirmClear = null)}>Cancelar</Button>
    <Button variant="destructive" busy={clearing} onclick={() => void clearFile()}>Vaciar archivo</Button>
  </div>
</Sheet>

<style>
  .logs {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--window);
  }

  .toolbar {
    display: flex;
    flex: 0 0 var(--toolbar-height);
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    /* Con la barra lateral plegada, Shell sube --toolbar-inset para los botones de ventana. */
    padding: 0 var(--s-4) 0 var(--toolbar-inset, var(--s-5));
  }

  h1 {
    font: var(--t-title);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: var(--control-height);
    padding: 0 var(--s-2);
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    font: var(--t-body);
    color: var(--label-2);
  }

  .live[aria-pressed="true"] {
    background: var(--fill-2);
    color: var(--label);
  }

  .live-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--label-3);
  }

  .live-dot.on {
    background: var(--green);
  }

  .notice {
    flex: 0 0 auto;
    padding: var(--s-1) var(--s-4);
    border-bottom: 0.5px solid var(--separator);
    background: color-mix(in srgb, var(--orange) 12%, transparent);
    font: var(--t-callout);
    color: var(--label);
  }

  .body {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
  }

  .main {
    container: logs-list / inline-size;
    position: relative;
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-width: 0;
    background: var(--group);
  }

  .list {
    flex: 1 1 auto;
    min-height: 0;
  }

  .hit {
    height: 100%;
  }

  .skeleton {
    padding: var(--s-4);
  }

  .older {
    position: absolute;
    top: var(--s-2);
    left: 50%;
    z-index: 1;
    transform: translateX(-50%);
    padding: var(--s-1) var(--s-3);
    border-radius: var(--r-pill);
    background: var(--hud);
    color: var(--on-hud);
    font: var(--t-callout);
  }

  .pill {
    position: absolute;
    bottom: calc(22px + var(--s-3));
    left: 50%;
    transform: translateX(-50%);
    padding: var(--s-1) var(--s-3);
    border: none;
    border-radius: var(--r-pill);
    background: var(--accent);
    color: var(--accent-text);
    font: var(--t-callout);
    box-shadow: var(--shadow-pop);
  }

  .status {
    display: flex;
    flex: 0 0 22px;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    padding: 0 var(--s-3);
    border-top: 0.5px solid var(--separator);
    background: var(--window);
    font: var(--t-caption);
    color: var(--label-2);
    white-space: nowrap;
  }

  .status-text {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .inspector-scroll {
    height: 100%;
    overflow-y: auto;
  }

  .sheet-text {
    margin-bottom: var(--s-5);
    color: var(--label-2);
  }

  .sheet-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s-2);
  }
</style>
