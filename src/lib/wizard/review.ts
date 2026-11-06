// Pantalla de revisión: el plan agrupado como lo lee una persona (04 §4.11).
// Puro: la vista solo pinta lo que devuelve `reviewSections`.

import type { Plan } from "../bindings/Plan";
import type { Step } from "../bindings/Step";
import type { StepGroup } from "../bindings/StepGroup";
import { formatBytes } from "../format";
import "../i18n/locales/wizard";
import { t } from "../i18n/index.svelte";
import { runnableSteps } from "./run";
import { idleRam } from "./scenario";

export interface ReviewSection {
  id: Exclude<StepGroup, "optional">;
  title: string;
  steps: Step[];
  /** Información extra bajo el grupo (MB de descarga, aviso de Homebrew…). */
  footer: string | null;
}

function sectionTitle(id: ReviewSection["id"]): string {
  switch (id) {
    case "download":
      return t("wizard.review.kind.download");
    case "brew_install":
      return t("wizard.review.kind.brewInstall");
    case "configure":
      return t("wizard.review.kind.configure");
    case "start":
      return t("wizard.review.kind.start");
  }
}

const ORDER: ReviewSection["id"][] = ["download", "brew_install", "configure", "start"];

/**
 * Avisos que se enseñan en la revisión. Si se muestra el aviso del PATH con su
 * botón (`setup_shell_preview` con `needed`), el del plan sobra.
 */
export function visibleNotices(plan: Plan, pathNoticeShown: boolean): string[] {
  // El backend marca cuál es el aviso del PATH (`path_notice`): no se compara el texto.
  return pathNoticeShown ? plan.notices.filter((n) => n !== plan.path_notice) : plan.notices;
}

/** Bytes que descarga un paso (solo las descargas de PHP traen tamaño). */
export function stepBytes(step: Step): number {
  return step.action.kind === "download_php" ? step.action.release.size : 0;
}

/** Los pasos obligatorios por grupo, en el orden del plan. Los opcionales van aparte. */
export function reviewSections(plan: Plan): ReviewSection[] {
  const sections: ReviewSection[] = [];
  for (const id of ORDER) {
    const steps = plan.steps.filter((s) => !s.optional && s.group === id);
    if (steps.length === 0) continue;
    let footer: string | null = null;
    if (id === "download") {
      const bytes = plan.totals.download_bytes || steps.reduce((sum, s) => sum + stepBytes(s), 0);
      if (bytes > 0) footer = t("wizard.review.totalSize", { size: formatBytes(bytes) });
    } else if (id === "brew_install" && steps.some((s) => s.action.kind === "brew_install")) {
      footer = t("wizard.review.brewFooter");
    }
    sections.push({ id, title: sectionTitle(id), steps, footer });
  }
  return sections;
}

/** Los pasos que el usuario decide (vienen desmarcados). */
export function optionalSteps(plan: Plan): Step[] {
  return plan.steps.filter((s) => s.optional);
}

/** Texto secundario de un opcional: lo que el título no dice. */
export function optionalNote(step: Step): string | null {
  return step.restarts_service ? t("wizard.review.restarts") : null;
}

/** Plan vacío: el Mac ya está como pide el escenario. */
export function isEmptyPlan(plan: Plan): boolean {
  return plan.steps.length === 0;
}

/** Pasos que se ejecutarán con lo marcado ahora. */
export function applyCount(plan: Plan, accepted: readonly string[]): number {
  return runnableSteps(plan, accepted).length;
}

/** Botón principal de la revisión: «Aplicar (N pasos)» o «Hecho» si no hay nada que hacer. */
export function applyLabel(plan: Plan, accepted: readonly string[]): string {
  const n = applyCount(plan, accepted);
  return n === 0 ? t("wizard.review.done") : t("wizard.review.apply", { count: n });
}

/** Solo las claves opcionales que existen en el plan: el backend ignora las demás. */
export function sanitizeAccepted(plan: Plan, accepted: readonly string[]): string[] {
  const optional = new Set(optionalSteps(plan).map((s) => s.key));
  return [...new Set(accepted)].filter((k) => optional.has(k));
}

/** Resumen de la cabecera: «≈ 25 MB en reposo · 65 MB de descarga». */
export function totalsLine(plan: Plan): string {
  const parts = [idleRam(plan.totals.idle_ram_mb)];
  if (plan.totals.download_bytes > 0)
    parts.push(t("wizard.review.download", { size: formatBytes(plan.totals.download_bytes) }));
  if (plan.totals.brew_formulas > 0) parts.push(t("wizard.review.formulas", { count: plan.totals.brew_formulas }));
  return parts.join(" · ");
}
