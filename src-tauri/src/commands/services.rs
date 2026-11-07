//! Servicios propios (06): catálogo, instalar, actualizar, desinstalar,
//! encender o apagar, arranque al iniciar sesión e importar datos de brew.
//! Y las herramientas (phpMyAdmin, pgAdmin): instalar o actualizar,
//! desinstalar, encender su servidor y abrir. El estado llega en
//! `Snapshot.managed` y `Snapshot.managed_tools`.

use elephant_core::services::{self, ImportReport, InstallProgress, ServiceKind, ServicesCatalog, StartReport, ToolKind, UpdateReport};
use tauri::ipc::Channel;
use tauri::AppHandle;

use super::blocking;
use crate::{actions, state};

#[tauri::command]
pub async fn service_catalog(force: bool) -> Result<ServicesCatalog, String> {
    blocking(move || Ok(services::catalog(force))).await
}

fn progress(channel: &Channel<InstallProgress>) -> (impl FnMut(&str) + '_, impl FnMut(u64, u64) + '_) {
    (
        move |line: &str| {
            let _ = channel.send(InstallProgress { line: Some(line.to_string()), bytes: None });
        },
        move |done, total| {
            let _ = channel.send(InstallProgress { line: None, bytes: Some((done, total)) });
        },
    )
}

/// Descarga e instala una versión del catálogo; el progreso llega por `on_event`.
#[tauri::command]
pub async fn service_install(
    app: AppHandle,
    service: ServiceKind,
    version: String,
    on_event: Channel<InstallProgress>,
) -> Result<(), String> {
    blocking(move || {
        let (mut on_line, mut on_bytes) = progress(&on_event);
        let result = services::install(service, &version, &mut on_line, &mut on_bytes);
        state::refresh_in_background(&app);
        result
    })
    .await
}

/// Instala `to` (un parche más reciente de la rama de `from`), la enciende si
/// la rama estaba en marcha y borra `from`.
#[tauri::command]
pub async fn service_update(
    app: AppHandle,
    service: ServiceKind,
    from: String,
    to: String,
    on_event: Channel<InstallProgress>,
) -> Result<UpdateReport, String> {
    blocking(move || {
        let (mut on_line, mut on_bytes) = progress(&on_event);
        let result = services::update(service, &from, &to, &mut on_line, &mut on_bytes);
        state::refresh_in_background(&app);
        result
    })
    .await
}

#[tauri::command]
pub async fn service_uninstall(app: AppHandle, service: ServiceKind, version: String) -> Result<(), String> {
    blocking(move || actions::managed_uninstall(&app, service, &version)).await
}

/// Enciende (con la comprobación de arranque) o apaga una rama.
#[tauri::command]
pub async fn service_power(app: AppHandle, service: ServiceKind, branch: String, on: bool) -> Result<StartReport, String> {
    blocking(move || actions::managed_power(&app, service, &branch, on)).await
}

#[tauri::command]
pub async fn service_autostart(app: AppHandle, service: ServiceKind, branch: String, on: bool) -> Result<(), String> {
    blocking(move || actions::managed_autostart(&app, service, &branch, on)).await
}

/// Copia los datos de Homebrew a la rama de su misma versión.
#[tauri::command]
pub async fn service_import(app: AppHandle, service: ServiceKind, branch: String) -> Result<ImportReport, String> {
    blocking(move || actions::managed_import(&app, service, &branch)).await
}

/// Instala o actualiza una herramienta a la última versión del catálogo.
/// Devuelve la versión instalada; el progreso llega por `on_event`.
#[tauri::command]
pub async fn tool_install(app: AppHandle, tool: ToolKind, on_event: Channel<InstallProgress>) -> Result<String, String> {
    blocking(move || {
        let (mut on_line, mut on_bytes) = progress(&on_event);
        let result = services::tools::install(tool, &mut on_line, &mut on_bytes);
        state::refresh_in_background(&app);
        result
    })
    .await
}

/// Borra lo que instaló Elephant (nunca un pgAdmin que no instaló).
#[tauri::command]
pub async fn tool_uninstall(app: AppHandle, tool: ToolKind) -> Result<(), String> {
    blocking(move || actions::tool_uninstall(&app, tool)).await
}

/// Enciende o apaga el servidor de phpMyAdmin.
#[tauri::command]
pub async fn tool_power(app: AppHandle, tool: ToolKind, on: bool) -> Result<(), String> {
    blocking(move || actions::tool_power(&app, tool, on)).await
}

/// Abre phpMyAdmin en el navegador (encendiendo su servidor) o pgAdmin.
/// Devuelve la URL o la app.
#[tauri::command]
pub async fn tool_open(app: AppHandle, tool: ToolKind) -> Result<String, String> {
    blocking(move || actions::tool_open(&app, tool)).await
}
