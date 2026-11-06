import { describe, expect, it } from "vitest";

import { detail, entry, frame } from "./fixtures";
import { entryLine, entryMarkdown, fence, foldFrames, formatTimestamp, trimFrames } from "./markdown";

describe("foldFrames", () => {
  it("junta los frames de vendor seguidos", () => {
    const frames = [frame(0, "/app/A.php"), frame(1, "/v/vendor/x.php", true), frame(2, "/v/vendor/y.php", true), frame(3, "/app/B.php"), frame(4, "/v/vendor/z.php", true)];
    const segments = foldFrames(frames);
    expect(segments.map((s) => (s.kind === "own" ? `own:${s.frame.index}` : `vendor:${s.frames.length}`))).toEqual([
      "own:0",
      "vendor:2",
      "own:3",
      "vendor:1",
    ]);
  });

  it("sin frames, nada", () => {
    expect(foldFrames([])).toEqual([]);
  });
});

describe("trimFrames", () => {
  it("8 propios más el primero de vendor (C14)", () => {
    const frames = [
      ...Array.from({ length: 10 }, (_, i) => frame(i, `/app/F${i}.php`)),
      frame(10, "/vendor/a.php", true),
      frame(11, "/vendor/b.php", true),
      frame(12, "/vendor/c.php", true),
    ];
    const t = trimFrames(frames);
    expect(t.shown.map((f) => f.index)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 10]);
    expect(t.omittedOwn).toBe(2);
    expect(t.omittedVendor).toBe(2);
  });

  it("conserva el orden si el vendor va primero", () => {
    const t = trimFrames([frame(0, "/vendor/a.php", true), frame(1, "/app/A.php")]);
    expect(t.shown.map((f) => f.index)).toEqual([0, 1]);
  });
});

describe("fence", () => {
  it("usa más comillas invertidas que el contenido", () => {
    expect(fence("a ``` b")).toBe("````\na ``` b\n````");
    expect(fence("plano", "json")).toBe("```json\nplano\n```");
  });
});

describe("formatTimestamp", () => {
  it("fecha y hora local con milisegundos", () => {
    const ms = new Date(2026, 0, 2, 3, 4, 5, 6).getTime();
    expect(formatTimestamp(ms)).toBe("2026-01-02 03:04:05.006");
  });
});

describe("entryLine", () => {
  it("una línea con nivel y proyecto", () => {
    const e = entry({ ts_ms: new Date(2026, 10, 6, 9, 5, 3, 7).getTime() });
    expect(entryLine(e, "tienda")).toBe("2026-11-06 09:05:03.007 ERROR [tienda] Undefined variable $pedido");
  });

  it("en HTTP, método, ruta y estado", () => {
    const e = entry({
      kind: "nginx_access",
      level: "error",
      http: { method: "GET", path: "/pedidos", status: 500, bytes: null, duration_ms: 1200, upstream_ms: null, ip: null, ua: null },
    });
    expect(entryLine(e, "tienda")).toContain("GET /pedidos → 500 · 1,2 s");
  });
});

describe("entryMarkdown", () => {
  it("incluye excepción, traza recortada y contexto", () => {
    const d = detail({
      entry: entry({
        exception: { class: "ErrorException", message: "Undefined variable $pedido", file: "/app/Http/C.php", line: 42 },
      }),
      frames: [frame(0, "/app/Http/C.php"), frame(1, "/vendor/laravel/framework/R.php", true, true), frame(2, "/vendor/x.php", true)],
      context: { userId: 7 },
    });
    const md = entryMarkdown(d, "tienda");
    expect(md).toMatch(/^### Error · tienda · /);
    expect(md).toContain("**Fuente:** App (canal `local`)");
    expect(md).toContain("**Excepción:** `ErrorException` en `/app/Http/C.php:42`");
    expect(md).toContain("#0 /app/Http/C.php(10): fn0()");
    expect(md).toContain("#1 /vendor/laravel/framework/R.php(11)");
    expect(md).not.toContain("#2 ");
    expect(md).toContain("+1 frames de vendor");
    expect(md).toContain('```json\n{\n  "userId": 7\n}\n```');
  });

  it("sin frames copia el cuerpo, recortado", () => {
    const md = entryMarkdown(detail({ body: `línea 1\n${"x".repeat(5000)}` }), "tienda");
    expect(md).toContain("**Entrada completa**");
    expect(md).toContain("… (recortado)");
  });

  it("no repite el cuerpo si es solo el mensaje ni muestra un contexto vacío", () => {
    const md = entryMarkdown(detail({ body: "Undefined variable $pedido\n", context: {} }), "tienda");
    expect(md).not.toContain("**Entrada completa**");
    expect(md).not.toContain("**Contexto**");
  });
});
