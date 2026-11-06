//! Vigilancia de los servicios y de los cambios hechos fuera de la app.
//!
//! - Cada 20 s, si `settings.watch_services`, `launchctl list`
//!   (`elephant_core::launchd`, unos 10 ms y sin `brew`). Si el PID o el código
//!   de salida de algún servicio no coincide con el snapshot, se pide un
//!   refresco completo. En el mismo tick se actualiza la memoria de la cabecera
//!   del menú con `ps`, solo si hay servicios en marcha.
//! - `notify` (FSEvents, sin coste en reposo) sobre `~/.elephant-stack`, sin
//!   recursión, y sobre `jobs/`. Un `state.stamp` escrito por otro proceso (la
//!   CLI, `elephant mcp`, una tarea larga) refresca el estado (C7); los que
//!   lleva el `pid` de la app se ignoran. Un cambio en `jobs/` solo vuelve a
//!   leer las tareas en curso.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use elephant_core::components::Component;
use elephant_core::launchd::{self, LaunchJob};
use elephant_core::stamp::{self, Stamp};
use elephant_core::{jobs, monitor, paths};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Manager};

use crate::state::{self, AppState};
use crate::tray;

const TICK: Duration = Duration::from_secs(20);
/// Un tick extra (al acercar el puntero al icono) como mucho cada 2 s.
const MIN_EXTRA_TICK: Duration = Duration::from_secs(2);
/// Los eventos de archivo se agrupan en ventanas de este tamaño: una tarea
/// que escribe su progreso sin parar no provoca más de 4 lecturas por segundo.
const BATCH: Duration = Duration::from_millis(250);

/// PID y último código de salida de cada servicio. Los que no tienen ninguno
/// de los dos no aparecen: para launchd y para Homebrew es lo mismo «no
/// cargado» que «parado sin error».
type Fingerprint = BTreeMap<String, (Option<i64>, Option<i64>)>;

#[derive(Default)]
struct Inner {
    last_tick: Option<Instant>,
    /// Huella de launchd que ya provocó un refresco: si Homebrew la describe
    /// de otra forma, no se refresca en cada tick.
    refreshed_for: Option<Fingerprint>,
    watcher: Option<RecommendedWatcher>,
    watched: BTreeSet<PathBuf>,
    events: Option<Sender<FsChange>>,
}

#[derive(Default)]
struct WatchState {
    inner: Mutex<Inner>,
    /// Un solo tick a la vez.
    ticking: Mutex<()>,
}

impl WatchState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn start(app: &AppHandle) {
    if app.try_state::<WatchState>().is_some() {
        return;
    }
    app.manage(WatchState::default());

    let (tx, rx) = mpsc::channel();
    app.state::<WatchState>().lock().events = Some(tx);
    let worker = app.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("elephant-watch-fs".into())
        .spawn(move || handle_fs_events(&worker, rx))
    {
        eprintln!("No se pudo vigilar ~/.elephant-stack: {error}");
    }
    ensure_fs_watch(app);

    let ticker = app.clone();
    let spawned = std::thread::Builder::new().name("elephant-watch".into()).spawn(move || {
        loop {
            // El primer refresco ya lo lanza `lib.rs` al arrancar.
            std::thread::sleep(TICK);
            ensure_fs_watch(&ticker);
            if ticker.state::<AppState>().settings().watch_services {
                tick(&ticker);
            }
        }
    });
    if let Err(error) = spawned {
        eprintln!("No se pudo vigilar los servicios: {error}");
    }
}

/// Un tick inmediato, por ejemplo al abrir el menú de la barra. No bloquea.
pub fn tick_now(app: &AppHandle) {
    let Some(watch) = app.try_state::<WatchState>() else {
        return;
    };
    if !app.state::<AppState>().settings().watch_services {
        return;
    }
    {
        let inner = watch.lock();
        if inner.last_tick.is_some_and(|at| at.elapsed() < MIN_EXTRA_TICK) {
            return;
        }
    }
    let app = app.clone();
    std::thread::spawn(move || tick(&app));
}

fn tick(app: &AppHandle) {
    let watch = app.state::<WatchState>();
    let Ok(_ticking) = watch.ticking.try_lock() else {
        return;
    };
    watch.lock().last_tick = Some(Instant::now());

    let jobs = match launchd::homebrew_jobs() {
        Ok(jobs) => jobs,
        Err(error) => {
            eprintln!("No se pudo consultar launchd: {error}");
            return;
        }
    };
    let state = app.state::<AppState>();
    let snapshot = state.snapshot();
    // Todavía no ha terminado el primer refresco: no hay con qué comparar.
    if snapshot.refreshed_at_ms == 0 {
        return;
    }
    let known = known_services(&snapshot.components);
    let now = jobs_fingerprint(&jobs, &known);
    let refresh = {
        let mut inner = watch.lock();
        let (refresh, remember) = decide_refresh(&now, &services_fingerprint(&snapshot.components), inner.refreshed_for.as_ref());
        inner.refreshed_for = remember;
        refresh
    };
    if refresh {
        state::refresh_in_background(app);
    }

    let pids = running_pids(&jobs, &known);
    let had_usage = state.usage_kb().is_some_and(|kb| kb > 0);
    if pids.is_empty() {
        if had_usage {
            state.set_usage(&[]);
            tray::set_usage(app, state.usage_kb());
        }
        return;
    }
    match monitor::usage_for(&pids) {
        Ok(usage) => {
            state.set_usage(&usage);
            tray::set_usage(app, state.usage_kb());
        }
        Err(error) => eprintln!("No se pudo medir la memoria de los servicios: {error}"),
    }
}

fn known_services(components: &[Component]) -> BTreeSet<&str> {
    components
        .iter()
        .filter_map(|c| c.service.as_ref())
        .map(|s| s.name.as_str())
        .collect()
}

/// El código de salida solo cuenta si el proceso no está en marcha, y un 0 es
/// lo mismo que no tener código.
fn normalized(pid: Option<i64>, exit: Option<i64>) -> Option<(Option<i64>, Option<i64>)> {
    let exit = if pid.is_some() { None } else { exit.filter(|code| *code != 0) };
    (pid.is_some() || exit.is_some()).then_some((pid, exit))
}

fn jobs_fingerprint(jobs: &[LaunchJob], known: &BTreeSet<&str>) -> Fingerprint {
    jobs.iter()
        .filter_map(|job| {
            let name = job.service_name()?;
            if !known.contains(name) {
                return None;
            }
            Some((name.to_string(), normalized(job.pid, job.last_exit)?))
        })
        .collect()
}

fn services_fingerprint(components: &[Component]) -> Fingerprint {
    components
        .iter()
        .filter_map(|c| c.service.as_ref())
        .filter_map(|s| Some((s.name.clone(), normalized(s.pid, s.exit_code)?)))
        .collect()
}

/// `(refrescar, huella que recordar)`. Se refresca si launchd no coincide con
/// el snapshot, salvo que ya se refrescara por esta misma huella.
fn decide_refresh(
    now: &Fingerprint,
    snapshot: &Fingerprint,
    refreshed_for: Option<&Fingerprint>,
) -> (bool, Option<Fingerprint>) {
    if now == snapshot {
        return (false, None);
    }
    if refreshed_for == Some(now) {
        return (false, refreshed_for.cloned());
    }
    (true, Some(now.clone()))
}

fn running_pids(jobs: &[LaunchJob], known: &BTreeSet<&str>) -> Vec<i64> {
    jobs.iter()
        .filter(|job| job.service_name().is_some_and(|name| known.contains(name)))
        .filter_map(|job| job.pid)
        .collect()
}

// ---------------------------------------------------------------------------
// Archivos
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FsChange {
    Stamp,
    Jobs,
    /// Ha aparecido `jobs/`: hay que empezar a vigilarla.
    JobsDir,
}

/// Qué significa un evento sobre `path`. Solo mira los nombres: FSEvents
/// puede dar la ruta real (`/private/var/…`) en lugar de la pedida.
fn classify(path: &Path) -> Option<FsChange> {
    let name = path.file_name()?.to_str()?;
    let parent = path.parent().and_then(Path::file_name).and_then(|p| p.to_str());
    match (parent, name) {
        (Some(".elephant-stack"), "state.stamp") => Some(FsChange::Stamp),
        (Some(".elephant-stack"), "jobs") => Some(FsChange::JobsDir),
        (Some("jobs"), file) if file.ends_with(".json") => Some(FsChange::Jobs),
        _ => None,
    }
}

/// Un aviso de otro proceso. Los de la propia app ya se han reflejado.
fn stamp_needs_refresh(stamp: Option<&Stamp>, own_pid: u32) -> bool {
    stamp.is_some_and(|s| s.pid != own_pid)
}

fn watcher(tx: Sender<FsChange>) -> notify::Result<RecommendedWatcher> {
    notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            return;
        };
        for change in event.paths.iter().filter_map(|path| classify(path)) {
            let _ = tx.send(change);
        }
    })
}

/// Empieza a vigilar `~/.elephant-stack` y `jobs/` en cuanto existen. Se
/// reintenta en cada tick, porque al principio puede no haber ninguna.
fn ensure_fs_watch(app: &AppHandle) {
    let Ok(dir) = paths::data_dir() else {
        return;
    };
    let watch = app.state::<WatchState>();
    let mut inner = watch.lock();
    let wanted = [dir.clone(), dir.join("jobs")];
    if wanted.iter().all(|path| inner.watched.contains(path)) {
        return;
    }
    if inner.watcher.is_none() {
        let Some(tx) = inner.events.clone() else {
            return;
        };
        match watcher(tx) {
            Ok(created) => inner.watcher = Some(created),
            Err(error) => {
                eprintln!("No se pudo vigilar ~/.elephant-stack: {error}");
                return;
            }
        }
    }
    for path in wanted {
        if inner.watched.contains(&path) || !path.is_dir() {
            continue;
        }
        let Some(watcher) = inner.watcher.as_mut() else {
            return;
        };
        match watcher.watch(&path, RecursiveMode::NonRecursive) {
            Ok(()) => {
                inner.watched.insert(path);
            }
            Err(error) => eprintln!("No se pudo vigilar {}: {error}", path.display()),
        }
    }
}

/// Recibe los eventos y los agrupa en ventanas de [`BATCH`].
fn handle_fs_events(app: &AppHandle, rx: Receiver<FsChange>) {
    while let Some(changes) = next_batch(&rx, BATCH) {
        if changes.contains(&FsChange::JobsDir) {
            ensure_fs_watch(app);
        }
        if changes.contains(&FsChange::Stamp) && stamp_needs_refresh(stamp::read().as_ref(), std::process::id()) {
            // El refresco completo ya vuelve a leer las tareas.
            state::refresh_in_background(app);
        } else if changes.contains(&FsChange::Jobs) || changes.contains(&FsChange::JobsDir) {
            let active = jobs::list_active();
            if app.state::<AppState>().snapshot().jobs != active {
                state::update(app, |s| s.jobs = active);
            }
        }
    }
}

/// Espera el primer evento y recoge los que lleguen en `window`. `None` si el
/// canal se ha cerrado.
fn next_batch(rx: &Receiver<FsChange>, window: Duration) -> Option<BTreeSet<FsChange>> {
    let first = rx.recv().ok()?;
    let mut changes = BTreeSet::from([first]);
    let deadline = Instant::now() + window;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(change) => {
                changes.insert(change);
            }
            Err(RecvTimeoutError::Timeout) => return Some(changes),
            Err(RecvTimeoutError::Disconnected) => return Some(changes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephant_core::brew::Service;
    use elephant_core::components::Kind;

    fn job(label: &str, pid: Option<i64>, last_exit: Option<i64>) -> LaunchJob {
        LaunchJob {
            label: label.into(),
            pid,
            last_exit,
        }
    }

    fn component(name: &str, pid: Option<i64>, exit_code: Option<i64>) -> Component {
        Component {
            id: name.into(),
            kind: Kind::Service,
            description: "",
            installed: true,
            version: None,
            origin: Some("Homebrew"),
            path: None,
            service: Some(Service {
                name: name.into(),
                status: if pid.is_some() { "started" } else { "none" }.into(),
                running: pid.is_some(),
                registered: true,
                pid,
                exit_code,
            }),
        }
    }

    #[test]
    fn launchd_and_the_snapshot_agree_on_a_quiet_system() {
        let components = [component("mysql", Some(10), Some(0)), component("redis", None, None)];
        let jobs = [
            job("homebrew.mxcl.mysql", Some(10), Some(0)),
            // Cargado y parado sin error: lo mismo que no cargado.
            job("homebrew.mxcl.redis", None, Some(0)),
            // Un servicio que Elephant no conoce no cuenta.
            job("homebrew.mxcl.ollama", Some(99), None),
            job("com.apple.Finder", Some(1), None),
        ];
        let known = known_services(&components);
        assert_eq!(jobs_fingerprint(&jobs, &known), services_fingerprint(&components));
        assert_eq!(running_pids(&jobs, &known), [10]);
    }

    #[test]
    fn a_stopped_or_crashed_service_is_a_difference() {
        let components = [component("mysql", Some(10), None)];
        let known = known_services(&components);
        let snapshot = services_fingerprint(&components);
        let stopped = jobs_fingerprint(&[], &known);
        assert_ne!(stopped, snapshot);
        let crashed = jobs_fingerprint(&[job("homebrew.mxcl.mysql", None, Some(1))], &known);
        assert_ne!(crashed, snapshot);
        let restarted = jobs_fingerprint(&[job("homebrew.mxcl.mysql", Some(11), None)], &known);
        assert_ne!(restarted, snapshot);
    }

    #[test]
    fn the_same_difference_refreshes_only_once() {
        let snapshot = Fingerprint::from([("mysql".to_string(), (Some(10), None))]);
        let now = Fingerprint::new();
        let (refresh, remember) = decide_refresh(&now, &snapshot, None);
        assert!(refresh);
        // Homebrew no lo ve igual que launchd: no se insiste en cada tick.
        let (again, remember) = decide_refresh(&now, &snapshot, remember.as_ref());
        assert!(!again);
        // Otro cambio sí vuelve a refrescar.
        let other = Fingerprint::from([("mysql".to_string(), (Some(12), None))]);
        assert!(decide_refresh(&other, &snapshot, remember.as_ref()).0);
        // Cuando coinciden, se olvida la huella.
        assert_eq!(decide_refresh(&snapshot, &snapshot, remember.as_ref()), (false, None));
    }

    #[test]
    fn events_are_classified_by_name() {
        let home = Path::new("/private/var/folders/x/home/.elephant-stack");
        assert_eq!(classify(&home.join("state.stamp")), Some(FsChange::Stamp));
        assert_eq!(classify(&home.join("state.stamp.tmp")), None);
        assert_eq!(classify(&home.join("settings.json")), None);
        assert_eq!(classify(&home.join("jobs")), Some(FsChange::JobsDir));
        assert_eq!(classify(&home.join("jobs/abc-1.json")), Some(FsChange::Jobs));
        assert_eq!(classify(&home.join("jobs/abc-1.log")), None);
        assert_eq!(classify(&home.join("logs/setup-1.log")), None);
        assert_eq!(classify(Path::new("/")), None);
    }

    #[test]
    fn the_apps_own_stamps_are_ignored() {
        let stamp = |pid| Stamp {
            at_ms: 1,
            what: "php".into(),
            pid,
        };
        assert!(!stamp_needs_refresh(Some(&stamp(42)), 42));
        assert!(stamp_needs_refresh(Some(&stamp(7)), 42));
        assert!(!stamp_needs_refresh(None, 42));
    }

    #[test]
    fn batches_group_events_and_end_when_the_channel_closes() {
        let (tx, rx) = mpsc::channel();
        tx.send(FsChange::Jobs).unwrap();
        tx.send(FsChange::Jobs).unwrap();
        tx.send(FsChange::Stamp).unwrap();
        let batch = next_batch(&rx, Duration::from_millis(20)).unwrap();
        assert_eq!(batch, BTreeSet::from([FsChange::Stamp, FsChange::Jobs]));
        drop(tx);
        assert_eq!(next_batch(&rx, Duration::from_millis(20)), None);
    }

    /// FSEvents de verdad sobre una carpeta temporal: un `state.stamp`
    /// escrito con `rename` (como `stamp::touch`) y una tarea nueva en `jobs/`.
    #[test]
    fn fsevents_report_stamps_and_jobs() {
        let root = std::env::temp_dir().join(format!("elephant-watch-{}-{}", std::process::id(), elephant_core::now_ms()));
        let dir = root.join(".elephant-stack");
        let jobs = dir.join("jobs");
        std::fs::create_dir_all(&jobs).unwrap();

        let (tx, rx) = mpsc::channel();
        let mut watcher = watcher(tx).unwrap();
        watcher.watch(&dir, RecursiveMode::NonRecursive).unwrap();
        watcher.watch(&jobs, RecursiveMode::NonRecursive).unwrap();
        // FSEvents tarda un momento en empezar a entregar.
        std::thread::sleep(Duration::from_millis(300));

        std::fs::write(dir.join("state.stamp.tmp"), "{}").unwrap();
        std::fs::rename(dir.join("state.stamp.tmp"), dir.join("state.stamp")).unwrap();
        std::fs::write(jobs.join("abcdefgh.json"), "{}").unwrap();
        std::fs::write(dir.join("otro.txt"), "x").unwrap();

        let mut seen = BTreeSet::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while seen.len() < 2 && Instant::now() < deadline {
            if let Ok(change) = rx.recv_timeout(Duration::from_millis(200)) {
                seen.insert(change);
            }
        }
        drop(watcher);
        let _ = std::fs::remove_dir_all(&root);
        assert!(seen.contains(&FsChange::Stamp), "{seen:?}");
        assert!(seen.contains(&FsChange::Jobs), "{seen:?}");
    }
}
