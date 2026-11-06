//! `service_control` y `component_install`.

use elephant_core::brew::{Operation, Service};
use elephant_core::{components, ops};
use elephant_core::{tr, tr_str};
use serde::{Deserialize, Serialize};

use super::check_service_name;
use crate::{Ctx, Kind, Op};

/// Lo que se puede hacer con un servicio. Se traduce a `brew::Operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceOp {
    Start,
    Stop,
    Restart,
    AutostartOn,
    AutostartOff,
}

impl ServiceOp {
    fn operation(self) -> Operation {
        match self {
            Self::Start => Operation::TurnOn,
            Self::Stop => Operation::TurnOff,
            Self::Restart => Operation::Restart,
            Self::AutostartOn => Operation::EnableAutostart,
            Self::AutostartOff => Operation::DisableAutostart,
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Start => tr_str!("arrancar", "start"),
            Self::Stop => tr_str!("detener", "stop"),
            Self::Restart => tr_str!("reiniciar", "restart"),
            Self::AutostartOn => tr_str!("activar al iniciar sesión", "turn on start at login for"),
            Self::AutostartOff => tr_str!("quitar del inicio de sesión", "turn off start at login for"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ServiceArgs {
    /// start, stop, restart, autostart-on o autostart-off.
    #[arg(value_enum, value_name = "OPERACION")]
    pub operation: ServiceOp,
    /// Servicio de Homebrew: mysql, postgresql@18, redis…
    #[arg(value_name = "NAME")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ServiceOut {
    pub name: String,
    pub operation: ServiceOp,
    /// Su estado después de la operación.
    pub service: Option<Service>,
}

pub(crate) struct ServiceControl;

impl Op for ServiceControl {
    const ID: &'static str = "service_control";
    const CLI: &'static [&'static str] = &["service"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ServiceArgs;
    type Out = ServiceOut;

    fn about() -> &'static str {
        "Arranca, detiene o reinicia un servicio de Homebrew (mysql, postgresql, redis, mailpit, nginx…) o activa o quita su arranque al iniciar sesión. Usa esto en lugar de brew services."
    }

    fn validate(_cx: &Ctx, a: &mut ServiceArgs) -> Result<(), String> {
        check_service_name(&a.name)
    }

    fn confirm_text(a: &ServiceArgs) -> String {
        format!("{} {}", a.operation.verb(), a.name)
    }

    fn run(_cx: &Ctx, a: ServiceArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ServiceOut, String> {
        let services = ops::service_control(&a.name, a.operation.operation())?;
        Ok(ServiceOut {
            service: services.into_iter().find(|s| s.name == a.name),
            name: a.name,
            operation: a.operation,
        })
    }

    fn text(o: &ServiceOut) -> String {
        match &o.service {
            Some(s) => format!(
                "{}: {}{}",
                o.name,
                if s.running { tr_str!("en marcha", "running") } else { tr_str!("parado", "stopped") },
                if s.registered { tr_str!(" · arranca al iniciar sesión", " · starts at login") } else { "" }
            ),
            None => {
                let name = &o.name;
                tr!("{name}: hecho", "{name}: done")
            }
        }
    }
}

// ── component_install ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct InstallArgs {
    /// mysql, postgresql@18, redis, mailpit, nginx, dnsmasq o composer.
    #[arg(value_name = "ID")]
    pub id: String,
    /// No esperar: devuelve el id de la tarea al momento.
    #[arg(long)]
    #[serde(default)]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct InstallOut {
    pub id: String,
    pub installed: bool,
}

pub(crate) struct ComponentInstall;

impl Op for ComponentInstall {
    const ID: &'static str = "component_install";
    const CLI: &'static [&'static str] = &["component", "install"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = true;
    type Args = InstallArgs;
    type Out = InstallOut;

    fn about() -> &'static str {
        "Instala un componente con Homebrew (mysql, postgresql@18, redis, mailpit, nginx, dnsmasq) o el Composer de Elephant. Tarea larga: espera 45 s y, si no ha terminado, devuelve un job_id."
    }

    fn validate(_cx: &Ctx, a: &mut InstallArgs) -> Result<(), String> {
        if a.id == "composer" || components::is_installable_service(&a.id) {
            Ok(())
        } else {
            let id = crate::fit::truncate_chars(&a.id, 64);
            Err(tr!(
                "No se puede instalar «{id}». Se pueden instalar: mysql, postgresql@18, redis, mailpit, nginx, dnsmasq y composer.",
                "Can’t install “{id}”. Installable: mysql, postgresql@18, redis, mailpit, nginx, dnsmasq and composer."
            ))
        }
    }

    fn confirm_text(a: &InstallArgs) -> String {
        if a.id == "composer" {
            tr!("instalar Composer", "install Composer")
        } else {
            let id = &a.id;
            tr!("instalar {id} con Homebrew", "install {id} with Homebrew")
        }
    }

    fn run(_cx: &Ctx, a: InstallArgs, on_line: &mut dyn FnMut(&str)) -> Result<InstallOut, String> {
        ops::install_component(&a.id, on_line)?;
        Ok(InstallOut { id: a.id, installed: true })
    }

    fn text(o: &InstallOut) -> String {
        let id = &o.id;
        tr!("{id} instalado", "{id} installed")
    }
}
