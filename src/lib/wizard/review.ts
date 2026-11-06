// Pantalla de revisión: el plan agrupado como lo lee una persona (04 §4.11).
// Puro: la vista solo pinta lo que devuelve `reviewSections`.

import type { Plan } from "../bindings/Plan";
import type { Step } from "../bindings/Step";
import type { StepGroup } from "../bindings/StepGroup";
import { formatBytes, plural } from "../format";
import { runnableSteps } from "./run";
import { idleRam } from "./scenario";

export interface ReviewSection {
  id: Exclude<StepGroup, "optional">;
  title: string;
  steps: Step[];
  /** Información extra bajo el grupo (MB de descarga, aviso de Homebrew…). */
  footer: string | null;
}

const TITLES: Record<ReviewSection["id"], string> = {
  download: "Se descargará",
  brew_install: "Se instalará con Homebrew",
  configure: "Se configurará",
  start: "Arrancará al iniciar sesión",
};

const ORDER: ReviewSection["id"][] = ["download", "brew_install", "configure", "start"];

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
      if (bytes > 0) footer = `${formatBytes(bytes)} en total.`;
    } else if (id === "brew_install" && steps.some((s) => s.action.kind === "brew_install")) {
      footer = "Homebrew puede actualizar librerías que estas fórmulas necesitan.";
    }
    sections.push({ id, title: TITLES[id], steps, footer });
  }
  return sections;
}

/** Los pasos que el usuario decide (vienen desmarcados). */
export function optionalSteps(plan: Plan): Step[] {
  return plan.steps.filter((s) => s.optional);
}

/** Texto secundario de un opcional: lo que el título no dice. */
export function optionalNote(step: Step): string | null {
  return step.restarts_service ? "El servicio se reiniciará un momento." : null;
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
  return n === 0 ? "Hecho" : `Aplicar (${plural(n, "paso")})`;
}

/** Solo las claves opcionales que existen en el plan: el backend ignora las demás. */
export function sanitizeAccepted(plan: Plan, accepted: readonly string[]): string[] {
  const optional = new Set(optionalSteps(plan).map((s) => s.key));
  return [...new Set(accepted)].filter((k) => optional.has(k));
}

/** Resumen de la cabecera: «≈ 25 MB en reposo · 65 MB de descarga». */
export function totalsLine(plan: Plan): string {
  const parts = [idleRam(plan.totals.idle_ram_mb)];
  if (plan.totals.download_bytes > 0) parts.push(`${formatBytes(plan.totals.download_bytes)} de descarga`);
  if (plan.totals.brew_formulas > 0) parts.push(plural(plan.totals.brew_formulas, "fórmula"));
  return parts.join(" · ");
}
