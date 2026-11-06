<script lang="ts">
  import { onDestroy } from "svelte";
  import Badge, { type BadgeTone } from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import Gauge from "../ui/Gauge.svelte";
  import Group from "../ui/Group.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Inspector from "../ui/Inspector.svelte";
  import ProgressRing from "../ui/ProgressRing.svelte";
  import Row from "../ui/Row.svelte";
  import SearchField from "../ui/SearchField.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import Select from "../ui/Select.svelte";
  import Sheet from "../ui/Sheet.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import StatusDot, { type DotState } from "../ui/StatusDot.svelte";
  import Switch from "../ui/Switch.svelte";
  import Toaster from "../ui/Toaster.svelte";
  import VirtualList from "../ui/VirtualList.svelte";
  import { ICON_NAMES } from "../ui/icons";
  import { toast } from "../ui/toasts.svelte";
  import { formatClock } from "../lib/format";

  // --- Apariencia ---------------------------------------------------------------
  type Theme = "system" | "light" | "dark";
  let theme: Theme = $state("system");
  let accent = $state("");
  let inactive = $state(false);

  $effect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
  });

  $effect(() => {
    const root = document.documentElement;
    if (accent) root.style.setProperty("--system-accent", accent);
    else root.style.removeProperty("--system-accent");
  });

  $effect(() => {
    document.documentElement.toggleAttribute("data-window-inactive", inactive);
  });

  const ACCENTS = [
    { value: "", label: "Azul (por defecto)" },
    { value: "#a550a7", label: "Morado" },
    { value: "#f74f9e", label: "Rosa" },
    { value: "#ff5257", label: "Rojo" },
    { value: "#f7821b", label: "Naranja" },
    { value: "#62ba46", label: "Verde" },
    { value: "#8c8c8c", label: "Grafito" },
  ];

  // --- Estado de los ejemplos ---------------------------------------------------
  let switches = $state({ on: true, off: false });
  let php = $state("8.4");
  let project = $state("todos");
  let search = $state("");
  let regexSearch = $state("error.*");
  let focusToken = $state(0);
  let selectedRow = $state("mysql");
  let gaugeValue = $state(412);
  let sheetOpen = $state(false);
  let inspectorOpen = $state(true);
  let inspectorWidth = $state(320);

  const TONES: BadgeTone[] = ["neutral", "accent", "green", "orange", "red", "purple"];
  const DOTS: { state: DotState; label: string }[] = [
    { state: "running", label: "En marcha" },
    { state: "stopped", label: "Detenido" },
    { state: "error", label: "Error · código 1" },
    { state: "busy", label: "Aplicando…" },
  ];

  const regexInvalid = $derived.by(() => {
    try {
      new RegExp(regexSearch);
      return false;
    } catch {
      return true;
    }
  });

  // --- Lista virtual ------------------------------------------------------------
  const LEVELS = ["DEBUG", "INFO", "WARNING", "ERROR"] as const;
  const start = Date.now() - 10_000 * 1000;
  let rows = $state(10_000);
  let follow = $state(true);
  let live: ReturnType<typeof setInterval> | undefined = $state();

  function toggleLive() {
    if (live) {
      clearInterval(live);
      live = undefined;
    } else {
      live = setInterval(() => (rows += 10), 100);
    }
  }

  onDestroy(() => clearInterval(live));

  // `?superposiciones` abre la hoja y muestra un aviso de cada tono, para capturas.
  if (new URLSearchParams(window.location.search).has("superposiciones")) {
    sheetOpen = true;
    toast("Copiado al portapapeles");
    toast("PHP 8.4 es ahora la versión por defecto", { tone: "success" });
    toast("No se pudo detener mysql: Homebrew está ocupado.", { tone: "error", action: { label: "Reintentar", onclick: () => {} } });
  }
</script>

<div class="gallery">
  <header class="toolbar">
    <h1>Trunk · Galería</h1>
    <div class="tools">
      <SegmentedControl
        label="Apariencia"
        options={[
          { value: "system", label: "Sistema" },
          { value: "light", label: "Claro" },
          { value: "dark", label: "Oscuro" },
        ]}
        value={theme}
        onchange={(v: Theme) => (theme = v)}
      />
      <Select label="Acento" options={ACCENTS} value={accent} onchange={(v: string) => (accent = v)} />
      <label class="inline">
        <Switch checked={inactive} label="Simular ventana inactiva" onchange={(v) => (inactive = v)} />
        Ventana inactiva
      </label>
    </div>
  </header>

  <main class="content">
    <!-- Tokens -->
    <section>
      <h2>Colores</h2>
      <div class="swatches">
        {#each ["--accent", "--accent-soft", "--label", "--label-2", "--label-3", "--fill", "--fill-2", "--fill-3", "--separator", "--window", "--group", "--raised", "--green", "--orange", "--red", "--blue", "--purple", "--lvl-debug", "--lvl-info", "--lvl-notice", "--lvl-warning", "--lvl-error", "--lvl-critical"] as token (token)}
          <div class="swatch">
            <span class="chip" style:background="var({token})"></span>
            <code>{token}</code>
          </div>
        {/each}
      </div>
      <div class="heroes">
        {#each ["--hero", "--hero-warn", "--hero-bad"] as token (token)}
          <div class="hero" style:background="var({token})">
            <div>
              <p class="hero-title">Todo en marcha</p>
              <p class="hero-sub">3 servicios · PHP 8.4 · 412 MB</p>
            </div>
            <Gauge value={gaugeValue} max={1024} label="{gaugeValue} MB" caption="RAM" />
          </div>
        {/each}
      </div>
    </section>

    <section>
      <h2>Tipografía</h2>
      <div class="type">
        <p style:font="var(--t-hero)">Hero · 26/32</p>
        <p style:font="var(--t-title)">Title · 17/22</p>
        <p style:font="var(--t-headline)">Headline · 13/16</p>
        <p style:font="var(--t-body)">Body · 13/16</p>
        <p style:font="var(--t-callout)">Callout · 12/15</p>
        <p style:font="var(--t-caption)">Caption · 11/13</p>
        <p style:font="var(--t-log)" class="num">09:05:03.007 · Log · 11,5/20</p>
      </div>
    </section>

    <section>
      <h2>Iconos ({ICON_NAMES.length})</h2>
      <div class="icons">
        {#each ICON_NAMES as name (name)}
          <div class="icon-cell">
            <Icon {name} />
            <Icon {name} size={20} />
            <code>{name}</code>
          </div>
        {/each}
      </div>
    </section>

    <!-- Controles -->
    <section>
      <h2>Button</h2>
      {#each ["regular", "large"] as const as size (size)}
        <div class="line">
          {#each ["primary", "secondary", "plain", "destructive"] as const as variant (variant)}
            <Button {variant} {size} onclick={() => toast(`Pulsado: ${variant}`)}>{variant}</Button>
          {/each}
          <Button {size} disabled>Desactivado</Button>
          <Button variant="primary" {size} busy>Instalando</Button>
          <Button {size} busy>Reiniciando</Button>
        </div>
      {/each}
    </section>

    <section>
      <h2>IconButton</h2>
      <div class="line">
        <IconButton icon="restart" label="Actualizar" onclick={() => toast("Actualizando…")} />
        <IconButton icon="more" label="Más acciones" />
        <IconButton icon="sidebar" label="Mostrar u ocultar la barra lateral" />
        <IconButton icon="stop" label="Detener" disabled />
      </div>
    </section>

    <section>
      <h2>Switch</h2>
      <div class="line">
        <Switch checked={switches.on} label="Detener mysql" onchange={(v) => (switches.on = v)} />
        <Switch checked={switches.off} label="Arrancar redis" onchange={(v) => (switches.off = v)} />
        <Switch checked={true} label="Detener nginx" disabled />
        <Switch checked={false} label="Arrancar mailpit" disabled />
      </div>
    </section>

    <section>
      <h2>SegmentedControl</h2>
      <div class="line">
        <SegmentedControl
          label="PHP por defecto"
          options={["8.1", "8.2", "8.3", "8.4"].map((v) => ({ value: v, label: v }))}
          value={php}
          onchange={(v: string) => (php = v)}
        />
        <SegmentedControl
          label="Fuente"
          options={[
            { value: "todas", label: "Todas" },
            { value: "app", label: "App" },
            { value: "http", label: "HTTP" },
            { value: "nginx", label: "nginx" },
            { value: "fpm", label: "PHP-FPM" },
          ]}
          value="http"
          onchange={(v: string) => toast(`Fuente: ${v}`)}
        />
      </div>
    </section>

    <section>
      <h2>Select</h2>
      <div class="line">
        <Select
          label="Proyecto"
          options={[
            { value: "todos", label: "Todos", badge: "5" },
            { value: "acme", label: "acme", badge: "2" },
            { value: "shop", label: "shop" },
            { value: "none", label: "Sin proyecto" },
            { value: "system", label: "Sistema", badge: "3" },
          ]}
          value={project}
          onchange={(v: string) => (project = v)}
        />
        <Select label="Editor" options={[{ value: "phpstorm", label: "PhpStorm" }]} value="phpstorm" onchange={() => {}} disabled />
      </div>
    </section>

    <section>
      <h2>SearchField</h2>
      <div class="stack">
        <SearchField value={search} placeholder="Buscar en los logs" oninput={(v) => (search = v)} {focusToken} />
        <p class="caption">Valor emitido (150 ms): «{search}»</p>
        <SearchField value={regexSearch} placeholder="Expresión regular" regex invalid={regexInvalid} oninput={(v) => (regexSearch = v)} />
        <SearchField value="([" placeholder="Regex inválida" regex invalid oninput={() => {}} />
        <div class="line"><Button onclick={() => focusToken++}>Simular ⌘F</Button></div>
      </div>
    </section>

    <!-- Listas agrupadas -->
    <section>
      <h2>Group y Row</h2>
      <Group title="Servicios" footer="Homebrew solo admite una operación a la vez.">
        {#each [{ id: "mysql", state: "running" as DotState, sub: "8.4.3 · En marcha · 84 MB" }, { id: "redis", state: "stopped" as DotState, sub: "7.4 · Al iniciar sesión" }, { id: "nginx", state: "error" as DotState, sub: "Error · código 1" }, { id: "mailpit", state: "busy" as DotState, sub: "Aplicando…" }] as s (s.id)}
          <Row title={s.id} subtitle={s.sub} selected={selectedRow === s.id} onactivate={() => (selectedRow = s.id)}>
            {#snippet leading()}<StatusDot state={s.state} label={s.sub} />{/snippet}
            {#snippet trailing()}
              {#if s.state === "busy"}<ProgressRing size={16} label="Aplicando" />{/if}
              <Switch checked={s.state === "running"} label="{s.state === 'running' ? 'Detener' : 'Arrancar'} {s.id}" disabled={s.state === "busy"} />
              <IconButton icon="more" label="Más acciones de {s.id}" />
            {/snippet}
          </Row>
        {/each}
      </Group>
      <Group title="Herramientas">
        <Row title="composer" />
        <Row title="node" subtitle="22.11.0">
          {#snippet trailing()}<Badge tone="neutral" label="Herd" />{/snippet}
        </Row>
      </Group>
    </section>

    <section>
      <h2>Badge y StatusDot</h2>
      <div class="line">
        {#each TONES as tone (tone)}<Badge {tone} label={tone} />{/each}
      </div>
      <div class="line">
        {#each DOTS as dot (dot.state)}
          <span class="inline"><StatusDot state={dot.state} label={dot.label} /> {dot.label}</span>
        {/each}
      </div>
    </section>

    <section>
      <h2>Gauge y ProgressRing</h2>
      <div class="line">
        <Gauge value={gaugeValue} max={1024} label="{gaugeValue} MB" caption="RAM" />
        <Gauge value={0} max={1024} label="0 MB" />
        <Gauge value={2048} max={1024} label="2 GB" caption="Lleno" />
        <input type="range" min="0" max="1024" bind:value={gaugeValue} aria-label="Valor del gauge" />
      </div>
      <div class="line">
        {#each [12, 16, 20] as const as size (size)}
          <ProgressRing {size} />
          <ProgressRing {size} value={0.6} />
        {/each}
      </div>
    </section>

    <section>
      <h2>Skeleton y EmptyState</h2>
      <div class="card"><Skeleton /></div>
      <div class="card"><Skeleton lines={1} /></div>
      <div class="card">
        <EmptyState icon="sitios" title="Tus sitios los sirve Herd por ahora" message="Elephant Stack los muestra en solo lectura." action={{ label: "Configurar con el asistente", onclick: () => toast("Asistente") }} />
      </div>
      <div class="card"><EmptyState icon="check" title="Sin errores en la última hora" /></div>
    </section>

    <!-- Superposiciones -->
    <section>
      <h2>Sheet y Toast</h2>
      <div class="line">
        <Button onclick={() => (sheetOpen = true)}>Abrir hoja</Button>
        <Button onclick={() => toast("Copiado al portapapeles")}>Toast info</Button>
        <Button onclick={() => toast("PHP 8.4 es ahora la versión por defecto", { tone: "success" })}>Toast éxito</Button>
        <Button onclick={() => toast("No se pudo detener mysql: Homebrew está ocupado.", { tone: "error", action: { label: "Reintentar", onclick: () => toast("Reintentando…") } })}>Toast error</Button>
      </div>
      <Sheet open={sheetOpen} title="Hace falta tu contraseña" onclose={() => (sheetOpen = false)}>
        <p>Para instalar dnsmasq, macOS te pedirá la contraseña de administrador.</p>
        <div class="line end">
          <Button onclick={() => (sheetOpen = false)}>Cancelar</Button>
          <Button variant="primary" onclick={() => (sheetOpen = false)}>Continuar</Button>
        </div>
      </Sheet>
    </section>

    <section>
      <h2>Inspector y VirtualList</h2>
      <div class="line">
        <Button onclick={() => (inspectorOpen = !inspectorOpen)}>{inspectorOpen ? "Ocultar" : "Mostrar"} inspector</Button>
        <Button onclick={toggleLive}>{live ? "Parar" : "Simular"} 100 entradas/s</Button>
        <Switch checked={follow} label="Seguir la cola" onchange={(v) => (follow = v)} />
        <span class="caption num">{rows} filas · seguir: {follow ? "sí" : "no"} · inspector: {inspectorWidth} px</span>
      </div>
      <div class="split">
        <div class="list">
          <VirtualList count={rows} rowHeight={20} {follow} onfollowchange={(v) => (follow = v)} onnearstart={() => toast("Pediría más historial")} label="Logs de ejemplo">
            {#snippet row(i)}
              <div class="log-row" class:odd={i % 2 === 1}>
                <span role="gridcell" class="num">{formatClock(start + i * 1000)}</span>
                <span role="gridcell" class="lvl lvl-{LEVELS[i % 4].toLowerCase()}">{LEVELS[i % 4]}</span>
                <span role="gridcell" class="msg">Entrada número {i + 1}</span>
              </div>
            {/snippet}
          </VirtualList>
        </div>
        <Inspector open={inspectorOpen} width={inspectorWidth} onresize={(w) => (inspectorWidth = w)}>
          <div class="inspector-body selectable">
            <h3>Detalle</h3>
            <p>Arrastra el borde izquierdo o usa ← → con el tirador enfocado.</p>
          </div>
        </Inspector>
      </div>
    </section>
  </main>
</div>

<Toaster />

<style>
  :global(body) {
    background: var(--window);
  }

  .gallery {
    height: 100vh;
    overflow: auto;
  }

  .toolbar {
    position: sticky;
    z-index: 10;
    top: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-4);
    min-height: var(--toolbar-height);
    padding: var(--s-2) var(--content-padding);
    background: var(--window);
    box-shadow: 0 0.5px 0 var(--separator);
  }

  h1 {
    font: var(--t-title);
  }

  h2 {
    margin-bottom: var(--s-3);
    font: var(--t-headline);
    color: var(--label-2);
  }

  h3 {
    font: var(--t-headline);
  }

  .tools,
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s-3);
  }

  .line + .line {
    margin-top: var(--s-3);
  }

  .end {
    justify-content: flex-end;
    margin-top: var(--s-4);
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    max-width: 360px;
  }

  .inline {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
  }

  .caption {
    font: var(--t-callout);
    color: var(--label-2);
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: var(--s-8);
    max-width: var(--content-max);
    margin: 0 auto;
    padding: var(--content-padding);
  }

  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: var(--s-2);
  }

  .swatch {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    font: var(--t-caption);
  }

  .chip {
    width: 24px;
    height: 24px;
    border-radius: var(--r-sm);
    box-shadow: var(--hairline);
  }

  .heroes {
    display: grid;
    gap: var(--s-3);
    margin-top: var(--s-4);
  }

  .hero {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 168px;
    padding: var(--s-6);
    border-radius: var(--r-lg);
    color: var(--on-hero);
    --gauge-color: var(--on-hero);
    --gauge-text: var(--on-hero);
  }

  .hero-title {
    font: var(--t-hero);
  }

  .hero-sub {
    margin-top: var(--s-1);
    opacity: 0.85;
  }

  .type {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  .icons {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: var(--s-2);
  }

  .icon-cell {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: var(--s-2);
    border-radius: var(--r-sm);
    background: var(--group);
    font: var(--t-caption);
    color: var(--label);
  }

  .card {
    margin-bottom: var(--s-3);
    padding: var(--s-3);
    border-radius: var(--r-md);
    background: var(--group);
    box-shadow: var(--shadow-1);
  }

  .split {
    display: flex;
    height: 320px;
    margin-top: var(--s-3);
    overflow: hidden;
    border-radius: var(--r-md);
    background: var(--window);
    box-shadow: var(--shadow-1);
  }

  .list {
    flex: 1;
    min-width: 0;
  }

  .log-row {
    display: flex;
    gap: var(--s-3);
    height: 100%;
    padding: 0 var(--s-3);
    font: var(--t-log);
  }

  .log-row.odd {
    background: var(--fill);
  }

  .lvl {
    width: 64px;
    font-weight: 600;
  }

  .lvl-debug {
    color: var(--lvl-debug);
  }

  .lvl-info {
    color: var(--lvl-info);
  }

  .lvl-warning {
    color: var(--lvl-warning);
  }

  .lvl-error {
    color: var(--lvl-error);
  }

  .msg {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .inspector-body {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
    padding: var(--s-4);
  }
</style>
