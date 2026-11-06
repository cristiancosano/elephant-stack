//! Tareas largas de los agentes (instalar un componente, crear un proyecto):
//! `~/.elephant-stack/jobs/<id>.json` con su estado y `<id>.log` con la salida.
//! Las crea la CLI, las ejecuta `elephant __job <id>` y la app las muestra.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

use crate::{fsutil, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl JobState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct JobInfo {
    pub id: String,
    /// Id de la operación: `component_install`, `project_create`.
    pub kind: String,
    pub args: serde_json::Value,
    pub client: Option<String>,
    pub state: JobState,
    /// De 0 a 1, si se conoce.
    pub progress: Option<f32>,
    pub message: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub finished_at_ms: Option<i64>,
}

fn dir_in(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("jobs")
}

fn dir() -> Result<PathBuf, String> {
    Ok(dir_in(&paths::home()?))
}

/// `[a-z0-9-]{8,40}`: así un id nunca sale de la carpeta de tareas.
pub fn check_id(id: &str) -> Result<(), String> {
    let valid = (8..=40).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if valid {
        Ok(())
    } else {
        Err(crate::tr!("Identificador de tarea no válido: {id}", "Invalid job ID: {id}"))
    }
}

/// Único entre procesos: milisegundos, pid y un contador.
fn new_id() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!(
        "{:x}-{:x}-{:x}",
        crate::now_ms(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub fn create(kind: &str, args: serde_json::Value, client: Option<&str>) -> Result<JobInfo, String> {
    create_in(&dir()?, kind, args, client)
}

pub fn save(job: &JobInfo) -> Result<(), String> {
    save_in(&dir()?, job)
}

/// Lee una tarea. Si figura en marcha pero su proceso ya no existe, la marca
/// como fallida (ver [`hold`]).
pub fn read(id: &str) -> Result<JobInfo, String> {
    let dir = dir()?;
    read_in(&dir, id).map(|job| reap_in(&dir, job, crate::now_ms()))
}

/// Las que están en cola o en marcha, de la más antigua a la más reciente. Las
/// que se quedaron sin proceso se marcan como fallidas y no salen.
pub fn list_active() -> Vec<JobInfo> {
    dir().map(|dir| list_active_in(&dir, crate::now_ms())).unwrap_or_default()
}

/// La toma `elephant __job` mientras ejecuta la tarea: un cerrojo sobre su log
/// que el sistema suelta si el proceso muere (kill, pánico, falta de memoria).
/// Así se sabe si una tarea «en marcha» sigue viva sin guardar pids.
pub fn hold(id: &str) -> Result<fs::File, String> {
    hold_in(&dir()?, id)
}

fn hold_in(dir: &Path, id: &str) -> Result<fs::File, String> {
    check_id(id)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{id}.log")))
        .map_err(|error| crate::tr!("No se pudo abrir el log de la tarea: {error}", "Couldn’t open the job log: {error}"))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => Err(crate::tr!("La tarea {id} ya se está ejecutando.", "Job {id} is already running.")),
        Err(fs::TryLockError::Error(error)) => {
            Err(crate::tr!("No se pudo bloquear la tarea {id}: {error}", "Couldn’t lock job {id}: {error}"))
        }
    }
}

/// Una tarea en cola tiene este margen para que su proceso arranque y tome el
/// cerrojo.
const QUEUED_GRACE_MS: i64 = 60_000;
fn orphaned() -> String {
    crate::tr!(
        "La tarea se interrumpió: su proceso terminó sin acabarla.",
        "The job was interrupted: its process ended before finishing it."
    )
}

/// Si la tarea sigue activa pero nadie tiene su cerrojo, su proceso murió:
/// se marca como fallida.
fn reap_in(dir: &Path, job: JobInfo, now_ms: i64) -> JobInfo {
    let suspect = match job.state {
        JobState::Running => true,
        JobState::Queued => now_ms - job.started_at_ms > QUEUED_GRACE_MS,
        JobState::Succeeded | JobState::Failed => false,
    };
    if !suspect {
        return job;
    }
    let Ok(_lock) = hold_in(dir, &job.id) else {
        return job;
    };
    // Con el cerrojo en la mano se vuelve a leer: puede que terminase justo
    // antes de soltarlo.
    let Ok(mut job) = read_in(dir, &job.id) else {
        return job;
    };
    if job.state.is_active() {
        job.state = JobState::Failed;
        job.message = Some(orphaned());
        job.finished_at_ms = Some(now_ms);
        let _ = save_in(dir, &job);
    }
    job
}

pub fn log_path(id: &str) -> Result<PathBuf, String> {
    check_id(id)?;
    Ok(dir()?.join(format!("{id}.log")))
}

/// Borra las tareas terminadas hace más de `max_age_days` días, con su log.
pub fn cleanup(max_age_days: u32) {
    if let Ok(dir) = dir() {
        cleanup_in(&dir, max_age_days, crate::now_ms());
    }
}

fn create_in(
    dir: &Path,
    kind: &str,
    args: serde_json::Value,
    client: Option<&str>,
) -> Result<JobInfo, String> {
    let job = JobInfo {
        id: new_id(),
        kind: kind.to_string(),
        args,
        client: client.map(str::to_string),
        state: JobState::Queued,
        progress: None,
        message: None,
        started_at_ms: crate::now_ms(),
        finished_at_ms: None,
    };
    save_in(dir, &job)?;
    Ok(job)
}

fn save_in(dir: &Path, job: &JobInfo) -> Result<(), String> {
    check_id(&job.id)?;
    fsutil::write_json_atomic(&dir.join(format!("{}.json", job.id)), job)
}

fn read_in(dir: &Path, id: &str) -> Result<JobInfo, String> {
    check_id(id)?;
    let text = fs::read_to_string(dir.join(format!("{id}.json")))
        .map_err(|_| crate::tr!("No existe la tarea {id}.", "Job {id} doesn’t exist."))?;
    serde_json::from_str(&text).map_err(|error| crate::tr!("La tarea {id} está dañada: {error}", "Job {id} is damaged: {error}"))
}

fn all_in(dir: &Path) -> Vec<JobInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name.strip_suffix(".json")?;
            read_in(dir, id).ok()
        })
        .collect()
}

fn list_active_in(dir: &Path, now_ms: i64) -> Vec<JobInfo> {
    let mut jobs: Vec<_> = all_in(dir)
        .into_iter()
        .filter(|job| job.state.is_active())
        .map(|job| reap_in(dir, job, now_ms))
        .filter(|job| job.state.is_active())
        .collect();
    jobs.sort_by(|a, b| a.started_at_ms.cmp(&b.started_at_ms).then_with(|| a.id.cmp(&b.id)));
    jobs
}

fn cleanup_in(dir: &Path, max_age_days: u32, now_ms: i64) {
    let max_age_ms = i64::from(max_age_days) * 24 * 60 * 60 * 1000;
    for job in all_in(dir) {
        let Some(finished) = job.finished_at_ms else {
            continue;
        };
        if !job.state.is_active() && now_ms - finished > max_age_ms {
            let _ = fs::remove_file(dir.join(format!("{}.json", job.id)));
            let _ = fs::remove_file(dir.join(format!("{}.log", job.id)));
            // El resultado que guarda `elephant __job` (elephant-api).
            let _ = fs::remove_file(dir.join(format!("{}.result.json", job.id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use serde_json::json;

    #[test]
    fn ids_are_valid_and_unique() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert!(check_id(&a).is_ok(), "{a}");
    }

    #[test]
    fn rejects_ids_that_could_escape_the_folder() {
        for bad in ["../../etc", "corto", "MAYUSCULAS-123", "a/b/c/d/e/f", &"a".repeat(41)] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn create_save_and_read_round_trip() {
        let dir = TempDir::new("jobs");
        let mut job = create_in(dir.path(), "component_install", json!({"id": "redis"}), Some("tty")).unwrap();
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(read_in(dir.path(), &job.id).unwrap(), job);

        job.state = JobState::Running;
        job.progress = Some(0.5);
        save_in(dir.path(), &job).unwrap();
        assert_eq!(read_in(dir.path(), &job.id).unwrap().progress, Some(0.5));
        assert!(read_in(dir.path(), "no-existe-1234").is_err());
    }

    #[test]
    fn only_queued_and_running_jobs_are_active() {
        let dir = TempDir::new("jobs-active");
        let first = create_in(dir.path(), "a", json!({}), None).unwrap();
        let mut done = create_in(dir.path(), "b", json!({}), None).unwrap();
        done.state = JobState::Succeeded;
        done.finished_at_ms = Some(crate::now_ms());
        save_in(dir.path(), &done).unwrap();
        fs::write(dir.path().join("basura.json"), "{").unwrap();

        let active = list_active_in(dir.path(), crate::now_ms());
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, first.id);
    }

    #[test]
    fn running_jobs_without_a_live_process_are_marked_failed() {
        let dir = TempDir::new("jobs-reap");
        let now = crate::now_ms();
        let mut alive = create_in(dir.path(), "a", json!({}), None).unwrap();
        alive.state = JobState::Running;
        save_in(dir.path(), &alive).unwrap();
        let mut dead = create_in(dir.path(), "b", json!({}), None).unwrap();
        dead.state = JobState::Running;
        save_in(dir.path(), &dead).unwrap();
        let mut stuck = create_in(dir.path(), "c", json!({}), None).unwrap();
        stuck.started_at_ms = now - 2 * QUEUED_GRACE_MS;
        save_in(dir.path(), &stuck).unwrap();
        let fresh = create_in(dir.path(), "d", json!({}), None).unwrap();

        // El proceso de `alive` tiene su cerrojo; los demás no.
        let lock = hold_in(dir.path(), &alive.id).unwrap();
        assert!(hold_in(dir.path(), &alive.id).is_err(), "el cerrojo es exclusivo");
        let active: Vec<String> = list_active_in(dir.path(), now).into_iter().map(|j| j.id).collect();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&alive.id) && active.contains(&fresh.id));
        let reaped = read_in(dir.path(), &dead.id).unwrap();
        assert_eq!(reaped.state, JobState::Failed);
        assert_eq!(reaped.message, Some(orphaned()));
        assert!(reaped.finished_at_ms.is_some());
        assert_eq!(read_in(dir.path(), &stuck.id).unwrap().state, JobState::Failed);

        // Al soltar el cerrojo (el proceso muere), también se marca.
        drop(lock);
        let job = read_in(dir.path(), &alive.id).unwrap();
        assert_eq!(reap_in(dir.path(), job, now).state, JobState::Failed);
    }

    #[test]
    fn cleanup_removes_old_finished_jobs_and_their_logs() {
        let dir = TempDir::new("jobs-cleanup");
        let now = crate::now_ms();
        let day = 24 * 60 * 60 * 1000;

        let mut old = create_in(dir.path(), "a", json!({}), None).unwrap();
        old.state = JobState::Failed;
        old.finished_at_ms = Some(now - 8 * day);
        save_in(dir.path(), &old).unwrap();
        fs::write(dir.path().join(format!("{}.log", old.id)), "salida").unwrap();
        fs::write(dir.path().join(format!("{}.result.json", old.id)), "{}").unwrap();

        let mut recent = create_in(dir.path(), "b", json!({}), None).unwrap();
        recent.state = JobState::Succeeded;
        recent.finished_at_ms = Some(now - day);
        save_in(dir.path(), &recent).unwrap();

        let running = create_in(dir.path(), "c", json!({}), None).unwrap();

        cleanup_in(dir.path(), 7, now);
        assert!(read_in(dir.path(), &old.id).is_err());
        assert!(!dir.path().join(format!("{}.log", old.id)).exists());
        assert!(!dir.path().join(format!("{}.result.json", old.id)).exists());
        assert!(read_in(dir.path(), &recent.id).is_ok());
        assert!(read_in(dir.path(), &running.id).is_ok());
    }
}
