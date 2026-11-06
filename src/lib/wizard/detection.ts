// Bienvenida: lo que el asistente ha encontrado en el Mac, en frases cortas
// («Herd en marcha · 12 sitios .test», «PostgreSQL 18 en marcha»; 04 §4.11).

import type { Detection } from "../bindings/Detection";
import { plural } from "../format";
import { OWN_SOURCE, branchOf, serviceLabel } from "./customize";

export type FindingTone = "ok" | "info" | "warning";

export interface Finding {
  id: string;
  text: string;
  tone: FindingTone;
}

function joinList(items: string[]): string {
  if (items.length <= 1) return items.join("");
  return `${items.slice(0, -1).join(", ")} y ${items[items.length - 1]}`;
}

export function findings(d: Detection): Finding[] {
  const out: Finding[] = [];

  if (!d.arch_arm64) {
    out.push({ id: "arch", text: "Este Mac no es Apple Silicon: los PHP de Elephant no funcionarán", tone: "warning" });
  }

  if (d.formulas.length > 0) {
    out.push({ id: "brew", text: `Homebrew · ${plural(d.formulas.length, "fórmula")}`, tone: "ok" });
  } else {
    out.push({ id: "brew", text: "Sin fórmulas de Homebrew", tone: "info" });
  }

  if (d.herd) {
    const parts = [d.herd.running ? "Herd en marcha" : "Herd instalado"];
    if (d.herd.sites !== null) {
      const tld = d.herd.tld ? ` .${d.herd.tld.replace(/^\./, "")}` : "";
      parts.push(`${plural(d.herd.sites, "sitio")}${tld}`);
    }
    out.push({ id: "herd", text: parts.join(" · "), tone: "ok" });
  }

  const own = d.php_versions.filter((v) => v.source === OWN_SOURCE).map((v) => v.id);
  const others = new Map<string, string[]>();
  for (const v of d.php_versions) {
    if (v.source === OWN_SOURCE) continue;
    const list = others.get(v.source) ?? [];
    const branch = branchOf(v.id);
    if (!list.includes(branch)) list.push(branch);
    others.set(v.source, list);
  }
  const php: string[] = [];
  if (own.length) php.push(`PHP ${joinList(own)} ${own.length === 1 ? "propio" : "propios"}`);
  for (const [origin, branches] of others) php.push(`PHP ${joinList(branches)} de ${origin}`);
  out.push(
    php.length
      ? { id: "php", text: php.join(" · "), tone: "ok" }
      : { id: "php", text: "Sin ninguna versión de PHP", tone: "info" },
  );

  if (d.composer_phar) out.push({ id: "composer", text: "Composer propio", tone: "ok" });

  for (const s of d.services) {
    if (!s.running) continue;
    out.push({ id: `svc:${s.name}`, text: `${serviceLabel(s.name).name} en marcha`, tone: "ok" });
  }

  if (d.path === "active") {
    out.push({ id: "path", text: "Selector php en la terminal", tone: "ok" });
  } else if (d.path === "shadowed_by_herd") {
    out.push({ id: "path", text: "El php de Herd va antes que el de Elephant en la terminal", tone: "info" });
  }

  for (const p of d.ports) {
    if (!p.busy || !p.owner_hint) continue;
    out.push({ id: `port:${p.port}`, text: `El puerto ${p.port} lo usa ${p.owner_hint}`, tone: "info" });
  }

  return out;
}

/** Banda de la bienvenida cuando Herd está instalado. */
export function herdBanner(d: Detection | null): string | null {
  if (!d?.herd) return null;
  return "Herd está instalado. Elephant convive con él: no toca sus sitios, su PHP ni sus servicios.";
}
