// Filtros de la vista Logs y su reflejo en el hash: `#/logs?project=…&level=error&range=1h`.
// El hash es la fuente de verdad (01 §7.5): así el tray, el Panel o Sitios abren una
// vista ya filtrada. Aquí se traduce en los dos sentidos y a `LogQuery`.

import type { LogLevel } from "../bindings/LogLevel";
import type { LogQuery } from "../bindings/LogQuery";
import type { ProjectSummary } from "../bindings/ProjectSummary";
import type { SourceKind } from "../bindings/SourceKind";
import type { StatusFilter } from "../bindings/StatusFilter";
import "../i18n/locales/logs";
import { t } from "../i18n/index.svelte";

/** «Todos», un id de proyecto, `none` (sin proyecto) o `system` (PHP-FPM). */
export type ProjectFilter = string;
export const ALL_PROJECTS = "all";
export const NO_PROJECT = "none";
export const SYSTEM = "system";

/** Fuentes de C19: Todas · App · HTTP · nginx · PHP-FPM. */
export type SourceFilter = "all" | "app" | "http" | "nginx" | "php-fpm";
/** Nivel mínimo. `debug` equivale a todos. */
export type LevelFilter = "debug" | "info" | "warning" | "error";
export type RangeFilter = "15m" | "1h" | "today" | "7d" | "all";
/** `5xx` o `4xx`: clase de estado HTTP. */
export type StatusClass = "4xx" | "5xx";
export type Mode = "chrono" | "grouped";

export interface Filters {
  project: ProjectFilter;
  source: SourceFilter;
  level: LevelFilter;
  range: RangeFilter;
  text: string;
  status: StatusClass | null;
  mode: Mode;
}

export const DEFAULT_FILTERS: Readonly<Filters> = Object.freeze({
  project: ALL_PROJECTS,
  source: "all",
  level: "debug",
  // 02 §12: «Hoy» por defecto para que buscar texto no recorra semanas de logs.
  range: "today",
  text: "",
  status: null,
  mode: "chrono",
});

// Las opciones se resuelven con `t()` al pintar (en un `$derived` o en la
// plantilla): una constante de módulo se quedaría con el idioma de ese momento.

export function sourceOptions(): { value: SourceFilter; label: string }[] {
  return [
    { value: "all", label: t("logs.filters.sources.all") },
    { value: "app", label: "App" },
    { value: "http", label: "HTTP" },
    { value: "nginx", label: "nginx" },
    { value: "php-fpm", label: "PHP-FPM" },
  ];
}

/** Valores del nivel mínimo, en el orden de las píldoras. */
export const LEVEL_VALUES: readonly LevelFilter[] = ["debug", "info", "warning", "error"];

export function levelOptions(): { value: LevelFilter; label: string }[] {
  return [
    { value: "debug", label: "Debug" },
    { value: "info", label: "Info" },
    { value: "warning", label: t("logs.filters.levels.warning") },
    { value: "error", label: t("logs.filters.levels.errorPlus") },
  ];
}

export function rangeOptions(): { value: RangeFilter; label: string }[] {
  return [
    { value: "15m", label: t("logs.filters.ranges.m15") },
    { value: "1h", label: t("logs.filters.ranges.h1") },
    { value: "today", label: t("logs.filters.ranges.today") },
    { value: "7d", label: t("logs.filters.ranges.d7") },
    { value: "all", label: t("logs.filters.ranges.all") },
  ];
}

export function modeOptions(): { value: Mode; label: string }[] {
  return [
    { value: "chrono", label: t("logs.filters.modes.chrono") },
    { value: "grouped", label: t("logs.filters.modes.grouped") },
  ];
}

const SOURCE_KINDS: Record<Exclude<SourceFilter, "all">, SourceKind> = {
  app: "laravel_app",
  http: "nginx_access",
  nginx: "nginx_error",
  "php-fpm": "php_fpm",
};

/** Otras formas de escribir la fuente en un enlace (01 usaba `laravel`). */
const SOURCE_ALIASES: Record<string, SourceFilter> = {
  all: "all",
  app: "app",
  laravel: "app",
  laravel_app: "app",
  http: "http",
  access: "http",
  nginx_access: "http",
  nginx: "nginx",
  nginx_error: "nginx",
  "php-fpm": "php-fpm",
  php_fpm: "php-fpm",
  fpm: "php-fpm",
};

const LEVEL_ALIASES: Record<string, LevelFilter> = {
  debug: "debug",
  all: "debug",
  info: "info",
  notice: "info",
  warning: "warning",
  warn: "warning",
  error: "error",
  critical: "error",
};

const RANGES = new Set<string>(["15m", "1h", "today", "7d", "all"] satisfies RangeFilter[]);

function pick<T extends string>(value: string | undefined, allowed: Record<string, T>, fallback: T): T {
  if (value === undefined) return fallback;
  return allowed[value.toLowerCase()] ?? fallback;
}

/** Lee los filtros de los parámetros del hash. Lo desconocido vuelve al valor por defecto. */
export function parseFilters(params: Readonly<Record<string, string>>): Filters {
  const range = params.range?.toLowerCase();
  const status = params.status?.toLowerCase();
  return {
    project: params.project?.trim() || ALL_PROJECTS,
    source: pick(params.source, SOURCE_ALIASES, DEFAULT_FILTERS.source),
    level: pick(params.level, LEVEL_ALIASES, DEFAULT_FILTERS.level),
    range: range && RANGES.has(range) ? (range as RangeFilter) : DEFAULT_FILTERS.range,
    text: params.q ?? "",
    status: status === "4xx" || status === "5xx" ? status : null,
    mode: params.mode === "grouped" ? "grouped" : "chrono",
  };
}

/** Parámetros para `setParams`: los valores por defecto se quitan (`null`) para que el hash quede corto. */
export function filtersToParams(f: Filters): Record<string, string | null> {
  const d = DEFAULT_FILTERS;
  return {
    project: f.project === d.project ? null : f.project,
    source: f.source === d.source ? null : f.source,
    level: f.level === d.level ? null : f.level,
    range: f.range === d.range ? null : f.range,
    q: f.text.trim() === "" ? null : f.text,
    status: f.status,
    mode: f.mode === d.mode ? null : f.mode,
  };
}

/** Claves del hash que son filtros (para guardar y restaurar la última vista). */
export const FILTER_KEYS = ["project", "source", "level", "range", "q", "status", "mode"] as const;

export function hasFilterParams(params: Readonly<Record<string, string>>): boolean {
  return FILTER_KEYS.some((k) => params[k] !== undefined && params[k] !== "");
}

/** ¿Hay algo que quitar con «Quitar filtros»? El modo y el periodo no cuentan. */
export function isFiltered(f: Filters): boolean {
  return (
    f.project !== ALL_PROJECTS ||
    f.source !== "all" ||
    f.level !== "debug" ||
    f.text.trim() !== "" ||
    f.status !== null
  );
}

/** Lo que quita «Quitar filtros»: se conservan el modo y el periodo. */
export function clearedFilters(f: Filters): Filters {
  return { ...DEFAULT_FILTERS, range: f.range, mode: f.mode };
}

const MINUTE = 60_000;

/** Inicio del periodo en ms, o `null` para «Todo». «Hoy» empieza a medianoche local. */
export function rangeSince(range: RangeFilter, now: number): number | null {
  switch (range) {
    case "15m":
      return now - 15 * MINUTE;
    case "1h":
      return now - 60 * MINUTE;
    case "today": {
      const d = new Date(now);
      return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
    }
    case "7d":
      return now - 7 * 24 * 60 * MINUTE;
    case "all":
      return null;
  }
}

const MIN_LEVEL: Record<LevelFilter, LogLevel | null> = {
  debug: null,
  info: "info",
  warning: "warning",
  error: "error",
};

/** Tamaño de página: 02 §6 (por defecto 200, tope 2000). */
export const PAGE_SIZE = 200;

export function toQuery(f: Filters, now: number, before: string | null = null): LogQuery {
  const status: StatusFilter | null = f.status ? { kind: "class", value: f.status === "5xx" ? 5 : 4 } : null;
  const text = f.text.trim();
  return {
    projects: f.project === ALL_PROJECTS ? [] : [f.project],
    kinds: f.source === "all" ? [] : [SOURCE_KINDS[f.source]],
    min_level: MIN_LEVEL[f.level],
    text: text === "" ? null : text,
    status,
    since_ms: rangeSince(f.range, now),
    until_ms: null,
    before,
    limit: PAGE_SIZE,
  };
}

/**
 * Lo que obliga a volver a cargar la lista. El modo también, porque cambia de
 * `logs_query` a `logs_groups`.
 */
export function filtersKey(f: Filters): string {
  return [f.project, f.source, f.level, f.range, f.text.trim(), f.status ?? "", f.mode].join("\u0000");
}

/**
 * Un enlace puede traer el proyecto por id, nombre o host (`?project=tienda.test`).
 * Devuelve el id si lo encuentra; si no, el valor tal cual (puede ser un id que aún
 * no está en la lista).
 */
export function resolveProject(value: string, summaries: readonly ProjectSummary[]): string {
  if (value === ALL_PROJECTS || value === NO_PROJECT || value === SYSTEM) return value;
  const needle = value.toLowerCase();
  const byId = summaries.find((s) => s.project.id === value);
  if (byId) return byId.project.id;
  const byName = summaries.find((s) => s.project.name.toLowerCase() === needle);
  if (byName) return byName.project.id;
  const byHost = summaries.find((s) => s.project.hosts.some((h) => h.toLowerCase() === needle));
  return byHost ? byHost.project.id : value;
}
