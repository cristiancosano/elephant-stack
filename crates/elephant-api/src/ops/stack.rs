//! Servicios propios de Elephant (06): `stack_list` (`stack list`),
//! `stack_control` (`stack control OPERACION SERVICIO RAMA`), `stack_install`
//! (`stack install SERVICIO VERSION`) y `stack_tool` (`stack tool OPERACION
//! HERRAMIENTA`). Servicios: MySQL, PostgreSQL, Redis y Mailpit; herramientas:
//! phpMyAdmin y pgAdmin.

use elephant_core::services::{self, BranchStatus, ManagedService, RunState, ServiceKind, ServiceRelease, ToolKind, ToolRelease, ToolStatus, tools};
use elephant_core::{tr, tr_str};
use serde::{Deserialize, Serialize};

use crate::text::table;
use crate::{Ctx, Kind, Op};

fn check_service(name: &str) -> Result<(), String> {
    ServiceKind::parse(name).map(drop).map_err(|_| {
        let name = crate::fit::truncate_chars(name, 32);
        let list = ServiceKind::ALL.map(ServiceKind::id).join(", ");
        tr!("Servicio desconocido: «{name}». Disponibles: {list}.", "Unknown service: “{name}”. Available: {list}.")
    })
}

fn check_tool(name: &str) -> Result<(), String> {
    ToolKind::parse(name).map(drop).map_err(|_| {
        let name = crate::fit::truncate_chars(name, 32);
        let list = ToolKind::ALL.map(ToolKind::id).join(", ");
        tr!("Herramienta desconocida: «{name}». Disponibles: {list}.", "Unknown tool: “{name}”. Available: {list}.")
    })
}

/// `8.4`, `8.4.11`: solo números y puntos, como mucho cuatro partes.
fn check_numbers(text: &str, parts: std::ops::RangeInclusive<usize>) -> Result<(), String> {
    let count = text.split('.').count();
    let valid = parts.contains(&count)
        && text.split('.').all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit()));
    if valid {
        Ok(())
    } else {
        let text = crate::fit::truncate_chars(text, 32);
        Err(tr!("Versión no válida: «{text}». Usa, por ejemplo, 8.4 o 8.4.11.", "Invalid version: “{text}”. Use, for example, 8.4 or 8.4.11."))
    }
}

fn state_text(b: &BranchStatus) -> String {
    match b.state {
        RunState::Running => tr!("en marcha", "running"),
        RunState::Stopped => tr!("parado", "stopped"),
        RunState::Error => b.error.clone().unwrap_or_else(|| tr!("error", "error")),
    }
}

// ── stack_list ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ListArgs {}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ListOut {
    /// Lo instalado por servicio: versiones, ramas con su estado y datos de Homebrew que se pueden copiar.
    pub services: Vec<ManagedService>,
    /// Versiones que se pueden instalar en este Mac y no lo están (vacío sin red ni caché).
    pub available: Vec<ServiceRelease>,
    /// Herramientas (phpMyAdmin, pgAdmin): lo instalado y su estado.
    pub tools: Vec<ToolStatus>,
    /// La versión más reciente de cada herramienta en el catálogo.
    pub tools_available: Vec<ToolRelease>,
}

pub(crate) struct StackList;

impl Op for StackList {
    const ID: &'static str = "stack_list";
    const CLI: &'static [&'static str] = &["stack", "list"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ListArgs;
    type Out = ListOut;

    fn about() -> &'static str {
        "Servicios propios de Elephant sin Homebrew (mysql, postgresql, redis, mailpit): versiones instaladas, cada rama con su estado (running, stopped o error con el motivo) y puerto, versiones que se pueden instalar y las herramientas phpmyadmin y pgadmin."
    }

    fn confirm_text(_a: &ListArgs) -> String {
        String::new()
    }

    fn run(_cx: &Ctx, _a: ListArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ListOut, String> {
        let services = services::status();
        let installed = |r: &ServiceRelease| {
            services.iter().any(|s| s.service == r.service && s.versions.iter().any(|v| v.version == r.version))
        };
        let catalog = services::catalog(false);
        let available = catalog.releases.iter().filter(|r| !installed(r)).cloned().collect();
        let tools_available = ToolKind::ALL.into_iter().filter_map(|t| catalog.latest_tool(t).cloned()).collect();
        Ok(ListOut { services, available, tools: tools::status(), tools_available })
    }

    fn text(o: &ListOut) -> String {
        let mut rows = vec![vec![
            tr!("SERVICIO", "SERVICE"),
            tr!("RAMA", "BRANCH"),
            tr!("VERSIÓN", "VERSION"),
            tr!("ESTADO", "STATE"),
            String::new(),
        ]];
        for s in &o.services {
            for b in &s.branches {
                let port = s.port;
                let mut extra = vec![tr!("puerto {port}", "port {port}")];
                if b.autostart {
                    extra.push(tr!("al iniciar sesión", "at login"));
                }
                rows.push(vec![s.service.id().into(), b.branch.clone(), b.version.clone(), state_text(b), extra.join(" · ")]);
            }
        }
        let mut out = if rows.len() > 1 { table(&rows) } else { tr!("No hay servicios propios instalados.", "No Elephant services installed.") };
        if !o.available.is_empty() {
            let list: Vec<String> = o.available.iter().map(|r| format!("{} {}", r.service.id(), r.version)).collect();
            let list = list.join(", ");
            out.push_str(&format!("\n{}", tr!("Se pueden instalar: {list}", "Available to install: {list}")));
        }
        for s in &o.services {
            if let Some(offer) = &s.import {
                let (source, version, name) = (&offer.source, &offer.version, &s.name);
                let line = match &offer.manual {
                    Some(manual) => tr!("Hay datos de Homebrew en {source} ({name} {version}). {manual}", "There’s Homebrew data in {source} ({name} {version}). {manual}"),
                    None => tr!(
                        "Hay datos de Homebrew en {source} ({name} {version}): se copian desde la vista Servicios de la app.",
                        "There’s Homebrew data in {source} ({name} {version}). Copy it from the Services view in the app."
                    ),
                };
                out.push_str(&format!("\n{line}"));
            }
        }
        for t in &o.tools {
            let name = &t.name;
            let latest = o.tools_available.iter().find(|r| r.tool == t.tool).map(|r| r.version.as_str());
            let mut line = match (&t.version, &t.external, latest) {
                (Some(version), _, Some(latest)) if latest != version => {
                    tr!("{name} {version} (hay {latest}: stack tool install)", "{name} {version} ({latest} available: stack tool install)")
                }
                (Some(version), _, _) => format!("{name} {version}"),
                (None, Some(path), _) => tr!("{name}: instalado fuera de Elephant en {path}", "{name}: installed outside Elephant in {path}"),
                (None, None, Some(latest)) => tr!("{name}: sin instalar ({latest} disponible)", "{name}: not installed ({latest} available)"),
                (None, None, None) => tr!("{name}: sin instalar", "{name}: not installed"),
            };
            if t.running
                && let Some(url) = &t.url
            {
                line.push_str(&format!(" · {}", tr!("en marcha en {url}", "running at {url}")));
            }
            if let Some(warning) = &t.warning {
                line.push_str(&format!(" · {warning}"));
            }
            out.push_str(&format!("\n{line}"));
        }
        out
    }
}

// ── stack_control ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StackOp {
    Start,
    Stop,
    AutostartOn,
    AutostartOff,
}

impl StackOp {
    fn verb(self) -> &'static str {
        match self {
            Self::Start => tr_str!("arrancar", "start"),
            Self::Stop => tr_str!("detener", "stop"),
            Self::AutostartOn => tr_str!("activar al iniciar sesión", "turn on start at login for"),
            Self::AutostartOff => tr_str!("quitar del inicio de sesión", "turn off start at login for"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ControlArgs {
    /// start, stop, autostart-on o autostart-off.
    #[arg(value_enum, value_name = "OPERACION")]
    pub operation: StackOp,
    /// Servicio: mysql, postgresql, redis o mailpit.
    #[arg(value_name = "SERVICE")]
    pub service: String,
    /// Rama instalada: 8.4, 18, 8.2, 1…
    #[arg(value_name = "BRANCH")]
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ControlOut {
    pub service: String,
    pub branch: String,
    pub operation: StackOp,
    /// Otras ramas del servicio que se han apagado al arrancar esta.
    pub stopped: Vec<String>,
    /// Su estado después de la operación.
    pub status: Option<BranchStatus>,
}

pub(crate) struct StackControl;

impl Op for StackControl {
    const ID: &'static str = "stack_control";
    const CLI: &'static [&'static str] = &["stack", "control"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ControlArgs;
    type Out = ControlOut;

    fn about() -> &'static str {
        "Arranca o detiene una rama de un servicio propio de Elephant (mysql 8.4, postgresql 18, redis 8.2, mailpit 1…), o activa o quita su arranque al iniciar sesión. Arrancar comprueba que responde, apaga las otras ramas y, si el puerto está ocupado, dice quién lo usa."
    }

    fn validate(_cx: &Ctx, a: &mut ControlArgs) -> Result<(), String> {
        check_service(&a.service)?;
        check_numbers(&a.branch, 1..=2)
    }

    fn confirm_text(a: &ControlArgs) -> String {
        format!("{} {} {}", a.operation.verb(), a.service, a.branch)
    }

    fn run(_cx: &Ctx, a: ControlArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ControlOut, String> {
        let kind = ServiceKind::parse(&a.service)?;
        let stopped = match a.operation {
            StackOp::Start => services::set_running(kind, &a.branch, true, &mut |_| {})?.stopped,
            StackOp::Stop => services::set_running(kind, &a.branch, false, &mut |_| {})?.stopped,
            StackOp::AutostartOn => services::set_autostart(kind, &a.branch, true).map(|()| Vec::new())?,
            StackOp::AutostartOff => services::set_autostart(kind, &a.branch, false).map(|()| Vec::new())?,
        };
        let status = services::status()
            .into_iter()
            .filter(|s| s.service == kind)
            .flat_map(|s| s.branches)
            .find(|b| b.branch == a.branch);
        Ok(ControlOut { service: a.service, branch: a.branch, operation: a.operation, stopped, status })
    }

    fn text(o: &ControlOut) -> String {
        let name = format!("{} {}", o.service, o.branch);
        let mut out = match &o.status {
            Some(b) => format!(
                "{name}: {}{}",
                state_text(b),
                if b.autostart { tr_str!(" · arranca al iniciar sesión", " · starts at login") } else { "" }
            ),
            None => tr!("{name}: hecho", "{name}: done"),
        };
        if !o.stopped.is_empty() {
            let list = o.stopped.join(", ");
            out.push_str(&format!(" · {}", tr!("detenido: {list}", "stopped: {list}")));
        }
        out
    }
}

// ── stack_install ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct InstallArgs {
    /// Servicio: mysql, postgresql, redis o mailpit.
    #[arg(value_name = "SERVICE")]
    pub service: String,
    /// Versión del catálogo (stack list): 8.4.11, 18.6.1…
    #[arg(value_name = "VERSION")]
    pub version: String,
    /// No esperar: devuelve el id de la tarea al momento.
    #[arg(long)]
    #[serde(default)]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct InstallOut {
    pub service: String,
    pub version: String,
}

pub(crate) struct StackInstall;

impl Op for StackInstall {
    const ID: &'static str = "stack_install";
    const CLI: &'static [&'static str] = &["stack", "install"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = true;
    type Args = InstallArgs;
    type Out = InstallOut;

    fn about() -> &'static str {
        "Descarga e instala una versión de un servicio propio de Elephant (binario oficial con su sha256) sin Homebrew. No lo arranca. Tarea larga: espera 45 s y, si no ha terminado, devuelve un job_id."
    }

    fn validate(_cx: &Ctx, a: &mut InstallArgs) -> Result<(), String> {
        check_service(&a.service)?;
        check_numbers(&a.version, 2..=4)
    }

    fn confirm_text(a: &InstallArgs) -> String {
        let (service, version) = (&a.service, &a.version);
        tr!("instalar {service} {version}", "install {service} {version}")
    }

    fn run(_cx: &Ctx, a: InstallArgs, on_line: &mut dyn FnMut(&str)) -> Result<InstallOut, String> {
        let kind = ServiceKind::parse(&a.service)?;
        services::install(kind, &a.version, on_line, &mut |_, _| {})?;
        Ok(InstallOut { service: a.service, version: a.version })
    }

    fn text(o: &InstallOut) -> String {
        let (service, version) = (&o.service, &o.version);
        tr!("{service} {version} instalado", "{service} {version} installed")
    }
}

// ── stack_tool ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolOp {
    Install,
    Uninstall,
    Start,
    Stop,
    Open,
}

impl ToolOp {
    fn verb(self) -> &'static str {
        match self {
            Self::Install => tr_str!("instalar o actualizar", "install or update"),
            Self::Uninstall => tr_str!("desinstalar", "uninstall"),
            Self::Start => tr_str!("encender el servidor de", "start the server of"),
            Self::Stop => tr_str!("apagar el servidor de", "stop the server of"),
            Self::Open => tr_str!("abrir", "open"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ToolArgs {
    /// install (o actualizar), uninstall, start, stop u open.
    #[arg(value_enum, value_name = "OPERACION")]
    pub operation: ToolOp,
    /// Herramienta: phpmyadmin o pgadmin.
    #[arg(value_name = "TOOL")]
    pub tool: String,
    /// No esperar: devuelve el id de la tarea al momento.
    #[arg(long)]
    #[serde(default)]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ToolOut {
    pub tool: String,
    pub operation: ToolOp,
    /// Lo instalado o abierto: la versión (install) o la URL o la app (open).
    pub detail: Option<String>,
    /// Su estado después de la operación.
    pub status: Option<ToolStatus>,
}

pub(crate) struct StackTool;

impl Op for StackTool {
    const ID: &'static str = "stack_tool";
    const CLI: &'static [&'static str] = &["stack", "tool"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = true;
    type Args = ToolArgs;
    type Out = ToolOut;

    fn about() -> &'static str {
        "Instala o actualiza (la última del catálogo), desinstala o abre phpmyadmin (servido con el PHP de Elephant en 127.0.0.1:8081) o pgadmin (app en ~/Applications, firma comprobada); start y stop encienden o apagan el servidor de phpmyadmin. Tarea larga: espera 45 s y, si no ha terminado, devuelve un job_id."
    }

    fn validate(_cx: &Ctx, a: &mut ToolArgs) -> Result<(), String> {
        check_tool(&a.tool)
    }

    fn confirm_text(a: &ToolArgs) -> String {
        format!("{} {}", a.operation.verb(), a.tool)
    }

    fn run(_cx: &Ctx, a: ToolArgs, on_line: &mut dyn FnMut(&str)) -> Result<ToolOut, String> {
        let tool = ToolKind::parse(&a.tool)?;
        let detail = match a.operation {
            ToolOp::Install => Some(tools::install(tool, on_line, &mut |_, _| {})?),
            ToolOp::Uninstall => tools::uninstall(tool).map(|()| None)?,
            ToolOp::Start => tools::set_running(tool, true).map(|()| None)?,
            ToolOp::Stop => tools::set_running(tool, false).map(|()| None)?,
            ToolOp::Open => Some(tools::open(tool)?),
        };
        let status = tools::status().into_iter().find(|t| t.tool == tool);
        Ok(ToolOut { tool: a.tool, operation: a.operation, detail, status })
    }

    fn text(o: &ToolOut) -> String {
        let name = o.status.as_ref().map_or(o.tool.as_str(), |s| s.name.as_str());
        let mut out = match (o.operation, &o.detail) {
            (ToolOp::Install, Some(version)) => tr!("{name} {version} instalado", "{name} {version} installed"),
            (ToolOp::Open, Some(target)) => tr!("{name}: abierto {target}", "{name}: opened {target}"),
            (ToolOp::Uninstall, _) => tr!("{name} desinstalado", "{name} uninstalled"),
            _ => match o.status.as_ref().map(|s| s.running) {
                Some(true) => tr!("{name}: en marcha", "{name}: running"),
                _ => tr!("{name}: parado", "{name}: stopped"),
            },
        };
        if let Some(warning) = o.status.as_ref().and_then(|s| s.warning.as_ref()) {
            out.push_str(&format!(" · {warning}"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_services_and_versions() {
        assert!(check_service("mysql").is_ok() && check_service("redis").is_ok() && check_service("postgresql").is_ok());
        assert!(check_service("MySQL").is_err() && check_service("postgresql@18").is_err());
        assert!(check_service("x").unwrap_err().contains("mysql, postgresql, redis, mailpit"));
        assert!(check_tool("phpmyadmin").is_ok() && check_tool("pgadmin").is_ok() && check_tool("adminer").is_err());
        assert!(check_numbers("8.4", 1..=2).is_ok() && check_numbers("18", 1..=2).is_ok());
        for bad in ["8.4.11", "", "8..4", "../x", "8.4; rm"] {
            assert!(check_numbers(bad, 1..=2).is_err(), "{bad}");
        }
        assert!(check_numbers("8.4.11", 2..=4).is_ok() && check_numbers("latest", 2..=4).is_err());
    }
}
