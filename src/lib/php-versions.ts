// Una sola lista de versiones de PHP de Elephant: las instaladas y las de
// php-builds que aún no están instaladas. Las versiones de otros programas
// (Herd) no se listan: el backend ya no las envía.

import type { PhpRelease } from "./bindings/PhpRelease";
import type { VersionInfo } from "./bindings/VersionInfo";

export type PhpEntry =
  | {
      kind: "installed";
      key: string;
      id: string;
      version: VersionInfo;
      /** Versión más reciente de la misma rama (8.4.27 → 8.4.28), si la hay. */
      update: PhpRelease | null;
    }
  | { kind: "available"; key: string; id: string; release: PhpRelease };

function versionKey(id: string): number[] {
  return id.split(".").map((part) => Number.parseInt(part, 10) || 0);
}

/** De la más reciente a la más antigua; `8.4` va justo detrás de las 8.4.x. */
export function compareVersions(a: string, b: string): number {
  const ka = versionKey(a);
  const kb = versionKey(b);
  for (let i = 0; i < Math.max(ka.length, kb.length); i++) {
    const diff = (kb[i] ?? -1) - (ka[i] ?? -1);
    if (diff !== 0) return diff;
  }
  return 0;
}

/** `8.4` de `8.4.27`, o null si la versión no tiene tres partes numéricas. */
export function branchOf(version: string): string | null {
  return /^\d+\.\d+\.\d+$/.test(version) ? version.split(".").slice(0, 2).join(".") : null;
}

/**
 * La release más reciente de la rama de `installed`, si es posterior a ella y
 * no está ya instalada.
 */
function newerInBranch(installed: string, releases: PhpRelease[], installedIds: Set<string>) {
  const branch = branchOf(installed);
  if (!branch) return null;
  const newer = releases
    .filter((r) => branchOf(r.version) === branch && !installedIds.has(r.version))
    .filter((r) => compareVersions(r.version, installed) < 0)
    .sort((a, b) => compareVersions(a.version, b.version));
  return newer[0] ?? null;
}

/**
 * Une instaladas y disponibles. Una release ya instalada no se repite, y la
 * que actualiza a una instalada aparece como su «Actualizar», no como otra fila.
 */
export function mergePhpVersions(installed: VersionInfo[], releases: PhpRelease[]): PhpEntry[] {
  const installedIds = new Set(installed.map((v) => v.id));

  const installedEntries = installed.map((version) => ({
    kind: "installed" as const,
    key: version.key,
    id: version.id,
    version,
    update: newerInBranch(version.id, releases, installedIds),
  }));
  // Con una versión instalada de una rama, el resto de esa rama no se ofrece
  // como fila aparte: la más reciente llega como «Actualizar» y las demás
  // (parches intermedios o anteriores) no aportan nada.
  const installedBranches = new Set(installed.flatMap((v) => branchOf(v.id) ?? []));

  const entries: PhpEntry[] = [
    ...installedEntries,
    ...releases
      .filter((r) => !installedIds.has(r.version) && !installedBranches.has(branchOf(r.version) ?? ""))
      .map((release) => ({
        kind: "available" as const,
        key: `release:${release.version}`,
        id: release.version,
        release,
      })),
  ];
  return entries.sort((a, b) => compareVersions(a.id, b.id) || a.key.localeCompare(b.key));
}
