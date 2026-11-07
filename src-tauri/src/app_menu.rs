//! Menú de la aplicación (01 §8 y 00 §5.11), con ids `app:*` y los textos en
//! el idioma de `i18n::locale()`. Al cambiar el idioma, [`rebuild`] lo
//! vuelve a construir entero.
//!
//! Solo se ve con la ventana abierta. Los elementos de navegación emiten
//! `nav:go` y los de la vista `ui:command`, así que los atajos funcionan
//! aunque el foco esté en un campo de texto, sin escuchar el teclado en JS.

use std::fmt;
use std::str::FromStr;

use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use elephant_core::tr;
use tauri::{AppHandle, Emitter, Wry};

use crate::state::UiCommand;
use crate::window;

/// Lo que hace cada elemento propio del menú. `Display` da su id.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AppAction {
    /// `app:nav:#/panel`
    Nav(String),
    /// `app:ui:toggle-sidebar`
    Ui(UiCommand),
    /// `app:quit`
    Quit,
}

fn ui_slug(command: UiCommand) -> &'static str {
    match command {
        UiCommand::Find => "find",
        UiCommand::Refresh => "refresh",
        UiCommand::ToggleSidebar => "toggle-sidebar",
        UiCommand::ToggleInspector => "toggle-inspector",
        UiCommand::Live => "live",
        UiCommand::ClearView => "clear-view",
    }
}

fn parse_ui(slug: &str) -> Option<UiCommand> {
    Some(match slug {
        "find" => UiCommand::Find,
        "refresh" => UiCommand::Refresh,
        "toggle-sidebar" => UiCommand::ToggleSidebar,
        "toggle-inspector" => UiCommand::ToggleInspector,
        "live" => UiCommand::Live,
        "clear-view" => UiCommand::ClearView,
        _ => return None,
    })
}

impl fmt::Display for AppAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nav(route) => write!(f, "app:nav:{route}"),
            Self::Ui(command) => write!(f, "app:ui:{}", ui_slug(*command)),
            Self::Quit => f.write_str("app:quit"),
        }
    }
}

impl FromStr for AppAction {
    type Err = ();

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let rest = id.strip_prefix("app:").ok_or(())?;
        if rest == "quit" {
            return Ok(Self::Quit);
        }
        if let Some(route) = rest.strip_prefix("nav:") {
            return if route.is_empty() { Err(()) } else { Ok(Self::Nav(route.to_string())) };
        }
        if let Some(command) = rest.strip_prefix("ui:") {
            return parse_ui(command).map(Self::Ui).ok_or(());
        }
        Err(())
    }
}

fn item(app: &AppHandle, action: AppAction, text: &str, accel: Option<&str>) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, action.to_string(), text, true, accel)
}

fn nav(app: &AppHandle, route: &str, text: &str, accel: Option<&str>) -> tauri::Result<MenuItem<Wry>> {
    item(app, AppAction::Nav(route.to_string()), text, accel)
}

fn ui(app: &AppHandle, command: UiCommand, text: &str, accel: &str) -> tauri::Result<MenuItem<Wry>> {
    item(app, AppAction::Ui(command), text, Some(accel))
}

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let separator = || PredefinedMenuItem::separator(app);
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        authors: Some(vec!["Cristian Cosano".into()]),
        ..AboutMetadata::default()
    };

    let elephant = Submenu::with_items(
        app,
        "Elephant Stack",
        true,
        &[
            &PredefinedMenuItem::about(app, Some(&tr!("Acerca de Elephant Stack", "About Elephant Stack")), Some(about))?,
            &separator()?,
            &nav(app, "#/ajustes/general", &tr!("Ajustes…", "Settings…"), Some("CmdOrCtrl+,"))?,
            &nav(app, "#/asistente", &tr!("Asistente de configuración…", "Setup assistant…"), None)?,
            &separator()?,
            &PredefinedMenuItem::services(app, Some(&tr!("Servicios", "Services")))?,
            &separator()?,
            &PredefinedMenuItem::hide(app, Some(&tr!("Ocultar Elephant Stack", "Hide Elephant Stack")))?,
            &PredefinedMenuItem::hide_others(app, Some(&tr!("Ocultar otros", "Hide others")))?,
            &PredefinedMenuItem::show_all(app, Some(&tr!("Mostrar todo", "Show all")))?,
            &separator()?,
            // Propio y no el predefinido: «Salir» siempre cierra la app, aunque
            // esté activado «Mantener en la barra de menús» (lib.rs).
            &item(app, AppAction::Quit, &tr!("Salir de Elephant Stack", "Quit Elephant Stack"), Some("CmdOrCtrl+Q"))?,
        ],
    )?;

    let edit = Submenu::with_items(
        app,
        &tr!("Edición", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&tr!("Deshacer", "Undo")))?,
            &PredefinedMenuItem::redo(app, Some(&tr!("Rehacer", "Redo")))?,
            &separator()?,
            &PredefinedMenuItem::cut(app, Some(&tr!("Cortar", "Cut")))?,
            &PredefinedMenuItem::copy(app, Some(&tr!("Copiar", "Copy")))?,
            &PredefinedMenuItem::paste(app, Some(&tr!("Pegar", "Paste")))?,
            &PredefinedMenuItem::select_all(app, Some(&tr!("Seleccionar todo", "Select all")))?,
            &separator()?,
            &ui(app, UiCommand::Find, &tr!("Buscar", "Find"), "CmdOrCtrl+F")?,
        ],
    )?;

    let view = Submenu::with_items(
        app,
        &tr!("Visualización", "View"),
        true,
        &[
            &nav(app, "#/panel", &tr!("Panel", "Overview"), Some("CmdOrCtrl+1"))?,
            &nav(app, "#/sitios", &tr!("Sitios", "Sites"), Some("CmdOrCtrl+2"))?,
            &nav(app, "#/servicios", &tr!("Servicios", "Services"), Some("CmdOrCtrl+3"))?,
            &nav(app, "#/php", "PHP", Some("CmdOrCtrl+4"))?,
            &nav(app, "#/node", "Node", Some("CmdOrCtrl+5"))?,
            &nav(app, "#/logs", "Logs", Some("CmdOrCtrl+6"))?,
            &separator()?,
            &ui(app, UiCommand::Live, &tr!("En vivo", "Live"), "CmdOrCtrl+L")?,
            // No borra nada, como ⌘K en Terminal (C20).
            &ui(app, UiCommand::ClearView, &tr!("Limpiar vista", "Clear view"), "CmdOrCtrl+K")?,
            &separator()?,
            &ui(
                app,
                UiCommand::ToggleSidebar,
                &tr!("Mostrar u ocultar la barra lateral", "Toggle sidebar"),
                "Cmd+Ctrl+S",
            )?,
            &ui(
                app,
                UiCommand::ToggleInspector,
                &tr!("Mostrar u ocultar el inspector", "Toggle inspector"),
                "Cmd+Alt+I",
            )?,
            &separator()?,
            &ui(app, UiCommand::Refresh, &tr!("Actualizar", "Refresh"), "CmdOrCtrl+R")?,
            &separator()?,
            &PredefinedMenuItem::fullscreen(app, Some(&tr!("Entrar en pantalla completa", "Enter full screen")))?,
        ],
    )?;

    let window_menu = Submenu::with_items(
        app,
        &tr!("Ventana", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&tr!("Minimizar", "Minimize")))?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
            &separator()?,
            &PredefinedMenuItem::close_window(app, Some(&tr!("Cerrar", "Close")))?,
        ],
    )?;
    #[cfg(target_os = "macos")]
    window_menu.set_as_windows_menu_for_nsapp()?;

    Menu::with_items(app, &[&elephant, &edit, &view, &window_menu])
}

/// Vuelve a construir el menú en el idioma actual (al cambiar el ajuste). En
/// el hilo principal, como todo lo que toca AppKit.
pub fn rebuild(app: &AppHandle) {
    let handle = app.clone();
    let scheduled = app.run_on_main_thread(move || match build(&handle) {
        Ok(menu) => {
            if let Err(error) = handle.set_menu(menu) {
                eprintln!("No se pudo poner el menú de la aplicación: {error}");
            }
        }
        Err(error) => eprintln!("No se pudo construir el menú de la aplicación: {error}"),
    });
    if let Err(error) = scheduled {
        eprintln!("No se pudo construir el menú de la aplicación: {error}");
    }
}

/// `true` si el id era de este menú (`app:*`).
pub fn handle(app: &AppHandle, id: &str) -> bool {
    if !id.starts_with("app:") {
        return false;
    }
    match id.parse::<AppAction>() {
        Ok(AppAction::Nav(route)) => window::open_main_window(app, Some(&route)),
        Ok(AppAction::Ui(command)) => {
            let _ = app.emit_to(window::MAIN, "ui:command", command);
        }
        Ok(AppAction::Quit) => app.exit(0),
        Err(()) => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMANDS: [UiCommand; 6] = [
        UiCommand::Find,
        UiCommand::Refresh,
        UiCommand::ToggleSidebar,
        UiCommand::ToggleInspector,
        UiCommand::Live,
        UiCommand::ClearView,
    ];

    #[test]
    fn every_action_survives_a_round_trip() {
        let mut actions = vec![
            AppAction::Quit,
            AppAction::Nav("#/panel".into()),
            AppAction::Nav("#/logs?level=error".into()),
        ];
        actions.extend(COMMANDS.map(AppAction::Ui));
        for action in actions {
            let id = action.to_string();
            assert!(id.starts_with("app:"));
            assert_eq!(id.parse::<AppAction>(), Ok(action), "{id}");
        }
    }

    #[test]
    fn ui_slugs_match_the_event_payload() {
        for command in COMMANDS {
            assert_eq!(serde_json::to_value(command).unwrap(), ui_slug(command));
        }
        assert_eq!(AppAction::Ui(UiCommand::ClearView).to_string(), "app:ui:clear-view");
        assert_eq!(AppAction::Ui(UiCommand::Live).to_string(), "app:ui:live");
    }

    #[test]
    fn foreign_ids_are_rejected() {
        for id in ["nav:#/panel", "app:", "app:nav:", "app:ui:zoom", "app:otra", "quit"] {
            assert!(id.parse::<AppAction>().is_err(), "{id}");
        }
    }
}
