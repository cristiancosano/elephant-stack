//! Comandos comunes: estado, servicios, PHP, ajustes y acciones de sistema.

use std::path::PathBuf;

use elephant_core::brew::Operation;
use elephant_core::monitor::{self, Usage};
use elephant_core::settings::Settings;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, Window};

use super::blocking;
use crate::actions::{self, SiteAction};
use crate::appearance::{self, Appearance};
use crate::state::{self, AppState, Snapshot};
use crate::tray;

/// Con un snapshot de más de 2 s se refresca en segundo plano; el resultado
/// llega por `state:changed`.
const STALE_MS: u64 = 2_000;

/// Al instante, con lo que haya.
#[tauri::command]
pub async fn get_snapshot(app: AppHandle, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let snapshot = state.snapshot();
    let now = u64::try_from(elephant_core::now_ms()).unwrap_or_default();
    if now.saturating_sub(snapshot.refreshed_at_ms) > STALE_MS {
        state::refresh_in_background(&app);
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn refresh(app: AppHandle) -> Result<(), String> {
    blocking(move || state::refresh(&app).map(drop)).await
}

#[tauri::command]
pub async fn service_operation(app: AppHandle, name: String, operation: Operation) -> Result<(), String> {
    blocking(move || actions::service_operation(&app, &name, operation)).await
}

#[tauri::command]
pub async fn services_bulk(app: AppHandle, operation: Operation) -> Result<(), String> {
    blocking(move || actions::services_bulk(&app, operation)).await
}

/// Las líneas de brew llegan por `on_line` según se producen.
#[tauri::command]
pub async fn install_component(app: AppHandle, id: String, on_line: Channel<String>) -> Result<(), String> {
    blocking(move || {
        actions::install_component(&app, &id, &mut |line| {
            let _ = on_line.send(line.to_string());
        })
    })
    .await
}

#[tauri::command]
pub async fn set_global_php(app: AppHandle, key: String) -> Result<(), String> {
    blocking(move || actions::set_global_php(&app, &key)).await
}

#[tauri::command]
pub async fn set_project_php(app: AppHandle, dir: String, key: String) -> Result<(), String> {
    blocking(move || actions::set_project_php(&app, &PathBuf::from(dir), &key)).await
}

/// También guarda el valor para la cabecera del menú de la barra.
#[tauri::command]
pub async fn process_usage(app: AppHandle, pids: Vec<i64>) -> Result<Vec<Usage>, String> {
    blocking(move || {
        let usage = monitor::usage_for(&pids)?;
        let state = app.state::<AppState>();
        state.set_usage(&usage);
        tray::set_usage(&app, state.usage_kb());
        Ok(usage)
    })
    .await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    blocking(move || actions::set_settings(&app, settings)).await
}

/// Ruta pedida por `nav:go` antes de que existiera la ventana. Se consume una vez.
#[tauri::command]
pub async fn take_pending_route(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.take_pending_route())
}

#[tauri::command]
pub async fn appearance() -> Result<Appearance, String> {
    Ok(appearance::current())
}

/// Respaldo del evento de foco: la interfaz avisa cuando deja de verse.
#[tauri::command]
pub async fn close_if_minimized(window: Window) -> Result<(), String> {
    let keep = window.app_handle().state::<AppState>().settings().keep_in_menu_bar;
    if keep && window.is_minimized().unwrap_or(false) {
        window.close().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_in_editor(app: AppHandle, file: String, line: Option<u32>) -> Result<(), String> {
    blocking(move || actions::open_in_editor(&app, &PathBuf::from(file), line)).await
}

#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    blocking(move || actions::open_url(&url)).await
}

#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    blocking(move || actions::reveal_in_finder(&PathBuf::from(path))).await
}

#[tauri::command]
pub async fn open_terminal(dir: String) -> Result<(), String> {
    blocking(move || actions::open_terminal(&PathBuf::from(dir))).await
}

#[tauri::command]
pub async fn site_action(app: AppHandle, project_id: String, action: SiteAction) -> Result<(), String> {
    blocking(move || actions::site(&app, &project_id, action)).await
}

#[tauri::command]
pub async fn open_service_log(name: String) -> Result<(), String> {
    blocking(move || actions::open_service_log(&name)).await
}
