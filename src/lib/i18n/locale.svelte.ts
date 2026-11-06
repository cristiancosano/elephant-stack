// Idioma actual de la interfaz. Lo fija `Snapshot.locale` (Rust ya ha resuelto
// «Sistema»); antes del primer snapshot, el que deja Rust en la ventana
// (`window.__ELEPHANT_LOCALE__`, src-tauri/src/window.rs) o el del navegador.
// Va aparte de `t()` para que format.ts lo lea sin importar los diccionarios.

import type { Locale } from "../bindings/Locale";

export type { Locale };

export const LOCALES: readonly Locale[] = ["es", "en"];

/** `es`, `es-ES` o `es_419` → español; cualquier otro idioma → inglés (como Rust). */
export function localeFromTag(tag: string | null | undefined): Locale {
  return tag && /^es(?:[-_.@]|$)/i.test(tag.trim()) ? "es" : "en";
}

export function isLocale(value: unknown): value is Locale {
  return value === "es" || value === "en";
}

/** El idioma con el que arranca la ventana, antes del primer snapshot. */
export function detectLocale(): Locale {
  const g = globalThis as { __ELEPHANT_LOCALE__?: unknown; navigator?: { language?: string } };
  if (isLocale(g.__ELEPHANT_LOCALE__)) return g.__ELEPHANT_LOCALE__;
  return localeFromTag(g.navigator?.language);
}

let current = $state<Locale>(detectLocale());

/** Reactivo: un componente o un `$derived` que lo lee se repinta al cambiar. */
export function getLocale(): Locale {
  return current;
}

/**
 * Cambia el idioma al momento. Los diccionarios que no son el español se cargan
 * aparte: usa `changeLocale` (index.svelte.ts), que los carga antes.
 */
export function setLocaleNow(locale: Locale): void {
  current = locale;
  if (typeof document !== "undefined") document.documentElement.lang = locale;
}

/** Etiqueta BCP 47 para `Intl`: `es-ES` o `en-US`. */
export function intlTag(locale: Locale = current): string {
  return locale === "es" ? "es-ES" : "en-US";
}
