//! Acento y apariencia del sistema (01 §5.3). Stub de P01: devuelve el azul
//! por defecto de macOS. P05 lee `NSColor.controlAccentColor` con objc2.

use elephant_core::settings::AppearanceMode;
use serde::Serialize;
use tauri::{AppHandle, WebviewWindow};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Appearance {
    /// `#rrggbb`.
    pub accent: String,
    /// Texto sobre el acento, `#rrggbb`.
    pub accent_text: String,
}

pub fn current() -> Appearance {
    Appearance {
        accent: "#007aff".into(),
        accent_text: "#ffffff".into(),
    }
}

/// Se ejecuta antes de pintar: fija el acento y el tema sin parpadeo.
pub fn init_script(a: &Appearance, mode: AppearanceMode) -> String {
    let theme = match mode {
        AppearanceMode::System => "null",
        AppearanceMode::Light => "\"light\"",
        AppearanceMode::Dark => "\"dark\"",
    };
    // Los valores van como literales JSON: nunca se interpola texto suelto.
    let accent = serde_json::to_string(&a.accent).unwrap_or_else(|_| "\"#007aff\"".into());
    let accent_text = serde_json::to_string(&a.accent_text).unwrap_or_else(|_| "\"#ffffff\"".into());
    format!(
        "(() => {{ const root = document.documentElement; \
         root.style.setProperty('--system-accent', {accent}); \
         root.style.setProperty('--system-accent-text', {accent_text}); \
         const theme = {theme}; if (theme) root.dataset.theme = theme; }})();"
    )
}

/// P05: `set_effects(sidebar)` y, si falla, la clase `no-vibrancy`.
pub fn apply_to_window(_window: &WebviewWindow) {}

/// P05: emite `appearance:changed` si el acento ha cambiado.
pub fn on_focus(_app: &AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_init_script_sets_accent_and_theme() {
        let script = init_script(&current(), AppearanceMode::Dark);
        assert!(script.contains("'--system-accent', \"#007aff\""));
        assert!(script.contains("const theme = \"dark\""));
        let script = init_script(&current(), AppearanceMode::System);
        assert!(script.contains("const theme = null"));
    }
}
