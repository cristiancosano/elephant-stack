import { describe, expect, it } from "vitest";

import { entry } from "./fixtures";
import {
  NOT_AVAILABLE,
  atLeast,
  canClear,
  chronological,
  errorCount,
  fileKeyOf,
  httpClass,
  isNotAvailable,
  levelLabel,
  levelTone,
  projectLabel,
  projectNames,
  shortClass,
  sourceLabel,
} from "./model";

describe("niveles", () => {
  it("compara por gravedad", () => {
    expect(atLeast("critical", "error")).toBe(true);
    expect(atLeast("warning", "error")).toBe(false);
  });

  it("etiquetas en español y colores agrupados", () => {
    expect(levelLabel("warning")).toBe("Aviso");
    expect(levelTone("emergency")).toBe("critical");
    expect(levelTone("notice")).toBe("notice");
  });
});

describe("etiquetas", () => {
  it("fuentes de C19", () => {
    expect(sourceLabel("laravel_app")).toBe("App");
    expect(sourceLabel("php_fpm")).toBe("PHP-FPM");
  });

  it("clases HTTP", () => {
    expect([200, 304, 404, 503].map(httpClass)).toEqual(["2xx", "3xx", "4xx", "5xx"]);
  });

  it("contador con tope", () => {
    expect(errorCount(500, true)).toBe("500+");
    expect(errorCount(3, false)).toBe("3");
  });
});

describe("projectLabel", () => {
  const names = projectNames([{ id: "p1", name: "tienda" }]);

  it("nombre del proyecto, host sin sitio, Sistema o Sin proyecto", () => {
    expect(projectLabel(entry(), names)).toBe("tienda");
    expect(projectLabel(entry({ project: null, kind: "nginx_error", host: "viejo.test" }), names)).toBe("viejo.test");
    expect(projectLabel(entry({ project: null, kind: "php_fpm" }), names)).toBe("Sistema");
    expect(projectLabel(entry({ project: null, kind: "nginx_error" }), names)).toBe("Sin proyecto");
    expect(projectLabel(entry({ project: "zz" }), names)).toBe("zz");
  });
});

describe("vaciar archivo", () => {
  it("saca la clave del archivo del id", () => {
    expect(fileKeyOf("00000000000000ab-1f")).toBe("00000000000000ab");
    expect(fileKeyOf("sin-guion-raro")).toBeNull();
    expect(fileKeyOf("-1")).toBeNull();
  });

  it("solo los logs de la app de un proyecto", () => {
    expect(canClear(entry())).toBe(true);
    expect(canClear(entry({ kind: "nginx_error" }))).toBe(false);
    expect(canClear(entry({ project: null }))).toBe(false);
  });
});

describe("shortClass", () => {
  it("se queda con el nombre corto de la clase", () => {
    expect(shortClass("Illuminate\\Database\\QueryException")).toBe("QueryException");
    expect(shortClass("ErrorException")).toBe("ErrorException");
  });
});

describe("chronological", () => {
  it("da la vuelta a una página de más reciente a más antigua", () => {
    const page = [entry({ id: "c", ts_ms: 3 }), entry({ id: "b2", ts_ms: 2 }), entry({ id: "b1", ts_ms: 2 }), entry({ id: "a", ts_ms: 1 })];
    expect(chronological(page).map((e) => e.id)).toEqual(["a", "b1", "b2", "c"]);
    expect(page[0].id).toBe("c"); // no la modifica
  });
});

describe("isNotAvailable", () => {
  it("reconoce el error de los stubs", () => {
    expect(isNotAvailable(NOT_AVAILABLE)).toBe(true);
    expect(isNotAvailable("La entrada ya no existe: el fichero ha cambiado.")).toBe(false);
  });
});
