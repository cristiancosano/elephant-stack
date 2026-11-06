// Pantalla Personalizar: el perfil como tabla editable (04 §4.11, C9 y C10).
// Puro: cada edición devuelve un perfil nuevo y no toca el anterior.

import type { Catalog } from "../bindings/Catalog";
import type { Detection } from "../bindings/Detection";
import type { Profile } from "../bindings/Profile";
import type { ServiceWant } from "../bindings/ServiceWant";
import type { VersionInfo } from "../bindings/VersionInfo";

/** Origen de las versiones propias en `VersionInfo.source`. */
export const OWN_SOURCE = "Elephant Stack";

/** El control de tres estados de cada servicio (C9). */
export const SERVICE_WANT_OPTIONS: { value: ServiceWant; label: string }[] = [
  { value: "keep", label: "No tocar" },
  { value: "installed", label: "Instalado y apagado" },
  { value: "autostart", label: "Al iniciar sesión" },
];

/**
 * Servicios que el asistente gestiona en esta versión (nginx y dnsmasq no entran,
 * 04 §4.9). Si el backend manda otros en el perfil, también se muestran.
 */
export const WIZARD_SERVICES = ["mysql", "postgresql@18", "redis", "mailpit"] as const;

const SERVICE_NAMES: Record<string, { name: string; purpose: string }> = {
  mysql: { name: "MySQL", purpose: "Base de datos" },
  mariadb: { name: "MariaDB", purpose: "Base de datos" },
  postgresql: { name: "PostgreSQL", purpose: "Base de datos" },
  redis: { name: "Redis", purpose: "Caché y colas" },
  mailpit: { name: "Mailpit", purpose: "Correo de pruebas" },
  nginx: { name: "nginx", purpose: "Servidor web" },
  dnsmasq: { name: "dnsmasq", purpose: "DNS para dominios .test" },
};

/** `postgresql@18` → «PostgreSQL 18»; un id desconocido se muestra tal cual. */
export function serviceLabel(id: string): { name: string; purpose: string | null } {
  const [base, version] = id.split("@");
  const known = SERVICE_NAMES[base];
  if (!known) return { name: id, purpose: null };
  return { name: version ? `${known.name} ${version}` : known.name, purpose: known.purpose };
}

/** Filas de la tabla de servicios: las del perfil más las conocidas, en orden estable. */
export function serviceRows(profile: Profile): { id: string; want: ServiceWant }[] {
  const ids: string[] = [...WIZARD_SERVICES];
  for (const id of Object.keys(profile.services).sort()) if (!ids.includes(id)) ids.push(id);
  return ids.map((id) => ({ id, want: profile.services[id] ?? "keep" }));
}

export function setService(profile: Profile, id: string, want: ServiceWant): Profile {
  return { ...profile, services: { ...profile.services, [id]: want } };
}

/** `8.4.27` → `8.4`. */
export function branchOf(version: string): string {
  return version.split(".").slice(0, 2).join(".");
}

function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map((n) => Number.parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

export function ownVersions(detection: Detection | null): VersionInfo[] {
  return (detection?.php_versions ?? []).filter((v) => v.source === OWN_SOURCE);
}

export interface PhpBranch {
  branch: string;
  /** Versión propia ya instalada de la rama, la más alta. */
  installed: string | null;
  /** Último parche publicado en php-builds, si hay catálogo. */
  latest: string | null;
  /** Bytes que se descargarían si hay que bajarla. */
  size: number;
}

/** Ramas que se pueden elegir: las del catálogo y las propias instaladas, de la más nueva a la más vieja. */
export function phpBranches(catalog: Catalog | null, detection: Detection | null): PhpBranch[] {
  const map = new Map<string, PhpBranch>();
  const get = (branch: string) => {
    let b = map.get(branch);
    if (!b) {
      b = { branch, installed: null, latest: null, size: 0 };
      map.set(branch, b);
    }
    return b;
  };
  for (const release of catalog?.php ?? []) {
    const b = get(branchOf(release.version));
    if (!b.latest || compareVersions(release.version, b.latest) > 0) {
      b.latest = release.version;
      b.size = release.size;
    }
  }
  for (const v of ownVersions(detection)) {
    const b = get(branchOf(v.id));
    if (!b.installed || compareVersions(v.id, b.installed) > 0) b.installed = v.id;
  }
  return [...map.values()].sort((a, b) => compareVersions(b.branch, a.branch));
}

/**
 * Ramas marcadas según el perfil. `any_own` («cualquier versión propia») se pinta
 * como las ramas propias que ya hay o, si no hay ninguna, la última del catálogo.
 */
export function checkedBranches(profile: Profile, branches: readonly PhpBranch[]): string[] {
  switch (profile.php.kind) {
    case "none":
      return [];
    case "branches":
      return [...profile.php.value];
    case "any_own": {
      const own = branches.filter((b) => b.installed).map((b) => b.branch);
      if (own.length > 0) return own;
      const newest = branches.find((b) => b.latest);
      return newest ? [newest.branch] : [];
    }
  }
}

/** Marca o desmarca una rama. Sin ninguna, el perfil pasa a «sin PHP propio». */
export function setBranch(profile: Profile, branches: readonly PhpBranch[], branch: string, on: boolean): Profile {
  const current = new Set(checkedBranches(profile, branches));
  if (on) current.add(branch);
  else current.delete(branch);
  const value = branches.map((b) => b.branch).filter((b) => current.has(b));
  // Ramas marcadas que no están en la lista (no debería pasar) se conservan al final.
  for (const b of current) if (!value.includes(b)) value.push(b);
  const next: Profile = { ...profile, php: value.length ? { kind: "branches", value } : { kind: "none" } };
  // La versión por defecto tiene que seguir siendo una de las elegidas.
  if (next.default_php && !defaultPhpOptions(next, branches).some((o) => o.value === next.default_php)) {
    next.default_php = null;
  }
  return next;
}

export interface DefaultPhpOption {
  /** `null` = «la que decida el plan». */
  value: string | null;
  label: string;
}

/** Radio de la versión por defecto: una por rama marcada, más la automática. */
export function defaultPhpOptions(profile: Profile, branches: readonly PhpBranch[]): DefaultPhpOption[] {
  const options: DefaultPhpOption[] = [{ value: null, label: "Automática (la más reciente)" }];
  const checked = new Set(checkedBranches(profile, branches));
  for (const b of branches) {
    if (!checked.has(b.branch)) continue;
    const version = b.installed ?? b.latest;
    if (!version) continue;
    options.push({ value: version, label: b.installed ? `PHP ${version}` : `PHP ${version} (se descargará)` });
  }
  return options;
}

export function setDefaultPhp(profile: Profile, version: string | null): Profile {
  return { ...profile, default_php: version };
}

export type Toggle = "composer" | "shim" | "launch_at_login";

export const TOGGLES: { key: Toggle; title: string; subtitle: string }[] = [
  { key: "composer", title: "Composer", subtitle: "Su propio composer.phar, que usa el PHP de cada proyecto." },
  { key: "shim", title: "Selector php", subtitle: "El comando php de la terminal elige la versión de cada proyecto." },
  { key: "launch_at_login", title: "Abrir Elephant al iniciar sesión", subtitle: "Solo en la barra de menús." },
];

export function setToggle(profile: Profile, key: Toggle, value: boolean): Profile {
  return { ...profile, [key]: value };
}
