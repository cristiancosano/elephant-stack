// Bienvenida: lo que el asistente ha encontrado en el Mac, en frases cortas
// («Herd en marcha · 12 sitios .test», «PostgreSQL 18 en marcha»; 04 §4.11).

import type { Detection } from "../bindings/Detection";
import "../i18n/locales/wizard";
import { fmt, t } from "../i18n/index.svelte";
import { OWN_SOURCE, branchOf, serviceLabel } from "./customize";

export type FindingTone = "ok" | "info" | "warning";

export interface Finding {
  id: string;
  text: string;
  tone: FindingTone;
}

export function findings(d: Detection): Finding[] {
  const out: Finding[] = [];

  if (!d.arch_arm64) {
    out.push({ id: "arch", text: t("wizard.detect.notAppleSilicon"), tone: "warning" });
  }

  if (d.formulas.length > 0) {
    out.push({ id: "brew", text: t("wizard.detect.brew", { count: d.formulas.length }), tone: "ok" });
  } else {
    out.push({ id: "brew", text: t("wizard.detect.noBrew"), tone: "info" });
  }

  if (d.herd) {
    const parts = [d.herd.running ? t("wizard.detect.herdRunning") : t("wizard.detect.herdInstalled")];
    if (d.herd.sites !== null) {
      const tld = d.herd.tld ? ` .${d.herd.tld.replace(/^\./, "")}` : "";
      parts.push(t("wizard.detect.herdSites", { count: d.herd.sites, tld }));
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
  if (own.length) php.push(t("wizard.detect.phpOwn", { count: own.length, list: fmt.list(own) }));
  for (const [origin, branches] of others) php.push(t("wizard.detect.php", { list: fmt.list(branches), origin }));
  out.push(
    php.length
      ? { id: "php", text: php.join(" · "), tone: "ok" }
      : { id: "php", text: t("wizard.detect.noPhp"), tone: "info" },
  );

  if (d.composer_phar) out.push({ id: "composer", text: t("wizard.detect.composer"), tone: "ok" });

  for (const s of d.services) {
    if (!s.running) continue;
    out.push({ id: `svc:${s.name}`, text: t("wizard.detect.running", { name: serviceLabel(s.name).name }), tone: "ok" });
  }

  if (d.path === "active") {
    out.push({ id: "path", text: t("wizard.detect.shim"), tone: "ok" });
  } else if (d.path === "shadowed_by_herd") {
    out.push({ id: "path", text: t("wizard.detect.herdFirst"), tone: "info" });
  }

  for (const p of d.ports) {
    if (!p.busy || !p.owner_hint) continue;
    out.push({ id: `port:${p.port}`, text: t("wizard.detect.port", { port: String(p.port), owner: p.owner_hint }), tone: "info" });
  }

  return out;
}

/** Banda de la bienvenida cuando Herd está instalado. */
export function herdBanner(d: Detection | null): string | null {
  if (!d?.herd) return null;
  return t("wizard.detect.herd");
}
