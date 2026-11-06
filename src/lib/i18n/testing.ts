// Ayudas de i18n para los tests de vitest. `test-setup.ts` deja el español fijo
// antes de cada test: los textos esperados de los tests se escriben en español
// salvo dentro de `withLocale("en", …)`. Carga todas las áreas y los dos idiomas.
import "./locales/agents";
import "./locales/logs";
import "./locales/sites";
import "./locales/wizard";
import en from "./locales/en/index";
import { getLocale, setLocaleNow, type Locale } from "./locale.svelte";
import { registerLocale } from "./translate";

registerLocale("en", en);

/** Fija el idioma de los tests que vienen detrás (hasta el siguiente `beforeEach`). */
export function setTestLocale(locale: Locale): void {
  setLocaleNow(locale);
}

/** Ejecuta `fn` con `locale` y vuelve al idioma anterior, también si falla. */
export function withLocale<T>(locale: Locale, fn: () => T): T {
  const previous = getLocale();
  setLocaleNow(locale);
  try {
    return fn();
  } finally {
    setLocaleNow(previous);
  }
}
