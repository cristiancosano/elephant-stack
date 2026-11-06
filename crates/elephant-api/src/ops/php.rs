//! `php_versions`, `php_set_global` (`php use`) y `php_set_project`
//! (`php isolate`). Los nombres de la CLI son los de Herd.

use std::path::PathBuf;

use elephant_core::php::{self, VersionInfo};
use elephant_core::project::{self, PhpReason, ProjectPhp};
use elephant_core::ops;
use elephant_core::tr;
use serde::{Deserialize, Serialize};

use super::{check_php_version, user_dir};
use crate::text::table;
use crate::{Ctx, Kind, Op};

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct VersionsArgs {
    /// Carpeta del proyecto (por defecto, la actual o la raíz del proyecto).
    #[arg(long)]
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct VersionsOut {
    /// Instaladas, de la más reciente a la más antigua.
    pub versions: Vec<VersionInfo>,
    /// La que se usa fuera de cualquier proyecto.
    pub global: Option<VersionInfo>,
    /// La que resuelve la carpeta, con el motivo.
    pub project: ProjectPhp,
}

pub(crate) struct PhpVersions;

impl Op for PhpVersions {
    const ID: &'static str = "php_versions";
    const CLI: &'static [&'static str] = &["php", "list"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = VersionsArgs;
    type Out = VersionsOut;

    fn about() -> &'static str {
        "Versiones de PHP instaladas (Elephant Stack y Herd), la global y la que usa una carpeta de proyecto, con el motivo: .php-version, ajuste global o la más reciente."
    }

    fn validate(cx: &Ctx, a: &mut VersionsArgs) -> Result<(), String> {
        a.dir = Some(cx.project_dir(a.dir.as_deref()));
        Ok(())
    }

    fn confirm_text(_a: &VersionsArgs) -> String {
        String::new()
    }

    fn run(cx: &Ctx, a: VersionsArgs, _on_line: &mut dyn FnMut(&str)) -> Result<VersionsOut, String> {
        let status = php::status()?;
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        Ok(VersionsOut { versions: status.versions, global: status.active, project: project::php_for_dir(&cx.home, &dir) })
    }

    fn text(o: &VersionsOut) -> String {
        let active = o.global.as_ref().map(|v| v.key.as_str());
        let mut rows = vec![vec![tr!("VERSIÓN", "VERSION"), tr!("ORIGEN", "SOURCE"), tr!("CLAVE", "KEY"), String::new()]];
        for v in &o.versions {
            let mark = if Some(v.key.as_str()) == active { "global" } else { "" };
            rows.push(vec![v.id.clone(), v.source.to_string(), v.key.clone(), mark.to_string()]);
        }
        let project = &o.project;
        let reason = match project.reason {
            PhpReason::ProjectFile => {
                let file =
                    project.file.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| ".php-version".into());
                tr!("por {file}", "from {file}")
            }
            PhpReason::Global => tr!("por el ajuste global", "from the global setting"),
            PhpReason::Newest => tr!("la más reciente (no hay nada elegido)", "newest (nothing chosen)"),
        };
        let dir = project.dir.display();
        let here = match &project.version {
            Some(v) => {
                let id = &v.id;
                tr!("{dir} usa PHP {id} {reason}", "{dir} uses PHP {id} {reason}")
            }
            None => {
                let error = project.error.clone().unwrap_or_else(|| tr!("sin versión", "no version"));
                format!("{dir}: {error} ({reason})")
            }
        };
        let n = o.versions.len();
        let count = if n == 1 { tr!("1 versión de PHP", "1 PHP version") } else { tr!("{n} versiones de PHP", "{n} PHP versions") };
        format!("{count}\n{}\n{here}", table(&rows))
    }
}

// ── php use ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct UseArgs {
    /// `8.4`, `8.4.27` o `8.4@Herd` (la clave de `elephant php list`).
    #[arg(value_name = "VERSION")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct UseOut {
    pub global: Option<VersionInfo>,
}

pub(crate) struct PhpSetGlobal;

impl Op for PhpSetGlobal {
    const ID: &'static str = "php_set_global";
    const CLI: &'static [&'static str] = &["php", "use"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = UseArgs;
    type Out = UseOut;

    fn about() -> &'static str {
        "Cambia la versión de PHP global (la que se usa fuera de un proyecto con .php-version). Solo versiones ya instaladas; la app y la barra de menús lo ven al momento."
    }

    fn validate(_cx: &Ctx, a: &mut UseArgs) -> Result<(), String> {
        a.version = a.version.trim().to_string();
        check_php_version(&a.version)
    }

    fn confirm_text(a: &UseArgs) -> String {
        let v = &a.version;
        tr!("cambiar el PHP global a {v}", "change the global PHP to {v}")
    }

    fn run(_cx: &Ctx, a: UseArgs, _on_line: &mut dyn FnMut(&str)) -> Result<UseOut, String> {
        Ok(UseOut { global: ops::set_global_php(&a.version)?.active })
    }

    fn text(o: &UseOut) -> String {
        match &o.global {
            Some(v) => {
                let (id, source) = (&v.id, &v.source);
                tr!("PHP global: {id} ({source})", "Global PHP: {id} ({source})")
            }
            None => tr!("PHP global cambiado", "Global PHP changed"),
        }
    }
}

// ── php isolate ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct IsolateArgs {
    /// `8.4`, `8.4.27` o `8.4@Herd`.
    #[arg(value_name = "VERSION")]
    pub version: String,
    /// Carpeta del proyecto (por defecto, la raíz del proyecto actual).
    #[arg(long)]
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct IsolateOut {
    pub dir: PathBuf,
    pub version: String,
}

pub(crate) struct PhpSetProject;

impl Op for PhpSetProject {
    const ID: &'static str = "php_set_project";
    const CLI: &'static [&'static str] = &["php", "isolate"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = IsolateArgs;
    type Out = IsolateOut;

    fn about() -> &'static str {
        "Fija la versión de PHP de un proyecto escribiendo su .php-version (como herd isolate). Solo versiones instaladas y carpetas dentro de tu carpeta personal."
    }

    fn validate(cx: &Ctx, a: &mut IsolateArgs) -> Result<(), String> {
        a.version = a.version.trim().to_string();
        check_php_version(&a.version)?;
        a.dir = Some(user_dir(cx, a.dir.as_deref())?);
        Ok(())
    }

    fn confirm_text(a: &IsolateArgs) -> String {
        let dir = crate::text::dialog_path(a.dir.as_deref());
        let version = &a.version;
        tr!("usar PHP {version} en el proyecto {dir}", "use PHP {version} in project {dir}")
    }

    fn run(cx: &Ctx, a: IsolateArgs, _on_line: &mut dyn FnMut(&str)) -> Result<IsolateOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        ops::set_project_php(&dir, &a.version)?;
        Ok(IsolateOut { dir, version: a.version })
    }

    fn text(o: &IsolateOut) -> String {
        let (dir, version) = (o.dir.display(), &o.version);
        tr!("{dir} usa PHP {version} (.php-version)", "{dir} uses PHP {version} (.php-version)")
    }
}
