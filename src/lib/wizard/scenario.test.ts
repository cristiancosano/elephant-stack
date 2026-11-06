import { describe, expect, it } from "vitest";

import { CARDS, plan, profile, step } from "./fixtures.test-helpers";
import {
  customInput,
  driftOf,
  idleRam,
  inputFromSetupFile,
  preselectedDatabase,
  presetInput,
  scenarioName,
} from "./scenario";
import { addedBlock, diffLines } from "./shell";

const file = (over = {}) => ({ version: 1, scenario: null, database: null, custom: null, completed_at: null, ...over });

describe("escenarios", () => {
  it("nombres: los de las tarjetas o los de respaldo", () => {
    expect(scenarioName("laravel", CARDS)).toBe("Laravel");
    expect(scenarioName("terminal", CARDS)).toBe("Solo terminal");
  });

  it("base de datos preseleccionada", () => {
    expect(preselectedDatabase(CARDS)).toBe("postgres");
    expect(preselectedDatabase([])).toBe("mysql");
  });

  it("entradas de los escenarios fijos", () => {
    expect(presetInput("laravel_services", null)).toEqual({ scenario: "laravel_services", database: "mysql" });
    expect(presetInput("terminal", "postgres")).toEqual({ scenario: "terminal" });
  });

  it("un perfil personalizado se envía siempre como custom", () => {
    expect(customInput(profile({ scenario: "laravel" }))).toEqual({
      scenario: "custom",
      profile: profile({ scenario: "custom" }),
    });
  });

  it("idleRam lleva siempre «≈»", () => {
    expect(idleRam(25).replace(/ /g, " ")).toBe("≈ 25 MB en reposo");
  });
});

describe("inputFromSetupFile", () => {
  it("traduce setup.json a la entrada de setup_preview", () => {
    expect(inputFromSetupFile(null)).toBeNull();
    expect(inputFromSetupFile(file())).toBeNull();
    expect(inputFromSetupFile(file({ scenario: "laravel" }))).toEqual({ scenario: "laravel" });
    expect(inputFromSetupFile(file({ scenario: "laravel_services", database: "postgres" }))).toEqual({
      scenario: "laravel_services",
      database: "postgres",
    });
    expect(inputFromSetupFile(file({ scenario: "custom" }))).toBeNull();
    expect(inputFromSetupFile(file({ scenario: "custom", custom: profile() }))).toEqual(customInput(profile()));
  });
});

describe("driftOf", () => {
  it("«Al día» o «N diferencias»", () => {
    expect(driftOf(plan())).toEqual({ kind: "up_to_date", label: "Al día" });
    expect(driftOf(plan([step("a")]))).toEqual({ kind: "differences", label: "1 diferencia", count: 1 });
    expect(driftOf(plan([step("a"), step("b", { optional: true })])).label).toBe("2 diferencias");
  });
});

describe("diff de ~/.zshrc", () => {
  const diff = [
    "--- /Users/c/.zshrc",
    "+++ /Users/c/.zshrc",
    "@@ -27,1 +27,4 @@",
    ' export PATH="$HOME/Library/Application Support/Herd/bin/:$PATH"',
    "+# >>> elephant-stack >>>",
    '+export PATH="$HOME/.elephant-stack/bin:$PATH"',
    "+# <<< elephant-stack <<<",
    "",
  ].join("\n");

  it("clasifica cada línea", () => {
    expect(diffLines(diff).map((l) => l.kind)).toEqual(["header", "header", "hunk", "context", "add", "add", "add"]);
    expect(diffLines("")).toEqual([]);
    expect(diffLines("-viejo\r\n+nuevo")).toEqual([
      { kind: "remove", text: "-viejo" },
      { kind: "add", text: "+nuevo" },
    ]);
  });

  it("«Copiar» da solo el bloque añadido, sin el «+»", () => {
    expect(addedBlock(diff)).toBe(
      '# >>> elephant-stack >>>\nexport PATH="$HOME/.elephant-stack/bin:$PATH"\n# <<< elephant-stack <<<',
    );
  });
});
