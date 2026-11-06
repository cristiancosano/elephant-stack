<script lang="ts">
  // Contenido del inspector (01 §7.5 y 02 §9): la entrada completa con texto
  // seleccionable, la excepción, la petición, la traza con vendor/ plegado, el contexto
  // JSON y las acciones. Mientras llega el detalle, muestra lo que ya se sabe de la fila.
  import type { LogEntry } from "../../lib/bindings/LogEntry";
  import type { LogGroup } from "../../lib/bindings/LogGroup";
  import { formatDuration, formatNumber, formatRelative } from "../../lib/format";
  import { hasContent } from "../../lib/logs/json";
  import { formatTimestamp } from "../../lib/logs/markdown";
  import { canClear, isNotAvailable, sourceLabel } from "../../lib/logs/model";
  import type { DetailState } from "../../lib/stores/logs.svelte";
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import Skeleton from "../../ui/Skeleton.svelte";
  import Frames from "./Frames.svelte";
  import JsonView from "./JsonView.svelte";
  import LevelPill from "./LevelPill.svelte";

  let {
    entry,
    detail,
    group,
    project,
    root,
    onopen,
    oncopy,
    onmarkdown,
    onclear,
  }: {
    /** La fila seleccionada (o la muestra del grupo). */
    entry: LogEntry | null;
    detail: DetailState | null;
    group: LogGroup | null;
    project: string;
    /** Raíz del proyecto, para acortar las rutas de la traza. */
    root: string | null;
    onopen: (file: string, line: number | null) => void;
    oncopy: () => void;
    onmarkdown: () => void;
    onclear: (entry: LogEntry) => void;
  } = $props();

  const data = $derived(detail?.data ?? null);
  const shown = $derived(data?.entry ?? entry);
  const exception = $derived(shown?.exception ?? null);
  const http = $derived(shown?.http ?? null);
  // El cuerpo repite el mensaje cuando la entrada es de una sola línea.
  const extraBody = $derived(data && data.body.trim() !== "" && data.body.trim() !== (shown?.message ?? "").trim() ? data.body : null);
</script>

{#if !shown}
  <EmptyState icon="logs" title="Ninguna entrada seleccionada" message="Elige una entrada de la lista para ver su detalle." />
{:else}
  <div class="detail">
    <header class="head">
      <LevelPill level={shown.level} />
      <span class="when num selectable">{formatTimestamp(shown.ts_ms)}</span>
    </header>

    <dl class="meta">
      <dt>Proyecto</dt>
      <dd class="selectable">{project}</dd>
      <dt>Fuente</dt>
      <dd>{sourceLabel(shown.kind)}{shown.channel ? ` · ${shown.channel}` : ""}</dd>
      {#if shown.host}
        <dt>Host</dt>
        <dd class="selectable">{shown.host}</dd>
      {/if}
      {#if group}
        <dt>Veces</dt>
        <dd class="num">
          {formatNumber(group.count)} · primera {formatRelative(group.first_ms)} · última {formatRelative(group.last_ms)}
        </dd>
      {/if}
    </dl>

    {#if http}
      <section>
        <h3>Petición</h3>
        <p class="mono selectable">{http.method} {http.path} → {http.status}</p>
        <p class="sub">
          {#if http.duration_ms !== null}{formatDuration(http.duration_ms)}{/if}
          {#if http.upstream_ms !== null} · PHP {formatDuration(http.upstream_ms)}{/if}
          {#if http.ip} · {http.ip}{/if}
        </p>
        {#if http.ua}<p class="sub selectable">{http.ua}</p>{/if}
      </section>
    {/if}

    <section>
      <h3>Mensaje</h3>
      <p class="message mono selectable">{shown.message}</p>
    </section>

    {#if exception}
      <section>
        <h3>Excepción</h3>
        <p class="mono selectable"><strong>{exception.class}</strong></p>
        {#if exception.message && exception.message !== shown.message}
          <p class="message mono selectable">{exception.message}</p>
        {/if}
        {#if exception.file}
          <p class="sub mono selectable">{exception.file}{exception.line !== null ? `:${exception.line}` : ""}</p>
        {/if}
      </section>
    {/if}

    <div class="actions">
      {#if exception?.file}
        <Button variant="primary" onclick={() => exception?.file && onopen(exception.file, exception.line)}>Abrir en el editor</Button>
      {/if}
      <Button onclick={oncopy}>Copiar</Button>
      <Button onclick={onmarkdown}>Copiar como Markdown</Button>
    </div>

    {#if detail?.loading}
      <Skeleton lines={4} />
    {:else if detail?.error}
      <p class="error" role="alert">
        {isNotAvailable(detail.error) ? "El detalle de las entradas todavía no está disponible." : detail.error}
      </p>
    {:else if data}
      {#if data.frames.length > 0}
        <section>
          <h3>Traza</h3>
          <Frames frames={data.frames} {root} onopen={(file, line) => onopen(file, line > 0 ? line : null)} />
        </section>
      {/if}

      {#if hasContent(data.context)}
        <section>
          <h3>Contexto</h3>
          <JsonView value={data.context} />
        </section>
      {/if}

      {#if extraBody}
        <details class="body">
          <summary>Entrada completa</summary>
          <pre class="mono selectable">{extraBody}</pre>
        </details>
      {/if}
    {/if}

    {#if canClear(shown)}
      <footer class="danger">
        <Button variant="plain" onclick={() => onclear(shown)}>Vaciar el archivo de log…</Button>
      </footer>
    {/if}
  </div>
{/if}

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    padding: var(--s-4);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--s-2);
  }

  .when {
    font: var(--t-callout);
    color: var(--label-2);
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--s-1) var(--s-3);
    margin: 0;
    font: var(--t-callout);
  }

  .meta dt {
    color: var(--label-2);
  }

  .meta dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: var(--s-1);
  }

  h3 {
    font: var(--t-caption);
    color: var(--label-2);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .mono {
    font: var(--t-log);
    line-height: 17px;
  }

  .message {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .sub {
    font: var(--t-callout);
    color: var(--label-2);
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
  }

  .error {
    font: var(--t-callout);
    color: var(--red);
  }

  .body summary {
    font: var(--t-callout);
    color: var(--label-2);
  }

  .body pre {
    margin: var(--s-2) 0 0;
    padding: var(--s-2) var(--s-3);
    overflow-x: auto;
    border-radius: var(--r-sm);
    background: var(--fill);
    white-space: pre;
  }

  .danger {
    padding-top: var(--s-3);
    border-top: 0.5px solid var(--separator);
  }
</style>
