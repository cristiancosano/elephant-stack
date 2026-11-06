import { describe, expect, it } from "vitest";
import { parse, sameRoute, serialize, toRoute, withParams, VIEWS } from "./route";

describe("parse", () => {
  it("lee la vista, la subruta y los parámetros", () => {
    expect(parse("#/logs?project=acme&level=error")).toEqual({
      view: "logs",
      sub: null,
      params: { project: "acme", level: "error" },
    });
    expect(parse("#/ajustes/general")).toEqual({ view: "ajustes", sub: "general", params: {} });
  });

  it("acepta variantes sin almohadilla o sin barra", () => {
    for (const hash of ["#/php", "#php", "/php", "php", "#//php/"]) {
      expect(parse(hash).view).toBe("php");
    }
  });

  it("devuelve el panel ante rutas vacías o desconocidas", () => {
    for (const hash of ["", "#", "#/", "#/nada?x=1", "#/../etc"]) {
      expect(parse(hash)).toEqual({ view: "panel", sub: null, params: {} });
    }
  });

  it("no distingue mayúsculas en la vista", () => {
    expect(parse("#/LOGS").view).toBe("logs");
  });

  it("decodifica caracteres especiales", () => {
    const r = parse("#/logs?project=mi%20tienda&contains=a%26b");
    expect(r.params).toEqual({ project: "mi tienda", contains: "a&b" });
  });

  it("conserva subrutas con varios segmentos", () => {
    expect(parse("#/asistente/personalizar/php").sub).toBe("personalizar/php");
  });

  it("tolera secuencias de escape inválidas", () => {
    expect(parse("#/ajustes/%E0%A4%A").sub).toBe("%E0%A4%A");
  });
});

describe("serialize", () => {
  it("escribe el hash canónico", () => {
    expect(serialize({ view: "logs", sub: null, params: { project: "x", level: "error" } })).toBe(
      "#/logs?project=x&level=error",
    );
    expect(serialize({ view: "ajustes", sub: "logs", params: {} })).toBe("#/ajustes/logs");
  });

  it("omite los parámetros vacíos", () => {
    expect(serialize({ view: "panel", sub: null, params: { a: "" } })).toBe("#/panel");
  });

  it("es la inversa de parse", () => {
    const hashes = [
      "#/panel",
      "#/logs?project=mi+tienda&level=error",
      "#/ajustes/agentes",
      "#/sitios?q=a%2Fb",
    ];
    for (const hash of hashes) {
      const r = parse(hash);
      expect(parse(serialize(r))).toEqual(r);
    }
    for (const view of VIEWS) {
      expect(parse(serialize({ view, sub: null, params: {} })).view).toBe(view);
    }
  });
});

describe("withParams", () => {
  it("añade, cambia y quita claves sin mutar la ruta", () => {
    const r = parse("#/logs?project=a&level=error");
    const next = withParams(r, { project: "b", level: null, since: "1h", q: "" });
    expect(next.params).toEqual({ project: "b", since: "1h" });
    expect(r.params).toEqual({ project: "a", level: "error" });
  });
});

describe("toRoute y sameRoute", () => {
  it("normaliza cadenas y compara por su forma canónica", () => {
    expect(toRoute("logs").view).toBe("logs");
    expect(sameRoute(toRoute("#logs?a=1"), toRoute("/logs?a=1"))).toBe(true);
    expect(sameRoute(toRoute("#/logs?a=1"), toRoute("#/logs?a=2"))).toBe(false);
  });
});
