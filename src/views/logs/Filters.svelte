<script lang="ts">
  // Barra de filtros (01 §7.5 con C1 y C19): búsqueda, proyecto con su contador de
  // errores, fuente, nivel mínimo y periodo. Todo cambio sube con `onchange` y acaba
  // en el hash.
  import type { ProjectSummary } from "../../lib/bindings/ProjectSummary";
  import {
    ALL_PROJECTS,
    LEVEL_VALUES,
    levelOptions,
    NO_PROJECT,
    rangeOptions,
    sourceOptions,
    SYSTEM,
    type Filters,
    type LevelFilter,
  } from "../../lib/logs/filters";
  import { errorCount } from "../../lib/logs/model";
  import { t } from "../../lib/i18n/index.svelte";
  import Icon from "../../ui/Icon.svelte";
  import SearchField from "../../ui/SearchField.svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Select from "../../ui/Select.svelte";

  let {
    filters,
    projects,
    focusToken,
    onchange,
  }: {
    filters: Filters;
    projects: ProjectSummary[];
    focusToken: number;
    onchange: (changes: Partial<Filters>) => void;
  } = $props();

  const projectOptions = $derived.by(() => {
    const options: { value: string; label: string; badge?: string }[] = [
      { value: ALL_PROJECTS, label: t("logs.filters.allProjects") },
    ];
    for (const s of projects) {
      const errors =
        s.errors_24h > 0
          ? t("logs.filters.errors", {
              count: s.errors_24h,
              n: errorCount(s.errors_24h, s.errors_capped),
            })
          : undefined;
      options.push({ value: s.project.id, label: s.project.name, badge: errors });
    }
    options.push(
      { value: NO_PROJECT, label: t("logs.filters.noProject") },
      { value: SYSTEM, label: t("logs.filters.system") },
    );
    // Un enlace con un proyecto que ya no existe: se muestra para poder quitarlo.
    if (!options.some((o) => o.value === filters.project)) options.push({ value: filters.project, label: t("logs.filters.unknownProject") });
    return options;
  });

  const levelColors: Record<LevelFilter, string> = {
    debug: "var(--label-3)",
    info: "var(--lvl-info)",
    warning: "var(--lvl-warning)",
    error: "var(--lvl-error)",
  };

  let chips: HTMLButtonElement[] = $state([]);

  function onLevelKey(event: KeyboardEvent, index: number) {
    const delta = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (delta === 0) return;
    event.preventDefault();
    const next = (index + delta + LEVEL_VALUES.length) % LEVEL_VALUES.length;
    onchange({ level: LEVEL_VALUES[next] });
    chips[next]?.focus();
  }
</script>

<div class="filters" role="toolbar" aria-label={t("logs.filters.label")}>
  <div class="search">
    <SearchField value={filters.text} placeholder={t("logs.filters.searchPlaceholder")} label={t("logs.filters.searchLabel")} {focusToken} oninput={(text) => onchange({ text })} />
  </div>

  <Select options={projectOptions} value={filters.project} label={t("logs.filters.project")} onchange={(project) => onchange({ project })} />

  <SegmentedControl options={sourceOptions()} value={filters.source} label={t("logs.filters.source")} onchange={(source) => onchange({ source })} />

  <div class="levels" role="radiogroup" aria-label={t("logs.filters.minLevel")}>
    {#each levelOptions() as option, i (option.value)}
      <button
        bind:this={chips[i]}
        type="button"
        role="radio"
        class="chip"
        aria-checked={filters.level === option.value}
        tabindex={filters.level === option.value ? 0 : -1}
        style:--chip={levelColors[option.value]}
        onclick={() => onchange({ level: option.value })}
        onkeydown={(e) => onLevelKey(e, i)}
      >
        <span class="dot" aria-hidden="true"></span>{option.label}
      </button>
    {/each}
  </div>

  <Select options={rangeOptions()} value={filters.range} label={t("logs.filters.range")} onchange={(range) => onchange({ range })} />

  {#if filters.status}
    <button type="button" class="chip removable" aria-label={t("logs.filters.statusRemove", { status: filters.status })} onclick={() => onchange({ status: null })}>
      {t("logs.filters.statusChip", { status: filters.status })}
      <Icon name="x" />
    </button>
  {/if}
</div>

<style>
  .filters {
    display: flex;
    flex: 0 0 auto;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s-2) var(--s-3);
    min-height: 44px;
    padding: var(--s-2) var(--s-4);
    border-bottom: 0.5px solid var(--separator);
  }

  .search {
    flex: 1 1 180px;
    min-width: 160px;
    max-width: 320px;
  }

  .search > :global(*) {
    width: 100%;
  }

  .levels {
    display: flex;
    gap: 2px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: var(--control-height);
    padding: 0 var(--s-2);
    border: none;
    border-radius: var(--r-pill);
    background: transparent;
    font: var(--t-callout);
    color: var(--label-2);
  }

  .chip[aria-checked="true"] {
    background: color-mix(in srgb, var(--chip) 16%, transparent);
    color: var(--label);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--chip);
  }

  .removable {
    background: var(--fill-2);
    color: var(--label);
  }
</style>
