//! `projects_list`, `project_needs`, `project_start_services` y
//! `project_create`.

use std::path::PathBuf;

use elephant_core::needs::{self, Needs};
use elephant_core::project::{self, Template};
use elephant_logs::{ProjectKind, ProjectOrigin};
use elephant_core::{tr, tr_str};
use serde::{Deserialize, Serialize};

use super::{check_php_version, user_dir};
use crate::text::{ago, table};
use crate::{Ctx, Kind, Op};

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct NoArgs {}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ProjectLine {
    /// Lo que aceptan `--project` y los ids de fuente de logs.
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub hosts: Vec<String>,
    pub kind: ProjectKind,
    pub origin: ProjectOrigin,
    pub php_version: Option<String>,
    pub errors_24h: u32,
    pub last_log_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ProjectsOut {
    pub projects: Vec<ProjectLine>,
}

pub(crate) struct ProjectsList;

impl Op for ProjectsList {
    const ID: &'static str = "projects_list";
    const CLI: &'static [&'static str] = &["project", "list"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = NoArgs;
    type Out = ProjectsOut;

    fn about() -> &'static str {
        "Proyectos PHP y Laravel locales (sitios .test de Herd y carpetas añadidas): id, nombre, hosts, tipo, versión de PHP y errores de las últimas 24 h. El id es el que usan las órdenes de logs."
    }

    fn confirm_text(_a: &NoArgs) -> String {
        String::new()
    }

    fn run(cx: &Ctx, _a: NoArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ProjectsOut, String> {
        Ok(ProjectsOut {
            projects: elephant_logs::summaries(&cx.home)
                .into_iter()
                .map(|s| ProjectLine {
                    id: s.project.id,
                    name: s.project.name,
                    root: s.project.root,
                    hosts: s.project.hosts,
                    kind: s.project.kind,
                    origin: s.project.origin,
                    php_version: s.project.php_version,
                    errors_24h: s.errors_24h,
                    last_log_ms: s.last_ms,
                })
                .collect(),
        })
    }

    fn text(o: &ProjectsOut) -> String {
        if o.projects.is_empty() {
            return tr!(
                "No hay proyectos: aparca una carpeta en Herd o añádela en Ajustes > Logs.",
                "No projects: park a folder in Herd or add it in Settings > Logs."
            );
        }
        let now = elephant_core::now_ms();
        let mut rows = vec![vec![
            "ID".to_string(),
            "HOST".into(),
            tr!("TIPO", "TYPE"),
            "PHP".into(),
            tr!("ERRORES 24 H", "ERRORS 24 H"),
            tr!("ÚLTIMO LOG", "LAST LOG"),
        ]];
        for p in &o.projects {
            rows.push(vec![
                p.id.clone(),
                p.hosts.first().cloned().unwrap_or_else(|| "—".into()),
                match p.kind {
                    ProjectKind::Laravel => "laravel".into(),
                    ProjectKind::Php => "php".into(),
                    ProjectKind::Other => tr!("otro", "other"),
                },
                p.php_version.clone().unwrap_or_else(|| "—".into()),
                p.errors_24h.to_string(),
                p.last_log_ms.map(|ms| ago(ms, now)).unwrap_or_else(|| "—".into()),
            ]);
        }
        let n = o.projects.len();
        let count = if n == 1 { tr!("1 proyecto", "1 project") } else { tr!("{n} proyectos", "{n} projects") };
        format!("{count}\n{}", table(&rows))
    }
}

// ── project needs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct DirArgs {
    /// Carpeta del proyecto (por defecto, la raíz del proyecto actual).
    #[arg(long)]
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct NeedsOut {
    pub dir: PathBuf,
    #[serde(flatten)]
    pub needs: Needs,
}

pub(crate) struct ProjectNeeds;

impl Op for ProjectNeeds {
    const ID: &'static str = "project_needs";
    const CLI: &'static [&'static str] = &["project", "needs"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = DirArgs;
    type Out = NeedsOut;

    fn about() -> &'static str {
        "Servicios que necesita un proyecto (mysql, postgresql, redis, mailpit…) según su .env, con el motivo de cada uno. Solo lee el .env; nunca lo escribe ni muestra sus valores."
    }

    fn validate(cx: &Ctx, a: &mut DirArgs) -> Result<(), String> {
        a.dir = Some(user_dir(cx, a.dir.as_deref())?);
        Ok(())
    }

    fn confirm_text(_a: &DirArgs) -> String {
        String::new()
    }

    fn run(cx: &Ctx, a: DirArgs, _on_line: &mut dyn FnMut(&str)) -> Result<NeedsOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        Ok(NeedsOut { needs: needs::project_needs(&dir), dir })
    }

    fn text(o: &NeedsOut) -> String {
        let mut lines = vec![if o.needs.services.is_empty() {
            let dir = o.dir.display();
            tr!("{dir} no necesita servicios de Homebrew", "{dir} doesn’t need any Homebrew services")
        } else {
            let (dir, list) = (o.dir.display(), o.needs.services.join(", "));
            tr!("{dir} necesita: {list}", "{dir} needs: {list}")
        }];
        lines.extend(o.needs.reasons.iter().map(|r| format!("- {r}")));
        lines.join("\n")
    }
}

// ── project start-services ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct StartedOut {
    pub dir: PathBuf,
    /// Los que se han arrancado (sin arranque al iniciar sesión).
    pub started: Vec<String>,
}

pub(crate) struct ProjectStartServices;

impl Op for ProjectStartServices {
    const ID: &'static str = "project_start_services";
    const CLI: &'static [&'static str] = &["project", "start-services"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = DirArgs;
    type Out = StartedOut;

    fn about() -> &'static str {
        "Arranca los servicios que necesita un proyecto según su .env (mysql, redis, mailpit…) y que estén parados, sin activarlos al iniciar sesión. Devuelve los que ha arrancado."
    }

    fn validate(cx: &Ctx, a: &mut DirArgs) -> Result<(), String> {
        a.dir = Some(user_dir(cx, a.dir.as_deref())?);
        Ok(())
    }

    fn confirm_text(a: &DirArgs) -> String {
        let dir = crate::text::dialog_path(a.dir.as_deref());
        tr!("arrancar los servicios que necesita {dir}", "start the services {dir} needs")
    }

    fn run(cx: &Ctx, a: DirArgs, _on_line: &mut dyn FnMut(&str)) -> Result<StartedOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        Ok(StartedOut { started: needs::start_project_services(&dir)?, dir })
    }

    fn text(o: &StartedOut) -> String {
        if o.started.is_empty() {
            tr!("No hacía falta arrancar nada", "Nothing needed starting")
        } else {
            let list = o.started.join(", ");
            tr!("Arrancados: {list}", "Started: {list}")
        }
    }
}

// ── project create ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TemplateArg {
    #[default]
    Laravel,
    Blank,
}

impl From<TemplateArg> for Template {
    fn from(t: TemplateArg) -> Self {
        match t {
            TemplateArg::Laravel => Template::Laravel,
            TemplateArg::Blank => Template::Blank,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct CreateArgs {
    /// Minúsculas, números y guiones: será también el host (`shop.test`).
    #[arg(value_name = "NAME")]
    pub name: String,
    /// laravel (composer create-project) o blank (public/index.php).
    #[arg(long, value_enum, default_value_t = TemplateArg::Laravel)]
    #[serde(default)]
    pub template: TemplateArg,
    /// Versión de PHP del proyecto (escribe .php-version).
    #[arg(long)]
    #[serde(default)]
    pub php: Option<String>,
    /// No esperar: devuelve el id de la tarea al momento.
    #[arg(long)]
    #[serde(default)]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct CreateOut {
    pub name: String,
    pub template: TemplateArg,
    pub root: PathBuf,
    /// `shop.test` si Herd lo sirve.
    pub host: Option<String>,
}

pub(crate) struct ProjectCreate;

impl Op for ProjectCreate {
    const ID: &'static str = "project_create";
    const CLI: &'static [&'static str] = &["project", "create"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = false;
    const LONG: bool = true;
    type Args = CreateArgs;
    type Out = CreateOut;

    fn about() -> &'static str {
        "Crea un proyecto Laravel (o una carpeta con public/index.php) en el directorio aparcado de Herd, que lo sirve como <nombre>.test. Opcional: versión de PHP. Tarea larga: puede devolver un job_id."
    }

    fn validate(_cx: &Ctx, a: &mut CreateArgs) -> Result<(), String> {
        elephant_core::validate::name(&a.name)?;
        if let Some(php) = a.php.as_mut() {
            *php = php.trim().to_string();
            check_php_version(php)?;
        }
        Ok(())
    }

    fn confirm_text(a: &CreateArgs) -> String {
        let kind = match a.template {
            TemplateArg::Laravel => "Laravel",
            TemplateArg::Blank => tr_str!("PHP vacío", "blank PHP"),
        };
        let php = a.php.as_ref().map(|v| tr!(" con PHP {v}", " with PHP {v}")).unwrap_or_default();
        let name = &a.name;
        tr!("crear el proyecto {kind} «{name}»{php} en Herd", "create {kind} project “{name}”{php} in Herd")
    }

    fn run(_cx: &Ctx, a: CreateArgs, on_line: &mut dyn FnMut(&str)) -> Result<CreateOut, String> {
        let created = project::create(&a.name, a.template.into(), a.php.as_deref(), on_line)?;
        Ok(CreateOut { name: a.name, template: a.template, root: created.root, host: created.host })
    }

    fn text(o: &CreateOut) -> String {
        let url = o.host.as_ref().map(|h| format!(" · http://{h}")).unwrap_or_default();
        let (name, root) = (&o.name, o.root.display());
        tr!(
            "Proyecto {name} creado en {root}{url}\nSiguiente paso: elephant project start-services --dir {root}",
            "Project {name} created in {root}{url}\nNext step: elephant project start-services --dir {root}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_text_cannot_be_split_by_a_folder_name() {
        let dir = PathBuf::from("/Users/u/x\n\nEsto solo consulta el estado; es seguro");
        let text = ProjectStartServices::confirm_text(&DirArgs { dir: Some(dir) });
        assert!(!text.contains('\n'), "{text}");
        assert!(text.starts_with("arrancar los servicios que necesita /Users/u/x\u{FFFD}\u{FFFD}Esto"));
    }
}
