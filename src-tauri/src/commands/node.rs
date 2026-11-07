//! Node y Composer de Elephant (06 fase 3): catálogo, instalar, actualizar,
//! desinstalar y elegir versión. El estado llega en `Snapshot.node` y
//! `Snapshot.composer`.

use std::path::PathBuf;

use elephant_core::composer::{self, ComposerCatalog, ComposerChannel};
use elephant_core::node::{self, NodeCatalog, NodeReplaceReport};
use elephant_core::services::InstallProgress;
use tauri::AppHandle;
use tauri::ipc::Channel;

use super::blocking;
use crate::actions;

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

/// La más reciente de cada versión mayor de nodejs.org, con las destacadas
/// (LTS con soporte y Current) marcadas.
#[tauri::command]
pub async fn node_catalog(force: bool) -> Result<NodeCatalog, String> {
    blocking(move || Ok(node::catalog(force))).await
}

/// Instala lo que pide `version` (`24`, `24.11.1`, `lts`). Devuelve la versión instalada.
#[tauri::command]
pub async fn node_install(app: AppHandle, version: String, on_event: Channel<InstallProgress>) -> Result<String, String> {
    blocking(move || {
        let (mut on_line, mut on_bytes) = progress(&on_event);
        actions::node_install(&app, &version, &mut on_line, &mut on_bytes)
    })
    .await
}

/// Instala la más reciente de la mayor de `old`, mueve a ella la global y los
/// proyectos que fijaban `old`, y borra `old`.
#[tauri::command]
pub async fn node_update(app: AppHandle, old: String, on_event: Channel<InstallProgress>) -> Result<NodeReplaceReport, String> {
    blocking(move || {
        let (mut on_line, mut on_bytes) = progress(&on_event);
        actions::node_update(&app, &old, &mut on_line, &mut on_bytes)
    })
    .await
}

#[tauri::command]
pub async fn node_uninstall(app: AppHandle, version: String) -> Result<(), String> {
    blocking(move || actions::node_uninstall(&app, &version)).await
}

/// `None` vuelve a «la más reciente».
#[tauri::command]
pub async fn node_set_global(app: AppHandle, spec: Option<String>) -> Result<(), String> {
    blocking(move || actions::node_set_global(&app, spec.as_deref())).await
}

/// Escribe el `.nvmrc` (o `.node-version`) de `dir`. Devuelve el archivo.
#[tauri::command]
pub async fn node_set_project(app: AppHandle, dir: String, spec: String) -> Result<String, String> {
    blocking(move || actions::node_set_project(&app, &PathBuf::from(dir), &spec)).await
}

#[tauri::command]
pub async fn composer_catalog(force: bool) -> Result<ComposerCatalog, String> {
    blocking(move || Ok(composer::catalog(force))).await
}

/// Instala o actualiza Composer a la última del canal. Devuelve la versión.
#[tauri::command]
pub async fn composer_install(app: AppHandle, channel: ComposerChannel, on_event: Channel<InstallProgress>) -> Result<String, String> {
    blocking(move || {
        let (mut on_line, _) = progress(&on_event);
        actions::composer_install(&app, channel, &mut on_line)
    })
    .await
}

#[tauri::command]
pub async fn composer_uninstall(app: AppHandle) -> Result<(), String> {
    blocking(move || actions::composer_uninstall(&app)).await
}
