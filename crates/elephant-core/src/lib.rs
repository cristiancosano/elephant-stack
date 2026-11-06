//! Lógica de Elephant Stack que comparten la app, la CLI `elephant` y la
//! fachada MCP. Es síncrona y no depende de Tauri ni de tokio.
//!
//! Las mutaciones de alto nivel están en [`ops`]: toman el cerrojo de Homebrew
//! y avisan a la app con [`stamp::touch`] al terminar bien.

pub mod audit;
pub mod brew;
pub mod components;
pub mod jobs;
pub mod launchd;
pub mod lock;
pub mod mailpit;
pub mod monitor;
pub mod needs;
pub mod ops;
pub mod paths;
pub mod php;
pub mod project;
pub mod redact;
pub mod settings;
pub mod setup;
pub mod stamp;
pub mod tools;
pub mod validate;

mod fsutil;

#[cfg(test)]
mod testutil;

/// Mensaje común de las funciones que todavía no están implementadas. Quien
/// llama lo trata como «función no disponible» y oculta lo que dependa de ella.
pub const NOT_AVAILABLE: &str = "Todavía no disponible.";

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
