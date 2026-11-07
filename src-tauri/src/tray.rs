//! Icono y menú de la barra de menús (01 §9 y 00 §5.10).
//!
//! El menú se describe con [`entries`], que es pura: a partir del `Snapshot`
//! devuelve un árbol de [`Entry`]. [`refresh`] lo aplica al menú nativo en el
//! hilo principal: si la estructura ([`shape`]) no cambia, solo actualiza en
//! sitio los textos, los estados y las marcas que difieren ([`diff`]); si
//! cambia, construye un menú nuevo y lo pone con `set_menu`. Así no parpadea y
//! funciona aunque el menú esté abierto.
//!
//! Los eventos de menú no se escuchan aquí: los despacha `lib.rs` (C25) y
//! llegan a [`handle`].

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Mutex, MutexGuard, OnceLock};

use elephant_core::brew::Operation;
use elephant_core::jobs::JobInfo;
use elephant_core::services::{RunState, ServiceKind};
use elephant_core::{tr, tr_str};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::actions::{self, SiteAction};
use crate::state::{self, AppState, ProblemKind, Snapshot};
use crate::{commands, watch, window};

pub const TRAY_ID: &str = "main";

/// Sitios que caben en el submenú; el resto, en «Ver todos…».
const MAX_SITES: usize = 15;

const TRAY_PNG: &[u8] = include_bytes!("../icons/tray.png");

// ---------------------------------------------------------------------------
// Acciones
// ---------------------------------------------------------------------------

/// Lo que hace cada elemento del menú. Su texto (`Display`) es el id del
/// elemento y `FromStr` es su inverso exacto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    /// `open` · `nav:#/logs?level=error`
    Open { route: Option<String> },
    /// `php:default:8.4@Herd` (la `key` de `PhpStatus`).
    PhpDefault(String),
    /// `svc:restart:mysql`
    Service(String, Operation),
    /// `svc-all:restart` · `svc-all:turn_off`
    ServicesAll(Operation),
    /// `svc-log:mysql`
    ServiceLog(String),
    /// `own:on:mysql:9.7` · `own:off:postgresql:18`: una rama de un servicio
    /// propio (06), encender o apagar.
    Managed(ServiceKind, String, bool),
    /// `site:browser:<project_id>` · `editor` · `terminal` · `copy`
    Site(String, SiteAction),
    /// `site-needs:<project_id>`: «Arrancar lo que necesita» (04 §4.10).
    SiteNeeds(String),
    /// `mailpit:open`
    OpenMailpit,
    /// `quit`
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseActionError;

impl fmt::Display for ParseActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("id de menú desconocido")
    }
}

impl std::error::Error for ParseActionError {}

fn operation_slug(op: Operation) -> &'static str {
    match op {
        Operation::TurnOn => "turn_on",
        Operation::TurnOff => "turn_off",
        Operation::Restart => "restart",
        Operation::EnableAutostart => "enable_autostart",
        Operation::DisableAutostart => "disable_autostart",
        Operation::TurnOnWithAutostart => "turn_on_with_autostart",
    }
}

fn parse_operation(slug: &str) -> Option<Operation> {
    Some(match slug {
        "turn_on" => Operation::TurnOn,
        "turn_off" => Operation::TurnOff,
        "restart" => Operation::Restart,
        "enable_autostart" => Operation::EnableAutostart,
        "disable_autostart" => Operation::DisableAutostart,
        "turn_on_with_autostart" => Operation::TurnOnWithAutostart,
        _ => return None,
    })
}

fn site_slug(action: SiteAction) -> &'static str {
    match action {
        SiteAction::Browser => "browser",
        SiteAction::Editor => "editor",
        SiteAction::Terminal => "terminal",
        SiteAction::CopyUrl => "copy",
    }
}

fn parse_site_action(slug: &str) -> Option<SiteAction> {
    Some(match slug {
        "browser" => SiteAction::Browser,
        "editor" => SiteAction::Editor,
        "terminal" => SiteAction::Terminal,
        "copy" => SiteAction::CopyUrl,
        _ => return None,
    })
}

impl fmt::Display for TrayAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { route: None } => f.write_str("open"),
            Self::Open { route: Some(route) } => write!(f, "nav:{route}"),
            Self::PhpDefault(key) => write!(f, "php:default:{key}"),
            Self::Service(name, op) => write!(f, "svc:{}:{name}", operation_slug(*op)),
            Self::ServicesAll(op) => write!(f, "svc-all:{}", operation_slug(*op)),
            Self::ServiceLog(name) => write!(f, "svc-log:{name}"),
            Self::Managed(kind, branch, on) => write!(f, "own:{}:{}:{branch}", if *on { "on" } else { "off" }, kind.id()),
            Self::Site(id, action) => write!(f, "site:{}:{id}", site_slug(*action)),
            Self::SiteNeeds(id) => write!(f, "site-needs:{id}"),
            Self::OpenMailpit => f.write_str("mailpit:open"),
            Self::Quit => f.write_str("quit"),
        }
    }
}

impl FromStr for TrayAction {
    type Err = ParseActionError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_string()).ok_or(ParseActionError);
        match id {
            "open" => return Ok(Self::Open { route: None }),
            "mailpit:open" => return Ok(Self::OpenMailpit),
            "quit" => return Ok(Self::Quit),
            _ => {}
        }
        // `splitn(2, ':')`: la ruta puede llevar dos puntos.
        let mut parts = id.splitn(2, ':');
        let (Some(prefix), Some(rest)) = (parts.next(), parts.next()) else {
            return Err(ParseActionError);
        };
        match prefix {
            "nav" => Ok(Self::Open {
                route: Some(non_empty(rest)?),
            }),
            "php" => {
                let key = rest.strip_prefix("default:").ok_or(ParseActionError)?;
                Ok(Self::PhpDefault(non_empty(key)?))
            }
            "svc" => {
                let (op, name) = rest.split_once(':').ok_or(ParseActionError)?;
                let op = parse_operation(op).ok_or(ParseActionError)?;
                Ok(Self::Service(non_empty(name)?, op))
            }
            // Homebrew solo sabe hacerlo en bloque con estas dos (ops::services_bulk).
            "svc-all" => match parse_operation(rest) {
                Some(op @ (Operation::Restart | Operation::TurnOff)) => Ok(Self::ServicesAll(op)),
                _ => Err(ParseActionError),
            },
            "svc-log" => Ok(Self::ServiceLog(non_empty(rest)?)),
            "own" => {
                let (on, rest) = rest.split_once(':').ok_or(ParseActionError)?;
                let on = match on {
                    "on" => true,
                    "off" => false,
                    _ => return Err(ParseActionError),
                };
                let (kind, branch) = rest.split_once(':').ok_or(ParseActionError)?;
                let kind = ServiceKind::parse(kind).map_err(|_| ParseActionError)?;
                Ok(Self::Managed(kind, non_empty(branch)?, on))
            }
            "site" => {
                let (action, project) = rest.split_once(':').ok_or(ParseActionError)?;
                let action = parse_site_action(action).ok_or(ParseActionError)?;
                Ok(Self::Site(non_empty(project)?, action))
            }
            "site-needs" => Ok(Self::SiteNeeds(non_empty(rest)?)),
            _ => Err(ParseActionError),
        }
    }
}

// ---------------------------------------------------------------------------
// Descripción del menú (pura)
// ---------------------------------------------------------------------------

/// Un elemento del menú. Con `children` es un submenú; con `separator`, una
/// línea. Sin `id` es informativo.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: Option<TrayAction>,
    pub text: String,
    pub enabled: bool,
    /// `Some` para los elementos con marca (versión de PHP, «Al iniciar sesión»).
    pub checked: Option<bool>,
    pub accel: Option<&'static str>,
    pub children: Vec<Entry>,
    pub separator: bool,
}

impl Entry {
    fn new(id: Option<TrayAction>, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            enabled: true,
            checked: None,
            accel: None,
            children: Vec::new(),
            separator: false,
        }
    }

    fn item(id: TrayAction, text: impl Into<String>) -> Self {
        Self::new(Some(id), text)
    }

    fn nav(route: &str, text: impl Into<String>) -> Self {
        Self::item(
            TrayAction::Open {
                route: Some(route.to_string()),
            },
            text,
        )
    }

    fn info(text: impl Into<String>) -> Self {
        Self::new(None, text).enabled(false)
    }

    fn check(id: TrayAction, text: impl Into<String>, checked: bool) -> Self {
        Self {
            checked: Some(checked),
            ..Self::item(id, text)
        }
    }

    fn submenu(text: impl Into<String>, children: Vec<Entry>) -> Self {
        Self {
            children,
            ..Self::new(None, text)
        }
    }

    fn separator() -> Self {
        Self {
            separator: true,
            ..Self::new(None, "")
        }
    }

    fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn accel(mut self, accel: &'static str) -> Self {
        self.accel = Some(accel);
        self
    }

    fn is_submenu(&self) -> bool {
        !self.children.is_empty()
    }
}

fn running_count(n: usize) -> String {
    if n == 1 {
        tr!("{n} servicio en marcha", "{n} service running")
    } else {
        tr!("{n} servicios en marcha", "{n} services running")
    }
}

fn error_count(n: u64) -> String {
    if n == 1 {
        tr!("{n} error", "{n} error")
    } else {
        tr!("{n} errores", "{n} errors")
    }
}

/// `412 MB`, `1,2 GB` (en inglés, `1.2 GB`).
fn format_memory(kb: u64) -> String {
    let mb = (kb + 512) / 1024;
    if mb < 1024 {
        format!("{mb} MB")
    } else {
        let tenths = (kb * 10 + 524_288) / 1_048_576;
        let (whole, tenth) = (tenths / 10, tenths % 10);
        tr!("{whole},{tenth} GB", "{whole}.{tenth} GB")
    }
}

/// `8.4.27` → `8.4`.
fn short_version(id: &str) -> &str {
    match id.match_indices('.').nth(1) {
        Some((index, _)) => &id[..index],
        None => id,
    }
}

/// Quién lanzó una tarea, para la fila informativa.
fn client_label(client: &str) -> String {
    if client.starts_with("claude-code") {
        return "Claude Code".into();
    }
    if let Some(rest) = client.strip_prefix("mcp:") {
        return rest.split_whitespace().next().unwrap_or(rest).to_string();
    }
    match client {
        "tty" => tr!("terminal", "terminal"),
        other => other.split_whitespace().next().unwrap_or(other).to_string(),
    }
}

fn job_text(job: &JobInfo) -> String {
    let arg = |key: &str| job.args.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let what = match (job.kind.as_str(), arg("id"), arg("name")) {
        ("component_install", Some(id), _) => tr!("Instalando {id}…", "Installing {id}…"),
        ("project_create", _, Some(name)) => tr!("Creando el proyecto {name}…", "Creating project {name}…"),
        _ => tr!("Tarea en curso…", "Job in progress…"),
    };
    match job.client.as_deref() {
        Some(client) => format!("⟳ {what} ({})", client_label(client)),
        None => format!("⟳ {what}"),
    }
}

/// Ruta de la vista donde se ve un problema.
fn problem_route(snapshot: &Snapshot, target: &str) -> &'static str {
    if target == "php" {
        "#/php"
    } else if snapshot.projects.iter().any(|p| p.id == target) {
        "#/sitios"
    } else {
        "#/servicios"
    }
}

fn busy_label<'a>(snapshot: &'a Snapshot, target: &str) -> Option<&'a str> {
    snapshot
        .busy
        .iter()
        .find(|b| b.target == target)
        .map(|b| b.label.as_str())
}

/// Servicios instalados, los que están en marcha primero (como en la ventana).
fn services(snapshot: &Snapshot) -> Vec<&elephant_core::brew::Service> {
    let mut list: Vec<_> = snapshot
        .components
        .iter()
        .filter(|c| c.installed)
        .filter_map(|c| c.service.as_ref())
        .collect();
    list.sort_by_key(|s| !s.running);
    list
}

/// Ramas instaladas de los servicios propios: `(servicio, nombre, rama)`.
fn own_branches(snapshot: &Snapshot) -> Vec<(ServiceKind, &str, &elephant_core::services::BranchStatus)> {
    snapshot
        .managed
        .iter()
        .flat_map(|m| m.branches.iter().map(move |b| (m.service, m.name.as_str(), b)))
        .collect()
}

fn header(snapshot: &Snapshot, usage_kb: Option<u64>) -> String {
    let own = own_branches(snapshot).iter().filter(|(_, _, b)| b.state == RunState::Running).count();
    let running = services(snapshot).iter().filter(|s| s.running).count() + own;
    if running == 0 {
        return tr!("○ Ningún servicio en marcha", "○ No services running");
    }
    let count = running_count(running);
    match usage_kb {
        Some(kb) if kb > 0 => format!("● {count} · {}", format_memory(kb)),
        _ => format!("● {count}"),
    }
}

fn php_menu(snapshot: &Snapshot) -> Entry {
    let php = &snapshot.php;
    let active_key = php.active.as_ref().map(|a| a.key.as_str());
    let mut children: Vec<Entry> = php
        .versions
        .iter()
        .map(|v| {
            Entry::check(
                TrayAction::PhpDefault(v.key.clone()),
                format!("{} · {}", v.id, v.source),
                Some(v.key.as_str()) == active_key,
            )
        })
        .collect();
    if !children.is_empty() {
        children.push(Entry::separator());
        children.push(Entry::nav("#/php", tr_str!("Usar en un proyecto…", "Use in project…")));
    }
    children.push(Entry::nav("#/php", tr_str!("Descargar otra versión…", "Download another version…")));

    let title = match &php.active {
        Some(active) => {
            let v = short_version(&active.id);
            tr!("PHP {v} por defecto", "Default PHP {v}")
        }
        None => tr!("PHP sin versión por defecto", "No default PHP"),
    };
    match busy_label(snapshot, "php") {
        Some(label) => Entry::submenu(format!("PHP · {label}"), children).enabled(false),
        None => Entry::submenu(title, children),
    }
}

fn services_menu(snapshot: &Snapshot) -> Option<Entry> {
    let list = services(snapshot);
    let own = own_branches(snapshot);
    if list.is_empty() && own.is_empty() {
        return None;
    }
    let any_running = list.iter().any(|s| s.running);
    let bulk_busy = busy_label(snapshot, "services");
    let mut children = Vec::new();
    // Los propios: un interruptor por rama (encender una apaga las demás).
    for (kind, name, branch) in &own {
        let running = branch.state == RunState::Running;
        let text = format!("{name} {}", branch.branch);
        let entry = match busy_label(snapshot, &branch.label) {
            Some(label) => Entry::check(TrayAction::Managed(*kind, branch.branch.clone(), !running), format!("{text} · {label}"), running)
                .enabled(false),
            None if branch.state == RunState::Error => {
                Entry::check(TrayAction::Managed(*kind, branch.branch.clone(), true), tr!("{text} · error", "{text} · error"), false)
            }
            None => Entry::check(TrayAction::Managed(*kind, branch.branch.clone(), !running), text, running),
        };
        children.push(entry);
    }
    if !list.is_empty() {
        if !children.is_empty() {
            children.push(Entry::separator());
        }
        children.extend([
            Entry::item(TrayAction::ServicesAll(Operation::Restart), tr_str!("Reiniciar todos", "Restart all"))
                .enabled(any_running && bulk_busy.is_none()),
            Entry::item(TrayAction::ServicesAll(Operation::TurnOff), tr_str!("Detener todos", "Stop all"))
                .enabled(any_running && bulk_busy.is_none()),
            Entry::separator(),
        ]);
    }
    for service in list {
        let name = &service.name;
        let svc = |op| TrayAction::Service(name.clone(), op);
        let mut items = if service.running {
            vec![
                Entry::item(svc(Operation::TurnOff), tr_str!("Detener", "Stop")),
                Entry::item(svc(Operation::Restart), tr_str!("Reiniciar", "Restart")),
            ]
        } else {
            vec![Entry::item(svc(Operation::TurnOn), tr_str!("Arrancar", "Start"))]
        };
        let autostart = if service.registered {
            Operation::DisableAutostart
        } else {
            Operation::EnableAutostart
        };
        items.push(Entry::check(svc(autostart), tr_str!("Al iniciar sesión", "Start at login"), service.registered));
        items.push(Entry::separator());
        items.push(Entry::item(TrayAction::ServiceLog(name.clone()), tr_str!("Abrir log", "Open log")));

        let dot = if service.running { "●" } else { "○" };
        let entry = match busy_label(snapshot, name).or(bulk_busy) {
            Some(label) => Entry::submenu(format!("{dot} {name} · {label}"), items).enabled(false),
            None => Entry::submenu(format!("{dot} {name}"), items),
        };
        children.push(entry);
    }
    let title = match bulk_busy {
        Some(label) => tr!("Servicios · {label}", "Services · {label}"),
        None => tr!("Servicios", "Services"),
    };
    Some(Entry::submenu(title, children))
}

fn sites_menu(snapshot: &Snapshot) -> Option<Entry> {
    let sites: Vec<_> = snapshot.projects.iter().filter(|p| !p.hosts.is_empty()).collect();
    if sites.is_empty() {
        return None;
    }
    let mut children: Vec<Entry> = sites
        .iter()
        .take(MAX_SITES)
        .map(|project| {
            let site = |action| TrayAction::Site(project.id.clone(), action);
            let needs = match busy_label(snapshot, &project.id) {
                Some(label) => Entry::item(TrayAction::SiteNeeds(project.id.clone()), label).enabled(false),
                None => Entry::item(
                    TrayAction::SiteNeeds(project.id.clone()),
                    tr_str!("Arrancar lo que necesita", "Start what it needs"),
                ),
            };
            Entry::submenu(
                project.hosts[0].clone(),
                vec![
                    Entry::item(site(SiteAction::Browser), tr_str!("Abrir en el navegador", "Open in browser")),
                    Entry::item(site(SiteAction::Editor), tr_str!("Abrir en el editor", "Open in editor")),
                    Entry::item(site(SiteAction::Terminal), tr_str!("Abrir terminal", "Open in Terminal")),
                    Entry::item(site(SiteAction::CopyUrl), tr_str!("Copiar URL", "Copy URL")),
                    Entry::separator(),
                    needs,
                    Entry::nav(&format!("#/logs?project={}", project.id), tr_str!("Ver logs", "View logs")),
                ],
            )
        })
        .collect();
    children.push(Entry::separator());
    children.push(Entry::nav("#/sitios", tr_str!("Ver todos…", "View all…")));
    Some(Entry::submenu(tr_str!("Sitios", "Sites"), children))
}

fn logs_menu(snapshot: &Snapshot) -> Option<Entry> {
    let logs = &snapshot.logs;
    if !logs.available {
        return None;
    }
    let recent = if logs.unseen_errors > 0 {
        let n = logs.unseen_errors;
        tr!("Errores recientes ({n})", "Recent errors ({n})")
    } else {
        tr!("Errores recientes", "Recent errors")
    };
    let mut children = vec![
        Entry::nav("#/logs", tr_str!("Abrir visor", "Open viewer")),
        Entry::nav("#/logs?level=error&range=1h", recent),
    ];
    if !logs.recent.is_empty() {
        children.push(Entry::separator());
        for project in logs.recent.iter().take(5) {
            children.push(Entry::nav(
                &format!("#/logs?project={}", project.id),
                format!("{} · {}", project.name, error_count(project.errors as u64)),
            ));
        }
    }
    Some(Entry::submenu("Logs", children))
}

/// El menú completo de 00 §5.10. Pura: no toca Tauri ni el sistema.
pub fn entries(snapshot: &Snapshot, usage_kb: Option<u64>) -> Vec<Entry> {
    let mut out = vec![Entry::info(header(snapshot, usage_kb))];
    for problem in &snapshot.problems {
        out.push(Entry::nav(
            problem_route(snapshot, &problem.target),
            format!("⚠ {}", problem.message),
        ));
    }
    out.extend(snapshot.jobs.iter().map(|job| Entry::info(job_text(job))));
    out.push(Entry::separator());
    out.push(Entry::item(TrayAction::Open { route: None }, tr_str!("Abrir Elephant Stack", "Open Elephant Stack")));
    out.push(Entry::separator());
    out.push(php_menu(snapshot));
    out.extend(services_menu(snapshot));
    out.extend(sites_menu(snapshot));
    out.extend(logs_menu(snapshot));
    if snapshot.mailpit.installed {
        out.push(
            Entry::item(TrayAction::OpenMailpit, tr_str!("Abrir Mailpit", "Open Mailpit"))
                .enabled(snapshot.mailpit.running),
        );
    }
    out.push(Entry::separator());
    out.push(Entry::nav("#/asistente", tr_str!("Asistente de configuración…", "Setup assistant…")));
    out.push(Entry::nav("#/ajustes/general", tr_str!("Ajustes…", "Settings…")).accel("CmdOrCtrl+,"));
    out.push(Entry::item(TrayAction::Quit, tr_str!("Salir de Elephant Stack", "Quit Elephant Stack")).accel("CmdOrCtrl+Q"));
    out
}

/// Texto junto al icono: la versión de PHP por defecto, si se ha pedido.
pub fn title(snapshot: &Snapshot) -> Option<String> {
    if !snapshot.settings.show_php_in_menu_bar {
        return None;
    }
    let active = snapshot.php.active.as_ref()?;
    Some(short_version(&active.id).to_string())
}

/// El icono lleva un punto si hay problemas o errores de logs sin ver.
pub fn warning(snapshot: &Snapshot) -> bool {
    !snapshot.problems.is_empty() || snapshot.logs.unseen_errors > 0
}

/// Estructura del menú: ids, tipo de cada elemento, atajos y jerarquía. Si no
/// cambia, el menú se actualiza en sitio.
pub fn shape(entries: &[Entry]) -> u64 {
    fn walk(entries: &[Entry], hasher: &mut DefaultHasher) {
        entries.len().hash(hasher);
        for entry in entries {
            entry.separator.hash(hasher);
            entry.id.as_ref().map(ToString::to_string).hash(hasher);
            entry.checked.is_some().hash(hasher);
            entry.accel.hash(hasher);
            entry.is_submenu().hash(hasher);
            walk(&entry.children, hasher);
        }
    }
    let mut hasher = DefaultHasher::new();
    walk(entries, &mut hasher);
    hasher.finish()
}

/// Un cambio en sitio sobre el elemento `n`, contando en preorden todos los
/// que no son separadores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Text(usize, String),
    Enabled(usize, bool),
    Checked(usize, bool),
}

/// Elementos que no son separadores, en preorden: el mismo orden en que se
/// guardan los handles nativos.
fn flatten(entries: &[Entry]) -> Vec<&Entry> {
    fn walk<'a>(entries: &'a [Entry], out: &mut Vec<&'a Entry>) {
        for entry in entries.iter().filter(|e| !e.separator) {
            out.push(entry);
            walk(&entry.children, out);
        }
    }
    let mut out = Vec::new();
    walk(entries, &mut out);
    out
}

/// Diferencias entre dos menús con la misma [`shape`].
pub fn diff(old: &[Entry], new: &[Entry]) -> Vec<Change> {
    let mut changes = Vec::new();
    for (n, (a, b)) in flatten(old).into_iter().zip(flatten(new)).enumerate() {
        if a.text != b.text {
            changes.push(Change::Text(n, b.text.clone()));
        }
        if a.enabled != b.enabled {
            changes.push(Change::Enabled(n, b.enabled));
        }
        if let (Some(was), Some(now)) = (a.checked, b.checked)
            && was != now
        {
            changes.push(Change::Checked(n, now));
        }
    }
    changes
}

/// Al pulsar un elemento con marca, macOS la invierte por su cuenta. Esto
/// deja la descripción guardada igual que el menú nativo y devuelve lo que
/// hay que corregir en él: con PHP, marcar solo la versión elegida (Tauri no
/// tiene radios). Así el siguiente [`diff`] repara la marca si la acción falla.
fn apply_click(entries: &mut [Entry], id: &TrayAction) -> Vec<Change> {
    fn walk(entries: &mut [Entry], id: &TrayAction, n: &mut usize, changes: &mut Vec<Change>) {
        for entry in entries.iter_mut().filter(|e| !e.separator) {
            if let Some(checked) = entry.checked.as_mut() {
                match (&entry.id, id) {
                    (Some(TrayAction::PhpDefault(key)), TrayAction::PhpDefault(clicked)) => {
                        *checked = key == clicked;
                        changes.push(Change::Checked(*n, *checked));
                    }
                    (Some(own), clicked) if own == clicked => *checked = !*checked,
                    _ => {}
                }
            }
            *n += 1;
            walk(&mut entry.children, id, n, changes);
        }
    }
    let mut changes = Vec::new();
    walk(entries, id, &mut 0, &mut changes);
    changes
}

/// Pinta un punto en la esquina superior derecha de una imagen RGBA, con un
/// hueco transparente alrededor para que se distinga del dibujo. El icono es
/// una plantilla: macOS lo colorea entero, así que solo importa el alfa.
pub fn with_dot(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let size = width.min(height) as f32;
    let radius = size * 0.17;
    let gap = size * 0.07;
    let cx = width as f32 - radius - 1.0;
    let cy = radius + 1.0;
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            let index = ((y * width + x) * 4) as usize;
            if index + 3 >= out.len() {
                return out;
            }
            if distance <= radius {
                out[index..index + 4].copy_from_slice(&[0, 0, 0, 255]);
            } else if distance <= radius + gap {
                out[index + 3] = 0;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Menú nativo
// ---------------------------------------------------------------------------

enum Native {
    Item(MenuItem<Wry>),
    Check(CheckMenuItem<Wry>),
    Sub(Submenu<Wry>),
}

impl Native {
    fn apply(&self, change: &Change) -> tauri::Result<()> {
        match (self, change) {
            (Self::Item(i), Change::Text(_, t)) => i.set_text(t),
            (Self::Check(i), Change::Text(_, t)) => i.set_text(t),
            (Self::Sub(i), Change::Text(_, t)) => i.set_text(t),
            (Self::Item(i), Change::Enabled(_, e)) => i.set_enabled(*e),
            (Self::Check(i), Change::Enabled(_, e)) => i.set_enabled(*e),
            (Self::Sub(i), Change::Enabled(_, e)) => i.set_enabled(*e),
            (Self::Check(i), Change::Checked(_, c)) => i.set_checked(*c),
            _ => Ok(()),
        }
    }
}

fn change_index(change: &Change) -> usize {
    match change {
        Change::Text(n, _) | Change::Enabled(n, _) | Change::Checked(n, _) => *n,
    }
}

struct Built {
    shape: u64,
    entries: Vec<Entry>,
    /// En el orden de [`flatten`].
    handles: Vec<Native>,
    title: Option<String>,
    warning: bool,
}

/// Lo que hay ahora en el menú nativo. Solo se toca en el hilo principal.
#[derive(Default)]
struct TrayMenu(Mutex<Option<Built>>);

impl TrayMenu {
    fn lock(&self) -> MutexGuard<'_, Option<Built>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn native_items(app: &AppHandle, entries: &[Entry], handles: &mut Vec<Native>) -> tauri::Result<Vec<Box<dyn IsMenuItem<Wry>>>> {
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.separator {
            items.push(Box::new(PredefinedMenuItem::separator(app)?));
            continue;
        }
        let id = entry.id.as_ref().map(ToString::to_string);
        if entry.is_submenu() {
            let submenu = Submenu::new(app, &entry.text, entry.enabled)?;
            // Preorden: el submenú antes que sus hijos, como en `flatten`.
            handles.push(Native::Sub(submenu.clone()));
            let children = native_items(app, &entry.children, handles)?;
            let refs: Vec<&dyn IsMenuItem<Wry>> = children.iter().map(|c| c.as_ref()).collect();
            submenu.append_items(&refs)?;
            items.push(Box::new(submenu));
        } else if let Some(checked) = entry.checked {
            let item = match id {
                Some(id) => CheckMenuItem::with_id(app, id, &entry.text, entry.enabled, checked, entry.accel)?,
                None => CheckMenuItem::new(app, &entry.text, entry.enabled, checked, entry.accel)?,
            };
            handles.push(Native::Check(item.clone()));
            items.push(Box::new(item));
        } else {
            let item = match id {
                Some(id) => MenuItem::with_id(app, id, &entry.text, entry.enabled, entry.accel)?,
                None => MenuItem::new(app, &entry.text, entry.enabled, entry.accel)?,
            };
            handles.push(Native::Item(item.clone()));
            items.push(Box::new(item));
        }
    }
    Ok(items)
}

fn native_menu(app: &AppHandle, entries: &[Entry]) -> tauri::Result<(Menu<Wry>, Vec<Native>)> {
    let mut handles = Vec::new();
    let items = native_items(app, entries, &mut handles)?;
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i.as_ref()).collect();
    let menu = Menu::with_items(app, &refs)?;
    Ok((menu, handles))
}

fn icon(warning: bool) -> tauri::Result<Image<'static>> {
    let base = Image::from_bytes(TRAY_PNG)?;
    if !warning {
        return Ok(base.to_owned());
    }
    static DOTTED: OnceLock<Vec<u8>> = OnceLock::new();
    let rgba = DOTTED.get_or_init(|| with_dot(base.rgba(), base.width(), base.height()));
    Ok(Image::new_owned(rgba.clone(), base.width(), base.height()))
}

/// Aplica la descripción al menú nativo. Solo en el hilo principal.
fn sync(app: &AppHandle, entries: Vec<Entry>, title: Option<String>, warning: bool) {
    let (Some(tray), Some(menu)) = (app.tray_by_id(TRAY_ID), app.try_state::<TrayMenu>()) else {
        return;
    };
    let mut built = menu.lock();
    let new_shape = shape(&entries);
    match built.as_mut() {
        Some(current) if current.shape == new_shape => {
            for change in diff(&current.entries, &entries) {
                if let Some(native) = current.handles.get(change_index(&change))
                    && let Err(error) = native.apply(&change)
                {
                    eprintln!("No se pudo actualizar el menú de la barra: {error}");
                }
            }
            current.entries = entries;
        }
        _ => match native_menu(app, &entries) {
            Ok((native, handles)) => {
                if let Err(error) = tray.set_menu(Some(native)) {
                    eprintln!("No se pudo poner el menú de la barra: {error}");
                    return;
                }
                let (title, warning) = built
                    .as_ref()
                    .map(|b| (b.title.clone(), b.warning))
                    .unwrap_or((None, false));
                *built = Some(Built {
                    shape: new_shape,
                    entries,
                    handles,
                    title,
                    warning,
                });
            }
            Err(error) => {
                eprintln!("No se pudo construir el menú de la barra: {error}");
                return;
            }
        },
    }
    let Some(current) = built.as_mut() else {
        return;
    };
    if current.title != title {
        let _ = tray.set_title(title.as_deref());
        current.title = title;
    }
    if current.warning != warning {
        match icon(warning) {
            Ok(image) => {
                let _ = tray.set_icon_with_as_template(Some(image), true);
                current.warning = warning;
            }
            Err(error) => eprintln!("No se pudo cambiar el icono de la barra: {error}"),
        }
    }
}

/// Programa [`sync`] en el hilo principal. El menú solo se toca allí, así que
/// nadie espera a ese hilo con el cerrojo del menú tomado. Si ya se está en
/// él, se aplica en el acto.
fn schedule_sync(app: &AppHandle, snapshot: &Snapshot, usage_kb: Option<u64>) {
    let entries = entries(snapshot, usage_kb);
    let title = title(snapshot);
    let warning = warning(snapshot);
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || sync(&handle, entries, title, warning)) {
        eprintln!("No se pudo actualizar el menú de la barra: {error}");
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TrayMenu::default());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(false)?)
        .icon_as_template(true)
        .tooltip("Elephant Stack")
        .show_menu_on_left_click(true)
        .on_tray_icon_event(|tray, event| {
            // Al acercar el puntero o pulsar: así la siguiente apertura está al
            // día. Las dos limitan su propio ritmo (C16).
            if matches!(event, TrayIconEvent::Enter { .. } | TrayIconEvent::Click { .. }) {
                let app = tray.app_handle().clone();
                watch::tick_now(&app);
                std::thread::spawn(move || commands::logs::refresh_brief(&app));
            }
        })
        .build(app)?;
    let state = app.state::<AppState>();
    schedule_sync(app, &state.snapshot(), state.usage_kb());
    Ok(())
}

/// Descarta el menú nativo y lo construye de nuevo con el snapshot actual
/// (al cambiar el idioma). Las dos cosas van en orden al hilo principal.
pub fn rebuild(app: &AppHandle) {
    let handle = app.clone();
    let reset = app.run_on_main_thread(move || {
        if let Some(menu) = handle.try_state::<TrayMenu>() {
            *menu.lock() = None;
        }
    });
    if let Err(error) = reset {
        eprintln!("No se pudo reconstruir el menú de la barra: {error}");
        return;
    }
    let state = app.state::<AppState>();
    schedule_sync(app, &state.snapshot(), state.usage_kb());
}

/// Lo llama solo `state::update`.
pub fn refresh(app: &AppHandle, snapshot: &Snapshot, usage_kb: Option<u64>) {
    schedule_sync(app, snapshot, usage_kb);
}

/// Solo cambia el texto de la cabecera (memoria de los servicios): con la
/// misma estructura, [`sync`] se queda en un `set_text`.
pub fn set_usage(app: &AppHandle, usage_kb: Option<u64>) {
    if let Some(state) = app.try_state::<AppState>() {
        schedule_sync(app, &state.snapshot(), usage_kb);
    }
}

/// `true` si el id era de este menú. Llega en el hilo principal: lo lento va
/// a `spawn_blocking`.
pub fn handle(app: &AppHandle, id: &str) -> bool {
    let Ok(action) = id.parse::<TrayAction>() else {
        return false;
    };
    if let Some(menu) = app.try_state::<TrayMenu>()
        && let Some(built) = menu.lock().as_mut()
    {
        for change in apply_click(&mut built.entries, &action) {
            if let Some(native) = built.handles.get(change_index(&change)) {
                let _ = native.apply(&change);
            }
        }
    }
    match action {
        TrayAction::Open { route } => window::open_main_window(app, route.as_deref()),
        TrayAction::Quit => app.exit(0),
        action => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || run(&app, action));
        }
    }
    true
}

/// Las acciones con `busy` ya apuntan sus errores como problemas
/// (`actions::tracked`); el resto lo hace aquí.
fn run(app: &AppHandle, action: TrayAction) {
    let result = match &action {
        TrayAction::PhpDefault(key) => actions::set_global_php(app, key),
        TrayAction::Service(name, op) => actions::service_operation(app, name, *op),
        TrayAction::ServicesAll(op) => actions::services_bulk(app, *op),
        TrayAction::ServiceLog(name) => untracked(app, name, actions::open_service_log(name)),
        TrayAction::Managed(kind, branch, on) => actions::managed_power(app, *kind, branch, *on).map(drop),
        TrayAction::Site(project, site) => untracked(app, project, actions::site(app, project, *site)),
        TrayAction::SiteNeeds(project) => match project_root(app, project) {
            Some(root) => actions::start_project_services(app, &root).map(drop),
            None => untracked(app, project, Err(actions::project_gone())),
        },
        TrayAction::OpenMailpit => {
            let url = app.state::<AppState>().snapshot().mailpit.url;
            untracked(app, "mailpit", actions::open_url(&url))
        }
        TrayAction::Open { .. } | TrayAction::Quit => Ok(()),
    };
    if let Err(error) = result {
        eprintln!("Acción del menú «{action}»: {error}");
    }
}



fn project_root(app: &AppHandle, project_id: &str) -> Option<PathBuf> {
    app.state::<AppState>()
        .snapshot()
        .projects
        .into_iter()
        .find(|p| p.id == project_id)
        .map(|p| p.root)
}

fn untracked(app: &AppHandle, target: &str, result: Result<(), String>) -> Result<(), String> {
    if let Err(error) = &result {
        state::add_problem(app, target, error.clone(), ProblemKind::ActionFailed);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Busy, LogProjectBrief, LogsBrief, McpBrief, Problem};
    use elephant_core::brew::Service;
    use elephant_core::components::{Component, Kind};
    use elephant_core::jobs::JobState;
    use elephant_core::mailpit::MailpitInfo;
    use elephant_core::php::{PhpStatus, VersionInfo};
    use elephant_core::settings::Settings;
    use elephant_logs::{Project, ProjectKind, ProjectOrigin};
    use std::path::PathBuf;

    fn empty() -> Snapshot {
        Snapshot {
            revision: 1,
            refreshed_at_ms: 1,
            components: Vec::new(),
            managed: Vec::new(),
            managed_tools: Vec::new(),
            php: PhpStatus::default(),
            projects: Vec::new(),
            busy: Vec::new(),
            problems: Vec::new(),
            jobs: Vec::new(),
            logs: LogsBrief::default(),
            mcp: McpBrief::default(),
            mailpit: MailpitInfo {
                installed: false,
                running: false,
                url: "http://127.0.0.1:8025".into(),
                messages: None,
            },
            setup_needed: false,
            settings: Settings::default(),
            locale: elephant_core::i18n::Locale::Es,
        }
    }

    fn service(name: &str, running: bool, registered: bool) -> Component {
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
                status: if running { "started" } else { "none" }.into(),
                running,
                registered,
                pid: running.then_some(100),
                exit_code: None,
            }),
        }
    }

    fn version(id: &str, source: &'static str, key: &str) -> VersionInfo {
        VersionInfo {
            key: key.into(),
            id: id.into(),
            source,
        }
    }

    fn project(id: &str, name: &str, host: Option<&str>) -> Project {
        Project {
            id: id.into(),
            name: name.into(),
            root: PathBuf::from(format!("/Users/ana/Herd/{name}")),
            hosts: host.into_iter().map(str::to_string).collect(),
            kind: ProjectKind::Laravel,
            origin: ProjectOrigin::HerdParked,
            php_version: None,
            log_tz: "UTC".into(),
        }
    }

    fn find<'a>(entries: &'a [Entry], text: &str) -> Option<&'a Entry> {
        flatten(entries).into_iter().find(|e| e.text == text)
    }

    fn texts(entries: &[Entry]) -> Vec<&str> {
        entries.iter().filter(|e| !e.separator).map(|e| e.text.as_str()).collect()
    }

    #[test]
    fn every_action_survives_a_round_trip() {
        let actions = [
            TrayAction::Open { route: None },
            TrayAction::Open {
                route: Some("#/logs?level=error&range=1h".into()),
            },
            TrayAction::Open {
                route: Some("#/raro:con:dos-puntos".into()),
            },
            TrayAction::PhpDefault("8.4@Herd".into()),
            TrayAction::PhpDefault("8.3.14".into()),
            TrayAction::Service("postgresql@18".into(), Operation::Restart),
            TrayAction::Service("mysql".into(), Operation::TurnOn),
            TrayAction::Service("mysql".into(), Operation::TurnOff),
            TrayAction::Service("redis".into(), Operation::EnableAutostart),
            TrayAction::Service("redis".into(), Operation::DisableAutostart),
            TrayAction::Service("redis".into(), Operation::TurnOnWithAutostart),
            TrayAction::ServicesAll(Operation::Restart),
            TrayAction::ServicesAll(Operation::TurnOff),
            TrayAction::ServiceLog("mysql".into()),
            TrayAction::Managed(ServiceKind::Mysql, "9.7".into(), true),
            TrayAction::Managed(ServiceKind::Postgresql, "18".into(), false),
            TrayAction::Site("0123456789abcdef".into(), SiteAction::Browser),
            TrayAction::Site("0123456789abcdef".into(), SiteAction::Editor),
            TrayAction::Site("0123456789abcdef".into(), SiteAction::Terminal),
            TrayAction::Site("0123456789abcdef".into(), SiteAction::CopyUrl),
            TrayAction::SiteNeeds("0123456789abcdef".into()),
            TrayAction::OpenMailpit,
            TrayAction::Quit,
        ];
        for action in actions {
            let id = action.to_string();
            assert_eq!(id.parse::<TrayAction>(), Ok(action), "{id}");
        }
    }

    #[test]
    fn ids_follow_the_plan() {
        assert_eq!(TrayAction::Service("mysql".into(), Operation::Restart).to_string(), "svc:restart:mysql");
        assert_eq!(TrayAction::ServicesAll(Operation::TurnOff).to_string(), "svc-all:turn_off");
        assert_eq!(TrayAction::PhpDefault("8.4@Herd".into()).to_string(), "php:default:8.4@Herd");
        assert_eq!(TrayAction::Site("x".into(), SiteAction::CopyUrl).to_string(), "site:copy:x");
        assert_eq!(TrayAction::SiteNeeds("x".into()).to_string(), "site-needs:x");
        assert_eq!(TrayAction::Managed(ServiceKind::Redis, "8.2".into(), false).to_string(), "own:off:redis:8.2");
        assert!("own:on:redis".parse::<TrayAction>().is_err() && "own:si:redis:8.2".parse::<TrayAction>().is_err());
        assert!("own:on:mariadb:11".parse::<TrayAction>().is_err());
        assert!("site-needs:".parse::<TrayAction>().is_err());
        assert!("site:needs:x".parse::<TrayAction>().is_err());
        assert_eq!(
            TrayAction::Open {
                route: Some("#/logs".into())
            }
            .to_string(),
            "nav:#/logs"
        );
    }

    fn own(kind: ServiceKind, name: &str, branch: &str, state: RunState) -> elephant_core::services::ManagedService {
        use elephant_core::services::{BranchStatus, InstalledVersion, ManagedService};
        ManagedService {
            service: kind,
            name: name.into(),
            port: kind.default_port(),
            web_port: kind.default_web_port(),
            versions: vec![InstalledVersion { version: format!("{branch}.1"), branch: branch.into(), path: "/x".into() }],
            branches: vec![BranchStatus {
                branch: branch.into(),
                version: format!("{branch}.1"),
                label: elephant_core::services::label(kind, branch),
                state,
                pid: (state == RunState::Running).then_some(7),
                autostart: false,
                error: None,
                data_dir: "/d".into(),
                data_ready: true,
                log_path: "/l".into(),
            }],
            import: None,
        }
    }

    #[test]
    fn own_services_get_a_switch_in_the_services_menu_and_count_in_the_header() {
        let mut snapshot = empty();
        snapshot.managed = vec![
            own(ServiceKind::Mysql, "MySQL", "9.7", RunState::Running),
            own(ServiceKind::Postgresql, "PostgreSQL", "18", RunState::Stopped),
        ];
        let menu = services_menu(&snapshot).unwrap();
        // Sin servicios de brew, ni «Reiniciar todos» ni «Detener todos».
        assert_eq!(texts(&menu.children), ["MySQL 9.7", "PostgreSQL 18"]);
        assert_eq!(menu.children[0].checked, Some(true));
        assert_eq!(menu.children[0].id, Some(TrayAction::Managed(ServiceKind::Mysql, "9.7".into(), false)));
        assert_eq!(menu.children[1].id, Some(TrayAction::Managed(ServiceKind::Postgresql, "18".into(), true)));
        assert!(header(&snapshot, None).contains('1'), "{}", header(&snapshot, None));
        snapshot.busy.push(Busy { target: "sh.elephant.postgresql.18".into(), label: "Arrancando…".into() });
        let menu = services_menu(&snapshot).unwrap();
        assert_eq!(menu.children[1].text, "PostgreSQL 18 · Arrancando…");
        assert!(!menu.children[1].enabled);
    }

    #[test]
    fn operation_slugs_match_serde() {
        for op in [
            Operation::TurnOn,
            Operation::TurnOff,
            Operation::Restart,
            Operation::EnableAutostart,
            Operation::DisableAutostart,
            Operation::TurnOnWithAutostart,
        ] {
            assert_eq!(serde_json::to_value(op).unwrap(), operation_slug(op));
        }
    }

    #[test]
    fn unknown_ids_are_not_ours() {
        for id in [
            "",
            "app:nav:#/panel",
            "nav:",
            "php:8.4",
            "php:default:",
            "svc:fly:mysql",
            "svc:restart:",
            "svc-all:turn_on",
            "site:open:x",
            "site:browser:",
            "mailpit",
            "3f2a9c1e-random-id-from-js",
        ] {
            assert!(id.parse::<TrayAction>().is_err(), "{id}");
        }
    }

    #[test]
    fn an_empty_snapshot_has_the_basic_menu() {
        let menu = entries(&empty(), None);
        assert_eq!(
            texts(&menu),
            [
                "○ Ningún servicio en marcha",
                "Abrir Elephant Stack",
                "PHP sin versión por defecto",
                "Asistente de configuración…",
                "Ajustes…",
                "Salir de Elephant Stack",
            ]
        );
        assert!(!menu[0].enabled, "la cabecera es informativa");
        let english = elephant_core::i18n::with_locale(elephant_core::i18n::Locale::En, || entries(&empty(), None));
        assert_eq!(
            texts(&english),
            [
                "○ No services running",
                "Open Elephant Stack",
                "No default PHP",
                "Setup assistant…",
                "Settings…",
                "Quit Elephant Stack",
            ]
        );
        assert!(!warning(&empty()));
        assert_eq!(title(&empty()), None);
        // Sin servicios, sitios, logs ni Mailpit no hay submenús vacíos.
        assert!(find(&menu, "Servicios").is_none());
        assert!(find(&menu, "Sitios").is_none());
        assert!(find(&menu, "Logs").is_none());
        assert!(find(&menu, "Abrir Mailpit").is_none());
        let settings = find(&menu, "Ajustes…").unwrap();
        assert_eq!(settings.accel, Some("CmdOrCtrl+,"));
        assert_eq!(
            settings.id,
            Some(TrayAction::Open {
                route: Some("#/ajustes/general".into())
            })
        );
    }

    #[test]
    fn a_failed_service_shows_a_warning_row_and_the_dot() {
        let mut snapshot = empty();
        snapshot.components = vec![service("mysql", false, true)];
        snapshot.problems = vec![Problem {
            id: "service_error:mysql".into(),
            target: "mysql".into(),
            message: "mysql se ha detenido (código 1)".into(),
            kind: ProblemKind::ServiceError,
        }];
        let menu = entries(&snapshot, None);
        let row = &menu[1];
        assert_eq!(row.text, "⚠ mysql se ha detenido (código 1)");
        assert_eq!(
            row.id,
            Some(TrayAction::Open {
                route: Some("#/servicios".into())
            })
        );
        assert!(row.enabled);
        assert!(warning(&snapshot));
    }

    #[test]
    fn unseen_log_errors_also_show_the_dot() {
        let mut snapshot = empty();
        snapshot.logs.unseen_errors = 2;
        assert!(warning(&snapshot));
    }

    #[test]
    fn only_the_default_php_is_checked() {
        let mut snapshot = empty();
        snapshot.php = PhpStatus {
            versions: vec![
                version("8.4.27", "Elephant Stack", "8.4.27"),
                version("8.3", "Herd", "8.3@Herd"),
            ],
            active: Some(version("8.4.27", "Elephant Stack", "8.4.27")),
        };
        let menu = entries(&snapshot, None);
        let php = find(&menu, "PHP 8.4 por defecto").expect("submenú de PHP");
        let checks: Vec<_> = php.children.iter().filter_map(|e| e.checked.map(|c| (e.text.as_str(), c))).collect();
        assert_eq!(checks, [("8.4.27 · Elephant Stack", true), ("8.3 · Herd", false)]);
        assert_eq!(php.children[1].id, Some(TrayAction::PhpDefault("8.3@Herd".into())));

        snapshot.settings.show_php_in_menu_bar = true;
        assert_eq!(title(&snapshot).as_deref(), Some("8.4"));
    }

    #[test]
    fn services_are_listed_running_first_with_their_actions() {
        let mut snapshot = empty();
        snapshot.components = vec![
            service("redis", false, false),
            service("nginx", true, true),
            Component {
                installed: false,
                ..service("mysql", false, false)
            },
        ];
        let menu = entries(&snapshot, Some(421_888));
        assert_eq!(menu[0].text, "● 1 servicio en marcha · 412 MB");

        let services = find(&menu, "Servicios").unwrap();
        assert_eq!(texts(&services.children), ["Reiniciar todos", "Detener todos", "● nginx", "○ redis"]);
        let nginx = find(&services.children, "● nginx").unwrap();
        assert_eq!(texts(&nginx.children), ["Detener", "Reiniciar", "Al iniciar sesión", "Abrir log"]);
        let autostart = find(&nginx.children, "Al iniciar sesión").unwrap();
        assert_eq!(autostart.checked, Some(true));
        assert_eq!(autostart.id, Some(TrayAction::Service("nginx".into(), Operation::DisableAutostart)));
        let redis = find(&services.children, "○ redis").unwrap();
        assert_eq!(texts(&redis.children), ["Arrancar", "Al iniciar sesión", "Abrir log"]);
        assert_eq!(
            find(&redis.children, "Al iniciar sesión").unwrap().id,
            Some(TrayAction::Service("redis".into(), Operation::EnableAutostart))
        );
    }

    #[test]
    fn a_busy_service_is_relabelled_and_disabled_in_place() {
        let mut snapshot = empty();
        snapshot.components = vec![service("mysql", true, false)];
        let before = entries(&snapshot, None);
        snapshot.busy = vec![Busy {
            target: "mysql".into(),
            label: "Reiniciando…".into(),
        }];
        let after = entries(&snapshot, None);
        let mysql = find(&after, "● mysql · Reiniciando…").unwrap();
        assert!(!mysql.enabled);
        assert_eq!(shape(&before), shape(&after), "solo cambian textos y estados");
        assert!(!diff(&before, &after).is_empty());
    }

    #[test]
    fn only_a_usage_change_is_a_single_text_change() {
        let mut snapshot = empty();
        snapshot.components = vec![service("nginx", true, true), service("mysql", true, false)];
        let before = entries(&snapshot, Some(100 * 1024));
        let after = entries(&snapshot, Some(1_258_291));
        assert_eq!(shape(&before), shape(&after));
        assert_eq!(
            diff(&before, &after),
            [Change::Text(0, "● 2 servicios en marcha · 1,2 GB".into())]
        );
    }

    #[test]
    fn a_service_that_stops_changes_the_shape() {
        let mut snapshot = empty();
        snapshot.components = vec![service("nginx", true, true)];
        let before = entries(&snapshot, None);
        snapshot.components = vec![service("nginx", false, true)];
        assert_ne!(shape(&before), shape(&entries(&snapshot, None)));
    }

    #[test]
    fn sites_logs_mailpit_and_jobs() {
        let mut snapshot = empty();
        snapshot.projects = vec![
            project("aaaa", "acme", Some("acme.test")),
            project("bbbb", "script", None),
        ];
        snapshot.logs = LogsBrief {
            available: true,
            unseen_errors: 2,
            recent: vec![LogProjectBrief {
                id: "aaaa".into(),
                name: "acme".into(),
                errors: 1,
                last_ms: Some(1),
            }],
        };
        snapshot.mailpit.installed = true;
        snapshot.jobs = vec![JobInfo {
            id: "abcdefgh".into(),
            kind: "component_install".into(),
            args: serde_json::json!({ "id": "redis" }),
            client: Some("claude-code 2.1.318".into()),
            state: JobState::Running,
            progress: None,
            message: None,
            started_at_ms: 1,
            finished_at_ms: None,
        }];
        let menu = entries(&snapshot, None);

        assert_eq!(menu[1].text, "⟳ Instalando redis… (Claude Code)");
        assert!(!menu[1].enabled);

        let sites = find(&menu, "Sitios").unwrap();
        assert_eq!(texts(&sites.children), ["acme.test", "Ver todos…"], "sin host no hay sitio");
        let acme = &sites.children[0];
        assert_eq!(acme.children[0].id, Some(TrayAction::Site("aaaa".into(), SiteAction::Browser)));
        assert_eq!(
            find(&acme.children, "Ver logs").unwrap().id,
            Some(TrayAction::Open {
                route: Some("#/logs?project=aaaa".into())
            })
        );

        let needs = find(&acme.children, "Arrancar lo que necesita").unwrap();
        assert_eq!(needs.id, Some(TrayAction::SiteNeeds("aaaa".into())));
        assert!(needs.enabled);

        let logs = find(&menu, "Logs").unwrap();
        assert_eq!(texts(&logs.children), ["Abrir visor", "Errores recientes (2)", "acme · 1 error"]);
        assert_eq!(
            logs.children[1].id,
            Some(TrayAction::Open {
                route: Some("#/logs?level=error&range=1h".into())
            })
        );

        let mailpit = find(&menu, "Abrir Mailpit").unwrap();
        assert!(!mailpit.enabled, "Mailpit detenido");
    }

    #[test]
    fn starting_what_a_site_needs_shows_progress_in_place() {
        let mut snapshot = empty();
        snapshot.projects = vec![project("aaaa", "acme", Some("acme.test"))];
        let before = entries(&snapshot, None);
        snapshot.busy = vec![Busy {
            target: "aaaa".into(),
            label: actions::needs_busy().into(),
        }];
        let after = entries(&snapshot, None);

        let site = &find(&after, "Sitios").unwrap().children[0];
        let needs = site
            .children
            .iter()
            .find(|e| e.id == Some(TrayAction::SiteNeeds("aaaa".into())))
            .unwrap();
        assert_eq!(needs.text, actions::needs_busy());
        assert!(!needs.enabled);
        assert_eq!(shape(&before), shape(&after), "se actualiza sin rehacer el menú");
    }

    #[test]
    fn clicking_a_php_version_moves_the_check() {
        let mut snapshot = empty();
        snapshot.php = PhpStatus {
            versions: vec![version("8.4.27", "Elephant Stack", "8.4.27"), version("8.3", "Herd", "8.3@Herd")],
            active: Some(version("8.4.27", "Elephant Stack", "8.4.27")),
        };
        let mut menu = entries(&snapshot, None);
        let changes = apply_click(&mut menu, &TrayAction::PhpDefault("8.3@Herd".into()));
        assert_eq!(changes.len(), 2);
        let php = find(&menu, "PHP 8.4 por defecto").unwrap();
        assert_eq!(php.children[0].checked, Some(false));
        assert_eq!(php.children[1].checked, Some(true));

        // Si el cambio falla, el estado real vuelve a marcar la 8.4.
        let fixes = diff(&menu, &entries(&snapshot, None));
        assert_eq!(fixes.len(), 2);
        assert!(fixes.iter().all(|c| matches!(c, Change::Checked(..))));
    }

    #[test]
    fn clicking_autostart_mirrors_the_native_toggle() {
        let mut snapshot = empty();
        snapshot.components = vec![service("redis", true, false)];
        let mut menu = entries(&snapshot, None);
        let id = TrayAction::Service("redis".into(), Operation::EnableAutostart);
        assert!(apply_click(&mut menu, &id).is_empty(), "macOS ya la ha invertido");
        // Si la acción falla, el siguiente diff quita la marca.
        assert_eq!(
            diff(&menu, &entries(&snapshot, None))
                .into_iter()
                .filter(|c| matches!(c, Change::Checked(_, false)))
                .count(),
            1
        );
    }

    #[test]
    fn helpers_format_like_macos() {
        assert_eq!(format_memory(0), "0 MB");
        assert_eq!(format_memory(421_888), "412 MB");
        assert_eq!(format_memory(1_048_576), "1,0 GB");
        assert_eq!(short_version("8.4.27"), "8.4");
        assert_eq!(short_version("8.4"), "8.4");
        assert_eq!(client_label("mcp:cursor 1.2"), "cursor");
        assert_eq!(client_label("tty"), "terminal");
    }

    #[test]
    fn the_dot_is_drawn_top_right_with_a_gap() {
        let (w, h) = (44u32, 44u32);
        let base = vec![255u8; (w * h * 4) as usize];
        let out = with_dot(&base, w, h);
        let pixel = |x: u32, y: u32| &out[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        // Centro del punto: opaco.
        let radius = 44.0 * 0.17;
        let (cx, cy) = ((44.0 - radius - 1.0) as u32, (radius + 1.0) as u32);
        assert_eq!(pixel(cx, cy)[3], 255);
        // Justo fuera del punto: transparente.
        let ring_x = (44.0 - radius - 1.0 - radius - 1.5) as u32;
        assert_eq!(pixel(ring_x, cy)[3], 0);
        // Lejos del punto: intacto.
        assert_eq!(pixel(2, 40), [255, 255, 255, 255]);
        assert_eq!(out.len(), base.len());
    }

    #[test]
    fn the_real_icon_gets_a_dot() {
        let image = Image::from_bytes(TRAY_PNG).unwrap();
        let out = with_dot(image.rgba(), image.width(), image.height());
        assert_eq!(out.len(), image.rgba().len());
        assert_ne!(out, image.rgba());
    }
}
