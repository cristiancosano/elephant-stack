// Una sola lista de versiones de PHP: las instaladas (propias o de Herd) y las
// de php-builds que aún no están instaladas.

import type { PhpRelease } from "./bindings/PhpRelease";
import type { VersionInfo } from "./bindings/VersionInfo";

export type PhpEntry =
  | { kind: "installed"; key: string; id: string; version: VersionInfo; release: PhpRelease | null }
  | { kind: "available"; key: string; id: string; release: PhpRelease };

function versionKey(id: string): number[] {
  return id.split(".").map((part) => Number.parseInt(part, 10) || 0);
}

/** De la más reciente a la más antigua; `8.4` (Herd) va justo detrás de las 8.4.x. */
export function compareVersions(a: string, b: string): number {
  const ka = versionKey(a);
  const kb = versionKey(b);
  for (let i = 0; i < Math.max(ka.length, kb.length); i++) {
    const diff = (kb[i] ?? -1) - (ka[i] ?? -1);
    if (diff !== 0) return diff;
  }
  return 0;
}

/**
 * Une instaladas y disponibles. Una release cuya versión ya está instalada no
 * se repite: se asocia a la fila instalada. La `8.4` de Herd es otra
 * instalación distinta de la `8.4.27` propia, así que ambas aparecen.
 */
export function mergePhpVersions(installed: VersionInfo[], releases: PhpRelease[]): PhpEntry[] {
  const byVersion = new Map(releases.map((r) => [r.version, r]));
  const installedIds = new Set(installed.map((v) => v.id));

  const entries: PhpEntry[] = [
    ...installed.map((version) => ({
      kind: "installed" as const,
      key: version.key,
      id: version.id,
      version,
      release: byVersion.get(version.id) ?? null,
    })),
    ...releases
      .filter((r) => !installedIds.has(r.version))
      .map((release) => ({
        kind: "available" as const,
        key: `release:${release.version}`,
        id: release.version,
        release,
      })),
  ];
  return entries.sort((a, b) => compareVersions(a.id, b.id) || a.key.localeCompare(b.key));
}
