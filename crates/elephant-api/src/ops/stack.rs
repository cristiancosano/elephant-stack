//! Servicios propios de Elephant (06): `stack_list` (`stack list`),
//! `stack_control` (`stack control OPERACION SERVICIO RAMA`) y `stack_install`
//! (`stack install SERVICIO VERSION`). En esta fase, solo MySQL.

use elephant_core::services::{self, BranchStatus, ManagedService, RunState, ServiceKind, ServiceRelease};
use elephant_core::{tr, tr_str};
use serde::{Deserialize, Serialize};

use crate::text::table;
use crate::{Ctx, Kind, Op};

fn check_service(name: &str) -> Result<(), String> {
    ServiceKind::parse(name).map(drop).map_err(|_| {
        let name = crate::fit::truncate_chars(name, 32);
        tr!("Servicio desconocido: «{name}». Disponible: mysql.", "Unknown service: “{name}”. Available: mysql.")
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
        "Servicios propios de Elephant (sin Homebrew; hoy, MySQL): versiones instaladas, cada rama con su estado (running, stopped o error con el motivo), puerto, y versiones que se pueden instalar."
    }

    fn confirm_text(_a: &ListArgs) -> String {
        String::new()
    }

    fn run(_cx: &Ctx, _a: ListArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ListOut, String> {
        let services = services::status();
        let installed = |r: &ServiceRelease| {
            services.iter().any(|s| s.service == r.service && s.versions.iter().any(|v| v.version == r.version))
        };
        let available = services::catalog(false).releases.into_iter().filter(|r| !installed(r)).collect();
        Ok(ListOut { services, available })
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
                let (source, version) = (&offer.source, &offer.version);
                out.push_str(&format!(
                    "\n{}",
                    tr!(
                        "Hay datos de Homebrew en {source} (MySQL {version}): se copian desde la vista Servicios de la app.",
                        "There’s Homebrew data in {source} (MySQL {version}). Copy it from the Services view in the app."
                    )
                ));
            }
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
    /// Servicio: mysql.
    #[arg(value_name = "SERVICE")]
    pub service: String,
    /// Rama instalada: 8.4, 9.7…
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
        "Arranca o detiene una rama de un servicio propio de Elephant (mysql 8.4…), o activa o quita su arranque al iniciar sesión. Arrancar comprueba que responde y apaga las otras ramas del servicio."
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
    /// Servicio: mysql.
    #[arg(value_name = "SERVICE")]
    pub service: String,
    /// Versión del catálogo (stack list): 8.4.11…
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_services_and_versions() {
        assert!(check_service("mysql").is_ok());
        assert!(check_service("MySQL").is_err() && check_service("redis").is_err());
        assert!(check_numbers("8.4", 1..=2).is_ok() && check_numbers("18", 1..=2).is_ok());
        for bad in ["8.4.11", "", "8..4", "../x", "8.4; rm"] {
            assert!(check_numbers(bad, 1..=2).is_err(), "{bad}");
        }
        assert!(check_numbers("8.4.11", 2..=4).is_ok() && check_numbers("latest", 2..=4).is_err());
    }
}
