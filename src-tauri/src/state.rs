//! Estado de la app: un único `Snapshot` que comparten la ventana y el menú de
//! la barra. [`update`] es el único punto que lo cambia, sube la revisión,
//! emite `state:changed` y refresca el tray.

use std::sync::{Mutex, MutexGuard};

use elephant_core::brew::Service;
use elephant_core::components::{self, Component};
use elephant_core::jobs::{self, JobInfo};
use elephant_core::mailpit::{self, MailpitInfo};
use elephant_core::monitor::Usage;
use elephant_core::php::{self, PhpStatus};
use elephant_core::settings::{McpMode, Settings};
use elephant_core::i18n::{self, Locale};
use elephant_core::{paths, setup, tr};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::{commands, tray};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Snapshot {
    /// Crece en cada cambio: la interfaz descarta los antiguos.
    #[cfg_attr(test, ts(type = "number"))]
    pub revision: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub refreshed_at_ms: u64,
    pub components: Vec<Component>,
    pub php: PhpStatus,
    /// Lo que descubre `elephant-logs`, incluidos los sitios de Herd (C17).
    pub projects: Vec<elephant_logs::Project>,
    pub busy: Vec<Busy>,
    pub problems: Vec<Problem>,
    /// Tareas de agentes en curso (CLI o MCP).
    pub jobs: Vec<JobInfo>,
    pub logs: LogsBrief,
    pub mcp: McpBrief,
    pub mailpit: MailpitInfo,
    /// No existe `setup.json`: hay que mostrar el asistente (C8).
    pub setup_needed: bool,
    pub settings: Settings,
    /// El idioma ya resuelto (`settings.language`, con «Sistema» convertido):
    /// la interfaz y el backend escriben en el mismo.
    pub locale: Locale,
}

/// Un objetivo con una operación en curso: «Reiniciando…».
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Busy {
    pub target: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Problem {
    pub id: String,
    pub target: String,
    pub message: String,
    pub kind: ProblemKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum ProblemKind {
    /// Homebrew dice que el servicio ha fallado.
    ServiceError,
    /// Una acción pedida desde la app o el menú no ha salido bien.
    ActionFailed,
}

impl ProblemKind {
    fn slug(self) -> &'static str {
        match self {
            Self::ServiceError => "service_error",
            Self::ActionFailed => "action_failed",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct LogsBrief {
    /// Hay motor de logs.
    pub available: bool,
    /// Errores de la última hora posteriores a la última visita a Logs (C16).
    pub unseen_errors: u32,
    /// Los 5 proyectos con errores más recientes, como mucho.
    pub recent: Vec<LogProjectBrief>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct LogProjectBrief {
    pub id: String,
    pub name: String,
    pub errors: u32,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub last_ms: Option<i64>,
}

/// Resumen del canal de agentes (C26; conserva el nombre).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct McpBrief {
    /// Existe `~/.elephant-stack/bin/elephant`.
    pub linked: bool,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub last_used_ms: Option<i64>,
    pub mode: McpMode,
}

/// Carga de `nav:go`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct NavGo {
    pub route: String,
}

/// Carga de `ui:command`, que envía el menú de la aplicación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum UiCommand {
    Find,
    Refresh,
    ToggleSidebar,
    ToggleInspector,
    Live,
    ClearView,
}

struct Inner {
    snapshot: Snapshot,
    usage: Option<Vec<Usage>>,
    pending_route: Option<String>,
    /// La interfaz de la ventana ya escucha `nav:go`: lo dice al pedir la ruta
    /// pendiente (registra los eventos antes) y deja de ser cierto al
    /// destruirse la ventana.
    ui_listening: bool,
    refreshing: bool,
    refresh_again: bool,
}

pub struct AppState(Mutex<Inner>);

impl AppState {
    /// Solo lee `setup.json`: el refresco completo llega después, en segundo plano.
    pub fn new(settings: Settings) -> Self {
        let snapshot = Snapshot {
            revision: 0,
            refreshed_at_ms: 0,
            components: Vec::new(),
            php: PhpStatus::default(),
            projects: Vec::new(),
            busy: Vec::new(),
            problems: Vec::new(),
            jobs: Vec::new(),
            logs: LogsBrief::default(),
            mcp: McpBrief {
                mode: settings.mcp.mode,
                ..McpBrief::default()
            },
            mailpit: MailpitInfo::default(),
            setup_needed: setup::state::load().is_none(),
            settings,
            locale: i18n::locale(),
        };
        Self(Mutex::new(Inner {
            snapshot,
            usage: None,
            pending_route: None,
            ui_listening: false,
            refreshing: false,
            refresh_again: false,
        }))
    }

    /// Un hilo que entró en pánico con el cerrojo no debe dejar la app sin estado.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn snapshot(&self) -> Snapshot {
        self.lock().snapshot.clone()
    }

    pub fn settings(&self) -> Settings {
        self.lock().snapshot.settings.clone()
    }

    /// Memoria total de los servicios en marcha, según el último sondeo de la
    /// ventana. `None` si todavía no hay ninguno.
    pub fn usage_kb(&self) -> Option<u64> {
        self.lock()
            .usage
            .as_ref()
            .map(|usage| usage.iter().map(|u| u.memory_kb).sum())
    }

    pub fn set_usage(&self, usage: &[Usage]) {
        self.lock().usage = Some(usage.to_vec());
    }

    /// Para una ventana nueva: su interfaz aún no escucha.
    pub fn set_pending_route(&self, route: Option<String>) {
        let mut inner = self.lock();
        inner.pending_route = route;
        inner.ui_listening = false;
    }

    /// Con la ventana ya creada: si su interfaz escucha, devuelve la ruta para
    /// emitirla con `nav:go`; si aún está cargando, la guarda para cuando la
    /// pida. Las dos cosas bajo el mismo cerrojo que `take_pending_route`,
    /// así que la ruta no se pierde entre medias.
    pub fn route_for_open_window(&self, route: Option<String>) -> Option<String> {
        let mut inner = self.lock();
        if inner.ui_listening {
            return route;
        }
        if route.is_some() {
            inner.pending_route = route;
        }
        None
    }

    /// La interfaz la pide al montar, después de registrar `nav:go`.
    pub fn take_pending_route(&self) -> Option<String> {
        let mut inner = self.lock();
        inner.ui_listening = true;
        inner.pending_route.take()
    }

    pub fn window_destroyed(&self) {
        self.lock().ui_listening = false;
    }
}

fn now_ms() -> u64 {
    u64::try_from(elephant_core::now_ms()).unwrap_or_default()
}

/// Único punto que emite `state:changed` y llama a `tray::refresh`. Sube la revisión.
pub fn update(app: &AppHandle, f: impl FnOnce(&mut Snapshot)) -> Snapshot {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut inner = state.lock();
        f(&mut inner.snapshot);
        inner.snapshot.revision += 1;
        inner.snapshot.clone()
    };
    let _ = app.emit("state:changed", &snapshot);
    tray::refresh(app, &snapshot, state.usage_kb());
    snapshot
}

/// Bloqueante: vuelve a leer todo lo que no son logs. Si Homebrew falla, el
/// resto se actualiza igual y se devuelve el error.
pub fn refresh(app: &AppHandle) -> Result<Snapshot, String> {
    let home = paths::home()?;
    let components = components::list();
    let services: Vec<Service> = components
        .as_ref()
        .map(|list| list.iter().filter_map(|c| c.service.clone()).collect())
        .unwrap_or_default();
    let php = php::status().unwrap_or_default();
    let projects = elephant_logs::projects::discover(&home);
    let jobs = jobs::list_active();
    let mailpit = components.as_ref().ok().map(|_| mailpit::info(&services));
    let mcp = commands::mcp::brief();
    let setup_needed = setup::state::load().is_none();

    let error = components.as_ref().err().cloned();
    let snapshot = update(app, |s| {
        if let Ok(list) = components {
            s.problems.retain(|p| p.kind != ProblemKind::ServiceError);
            s.problems.extend(service_problems(&list));
            s.components = list;
        }
        if let Some(mailpit) = mailpit {
            s.mailpit = mailpit;
        }
        s.php = php;
        s.projects = projects;
        s.jobs = jobs;
        s.mcp = mcp;
        s.setup_needed = setup_needed;
        s.refreshed_at_ms = now_ms();
    });
    match error {
        Some(error) => Err(error),
        None => Ok(snapshot),
    }
}

/// Lanza un refresco en otro hilo. Si ya hay uno en marcha, se repite una
/// sola vez al terminar, así que varias peticiones seguidas se agrupan.
pub fn refresh_in_background(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if inner.refreshing {
            inner.refresh_again = true;
            return;
        }
        inner.refreshing = true;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            if let Err(error) = refresh(&app) {
                eprintln!("No se pudo refrescar el estado: {error}");
            }
            let state = app.state::<AppState>();
            let mut inner = state.lock();
            if inner.refresh_again {
                inner.refresh_again = false;
            } else {
                inner.refreshing = false;
                break;
            }
        }
    });
}

pub fn set_busy(app: &AppHandle, target: &str, label: &str) {
    update(app, |s| {
        s.busy.retain(|b| b.target != target);
        s.busy.push(Busy {
            target: target.to_string(),
            label: label.to_string(),
        });
    });
}

pub fn clear_busy(app: &AppHandle, target: &str) {
    update(app, |s| s.busy.retain(|b| b.target != target));
}

/// Un problema por objetivo y tipo: el nuevo sustituye al anterior.
pub fn add_problem(app: &AppHandle, target: &str, message: String, kind: ProblemKind) {
    update(app, |s| {
        let problem = Problem {
            id: format!("{}:{target}", kind.slug()),
            target: target.to_string(),
            message,
            kind,
        };
        s.problems.retain(|p| p.id != problem.id);
        s.problems.push(problem);
    });
}

pub fn clear_problems(app: &AppHandle, target: &str) {
    update(app, |s| s.problems.retain(|p| p.target != target));
}

/// Servicios que Homebrew marca como fallidos.
fn service_problems(components: &[Component]) -> Vec<Problem> {
    components
        .iter()
        .filter_map(|c| c.service.as_ref())
        .filter(|service| service.status == "error")
        .map(|service| Problem {
            id: format!("{}:{}", ProblemKind::ServiceError.slug(), service.name),
            target: service.name.clone(),
            message: match service.exit_code {
                Some(code) => tr!(
                    "{name} se ha detenido (código {code})",
                    "{name} stopped (code {code})",
                    name = service.name
                ),
                None => tr!("{name} se ha detenido", "{name} stopped", name = service.name),
            },
            kind: ProblemKind::ServiceError,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephant_core::components::Kind;

    fn component(name: &str, status: &str, exit_code: Option<i64>) -> Component {
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
                status: status.into(),
                running: status == "started",
                registered: true,
                pid: None,
                exit_code,
            }),
        }
    }

    #[test]
    fn failed_services_become_problems() {
        let problems = service_problems(&[
            component("mysql", "error", Some(1)),
            component("redis", "started", None),
            component("nginx", "error", None),
        ]);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].message, "mysql se ha detenido (código 1)");
        assert_eq!(problems[0].id, "service_error:mysql");
        assert_eq!(problems[1].message, "nginx se ha detenido");
        let english = i18n::with_locale(Locale::En, || service_problems(&[component("mysql", "error", Some(1))]));
        assert_eq!(english[0].message, "mysql stopped (code 1)");
    }

    #[test]
    fn the_snapshot_carries_the_resolved_locale() {
        let state = AppState::new(Settings::default());
        assert_eq!(state.snapshot().locale, Locale::Es);
        assert_eq!(serde_json::to_value(state.snapshot()).unwrap()["locale"], "es");
    }

    #[test]
    fn usage_is_unknown_until_the_first_poll() {
        let state = AppState::new(Settings::default());
        assert_eq!(state.usage_kb(), None);
        state.set_usage(&[
            Usage { pid: 1, memory_kb: 100, cpu_percent: 0.0, processes: 1 },
            Usage { pid: 2, memory_kb: 50, cpu_percent: 1.0, processes: 2 },
        ]);
        assert_eq!(state.usage_kb(), Some(150));
    }

    #[test]
    fn the_pending_route_is_taken_once() {
        let state = AppState::new(Settings::default());
        state.set_pending_route(Some("#/logs".into()));
        assert_eq!(state.take_pending_route().as_deref(), Some("#/logs"));
        assert_eq!(state.take_pending_route(), None);
    }

    #[test]
    fn a_route_for_a_window_that_is_still_loading_waits_for_its_ui() {
        let state = AppState::new(Settings::default());
        // Creada sin ruta (abrir la app); la interfaz todavía no escucha.
        state.set_pending_route(None);
        assert_eq!(state.route_for_open_window(Some("#/ajustes/general".into())), None);
        assert_eq!(state.take_pending_route().as_deref(), Some("#/ajustes/general"));
        // Ya escucha: la ruta va por `nav:go` y no se queda guardada.
        assert_eq!(state.route_for_open_window(Some("#/logs".into())).as_deref(), Some("#/logs"));
        assert_eq!(state.route_for_open_window(None), None);
        state.window_destroyed();
        assert_eq!(state.route_for_open_window(Some("#/panel".into())), None);
        assert_eq!(state.take_pending_route().as_deref(), Some("#/panel"));
    }

    #[test]
    fn ui_commands_are_kebab_case() {
        assert_eq!(serde_json::to_string(&UiCommand::ToggleSidebar).unwrap(), "\"toggle-sidebar\"");
        assert_eq!(serde_json::to_string(&UiCommand::ClearView).unwrap(), "\"clear-view\"");
    }
}
