//! Vigilancia de servicios y de cambios hechos fuera de la app. Stub de P01.
//! P05: `launchctl list` cada 20 s (`elephant_core::launchd`) si
//! `settings.watch_services`, y `notify` sobre `state.stamp` y `jobs/`.

use tauri::AppHandle;

pub fn start(_app: &AppHandle) {}

/// Un tick inmediato, por ejemplo al abrir el menú de la barra.
#[allow(dead_code)] // Lo llama el tray de P05.
pub fn tick_now(_app: &AppHandle) {}
