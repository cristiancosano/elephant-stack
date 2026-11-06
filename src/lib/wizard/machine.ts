// Máquina de pasos del asistente (C9 y C10): Bienvenida → Escenario →
// Personalizar (solo si se pide) → Revisar → Aplicar y resultado.
//
// Es un reductor puro: `reduce(estado, evento)` devuelve un estado nuevo. Las
// llamadas a Rust las hace el store (stores/wizard.svelte.ts) y le pasa sus
// resultados como eventos. Así cada regla se prueba sin Tauri.

import type { ApplyReport } from "../bindings/ApplyReport";
import type { Database } from "../bindings/Database";
import type { Plan } from "../bindings/Plan";
import type { Profile } from "../bindings/Profile";
import type { ProfileInput } from "../bindings/ProfileInput";
import type { ScenarioCard } from "../bindings/ScenarioCard";
import type { ScenarioId } from "../bindings/ScenarioId";
import type { SetupProgress } from "../bindings/SetupProgress";
import { applyLabel, sanitizeAccepted } from "./review";
import { applyProgress, failRun, finishRun, startRun, type Run } from "./run";
import { customInput, presetInput, scenarioName } from "./scenario";
import "../i18n/locales/wizard";
import { t } from "../i18n/index.svelte";

export type Screen = "welcome" | "scenario" | "customize" | "review" | "apply";
export type Preset = Exclude<ScenarioId, "custom">;

/** Perfil en edición en Personalizar. */
export interface Draft {
  profile: Profile;
  /** Escenario del que parte («Ajustar»), o `null` si parte del sistema («Personalizado»). */
  basedOn: Preset | null;
  /** Base de datos de la tarjeta de origen, si era «Laravel con servicios». */
  database: Database | null;
  /** Se ha cambiado algo: desde ese momento es «Personalizado (basado en X)». */
  dirty: boolean;
}

export interface WizardState {
  screen: Screen;
  /** 1 al avanzar y -1 al retroceder: sentido de la transición. */
  direction: 1 | -1;
  /** Elección actual. `custom` solo con un borrador. */
  scenario: ScenarioId | null;
  /** La de «Laravel con servicios»; se recuerda aunque se elija otra tarjeta. */
  database: Database | null;
  draft: Draft | null;
  /** La revisión se abrió desde Personalizar: «Atrás» vuelve allí. */
  viaCustomize: boolean;
  /** `null` mientras se calcula. */
  plan: Plan | null;
  /** Huella de la entrada con la que se calculó `plan`. */
  planInput: string | null;
  /** Opcionales marcados (vienen desmarcados). */
  accepted: string[];
  /** El sistema cambió entre la revisión y la aplicación: hay que volver a revisar. */
  stale: boolean;
  run: Run | null;
}

export type WizardEvent =
  | { type: "start" }
  | { type: "select"; scenario: ScenarioId }
  | { type: "database"; database: Database }
  | { type: "customize"; profile: Profile; basedOn: Preset | null; database: Database | null }
  | { type: "reopen_customize" }
  | { type: "edit"; profile: Profile }
  | { type: "review" }
  | { type: "plan_loaded"; plan: Plan; input: ProfileInput }
  | { type: "toggle"; key: string; on: boolean }
  | { type: "back" }
  | { type: "apply_started" }
  | { type: "progress"; event: SetupProgress }
  | { type: "cancel_requested" }
  | { type: "apply_done"; report: ApplyReport }
  | { type: "apply_stale"; plan: Plan }
  | { type: "apply_failed"; error: string }
  | { type: "retry" }
  | { type: "enter"; screen: "scenario" | "review"; scenario: ScenarioId | null; database: Database | null; draft: Draft | null };

export function initialState(): WizardState {
  return {
    screen: "welcome",
    direction: 1,
    scenario: null,
    database: null,
    draft: null,
    viaCustomize: false,
    plan: null,
    planInput: null,
    accepted: [],
    stale: false,
    run: null,
  };
}

/** Huella estable de una entrada (el orden de las claves del perfil lo fija Rust). */
export function inputKey(input: ProfileInput): string {
  return JSON.stringify(input, (_k, v: unknown) =>
    v && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)))
      : v,
  );
}

/** Lo que se envía a `setup_preview` con la elección actual. */
export function currentInput(s: WizardState): ProfileInput | null {
  switch (s.scenario) {
    case null:
      return null;
    case "custom":
      return s.draft ? customInput(s.draft.profile) : null;
    default:
      return presetInput(s.scenario, s.database);
  }
}

/**
 * ¿Hay que pedir un perfil a Rust para abrir Personalizar? No, si ya hay un
 * borrador que sirve: así «Atrás» y volver a «Ajustar» conservan lo editado.
 */
export function needsProfile(s: WizardState, basedOn: Preset | null, database: Database | null): boolean {
  const d = s.draft;
  if (!d) return true;
  if (basedOn === null) return !(d.dirty || d.basedOn === null);
  if (d.basedOn !== basedOn) return true;
  return basedOn === "laravel_services" && d.database !== database;
}

/** «Laravel», «Personalizado» o «Personalizado (basado en Laravel)». */
export function choiceLabel(s: WizardState, cards: readonly ScenarioCard[] = []): string | null {
  if (!s.scenario) return null;
  if (s.scenario !== "custom") return scenarioName(s.scenario, cards);
  const base = s.draft?.basedOn;
  return base
    ? t("wizard.custom.basedOn", { name: scenarioName(base, cards) })
    : t("wizard.scenarios.custom");
}

/** Pantallas del indicador de puntos. Personalizar solo cuenta si forma parte del camino. */
export function screens(s: WizardState): Screen[] {
  const withCustomize = s.screen === "customize" || s.viaCustomize || s.scenario === "custom";
  return withCustomize
    ? ["welcome", "scenario", "customize", "review", "apply"]
    : ["welcome", "scenario", "review", "apply"];
}

export function canGoBack(s: WizardState): boolean {
  return s.screen === "scenario" || s.screen === "customize" || s.screen === "review";
}

export type PrimaryAction = "start" | "review" | "apply" | null;

/** Botón principal (↵) de cada pantalla. En «Aplicar y resultado» no hay. */
export function primary(s: WizardState): { action: PrimaryAction; label: string; enabled: boolean } {
  switch (s.screen) {
    case "welcome":
      return { action: "start", label: t("wizard.primary.start"), enabled: true };
    case "scenario":
      return { action: "review", label: t("wizard.primary.continue"), enabled: currentInput(s) !== null };
    case "customize":
      return { action: "review", label: t("wizard.primary.review"), enabled: s.draft !== null };
    case "review":
      return s.plan
        ? { action: "apply", label: applyLabel(s.plan, s.accepted), enabled: true }
        : { action: "apply", label: t("wizard.primary.apply"), enabled: false };
    case "apply":
      return { action: null, label: "", enabled: false };
  }
}

function forward(s: WizardState, screen: Screen): WizardState {
  return { ...s, screen, direction: 1 };
}

export function reduce(s: WizardState, e: WizardEvent): WizardState {
  switch (e.type) {
    case "start":
      return s.screen === "welcome" ? forward(s, "scenario") : s;

    case "select":
      if (s.screen !== "scenario") return s;
      if (e.scenario === "custom" && !s.draft) return s;
      return { ...s, scenario: e.scenario };

    case "database":
      if (s.screen !== "scenario") return s;
      return { ...s, scenario: "laravel_services", database: e.database };

    case "customize": {
      if (s.screen !== "scenario" && s.screen !== "customize") return s;
      const draft: Draft = { profile: e.profile, basedOn: e.basedOn, database: e.database, dirty: false };
      // «Ajustar» sin tocar nada sigue siendo el escenario de origen; «Personalizado» parte del sistema.
      const scenario: ScenarioId = e.basedOn ?? "custom";
      const database = e.basedOn === "laravel_services" ? (e.database ?? s.database) : s.database;
      return { ...forward(s, "customize"), draft, scenario, database };
    }

    case "reopen_customize":
      if (s.screen !== "scenario" || !s.draft) return s;
      return {
        ...forward(s, "customize"),
        scenario: s.draft.dirty || !s.draft.basedOn ? "custom" : s.draft.basedOn,
        database: s.draft.basedOn === "laravel_services" ? (s.draft.database ?? s.database) : s.database,
      };

    case "edit":
      if (s.screen !== "customize" || !s.draft) return s;
      return { ...s, scenario: "custom", draft: { ...s.draft, profile: e.profile, dirty: true } };

    case "review":
      if ((s.screen !== "scenario" && s.screen !== "customize") || currentInput(s) === null) return s;
      return { ...forward(s, "review"), viaCustomize: s.screen === "customize", plan: null, stale: false };

    case "plan_loaded": {
      if (s.screen !== "review") return s;
      const key = inputKey(e.input);
      const keep = key === s.planInput ? s.accepted : [];
      return { ...s, plan: e.plan, planInput: key, accepted: sanitizeAccepted(e.plan, keep) };
    }

    case "toggle": {
      if (s.screen !== "review" || !s.plan) return s;
      const rest = s.accepted.filter((k) => k !== e.key);
      return { ...s, accepted: sanitizeAccepted(s.plan, e.on ? [...rest, e.key] : rest) };
    }

    case "back":
      switch (s.screen) {
        case "scenario":
          return { ...s, screen: "welcome", direction: -1 };
        case "customize":
          return { ...s, screen: "scenario", direction: -1 };
        case "review":
          return {
            ...s,
            screen: s.viaCustomize && s.draft ? "customize" : "scenario",
            direction: -1,
            stale: false,
          };
        default:
          return s;
      }

    case "apply_started":
      if (s.screen !== "review" || !s.plan) return s;
      return { ...forward(s, "apply"), run: startRun(s.plan, s.accepted) };

    case "progress":
      if (!s.run || s.run.finished) return s;
      return { ...s, run: applyProgress(s.run, e.event) };

    case "cancel_requested":
      if (!s.run || s.run.finished) return s;
      return { ...s, run: { ...s.run, cancelling: true } };

    case "apply_done":
      if (!s.run) return s;
      return { ...s, run: finishRun(s.run, e.report) };

    case "apply_failed":
      if (!s.run) return s;
      return { ...s, run: failRun(s.run, e.error) };

    case "apply_stale":
      // No se ha ejecutado nada: se vuelve a revisar el plan nuevo, conservando lo marcado que siga existiendo.
      return {
        ...s,
        screen: "review",
        direction: -1,
        run: null,
        plan: e.plan,
        stale: true,
        accepted: sanitizeAccepted(e.plan, s.accepted),
      };

    case "retry":
      if (s.screen !== "apply" || !s.run?.finished || currentInput(s) === null) return s;
      return { ...s, screen: "review", direction: 1, run: null, plan: null, stale: false };

    case "enter":
      return {
        ...initialState(),
        screen: e.screen,
        scenario: e.scenario === "custom" && !e.draft ? null : e.scenario,
        database: e.database,
        draft: e.draft,
      };
  }
}
