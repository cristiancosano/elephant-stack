// Modo «Agrupados»: lo que llega en directo se suma a su grupo por huella, sin volver
// a pedir `logs_groups`. Puro para poder probarlo.

import type { LogEntry } from "../bindings/LogEntry";
import type { LogGroup } from "../bindings/LogGroup";

/** Ordenados por la última vez que se vieron, los más recientes primero. */
export function sortGroups(groups: LogGroup[]): LogGroup[] {
  return groups.sort((a, b) => b.last_ms - a.last_ms || b.count - a.count);
}

export function mergeIntoGroups(groups: readonly LogGroup[], entries: readonly LogEntry[]): LogGroup[] {
  if (entries.length === 0) return groups.slice();
  const byFingerprint = new Map(groups.map((g) => [g.fingerprint, { ...g, projects: g.projects.slice() }]));
  for (const e of entries) {
    const g = byFingerprint.get(e.fingerprint);
    if (!g) {
      byFingerprint.set(e.fingerprint, {
        fingerprint: e.fingerprint,
        count: 1,
        first_ms: e.ts_ms,
        last_ms: e.ts_ms,
        sample: e,
        projects: e.project ? [e.project] : [],
      });
      continue;
    }
    g.count += 1;
    g.first_ms = Math.min(g.first_ms, e.ts_ms);
    if (e.ts_ms >= g.last_ms) {
      g.last_ms = e.ts_ms;
      g.sample = e;
    }
    if (e.project && !g.projects.includes(e.project)) g.projects.push(e.project);
  }
  return sortGroups([...byFingerprint.values()]);
}
