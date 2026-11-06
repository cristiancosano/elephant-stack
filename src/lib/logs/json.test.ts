import { describe, expect, it } from "vitest";

import { MAX_PRETTY_CHARS, hasContent, prettyJson, tokenize, type Token } from "./json";

const kinds = (tokens: Token[]) => tokens.filter((t) => t.kind !== "space").map((t) => `${t.kind}:${t.text}`);

describe("tokenize", () => {
  it("distingue claves, cadenas, números y literales", () => {
    const tokens = tokenize('{"id": 42, "name": "Ana", "ok": true, "x": null, "r": -1.5e3}');
    expect(kinds(tokens)).toEqual([
      "punct:{",
      'key:"id"',
      "punct::",
      "number:42",
      "punct:,",
      'key:"name"',
      "punct::",
      'string:"Ana"',
      "punct:,",
      'key:"ok"',
      "punct::",
      "literal:true",
      "punct:,",
      'key:"x"',
      "punct::",
      "literal:null",
      "punct:,",
      'key:"r"',
      "punct::",
      "number:-1.5e3",
      "punct:}",
    ]);
  });

  it("respeta las comillas escapadas", () => {
    const tokens = tokenize('["a \\"b\\" c"]');
    expect(kinds(tokens)).toEqual(["punct:[", 'string:"a \\"b\\" c"', "punct:]"]);
  });

  it("una cadena en un array no es clave aunque la siga otra línea", () => {
    const tokens = tokenize('[\n  "a",\n  "b"\n]');
    expect(tokens.filter((t) => t.kind === "key")).toHaveLength(0);
  });

  it("no pierde ningún carácter, ni con texto que no es JSON", () => {
    const inputs = ['{"a":1}', "no es json: [object] (Clase)", '{"abierta": "sin cerrar\n}', "truex nullish 12abc"];
    for (const text of inputs) {
      expect(
        tokenize(text)
          .map((t) => t.text)
          .join(""),
      ).toBe(text);
    }
  });

  it("junta el texto suelto en un solo token", () => {
    const tokens = tokenize("hola");
    expect(tokens).toEqual([{ kind: "text", text: "hola" }]);
  });

  it("no confunde palabras que empiezan como un literal", () => {
    expect(tokenize("nullish").some((t) => t.kind === "literal")).toBe(false);
  });
});

describe("prettyJson", () => {
  it("sangra con dos espacios", () => {
    expect(prettyJson({ a: [1] }).text).toBe('{\n  "a": [\n    1\n  ]\n}');
  });

  it("recorta lo enorme", () => {
    const big = { s: "x".repeat(MAX_PRETTY_CHARS + 10) };
    const out = prettyJson(big);
    expect(out.truncated).toBe(true);
    expect(out.text.length).toBe(MAX_PRETTY_CHARS);
  });
});

describe("hasContent", () => {
  it("ignora los vacíos", () => {
    expect(hasContent(null)).toBe(false);
    expect(hasContent({})).toBe(false);
    expect(hasContent([])).toBe(false);
    expect(hasContent({ a: 1 })).toBe(true);
    expect(hasContent("x")).toBe(true);
  });
});
