// Progreso de `setup_apply`: una fila por paso, alimentada por los eventos del
// canal y cerrada con el informe final (04 §4.5 y §4.11). Puro e inmutable.

import type { ApplyReport } from "../bindings/ApplyReport";
import type { Plan } from "../bindings/Plan";
import type { SetupProgress } from "../bindings/SetupProgress";
import type { StepState } from "../bindings/StepState";
import { plural } from "../format";

/** Líneas que se guardan por paso para «Ver salida»; el log completo está en disco. */
export const MAX_LOG_LINES = 400;

export interface RunStep {
  key: string;
  title: string;
  state: StepState;
  /** Última línea de brew o curl. */
  line: string | null;
  /** Descargas: [hecho, total]. */
  bytes: [number, number] | null;
  reason: string | null;
  log: string[];
}

export interface Run {
  steps: RunStep[];
  finished: boolean;
  cancelling: boolean;
  report: ApplyReport | null;
  /** El comando falló entero (otra ejecución en marcha, función no disponible…). */
  error: string | null;
}

/** Los pasos que se van a ejecutar: los obligatorios y los opcionales marcados, en orden. */
export function runnableSteps(plan: Plan, accepted: readonly string[]) {
  const chosen = new Set(accepted);
  return plan.steps.filter((s) => !s.optional || chosen.has(s.key));
}

export function startRun(plan: Plan, accepted: readonly string[]): Run {
  return {
    steps: runnableSteps(plan, accepted).map((s) => ({
      key: s.key,
      title: s.title,
      state: "pending",
      line: null,
      bytes: null,
      reason: null,
      log: [],
    })),
    finished: false,
    cancelling: false,
    report: null,
    error: null,
  };
}

function appendLog(log: string[], line: string | null): string[] {
  if (!line) return log;
  const next = log.length >= MAX_LOG_LINES ? log.slice(log.length - MAX_LOG_LINES + 1) : log.slice();
  next.push(line);
  return next;
}

/** Aplica un evento. Un paso que no estaba en el plan (no debería ocurrir) se añade al final. */
export function applyProgress(run: Run, e: SetupProgress): Run {
  const index = run.steps.findIndex((s) => s.key === e.key);
  const base: RunStep =
    index === -1
      ? { key: e.key, title: e.key, state: "pending", line: null, bytes: null, reason: null, log: [] }
      : run.steps[index];
  const step: RunStep = {
    ...base,
    state: e.state,
    line: e.line ?? base.line,
    bytes: e.bytes ?? base.bytes,
    reason: e.reason ?? base.reason,
    log: appendLog(base.log, e.line),
  };
  const steps = run.steps.slice();
  if (index === -1) steps.push(step);
  else steps[index] = step;
  return { ...run, steps };
}

/**
 * Cierra la ejecución con el informe, que manda sobre los eventos (alguno pudo
 * perderse). Lo que siga pendiente o en curso queda como saltado.
 */
export function finishRun(run: Run, report: ApplyReport): Run {
  const done = new Set(report.done);
  const failed = new Map(report.failed);
  const skipped = new Map(report.skipped);
  const steps = run.steps.map((s): RunStep => {
    if (done.has(s.key)) return { ...s, state: "ok" };
    if (failed.has(s.key)) return { ...s, state: "failed", reason: failed.get(s.key) ?? s.reason };
    if (skipped.has(s.key)) return { ...s, state: "skipped", reason: skipped.get(s.key) ?? s.reason };
    if (s.state === "pending" || s.state === "running") {
      return { ...s, state: "skipped", reason: s.reason ?? "No se ejecutó" };
    }
    return s;
  });
  return { ...run, steps, finished: true, cancelling: false, report };
}

/** El comando entero falló: no se ha ejecutado nada más. */
export function failRun(run: Run, error: string): Run {
  return { ...run, finished: true, cancelling: false, error };
}

export interface RunSummary {
  ok: number;
  failed: number;
  skipped: number;
  running: number;
  pending: number;
  /** Fracción de 0 a 1 de pasos terminados (para la barra global). */
  fraction: number;
}

export function summarize(run: Run): RunSummary {
  const s = { ok: 0, failed: 0, skipped: 0, running: 0, pending: 0 };
  for (const step of run.steps) {
    if (step.state === "ok") s.ok += 1;
    else if (step.state === "failed") s.failed += 1;
    else if (step.state === "skipped") s.skipped += 1;
    else if (step.state === "running") s.running += 1;
    else s.pending += 1;
  }
  const total = run.steps.length;
  const fraction = total === 0 ? 1 : (s.ok + s.failed + s.skipped) / total;
  return { ...s, fraction };
}

/** Hay algo que «Reintentar lo que falta» puede arreglar. */
export function canRetry(run: Run): boolean {
  if (!run.finished) return false;
  if (run.error) return true;
  const s = summarize(run);
  return s.failed > 0 || s.skipped > 0;
}

/** Titular del resultado: habla de consecuencias, no de comandos. */
export function headline(run: Run): string {
  if (!run.finished) return run.cancelling ? "Cancelando…" : "Aplicando los cambios…";
  if (run.error) return "No se ha podido aplicar";
  const s = summarize(run);
  if (run.steps.length === 0) return "Tu Mac ya estaba listo";
  if (s.failed === 0 && s.skipped === 0) return "Todo listo";
  if (s.ok === 0) return "No se ha hecho ningún cambio";
  return "Listo, con algún problema";
}

/** «5 hechos · 1 ha fallado · 2 saltados». */
export function resultLine(run: Run): string {
  const s = summarize(run);
  const parts = [plural(s.ok, "hecho")];
  if (s.failed) parts.push(s.failed === 1 ? "1 ha fallado" : `${s.failed} han fallado`);
  if (s.skipped) parts.push(plural(s.skipped, "saltado"));
  return parts.join(" · ");
}

/** Texto de estado de una fila, para la columna secundaria y VoiceOver. */
export function stateLabel(step: RunStep): string {
  switch (step.state) {
    case "pending":
      return "Pendiente";
    case "running":
      return "En curso";
    case "ok":
      return "Hecho";
    case "failed":
      return step.reason ? `Ha fallado: ${step.reason}` : "Ha fallado";
    case "skipped":
      return step.reason ? `Saltado: ${step.reason}` : "Saltado";
  }
}

/** Fracción descargada, si el paso informa de bytes. */
export function downloadFraction(step: RunStep): number | null {
  if (!step.bytes) return null;
  const [done, total] = step.bytes;
  if (total <= 0) return null;
  return Math.min(1, Math.max(0, done / total));
}
