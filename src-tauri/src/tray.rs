//! Icono y menú de la barra de menús. Stub de P01: el menú de siempre (Abrir y
//! Salir). P05 construye el menú de 00 §5.10 a partir del `Snapshot`.
//!
//! Los eventos de menú no se escuchan aquí: los despacha `lib.rs` (C25).

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

use crate::state::Snapshot;
use crate::window;

pub const TRAY_ID: &str = "main";

const OPEN: &str = "open";
const QUIT: &str = "quit";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Abrir Elephant Stack", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Salir de Elephant Stack", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("Elephant Stack")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .build(app)?;
    Ok(())
}

/// Lo llama solo `state::update`.
pub fn refresh(_app: &AppHandle, _snapshot: &Snapshot, _usage_kb: Option<u64>) {}

/// Solo cambia el texto de la cabecera (memoria de los servicios).
pub fn set_usage(_app: &AppHandle, _usage_kb: Option<u64>) {}

/// `true` si el id era de este menú.
pub fn handle(app: &AppHandle, id: &str) -> bool {
    match id {
        OPEN => window::open_main_window(app, None),
        QUIT => app.exit(0),
        _ => return false,
    }
    true
}
