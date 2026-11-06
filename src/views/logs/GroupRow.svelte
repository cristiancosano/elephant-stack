<script lang="ts">
  // Fila del modo «Agrupados» (02 §9): cuántas veces, nivel, mensaje de muestra,
  // proyectos y cuándo se vio por última vez.
  import type { LogGroup } from "../../lib/bindings/LogGroup";
  import { formatNumber, formatRelative } from "../../lib/format";
  import "../../lib/i18n/locales/logs";
  import { t } from "../../lib/i18n/index.svelte";
  import LevelPill from "./LevelPill.svelte";

  let { group, projects, selected, now }: { group: LogGroup; projects: string; selected: boolean; now: number } = $props();

  const sample = $derived(group.sample);
  const text = $derived(
    sample.http ? `${sample.http.method} ${sample.http.path} → ${sample.http.status}` : sample.exception ? `${sample.exception.class}: ${sample.message}` : sample.message,
  );
</script>

<div class="cells" class:selected>
  <span role="gridcell" aria-selected={selected} class="count num" title={t("logs.group.count")}>×{formatNumber(group.count)}</span>
  <span role="gridcell" aria-selected={selected} class="level"><LevelPill level={sample.level} /></span>
  <span role="gridcell" aria-selected={selected} class="project" title={projects}>{projects}</span>
  <span role="gridcell" aria-selected={selected} class="message">{text}</span>
  <span role="gridcell" aria-selected={selected} class="last">{formatRelative(group.last_ms, now)}</span>
</div>

<style>
  .cells {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    height: 100%;
    padding: 0 var(--s-3);
    font: var(--t-log);
    white-space: nowrap;
  }

  .cells.selected {
    background: var(--accent-soft);
  }

  :global(:root[data-window-inactive]) .cells.selected {
    background: var(--sidebar-selected);
  }

  .count {
    flex: 0 0 auto;
    width: 56px;
    font-weight: 600;
    text-align: right;
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
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .last {
    flex: 0 0 auto;
    color: var(--label-2);
    font: var(--t-callout);
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
