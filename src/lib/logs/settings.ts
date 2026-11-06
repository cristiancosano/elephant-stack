// Lógica de Ajustes ▸ Logs: carpetas raíz y, por proyecto, zona horaria y rutas
// extra de logs (`ProjectOverride`, 02 §4.4).

import type { Project } from "../bindings/Project";
import type { ProjectOverride } from "../bindings/ProjectOverride";

export interface RootFolder {
  /** Carpeta que contiene los proyectos (o el proyecto, si se añadió él mismo). */
  path: string;
  projects: Project[];
}

function trimSlash(path: string): string {
  return path.length > 1 ? path.replace(/\/+$/, "") : path;
}

function isInside(path: string, root: string): boolean {
  const p = trimSlash(path);
  const r = trimSlash(root);
  return p === r || p.startsWith(r === "/" ? "/" : `${r}/`);
}

/**
 * Las carpetas de `projects.json` (`logs_roots`), en su orden, con los proyectos de
 * origen `user` que salen de cada una. Un proyecto va a la carpeta más concreta
 * que lo contiene.
 */
export function groupRoots(roots: readonly string[], projects: readonly Project[]): RootFolder[] {
  const folders = roots.map((path) => ({ path, projects: [] as Project[] }));
  for (const p of projects) {
    if (p.origin !== "user") continue;
    const best = folders
      .filter((f) => isInside(p.root, f.path))
      .sort((a, b) => trimSlash(b.path).length - trimSlash(a.path).length)[0];
    best?.projects.push(p);
  }
  for (const f of folders) f.projects.sort((a, b) => a.name.localeCompare(b.name, "es"));
  return folders;
}

/** Lo que muestra la hoja «Ajustar…» al abrirse: el ajuste guardado, si lo hay. */
export function overrideFields(o: ProjectOverride | null): { timezone: string; globsText: string } {
  return { timezone: o?.timezone ?? "", globsText: (o?.log_globs ?? []).join("\n") };
}

/** `/Users/ana/Code/tienda` → `~/Code/tienda`, si `home` es su prefijo. */
export function tildePath(path: string, home: string | null): string {
  if (!home) return path;
  const h = home.replace(/\/+$/, "");
  if (path === h) return "~";
  return path.startsWith(`${h}/`) ? `~${path.slice(h.length)}` : path;
}

/** Deduce `$HOME` de una ruta de proyecto como `/Users/ana/…`. */
export function guessHome(paths: readonly string[]): string | null {
  for (const p of paths) {
    const m = /^(\/Users\/[^/]+)(?:\/|$)/.exec(p);
    if (m) return m[1];
  }
  return null;
}

export interface GlobsResult {
  globs: string[];
  errors: string[];
}

/**
 * Una ruta por línea: absoluta y con `*` solo en el nombre del archivo (02 §5.1).
 * Las líneas vacías se ignoran y las repetidas se quitan.
 */
export function parseGlobs(text: string): GlobsResult {
  const globs: string[] = [];
  const errors: string[] = [];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line === "") continue;
    if (!line.startsWith("/")) {
      errors.push(`«${line}» no es una ruta absoluta (debe empezar por /).`);
      continue;
    }
    const slash = line.lastIndexOf("/");
    const dir = line.slice(0, slash);
    const name = line.slice(slash + 1);
    if (dir.includes("*")) {
      errors.push(`«${line}»: el * solo puede ir en el nombre del archivo, no en las carpetas.`);
      continue;
    }
    if (name === "") {
      errors.push(`«${line}»: falta el nombre del archivo (por ejemplo, *.log).`);
      continue;
    }
    if (!globs.includes(line)) globs.push(line);
  }
  return { globs, errors };
}

/** Zonas IANA que conoce el motor de JS, más `UTC`. Vacío si el motor no las da. */
export function knownTimezones(): string[] {
  const intl = Intl as unknown as { supportedValuesOf?: (key: string) => string[] };
  try {
    const zones = intl.supportedValuesOf?.("timeZone") ?? [];
    return zones.includes("UTC") ? zones : ["UTC", ...zones];
  } catch {
    return [];
  }
}

/**
 * Vacío = automática (02 §5.2). Si el motor da la lista de zonas, se exige una de
 * ella; si no, al menos la forma `Región/Ciudad` o `UTC`.
 */
export function validateTimezone(value: string, known: readonly string[]): string | null {
  const tz = value.trim();
  if (tz === "") return null;
  if (known.length > 0) {
    return known.includes(tz) ? null : `«${tz}» no es una zona horaria conocida (por ejemplo, Europe/Madrid o UTC).`;
  }
  return /^(?:UTC|[A-Za-z_]+(?:\/[A-Za-z0-9_+-]+)+)$/.test(tz)
    ? null
    : `«${tz}» no parece una zona horaria (por ejemplo, Europe/Madrid o UTC).`;
}

/** Lo que se envía con `logs_set_override`. */
export function buildOverride(timezone: string, globsText: string): ProjectOverride {
  const tz = timezone.trim();
  return { log_globs: parseGlobs(globsText).globs, timezone: tz === "" ? null : tz };
}
