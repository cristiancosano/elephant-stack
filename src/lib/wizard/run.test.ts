import { describe, expect, it } from "vitest";

import type { SetupProgress } from "../bindings/SetupProgress";
import { plan, step } from "./fixtures.test-helpers";
import {
  MAX_LOG_LINES,
  applyProgress,
  canRetry,
  downloadFraction,
  failRun,
  finishRun,
  headline,
  resultLine,
  startRun,
  stateLabel,
  summarize,
} from "./run";

const ev = (key: string, over: Partial<SetupProgress> = {}): SetupProgress => ({
  key,
  state: "running",
  line: null,
  bytes: null,
  reason: null,
  ...over,
});

const p = plan([
  step("download-php:8.5.12", { group: "download" }),
  step("brew-install:redis", { group: "brew_install" }),
  step("autostart-off:postgresql@18", { optional: true }),
]);

describe("startRun", () => {
  it("solo incluye los obligatorios y los opcionales aceptados, todos pendientes", () => {
    expect(startRun(p, []).steps.map((s) => s.key)).toEqual(["download-php:8.5.12", "brew-install:redis"]);
    const run = startRun(p, ["autostart-off:postgresql@18"]);
    expect(run.steps).toHaveLength(3);
    expect(run.steps.every((s) => s.state === "pending")).toBe(true);
    expect(headline(run)).toBe("Aplicando los cambios…");
  });
});

describe("applyProgress", () => {
  it("guarda estado, última línea, bytes y el log del paso", () => {
    let run = startRun(p, []);
    run = applyProgress(run, ev("brew-install:redis", { line: "==> Downloading redis" }));
    run = applyProgress(run, ev("brew-install:redis", { line: "==> Pouring redis" }));
    run = applyProgress(run, ev("download-php:8.5.12", { bytes: [10, 40] }));
    const redis = run.steps[1];
    expect(redis.state).toBe("running");
    expect(redis.line).toBe("==> Pouring redis");
    expect(redis.log).toEqual(["==> Downloading redis", "==> Pouring redis"]);
    expect(downloadFraction(run.steps[0])).toBe(0.25);
    expect(downloadFraction(redis)).toBeNull();
  });

  it("no pierde la última línea cuando un evento llega sin línea", () => {
    let run = startRun(p, []);
    run = applyProgress(run, ev("brew-install:redis", { line: "==> Pouring redis" }));
    run = applyProgress(run, ev("brew-install:redis", { state: "ok" }));
    expect(run.steps[1]).toMatchObject({ state: "ok", line: "==> Pouring redis" });
  });

  it("limita el log de cada paso", () => {
    let run = startRun(p, []);
    for (let i = 0; i < MAX_LOG_LINES + 10; i += 1) run = applyProgress(run, ev("brew-install:redis", { line: `l${i}` }));
    const log = run.steps[1].log;
    expect(log).toHaveLength(MAX_LOG_LINES);
    expect(log.at(-1)).toBe(`l${MAX_LOG_LINES + 9}`);
  });

  it("un paso desconocido se añade al final sin romper nada", () => {
    const run = applyProgress(startRun(p, []), ev("brew-update", { state: "ok" }));
    expect(run.steps.at(-1)).toMatchObject({ key: "brew-update", state: "ok" });
  });

  it("no muta la ejecución anterior", () => {
    const before = startRun(p, []);
    applyProgress(before, ev("brew-install:redis", { state: "ok", line: "x" }));
    expect(before.steps[1].state).toBe("pending");
    expect(before.steps[1].log).toEqual([]);
  });
});

describe("finishRun", () => {
  it("el informe manda y lo que quedó pendiente pasa a saltado", () => {
    let run = startRun(p, ["autostart-off:postgresql@18"]);
    run = applyProgress(run, ev("download-php:8.5.12", { state: "running" }));
    run = finishRun(run, {
      done: ["download-php:8.5.12"],
      failed: [["brew-install:redis", "Sin conexión a Internet"]],
      skipped: [],
      log_path: "/tmp/setup.log",
    });
    expect(run.finished).toBe(true);
    expect(run.steps.map((s) => s.state)).toEqual(["ok", "failed", "skipped"]);
    expect(run.steps[1].reason).toBe("Sin conexión a Internet");
    expect(run.steps[2].reason).toBe("No se ejecutó");
    expect(summarize(run)).toMatchObject({ ok: 1, failed: 1, skipped: 1, fraction: 1 });
    expect(canRetry(run)).toBe(true);
    expect(headline(run)).toBe("Listo, con algún problema");
    expect(resultLine(run)).toBe("1 hecho · 1 ha fallado · 1 saltado");
    expect(stateLabel(run.steps[1])).toBe("Ha fallado: Sin conexión a Internet");
  });

  it("todo bien: «Todo listo» y nada que reintentar", () => {
    const run = finishRun(startRun(p, []), {
      done: ["download-php:8.5.12", "brew-install:redis"],
      failed: [],
      skipped: [],
      log_path: "/tmp/setup.log",
    });
    expect(headline(run)).toBe("Todo listo");
    expect(canRetry(run)).toBe(false);
    expect(resultLine(run)).toBe("2 hechos");
  });

  it("un plan vacío aplicado dice que el Mac ya estaba listo", () => {
    const run = finishRun(startRun(plan(), []), { done: [], failed: [], skipped: [], log_path: "" });
    expect(headline(run)).toBe("Tu Mac ya estaba listo");
    expect(summarize(run).fraction).toBe(1);
  });

  it("cancelado: los saltados se pueden reintentar", () => {
    const run = finishRun(
      { ...startRun(p, []), cancelling: true },
      { done: ["download-php:8.5.12"], failed: [], skipped: [["brew-install:redis", "Cancelado"]], log_path: "" },
    );
    expect(run.cancelling).toBe(false);
    expect(run.steps[1]).toMatchObject({ state: "skipped", reason: "Cancelado" });
    expect(canRetry(run)).toBe(true);
  });
});

describe("failRun", () => {
  it("un error del comando entero se puede reintentar", () => {
    const run = failRun(startRun(p, []), "Homebrew está ocupado con otra operación");
    expect(run.finished).toBe(true);
    expect(headline(run)).toBe("No se ha podido aplicar");
    expect(canRetry(run)).toBe(true);
    expect(canRetry(startRun(p, []))).toBe(false);
  });
});
