// Escenarios del asistente: nombres, entradas para `setup_preview` y el estado
// de Ajustes ▸ Escenario. Puro: sin Svelte ni IPC.

import type { Database } from "../bindings/Database";
import type { Plan } from "../bindings/Plan";
import type { Profile } from "../bindings/Profile";
import type { ProfileInput } from "../bindings/ProfileInput";
import type { ScenarioCard } from "../bindings/ScenarioCard";
import type { ScenarioId } from "../bindings/ScenarioId";
import type { SetupFile } from "../bindings/SetupFile";
import { formatNumber, plural } from "../format";

/** Nombres de respaldo, por si las tarjetas todavía no han llegado. */
export const SCENARIO_NAMES: Record<ScenarioId, string> = {
  laravel: "Laravel",
  laravel_services: "Laravel con servicios",
  terminal: "Solo terminal",
  custom: "Personalizado",
};

export const DATABASE_NAMES: Record<Database, string> = {
  mysql: "MySQL",
  postgres: "PostgreSQL",
};

/** Base de datos por defecto si nada la preselecciona: la de Sail y Forge (04 §4.1). */
export const DEFAULT_DATABASE: Database = "mysql";

export function scenarioName(id: ScenarioId, cards: readonly ScenarioCard[] = []): string {
  return cards.find((c) => c.id === id)?.name ?? SCENARIO_NAMES[id];
}

/** La base de datos que la tarjeta de «Laravel con servicios» marca de entrada. */
export function preselectedDatabase(cards: readonly ScenarioCard[]): Database {
  const card = cards.find((c) => c.id === "laravel_services");
  return card?.databases.find((d) => d.preselected)?.database ?? card?.databases[0]?.database ?? DEFAULT_DATABASE;
}

/**
 * Entrada de un escenario fijo. `custom` no tiene entrada sin un perfil: para eso
 * está `customInput`.
 */
export function presetInput(id: Exclude<ScenarioId, "custom">, database: Database | null): ProfileInput {
  switch (id) {
    case "laravel":
      return { scenario: "laravel" };
    case "laravel_services":
      return { scenario: "laravel_services", database: database ?? DEFAULT_DATABASE };
    case "terminal":
      return { scenario: "terminal" };
  }
}

/** Un perfil editado se envía siempre como Personalizado, aunque partiera de otro escenario. */
export function customInput(profile: Profile): ProfileInput {
  return { scenario: "custom", profile: { ...profile, scenario: "custom" } };
}

/** Lo guardado en `setup.json` como entrada de `setup_preview`. `null` = «No cambiar nada». */
export function inputFromSetupFile(file: SetupFile | null): ProfileInput | null {
  if (!file?.scenario) return null;
  if (file.scenario === "custom") return file.custom ? customInput(file.custom) : null;
  return presetInput(file.scenario, file.database);
}

export type Drift =
  | { kind: "up_to_date"; label: string }
  | { kind: "differences"; label: string; count: number };

/** «Al día» si el plan no tiene pasos; si no, «N diferencias» (04 §4.8). */
export function driftOf(plan: Plan): Drift {
  const count = plan.steps.length;
  if (count === 0) return { kind: "up_to_date", label: "Al día" };
  return { kind: "differences", label: plural(count, "diferencia"), count };
}

/** RAM estimada en reposo: siempre con «≈», porque es una estimación (04 §4.4). */
export function idleRam(mb: number): string {
  return `≈\u00a0${formatNumber(mb)}\u00a0MB en reposo`;
}
