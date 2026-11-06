//! Comandos del asistente (00 §5.6, C11): envoltorios finos sobre
//! `elephant_core::setup`. El frontend solo envía el escenario elegido y las
//! claves de los pasos opcionales que acepta; el plan lo calcula siempre Rust.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use elephant_core::paths;
use elephant_core::setup::{
    self, ApplyContext, ApplyOutcome, ApplyReport, Catalog, Detection, Plan, Profile, ProfileInput,
    SETUP_VERSION, ScenarioCard, ScenarioId, SetupFile, SetupProgress, ShellPreview,
};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use super::blocking;
use crate::{sidecar, state};

#[derive(Default)]
pub struct SetupState {
    /// Último plan enseñado en la revisión y lo que lo produjo: `setup_apply`
    /// solo ejecuta ese, recalculado.
    pending: Mutex<Option<(ProfileInput, Plan)>>,
    /// Rechaza una segunda ejecución mientras hay una en marcha.
    running: Mutex<bool>,
    cancel: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Marca la ejecución en curso y la libera al soltarse, también si hay un pánico.
struct Running<'a>(&'a Mutex<bool>);

impl<'a> Running<'a> {
    fn start(flag: &'a Mutex<bool>) -> Result<Self, String> {
        let mut running = lock(flag);
        if *running {
            return Err("Ya se está aplicando un plan. Espera a que termine.".into());
        }
        *running = true;
        Ok(Self(flag))
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        *lock(self.0) = false;
    }
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

/// Lo que se guarda en `setup.json` al terminar de aplicar.
fn setup_file_for(input: &ProfileInput, completed_at: String) -> SetupFile {
    let (scenario, database, custom) = match input {
        ProfileInput::Laravel => (ScenarioId::Laravel, None, None),
        ProfileInput::LaravelServices { database } => {
            (ScenarioId::LaravelServices, Some(*database), None)
        }
        ProfileInput::Terminal => (ScenarioId::Terminal, None, None),
        ProfileInput::Custom { profile } => (ScenarioId::Custom, None, Some(profile.clone())),
    };
    SetupFile {
        version: SETUP_VERSION,
        scenario: Some(scenario),
        database,
        custom,
        completed_at: Some(completed_at),
    }
}

fn detect(app: &AppHandle) -> Detection {
    let launch_at_login = app.autolaunch().is_enabled().unwrap_or(false);
    setup::detect(launch_at_login)
}

/// detect + catalog + resolve + plan, con la RAM real de lo que está en marcha.
fn plan_for(app: &AppHandle, input: &ProfileInput) -> Result<Plan, String> {
    let detection = detect(app);
    let catalog = setup::catalog(false);
    let profile = setup::resolve(input, &detection, &catalog)?;
    Ok(setup::plan_measured(&detection, &profile, &catalog))
}

/// Tras cambiar algo: la app, el tray y la interfaz leen el estado nuevo.
fn refresh(app: &AppHandle) {
    if state::refresh(app).is_err() {
        state::refresh_in_background(app);
    }
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
        let plan = plan_for(&app, &input)?;
        *lock(&app.state::<SetupState>().pending) = Some((input, plan.clone()));
        Ok(plan)
    })
    .await
}

/// Vuelve a detectar y a planificar. Si el plan ya no es el revisado, no
/// ejecuta nada y devuelve el nuevo (`Stale`) para revisarlo otra vez.
#[tauri::command]
pub async fn setup_apply(
    app: AppHandle,
    plan_id: String,
    accepted: Vec<String>,
    on_event: Channel<SetupProgress>,
) -> Result<ApplyOutcome, String> {
    blocking(move || {
        let state = app.state::<SetupState>();
        let _running = Running::start(&state.running)?;
        let input = lock(&state.pending)
            .as_ref()
            .filter(|(_, plan)| plan.id == plan_id)
            .map(|(input, _)| input.clone())
            .ok_or("Este plan ya no es el último que has revisado. Vuelve a revisarlo.")?;
        state.cancel.store(false, Ordering::Relaxed);

        let fresh = plan_for(&app, &input)?;
        let shim_source = sidecar::path("php-selector").unwrap_or_default();
        let launch_at_login = |enabled: bool| -> Result<(), String> {
            let autolaunch = app.autolaunch();
            let result = if enabled {
                autolaunch.enable()
            } else {
                autolaunch.disable()
            };
            result.map_err(|error| format!("No se pudo cambiar el inicio de sesión: {error}"))
        };
        let ctx = ApplyContext {
            shim_source,
            launch_at_login: &launch_at_login,
        };
        let accepted: BTreeSet<String> = accepted.into_iter().collect();
        let outcome =
            setup::apply_if_current(&plan_id, fresh, &accepted, &ctx, &state.cancel, &|event| {
                let _ = on_event.send(event);
            });

        match &outcome {
            ApplyOutcome::Stale { plan } => {
                *lock(&state.pending) = Some((input, plan.clone()));
            }
            ApplyOutcome::Done { .. } => {
                *lock(&state.pending) = None;
                // Cancelar a medias no da el asistente por terminado.
                if !state.cancel.load(Ordering::Relaxed) {
                    setup::state::save(&setup_file_for(&input, elephant_core::now_rfc3339()))?;
                }
                refresh(&app);
            }
        }
        Ok(outcome)
    })
    .await
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
pub async fn php_download(
    app: AppHandle,
    version: String,
    on_event: Channel<SetupProgress>,
) -> Result<ApplyReport, String> {
    blocking(move || {
        let report = setup::download_php(&version, &|event| {
            let _ = on_event.send(event);
        })?;
        refresh(&app);
        Ok(report)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephant_core::setup::Database;

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

    #[test]
    fn the_saved_file_remembers_the_choice() {
        let file = setup_file_for(
            &ProfileInput::LaravelServices {
                database: Database::Postgres,
            },
            "2026-11-06T10:20:00Z".into(),
        );
        assert_eq!(file.scenario, Some(ScenarioId::LaravelServices));
        assert_eq!(file.database, Some(Database::Postgres));
        assert_eq!(file.version, SETUP_VERSION);
        assert!(!status_of(Some(file)).news);

        let detection = Detection::default();
        let profile = setup::profile_from_system(&detection);
        let file = setup_file_for(
            &ProfileInput::Custom {
                profile: profile.clone(),
            },
            "x".into(),
        );
        assert_eq!(
            (file.scenario, file.custom),
            (Some(ScenarioId::Custom), Some(profile))
        );
    }

    #[test]
    fn only_one_apply_runs_at_a_time() {
        let flag = Mutex::new(false);
        let first = Running::start(&flag).unwrap();
        assert!(Running::start(&flag).is_err());
        drop(first);
        assert!(Running::start(&flag).is_ok());
    }
}
