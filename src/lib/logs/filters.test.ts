import { describe, expect, it } from "vitest";

import { parse, serialize, withParams } from "../route";
import {
  DEFAULT_FILTERS,
  clearedFilters,
  filtersKey,
  filtersToParams,
  hasFilterParams,
  isFiltered,
  parseFilters,
  rangeSince,
  resolveProject,
  toQuery,
  type Filters,
} from "./filters";
import { summary } from "./fixtures";

const NOW = new Date(2026, 10, 6, 15, 30, 0).getTime();

describe("parseFilters", () => {
  it("sin parámetros da los valores por defecto", () => {
    expect(parseFilters({})).toEqual(DEFAULT_FILTERS);
  });

  it("lee el enlace del tray «Errores recientes»", () => {
    const f = parseFilters(parse("#/logs?level=error&range=1h").params);
    expect(f.level).toBe("error");
    expect(f.range).toBe("1h");
    expect(f.project).toBe("all");
  });

  it("acepta los alias de fuente y nivel", () => {
    expect(parseFilters({ source: "laravel" }).source).toBe("app");
    expect(parseFilters({ source: "PHP_FPM" }).source).toBe("php-fpm");
    expect(parseFilters({ level: "warn" }).level).toBe("warning");
    expect(parseFilters({ level: "critical" }).level).toBe("error");
  });

  it("descarta los valores desconocidos", () => {
    const f = parseFilters({ source: "syslog", level: "trace", range: "3y", status: "2xx", mode: "x" });
    expect(f).toEqual(DEFAULT_FILTERS);
  });

  it("lee el texto, el estado y el modo", () => {
    const f = parseFilters({ q: "SQLSTATE", status: "5XX", mode: "grouped", project: "abc" });
    expect(f).toMatchObject({ text: "SQLSTATE", status: "5xx", mode: "grouped", project: "abc" });
  });
});

describe("filtersToParams", () => {
  it("ida y vuelta por el hash sin perder nada", () => {
    const f: Filters = {
      project: "0123456789abcdef",
      source: "http",
      level: "warning",
      range: "7d",
      text: "pedido 42 & más",
      status: "5xx",
      mode: "grouped",
    };
    const route = withParams(parse("#/logs"), filtersToParams(f));
    const back = parseFilters(parse(serialize(route)).params);
    expect(back).toEqual(f);
  });

  it("los valores por defecto no se escriben en el hash", () => {
    const route = withParams(parse("#/logs?level=error"), filtersToParams(DEFAULT_FILTERS));
    expect(serialize(route)).toBe("#/logs");
  });

  it("un texto de solo espacios no se guarda", () => {
    expect(filtersToParams({ ...DEFAULT_FILTERS, text: "   " }).q).toBeNull();
  });
});

describe("hasFilterParams e isFiltered", () => {
  it("distingue un enlace con filtros de uno sin ellos", () => {
    expect(hasFilterParams({})).toBe(false);
    expect(hasFilterParams({ entry: "x-1" })).toBe(false);
    expect(hasFilterParams({ range: "1h" })).toBe(true);
  });

  it("el periodo y el modo no cuentan como filtro que quitar", () => {
    expect(isFiltered({ ...DEFAULT_FILTERS, range: "1h", mode: "grouped" })).toBe(false);
    expect(isFiltered({ ...DEFAULT_FILTERS, text: "x" })).toBe(true);
    const cleared = clearedFilters({ ...DEFAULT_FILTERS, level: "error", range: "1h", mode: "grouped" });
    expect(cleared).toEqual({ ...DEFAULT_FILTERS, range: "1h", mode: "grouped" });
  });
});

describe("rangeSince", () => {
  it("calcula cada periodo", () => {
    expect(rangeSince("15m", NOW)).toBe(NOW - 15 * 60_000);
    expect(rangeSince("1h", NOW)).toBe(NOW - 3_600_000);
    expect(rangeSince("7d", NOW)).toBe(NOW - 7 * 86_400_000);
    expect(rangeSince("all", NOW)).toBeNull();
  });

  it("«Hoy» empieza a medianoche local", () => {
    expect(rangeSince("today", NOW)).toBe(new Date(2026, 10, 6).getTime());
  });
});

describe("toQuery", () => {
  it("traduce cada filtro al contrato de Rust", () => {
    const q = toQuery(
      { project: "none", source: "nginx", level: "error", range: "1h", text: "  timeout ", status: "4xx", mode: "chrono" },
      NOW,
      "abc-1",
    );
    expect(q).toEqual({
      projects: ["none"],
      kinds: ["nginx_error"],
      min_level: "error",
      text: "timeout",
      status: { kind: "class", value: 4 },
      since_ms: NOW - 3_600_000,
      until_ms: null,
      before: "abc-1",
      limit: 200,
    });
  });

  it("sin filtros pide todo de hoy", () => {
    const q = toQuery(DEFAULT_FILTERS, NOW);
    expect(q.projects).toEqual([]);
    expect(q.kinds).toEqual([]);
    expect(q.min_level).toBeNull();
    expect(q.text).toBeNull();
    expect(q.status).toBeNull();
    expect(q.before).toBeNull();
  });

  it("cada fuente va a su SourceKind", () => {
    const kinds = (["app", "http", "nginx", "php-fpm"] as const).map((source) => toQuery({ ...DEFAULT_FILTERS, source }, NOW).kinds[0]);
    expect(kinds).toEqual(["laravel_app", "nginx_access", "nginx_error", "php_fpm"]);
  });
});

describe("filtersKey", () => {
  it("cambia con lo que obliga a recargar y no con espacios sobrantes", () => {
    const base = filtersKey(DEFAULT_FILTERS);
    expect(filtersKey({ ...DEFAULT_FILTERS, text: " " })).toBe(base);
    expect(filtersKey({ ...DEFAULT_FILTERS, mode: "grouped" })).not.toBe(base);
    expect(filtersKey({ ...DEFAULT_FILTERS, project: "p1" })).not.toBe(base);
  });
});

describe("resolveProject", () => {
  const list = [summary({ id: "aaaa", name: "Tienda", hosts: ["tienda.test"] }), summary({ id: "bbbb", name: "blog", hosts: [] })];

  it("acepta id, nombre o host", () => {
    expect(resolveProject("bbbb", list)).toBe("bbbb");
    expect(resolveProject("tienda", list)).toBe("aaaa");
    expect(resolveProject("TIENDA.test", list)).toBe("aaaa");
  });

  it("deja tal cual los especiales y lo que no encuentra", () => {
    expect(resolveProject("none", list)).toBe("none");
    expect(resolveProject("system", list)).toBe("system");
    expect(resolveProject("cccc", list)).toBe("cccc");
  });
});
