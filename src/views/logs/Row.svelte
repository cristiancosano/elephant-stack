<script lang="ts">
  // Una fila de 20 px de la lista (01 §7.5): hora, nivel, proyecto y mensaje. En las
  // peticiones HTTP, método, ruta, estado coloreado y duración. Una entrada multilínea
  // ocupa una sola fila; la traza se ve en el inspector.
  import type { LogEntry } from "../../lib/bindings/LogEntry";
  import { formatClock, formatDuration } from "../../lib/format";
  import { httpClass, shortClass } from "../../lib/logs/model";
  import LevelPill from "./LevelPill.svelte";

  let { entry, project, selected }: { entry: LogEntry; project: string; selected: boolean } = $props();

  const http = $derived(entry.http);
  const slow = $derived(http?.duration_ms != null && http.duration_ms >= 1000);
</script>

<div class="cells" class:selected>
  <span role="gridcell" aria-selected={selected} class="time num">{formatClock(entry.ts_ms)}</span>
  <span role="gridcell" aria-selected={selected} class="level"><LevelPill level={entry.level} /></span>
  <span role="gridcell" aria-selected={selected} class="project" title={project}>{project}</span>
  {#if http}
    <span role="gridcell" aria-selected={selected} class="message http">
      <span class="method">{http.method}</span>
      <span class="status num http-{httpClass(http.status)}">{http.status}</span>
      <span class="path">{http.path}</span>
      {#if http.duration_ms !== null}
        <span class="duration num" class:slow>{formatDuration(http.duration_ms)}</span>
      {/if}
    </span>
  {:else}
    <span role="gridcell" aria-selected={selected} class="message">
      <span class="text">{entry.message}</span>
      {#if entry.exception}
        <span class="extra" title={entry.exception.class}>{shortClass(entry.exception.class)}</span>
      {/if}
    </span>
  {/if}
</div>

<style>
  .cells {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    height: 100%;
    padding: 0 var(--s-3);
    font: var(--t-log);
    color: var(--label);
    white-space: nowrap;
  }

  .cells.selected {
    background: var(--accent-soft);
  }

  :global(:root[data-window-inactive]) .cells.selected {
    background: var(--sidebar-selected);
  }

  .time {
    flex: 0 0 auto;
    width: 88px;
    color: var(--label-2);
  }

  .level {
    flex: 0 0 auto;
    width: 64px;
  }

  .project {
    flex: 0 0 auto;
    width: 112px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--label-2);
  }

  .message {
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    gap: var(--s-2);
    overflow: hidden;
  }

  .text,
  .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .path {
    flex: 1 1 auto;
    min-width: 0;
  }

  /* La clase de la excepción cede antes que el mensaje. */
  .extra {
    flex: 0 8 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    /* --label-3 se queda en unos 2:1 sobre blanco: corto para texto de 11,5 px. */
    color: var(--label-2);
  }

  .method {
    flex: 0 0 auto;
    width: 48px;
    color: var(--label-2);
  }

  .status {
    flex: 0 0 auto;
    width: 28px;
    font-weight: 600;
  }

  .duration {
    flex: 0 0 auto;
    color: var(--label-2);
  }

  .duration.slow {
    font-weight: 600;
    color: var(--label);
  }

  .http-2xx {
    color: var(--http-2xx);
  }
  .http-3xx {
    color: var(--http-3xx);
  }
  .http-4xx {
    color: var(--http-4xx);
  }
  .http-5xx {
    color: var(--http-5xx);
  }

  /* La lista se estrecha con el inspector abierto: el mensaje es lo último que cede. */
  @container logs-list (max-width: 600px) {
    .project {
      width: 72px;
    }
  }

  @container logs-list (max-width: 480px) {
    .project {
      display: none;
    }
  }
</style>
