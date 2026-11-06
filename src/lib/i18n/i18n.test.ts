import { describe, expect, it } from "vitest";
import { fmt } from "../format";
import { changeLocale, getLocale, i18n, interpolate, localeFromTag, t, translate } from "./index.svelte";
import { withLocale } from "./testing";
import { AREAS } from "./locales";
import en from "./locales/en/index";
import common from "./locales/es/common";
import settings from "./locales/es/settings";
import type { Plural, Tree } from "./types";

// Las salidas llevan espacio duro entre cifra y unidad; en los tests se escribe «~».
const s = (text: string) => text.replaceAll("~", " ");

describe("idioma", () => {
  it("los tests empiezan en español", () => {
    expect(getLocale()).toBe("es");
    expect(i18n.locale).toBe("es");
  });

  it("reconoce el español por el prefijo, como Rust", () => {
    for (const tag of ["es", "es-ES", "es_419", "ES-mx"]) expect(localeFromTag(tag)).toBe("es");
    for (const tag of ["en-US", "fr", "eu-ES", "", null, undefined]) expect(localeFromTag(tag)).toBe("en");
  });

  it("changeLocale cambia al momento si los textos ya están", async () => {
    await changeLocale("en");
    expect(t("settings.title")).toBe("Settings");
    await changeLocale("es");
    expect(t("settings.title")).toBe("Ajustes");
  });
});

describe("t", () => {
  it("da el texto del idioma actual", () => {
    expect(t("settings.general.language.label")).toBe("Idioma");
    expect(withLocale("en", () => t("settings.general.language.label"))).toBe("Language");
  });

  it("interpola los parámetros", () => {
    expect(t("settings.general.phpSource.recommended", { source: "x/y" })).toBe("x/y (recomendado)");
    expect(withLocale("en", () => t("settings.general.saveFailed", { error: "disk" }))).toBe(
      "Couldn’t save the setting: disk",
    );
  });

  it("elige el plural con Intl.PluralRules y formatea count", () => {
    const versions = (count: number) => t("settings.general.phpSource.versions", { count, list: "8.4" });
    expect(versions(1)).toBe("1 versión: 8.4");
    expect(versions(3)).toBe("3 versiones: 8.4");
    expect(versions(12345)).toBe("12.345 versiones: 8.4");
    expect(withLocale("en", () => versions(1))).toBe("1 version: 8.4");
    expect(withLocale("en", () => versions(12345))).toBe("12,345 versions: 8.4");
  });

  it("una clave que no existe se devuelve tal cual", () => {
    expect(translate("settings.noExiste")).toBe("settings.noExiste");
    expect(translate("sinpunto")).toBe("sinpunto");
  });

  it("deja las llaves sin parámetro como están", () => {
    expect(interpolate("{a} y {b}", { a: "uno" }, "es")).toBe("uno y {b}");
  });

  it("comprueba las claves y los parámetros al compilar", () => {
    // @ts-expect-error: la clave no existe.
    expect(t("settings.nope")).toBe("settings.nope");
    // @ts-expect-error: falta el parámetro `error`.
    expect(t("settings.general.saveFailed")).toContain("{error}");
    // @ts-expect-error: un plural pide `count`.
    expect(t("settings.general.phpSource.versions", { list: "x" })).toContain("x");
    // @ts-expect-error: este texto no lleva parámetros.
    expect(t("settings.title", { x: 1 })).toBe("Ajustes");
  });
});

// Recorre un diccionario y da cada hoja con su ruta.
function leaves(tree: Tree, prefix = ""): [string, string | Plural][] {
  return Object.entries(tree).flatMap(([key, value]) => {
    const path = `${prefix}${key}`;
    if (typeof value === "string" || typeof (value as Plural).other === "string") {
      return [[path, value as string | Plural]] as [string, string | Plural][];
    }
    return leaves(value as Tree, `${path}.`);
  });
}

function placeholders(value: string | Plural): string[] {
  const texts = typeof value === "string" ? [value] : Object.values(value);
  return [...new Set(texts.flatMap((text) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]!)))].sort();
}

describe("diccionarios", () => {
  it("el inglés usa los mismos parámetros que el español", async () => {
    const es: Record<string, Tree> = {
      common,
      settings,
      ...Object.fromEntries(
        await Promise.all(
          AREAS.filter((a) => a !== "common" && a !== "settings").map(
            async (a) => [a, (await import(`./locales/es/${a}.ts`)).default as Tree] as const,
          ),
        ),
      ),
    };
    for (const area of AREAS) {
      const english = new Map(leaves(en[area] as Tree));
      for (const [path, value] of leaves(es[area]!)) {
        const other = english.get(path);
        expect(other, `${area}.${path}`).toBeDefined();
        expect(placeholders(other!), `${area}.${path}`).toEqual(placeholders(value));
      }
    }
  });

  it("los textos no llevan espacios al principio ni al final", () => {
    for (const area of AREAS) {
      for (const [path, value] of leaves(en[area] as Tree)) {
        for (const text of typeof value === "string" ? [value] : Object.values(value)) {
          expect(text.trim(), `${area}.${path}`).toBe(text);
        }
      }
    }
  });
});

describe("formatos en inglés", () => {
  const en = <T>(fn: () => T) => withLocale("en", fn);

  it("usa los separadores del inglés", () => {
    expect(en(() => fmt.number(1234))).toBe("1,234");
    expect(en(() => fmt.bytes(8_500_000))).toBe(s("8.5~MB"));
    expect(en(() => fmt.memory(1.25 * 1024 * 1024))).toBe(s("1.3~GB"));
    expect(en(() => fmt.duration(1234))).toBe(s("1.2~s"));
  });

  it("escribe el porcentaje sin espacio", () => {
    expect(en(() => fmt.cpu(12.4))).toBe("12%");
    expect(en(() => fmt.cpu(0.3))).toBe("<1%");
    expect(fmt.cpu(12.4)).toBe(s("12~%"));
  });

  it("da fechas cortas y relativas", () => {
    const now = new Date(2026, 10, 6, 12, 0, 0).getTime();
    const min = 60_000;
    expect(en(() => fmt.shortDate(new Date(2026, 8, 3), now))).toBe("Sep 3");
    expect(en(() => fmt.shortDate(new Date(2025, 0, 15), now))).toBe("Jan 15, 2025");
    expect(en(() => fmt.relative(now - 10_000, now))).toBe("just now");
    expect(en(() => fmt.relative(now - 5 * min, now))).toBe(s("5~min ago"));
    expect(en(() => fmt.relative(now + 5 * min, now))).toBe(s("in 5~min"));
    expect(en(() => fmt.relative(now - 120 * min, now))).toBe(s("2~hr ago"));
    expect(en(() => fmt.relative(new Date(2026, 10, 5, 23, 0).getTime(), now))).toBe("yesterday");
    expect(en(() => fmt.relative(new Date(2026, 10, 3, 18, 0).getTime(), now))).toBe("3 days ago");
  });

  it("une listas según el idioma", () => {
    expect(fmt.list(["a", "b", "c"])).toBe("a, b y c");
    expect(en(() => fmt.list(["a", "b", "c"]))).toBe("a, b, and c");
  });
});
