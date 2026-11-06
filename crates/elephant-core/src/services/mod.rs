//! Servicios propios (06): binarios oficiales en
//! `~/.elephant-stack/services/<servicio>/<versión>`, datos por rama en
//! `~/.elephant-stack/data/<servicio>/<rama>` y un agente de launchd propio
//! por rama (`sh.elephant.<servicio>.<rama>`). En esta fase, solo MySQL.
//!
//! - [`catalog`] lee `services.json` (versiones, URL oficial y sha256).
//! - [`install`] descarga, comprueba y descomprime.
//! - Encender = `bootstrap` + `kickstart`, apagar = `bootout`. Tras encender
//!   se comprueba durante unos segundos que el proceso sigue vivo y que el
//!   puerto responde; si no, queda en `error` con la última línea útil de su
//!   log, y el agente se descarga para que launchd no lo reintente sin fin.
//! - Solo una rama de cada servicio en marcha: al encender otra, se apaga la
//!   anterior.
//!
//! Las funciones públicas toman el cerrojo del servicio y avisan con
//! [`stamp::touch`] al terminar bien.

use std::fs;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::launchd::{self, AgentPlist, LaunchJob};
use crate::setup::download::Tools;
use crate::setup::presets;
use crate::{fsutil, stamp, tr};

pub mod catalog;
mod install;
mod mysql;

pub use catalog::{ServiceRelease, ServicesCatalog, current_platform};
pub use mysql::ImportOffer;

/// Prefijo de las etiquetas de launchd de Elephant.
pub const LABEL_PREFIX: &str = "sh.elephant.";

/// Un servicio que Elephant sabe instalar y ejecutar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ServiceKind {
    Mysql,
}

impl ServiceKind {
    pub const ALL: [Self; 1] = [Self::Mysql];

    /// Nombre en el índice, en las rutas y en las etiquetas.
    pub fn id(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
        }
    }

    /// Nombre para las personas.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mysql => "MySQL",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Mysql => 3306,
        }
    }

    pub fn parse(id: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.id() == id)
            .ok_or_else(|| tr!("Servicio desconocido: {id}", "Unknown service: {id}"))
    }

    /// La rama de una versión, la que comparte los datos: `8.4` de `8.4.7`.
    pub fn branch_of(self, version: &str) -> String {
        match self {
            Self::Mysql => presets::branch_of(version),
        }
    }

    fn daemon(self, install: &Path) -> PathBuf {
        match self {
            Self::Mysql => install.join("bin/mysqld"),
        }
    }

    fn version_of(self, install: &Path) -> Option<String> {
        match self {
            Self::Mysql => mysql::version_of(install),
        }
    }

    /// Un archivo que solo existe en unos datos ya preparados.
    fn data_marker(self) -> &'static str {
        match self {
            Self::Mysql => "mysql.ibd",
        }
    }
}

/// Carpetas, herramientas y etiquetas con las que trabaja el supervisor. Los
/// tests usan un HOME temporal, otro puerto y el sufijo `.test`.
#[derive(Debug, Clone)]
pub(crate) struct Env {
    pub tools: Tools,
    pub agents_dir: PathBuf,
    pub label_suffix: String,
    pub port: Option<u16>,
    pub brew_prefix: PathBuf,
    /// Solo tests: si los datos de brew están en uso, sin mirar `ps`.
    pub brew_in_use: Option<bool>,
    pub start_timeout: Duration,
}

impl Env {
    pub(crate) fn system() -> Result<Self, String> {
        let tools = Tools::system()?;
        let brew_prefix = ["/opt/homebrew", "/usr/local"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.join("bin/brew").is_file())
            .unwrap_or_else(|| PathBuf::from("/opt/homebrew"));
        Ok(Self {
            agents_dir: tools.home.join("Library/LaunchAgents"),
            tools,
            label_suffix: String::new(),
            port: None,
            brew_prefix,
            brew_in_use: None,
            start_timeout: Duration::from_secs(30),
        })
    }

    fn data(&self) -> PathBuf {
        self.tools.data()
    }

    fn port(&self, kind: ServiceKind) -> u16 {
        self.port.unwrap_or_else(|| kind.default_port())
    }

    fn label(&self, kind: ServiceKind, branch: &str) -> String {
        format!("{LABEL_PREFIX}{}.{branch}{}", kind.id(), self.label_suffix)
    }

    fn plist_path(&self, label: &str) -> PathBuf {
        self.agents_dir.join(format!("{label}.plist"))
    }

    fn versions_dir(&self, kind: ServiceKind) -> PathBuf {
        self.data().join("services").join(kind.id())
    }

    fn install_dir(&self, kind: ServiceKind, version: &str) -> PathBuf {
        self.versions_dir(kind).join(version)
    }

    fn data_dir(&self, kind: ServiceKind, branch: &str) -> PathBuf {
        self.data().join("data").join(kind.id()).join(branch)
    }

    fn import_marker(&self, kind: ServiceKind, branch: &str) -> PathBuf {
        self.data().join("data").join(kind.id()).join(format!("{branch}.imported"))
    }

    fn run_dir(&self) -> PathBuf {
        self.data().join("run")
    }

    fn socket_path(&self, kind: ServiceKind, branch: &str) -> PathBuf {
        self.run_dir().join(format!("{}-{branch}.sock", kind.id()))
    }

    fn error_path(&self, kind: ServiceKind, branch: &str) -> PathBuf {
        self.run_dir().join(format!("{}-{branch}.error", kind.id()))
    }

    fn log_path(&self, kind: ServiceKind, branch: &str) -> PathBuf {
        self.data().join("logs").join(format!("{}-{branch}.err", kind.id()))
    }
}

// ── Estado ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum RunState {
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InstalledVersion {
    pub version: String,
    pub branch: String,
    pub path: String,
}

/// Una rama instalada: la ejecuta su versión más reciente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BranchStatus {
    pub branch: String,
    /// La versión que se ejecuta.
    pub version: String,
    /// Etiqueta de launchd.
    pub label: String,
    pub state: RunState,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub pid: Option<i64>,
    /// Arranca al iniciar sesión.
    pub autostart: bool,
    /// Motivo de `error`, ya traducido.
    pub error: Option<String>,
    pub data_dir: String,
    /// Los datos ya están preparados (el primer arranque los crea).
    pub data_ready: bool,
    pub log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ManagedService {
    pub service: ServiceKind,
    pub name: String,
    pub port: u16,
    /// De la más reciente a la más antigua.
    pub versions: Vec<InstalledVersion>,
    pub branches: Vec<BranchStatus>,
    /// Datos de Homebrew que se pueden copiar.
    pub import: Option<ImportOffer>,
}

fn installed_versions(env: &Env, kind: ServiceKind) -> Vec<InstalledVersion> {
    let Ok(entries) = fs::read_dir(env.versions_dir(kind)) else {
        return Vec::new();
    };
    let mut versions: Vec<InstalledVersion> = entries
        .flatten()
        .filter_map(|entry| {
            let version = entry.file_name().into_string().ok()?;
            (catalog::is_version(&version) && kind.daemon(&entry.path()).is_file()).then(|| InstalledVersion {
                branch: kind.branch_of(&version),
                path: entry.path().display().to_string(),
                version,
            })
        })
        .collect();
    versions.sort_by_key(|v| std::cmp::Reverse(presets::version_key(&v.version)));
    versions
}

/// La versión que ejecuta una rama: la más reciente instalada.
fn version_for(env: &Env, kind: ServiceKind, branch: &str) -> Option<String> {
    installed_versions(env, kind)
        .into_iter()
        .find(|v| v.branch == branch)
        .map(|v| v.version)
}

fn branches_of(versions: &[InstalledVersion]) -> Vec<String> {
    let mut branches: Vec<String> = Vec::new();
    for v in versions {
        if !branches.contains(&v.branch) {
            branches.push(v.branch.clone());
        }
    }
    branches
}

/// Desde `offset`, como mucho los últimos 64 KB.
fn read_from(path: &Path, offset: u64) -> String {
    let Ok(mut file) = fs::File::open(path) else {
        return String::new();
    };
    let len = file.metadata().map_or(0, |m| m.len());
    let start = offset.max(len.saturating_sub(64 * 1024));
    use std::io::Seek;
    if file.seek(std::io::SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// La última línea `[ERROR]` que explica algo (no el «Aborting» final) o, si
/// no hay, la última que no está vacía, sin la marca de tiempo ni el hilo.
pub(crate) fn last_useful_line(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let line = lines
        .iter()
        .rev()
        .find(|l| l.contains("[ERROR]") && !l.ends_with("Aborting"))
        .or_else(|| lines.iter().rev().find(|l| l.contains("[ERROR]")))
        .or_else(|| lines.last())?;
    let mut rest = *line;
    if let Some((first, after)) = rest.split_once(' ')
        && first.len() >= 19
        && first.as_bytes()[4] == b'-'
        && first.contains('T')
    {
        rest = after.trim_start();
        if let Some((thread, after)) = rest.split_once(' ')
            && thread.bytes().all(|b| b.is_ascii_digit())
        {
            rest = after.trim_start();
        }
    }
    Some(rest.chars().take(300).collect())
}

fn branch_status(env: &Env, kind: ServiceKind, branch: &str, version: &str, jobs: &[LaunchJob]) -> BranchStatus {
    let label = env.label(kind, branch);
    let job = jobs.iter().find(|job| job.label == label);
    let autostart = fs::read_to_string(env.plist_path(&label)).is_ok_and(|xml| AgentPlist::run_at_load_in(&xml));
    let saved_error = fs::read_to_string(env.error_path(kind, branch)).ok().filter(|t| !t.trim().is_empty());
    let log = env.log_path(kind, branch);
    let name = kind.name();
    let (state, error) = match job {
        Some(LaunchJob { pid: Some(_), .. }) => (RunState::Running, None),
        _ if saved_error.is_some() => (RunState::Error, saved_error.map(|t| t.trim().to_string())),
        // Cargado, sin proceso y con un código de error: se ha caído y launchd lo reintenta.
        Some(LaunchJob { pid: None, last_exit: Some(code), .. }) if *code != 0 => {
            let detail = last_useful_line(&read_from(&log, 0)).unwrap_or_default();
            (
                RunState::Error,
                Some(tr!(
                    "{name} {branch} se ha detenido (código {code}). {detail}",
                    "{name} {branch} stopped (code {code}). {detail}"
                )),
            )
        }
        _ => (RunState::Stopped, None),
    };
    let data_dir = env.data_dir(kind, branch);
    BranchStatus {
        branch: branch.to_string(),
        version: version.to_string(),
        state,
        pid: job.and_then(|job| job.pid),
        autostart,
        error,
        data_ready: data_dir.join(kind.data_marker()).exists(),
        data_dir: data_dir.display().to_string(),
        log_path: log.display().to_string(),
        label,
    }
}

fn status_in(env: &Env, jobs: &[LaunchJob]) -> Vec<ManagedService> {
    ServiceKind::ALL
        .into_iter()
        .map(|kind| {
            let versions = installed_versions(env, kind);
            let branches = branches_of(&versions)
                .into_iter()
                .filter_map(|branch| {
                    let version = versions.iter().find(|v| v.branch == branch)?.version.clone();
                    Some(branch_status(env, kind, &branch, &version, jobs))
                })
                .collect();
            ManagedService {
                service: kind,
                name: kind.name().to_string(),
                port: env.port(kind),
                versions,
                branches,
                import: match kind {
                    ServiceKind::Mysql => mysql::brew_offer(env),
                },
            }
        })
        .collect()
}

/// Lo instalado, su estado y lo que se puede importar. Solo lee: `launchctl
/// list`, carpetas y, si hay datos de brew, `ps`.
pub fn status() -> Vec<ManagedService> {
    let Ok(env) = Env::system() else {
        return Vec::new();
    };
    let jobs = launchd::jobs_with_prefix(LABEL_PREFIX).unwrap_or_default();
    status_in(&env, &jobs)
}

/// El log de una etiqueta de Elephant (`sh.elephant.mysql.8.4`), si existe.
pub fn log_for_label(label: &str) -> Option<PathBuf> {
    status()
        .into_iter()
        .flat_map(|s| s.branches)
        .find(|b| b.label == label)
        .map(|b| PathBuf::from(b.log_path))
        .filter(|path| path.is_file())
}

// ── Catálogo e instalación ───────────────────────────────────────────────────

/// Versiones de esta plataforma. Una hora en memoria y copia en disco.
pub fn catalog(force_refresh: bool) -> ServicesCatalog {
    catalog::catalog(force_refresh)
}

fn release_for(kind: ServiceKind, version: &str) -> Result<ServiceRelease, String> {
    let name = kind.name();
    catalog(false)
        .find(kind, version)
        .cloned()
        .ok_or_else(|| tr!("{name} {version} no está en el catálogo de Elephant.", "{name} {version} isn’t in Elephant’s catalog."))
}

/// Descarga e instala una versión del catálogo. Se pueden tener varias.
pub fn install(
    kind: ServiceKind,
    version: &str,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let env = Env::system()?;
    let release = release_for(kind, version)?;
    install::install(&env, &release, on_line, on_bytes)?;
    stamp::touch("services");
    Ok(())
}

/// Cerrojo de un servicio entre la app, la CLI y las tareas: nunca se
/// encienden o se borran dos cosas del mismo servicio a la vez.
fn lock(env: &Env, kind: ServiceKind) -> Result<fs::File, String> {
    let path = env.data().join("locks").join(format!("{}.lock", kind.id()));
    download::dir_for(&path)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|error| tr!("No se pudo abrir el cerrojo: {error}", "Couldn’t open the lock: {error}"))?;
    file.lock()
        .map_err(|error| tr!("No se pudo tomar el cerrojo: {error}", "Couldn’t take the lock: {error}"))?;
    Ok(file)
}

use crate::setup::download;

fn job(label: &str) -> Option<LaunchJob> {
    launchd::jobs_with_prefix(label)
        .ok()?
        .into_iter()
        .find(|job| job.label == label)
}

fn is_running(label: &str) -> bool {
    job(label).is_some_and(|job| job.pid.is_some())
}

/// Borra una versión instalada. Los datos de su rama no se tocan. Si era la
/// última de su rama, también se quita su agente de launchd.
pub fn uninstall(kind: ServiceKind, version: &str) -> Result<(), String> {
    let env = Env::system()?;
    let _lock = lock(&env, kind)?;
    uninstall_in(&env, kind, version)?;
    stamp::touch("services");
    Ok(())
}

fn uninstall_in(env: &Env, kind: ServiceKind, version: &str) -> Result<(), String> {
    let name = kind.name();
    let branch = kind.branch_of(version);
    let label = env.label(kind, &branch);
    if !catalog::is_version(version) || !installed_versions(env, kind).iter().any(|v| v.version == version) {
        return Err(tr!("{name} {version} no está instalado.", "{name} {version} isn’t installed."));
    }
    if version_for(env, kind, &branch).as_deref() == Some(version) && is_running(&label) {
        return Err(tr!(
            "{name} {branch} está en marcha: apágalo antes de desinstalar {name} {version}.",
            "{name} {branch} is running. Turn it off before uninstalling {name} {version}."
        ));
    }
    let root = env.versions_dir(kind);
    let target = root.join(version);
    match (root.canonicalize(), target.canonicalize()) {
        (Ok(base), Ok(resolved)) if resolved.parent() == Some(base.as_path()) => fs::remove_dir_all(&resolved)
            .map_err(|error| tr!("No se pudo borrar {name} {version}: {error}", "Couldn’t delete {name} {version}: {error}"))?,
        _ => return Err(tr!("No se encuentra la carpeta de {name} {version}.", "Couldn’t find the {name} {version} folder.")),
    }
    match version_for(env, kind, &branch) {
        None => {
            launchd::bootout(&label)?;
            let _ = fs::remove_file(env.plist_path(&label));
            let _ = fs::remove_file(env.error_path(kind, &branch));
        }
        Some(_) if env.plist_path(&label).exists() => {
            let autostart = fs::read_to_string(env.plist_path(&label)).is_ok_and(|x| AgentPlist::run_at_load_in(&x));
            write_plist(env, kind, &branch, autostart)?;
        }
        Some(_) => {}
    }
    Ok(())
}

/// Resultado de actualizar una versión a otra más reciente de su rama.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UpdateReport {
    pub from: String,
    pub to: String,
    /// Estaba en marcha y se ha vuelto a encender con la versión nueva.
    pub restarted: bool,
}

/// Descarga `to` (un parche más reciente de la rama de `from`), la pone en
/// marcha si la rama lo estaba y, solo si todo ha ido bien, borra `from`.
pub fn update(
    kind: ServiceKind,
    from: &str,
    to: &str,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<UpdateReport, String> {
    let env = Env::system()?;
    let name = kind.name();
    let branch = kind.branch_of(from);
    let newer = presets::version_key(to) > presets::version_key(from);
    if kind.branch_of(to) != branch || !newer {
        return Err(tr!(
            "{name} {to} no es una versión más reciente de la rama de {name} {from}.",
            "{name} {to} isn’t a newer version in the {name} {from} branch."
        ));
    }
    if !installed_versions(&env, kind).iter().any(|v| v.version == from) {
        return Err(tr!("{name} {from} no está instalado.", "{name} {from} isn’t installed."));
    }
    let release = release_for(kind, to)?;
    install::install(&env, &release, on_line, on_bytes)?;
    let _lock = lock(&env, kind)?;
    let label = env.label(kind, &branch);
    let restarted = is_running(&label);
    if restarted {
        on_line(&tr!("Reiniciando {name} {branch} con {to}", "Restarting {name} {branch} with {to}"));
        start_in(&env, kind, &branch, on_line)?;
    }
    uninstall_in(&env, kind, from)?;
    stamp::touch("services");
    Ok(UpdateReport { from: from.to_string(), to: to.to_string(), restarted })
}

// ── Encender y apagar ────────────────────────────────────────────────────────

/// Resultado de encender una rama.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StartReport {
    /// Otras ramas del mismo servicio que se han apagado (`MySQL 9.7`).
    pub stopped: Vec<String>,
    /// Se han preparado los datos por primera vez.
    pub initialized: bool,
}

fn port_open(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Quien no puede escribir en sus datos no puede arrancar: es el caso de
/// `/opt/homebrew/var/mysql` con otro dueño, en el que `mysqld_safe` salía
/// con 0 sin decir nada.
pub(crate) fn check_owned_and_writable(dir: &Path) -> Result<(), String> {
    check_owner(dir)?;
    let shown = dir.display().to_string();
    let probe = dir.join(".elephant-write-test");
    fs::write(&probe, b"").map_err(|error| {
        tr!("No se puede escribir en {shown}: {error}", "Can’t write to {shown}: {error}")
    })?;
    let _ = fs::remove_file(probe);
    Ok(())
}

/// Solo lee: `dir` y lo que contiene son del usuario actual.
pub(crate) fn check_owner(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let me = fsutil::current_uid();
    let shown = dir.display().to_string();
    let not_mine = |path: &Path, uid: u32| {
        let owner = owner_name(uid);
        let path = path.display();
        tr!(
            "{path} es de otro usuario ({owner}): Elephant no puede usar esos datos. Cambia su dueño o importa una copia.",
            "{path} belongs to another user ({owner}), so Elephant can’t use that data. Change its owner or import a copy."
        )
    };
    let meta = fs::metadata(dir).map_err(|error| tr!("No se puede leer {shown}: {error}", "Can’t read {shown}: {error}"))?;
    if meta.uid() != me {
        return Err(not_mine(dir, meta.uid()));
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = fs::symlink_metadata(entry.path())
                && meta.uid() != me
            {
                return Err(not_mine(&entry.path(), meta.uid()));
            }
        }
    }
    Ok(())
}

fn owner_name(uid: u32) -> String {
    std::process::Command::new("/usr/bin/id")
        .args(["-nu", &uid.to_string()])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("uid {uid}"))
}

/// Deja la carpeta de datos lista: comprueba que es del usuario y, si está
/// vacía o no existe, la prepara. Devuelve si la ha preparado ahora.
fn prepare_data(env: &Env, kind: ServiceKind, install: &Path, branch: &str, on_line: &mut dyn FnMut(&str)) -> Result<bool, String> {
    let dir = env.data_dir(kind, branch);
    for needed in [env.run_dir(), env.log_path(kind, branch).parent().map(Path::to_path_buf).unwrap_or_default()] {
        fs::create_dir_all(&needed).map_err(|error| {
            let dir = needed.display();
            tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
        })?;
    }
    let empty = match fs::read_dir(&dir) {
        Ok(mut entries) => {
            check_owned_and_writable(&dir)?;
            entries.next().is_none()
        }
        Err(_) => {
            if let Some(parent) = dir.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    let dir = parent.display();
                    tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
                })?;
                check_owned_and_writable(parent)?;
            }
            true
        }
    };
    if !empty {
        return Ok(false);
    }
    match kind {
        ServiceKind::Mysql => mysql::initialize(env, install, branch, on_line)?,
    }
    Ok(true)
}

fn program(env: &Env, kind: ServiceKind, install: &Path, branch: &str) -> Vec<String> {
    match kind {
        ServiceKind::Mysql => mysql::daemon_args(env, install, branch),
    }
}

fn plist_for(env: &Env, kind: ServiceKind, branch: &str, autostart: bool) -> Result<AgentPlist, String> {
    let name = kind.name();
    let version = version_for(env, kind, branch)
        .ok_or_else(|| tr!("{name} {branch} no está instalado.", "{name} {branch} isn’t installed."))?;
    Ok(AgentPlist {
        label: env.label(kind, branch),
        program: program(env, kind, &env.install_dir(kind, &version), branch),
        log: env.log_path(kind, branch).display().to_string(),
        run_at_load: autostart,
    })
}

fn write_plist(env: &Env, kind: ServiceKind, branch: &str, autostart: bool) -> Result<PathBuf, String> {
    let plist = plist_for(env, kind, branch, autostart)?;
    let path = env.plist_path(&plist.label);
    fsutil::write_atomic(&path, plist.to_xml().as_bytes())?;
    Ok(path)
}

/// Lo que se ve de un agente mientras arranca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    /// Proceso vivo y el puerto acepta conexiones.
    Up,
    /// Todavía no responde.
    Waiting,
    /// El proceso ha terminado (con su código) o launchd ya no lo tiene.
    Exited(Option<i64>),
}

pub(crate) fn probe(job: Option<&LaunchJob>, port_open: bool) -> Probe {
    match job {
        None => Probe::Exited(None),
        Some(LaunchJob { pid: Some(_), .. }) if port_open => Probe::Up,
        Some(LaunchJob { pid: Some(_), .. }) => Probe::Waiting,
        Some(LaunchJob { pid: None, last_exit: Some(code), .. }) => Probe::Exited(Some(*code)),
        Some(LaunchJob { pid: None, last_exit: None, .. }) => Probe::Waiting,
    }
}

/// Espera a que el proceso esté vivo y el puerto responda. Si termina antes o
/// se agota el tiempo, devuelve el motivo con la última línea útil del log.
fn wait_started(env: &Env, kind: ServiceKind, branch: &str, label: &str, log_offset: u64) -> Result<(), String> {
    let port = env.port(kind);
    let deadline = Instant::now() + env.start_timeout;
    let name = kind.name();
    let reason = loop {
        let job = job(label);
        match probe(job.as_ref(), port_open(port)) {
            Probe::Up => return Ok(()),
            Probe::Exited(code) => {
                let code = code.map_or_else(|| "?".to_string(), |c| c.to_string());
                break tr!("ha terminado al arrancar (código {code})", "exited while starting (code {code})");
            }
            Probe::Waiting if Instant::now() >= deadline => {
                let secs = env.start_timeout.as_secs();
                break tr!("no responde en el puerto {port} tras {secs} s", "isn’t answering on port {port} after {secs} s");
            }
            Probe::Waiting => std::thread::sleep(Duration::from_millis(250)),
        }
    };
    let log = env.log_path(kind, branch);
    let detail = last_useful_line(&read_from(&log, log_offset))
        .map(|line| format!(": {line}"))
        .unwrap_or_default();
    Err(tr!(
        "{name} {branch} no ha arrancado: {reason}{detail}",
        "{name} {branch} didn’t start: {reason}{detail}"
    ))
}

fn start_in(env: &Env, kind: ServiceKind, branch: &str, on_line: &mut dyn FnMut(&str)) -> Result<StartReport, String> {
    let name = kind.name();
    let version = version_for(env, kind, branch)
        .ok_or_else(|| tr!("{name} {branch} no está instalado.", "{name} {branch} isn’t installed."))?;
    let label = env.label(kind, branch);
    let mut report = StartReport::default();

    // Una sola rama de cada servicio: comparten el puerto.
    for other in branches_of(&installed_versions(env, kind)) {
        let other_label = env.label(kind, &other);
        if other != branch && launchd::is_loaded(&other_label) {
            let was_running = is_running(&other_label);
            launchd::bootout(&other_label)?;
            let _ = fs::remove_file(env.error_path(kind, &other));
            if was_running {
                report.stopped.push(format!("{name} {other}"));
            }
        }
    }
    let _ = fs::remove_file(env.error_path(kind, branch));

    let result = (|| {
        let autostart = fs::read_to_string(env.plist_path(&label)).is_ok_and(|x| AgentPlist::run_at_load_in(&x));
        let plist = plist_for(env, kind, branch, autostart)?;
        let xml = plist.to_xml();
        let path = env.plist_path(&label);
        let unchanged = fs::read_to_string(&path).is_ok_and(|old| old == xml);
        if unchanged && is_running(&label) {
            return Ok(());
        }
        if launchd::is_loaded(&label) {
            launchd::bootout(&label)?;
        }
        wait_port_free(env.port(kind));
        let port = env.port(kind);
        let socket = env.socket_path(kind, branch);
        if socket.as_os_str().len() > 103 {
            let path = socket.display();
            return Err(tr!("La ruta del socket es demasiado larga: {path}", "The socket path is too long: {path}"));
        }
        if port_open(port) {
            return Err(tr!(
                "El puerto {port} ya está en uso (¿otro MySQL, como el de Homebrew?). Detenlo y vuelve a intentarlo.",
                "Port {port} is already in use (another MySQL, like Homebrew’s?). Stop it and try again."
            ));
        }
        report.initialized = prepare_data(env, kind, &env.install_dir(kind, &version), branch, on_line)?;
        fsutil::write_atomic(&path, xml.as_bytes())?;
        let offset = fs::metadata(env.log_path(kind, branch)).map_or(0, |m| m.len());
        on_line(&tr!("Arrancando {name} {version}", "Starting {name} {version}"));
        launchd::bootstrap(&path)?;
        launchd::kickstart(&label)?;
        wait_started(env, kind, branch, &label, offset)
    })();
    if let Err(error) = &result {
        // Sin esto, KeepAlive lo reintentaría cada 10 s sin fin.
        let _ = launchd::bootout(&label);
        let _ = fsutil::write_atomic(&env.error_path(kind, branch), error.as_bytes());
    }
    result.map(|()| report)
}

/// Tras un `bootout`, el proceso puede tardar unos segundos en soltar el puerto.
fn wait_port_free(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while port_open(port) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn stop_in(env: &Env, kind: ServiceKind, branch: &str) -> Result<(), String> {
    let label = env.label(kind, branch);
    launchd::bootout(&label)?;
    let _ = fs::remove_file(env.error_path(kind, branch));
    Ok(())
}

/// Enciende (y comprueba que arranca) o apaga una rama. Encender una apaga
/// las demás del mismo servicio. No cambia el arranque al iniciar sesión.
pub fn set_running(kind: ServiceKind, branch: &str, on: bool, on_line: &mut dyn FnMut(&str)) -> Result<StartReport, String> {
    let env = Env::system()?;
    let _lock = lock(&env, kind)?;
    let result = if on {
        start_in(&env, kind, branch, on_line)
    } else {
        stop_in(&env, kind, branch).map(|()| StartReport::default())
    };
    // También un fallo cambia el estado (pasa a `error`).
    stamp::touch("services");
    result
}

/// Arranque al iniciar sesión (`RunAtLoad`). Solo una rama de cada servicio
/// lo tiene: activarlo en una lo quita de las demás. No enciende ni apaga.
pub fn set_autostart(kind: ServiceKind, branch: &str, on: bool) -> Result<(), String> {
    let env = Env::system()?;
    let _lock = lock(&env, kind)?;
    set_autostart_in(&env, kind, branch, on, &mut |_| {})?;
    stamp::touch("services");
    Ok(())
}

fn set_autostart_in(env: &Env, kind: ServiceKind, branch: &str, on: bool, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    if on {
        for other in branches_of(&installed_versions(env, kind)) {
            if other != branch && env.plist_path(&env.label(kind, &other)).exists() {
                write_plist(env, kind, &other, false)?;
            }
        }
        // Al iniciar sesión no hay nadie que prepare los datos: se hace ahora.
        let name = kind.name();
        let version = version_for(env, kind, branch)
            .ok_or_else(|| tr!("{name} {branch} no está instalado.", "{name} {branch} isn’t installed."))?;
        prepare_data(env, kind, &env.install_dir(kind, &version), branch, on_line)?;
    }
    write_plist(env, kind, branch, on).map(drop)
}

// ── Importar datos ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImportReport {
    pub source: String,
    pub target: String,
    /// Dónde han quedado los datos que tenía la rama, si tenía.
    pub backup: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
}

/// Copia (nunca mueve) los datos de Homebrew a la rama de su misma versión,
/// que tiene que estar instalada y apagada. Si la rama ya tenía datos, se
/// guardan al lado (`8.4.pre-import-<fecha>`). El original no se toca.
pub fn import_brew_data(kind: ServiceKind, on_line: &mut dyn FnMut(&str), on_bytes: &mut dyn FnMut(u64, u64)) -> Result<ImportReport, String> {
    let env = Env::system()?;
    let _lock = lock(&env, kind)?;
    let report = import_in(&env, kind, on_line, on_bytes)?;
    stamp::touch("services");
    Ok(report)
}

fn import_in(env: &Env, kind: ServiceKind, on_line: &mut dyn FnMut(&str), on_bytes: &mut dyn FnMut(u64, u64)) -> Result<ImportReport, String> {
    let name = kind.name();
    let offer = match kind {
        ServiceKind::Mysql => mysql::brew_offer(env),
    }
    .ok_or_else(|| tr!("No hay datos de {name} de Homebrew que copiar.", "There’s no Homebrew {name} data to copy."))?;
    if let Some(blocked) = offer.blocked {
        return Err(blocked);
    }
    let branch = offer.branch.as_str();
    if version_for(env, kind, branch).is_none() {
        return Err(tr!(
            "Instala {name} {branch} para copiar estos datos: son de {name} {version}.",
            "Install {name} {branch} to copy this data. It’s from {name} {version}.",
            version = offer.version
        ));
    }
    if is_running(&env.label(kind, branch)) {
        return Err(tr!(
            "{name} {branch} está en marcha: apágalo antes de copiar los datos.",
            "{name} {branch} is running. Turn it off before copying the data."
        ));
    }
    let source = PathBuf::from(&offer.source);
    let target = env.data_dir(kind, branch);
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    fs::create_dir_all(&parent).map_err(|error| {
        let dir = parent.display();
        tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
    })?;
    let tmp = parent.join(format!(".import-{branch}"));
    let _ = fs::remove_dir_all(&tmp);
    let from = source.display().to_string();
    on_line(&tr!("Copiando {from}", "Copying {from}"));
    let mut done = 0;
    let total = offer.size;
    if let Err(error) = mysql::copy_tree(&source, &tmp, &mut done, &mut |bytes| on_bytes(bytes, total.max(bytes))) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(error);
    }
    let backup = if target.exists() {
        let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
        let backup = parent.join(format!("{branch}.pre-import-{stamp}"));
        fs::rename(&target, &backup).map_err(|error| {
            let _ = fs::remove_dir_all(&tmp);
            tr!("No se pudieron apartar los datos actuales: {error}", "Couldn’t set the current data aside: {error}")
        })?;
        Some(backup.display().to_string())
    } else {
        None
    };
    fs::rename(&tmp, &target).map_err(|error| tr!("No se pudieron colocar los datos: {error}", "Couldn’t put the data in place: {error}"))?;
    let _ = fs::write(env.import_marker(kind, branch), format!("{from}\n"));
    let _ = fs::remove_file(env.error_path(kind, branch));
    Ok(ImportReport {
        source: from,
        target: target.display().to_string(),
        backup,
        bytes: done,
    })
}

#[cfg(test)]
mod tests;
