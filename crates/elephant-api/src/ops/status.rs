//! `elephant_status` y `components_list`.

use elephant_core::components::{self, Component, Kind as ComponentKind};
use elephant_core::jobs::{self, JobState};
use elephant_core::monitor::{self, Usage};
use elephant_core::project::{self, PhpReason, ProjectPhp};
use elephant_core::{launchd, now_ms, php};
use serde::{Deserialize, Serialize};

use crate::text::{ago, table};
use crate::{Ctx, Kind, Op};

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct StatusArgs {
    /// Resumen rápido (≤ 30 ms, sin brew): lo que inyecta la skill.
    #[arg(long)]
    #[serde(default)]
    pub brief: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ServiceLine {
    pub name: String,
    pub running: bool,
    pub pid: Option<i64>,
    /// Último código de salida (de launchd con `brief`; de brew sin él).
    pub last_exit: Option<i64>,
    /// Arranca al iniciar sesión (sin `brief`).
    pub autostart: Option<bool>,
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct JobLine {
    pub id: String,
    pub kind: String,
    pub state: JobState,
    pub client: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ErrorCount {
    /// Id de proyecto, `none` (nginx sin proyecto) o `system` (PHP-FPM).
    pub project: String,
    pub errors: u32,
    pub last_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct StatusOut {
    pub brief: bool,
    pub services: Vec<ServiceLine>,
    /// Versión global.
    pub php_global: Option<php::VersionInfo>,
    /// La del proyecto de la carpeta actual.
    pub php_project: ProjectPhp,
    /// Errores de la última hora por proyecto (sin `brief`).
    pub errors_1h: Vec<ErrorCount>,
    pub jobs: Vec<JobLine>,
}

pub(crate) struct Status;

impl Op for Status {
    const ID: &'static str = "elephant_status";
    const CLI: &'static [&'static str] = &["status"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = StatusArgs;
    type Out = StatusOut;

    fn about() -> &'static str {
        "Estado del entorno PHP local: servicios de Homebrew (en marcha, PID, RAM), PHP global y del proyecto, errores de la última hora por proyecto y tareas en curso. Con brief, un resumen sin brew en ≤ 30 ms."
    }

    fn confirm_text(_a: &StatusArgs) -> String {
        String::new()
    }

    fn run(cx: &Ctx, a: StatusArgs, _on_line: &mut dyn FnMut(&str)) -> Result<StatusOut, String> {
        let services = if a.brief { brief_services()? } else { full_services()? };
        let errors_1h = if a.brief {
            Vec::new()
        } else {
            let mut counts: Vec<ErrorCount> = elephant_logs::count_errors(&cx.home, now_ms() - 3_600_000)
                .into_iter()
                .filter(|(_, n, _)| *n > 0)
                .map(|(project, errors, last_ms)| ErrorCount { project, errors, last_ms })
                .collect();
            counts.sort_by(|a, b| b.errors.cmp(&a.errors).then_with(|| a.project.cmp(&b.project)));
            counts
        };
        Ok(StatusOut {
            brief: a.brief,
            services,
            php_global: php::status()?.active,
            php_project: project::php_for_dir(&cx.home, &cx.project_dir(None)),
            errors_1h,
            jobs: jobs::list_active()
                .into_iter()
                .map(|job| JobLine { id: job.id, kind: job.kind, state: job.state, client: job.client, message: job.message })
                .collect(),
        })
    }

    fn text(o: &StatusOut) -> String {
        let mut lines = Vec::new();
        let running = o.services.iter().filter(|s| s.running).count();
        let list: Vec<String> = o
            .services
            .iter()
            .map(|s| {
                let mut item = format!("{} {}", s.name, if s.running { "●" } else { "○" });
                if !s.running && let Some(code) = s.last_exit.filter(|code| *code != 0) {
                    item.push_str(&format!(" (salió con {code})"));
                }
                if let Some(mb) = s.memory_mb {
                    item.push_str(&format!(" {mb} MB"));
                }
                item
            })
            .collect();
        lines.push(if list.is_empty() {
            "Servicios: ninguno de Homebrew".to_string()
        } else {
            format!("Servicios ({running} en marcha): {}", list.join(" · "))
        });
        lines.push(php_line(o.php_global.as_ref(), &o.php_project));
        if !o.brief {
            if o.errors_1h.is_empty() {
                lines.push("Errores en la última hora: ninguno".to_string());
            } else {
                let now = now_ms();
                let items: Vec<String> = o
                    .errors_1h
                    .iter()
                    .map(|e| match e.last_ms {
                        Some(last) => format!("{} {} (último {})", e.project, e.errors, ago(last, now)),
                        None => format!("{} {}", e.project, e.errors),
                    })
                    .collect();
                lines.push(format!("Errores en la última hora: {} (elephant logs errors)", items.join(" · ")));
            }
        }
        if o.jobs.is_empty() {
            lines.push("Tareas en curso: ninguna".to_string());
        } else {
            for job in &o.jobs {
                lines.push(format!(
                    "Tarea {} {}: {}{}",
                    job.id,
                    job.kind,
                    job.message.as_deref().unwrap_or("en marcha"),
                    job.client.as_deref().map(|c| format!(" ({c})")).unwrap_or_default()
                ));
            }
        }
        lines.join("\n")
    }
}

fn php_line(global: Option<&php::VersionInfo>, project: &ProjectPhp) -> String {
    let global = global.map(|v| format!("{} ({})", v.id, v.source)).unwrap_or_else(|| "ninguna".into());
    let here = match (&project.version, project.reason) {
        (Some(v), PhpReason::ProjectFile) => format!("{} por .php-version", v.id),
        (Some(v), _) => format!("{} (la global)", v.id),
        (None, _) => format!("no disponible: {}", project.error.as_deref().unwrap_or("sin versiones")),
    };
    format!("PHP: global {global} · en esta carpeta {here}")
}

/// `launchctl list` (< 10 ms): nunca brew.
fn brief_services() -> Result<Vec<ServiceLine>, String> {
    let mut services: Vec<ServiceLine> = launchd::homebrew_jobs()?
        .into_iter()
        .filter_map(|job| {
            Some(ServiceLine {
                name: job.service_name()?.to_string(),
                running: job.pid.is_some(),
                pid: job.pid,
                last_exit: job.last_exit,
                autostart: None,
                memory_mb: None,
            })
        })
        .collect();
    services.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(services)
}

fn full_services() -> Result<Vec<ServiceLine>, String> {
    let services = elephant_core::brew::list_services()?;
    let pids: Vec<i64> = services.iter().filter_map(|s| s.pid).collect();
    let usage = monitor::usage_for(&pids).unwrap_or_default();
    Ok(services
        .into_iter()
        .map(|s| ServiceLine {
            memory_mb: s.pid.and_then(|pid| usage.iter().find(|u| u.pid == pid)).map(|u| u.memory_kb / 1024),
            name: s.name,
            running: s.running,
            pid: s.pid,
            last_exit: s.exit_code,
            autostart: Some(s.registered),
        })
        .collect())
}

// ── components_list ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KindFilter {
    Service,
    Tool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ComponentsArgs {
    /// Solo servicios o solo herramientas.
    #[arg(long, value_enum)]
    #[serde(default)]
    pub kind: Option<KindFilter>,
    /// Solo los instalados.
    #[arg(long)]
    #[serde(default)]
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ComponentLine {
    #[serde(flatten)]
    pub component: Component,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ComponentsOut {
    pub components: Vec<ComponentLine>,
}

pub(crate) struct ComponentsList;

impl Op for ComponentsList {
    const ID: &'static str = "components_list";
    const CLI: &'static [&'static str] = &["component", "list"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ComponentsArgs;
    type Out = ComponentsOut;

    fn about() -> &'static str {
        "Componentes del entorno (mysql, postgresql, redis, mailpit, nginx, php, composer, node…): instalados o no, versión, origen (Homebrew, Herd, Elephant Stack), estado del servicio y consumo de RAM y CPU."
    }

    fn confirm_text(_a: &ComponentsArgs) -> String {
        String::new()
    }

    fn run(_cx: &Ctx, a: ComponentsArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ComponentsOut, String> {
        let list: Vec<Component> = components::list()?
            .into_iter()
            .filter(|c| match a.kind {
                Some(KindFilter::Service) => c.kind == ComponentKind::Service,
                Some(KindFilter::Tool) => c.kind == ComponentKind::Tool,
                None => true,
            })
            .filter(|c| !a.installed || c.installed)
            .collect();
        let pids: Vec<i64> = list.iter().filter_map(|c| c.service.as_ref()?.pid).collect();
        let usage = monitor::usage_for(&pids).unwrap_or_default();
        Ok(ComponentsOut {
            components: list
                .into_iter()
                .map(|component| {
                    let pid = component.service.as_ref().and_then(|s| s.pid);
                    let usage = pid.and_then(|pid| usage.iter().find(|u| u.pid == pid).cloned());
                    ComponentLine { component, usage }
                })
                .collect(),
        })
    }

    fn text(o: &ComponentsOut) -> String {
        let installed = o.components.iter().filter(|c| c.component.installed).count();
        let mut rows = vec![vec![
            "COMPONENTE".to_string(),
            "TIPO".into(),
            "ESTADO".into(),
            "VERSIÓN".into(),
            "ORIGEN".into(),
            "RAM".into(),
        ]];
        for line in &o.components {
            let c = &line.component;
            let state = match (&c.service, c.installed) {
                (_, false) => "no instalado".to_string(),
                (Some(s), true) if s.running => "en marcha".to_string(),
                (Some(_), true) => "parado".to_string(),
                (None, true) => "instalado".to_string(),
            };
            rows.push(vec![
                c.id.clone(),
                match c.kind {
                    ComponentKind::Service => "servicio".into(),
                    ComponentKind::Tool => "herramienta".into(),
                },
                state,
                c.version.clone().unwrap_or_else(|| "—".into()),
                c.origin.unwrap_or("—").to_string(),
                line.usage.as_ref().map(|u| format!("{} MB", u.memory_kb / 1024)).unwrap_or_else(|| "—".into()),
            ]);
        }
        format!("{} componentes, {installed} instalados\n{}", o.components.len(), table(&rows))
    }
}
