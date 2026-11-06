// Comprobaciones en inglés de los textos que se construyen fuera de las vistas
// (stores y módulos de lib). Los tests de cada módulo siguen en español.
import { describe, expect, it } from "vitest";
import { withLocale } from "./testing";
import { levelLabel, originLabel } from "../logs/model";
import { modeOptions, rangeOptions, sourceOptions } from "../logs/filters";
import { unseenLabel } from "../logs/follow";
import { parseGlobs, validateTimezone } from "../logs/settings";
import { applyLabel, totalsLine } from "../wizard/review";
import { finishRun, headline, resultLine, startRun } from "../wizard/run";
import { driftOf, fallbackScenarioName, idleRam } from "../wizard/scenario";
import { serviceLabel, serviceWantOptions } from "../wizard/customize";
import { plan, step } from "../wizard/fixtures.test-helpers";
import { clientName, serviceStatusText } from "../stores/app.svelte";
import type { Service } from "../bindings/Service";

const en = <T>(fn: () => T) => withLocale("en", fn);
const plain = (text: string) => text.replaceAll(" ", " ");

describe("inglés", () => {
  it("logs: niveles, orígenes y filtros", () => {
    expect(en(() => levelLabel("warning"))).toBe("Warning");
    expect(en(() => originLabel("herd_linked"))).toBe("Herd (linked)");
    expect(en(() => sourceOptions()[0].label)).toBe("All");
    expect(en(() => rangeOptions().map((o) => plain(o.label)))).toEqual(["15 min", "1 hr", "Today", "7 days", "All"]);
    expect(en(() => modeOptions().map((o) => o.label))).toEqual(["Chronological", "Grouped"]);
    expect(en(() => unseenLabel({ following: false, unseen: 3 }))).toBe("3 new");
    expect(en(() => parseGlobs("relativa/*.log").errors[0])).toBe("“relativa/*.log” isn’t an absolute path (it must start with /).");
    expect(en(() => validateTimezone("Mars/Base", ["UTC"]))).toContain("isn’t a known time zone");
    // Y se repintan al volver al español: no se quedan con el idioma de la primera llamada.
    expect(sourceOptions()[0].label).toBe("Todas");
  });

  it("asistente: plan, ejecución y escenarios", () => {
    const p = plan([step("a", { group: "download" }), step("b", { group: "configure" })], {
      totals: { download_bytes: 0, brew_formulas: 2, idle_ram_mb: 25 },
    });
    expect(en(() => applyLabel(p, []))).toBe("Apply (2 steps)");
    expect(en(() => applyLabel(plan(), []))).toBe("Done");
    expect(en(() => plain(totalsLine(p)))).toBe("≈ 25 MB idle · 2 formulas");
    const run = finishRun(startRun(p, []), { done: ["a"], failed: [["b", "boom"]], skipped: [], log_path: "" });
    expect(en(() => headline(run))).toBe("Done, with some problems");
    expect(en(() => resultLine(run))).toBe("1 done · 1 failed");
    expect(en(() => fallbackScenarioName("laravel_services"))).toBe("Laravel with services");
    expect(en(() => driftOf(plan()).label)).toBe("Up to date");
    expect(en(() => plain(idleRam(1200)))).toBe("≈ 1,200 MB idle");
    expect(en(() => serviceLabel("redis").purpose)).toBe("Cache and queues");
    expect(en(() => serviceWantOptions().map((o) => o.label))).toEqual(["Don’t touch", "Installed, off", "At login"]);
  });

  it("servicios: estado y tareas", () => {
    const service = { running: false, status: "error", exit_code: 1, registered: true } as Service;
    expect(en(() => serviceStatusText(service))).toBe("Error · exit code 1 · At login");
    expect(serviceStatusText(service)).toBe("Error · código 1 · Al iniciar sesión");
    expect(en(() => clientName("tty"))).toBe("the terminal");
  });
});
