//! Idioma de lo que leen la persona y el agente: la app, el menú de la barra,
//! los diálogos de confirmación, la orden `elephant` y el motor de logs
//! (docs/i18n.md).
//!
//! Es un crate aparte y ligero para que lo usen `elephant-core` y
//! `elephant-logs`, que no dependen el uno del otro. `elephant-core` lo
//! reexporta como `elephant_core::i18n` (con `resolve`) y `elephant_core::tr`.
//!
//! Los textos se escriben en el sitio donde se usan, con los dos idiomas a la
//! vez, mediante [`tr!`](crate::tr) y [`tr_str!`](crate::tr_str):
//!
//! ```
//! use elephant_i18n::{tr, tr_str};
//! let svc = "mysql";
//! let title: String = tr!("Arrancar", "Start");
//! let stopped: String = tr!("{name} se ha detenido", "{name} stopped", name = svc);
//! let busy: &'static str = tr_str!("Reiniciando…", "Restarting…");
//! # let _ = (title, stopped, busy);
//! ```
//!
//! El compilador obliga a dar los dos idiomas y comprueba los argumentos de
//! cada uno. Lo que leen las máquinas (`--json`, ids, códigos de salida y
//! nombres de campos) no se traduce.

use std::cell::Cell;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};


/// Idioma ya resuelto. `Language::System` se convierte en uno de estos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Locale {
    Es,
    En,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::Es, Locale::En];

    /// Código BCP 47 corto: `es` o `en`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Es => "es",
            Self::En => "en",
        }
    }

    /// `es`, `es-ES`, `es_419` o `es_ES.UTF-8` dan español; cualquier otro
    /// idioma, inglés.
    pub fn from_tag(tag: &str) -> Self {
        let tag = tag.trim().trim_matches('"');
        let lang = tag.split(['-', '_', '.', '@']).next().unwrap_or_default();
        if lang.eq_ignore_ascii_case("es") { Self::Es } else { Self::En }
    }
}

/// El idioma preferido de macOS: el primero de `AppleLanguages` y, si no se
/// puede leer, `LC_ALL`, `LC_MESSAGES` o `LANG`. Español si empieza por «es»;
/// inglés en cualquier otro caso.
pub fn system_locale() -> Locale {
    apple_languages()
        .or_else(|| locale_from_env(|key| std::env::var(key).ok()))
        .unwrap_or(Locale::En)
}

/// `defaults read -g AppleLanguages`: unos 8 ms, una vez por proceso.
fn apple_languages() -> Option<Locale> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_apple_languages(&String::from_utf8_lossy(&output.stdout))
}

/// La salida de `defaults` es una lista de NeXTSTEP: `(\n    "es-ES",\n    en\n)`.
fn parse_apple_languages(text: &str) -> Option<Locale> {
    text.trim()
        .trim_start_matches('(')
        .split([',', '\n'])
        .map(|item| item.trim().trim_matches('"').trim())
        .find(|item| !item.is_empty() && *item != ")")
        .map(Locale::from_tag)
}

/// Variables de entorno de POSIX, en su orden de prioridad. `C` y `POSIX` no
/// dicen nada del idioma.
fn locale_from_env(get: impl Fn(&str) -> Option<String>) -> Option<Locale> {
    ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(|key| {
        let value = get(key)?;
        let value = value.trim();
        if value.is_empty() || value == "C" || value == "POSIX" || value.starts_with("C.") {
            return None;
        }
        Some(Locale::from_tag(value))
    })
}

/// 0: sin resolver todavía; si no, `encode(locale)`.
static CURRENT: AtomicU8 = AtomicU8::new(0);

thread_local! {
    /// Lo fija [`with_locale`] solo en este hilo (tests en inglés).
    static SCOPED: Cell<Option<Locale>> = const { Cell::new(None) };
}

fn encode(locale: Locale) -> u8 {
    match locale {
        Locale::Es => 1,
        Locale::En => 2,
    }
}

fn decode(value: u8) -> Option<Locale> {
    match value {
        1 => Some(Locale::Es),
        2 => Some(Locale::En),
        _ => None,
    }
}

/// El idioma con el que se escribe ahora. La primera vez lo resuelve a partir
/// de `settings.json` y lo guarda para el resto del proceso; la app lo cambia
/// con [`set_override`] al cambiar el ajuste.
///
/// En los tests (`cfg(test)` o la característica `test-locale`) es español,
/// salvo dentro de [`with_locale`]. En debug, `ELEPHANT_STACK_LANG=es|en`
/// manda sobre el ajuste, para los tests que ejecutan la CLI.
pub fn locale() -> Locale {
    if let Some(locale) = SCOPED.with(Cell::get) {
        return locale;
    }
    if let Some(locale) = decode(CURRENT.load(Ordering::Relaxed)) {
        return locale;
    }
    let locale = initial();
    // Si otro hilo lo ha resuelto o fijado entretanto, gana el suyo.
    match CURRENT.compare_exchange(0, encode(locale), Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => locale,
        Err(other) => decode(other).unwrap_or(locale),
    }
}

fn initial() -> Locale {
    if let Some(locale) = env_override() {
        return locale;
    }
    if cfg!(any(test, feature = "test-locale")) {
        return Locale::Es;
    }
    from_settings_file().unwrap_or_else(system_locale)
}

/// `language` de `~/.elephant-stack/settings.json` (lo escribe la app; ver
/// `elephant_core::settings`): `"es"` o `"en"`. `"system"`, un valor
/// desconocido o un archivo ausente o dañado dan `None`, y se lee macOS.
fn from_settings_file() -> Option<Locale> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    let path = std::path::Path::new(&home).join(".elephant-stack/settings.json");
    let text = std::fs::read_to_string(path).ok()?;
    language_from_settings(&text)
}

fn language_from_settings(text: &str) -> Option<Locale> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    match value.get("language")?.as_str()? {
        "es" => Some(Locale::Es),
        "en" => Some(Locale::En),
        _ => None,
    }
}

/// `ELEPHANT_STACK_LANG`, solo en debug, como `paths::tool_override`.
fn env_override() -> Option<Locale> {
    if !cfg!(debug_assertions) {
        return None;
    }
    match std::env::var("ELEPHANT_STACK_LANG").ok()?.trim() {
        "es" => Some(Locale::Es),
        "en" => Some(Locale::En),
        _ => None,
    }
}

/// Fija el idioma de todo el proceso: la app lo llama al arrancar y al cambiar
/// el ajuste, para que el siguiente texto ya salga en el idioma nuevo.
pub fn set_override(locale: Locale) {
    CURRENT.store(encode(locale), Ordering::Relaxed);
}

/// Ejecuta `f` con `locale` solo en este hilo y lo deshace al salir, también
/// si `f` entra en pánico. Para tests: no afecta a los que corren en paralelo.
pub fn with_locale<T>(locale: Locale, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Locale>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPED.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(SCOPED.with(|cell| cell.replace(Some(locale))));
    f()
}

/// Texto traducido como `String`: `tr!("español", "English")` o, con
/// argumentos de `format!`, `tr!("{name} se ha detenido", "{name} stopped",
/// name = svc)`. Los dos textos tienen que usar los mismos argumentos.
#[macro_export]
macro_rules! tr {
    // `format!` también sin argumentos: captura las variables del texto
    // (`tr!("{svc} se ha detenido", "{svc} stopped")`).
    ($es:literal, $en:literal $(,)?) => {{
        #[allow(clippy::useless_format)]
        let text = match $crate::locale() {
            $crate::Locale::Es => ::std::format!($es),
            $crate::Locale::En => ::std::format!($en),
        };
        text
    }};
    ($es:literal, $en:literal, $($arg:tt)+) => {
        match $crate::locale() {
            $crate::Locale::Es => ::std::format!($es, $($arg)+),
            $crate::Locale::En => ::std::format!($en, $($arg)+),
        }
    };
}

/// Como [`tr!`] sin argumentos, pero da un `&'static str`: para constantes,
/// etiquetas de `busy` o las descripciones de `clap`. Sin `format!`, así que
/// las llaves se escriben tal cual.
#[macro_export]
macro_rules! tr_str {
    ($es:literal, $en:literal $(,)?) => {
        match $crate::locale() {
            $crate::Locale::Es => $es,
            $crate::Locale::En => $en,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_that_start_with_es_are_spanish() {
        for tag in ["es", "es-ES", "es_419", "es_ES.UTF-8", "ES", "\"es-MX\""] {
            assert_eq!(Locale::from_tag(tag), Locale::Es, "{tag}");
        }
        for tag in ["en", "en-GB", "fr-FR", "eu-ES", "ca", "", "est"] {
            assert_eq!(Locale::from_tag(tag), Locale::En, "{tag}");
        }
    }

    #[test]
    fn reads_the_first_apple_language() {
        assert_eq!(parse_apple_languages("(\n    \"es-ES\"\n)\n"), Some(Locale::Es));
        assert_eq!(parse_apple_languages("(\n    en,\n    \"es-ES\"\n)\n"), Some(Locale::En));
        assert_eq!(parse_apple_languages("(\n    \"fr-FR\",\n    es\n)"), Some(Locale::En));
        assert_eq!(parse_apple_languages("(\n)\n"), None);
        assert_eq!(parse_apple_languages(""), None);
    }

    #[test]
    fn environment_follows_posix_priority() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
        };
        assert_eq!(locale_from_env(env(&[("LANG", "es_ES.UTF-8")])), Some(Locale::Es));
        assert_eq!(locale_from_env(env(&[("LANG", "es_ES.UTF-8"), ("LC_ALL", "en_US.UTF-8")])), Some(Locale::En));
        assert_eq!(locale_from_env(env(&[("LC_ALL", "C"), ("LANG", "es_ES")])), Some(Locale::Es));
        assert_eq!(locale_from_env(env(&[("LC_ALL", "C.UTF-8")])), None);
        assert_eq!(locale_from_env(env(&[])), None);
    }

    #[test]
    fn reads_the_language_of_settings_json() {
        assert_eq!(language_from_settings(r#"{"language":"en"}"#), Some(Locale::En));
        assert_eq!(language_from_settings(r#"{"language":"es","editor":"cursor"}"#), Some(Locale::Es));
        assert_eq!(language_from_settings(r#"{"language":"system"}"#), None);
        assert_eq!(language_from_settings(r#"{}"#), None);
        assert_eq!(language_from_settings("no es json"), None);
    }

    #[test]
    fn tests_write_in_spanish_unless_told_otherwise() {
        assert_eq!(locale(), Locale::Es);
        assert_eq!(tr!("Arrancar", "Start"), "Arrancar");
        let english = with_locale(Locale::En, || tr!("Arrancar", "Start"));
        assert_eq!(english, "Start");
        assert_eq!(locale(), Locale::Es);
    }

    #[test]
    fn tr_formats_both_languages() {
        let svc = String::from("mysql");
        let code = 1;
        assert_eq!(tr!("{name} se ha detenido", "{name} stopped", name = svc), "mysql se ha detenido");
        assert_eq!(
            with_locale(Locale::En, || tr!("{svc} se ha detenido (código {code})", "{svc} stopped (code {code})")),
            "mysql stopped (code 1)"
        );
        let label: &'static str = with_locale(Locale::En, || tr_str!("Reiniciando…", "Restarting…"));
        assert_eq!(label, "Restarting…");
    }

    #[test]
    fn with_locale_restores_after_a_panic() {
        let result = std::panic::catch_unwind(|| with_locale(Locale::En, || panic!("x")));
        assert!(result.is_err());
        assert_eq!(locale(), Locale::Es);
    }

    #[test]
    fn locales_serialize_as_codes() {
        for locale in Locale::ALL {
            assert_eq!(serde_json::to_value(locale).unwrap(), locale.code());
        }
    }

    #[test]
    #[ignore = "lee el idioma del Mac"]
    fn reads_the_real_system_locale() {
        println!("{:?}", system_locale());
    }
}
