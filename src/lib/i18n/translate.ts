// `t()`: busca una clave («settings.general.language.label») en el diccionario
// del idioma actual, elige el plural e interpola los parámetros.
//
// El español de las áreas del chunk inicial va siempre cargado (locales/index.ts);
// el de las áreas diferidas, con su vista (locales/<área>.ts); el inglés, entero
// y aparte, solo cuando hace falta (`loadLocale`). Si falta un texto en inglés se
// usa el español, y si tampoco está, la propia clave.

import { getLocale, intlTag, type Locale } from "./locale.svelte";
import { eagerEs, loaders, type Area, type Messages } from "./locales";
import type { ArgsFor, At, Dictionary, ParamValue, Paths, Plural, Tree } from "./types";

/** Cualquier clave válida: `"<área>.<ruta>"`. */
export type MessageKey = { [A in Area]: `${A}.${Paths<Messages[A]>}` }[Area];

type Loaded = { [A in Area]?: Tree };

const loaded: Record<Locale, Loaded> = { es: { ...eagerEs }, en: {} };
const ready = new Set<Locale>(["es"]);

/** Añade el español de un área diferida. Lo llama locales/<área>.ts al importarse. */
export function registerArea<A extends Area>(area: A, es: Messages[A]): void {
  loaded.es[area] = es as Tree;
}

/** Pone un idioma completo ya cargado (lo usan `loadLocale` y los tests). */
export function registerLocale(locale: Exclude<Locale, "es">, dictionary: Dictionary<Messages>): void {
  loaded[locale] = { ...(dictionary as unknown as Loaded) };
  ready.add(locale);
}

export function isLocaleLoaded(locale: Locale): boolean {
  return ready.has(locale);
}

/** Carga los diccionarios de `locale` si todavía no están. */
export async function loadLocale(locale: Locale): Promise<void> {
  if (locale === "es" || ready.has(locale)) return;
  const { default: dictionary } = await loaders[locale]();
  registerLocale(locale, dictionary);
}

function lookup(tree: Tree | undefined, path: string[]): string | Plural | undefined {
  let node: string | Plural | Tree | undefined = tree;
  for (const part of path) {
    if (node === undefined || typeof node === "string") return undefined;
    node = (node as Tree)[part];
  }
  if (node === undefined || typeof node === "string" || isPlural(node)) return node;
  return undefined;
}

function isPlural(node: Plural | Tree): node is Plural {
  return typeof (node as Plural).other === "string";
}

const pluralRules = new Map<Locale, Intl.PluralRules>();
const numbers = new Map<Locale, Intl.NumberFormat>();

function pluralForm(locale: Locale, plural: Plural, count: number): string {
  let rules = pluralRules.get(locale);
  if (!rules) pluralRules.set(locale, (rules = new Intl.PluralRules(intlTag(locale))));
  const form = rules.select(count) as keyof Plural;
  return plural[form] ?? plural.other;
}

function formatParam(locale: Locale, value: ParamValue): string {
  if (typeof value === "string") return value;
  let format = numbers.get(locale);
  if (!format) numbers.set(locale, (format = new Intl.NumberFormat(intlTag(locale), { maximumFractionDigits: 1 })));
  return format.format(value);
}

/** Sustituye `{nombre}` por su parámetro. Los que no llegan se quedan como están. */
export function interpolate(text: string, params: Record<string, ParamValue> | undefined, locale: Locale): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    Object.hasOwn(params, name) ? formatParam(locale, params[name]!) : whole,
  );
}

let warned = new Set<string>();

function missing(key: string): string {
  if (import.meta.env?.DEV && !warned.has(key)) {
    warned.add(key);
    console.warn(`i18n: falta el texto «${key}» (¿se ha importado locales/<área>.ts?)`);
  }
  return key;
}

/**
 * El texto de `key` en el idioma actual. Los parámetros son obligatorios si el
 * texto los usa, y `count` si es un plural:
 *
 *     t("settings.title")
 *     t("services.stopped", { name: "mysql" })
 *     t("services.running", { count: 3 })
 *
 * Reactivo: dentro de un componente se repinta al cambiar el idioma. No lo
 * llames al cargar un módulo (en constantes de primer nivel): ahí se quedaría
 * con el idioma de ese momento.
 */
export function t<K extends MessageKey>(key: K, ...args: ArgsFor<At<Messages, K>>): string {
  return translate(key, args[0] as Record<string, ParamValue> | undefined);
}

/** `t()` sin tipos, para claves que se construyen en tiempo de ejecución. Evítalo. */
export function translate(key: string, params?: Record<string, ParamValue>): string {
  const locale = getLocale();
  const dot = key.indexOf(".");
  if (dot < 0) return missing(key);
  const area = key.slice(0, dot) as Area;
  const path = key.slice(dot + 1).split(".");
  let used: Locale = locale;
  let value = lookup(loaded[locale][area], path);
  if (value === undefined && locale !== "es") {
    used = "es";
    value = lookup(loaded.es[area], path);
  }
  if (value === undefined) return missing(key);
  const text = typeof value === "string" ? value : pluralForm(used, value, Number(params?.count ?? 0));
  return interpolate(text, params, used);
}

/** Solo para tests: olvida los avisos de claves que faltan. */
export function resetMissingWarnings(): void {
  warned = new Set();
}
