// Puerta de entrada de la internacionalización (docs/i18n.md). Las vistas y los
// stores importan de aquí:
//
//     import { t, fmt } from "../lib/i18n/index.svelte";
//     t("settings.general.language.label")
//     t("services.running", { count })          // plural con Intl.PluralRules
//     fmt.bytes(8_500_000)                      // «8,5 MB» o «8.5 MB»
//
// El idioma lo decide Rust (`Snapshot.locale`, que ya resuelve «Sistema»):
// App.svelte llama a `changeLocale` cada vez que cambia.

import { getLocale, setLocaleNow, type Locale } from "./locale.svelte";
import { loadLocale } from "./translate";

export { detectLocale, getLocale, intlTag, isLocale, localeFromTag, LOCALES, type Locale } from "./locale.svelte";
export { interpolate, isLocaleLoaded, loadLocale, t, translate, type MessageKey } from "./translate";
export { AREAS, DEFERRED_AREAS, type Area, type Messages } from "./locales";
export type { Dictionary, Plural, Tree } from "./types";
export { fmt } from "../format";

/** El idioma actual, reactivo: `i18n.locale`. */
export const i18n = {
  get locale(): Locale {
    return getLocale();
  },
};

let requested: Locale | null = null;

/**
 * Cambia el idioma después de cargar sus textos, para no pintar a medias. Si
 * llegan varias peticiones seguidas, gana la última. Si la carga falla, se queda
 * el idioma que había.
 */
export async function changeLocale(locale: Locale): Promise<void> {
  requested = locale;
  try {
    await loadLocale(locale);
  } catch (e) {
    console.error(`No se pudieron cargar los textos de «${locale}»`, e);
    return;
  }
  if (requested === locale) setLocaleNow(locale);
}

/** Para arrancar la ventana: carga el idioma inicial y pone `<html lang>`. */
export function startLocale(): Promise<void> {
  return changeLocale(getLocale());
}
