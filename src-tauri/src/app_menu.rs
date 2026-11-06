//! Menú de la aplicación (01 §8 y 00 §5.11). Stub de P01: el menú por defecto
//! de Tauri. P05 lo sustituye por el menú en español con ids `app:*`.

use tauri::menu::Menu;
use tauri::{AppHandle, Wry};

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    Menu::default(app)
}

/// `true` si el id era de este menú (`app:*`).
pub fn handle(_app: &AppHandle, _id: &str) -> bool {
    false
}
