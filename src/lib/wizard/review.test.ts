import { describe, expect, it } from "vitest";

import { catalog, plan, step } from "./fixtures.test-helpers";
import {
  applyCount,
  applyLabel,
  isEmptyPlan,
  optionalNote,
  optionalSteps,
  reviewSections,
  sanitizeAccepted,
  totalsLine,
  visibleNotices,
} from "./review";

/** Las cifras llevan espacio duro antes de la unidad. */
const plain = (s: string | null) => (s ?? "").replace(/ /g, " ");

const release = catalog(["8.5.12"]).php[0];

const p = plan(
  [
    step("brew-update", { group: "brew_install", action: { kind: "brew_update" } }),
    step("download-php:8.5.12", { group: "download", action: { kind: "download_php", release } }),
    step("brew-install:redis", { group: "brew_install", action: { kind: "brew_install", formula: "redis" } }),
    step("install-shim", { group: "configure" }),
    step("autostart:mailpit", { group: "start" }),
    step("autostart-off:postgresql@18", { group: "optional", optional: true, restarts_service: true }),
  ],
  { totals: { download_bytes: 65_000_000, brew_formulas: 1, idle_ram_mb: 25 } },
);

describe("reviewSections", () => {
  it("agrupa los obligatorios en el orden de la revisión y deja fuera los opcionales", () => {
    const sections = reviewSections(p);
    expect(sections.map((s) => s.title)).toEqual([
      "Se descargará",
      "Se instalará con Homebrew",
      "Se configurará",
      "Arrancará al iniciar sesión",
    ]);
    expect(sections[1].steps.map((s) => s.key)).toEqual(["brew-update", "brew-install:redis"]);
    expect(sections.flatMap((s) => s.steps).some((s) => s.optional)).toBe(false);
  });

  it("explica los MB de descarga y el aviso de Homebrew", () => {
    const [download, brew] = reviewSections(p);
    expect(plain(download.footer)).toBe("65 MB en total.");
    expect(brew.footer).toMatch(/Homebrew puede actualizar/);
  });

  it("sin pasos no hay grupos", () => {
    expect(reviewSections(plan())).toEqual([]);
    expect(isEmptyPlan(plan())).toBe(true);
  });
});

describe("opcionales", () => {
  it("avisan si el servicio se reinicia", () => {
    const [opt] = optionalSteps(p);
    expect(opt.key).toBe("autostart-off:postgresql@18");
    expect(optionalNote(opt)).toBe("El servicio se reiniciará un momento.");
    expect(optionalNote(step("x", { optional: true }))).toBeNull();
  });

  it("sanitizeAccepted quita duplicados y claves que no son opcionales", () => {
    expect(sanitizeAccepted(p, ["autostart-off:postgresql@18", "autostart-off:postgresql@18", "install-shim", "x"])).toEqual([
      "autostart-off:postgresql@18",
    ]);
  });
});

describe("botón principal", () => {
  it("«Aplicar (N pasos)» o «Hecho»", () => {
    expect(applyCount(p, [])).toBe(5);
    expect(applyLabel(p, ["autostart-off:postgresql@18"])).toBe("Aplicar (6 pasos)");
    expect(applyLabel(plan([step("a")]), [])).toBe("Aplicar (1 paso)");
    expect(applyLabel(plan(), [])).toBe("Hecho");
    // Solo opcionales sin marcar: no hay nada que ejecutar.
    expect(applyLabel(plan([step("o", { optional: true })]), [])).toBe("Hecho");
  });
});

describe("totalsLine", () => {
  it("resume RAM, descarga y fórmulas", () => {
    expect(plain(totalsLine(p))).toBe("≈ 25 MB en reposo · 65 MB de descarga · 1 fórmula");
    expect(plain(totalsLine(plan()))).toBe("≈ 0 MB en reposo");
  });
});

describe("avisos", () => {
  it("no repite el aviso del PATH cuando se enseña el de «Añadirlo por mí»", () => {
    const pathNotice = "El selector php no está en el PATH de tu terminal. Puedes añadirlo a ~/.zshrc.";
    const withPath = plan([], {
      notices: [pathNotice, "Herd sigue sirviendo tus sitios .test."],
      path_notice: pathNotice,
    });
    expect(visibleNotices(withPath, true)).toEqual(["Herd sigue sirviendo tus sitios .test."]);
    expect(visibleNotices(withPath, false)).toHaveLength(2);
  });
});
