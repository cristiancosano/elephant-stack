//! Comandos del asistente (00 §5.6, C11). Stub de P01 sobre las funciones de
//! `elephant_core::setup`; P04 implementa los dos.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use elephant_core::setup::{
    self, ApplyOutcome, ApplyReport, Catalog, Detection, Plan, Profile, ProfileInput, SETUP_VERSION,
    ScenarioCard, SetupFile, SetupProgress, ShellPreview,
};
use elephant_core::{NOT_AVAILABLE, paths};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use super::blocking;
use crate::state;

#[derive(Default)]
pub struct SetupState {
    /// Último plan enseñado en la revisión: `setup_apply` solo ejecuta ese.
    pending: Mutex<Option<Plan>>,
    /// Rechaza una segunda ejecución mientras hay una en marcha.
    #[allow(dead_code)] // P04.
    running: Mutex<bool>,
    cancel: AtomicBool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct SetupStatus {
    /// No hay `setup.json`: hay que mostrar el asistente.
    pub needed: bool,
    pub state: Option<SetupFile>,
    /// Hay pasos nuevos para el escenario guardado (`SETUP_VERSION`).
    pub news: bool,
}

fn status_of(file: Option<SetupFile>) -> SetupStatus {
    SetupStatus {
        needed: file.is_none(),
        news: file.as_ref().is_some_and(|f| f.version < SETUP_VERSION),
        state: file,
    }
}

fn detect(app: &AppHandle) -> Detection {
    let launch_at_login = app.autolaunch().is_enabled().unwrap_or(false);
    setup::detect(launch_at_login)
}

#[tauri::command]
pub async fn setup_status() -> Result<SetupStatus, String> {
    blocking(|| Ok(status_of(setup::state::load()))).await
}

#[tauri::command]
pub async fn setup_detect(app: AppHandle) -> Result<Detection, String> {
    blocking(move || Ok(detect(&app))).await
}

#[tauri::command]
pub async fn setup_scenarios(app: AppHandle) -> Result<Vec<ScenarioCard>, String> {
    blocking(move || Ok(setup::scenarios(&detect(&app), &setup::catalog(false)))).await
}

/// `None` = el estado actual del sistema (C10).
#[tauri::command]
pub async fn setup_profile(app: AppHandle, input: Option<ProfileInput>) -> Result<Profile, String> {
    blocking(move || {
        let detection = detect(&app);
        match input {
            None => Ok(setup::profile_from_system(&detection)),
            Some(input) => setup::resolve(&input, &detection, &setup::catalog(false)),
        }
    })
    .await
}

/// Calcula el plan y lo guarda: es el único que `setup_apply` acepta.
#[tauri::command]
pub async fn setup_preview(app: AppHandle, input: ProfileInput) -> Result<Plan, String> {
    blocking(move || {
        let detection = detect(&app);
        let catalog = setup::catalog(false);
        let profile = setup::resolve(&input, &detection, &catalog)?;
        let plan = setup::plan(&detection, &profile, &catalog);
        let state = app.state::<SetupState>();
        *state.pending.lock().unwrap_or_else(|p| p.into_inner()) = Some(plan.clone());
        Ok(plan)
    })
    .await
}

#[tauri::command]
pub async fn setup_apply(
    plan_id: String,
    accepted: Vec<String>,
    on_event: Channel<SetupProgress>,
) -> Result<ApplyOutcome, String> {
    let _ = (plan_id, accepted, on_event);
    Err(NOT_AVAILABLE.into())
}

/// Los pasos pendientes se saltan al terminar el que está en curso.
#[tauri::command]
pub async fn setup_cancel(state: State<'_, SetupState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

/// «No cambiar nada»: guarda `scenario: null` y no ejecuta ningún comando.
#[tauri::command]
pub async fn setup_skip(app: AppHandle) -> Result<SetupFile, String> {
    blocking(move || {
        let file = SetupFile {
            version: SETUP_VERSION,
            scenario: None,
            database: None,
            custom: None,
            completed_at: Some(elephant_core::now_rfc3339()),
        };
        setup::state::save(&file)?;
        state::update(&app, |s| s.setup_needed = false);
        Ok(file)
    })
    .await
}

#[tauri::command]
pub async fn setup_shell_preview() -> Result<ShellPreview, String> {
    blocking(|| setup::shell::preview(&paths::home()?)).await
}

#[tauri::command]
pub async fn setup_shell_apply(hash: String) -> Result<(), String> {
    blocking(move || setup::shell::apply(&paths::home()?, &hash)).await
}

#[tauri::command]
pub async fn php_catalog() -> Result<Catalog, String> {
    blocking(|| Ok(setup::catalog(false))).await
}

#[tauri::command]
pub async fn php_download(version: String, on_event: Channel<SetupProgress>) -> Result<ApplyReport, String> {
    blocking(move || {
        setup::download_php(&version, &|event| {
            let _ = on_event.send(event);
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_setup_json() {
        assert!(status_of(None).needed);
        let old = SetupFile {
            version: 0,
            scenario: None,
            database: None,
            custom: None,
            completed_at: None,
        };
        let status = status_of(Some(old));
        assert!(!status.needed);
        assert!(status.news);
    }
}
