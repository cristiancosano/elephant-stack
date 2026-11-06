// Vocabulario de la vista Logs: niveles, fuentes, etiquetas y nombres de proyecto.
// Puro, para poder probarlo y compartirlo entre la vista, el inspector y Ajustes.

import type { LogEntry } from "../bindings/LogEntry";
import type { LogLevel } from "../bindings/LogLevel";
import type { Project } from "../bindings/Project";
import type { ProjectOrigin } from "../bindings/ProjectOrigin";
import type { SourceKind } from "../bindings/SourceKind";
// Textos del área diferida «logs»: este módulo la registra al cargarse.
import "../i18n/locales/logs";
import { t } from "../i18n/index.svelte";

/**
 * Mensaje de los stubs de Rust (00 §4.4): la función todavía no existe. Es un
 * centinela que se compara con el error de Rust, no un texto: no se traduce.
 */
export const NOT_AVAILABLE = "Todavía no disponible.";

export function isNotAvailable(message: string): boolean {
  return message.trim() === NOT_AVAILABLE;
}

/** De menos a más grave, como `LogLevel` en Rust. */
export const LEVELS: readonly LogLevel[] = [
  "debug",
  "info",
  "notice",
  "warning",
  "error",
  "critical",
  "alert",
  "emergency",
];

export function levelRank(level: LogLevel): number {
  return LEVELS.indexOf(level);
}

export function atLeast(level: LogLevel, min: LogLevel): boolean {
  return levelRank(level) >= levelRank(min);
}

export function levelLabel(level: LogLevel): string {
  return t(`logs.level.${level}`);
}

/**
 * Familia de color del nivel, para la clase `lvl-<familia>` (tokens `--lvl-*`).
 * Alert y Emergency comparten el color de Critical.
 */
export function levelTone(level: LogLevel): "debug" | "info" | "notice" | "warning" | "error" | "critical" {
  return levelRank(level) >= levelRank("critical") ? "critical" : (level as "debug" | "info" | "notice" | "warning" | "error");
}

const SOURCE_LABELS: Record<SourceKind, string> = {
  laravel_app: "App",
  nginx_access: "HTTP",
  nginx_error: "nginx",
  php_fpm: "PHP-FPM",
};

export function sourceLabel(kind: SourceKind): string {
  return SOURCE_LABELS[kind];
}

const ORIGIN_KEYS = {
  elephant: "logs.origin.elephant",
  herd_parked: "logs.origin.herdParked",
  herd_linked: "logs.origin.herdLinked",
  user: "logs.origin.user",
} as const satisfies Record<ProjectOrigin, string>;

export function originLabel(origin: ProjectOrigin): string {
  return t(ORIGIN_KEYS[origin]);
}

/** Familia de color de un estado HTTP (tokens `--http-*`). */
export function httpClass(status: number): "2xx" | "3xx" | "4xx" | "5xx" {
  if (status >= 500) return "5xx";
  if (status >= 400) return "4xx";
  if (status >= 300) return "3xx";
  return "2xx";
}

/** Columna «proyecto» de una fila: nombre, host sin sitio, «Sistema» o «Sin proyecto». */
export function projectLabel(entry: LogEntry, names: ReadonlyMap<string, string>): string {
  if (entry.project) return names.get(entry.project) ?? entry.project;
  if (entry.kind === "php_fpm") return t("logs.project.system");
  return entry.host ?? t("logs.project.none");
}

/** `Illuminate\\Database\\QueryException` → `QueryException`. */
export function shortClass(fqcn: string): string {
  const i = fqcn.lastIndexOf("\\");
  return i === -1 ? fqcn : fqcn.slice(i + 1);
}

/** Mapa id → nombre a partir de una lista de proyectos. */
export function projectNames(projects: readonly Pick<Project, "id" | "name">[]): Map<string, string> {
  return new Map(projects.map((p) => [p.id, p.name]));
}

/** `500+` cuando el motor llegó al tope de lectura (02 §6). */
export function errorCount(n: number, capped: boolean): string {
  return capped ? `${n}+` : String(n);
}

/**
 * Clave del archivo de una entrada: su id es `<file_key:016x>-<offset:x>` (02 §5.2).
 * Es lo que pide `logs_clear`.
 */
export function fileKeyOf(entryId: string): string | null {
  const dash = entryId.indexOf("-");
  if (dash <= 0) return null;
  const key = entryId.slice(0, dash);
  return /^[0-9a-f]+$/i.test(key) ? key : null;
}

/**
 * Orden cronológico (las más antiguas primero) de lo que llega de Rust, que pagina de
 * más reciente a más antigua. Ante empates conserva el orden inverso al recibido, que
 * es el del archivo.
 */
export function chronological(entries: readonly LogEntry[]): LogEntry[] {
  return entries
    .slice()
    .reverse()
    .sort((a, b) => a.ts_ms - b.ts_ms);
}

/** Solo los logs de la app de un proyecto se pueden vaciar; los de Herd, nunca. */
export function canClear(entry: LogEntry): boolean {
  return entry.kind === "laravel_app" && entry.project !== null && fileKeyOf(entry.id) !== null;
}
