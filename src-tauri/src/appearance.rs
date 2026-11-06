//! Acento y apariencia del sistema (01 §5.3 y §5.2).
//!
//! El acento sale de `NSColor.controlAccentColor` convertido a sRGB. El código
//! de AppKit está aislado en el módulo `macos`; si falla o no hay AppKit, se
//! usa el azul por defecto de macOS. No se observa ninguna notificación: al
//! recuperar el foco se vuelve a leer y, si ha cambiado, se emite
//! `appearance:changed`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use elephant_core::settings::AppearanceMode;
use serde::Serialize;
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Emitter, WebviewWindow};

use crate::window;

const DEFAULT_ACCENT: (f64, f64, f64) = (0.0, 122.0 / 255.0, 1.0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Appearance {
    /// `#rrggbb`.
    pub accent: String,
    /// Texto sobre el acento, `#rrggbb`.
    pub accent_text: String,
}

/// El último acento enviado a la ventana.
static LAST: Mutex<Option<Appearance>> = Mutex::new(None);
/// `set_effects` falló una vez: las ventanas siguientes nacen con `no-vibrancy`.
static NO_VIBRANCY: AtomicBool = AtomicBool::new(false);

pub fn current() -> Appearance {
    #[cfg(target_os = "macos")]
    let rgb = macos::accent().unwrap_or(DEFAULT_ACCENT);
    #[cfg(not(target_os = "macos"))]
    let rgb = DEFAULT_ACCENT;
    from_rgb(rgb)
}

/// Componentes de 0 a 1 en sRGB.
fn from_rgb((r, g, b): (f64, f64, f64)) -> Appearance {
    Appearance {
        accent: hex(r, g, b),
        accent_text: text_on(r, g, b).to_string(),
    }
}

fn channel(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn hex(r: f64, g: f64, b: f64) -> String {
    format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
}

/// Luminancia relativa de WCAG 2.
fn luminance(r: f64, g: f64, b: f64) -> f64 {
    let linear = |c: f64| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Blanco, como hace macOS sobre cualquier acento, salvo que el acento sea
/// tan claro (el amarillo) que el blanco apenas contraste.
fn text_on(r: f64, g: f64, b: f64) -> &'static str {
    let with_white = 1.05 / (luminance(r, g, b) + 0.05);
    if with_white >= 2.0 { "#ffffff" } else { "#000000" }
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
    let no_vibrancy = NO_VIBRANCY.load(Ordering::Relaxed);
    if let Ok(mut last) = LAST.lock() {
        *last = Some(a.clone());
    }
    format!(
        "(() => {{ const root = document.documentElement; \
         root.style.setProperty('--system-accent', {accent}); \
         root.style.setProperty('--system-accent-text', {accent_text}); \
         const theme = {theme}; if (theme) root.dataset.theme = theme; \
         if ({no_vibrancy}) root.classList.add('no-vibrancy'); }})();"
    )
}

/// El material `sidebar` detrás de la barra lateral. Si no se puede poner, la
/// interfaz usa `--sidebar-fallback` con la clase `no-vibrancy`.
pub fn apply_to_window(window: &WebviewWindow) {
    let effects = EffectsBuilder::new()
        .effect(Effect::Sidebar)
        .state(EffectState::FollowsWindowActiveState)
        .build();
    if let Err(error) = window.set_effects(effects) {
        eprintln!("Sin vibrancy en la ventana: {error}");
        NO_VIBRANCY.store(true, Ordering::Relaxed);
        let _ = window.eval("document.documentElement.classList.add('no-vibrancy')");
    }
}

/// Al recuperar el foco: emite `appearance:changed` si el acento ha cambiado.
pub fn on_focus(app: &AppHandle) {
    let now = current();
    let changed = match LAST.lock() {
        Ok(mut last) if last.as_ref() != Some(&now) => {
            *last = Some(now.clone());
            true
        }
        _ => false,
    };
    if changed {
        let _ = app.emit_to(window::MAIN, "appearance:changed", &now);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_app_kit::{NSColor, NSColorSpace};

    /// `controlAccentColor` en sRGB. Es un color dinámico: hay que convertirlo
    /// antes de leer sus componentes, o AppKit lanza una excepción.
    pub fn accent() -> Option<(f64, f64, f64)> {
        let color = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        let rgb = (color.redComponent(), color.greenComponent(), color.blueComponent());
        let valid = [rgb.0, rgb.1, rgb.2].iter().all(|c| c.is_finite());
        valid.then_some(rgb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_init_script_sets_accent_and_theme() {
        let blue = from_rgb(DEFAULT_ACCENT);
        let script = init_script(&blue, AppearanceMode::Dark);
        assert!(script.contains("'--system-accent', \"#007aff\""));
        assert!(script.contains("'--system-accent-text', \"#ffffff\""));
        assert!(script.contains("const theme = \"dark\""));
        let script = init_script(&blue, AppearanceMode::System);
        assert!(script.contains("const theme = null"));
        assert!(script.contains("if (false) root.classList.add('no-vibrancy')"));
    }

    #[test]
    fn colors_become_hex() {
        assert_eq!(hex(0.0, 122.0 / 255.0, 1.0), "#007aff");
        assert_eq!(hex(1.2, -0.1, 0.5), "#ff0080");
    }

    #[test]
    fn text_on_the_accent_is_white_unless_the_accent_is_very_light() {
        // Azul, morado, rosa, rojo, naranja, verde y grafito de macOS.
        for (r, g, b) in [
            (0.0, 0.478, 1.0),
            (0.584, 0.239, 0.588),
            (0.969, 0.31, 0.62),
            (0.878, 0.22, 0.243),
            (0.969, 0.51, 0.106),
            (0.384, 0.729, 0.275),
            (0.596, 0.596, 0.596),
        ] {
            assert_eq!(text_on(r, g, b), "#ffffff", "({r}, {g}, {b})");
        }
        // Amarillo.
        assert_eq!(text_on(1.0, 0.776, 0.0), "#000000");
    }

    /// Lee el acento real de macOS sin cambiar nada.
    #[cfg(target_os = "macos")]
    #[test]
    fn reads_the_system_accent() {
        let a = current();
        assert_eq!(a.accent.len(), 7);
        assert!(a.accent.starts_with('#'));
    }
}
