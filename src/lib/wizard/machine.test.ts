import { describe, expect, it } from "vitest";

import { CARDS, plan, profile, step } from "./fixtures.test-helpers";
import {
  canGoBack,
  choiceLabel,
  currentInput,
  initialState,
  inputKey,
  needsProfile,
  primary,
  reduce,
  screens,
  type WizardEvent,
  type WizardState,
} from "./machine";

function run(events: WizardEvent[], from: WizardState = initialState()): WizardState {
  return events.reduce(reduce, from);
}

const atScenario = () => run([{ type: "start" }]);

describe("navegación", () => {
  it("Empezar lleva de la bienvenida al escenario", () => {
    const s = atScenario();
    expect(s.screen).toBe("scenario");
    expect(s.direction).toBe(1);
    expect(primary(s)).toMatchObject({ label: "Continuar", enabled: false });
  });

  it("no se puede revisar sin elegir escenario", () => {
    const s = run([{ type: "review" }], atScenario());
    expect(s.screen).toBe("scenario");
  });

  it("elegir una tarjeta y continuar abre la revisión cargando", () => {
    const s = run([{ type: "select", scenario: "laravel" }, { type: "review" }], atScenario());
    expect(s.screen).toBe("review");
    expect(s.plan).toBeNull();
    expect(primary(s).enabled).toBe(false);
    expect(currentInput(s)).toEqual({ scenario: "laravel" });
  });

  it("ir atrás conserva la elección, la base de datos y lo marcado", () => {
    const p = plan([step("a"), step("opt", { optional: true })]);
    const input = { scenario: "laravel_services", database: "postgres" } as const;
    let s = run(
      [
        { type: "database", database: "postgres" },
        { type: "review" },
        { type: "plan_loaded", plan: p, input },
        { type: "toggle", key: "opt", on: true },
        { type: "back" },
      ],
      atScenario(),
    );
    expect(s.screen).toBe("scenario");
    expect(s.direction).toBe(-1);
    expect(s.scenario).toBe("laravel_services");
    expect(s.database).toBe("postgres");

    // Elegir otra tarjeta y volver no pierde la base de datos.
    s = run([{ type: "select", scenario: "laravel" }, { type: "select", scenario: "laravel_services" }], s);
    expect(s.database).toBe("postgres");

    // El mismo plan otra vez: lo marcado sigue marcado.
    s = run([{ type: "review" }, { type: "plan_loaded", plan: p, input }], s);
    expect(s.accepted).toEqual(["opt"]);
  });

  it("con otra entrada, los opcionales vuelven a venir desmarcados", () => {
    const p = plan([step("opt", { optional: true })]);
    let s = run(
      [
        { type: "select", scenario: "laravel" },
        { type: "review" },
        { type: "plan_loaded", plan: p, input: { scenario: "laravel" } },
        { type: "toggle", key: "opt", on: true },
        { type: "back" },
        { type: "select", scenario: "terminal" },
        { type: "review" },
      ],
      atScenario(),
    );
    s = reduce(s, { type: "plan_loaded", plan: p, input: { scenario: "terminal" } });
    expect(s.accepted).toEqual([]);
  });

  it("no se puede volver atrás desde la bienvenida ni durante la aplicación", () => {
    expect(canGoBack(initialState())).toBe(false);
    const s = run(
      [
        { type: "select", scenario: "laravel" },
        { type: "review" },
        { type: "plan_loaded", plan: plan([step("a")]), input: { scenario: "laravel" } },
        { type: "apply_started" },
      ],
      atScenario(),
    );
    expect(s.screen).toBe("apply");
    expect(canGoBack(s)).toBe(false);
    expect(reduce(s, { type: "back" })).toBe(s);
  });
});

describe("Personalizar", () => {
  it("«Ajustar» sin tocar nada sigue siendo el escenario de origen", () => {
    const s = run(
      [{ type: "customize", profile: profile(), basedOn: "laravel", database: null }],
      atScenario(),
    );
    expect(s.screen).toBe("customize");
    expect(s.scenario).toBe("laravel");
    expect(choiceLabel(s, CARDS)).toBe("Laravel");
    expect(currentInput(s)).toEqual({ scenario: "laravel" });
  });

  it("al tocar algo pasa a «Personalizado (basado en X)» y envía el perfil como custom", () => {
    const edited = profile({ composer: false });
    const s = run(
      [
        { type: "customize", profile: profile(), basedOn: "laravel_services", database: "postgres" },
        { type: "edit", profile: edited },
      ],
      atScenario(),
    );
    expect(s.scenario).toBe("custom");
    expect(choiceLabel(s, CARDS)).toBe("Personalizado (basado en Laravel con servicios)");
    expect(currentInput(s)).toEqual({ scenario: "custom", profile: { ...edited, scenario: "custom" } });
  });

  it("«Personalizado» parte del sistema y se llama solo «Personalizado»", () => {
    const s = run(
      [{ type: "customize", profile: profile({ scenario: "custom" }), basedOn: null, database: null }],
      atScenario(),
    );
    expect(s.scenario).toBe("custom");
    expect(choiceLabel(s)).toBe("Personalizado");
    expect(screens(s)).toContain("customize");
  });

  it("volver de la revisión regresa a Personalizar con lo editado", () => {
    const edited = profile({ shim: false });
    let s = run(
      [
        { type: "customize", profile: profile(), basedOn: "laravel", database: null },
        { type: "edit", profile: edited },
        { type: "review" },
      ],
      atScenario(),
    );
    expect(s.viaCustomize).toBe(true);
    s = reduce(s, { type: "back" });
    expect(s.screen).toBe("customize");
    expect(s.draft?.profile).toEqual(edited);

    // Y de ahí al escenario, y otra vez «Ajustar» en la misma tarjeta: no hace falta pedir el perfil.
    s = reduce(s, { type: "back" });
    expect(needsProfile(s, "laravel", null)).toBe(false);
    s = reduce(s, { type: "reopen_customize" });
    expect(s.screen).toBe("customize");
    expect(s.scenario).toBe("custom");
    expect(s.draft?.profile).toEqual(edited);
  });

  it("«Ajustar» en otra tarjeta pide un perfil nuevo", () => {
    const s = run(
      [{ type: "customize", profile: profile(), basedOn: "laravel", database: null }, { type: "back" }],
      atScenario(),
    );
    expect(needsProfile(s, "terminal", null)).toBe(true);
    expect(needsProfile(s, "laravel", null)).toBe(false);
    // Desde el sistema: el borrador de «Ajustar» sin tocar no sirve.
    expect(needsProfile(s, null, null)).toBe(true);
    expect(needsProfile(initialState(), "laravel", null)).toBe(true);
  });

  it("cambiar la base de datos de la tarjeta invalida su borrador", () => {
    const s = run(
      [
        { type: "customize", profile: profile(), basedOn: "laravel_services", database: "mysql" },
        { type: "back" },
      ],
      atScenario(),
    );
    expect(needsProfile(s, "laravel_services", "mysql")).toBe(false);
    expect(needsProfile(s, "laravel_services", "postgres")).toBe(true);
  });

  it("un borrador editado se reutiliza desde «Personalizado»", () => {
    const s = run(
      [
        { type: "customize", profile: profile(), basedOn: "laravel", database: null },
        { type: "edit", profile: profile({ composer: false }) },
        { type: "back" },
      ],
      atScenario(),
    );
    expect(needsProfile(s, null, null)).toBe(false);
  });

  it("«custom» no se puede elegir sin borrador", () => {
    const s = reduce(atScenario(), { type: "select", scenario: "custom" });
    expect(s.scenario).toBeNull();
  });
});

describe("revisión", () => {
  const input = { scenario: "laravel" } as const;
  const reviewing = (p = plan()) =>
    run([{ type: "select", scenario: "laravel" }, { type: "review" }, { type: "plan_loaded", plan: p, input }], atScenario());

  it("plan vacío → «Hecho»", () => {
    expect(primary(reviewing())).toEqual({ action: "apply", label: "Hecho", enabled: true });
  });

  it("cuenta los obligatorios y los opcionales marcados", () => {
    let s = reviewing(plan([step("a"), step("b"), step("opt", { optional: true })]));
    expect(primary(s).label).toBe("Aplicar (2 pasos)");
    s = reduce(s, { type: "toggle", key: "opt", on: true });
    expect(primary(s).label).toBe("Aplicar (3 pasos)");
    s = reduce(s, { type: "toggle", key: "opt", on: false });
    expect(s.accepted).toEqual([]);
  });

  it("solo se pueden marcar claves opcionales del plan", () => {
    let s = reviewing(plan([step("a"), step("opt", { optional: true })]));
    s = reduce(s, { type: "toggle", key: "a", on: true });
    s = reduce(s, { type: "toggle", key: "inventada", on: true });
    expect(s.accepted).toEqual([]);
  });

  it("un plan que llega tarde a otra pantalla se ignora", () => {
    const s = reduce(atScenario(), { type: "plan_loaded", plan: plan(), input });
    expect(s.plan).toBeNull();
  });
});

describe("aplicar", () => {
  const input = { scenario: "laravel" } as const;
  const p = plan([step("a"), step("b"), step("opt", { optional: true }), step("opt2", { optional: true })]);
  const applying = () =>
    run(
      [
        { type: "select", scenario: "laravel" },
        { type: "review" },
        { type: "plan_loaded", plan: p, input },
        { type: "toggle", key: "opt", on: true },
        { type: "apply_started" },
      ],
      atScenario(),
    );

  it("ejecuta los obligatorios y los opcionales marcados", () => {
    expect(applying().run?.steps.map((x) => x.key)).toEqual(["a", "b", "opt"]);
  });

  it("Stale → vuelve a revisar el plan nuevo conservando lo marcado", () => {
    const fresh = plan([step("a"), step("opt", { optional: true }), step("c")], { id: "plan-2" });
    const s = reduce(applying(), { type: "apply_stale", plan: fresh });
    expect(s.screen).toBe("review");
    expect(s.stale).toBe(true);
    expect(s.run).toBeNull();
    expect(s.plan?.id).toBe("plan-2");
    expect(s.accepted).toEqual(["opt"]);
    expect(primary(s).label).toBe("Aplicar (3 pasos)");
    // Volver a aplicar limpia el aviso al cambiar de pantalla.
    expect(reduce(s, { type: "back" }).stale).toBe(false);
  });

  it("reintentar lo que falta pide un plan nuevo con la misma elección", () => {
    let s = applying();
    s = reduce(s, { type: "progress", event: { key: "a", state: "failed", line: null, bytes: null, reason: "Sin conexión a Internet" } });
    s = reduce(s, {
      type: "apply_done",
      report: { done: ["b", "opt"], failed: [["a", "Sin conexión a Internet"]], skipped: [], log_path: "/tmp/x.log" },
    });
    expect(s.run?.finished).toBe(true);
    s = reduce(s, { type: "retry" });
    expect(s.screen).toBe("review");
    expect(s.plan).toBeNull();
    expect(s.run).toBeNull();
    expect(currentInput(s)).toEqual(input);

    // El plan nuevo solo trae lo que falta; lo marcado que ya no existe se descarta.
    s = reduce(s, { type: "plan_loaded", plan: plan([step("a")], { id: "plan-3" }), input });
    expect(s.accepted).toEqual([]);
    expect(primary(s).label).toBe("Aplicar (1 paso)");
  });

  it("no se reintenta mientras sigue en marcha", () => {
    const s = applying();
    expect(reduce(s, { type: "retry" })).toBe(s);
  });

  it("los eventos después del final no cambian nada", () => {
    let s = reduce(applying(), { type: "apply_failed", error: "Homebrew está ocupado" });
    expect(s.run?.error).toBe("Homebrew está ocupado");
    const before = s;
    s = reduce(s, { type: "progress", event: { key: "a", state: "ok", line: null, bytes: null, reason: null } });
    expect(s).toBe(before);
  });
});

describe("entrada desde Ajustes ▸ Escenario", () => {
  it("«Cambiar de escenario» abre la elección con el actual marcado", () => {
    const s = reduce(initialState(), {
      type: "enter",
      screen: "scenario",
      scenario: "laravel_services",
      database: "postgres",
      draft: null,
    });
    expect(s.screen).toBe("scenario");
    expect(currentInput(s)).toEqual({ scenario: "laravel_services", database: "postgres" });
  });

  it("un Personalizado guardado entra con su perfil como borrador editado", () => {
    const p = profile({ scenario: "custom" });
    const s = reduce(initialState(), {
      type: "enter",
      screen: "review",
      scenario: "custom",
      database: null,
      draft: { profile: p, basedOn: null, database: null, dirty: true },
    });
    expect(s.screen).toBe("review");
    expect(currentInput(s)).toEqual({ scenario: "custom", profile: p });
    expect(reduce(s, { type: "back" }).screen).toBe("scenario");
  });

  it("custom sin perfil no se puede elegir", () => {
    const s = reduce(initialState(), { type: "enter", screen: "scenario", scenario: "custom", database: null, draft: null });
    expect(s.scenario).toBeNull();
  });
});

describe("inputKey", () => {
  it("no depende del orden de las claves", () => {
    const a = { scenario: "custom", profile: profile({ services: { redis: "keep", mailpit: "installed" } }) } as const;
    const b = { scenario: "custom", profile: { ...profile(), services: { mailpit: "installed", redis: "keep" } } } as const;
    expect(inputKey(a)).toBe(inputKey(b));
    expect(inputKey({ scenario: "laravel" })).not.toBe(inputKey({ scenario: "terminal" }));
  });
});

describe("indicador de puntos", () => {
  it("Personalizar solo aparece si forma parte del camino", () => {
    expect(screens(atScenario())).toEqual(["welcome", "scenario", "review", "apply"]);
    const s = run([{ type: "customize", profile: profile(), basedOn: "laravel", database: null }], atScenario());
    expect(screens(s)).toEqual(["welcome", "scenario", "customize", "review", "apply"]);
  });
});
