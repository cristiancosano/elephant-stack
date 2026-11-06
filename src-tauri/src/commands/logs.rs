//! Comandos del visor de logs (00 §5.6) sobre `elephant-logs`, y el resumen
//! de errores del menú de la barra (C16).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};

use elephant_core::paths;
use elephant_logs::{
    EntryDetail, LogBatch, LogGroup, LogHub, LogPage, LogQuery, Project, ProjectOverride,
    ProjectSummary,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use super::blocking;
use crate::state::{self, AppState, LogProjectBrief, LogsBrief};

/// El contador del tray se recalcula como mucho una vez cada 30 s (C16).
const BRIEF_EVERY_MS: i64 = 30_000;
/// «Sin ver» y «recientes» miran la última hora.
const BRIEF_WINDOW_MS: i64 = 3_600_000;
const BRIEF_PROJECTS: usize = 5;

/// El `LogHub` vive mientras la ventana existe: se crea con el primer comando
/// y se suelta en el `Destroyed` de `main` (C3). Su hilo de sondeo solo existe
/// mientras hay suscripciones.
#[derive(Default)]
pub struct LogsState {
    hub: Mutex<Option<LogHub>>,
    /// Última visita a la vista Logs (C16).
    seen_at_ms: AtomicI64,
    /// Último recálculo del contador del tray.
    last_brief_ms: AtomicI64,
}

impl LogsState {
    fn lock(&self) -> MutexGuard<'_, Option<LogHub>> {
        self.hub
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Tras cambiar `projects.json`, para que el hub no sirva el catálogo
    /// anterior durante sus 3 s de caché.
    fn invalidate(&self) {
        if let Some(hub) = self.lock().as_ref() {
            hub.invalidate();
        }
    }

    fn with_hub<T>(&self, f: impl FnOnce(&LogHub) -> T) -> Result<T, String> {
        let home = paths::home()?;
        let mut guard = self.lock();
        Ok(f(guard.get_or_insert_with(|| LogHub::new(home))))
    }
}

/// Suelta el hub: termina el sondeo y cierra los archivos.
pub fn release(app: &AppHandle) {
    let hub = app.state::<LogsState>().lock().take();
    drop(hub);
}

/// Reserva el turno de recálculo si han pasado 30 s desde el anterior.
fn claim_turn(last: &AtomicI64, now_ms: i64) -> bool {
    let previous = last.load(Ordering::Relaxed);
    if previous != 0 && now_ms - previous < BRIEF_EVERY_MS {
        return false;
    }
    last.compare_exchange(previous, now_ms, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

/// Recalcula `snapshot.logs`, como mucho una vez cada 30 s y en otro hilo:
/// contar errores puede costar cientos de ms. Lo llama el tray en `Enter` y
/// `Click` (C16), nunca un temporizador.
pub fn refresh_brief(app: &AppHandle) {
    let now = elephant_core::now_ms();
    let logs = app.state::<LogsState>();
    if !claim_turn(&logs.last_brief_ms, now) {
        return;
    }
    let seen_at = logs.seen_at_ms.load(Ordering::Relaxed);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("elephant-logs-resumen".into())
        .spawn(move || {
            let Ok(home) = paths::home() else { return };
            let projects = app.state::<AppState>().snapshot().projects;
            let brief = compute_brief(&home, &projects, seen_at, now);
            state::update(&app, |s| s.logs = brief);
        });
    if spawned.is_err() {
        // Sin hilo no hay recálculo: que el siguiente `Enter` lo intente.
        reset_turn(&logs.last_brief_ms);
    }
}

fn reset_turn(last: &AtomicI64) {
    last.store(0, Ordering::Relaxed);
}

fn compute_brief(home: &Path, projects: &[Project], seen_at_ms: i64, now_ms: i64) -> LogsBrief {
    let since = now_ms - BRIEF_WINDOW_MS;
    let hour = elephant_logs::count_errors(home, since);
    let unseen = if seen_at_ms > since {
        elephant_logs::count_errors(home, seen_at_ms + 1)
    } else {
        hour.clone()
    };
    brief_from(&hour, &unseen, projects)
}

/// «Sin ver» suma todo (también lo que no es de ningún proyecto); «recientes»
/// son los proyectos con errores en la última hora, el más reciente primero.
fn brief_from(
    hour: &[(String, u32, Option<i64>)],
    unseen: &[(String, u32, Option<i64>)],
    projects: &[Project],
) -> LogsBrief {
    let mut recent: Vec<LogProjectBrief> = hour
        .iter()
        .filter_map(|(id, errors, last_ms)| {
            let project = projects.iter().find(|p| p.id == *id)?;
            Some(LogProjectBrief {
                id: id.clone(),
                name: project.name.clone(),
                errors: *errors,
                last_ms: *last_ms,
            })
        })
        .collect();
    recent.sort_by(|a, b| b.last_ms.cmp(&a.last_ms).then_with(|| a.name.cmp(&b.name)));
    recent.truncate(BRIEF_PROJECTS);
    LogsBrief {
        available: true,
        unseen_errors: unseen
            .iter()
            .map(|(_, n, _)| *n)
            .fold(0u32, u32::saturating_add),
        recent,
    }
}

fn summaries() -> Result<Vec<ProjectSummary>, String> {
    Ok(elephant_logs::summaries(&paths::home()?))
}

#[tauri::command]
pub async fn logs_projects(app: AppHandle) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || app.state::<LogsState>().with_hub(LogHub::summaries)).await
}

#[tauri::command]
pub async fn logs_query(app: AppHandle, q: LogQuery) -> Result<LogPage, String> {
    blocking(move || app.state::<LogsState>().with_hub(|hub| hub.query(&q))?).await
}

#[tauri::command]
pub async fn logs_entry(app: AppHandle, id: String) -> Result<EntryDetail, String> {
    blocking(move || app.state::<LogsState>().with_hub(|hub| hub.entry(&id))?).await
}

#[tauri::command]
pub async fn logs_groups(app: AppHandle, q: LogQuery) -> Result<Vec<LogGroup>, String> {
    blocking(move || app.state::<LogsState>().with_hub(|hub| hub.groups(&q))?).await
}

/// Los lotes llegan por el `Channel` como mucho cada 100 ms. Si el canal se
/// rompe (la vista se ha ido), la suscripción se cancela sola.
#[tauri::command]
pub async fn logs_subscribe(
    app: AppHandle,
    q: LogQuery,
    on_batch: Channel<LogBatch>,
) -> Result<u32, String> {
    blocking(move || {
        app.state::<LogsState>()
            .with_hub(|hub| hub.subscribe(q, Box::new(move |batch| on_batch.send(batch).is_ok())))
    })
    .await
}

/// Sin hub no hay nada que cancelar: no se crea uno para eso.
#[tauri::command]
pub async fn logs_unsubscribe(app: AppHandle, id: u32) -> Result<(), String> {
    blocking(move || {
        if let Some(hub) = app.state::<LogsState>().lock().as_ref() {
            hub.unsubscribe(id);
        }
        Ok(())
    })
    .await
}

/// Carpetas raíz que ha añadido el usuario, para Ajustes ▸ Logs.
#[tauri::command]
pub async fn logs_roots() -> Result<Vec<String>, String> {
    blocking(move || {
        Ok(elephant_logs::projects::roots(&paths::home()?)
            .into_iter()
            .map(|root| root.display().to_string())
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn logs_add_root(app: AppHandle, path: String) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || {
        elephant_logs::projects::add_root(&paths::home()?, &PathBuf::from(path))?;
        app.state::<LogsState>().invalidate();
        elephant_core::stamp::touch("projects");
        summaries()
    })
    .await
}

#[tauri::command]
pub async fn logs_remove_root(app: AppHandle, path: String) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || {
        elephant_logs::projects::remove_root(&paths::home()?, &PathBuf::from(path))?;
        app.state::<LogsState>().invalidate();
        elephant_core::stamp::touch("projects");
        summaries()
    })
    .await
}

#[tauri::command]
pub async fn logs_set_override(
    app: AppHandle,
    project: String,
    o: ProjectOverride,
) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || {
        elephant_logs::projects::set_override(&paths::home()?, &project, o)?;
        app.state::<LogsState>().invalidate();
        summaries()
    })
    .await
}

/// Vacía un archivo de un proyecto o de Elephant (nunca de Herd). La interfaz
/// pide confirmación antes.
#[tauri::command]
pub async fn logs_clear(file_key: String) -> Result<(), String> {
    blocking(move || elephant_logs::clear(&paths::home()?, &file_key)).await
}

/// Al entrar en la vista Logs: el contador del tray vuelve a 0.
#[tauri::command]
pub async fn logs_mark_seen(app: AppHandle, logs: State<'_, LogsState>) -> Result<(), String> {
    logs.seen_at_ms
        .store(elephant_core::now_ms(), Ordering::Relaxed);
    state::update(&app, |s| s.logs.unseen_errors = 0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephant_logs::{ProjectKind, ProjectOrigin};

    fn project(id: &str, name: &str) -> Project {
        Project {
            id: id.into(),
            name: name.into(),
            root: PathBuf::from("/x"),
            hosts: Vec::new(),
            kind: ProjectKind::Laravel,
            origin: ProjectOrigin::HerdParked,
            php_version: None,
            log_tz: "UTC".into(),
        }
    }

    #[test]
    fn the_brief_is_recalculated_at_most_every_30_seconds() {
        let last = AtomicI64::new(0);
        assert!(claim_turn(&last, 1_000_000));
        assert!(!claim_turn(&last, 1_000_000 + 29_999));
        assert!(claim_turn(&last, 1_000_000 + 30_000));
        reset_turn(&last);
        assert!(claim_turn(&last, 1_000_000 + 30_001));
    }

    #[test]
    fn the_brief_counts_everything_unseen_and_lists_recent_projects() {
        let projects: Vec<Project> = (0..7)
            .map(|i| project(&format!("p{i}"), &format!("proyecto {i}")))
            .collect();
        let mut hour: Vec<(String, u32, Option<i64>)> = (0..7)
            .map(|i| (format!("p{i}"), i + 1, Some(i64::from(i) * 1000)))
            .collect();
        hour.push(("none".into(), 4, Some(99_999)));
        let unseen = vec![
            ("p6".to_string(), 2, Some(6000)),
            ("system".to_string(), 1, Some(1)),
        ];
        let brief = brief_from(&hour, &unseen, &projects);
        assert!(brief.available);
        assert_eq!(brief.unseen_errors, 3);
        let names: Vec<&str> = brief.recent.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "proyecto 6",
                "proyecto 5",
                "proyecto 4",
                "proyecto 3",
                "proyecto 2"
            ]
        );
        assert_eq!(brief.recent[0].errors, 7);
    }
}
