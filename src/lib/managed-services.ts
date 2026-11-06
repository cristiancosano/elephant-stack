// Una sola lista por servicio propio (06): las versiones instaladas y las del
// catálogo que aún no lo están, como lib/php-versions con PHP. Cada rama
// (8.4, 9.7…) ejecuta su versión más reciente; las demás de la rama no se usan.

import { compareVersions } from "./php-versions";
import type { BranchStatus } from "./bindings/BranchStatus";
import type { InstalledVersion } from "./bindings/InstalledVersion";
import type { ManagedService } from "./bindings/ManagedService";
import type { ServiceRelease } from "./bindings/ServiceRelease";

export type ManagedEntry =
  | {
      kind: "installed";
      key: string;
      version: InstalledVersion;
      /** Estado de su rama, solo en la versión que la ejecuta. */
      branch: BranchStatus | null;
      /** Versión más reciente de la misma rama en el catálogo, si la hay. */
      update: ServiceRelease | null;
      /** La que ejecuta su rama, si no es esta. */
      usedInstead: string | null;
    }
  | { kind: "available"; key: string; release: ServiceRelease };

/** La más reciente de la rama, si es posterior a `version` y no está instalada. */
function newerInBranch(version: string, branch: string, releases: ServiceRelease[], installed: Set<string>) {
  return (
    releases
      .filter((r) => r.branch === branch && !installed.has(r.version) && compareVersions(r.version, version) < 0)
      .sort((a, b) => compareVersions(a.version, b.version))[0] ?? null
  );
}

/**
 * Une instaladas y disponibles. De una rama ya instalada no se ofrecen otras
 * versiones (la más reciente llega como «Actualizar»); de las demás, solo la
 * última de cada rama.
 */
export function mergeManagedVersions(service: ManagedService, catalog: ServiceRelease[]): ManagedEntry[] {
  const releases = catalog.filter((r) => r.service === service.service);
  const installed = new Set(service.versions.map((v) => v.version));
  const branches = new Map(service.branches.map((b) => [b.branch, b]));

  const entries: ManagedEntry[] = service.versions.map((version) => {
    const branch = branches.get(version.branch) ?? null;
    const inUse = branch?.version === version.version;
    return {
      kind: "installed",
      key: `installed:${version.version}`,
      version,
      branch: inUse ? branch : null,
      update: inUse ? newerInBranch(version.version, version.branch, releases, installed) : null,
      usedInstead: inUse ? null : (branch?.version ?? null),
    };
  });

  const offered = new Set(service.versions.map((v) => v.branch));
  const newestFirst = [...releases].sort((a, b) => compareVersions(a.version, b.version));
  for (const release of newestFirst) {
    if (offered.has(release.branch)) continue;
    offered.add(release.branch);
    entries.push({ kind: "available", key: `release:${release.version}`, release });
  }

  const versionOf = (e: ManagedEntry) => (e.kind === "installed" ? e.version.version : e.release.version);
  return entries.sort((a, b) => compareVersions(versionOf(a), versionOf(b)) || a.key.localeCompare(b.key));
}

/** La rama de un servicio que está en marcha, si hay una. */
export function runningBranch(service: ManagedService): BranchStatus | null {
  return service.branches.find((b) => b.state === "running") ?? null;
}
