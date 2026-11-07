//! Node y Composer de Elephant (06, fase 3): `node_list` (`node list`),
//! `node_install` (`node install VERSION`), `node_set_global` (`node use`),
//! `node_set_project` (`node isolate`), `node_reshim` (`node reshim`),
//! `composer_list` (`composer list`) y `composer_install` (`composer
//! install`). Sin desinstalar: eso solo se hace desde la app (00 §5.9).

use std::path::PathBuf;

use elephant_core::composer::{self, ComposerCatalog, ComposerChannel, ComposerStatus};
use elephant_core::node::{self, NodeChoice, NodeInstalled, NodeReason, NodeRelease};
use elephant_core::php::Layout;
use elephant_core::selectors::PathAdvice;
use elephant_core::tr;
use serde::{Deserialize, Serialize};

use super::user_dir;
use crate::text::table;
use crate::{Ctx, Kind, Op};

/// `24`, `v24.11.1`, `lts/*`, `lts/krypton` o `node`.
fn check_spec(text: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > 40 || text.chars().any(|c| c.is_control() || c.is_whitespace()) {
        let text = crate::fit::truncate_chars(text, 32);
        return Err(tr!(
            "Versión de Node no válida: «{text}». Usa, por ejemplo, 24, 24.11.1, lts o lts/krypton.",
            "Invalid Node version: “{text}”. Use, for example, 24, 24.11.1, lts or lts/krypton."
        ));
    }
    node::check_spec(text).map_err(|error| error.trim_start_matches("Elephant Stack: ").to_string())
}

fn reason_text(choice: &NodeChoice) -> String {
    match choice.reason {
        NodeReason::ProjectFile => {
            let file = choice.file.clone().unwrap_or_else(|| ".nvmrc".into());
            tr!("por {file}", "from {file}")
        }
        NodeReason::Global => tr!("por el ajuste global", "from the global setting"),
        NodeReason::Newest => tr!("la más reciente (no hay nada elegido)", "newest (nothing chosen)"),
    }
}

fn path_warning(path: &PathAdvice) -> Option<String> {
    let line = &path.line;
    match (&path.shadowed_by, path.configured) {
        (Some(who), _) => Some(tr!(
            "Aviso: en ~/.zshrc, {who} se carga después de ~/.elephant-stack/bin y queda delante en el PATH. Mueve esta línea al final: {line}",
            "Warning: in ~/.zshrc, {who} loads after ~/.elephant-stack/bin and ends up first in PATH. Move this line to the end: {line}"
        )),
        (None, false) => Some(tr!(
            "Aviso: ~/.elephant-stack/bin no está en el PATH de la shell. Añade al final de ~/.zshrc: {line}",
            "Warning: ~/.elephant-stack/bin isn’t in the shell’s PATH. Add this to the end of ~/.zshrc: {line}"
        )),
        _ => None,
    }
}

// ── node_list ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ListArgs {
    /// Carpeta del proyecto (por defecto, la actual).
    #[arg(long)]
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// Todas las versiones mayores, no solo las LTS con soporte y la Current.
    #[arg(long)]
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ListOut {
    /// Instaladas, de la más reciente a la más antigua.
    pub versions: Vec<NodeInstalled>,
    /// El ajuste global (`24`, `lts/*`); vacío = la más reciente.
    pub global: Option<String>,
    /// La que resuelve la carpeta, con el motivo.
    pub here: NodeChoice,
    /// La más reciente de cada mayor que se puede instalar (sin red ni caché, vacío).
    pub available: Vec<NodeRelease>,
    /// Si la terminal encuentra antes los selectores de Elephant que nvm o Herd.
    pub path: Option<PathAdvice>,
    /// `~/.nvm`, si existe: Elephant no lo usa.
    pub nvm: Option<String>,
}

pub(crate) struct NodeList;

impl Op for NodeList {
    const ID: &'static str = "node_list";
    const CLI: &'static [&'static str] = &["node", "list"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ListArgs;
    type Out = ListOut;

    fn about() -> &'static str {
        "Versiones de Node de Elephant (sin nvm): instaladas con sus paquetes globales, la global, la que usa una carpeta y por qué (.nvmrc, .node-version, global o la más reciente), las que se pueden instalar (LTS con soporte y Current; --all, todas) y el aviso del PATH."
    }

    fn validate(cx: &Ctx, a: &mut ListArgs) -> Result<(), String> {
        a.dir = Some(a.dir.clone().unwrap_or_else(|| cx.cwd.clone()));
        Ok(())
    }

    fn confirm_text(_a: &ListArgs) -> String {
        String::new()
    }

    fn run(cx: &Ctx, a: ListArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ListOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        let status = node::status(std::slice::from_ref(&dir));
        let installed = |r: &NodeRelease| status.versions.iter().any(|v| v.version == r.version);
        let available =
            node::catalog(false).releases.into_iter().filter(|r| (a.all || r.featured) && !installed(r)).collect();
        let here = status.projects.into_iter().next().unwrap_or_else(|| node::choice_for(&elephant_php_layout(cx), &dir));
        Ok(ListOut { versions: status.versions, global: status.global, here, available, path: status.path, nvm: status.nvm })
    }

    fn text(o: &ListOut) -> String {
        let mut rows = vec![vec![tr!("VERSIÓN", "VERSION"), "LTS".into(), "NPM".into(), tr!("GLOBALES", "GLOBALS"), String::new()]];
        for v in &o.versions {
            let mark = if o.here.version.as_deref() == Some(v.version.as_str()) { tr!("aquí", "here") } else { String::new() };
            rows.push(vec![
                v.version.clone(),
                v.lts.clone().unwrap_or_default(),
                v.npm.clone().unwrap_or_default(),
                v.globals.join(" "),
                mark,
            ]);
        }
        let n = o.versions.len();
        let mut out = if n == 0 {
            tr!("No hay ninguna versión de Node de Elephant.", "No Elephant Node versions.")
        } else {
            let count = if n == 1 { tr!("1 versión de Node", "1 Node version") } else { tr!("{n} versiones de Node", "{n} Node versions") };
            format!("{count}\n{}", table(&rows))
        };
        let dir = &o.here.dir;
        let reason = reason_text(&o.here);
        out.push('\n');
        out.push_str(&match (&o.here.version, &o.here.error) {
            (Some(version), _) => tr!("{dir} usa Node {version} {reason}", "{dir} uses Node {version} {reason}"),
            (None, Some(error)) => format!("{dir}: {}", error.trim_start_matches("Elephant Stack: ")),
            (None, None) => format!("{dir}: {reason}"),
        });
        if !o.available.is_empty() {
            let list: Vec<String> = o
                .available
                .iter()
                .map(|r| match &r.lts {
                    Some(lts) => format!("{} (LTS {lts})", r.version),
                    None => r.version.clone(),
                })
                .collect();
            let list = list.join(", ");
            out.push_str(&format!("\n{}", tr!("Se pueden instalar: {list}", "Available to install: {list}")));
        }
        if let Some(warning) = o.path.as_ref().and_then(path_warning) {
            out.push_str(&format!("\n{warning}"));
        }
        if let Some(nvm) = &o.nvm {
            out.push_str(&format!(
                "\n{}",
                tr!(
                    "Hay nvm en {nvm}: Elephant no lo usa ni lo toca. En la terminal gana lo que vaya antes en el PATH.",
                    "nvm is in {nvm}. Elephant doesn’t use or touch it. In the terminal, whatever comes first in PATH wins."
                )
            ));
        }
        out
    }
}

fn elephant_php_layout(cx: &Ctx) -> Layout {
    Layout::from_home(&cx.home)
}

// ── node_install ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct InstallArgs {
    /// 24, 24.11.1, lts, lts/krypton o node (la más reciente).
    #[arg(value_name = "VERSION")]
    pub version: String,
    /// No esperar: devuelve el id de la tarea al momento.
    #[arg(long)]
    #[serde(default)]
    pub detach: bool,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct InstallOut {
    pub version: String,
}

pub(crate) struct NodeInstall;

impl Op for NodeInstall {
    const ID: &'static str = "node_install";
    const CLI: &'static [&'static str] = &["node", "install"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = true;
    type Args = InstallArgs;
    type Out = InstallOut;

    fn about() -> &'static str {
        "Descarga de nodejs.org (con su SHASUMS256) e instala una versión de Node en Elephant: 24 (la más reciente de la 24), 24.11.1, lts, lts/krypton o node. Crea los selectores node, npm y npx. No cambia la global. Tarea larga: espera 45 s y, si no ha terminado, devuelve un job_id."
    }

    fn validate(_cx: &Ctx, a: &mut InstallArgs) -> Result<(), String> {
        a.version = a.version.trim().to_string();
        check_spec(&a.version)
    }

    fn confirm_text(a: &InstallArgs) -> String {
        let version = &a.version;
        tr!("instalar Node {version}", "install Node {version}")
    }

    fn run(_cx: &Ctx, a: InstallArgs, on_line: &mut dyn FnMut(&str)) -> Result<InstallOut, String> {
        Ok(InstallOut { version: node::install(&a.version, on_line, &mut |_, _| {})? })
    }

    fn text(o: &InstallOut) -> String {
        let version = &o.version;
        tr!("Node {version} instalado", "Node {version} installed")
    }
}

// ── node use ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct UseArgs {
    /// 24, 24.11.1, lts/* o node (vuelve a «la más reciente»).
    #[arg(value_name = "VERSION")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct UseOut {
    pub global: Option<String>,
    /// La que resuelve ahora fuera de cualquier proyecto.
    pub active: Option<String>,
}

pub(crate) struct NodeSetGlobal;

impl Op for NodeSetGlobal {
    const ID: &'static str = "node_set_global";
    const CLI: &'static [&'static str] = &["node", "use"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = UseArgs;
    type Out = UseOut;

    fn about() -> &'static str {
        "Cambia la versión de Node global de Elephant (la que se usa fuera de un proyecto con .nvmrc o .node-version): 24, 24.11.1, lts/* o node. Solo versiones ya instaladas."
    }

    fn validate(_cx: &Ctx, a: &mut UseArgs) -> Result<(), String> {
        a.version = a.version.trim().to_string();
        check_spec(&a.version)
    }

    fn confirm_text(a: &UseArgs) -> String {
        let v = &a.version;
        tr!("cambiar el Node global a {v}", "change the global Node to {v}")
    }

    fn run(_cx: &Ctx, a: UseArgs, _on_line: &mut dyn FnMut(&str)) -> Result<UseOut, String> {
        let status = node::set_global(Some(&a.version))?;
        Ok(UseOut { global: status.global, active: status.active })
    }

    fn text(o: &UseOut) -> String {
        let global = o.global.clone().unwrap_or_default();
        match &o.active {
            Some(active) => tr!("Node global: {global} ({active})", "Global Node: {global} ({active})"),
            None => tr!("Node global: {global}", "Global Node: {global}"),
        }
    }
}

// ── node isolate ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct IsolateArgs {
    /// 24, 24.11.1, lts/* o lts/krypton.
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
    /// El `.nvmrc` (o `.node-version`) escrito.
    pub file: PathBuf,
    pub version: String,
}

pub(crate) struct NodeSetProject;

impl Op for NodeSetProject {
    const ID: &'static str = "node_set_project";
    const CLI: &'static [&'static str] = &["node", "isolate"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = IsolateArgs;
    type Out = IsolateOut;

    fn about() -> &'static str {
        "Fija la versión de Node de un proyecto escribiendo su .nvmrc (o su .node-version, si ya usa ese), compatible con nvm. Solo versiones instaladas y carpetas dentro de tu carpeta personal."
    }

    fn validate(cx: &Ctx, a: &mut IsolateArgs) -> Result<(), String> {
        a.version = a.version.trim().to_string();
        check_spec(&a.version)?;
        a.dir = Some(user_dir(cx, a.dir.as_deref())?);
        Ok(())
    }

    fn confirm_text(a: &IsolateArgs) -> String {
        let dir = crate::text::dialog_path(a.dir.as_deref());
        let version = &a.version;
        tr!("usar Node {version} en el proyecto {dir}", "use Node {version} in project {dir}")
    }

    fn run(cx: &Ctx, a: IsolateArgs, _on_line: &mut dyn FnMut(&str)) -> Result<IsolateOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        let file = node::set_project(&dir, &a.version)?;
        Ok(IsolateOut { dir, file, version: a.version })
    }

    fn text(o: &IsolateOut) -> String {
        let (file, version) = (o.file.display(), &o.version);
        tr!("{file}: Node {version}", "{file}: Node {version}")
    }
}

// ── node reshim ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ReshimArgs {}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ReshimOut {
    /// Selectores nuevos en ~/.elephant-stack/bin.
    pub added: Vec<String>,
    /// Los que ya no tiene ninguna versión.
    pub removed: Vec<String>,
    /// Ya había un archivo con ese nombre que no es de Elephant: no se ha tocado.
    pub skipped: Vec<String>,
}

pub(crate) struct NodeReshim;

impl Op for NodeReshim {
    const ID: &'static str = "node_reshim";
    const CLI: &'static [&'static str] = &["node", "reshim"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ReshimArgs;
    type Out = ReshimOut;

    fn about() -> &'static str {
        "Regenera los selectores de ~/.elephant-stack/bin para los ejecutables de las versiones de Node (node, npm, npx, corepack y paquetes globales como pnpm). npm -g por el selector ya lo hace solo."
    }

    fn confirm_text(_a: &ReshimArgs) -> String {
        tr!("regenerar los selectores de Node", "regenerate the Node selectors")
    }

    fn run(_cx: &Ctx, _a: ReshimArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ReshimOut, String> {
        let report = node::reshim()?;
        Ok(ReshimOut { added: report.added, removed: report.removed, skipped: report.skipped })
    }

    fn text(o: &ReshimOut) -> String {
        let mut parts = Vec::new();
        if !o.added.is_empty() {
            let list = o.added.join(", ");
            parts.push(tr!("nuevos: {list}", "added: {list}"));
        }
        if !o.removed.is_empty() {
            let list = o.removed.join(", ");
            parts.push(tr!("quitados: {list}", "removed: {list}"));
        }
        if !o.skipped.is_empty() {
            let list = o.skipped.join(", ");
            parts.push(tr!("sin tocar (no son de Elephant): {list}", "left alone (not Elephant’s): {list}"));
        }
        if parts.is_empty() {
            tr!("Selectores al día", "Selectors up to date")
        } else {
            format!("{} {}", tr!("Selectores:", "Selectors:"), parts.join(" · "))
        }
    }
}

// ── composer_list ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ComposerListArgs {
    /// Carpeta del proyecto (por defecto, la actual): dice con qué PHP se ejecuta ahí.
    #[arg(long)]
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ComposerListOut {
    pub installed: ComposerStatus,
    /// Las versiones de los canales stable y 2.2 LTS.
    pub catalog: ComposerCatalog,
    pub dir: PathBuf,
    /// El PHP con el que se ejecuta en `dir`.
    pub php: Option<String>,
    /// Por qué no hay PHP para `dir` (no está instalado el que pide su .php-version…).
    pub php_error: Option<String>,
}

pub(crate) struct ComposerList;

impl Op for ComposerList {
    const ID: &'static str = "composer_list";
    const CLI: &'static [&'static str] = &["composer", "list"];
    const KIND: Kind = Kind::Read;
    const IDEMPOTENT: bool = true;
    const LONG: bool = false;
    type Args = ComposerListArgs;
    type Out = ComposerListOut;

    fn about() -> &'static str {
        "Composer de Elephant: la versión instalada y su canal, las disponibles (stable y 2.2 LTS de getcomposer.org) y con qué PHP se ejecuta en una carpeta (el del selector de PHP, que respeta .php-version)."
    }

    fn validate(cx: &Ctx, a: &mut ComposerListArgs) -> Result<(), String> {
        a.dir = Some(a.dir.clone().unwrap_or_else(|| cx.cwd.clone()));
        Ok(())
    }

    fn confirm_text(_a: &ComposerListArgs) -> String {
        String::new()
    }

    fn run(cx: &Ctx, a: ComposerListArgs, _on_line: &mut dyn FnMut(&str)) -> Result<ComposerListOut, String> {
        let dir = a.dir.unwrap_or_else(|| cx.cwd.clone());
        let php = elephant_php_layout(cx).resolve(&dir).map(|v| v.id);
        Ok(ComposerListOut { installed: composer::status(), catalog: composer::catalog(false), dir, php_error: php.clone().err(), php: php.ok() })
    }

    fn text(o: &ComposerListOut) -> String {
        let latest = |channel| o.catalog.get(channel).map(|v| v.version.clone());
        let mut out = match (&o.installed.version, o.installed.installed) {
            (Some(version), _) => {
                let channel = o.installed.channel.unwrap_or(ComposerChannel::Stable);
                let mut line = format!("Composer {version}{}", if channel == ComposerChannel::Lts { " (2.2 LTS)" } else { "" });
                if let Some(latest) = latest(channel).filter(|l| l != version) {
                    line.push_str(&tr!(" · hay {latest}: composer install", " · {latest} available: composer install"));
                }
                line
            }
            (None, true) => tr!("Composer del asistente antiguo (sin versión): reinstálalo con composer install", "Composer from the old setup assistant (no version): reinstall it with composer install"),
            (None, false) => match latest(ComposerChannel::Stable) {
                Some(latest) => tr!("Composer sin instalar ({latest} disponible)", "Composer not installed ({latest} available)"),
                None => tr!("Composer sin instalar", "Composer not installed"),
            },
        };
        if let Some(lts) = latest(ComposerChannel::Lts) {
            out.push_str(&format!("\n{}", tr!("LTS para PHP < 7.2.5: {lts} (composer install --channel lts)", "LTS for PHP < 7.2.5: {lts} (composer install --channel lts)")));
        }
        let dir = o.dir.display();
        out.push_str(&format!(
            "\n{}",
            match (&o.php, &o.php_error) {
                (Some(php), _) => tr!("En {dir} se ejecuta con PHP {php}", "In {dir} it runs with PHP {php}"),
                (None, error) => format!("{dir}: {}", error.as_deref().unwrap_or("").trim_start_matches("Elephant Stack: ")),
            }
        ));
        out
    }
}

// ── composer_install ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Channel {
    Stable,
    Lts,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, clap::Args)]
pub(crate) struct ComposerInstallArgs {
    /// stable (por defecto) o lts (2.2, para PHP anterior a 7.2.5).
    #[arg(long, value_enum)]
    #[serde(default)]
    pub channel: Option<Channel>,
    /// No esperar: devuelve el id de la tarea al momento.
    #[arg(long)]
    #[serde(default)]
    pub detach: bool,
}

pub(crate) struct ComposerInstall;

impl Op for ComposerInstall {
    const ID: &'static str = "composer_install";
    const CLI: &'static [&'static str] = &["composer", "install"];
    const KIND: Kind = Kind::Mutate;
    const IDEMPOTENT: bool = true;
    const LONG: bool = true;
    type Args = ComposerInstallArgs;
    type Out = InstallOut;

    fn about() -> &'static str {
        "Instala o actualiza el Composer de Elephant (composer.phar oficial con su sha256) a la última del canal: stable o lts (2.2). Deja el selector composer, que usa el PHP del proyecto. Tarea larga: espera 45 s y, si no ha terminado, devuelve un job_id."
    }

    fn confirm_text(a: &ComposerInstallArgs) -> String {
        match a.channel {
            Some(Channel::Lts) => tr!("instalar Composer 2.2 LTS", "install Composer 2.2 LTS"),
            _ => tr!("instalar o actualizar Composer", "install or update Composer"),
        }
    }

    fn run(_cx: &Ctx, a: ComposerInstallArgs, on_line: &mut dyn FnMut(&str)) -> Result<InstallOut, String> {
        let channel = match a.channel {
            Some(Channel::Lts) => ComposerChannel::Lts,
            // Sin canal, el de la versión instalada (o stable).
            Some(Channel::Stable) => ComposerChannel::Stable,
            None => composer::status().channel.unwrap_or(ComposerChannel::Stable),
        };
        Ok(InstallOut { version: composer::install(channel, on_line)? })
    }

    fn text(o: &InstallOut) -> String {
        let version = &o.version;
        tr!("Composer {version} instalado", "Composer {version} installed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_node_versions() {
        for good in ["24", "v24.11.1", "lts", "lts/*", "lts/krypton", "node", "20.x"] {
            assert!(check_spec(good).is_ok(), "{good}");
        }
        for bad in ["", "system", "24; rm", "lts/-1", "a b", &"9".repeat(50)] {
            assert!(check_spec(bad).is_err(), "{bad}");
        }
    }
}
