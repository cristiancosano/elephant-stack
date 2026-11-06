//! Acciones de la app: la única implementación, que comparten los comandos, el
//! menú de la barra y el menú de la aplicación. Son síncronas: quien llama
//! decide el hilo.
//!
//! Patrón: `set_busy` → `ops::*` → `refresh` → `clear_busy`. Si falla, se
//! añade un problema `ActionFailed` y se devuelve el error.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use elephant_core::brew::Operation;
use elephant_core::services::{self, ServiceKind};
use elephant_core::{i18n, needs, ops, tr, tr_str};
use elephant_core::settings::{self, Editor, Settings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::state::{self, AppState, ProblemKind};
use crate::{app_menu, tray, window};

/// Objetivo de `busy` y de los problemas de las acciones sobre todos los servicios.
const ALL_SERVICES: &str = "services";
/// Objetivo de las acciones sobre PHP.
const PHP: &str = "php";

fn busy_label(op: Operation) -> &'static str {
    match op {
        Operation::TurnOn | Operation::TurnOnWithAutostart => tr_str!("Arrancando…", "Starting…"),
        Operation::TurnOff => tr_str!("Deteniendo…", "Stopping…"),
        Operation::Restart => tr_str!("Reiniciando…", "Restarting…"),
        Operation::EnableAutostart => {
            tr_str!("Activando el arranque al iniciar sesión…", "Turning on start at login…")
        }
        Operation::DisableAutostart => {
            tr_str!("Quitando el arranque al iniciar sesión…", "Turning off start at login…")
        }
    }
}

/// Envuelve una operación con el estado `busy`, el refresco y los problemas.
fn tracked<T>(
    app: &AppHandle,
    target: &str,
    label: &str,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    state::set_busy(app, target, label);
    let result = work();
    if let Err(error) = state::refresh(app) {
        eprintln!("No se pudo refrescar el estado: {error}");
    }
    state::clear_busy(app, target);
    match &result {
        Ok(_) => state::clear_problems(app, target),
        Err(error) => state::add_problem(app, target, error.clone(), ProblemKind::ActionFailed),
    }
    result
}

pub fn service_operation(app: &AppHandle, name: &str, op: Operation) -> Result<(), String> {
    tracked(app, name, busy_label(op), || ops::service_control(name, op)).map(drop)
}

pub fn services_bulk(app: &AppHandle, op: Operation) -> Result<(), String> {
    let label = match op {
        Operation::TurnOff => tr_str!("Deteniendo todos…", "Stopping all…"),
        _ => tr_str!("Reiniciando todos…", "Restarting all…"),
    };
    tracked(app, ALL_SERVICES, label, || ops::services_bulk(op)).map(drop)
}

pub fn install_component(app: &AppHandle, id: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    tracked(app, id, tr_str!("Instalando…", "Installing…"), || ops::install_component(id, on_line))
}

pub fn set_global_php(app: &AppHandle, key: &str) -> Result<(), String> {
    tracked(app, PHP, php_busy(), || ops::set_global_php(key)).map(drop)
}

pub fn set_project_php(app: &AppHandle, dir: &Path, key: &str) -> Result<(), String> {
    tracked(app, PHP, php_busy(), || ops::set_project_php(dir, key))
}

fn php_busy() -> &'static str {
    tr_str!("Cambiando la versión de PHP…", "Changing PHP version…")
}

/// Cambia `old` por `new` en la versión por defecto y en los proyectos que la
/// fijaban, y borra `old`. Los proyectos son los que conoce la app.
pub fn replace_php(
    app: &AppHandle,
    old: &str,
    new: &str,
) -> Result<elephant_core::php::PhpReplaceReport, String> {
    let roots: Vec<PathBuf> = app
        .state::<AppState>()
        .snapshot()
        .projects
        .into_iter()
        .map(|project| project.root)
        .collect();
    tracked(app, PHP, tr_str!("Actualizando PHP…", "Updating PHP…"), || ops::replace_php(old, new, &roots))
}

// ── Servicios propios (06) ──────────────────────────────────────────────────
// El objetivo de `busy` y de los problemas es la etiqueta de launchd de la
// rama (`sh.elephant.mysql.8.4`), la misma que lleva `BranchStatus.label`.

fn branch_target(kind: ServiceKind, branch: &str) -> String {
    services::label(kind, branch)
}

pub fn managed_power(
    app: &AppHandle,
    kind: ServiceKind,
    branch: &str,
    on: bool,
) -> Result<services::StartReport, String> {
    let label = if on { tr_str!("Arrancando…", "Starting…") } else { tr_str!("Deteniendo…", "Stopping…") };
    tracked(app, &branch_target(kind, branch), label, || services::set_running(kind, branch, on, &mut |_| {}))
}

pub fn managed_autostart(app: &AppHandle, kind: ServiceKind, branch: &str, on: bool) -> Result<(), String> {
    let label = if on {
        tr_str!("Activando el arranque al iniciar sesión…", "Turning on start at login…")
    } else {
        tr_str!("Quitando el arranque al iniciar sesión…", "Turning off start at login…")
    };
    tracked(app, &branch_target(kind, branch), label, || services::set_autostart(kind, branch, on))
}

pub fn managed_uninstall(app: &AppHandle, kind: ServiceKind, version: &str) -> Result<(), String> {
    let target = branch_target(kind, &kind.branch_of(version));
    tracked(app, &target, tr_str!("Desinstalando…", "Uninstalling…"), || services::uninstall(kind, version))
}

pub fn managed_import(app: &AppHandle, kind: ServiceKind, branch: &str) -> Result<services::ImportReport, String> {
    tracked(app, &branch_target(kind, branch), tr_str!("Copiando los datos…", "Copying the data…"), || {
        services::import_brew_data(kind, &mut |_| {}, &mut |_, _| {})
    })
}

/// Texto de `busy` mientras se arranca lo que necesita un proyecto (lo
/// muestra también el submenú del sitio en el tray).
pub fn needs_busy() -> &'static str {
    tr_str!("Arrancando lo que necesita…", "Starting what it needs…")
}

/// Error de una acción sobre un proyecto que ya no está en el estado.
pub fn project_gone() -> String {
    tr!("Ese proyecto ya no existe.", "That project no longer exists.")
}

/// «Arrancar lo que necesita» un proyecto (04 §4.10), desde el tray o desde
/// la ventana. El objetivo de `busy` y del problema es el id del proyecto, o
/// la carpeta si no es un proyecto conocido. Devuelve los servicios arrancados.
pub fn start_project_services(app: &AppHandle, dir: &Path) -> Result<Vec<String>, String> {
    let target = app
        .state::<AppState>()
        .snapshot()
        .projects
        .into_iter()
        .find(|project| project.root == dir)
        .map_or_else(|| dir.display().to_string(), |project| project.id);
    tracked(app, &target, needs_busy(), || needs::start_project_services(dir))
}

/// La interfaz no conoce `extra`: se conservan las claves guardadas que no
/// envía. Aplica el tema a la ventana y emite el estado nuevo.
pub fn set_settings(app: &AppHandle, s: Settings) -> Result<Settings, String> {
    let s = normalize_php_source(s)?;
    let previous = app.state::<AppState>().settings().language;
    let merged = merge_extra(s, settings::load());
    settings::save(&merged)?;
    window::apply_theme(app, merged.appearance);
    // El idioma se aplica al momento: lo siguiente que se escriba ya sale en
    // el nuevo, y los menús nativos se vuelven a construir.
    let relabel = merged.language != previous && {
        let locale = i18n::resolve(merged.language);
        let changed = locale != i18n::locale();
        i18n::set_override(locale);
        changed
    };
    let mode = merged.mcp.mode;
    let locale = i18n::locale();
    state::update(app, |snapshot| {
        snapshot.settings = merged.clone();
        snapshot.mcp.mode = mode;
        snapshot.locale = locale;
    });
    if relabel {
        app_menu::rebuild(app);
        tray::rebuild(app);
        // Las descripciones de los componentes y los motivos de los proyectos
        // salen de core en el idioma de cuando se leyeron: se vuelven a leer.
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(error) = state::refresh(&handle) {
                eprintln!("No se pudo refrescar el estado: {error}");
            }
        });
    }
    Ok(merged)
}

/// Una fuente vacía vuelve a la por defecto; una mal escrita no se guarda.
fn normalize_php_source(mut s: Settings) -> Result<Settings, String> {
    s.php_source = match s.php_source.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(source) => {
            elephant_core::setup::parse_source(source)?;
            Some(source.to_string())
        }
    };
    Ok(s)
}

fn merge_extra(mut new: Settings, saved: Settings) -> Settings {
    for (key, value) in saved.extra {
        new.extra.entry(key).or_insert(value);
    }
    new
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum SiteAction {
    Browser,
    Editor,
    Terminal,
    CopyUrl,
}

/// Acciones de solo lectura sobre un proyecto (C17): Elephant no escribe en Herd.
pub fn site(app: &AppHandle, project_id: &str, action: SiteAction) -> Result<(), String> {
    let project = app
        .state::<AppState>()
        .snapshot()
        .projects
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or_else(project_gone)?;
    let url = || {
        let name = &project.name;
        project
            .hosts
            .first()
            .map(|host| format!("http://{host}"))
            .ok_or_else(|| tr!("{name} no tiene dirección web.", "{name} has no web address."))
    };
    match action {
        SiteAction::Browser => open_url(&url()?),
        SiteAction::CopyUrl => copy_to_clipboard(&url()?),
        SiteAction::Editor => open_in_editor(app, &project.root, None),
        SiteAction::Terminal => open_terminal(&project.root),
    }
}

pub fn open_in_editor(app: &AppHandle, file: &Path, line: Option<u32>) -> Result<(), String> {
    if !file.is_absolute() {
        let path = file.display();
        return Err(tr!("La ruta debe ser absoluta: {path}", "The path must be absolute: {path}"));
    }
    let editor = app.state::<AppState>().settings().editor;
    open(&[editor_url(editor, file, line).as_str()])
}

/// Solo `http` y `https`: nada de `file:` ni esquemas de otras apps.
pub fn open_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(tr!("Solo se pueden abrir direcciones web: {url}", "Only web addresses can be opened: {url}"));
    }
    open(&[url])
}

pub fn reveal_in_finder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        let path = path.display();
        return Err(tr!("No existe: {path}", "Doesn’t exist: {path}"));
    }
    open(&[OsStr::new("-R"), path.as_os_str()])
}

pub fn open_terminal(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        let dir = dir.display();
        return Err(tr!("No existe la carpeta {dir}", "The folder {dir} doesn’t exist"));
    }
    open(&[OsStr::new("-a"), OsStr::new("Terminal"), dir.as_os_str()])
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(copy_failed)?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).map_err(copy_failed)?;
    }
    let status = child.wait().map_err(copy_failed)?;
    if status.success() {
        Ok(())
    } else {
        Err(tr!("No se pudo copiar al portapapeles.", "Couldn’t copy to the clipboard."))
    }
}

fn copy_failed(error: std::io::Error) -> String {
    tr!("No se pudo copiar: {error}", "Couldn’t copy: {error}")
}

/// El log de un servicio de Homebrew, si existe.
pub fn service_log_path(name: &str) -> Option<PathBuf> {
    service_log_candidates(Path::new("/opt/homebrew/var/log"), name)
        .into_iter()
        .find(|path| path.is_file())
}

/// `name` es un servicio de brew o la etiqueta de launchd de una rama propia
/// (`sh.elephant.mysql.8.4`), que ningún nombre de brew puede tener.
pub fn open_service_log(name: &str) -> Result<(), String> {
    let own = name.starts_with(services::LABEL_PREFIX).then(|| services::log_for_label(name)).flatten();
    let path = own
        .or_else(|| service_log_path(name)).ok_or_else(|| tr!("{name} no tiene un archivo de log.", "{name} has no log file."))?;
    open(&[OsStr::new("-a"), OsStr::new("Console"), path.as_os_str()])
}

/// Rutas habituales del log de un servicio: `redis.log`,
/// `nginx/error.log`, y `postgresql.log` para `postgresql@18`.
fn service_log_candidates(dir: &Path, name: &str) -> Vec<PathBuf> {
    let safe = !name.is_empty()
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'@' | b'.' | b'-' | b'_'));
    if !safe {
        return Vec::new();
    }
    let mut candidates = vec![dir.join(format!("{name}.log")), dir.join(name).join("error.log")];
    if let Some((base, _)) = name.split_once('@') {
        candidates.push(dir.join(format!("{base}.log")));
    }
    candidates
}

fn open<S: AsRef<OsStr>>(args: &[S]) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo abrir: {error}", "Couldn’t open: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error = stderr.trim();
        Err(tr!("No se pudo abrir: {error}", "Couldn’t open: {error}"))
    }
}

/// URL que abre el archivo en el editor elegido, en la línea pedida.
fn editor_url(editor: Editor, file: &Path, line: Option<u32>) -> String {
    let path = percent_encode_path(&file.to_string_lossy());
    match editor {
        Editor::Phpstorm => match line {
            Some(line) => format!("phpstorm://open?file={path}&line={line}"),
            None => format!("phpstorm://open?file={path}"),
        },
        Editor::Vscode | Editor::Cursor => {
            let scheme = if editor == Editor::Cursor { "cursor" } else { "vscode" };
            match line {
                Some(line) => format!("{scheme}://file{path}:{line}"),
                None => format!("{scheme}://file{path}"),
            }
        }
    }
}

/// Codifica todo salvo los caracteres seguros de una ruta, `/` incluido.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~' | b'@') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_source_is_trimmed_validated_and_empty_means_default() {
        let with = |source: Option<&str>| Settings {
            php_source: source.map(str::to_string),
            ..Settings::default()
        };
        assert_eq!(normalize_php_source(with(Some("  "))).unwrap().php_source, None);
        assert_eq!(normalize_php_source(with(None)).unwrap().php_source, None);
        assert_eq!(
            normalize_php_source(with(Some(" yo/php "))).unwrap().php_source.as_deref(),
            Some("yo/php")
        );
        assert!(normalize_php_source(with(Some("http://inseguro.test/index.json"))).is_err());
        assert!(normalize_php_source(with(Some("no-es-un-repo"))).is_err());
    }

    #[test]
    fn editor_urls_for_each_editor() {
        let file = Path::new("/Users/ana/Code/shop/app/Http/Kernel.php");
        assert_eq!(
            editor_url(Editor::Vscode, file, Some(42)),
            "vscode://file/Users/ana/Code/shop/app/Http/Kernel.php:42"
        );
        assert_eq!(
            editor_url(Editor::Cursor, file, None),
            "cursor://file/Users/ana/Code/shop/app/Http/Kernel.php"
        );
        assert_eq!(
            editor_url(Editor::Phpstorm, file, Some(7)),
            "phpstorm://open?file=/Users/ana/Code/shop/app/Http/Kernel.php&line=7"
        );
    }

    #[test]
    fn paths_with_spaces_and_accents_are_encoded() {
        let file = Path::new("/Users/ana/Mis proyectos/año&x.php");
        assert_eq!(
            editor_url(Editor::Vscode, file, Some(1)),
            "vscode://file/Users/ana/Mis%20proyectos/a%C3%B1o%26x.php:1"
        );
    }

    #[test]
    fn only_web_urls_can_be_opened() {
        assert!(open_url("file:///etc/passwd").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
        assert!(open_url("phpstorm://open?file=/x").is_err());
    }

    #[test]
    fn service_log_candidates_cover_versioned_formulas() {
        let dir = Path::new("/opt/homebrew/var/log");
        assert_eq!(
            service_log_candidates(dir, "postgresql@18"),
            [
                dir.join("postgresql@18.log"),
                dir.join("postgresql@18/error.log"),
                dir.join("postgresql.log"),
            ]
        );
        assert!(service_log_candidates(dir, "../etc").is_empty());
        assert!(service_log_candidates(dir, "a/b").is_empty());
        assert!(service_log_candidates(dir, "").is_empty());
    }

    #[test]
    fn saved_extra_keys_survive_a_save_from_the_ui() {
        let mut saved = Settings::default();
        saved.extra.insert("futuro".into(), serde_json::json!(1));
        let new = Settings {
            watch_services: false,
            ..Settings::default()
        };
        let merged = merge_extra(new, saved);
        assert_eq!(merged.extra["futuro"], 1);
        assert!(!merged.watch_services);
    }

    #[test]
    fn every_operation_has_a_busy_label() {
        assert_eq!(busy_label(Operation::Restart), "Reiniciando…");
        assert_eq!(busy_label(Operation::TurnOnWithAutostart), "Arrancando…");
    }
}
