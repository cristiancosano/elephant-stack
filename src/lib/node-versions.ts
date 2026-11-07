// Una sola lista de versiones de Node de Elephant: las instaladas y las de
// nodejs.org que aún no lo están (06, fase 3). Como la de PHP, pero por versión
// mayor: una release nueva de una mayor instalada llega como «Actualizar».

import type { NodeInstalled } from "./bindings/NodeInstalled";
import type { NodeRelease } from "./bindings/NodeRelease";
import { compareVersions } from "./php-versions";

export type NodeEntry =
  | {
      kind: "installed";
      key: string;
      id: string;
      version: NodeInstalled;
      /** La más reciente de su mayor (24.11.1 → 24.22.2), si no está ya instalada. */
      update: NodeRelease | null;
    }
  | { kind: "available"; key: string; id: string; release: NodeRelease };

/**
 * Une instaladas y disponibles. Por defecto solo se ofrecen las destacadas
 * (LTS con soporte y la Current); con `all`, la más reciente de cada mayor.
 */
export function mergeNodeVersions(installed: NodeInstalled[], releases: NodeRelease[], all = false): NodeEntry[] {
  const ids = new Set(installed.map((v) => v.version));
  const majors = new Set(installed.map((v) => v.major));
  const newestInstalled = new Map<number, string>();
  for (const v of installed) {
    const known = newestInstalled.get(v.major);
    if (!known || compareVersions(v.version, known) < 0) newestInstalled.set(v.major, v.version);
  }
  const entries: NodeEntry[] = [
    ...installed.map((version) => {
      const release = releases.find((r) => r.major === version.major) ?? null;
      // Solo la más reciente de su mayor se ofrece a actualizar.
      const newer =
        release &&
        !ids.has(release.version) &&
        newestInstalled.get(version.major) === version.version &&
        compareVersions(release.version, version.version) < 0;
      return { kind: "installed" as const, key: version.version, id: version.version, version, update: newer ? release : null };
    }),
    ...releases
      .filter((r) => !majors.has(r.major) && (all || r.featured))
      .map((release) => ({ kind: "available" as const, key: `release:${release.version}`, id: release.version, release })),
  ];
  return entries.sort((a, b) => compareVersions(a.id, b.id) || a.key.localeCompare(b.key));
}

/** Las que no se muestran sin «Mostrar todas»: para saber si hace falta el botón. */
export function hiddenReleases(installed: NodeInstalled[], releases: NodeRelease[]): number {
  const majors = new Set(installed.map((v) => v.major));
  return releases.filter((r) => !majors.has(r.major) && !r.featured).length;
}

/**
 * Lo que se escribe en el ajuste global o en el `.nvmrc` al elegir una versión:
 * su mayor (`24`), que sigue valiendo tras actualizar, salvo que haya otra más
 * reciente de esa mayor instalada; entonces, la versión exacta.
 */
export function specFor(version: string, installed: NodeInstalled[]): string {
  const major = version.split(".")[0] ?? version;
  const newer = installed.some((v) => v.version.split(".")[0] === major && compareVersions(v.version, version) < 0);
  return newer ? version : major;
}

/** `Krypton` de `krypton`. */
export function ltsName(lts: string): string {
  return lts.charAt(0).toUpperCase() + lts.slice(1);
}
