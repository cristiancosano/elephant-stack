import { describe, expect, it } from "vitest";

import { project } from "./fixtures";
import {
  buildOverride,
  groupRoots,
  guessHome,
  overrideFields,
  parseGlobs,
  tildePath,
  validateTimezone,
} from "./settings";

describe("groupRoots", () => {
  it("reparte los proyectos del usuario en la carpeta más concreta e ignora los de Herd", () => {
    const roots = groupRoots(
      ["/Users/ana/Otros", "/Users/ana/Code/", "/Users/ana/Code/tienda"],
      [
        project({ id: "1", name: "b", root: "/Users/ana/Code/b", origin: "user" }),
        project({ id: "2", name: "a", root: "/Users/ana/Code/grupo/a", origin: "user" }),
        project({ id: "3", name: "tienda", root: "/Users/ana/Code/tienda", origin: "user" }),
        project({ id: "4", name: "herd", root: "/Users/ana/Code/herd", origin: "herd_parked" }),
        project({ id: "5", name: "x", root: "/Users/ana/Codex/x", origin: "user" }),
      ],
    );
    expect(roots.map((r) => [r.path, r.projects.map((p) => p.name)])).toEqual([
      ["/Users/ana/Otros", []],
      ["/Users/ana/Code/", ["a", "b"]],
      ["/Users/ana/Code/tienda", ["tienda"]],
    ]);
  });
});

describe("overrideFields", () => {
  it("parte del ajuste guardado o de los valores automáticos", () => {
    expect(overrideFields(null)).toEqual({ timezone: "", globsText: "" });
    expect(overrideFields({ timezone: "UTC", log_globs: ["/a/*.log", "/b/x.log"] })).toEqual({
      timezone: "UTC",
      globsText: "/a/*.log\n/b/x.log",
    });
  });
});

describe("rutas", () => {
  it("abrevia la carpeta personal", () => {
    expect(tildePath("/Users/ana/Code", "/Users/ana")).toBe("~/Code");
    expect(tildePath("/Users/ana", "/Users/ana/")).toBe("~");
    expect(tildePath("/Users/anabel/x", "/Users/ana")).toBe("/Users/anabel/x");
    expect(tildePath("/opt/x", null)).toBe("/opt/x");
  });

  it("deduce $HOME de una ruta de proyecto", () => {
    expect(guessHome(["/opt/x", "/Users/ana/Code/a"])).toBe("/Users/ana");
    expect(guessHome(["/opt/x"])).toBeNull();
  });
});

describe("parseGlobs", () => {
  it("acepta rutas absolutas con * en el nombre", () => {
    const r = parseGlobs("/srv/app/logs/*.log\n\n  /srv/app/worker.log  \n/srv/app/logs/*.log");
    expect(r).toEqual({ globs: ["/srv/app/logs/*.log", "/srv/app/worker.log"], errors: [] });
  });

  it("explica cada línea mala", () => {
    const r = parseGlobs("logs/*.log\n/srv/*/x.log\n/srv/app/");
    expect(r.globs).toEqual([]);
    expect(r.errors).toHaveLength(3);
    expect(r.errors[0]).toContain("no es una ruta absoluta");
    expect(r.errors[1]).toContain("solo puede ir en el nombre");
    expect(r.errors[2]).toContain("falta el nombre");
  });
});

describe("validateTimezone", () => {
  const known = ["UTC", "Europe/Madrid", "America/Argentina/Buenos_Aires"];

  it("vacío es automática", () => {
    expect(validateTimezone("  ", known)).toBeNull();
  });

  it("con lista, exige una conocida", () => {
    expect(validateTimezone("Europe/Madrid", known)).toBeNull();
    expect(validateTimezone("Europe/Mordor", known)).toContain("no es una zona horaria conocida");
  });

  it("sin lista, comprueba la forma", () => {
    expect(validateTimezone("America/Argentina/Buenos_Aires", [])).toBeNull();
    expect(validateTimezone("UTC", [])).toBeNull();
    expect(validateTimezone("hora de Madrid", [])).toContain("no parece");
  });
});

describe("buildOverride", () => {
  it("zona vacía es null y las rutas se limpian", () => {
    expect(buildOverride(" ", " /a/*.log \n")).toEqual({ log_globs: ["/a/*.log"], timezone: null });
    expect(buildOverride("Europe/Madrid", "")).toEqual({ log_globs: [], timezone: "Europe/Madrid" });
  });
});
