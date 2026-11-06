//! Comandos del visor de logs (00 §5.6). Stub de P01 sobre las funciones de
//! `elephant-logs`, que también son stubs; P03 implementa los dos.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};

use elephant_core::paths;
use elephant_logs::{
    EntryDetail, LogBatch, LogGroup, LogHub, LogPage, LogQuery, ProjectOverride, ProjectSummary,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use super::blocking;
use crate::state;

/// El `LogHub` vive mientras la ventana existe: se crea con el primer comando
/// y se suelta en el `Destroyed` de `main` (C3).
#[derive(Default)]
pub struct LogsState {
    hub: Mutex<Option<LogHub>>,
    /// Última visita a la vista Logs (C16).
    seen_at_ms: AtomicI64,
    /// Último recálculo del contador del tray: como mucho uno cada 30 s.
    last_brief_ms: AtomicI64,
}

impl LogsState {
    fn lock(&self) -> MutexGuard<'_, Option<LogHub>> {
        self.hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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

/// Recalcula `snapshot.logs`, como mucho una vez cada 30 s. P03.
#[allow(dead_code)] // Lo llama el tray de P05.
pub fn refresh_brief(app: &AppHandle) {
    let state = app.state::<LogsState>();
    let _ = state.last_brief_ms.load(Ordering::Relaxed);
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

#[tauri::command]
pub async fn logs_subscribe(app: AppHandle, q: LogQuery, on_batch: Channel<LogBatch>) -> Result<u32, String> {
    blocking(move || {
        app.state::<LogsState>()
            .with_hub(|hub| hub.subscribe(q, Box::new(move |batch| on_batch.send(batch).is_ok())))
    })
    .await
}

#[tauri::command]
pub async fn logs_unsubscribe(app: AppHandle, id: u32) -> Result<(), String> {
    blocking(move || app.state::<LogsState>().with_hub(|hub| hub.unsubscribe(id))).await
}

#[tauri::command]
pub async fn logs_add_root(path: String) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || {
        elephant_logs::projects::add_root(&paths::home()?, &PathBuf::from(path))?;
        elephant_core::stamp::touch("projects");
        summaries()
    })
    .await
}

#[tauri::command]
pub async fn logs_remove_root(path: String) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || {
        elephant_logs::projects::remove_root(&paths::home()?, &PathBuf::from(path))?;
        elephant_core::stamp::touch("projects");
        summaries()
    })
    .await
}

#[tauri::command]
pub async fn logs_set_override(project: String, o: ProjectOverride) -> Result<Vec<ProjectSummary>, String> {
    blocking(move || {
        elephant_logs::projects::set_override(&paths::home()?, &project, o)?;
        summaries()
    })
    .await
}

#[tauri::command]
pub async fn logs_clear(file_key: String) -> Result<(), String> {
    blocking(move || elephant_logs::clear(&paths::home()?, &file_key)).await
}

/// Al entrar en la vista Logs: el contador del tray vuelve a 0.
#[tauri::command]
pub async fn logs_mark_seen(app: AppHandle, logs: State<'_, LogsState>) -> Result<(), String> {
    logs.seen_at_ms.store(elephant_core::now_ms(), Ordering::Relaxed);
    state::update(&app, |s| s.logs.unseen_errors = 0);
    Ok(())
}
