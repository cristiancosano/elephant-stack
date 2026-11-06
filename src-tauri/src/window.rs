//! La ventana única. Se destruye al cerrarla para liberar WebKit, así que se
//! crea cada vez que hace falta (`"create": false` en `tauri.conf.json`, C24).

use elephant_core::settings::AppearanceMode;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewWindowBuilder, Window, WindowEvent};

use crate::appearance;
use crate::commands;
use crate::state::{AppState, NavGo};

pub const MAIN: &str = "main";

fn keep_in_menu_bar(app: &AppHandle) -> bool {
    app.state::<AppState>().settings().keep_in_menu_bar
}

/// Si la ventana existe, la trae delante y emite `nav:go` (o, si su interfaz
/// aún está cargando, le deja la ruta pendiente). Si no, guarda la ruta (la
/// ventana la pide con `take_pending_route` al montar) y la crea.
pub fn open_main_window(app: &AppHandle, route: Option<&str>) {
    show_in_dock(app, true);
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let state = app.state::<AppState>();
        if let Some(route) = state.route_for_open_window(route.map(str::to_string)) {
            let _ = app.emit_to(MAIN, "nav:go", NavGo { route });
        }
        return;
    }

    let state = app.state::<AppState>();
    state.set_pending_route(route.map(str::to_string));
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == MAIN) else {
        eprintln!("Falta la ventana «{MAIN}» en tauri.conf.json");
        return;
    };
    let mode = state.settings().appearance;
    let built = WebviewWindowBuilder::from_config(app, config).and_then(|builder| {
        builder
            .initialization_script(appearance::init_script(&appearance::current(), mode))
            .theme(theme_for(mode))
            .build()
    });
    match built {
        Ok(window) => {
            appearance::apply_to_window(&window);
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("No se pudo abrir la ventana: {error}"),
    }
}

/// Con la ventana abierta la app sale en el Dock y en Cmd+Tab; sin ella, solo
/// en la barra de menús.
pub fn show_in_dock(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if visible {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    let app = window.app_handle();
    match event {
        // Minimizar deja la ventana en el Dock, que es justo lo que se quiere
        // evitar. macOS no siempre avisa con Resized al minimizar, pero sí
        // quita el foco; se comprueba al terminar la animación.
        WindowEvent::Resized(_) | WindowEvent::Focused(false) if keep_in_menu_bar(app) => {
            if matches!(event, WindowEvent::Focused(false)) {
                set_inactive(app, true);
            }
            let window = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(600));
                if window.is_minimized().unwrap_or(false) {
                    let _ = window.close();
                }
            });
        }
        WindowEvent::Focused(focused) => {
            set_inactive(app, !focused);
            if *focused {
                appearance::on_focus(app);
            }
        }
        WindowEvent::Destroyed => {
            app.state::<AppState>().window_destroyed();
            commands::logs::release(app);
            if keep_in_menu_bar(app) {
                show_in_dock(app, false);
            }
        }
        _ => {}
    }
}

/// `data-window-inactive` en `<html>`: la interfaz atenúa la selección y el
/// acento, como AppKit.
fn set_inactive(app: &AppHandle, inactive: bool) {
    if let Some(webview) = app.get_webview_window(MAIN) {
        let _ = webview.eval(format!(
            "document.documentElement.toggleAttribute('data-window-inactive', {inactive})"
        ));
    }
}

/// `set_theme` para que el material nativo coincida y `data-theme` para los
/// tokens. «Sistema» deja los dos sin forzar.
pub fn apply_theme(app: &AppHandle, mode: AppearanceMode) {
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    let _ = window.set_theme(theme_for(mode));
    let _ = window.eval(theme_script(mode));
}

fn theme_for(mode: AppearanceMode) -> Option<Theme> {
    match mode {
        AppearanceMode::System => None,
        AppearanceMode::Light => Some(Theme::Light),
        AppearanceMode::Dark => Some(Theme::Dark),
    }
}

fn theme_script(mode: AppearanceMode) -> &'static str {
    match mode {
        AppearanceMode::System => "delete document.documentElement.dataset.theme",
        AppearanceMode::Light => "document.documentElement.dataset.theme = 'light'",
        AppearanceMode::Dark => "document.documentElement.dataset.theme = 'dark'",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_mode_forces_nothing() {
        assert_eq!(theme_for(AppearanceMode::System), None);
        assert_eq!(theme_for(AppearanceMode::Dark), Some(Theme::Dark));
        assert!(theme_script(AppearanceMode::System).starts_with("delete"));
        assert!(theme_script(AppearanceMode::Light).ends_with("'light'"));
    }
}
