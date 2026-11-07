//! Lógica de Elephant Stack que comparten la app, la CLI `elephant` y la
//! fachada MCP. Es síncrona y no depende de Tauri ni de tokio.
//!
//! Las mutaciones de alto nivel están en [`ops`]: toman el cerrojo de Homebrew
//! y avisan a la app con [`stamp::touch`] al terminar bien.

pub mod audit;
pub mod brew;
pub mod components;
pub mod composer;
pub mod i18n;
pub mod jobs;
pub mod launchd;
pub mod lock;
pub mod mailpit;
pub mod monitor;
pub mod needs;
pub mod node;
pub mod ops;
pub mod paths;
pub mod php;
pub mod project;
pub mod redact;
pub mod selectors;
pub mod services;
pub mod settings;
pub mod setup;
pub mod stamp;
pub mod tools;
pub mod validate;

mod fsutil;

pub use elephant_i18n::{tr, tr_str};

#[cfg(test)]
mod testutil;

/// Mensaje común de las funciones que todavía no están implementadas. Quien
/// llama lo trata como «función no disponible» y oculta lo que dependa de ella.
/// Es un centinela que se compara (también en la interfaz): no se traduce.
pub const NOT_AVAILABLE: &str = "Todavía no disponible.";

/// Error de lo que solo se puede hacer con permisos de administrador (código
/// de salida 5 de la CLI). Hoy no lo devuelve ninguna función de core; quien
/// lo devuelva tiene que usar esta función, y quien lo clasifique,
/// [`is_needs_admin`], que lo reconoce en cualquier idioma.
pub fn needs_admin_error() -> String {
    tr!(
        "Esta acción necesita permisos de administrador: hazla desde Elephant Stack.",
        "This needs administrator privileges: do it from Elephant Stack."
    )
}

/// `true` si `message` viene de [`needs_admin_error`], en el idioma que sea
/// (la CLI puede leer un error escrito por otro proceso con otro idioma).
pub fn is_needs_admin(message: &str) -> bool {
    i18n::Locale::ALL
        .into_iter()
        .any(|locale| message.contains(&i18n::with_locale(locale, needs_admin_error)))
        || ["permisos de administrador", "administrator privileges"]
            .iter()
            .any(|marker| message.contains(marker))
}

/// Milisegundos desde 1970 en UTC.
pub fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// Ahora en RFC 3339 y UTC, al segundo: `2026-11-06T10:20:00Z`.
pub fn now_rfc3339() -> String {
    let now = jiff::Timestamp::now();
    jiff::Timestamp::from_second(now.as_second())
        .unwrap_or(now)
        .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn rfc3339_has_no_fraction() {
        let now = super::now_rfc3339();
        assert!(now.ends_with('Z'), "{now}");
        assert_eq!(now.len(), "2026-11-06T10:20:00Z".len(), "{now}");
    }
}
