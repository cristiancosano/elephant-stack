//! `php_versions`, `php_set_global` (`php use`) y `php_set_project`
//! (`php isolate`). Los nombres de la CLI son los de Herd.

use std::path::PathBuf;

use elephant_core::php::{self, VersionInfo};
use elephant_core::project::{self, PhpReason, ProjectPhp};
use elephant_core::ops;
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
        let mut rows = vec![vec!["VERSIÓN".to_string(), "ORIGEN".into(), "CLAVE".into(), String::new()]];
        for v in &o.versions {
            let mark = if Some(v.key.as_str()) == active { "global" } else { "" };
            rows.push(vec![v.id.clone(), v.source.to_string(), v.key.clone(), mark.to_string()]);
        }
        let project = &o.project;
        let reason = match project.reason {
            PhpReason::ProjectFile => format!(
                "por {}",
                project.file.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| ".php-version".into())
            ),
            PhpReason::Global => "por el ajuste global".to_string(),
            PhpReason::Newest => "la más reciente (no hay nada elegido)".to_string(),
        };
        let here = match &project.version {
            Some(v) => format!("{} usa PHP {} {reason}", project.dir.display(), v.id),
            None => format!(
                "{}: {} ({reason})",
                project.dir.display(),
                project.error.as_deref().unwrap_or("sin versión")
            ),
        };
        format!("{} versiones de PHP\n{}\n{here}", o.versions.len(), table(&rows))
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
        format!("cambiar el PHP global a {}", a.version)
    }

    fn run(_cx: &Ctx, a: UseArgs, _on_line: &mut dyn FnMut(&str)) -> Result<UseOut, String> {
        Ok(UseOut { global: ops::set_global_php(&a.version)?.active })
    }

    fn text(o: &UseOut) -> String {
        match &o.global {
            Some(v) => format!("PHP global: {} ({})", v.id, v.source),
            None => "PHP global cambiado".to_string(),
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
        let dir = a.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
        format!("usar PHP {} en el proyecto {dir}", a.version)
    }

    fn run(cx: &Ctx, a: IsolateArgs, _on_line: &mut dyn FnMut(&str)) -> Result<IsolateOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        ops::set_project_php(&dir, &a.version)?;
        Ok(IsolateOut { dir, version: a.version })
    }

    fn text(o: &IsolateOut) -> String {
        format!("{} usa PHP {} (.php-version)", o.dir.display(), o.version)
    }
}
