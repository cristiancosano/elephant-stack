//! Node de Elephant (06, fase 3), sin nvm ni Homebrew: el catálogo oficial
//! (`nodejs.org/dist/index.json`), los paquetes oficiales verificados con
//! `SHASUMS256.txt` en `~/.elephant-stack/node/<versión>/`, y los selectores
//! de `bin/` (`node`, `npm`, `npx`, `corepack` y los paquetes globales de npm).
//!
//! La elección de versión (`.nvmrc`, `.node-version`, global, la más reciente)
//! vive en `elephant_php::node`, que comparten el selector y la app.

use std::fs;
use std::path::{Path, PathBuf};

use elephant_php::Layout;
use elephant_php::node::{self as select, NodeSpec};
use serde::{Deserialize, Serialize};

use crate::services::install::{Package, unpack};
use crate::setup::catalog::MemoryCache;
use crate::setup::download::{self, Tools};
use crate::{fsutil, paths, selectors, stamp, tr};

pub const INDEX_URL: &str = "https://nodejs.org/dist/index.json";
const DIST: &str = "https://nodejs.org/dist";

static MEMORY: MemoryCache<Vec<NodeRelease>> = MemoryCache::new();

/// Una versión publicada en `index.json` para esta plataforma.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeRelease {
    /// `24.11.1`, sin la `v`.
    pub version: String,
    pub major: u32,
    /// `2025-11-11`.
    pub date: String,
    /// Nombre en clave de la LTS en minúsculas (`krypton`), si lo es.
    pub lts: Option<String>,
    /// La versión de npm que trae.
    pub npm: Option<String>,
    /// Corrige vulnerabilidades.
    #[serde(default)]
    pub security: bool,
    /// Se muestra por defecto: es la más reciente de una LTS con soporte o
    /// de la Current.
    #[serde(default)]
    pub featured: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeCatalog {
    /// La más reciente de cada versión mayor, de la más reciente a la más antigua.
    pub releases: Vec<NodeRelease>,
    /// Sin red ni copia en disco.
    pub offline: bool,
}

/// Una versión instalada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeInstalled {
    pub version: String,
    pub major: u32,
    pub lts: Option<String>,
    pub path: String,
    /// La versión de npm que trae.
    pub npm: Option<String>,
    /// Paquetes globales de npm con ejecutable (`pnpm`), sin `npm`, `npx` ni `corepack`.
    pub globals: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum NodeReason {
    /// `.nvmrc` o `.node-version`.
    ProjectFile,
    Global,
    /// No hay nada elegido: la más reciente instalada.
    Newest,
}

/// La versión que se usa en una carpeta, con el motivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeChoice {
    pub dir: String,
    /// Lo que se pide (`20`, `lts/*`), si hay algo elegido.
    pub requested: Option<String>,
    /// El `.nvmrc` o `.node-version` que lo pide.
    pub file: Option<String>,
    pub reason: NodeReason,
    /// La que resuelve; `None` si lo pedido no está instalado (ver `error`).
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeStatus {
    /// De la más reciente a la más antigua.
    pub versions: Vec<NodeInstalled>,
    /// El ajuste global tal cual (`24`, `lts/*`); `None` = la más reciente.
    pub global: Option<String>,
    /// La que se usa fuera de cualquier proyecto.
    pub active: Option<String>,
    /// El ajuste global pide algo que no está instalado.
    pub global_error: Option<String>,
    /// `~/.nvm`, si existe: Elephant no lo usa.
    pub nvm: Option<String>,
    /// Los proyectos que conoce la app y su versión.
    pub projects: Vec<NodeChoice>,
    pub path: Option<selectors::PathAdvice>,
}

/// Resultado de sustituir una versión por otra más reciente de su misma mayor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeReplaceReport {
    pub old: String,
    pub new: String,
    pub global_changed: bool,
    /// Archivos `.nvmrc` o `.node-version` que fijaban la antigua.
    pub projects: Vec<String>,
}

// ── Catálogo ─────────────────────────────────────────────────────────────────

/// Etiqueta de `files` en `index.json` y sufijo del paquete para esta plataforma.
fn platform() -> Option<(&'static str, &'static str)> {
    match crate::services::catalog::current_platform() {
        "macos-arm64" => Some(("osx-arm64-tar", "darwin-arm64")),
        "macos-x86_64" => Some(("osx-x64-tar", "darwin-x64")),
        "linux-arm64" => Some(("linux-arm64", "linux-arm64")),
        "linux-x86_64" => Some(("linux-x64", "linux-x64")),
        _ => None,
    }
}

#[derive(Deserialize)]
struct RawRelease {
    version: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    lts: serde_json::Value,
    #[serde(default)]
    npm: Option<String>,
    #[serde(default)]
    security: bool,
}

fn is_date(text: &str) -> bool {
    text.len() == 10 && text.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

/// Las versiones estables (`vX.Y.Z`, sin `-alpha`, `-rc`…) que tienen paquete
/// para `files_tag`, de la más reciente a la más antigua.
pub(crate) fn parse_index(json: &str, files_tag: &str) -> Result<Vec<NodeRelease>, String> {
    let raw: Vec<RawRelease> = serde_json::from_str(json)
        .map_err(|error| tr!("Respuesta inesperada de nodejs.org: {error}", "Unexpected response from nodejs.org: {error}"))?;
    let mut releases: Vec<NodeRelease> = raw
        .into_iter()
        .filter(|r| r.files.iter().any(|f| f == files_tag))
        .filter_map(|r| {
            let version = r.version.strip_prefix('v')?.to_string();
            if !select::is_version_id(&version) || !is_date(&r.date) {
                return None;
            }
            let lts = r.lts.as_str().map(str::to_ascii_lowercase).filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_lowercase()));
            Some(NodeRelease {
                major: version.split('.').next()?.parse().ok()?,
                version,
                date: r.date,
                lts,
                npm: r.npm.filter(|v| v.len() <= 32),
                security: r.security,
                featured: false,
            })
        })
        .collect();
    releases.sort_by_key(|r| std::cmp::Reverse(crate::setup::presets::version_key(&r.version)));
    Ok(releases)
}

/// Fin de soporte aproximado de una LTS: abril del tercer año tras su
/// primera versión (Node 20: 2023-04-18 → 2026-04-30). Un mes de margen.
fn lts_supported(first_date: &str, today: jiff::civil::Date) -> bool {
    let Ok(first) = first_date.parse::<jiff::civil::Date>() else {
        return false;
    };
    first.checked_add(jiff::Span::new().years(3).days(30)).is_ok_and(|end| today <= end)
}

/// La más reciente de cada mayor. Destaca las LTS con soporte y la Current
/// (la mayor más reciente, si aún no es LTS).
pub(crate) fn newest_per_major(releases: &[NodeRelease], today: jiff::civil::Date) -> Vec<NodeRelease> {
    let mut out: Vec<NodeRelease> = Vec::new();
    for release in releases {
        if !out.iter().any(|r| r.major == release.major) {
            out.push(release.clone());
        }
    }
    let newest_lts = out.iter().filter(|r| r.lts.is_some()).map(|r| r.major).max().unwrap_or(0);
    for release in &mut out {
        let first = releases.iter().filter(|r| r.major == release.major).map(|r| r.date.as_str()).min().unwrap_or("");
        release.featured = match &release.lts {
            Some(_) => lts_supported(first, today),
            None => release.major > newest_lts && release.major == releases.first().map_or(0, |r| r.major),
        };
    }
    out
}

fn cache_path(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("cache/node-index.json")
}

/// Todas las versiones de esta plataforma. Una hora en memoria y copia en
/// disco para trabajar sin red; `None` sin red ni copia.
fn all_releases(force_refresh: bool) -> Option<Vec<NodeRelease>> {
    let (tag, _) = platform()?;
    if !force_refresh && let Some(list) = MEMORY.get(tag) {
        return Some(list);
    }
    let home = paths::home().ok();
    let fetched = download::fetch_text(&download::curl_path(), INDEX_URL).ok().and_then(|json| parse_index(&json, tag).ok());
    match fetched {
        Some(list) if !list.is_empty() => {
            if let Some(home) = &home {
                let _ = fsutil::write_json_atomic(&cache_path(home), &list);
            }
            MEMORY.put(tag.into(), list.clone());
            Some(list)
        }
        _ => {
            let text = fs::read_to_string(cache_path(home.as_deref()?)).ok()?;
            serde_json::from_str(&text).ok()
        }
    }
}

fn today() -> jiff::civil::Date {
    jiff::Zoned::now().date()
}

pub fn catalog(force_refresh: bool) -> NodeCatalog {
    match all_releases(force_refresh) {
        Some(list) => NodeCatalog { releases: newest_per_major(&list, today()), offline: false },
        None => NodeCatalog { releases: Vec::new(), offline: true },
    }
}

/// La versión del catálogo que pide `requested` (`24`, `lts`, `24.11.1`, `node`).
pub(crate) fn pick<'a>(releases: &'a [NodeRelease], requested: &str) -> Result<&'a NodeRelease, String> {
    let spec = select::parse_spec(requested)?;
    releases.iter().find(|r| spec.matches_id(&r.version, r.lts.as_deref())).ok_or_else(|| {
        let requested = requested.trim();
        tr!(
            "Node {requested} no está en nodejs.org para este Mac.",
            "Node {requested} isn’t on nodejs.org for this Mac."
        )
    })
}

// ── Instalar ─────────────────────────────────────────────────────────────────

/// Lo que necesita la instalación. Los tests usan dobles en una carpeta temporal.
pub(crate) struct NodeEnv {
    pub tools: Tools,
    /// El binario de los selectores que viaja con la app.
    pub shim_source: Option<PathBuf>,
    /// Sufijo del paquete: `darwin-arm64`.
    pub suffix: &'static str,
}

impl NodeEnv {
    fn system() -> Result<Self, String> {
        let (_, suffix) = platform().ok_or_else(|| tr!("Node de Elephant no está disponible en este sistema.", "Elephant’s Node isn’t available on this system."))?;
        Ok(Self { tools: Tools::system()?, shim_source: selectors::bundled_source(), suffix })
    }

    fn layout(&self) -> Layout {
        Layout::from_home(&self.tools.home)
    }
}

/// `v24.11.1` de `bin/node --version`.
fn node_version_of(dir: &Path) -> Option<String> {
    let output = std::process::Command::new(dir.join("bin/node"))
        .arg("--version")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Descarga, verifica y deja `node/<versión>/`, y regenera los selectores.
/// Si ya está instalada, solo regenera los selectores.
pub(crate) fn install_in(
    env: &NodeEnv,
    release: &NodeRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let version = release.version.as_str();
    if !select::is_version_id(version) {
        return Err(tr!("Versión de Node no válida: {version}", "Invalid Node version: {version}"));
    }
    selectors::check_shim(&env.tools, env.shim_source.as_deref())?;
    let layout = env.layout();
    let dest = layout.node_root().join(version);
    if dest.join("bin/node").is_file() {
        on_line(&tr!("Node {version} ya está instalado.", "Node {version} is already installed."));
    } else {
        let asset = format!("node-v{version}-{}.tar.gz", env.suffix);
        let url = format!("{DIST}/v{version}/{asset}");
        on_line(&tr!("Consultando la firma de {asset}", "Checking the signature of {asset}"));
        let sums = download::fetch_text(&env.tools.curl, &format!("{DIST}/v{version}/SHASUMS256.txt"))?;
        let expected = download::expected_hash(&sums, &asset)
            .ok_or_else(|| tr!("No se encuentra la firma de {asset}.", "Couldn’t find the signature of {asset}."))?;
        let size = download::content_length(&env.tools.curl, &url).unwrap_or(0);
        let part = env.tools.data().join("cache").join(format!("{asset}.part"));
        download::dir_for(&part)?;
        download::dir_for(&dest)?;
        let _ = fs::remove_file(&part);
        on_line(&tr!("Descargando {url}", "Downloading {url}"));
        download::download_file(&env.tools.curl, &url, &part, size, on_bytes)?;
        let package = Package { tar: &env.tools.tar, archive: &part, sha256: &expected, rooted: true, name: "Node", version };
        let lts = release.lts.clone().unwrap_or_default();
        let result = unpack(&package, &dest, on_line, &mut |tmp| {
            match node_version_of(tmp) {
                Some(found) if found == format!("v{version}") => {}
                Some(found) => {
                    return Err(tr!(
                        "El paquete dice ser Node {version} pero contiene Node {found}.",
                        "The package says it’s Node {version} but contains Node {found}."
                    ));
                }
                None => {
                    return Err(tr!(
                        "El Node {version} descargado no arranca en este Mac.",
                        "The downloaded Node {version} doesn’t run on this Mac."
                    ));
                }
            }
            fs::write(tmp.join(select::LTS_MARKER), &lts).map_err(|error| error.to_string())
        });
        let _ = fs::remove_file(&part);
        result?;
    }
    selectors::ensure_shim(&env.tools, env.shim_source.as_deref())?;
    layout.reshim()?;
    Ok(())
}

/// Instala la versión del catálogo que pide `requested` (`24`, `lts`,
/// `24.11.1`). Se pueden tener varias. Devuelve la versión instalada.
pub fn install(requested: &str, on_line: &mut dyn FnMut(&str), on_bytes: &mut dyn FnMut(u64, u64)) -> Result<String, String> {
    let env = NodeEnv::system()?;
    let releases = all_releases(false).ok_or_else(|| {
        tr!(
            "No se ha podido consultar las versiones de Node (nodejs.org). Comprueba la conexión.",
            "Couldn’t check the Node versions (nodejs.org). Check the connection."
        )
    })?;
    let release = pick(&releases, requested)?.clone();
    let _lock = lock(&env)?;
    install_in(&env, &release, on_line, on_bytes)?;
    stamp::touch("node");
    Ok(release.version)
}

fn lock(env: &NodeEnv) -> Result<fs::File, String> {
    let path = env.tools.data().join("locks/node.lock");
    download::dir_for(&path)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|error| tr!("No se pudo abrir el cerrojo: {error}", "Couldn’t open the lock: {error}"))?;
    file.lock().map_err(|error| tr!("No se pudo tomar el cerrojo: {error}", "Couldn’t take the lock: {error}"))?;
    Ok(file)
}

// ── Desinstalar y actualizar ─────────────────────────────────────────────────

/// Borra solo una carpeta de versión dentro de `node/`.
fn remove_version(layout: &Layout, version: &str) -> Result<(), String> {
    let root = layout.node_root();
    let target = root.join(version);
    match (root.canonicalize(), target.canonicalize()) {
        (Ok(base), Ok(resolved)) if select::is_version_id(version) && resolved.parent() == Some(base.as_path()) => {
            fs::remove_dir_all(&resolved)
                .map_err(|error| tr!("No se pudo borrar Node {version}: {error}", "Couldn’t delete Node {version}: {error}"))
        }
        _ => Err(tr!("Node {version} no está instalado.", "Node {version} isn’t installed.")),
    }
}

/// `true` si `text` (un `.nvmrc`, el ajuste global) fija exactamente `version`.
fn pins(text: &str, version: &str) -> bool {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    select::parse_spec(first).is_ok_and(|spec| spec.is_exact() && spec.matches_id(version, None))
}

/// La misma forma que tenía: con `v` delante si la llevaba.
fn repin(text: &str, version: &str) -> String {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if first.starts_with('v') || first.starts_with('V') { format!("v{version}\n") } else { format!("{version}\n") }
}

pub(crate) fn uninstall_in(layout: &Layout, version: &str) -> Result<bool, String> {
    remove_version(layout, version)?;
    let global_cleared = layout.node_global().is_some_and(|g| pins(&g, version));
    if global_cleared {
        layout.clear_node_global()?;
    }
    layout.reshim()?;
    Ok(global_cleared)
}

/// Borra una versión. Si el ajuste global la fijaba exactamente, vuelve a
/// «la más reciente».
pub fn uninstall(version: &str) -> Result<(), String> {
    let env = NodeEnv::system()?;
    let _lock = lock(&env)?;
    uninstall_in(&env.layout(), version)?;
    stamp::touch("node");
    Ok(())
}

/// Sustituye `old` por `new` (más reciente, de la misma mayor y ya
/// instalada): el ajuste global y los `.nvmrc`/`.node-version` de
/// `project_roots` que fijaban `old` pasan a `new`, y se borra `old`. Lo que
/// pide la mayor (`24`) no cambia: ya resuelve a la más reciente.
pub fn replace_in(layout: &Layout, old: &str, new: &str, project_roots: &[PathBuf]) -> Result<NodeReplaceReport, String> {
    let key = crate::setup::presets::version_key;
    let installed: Vec<String> = layout.node_versions().into_iter().map(|v| v.id).collect();
    let major = |v: &str| v.split('.').next().unwrap_or("").to_string();
    if !select::is_version_id(old) || !select::is_version_id(new) || major(old) != major(new) || key(new) <= key(old) {
        return Err(tr!(
            "Node {new} no es una versión más reciente de la mayor de Node {old}.",
            "Node {new} isn’t a newer version in the Node {old} major line."
        ));
    }
    for version in [old, new] {
        if !installed.iter().any(|id| id == version) {
            return Err(tr!("Node {version} no está instalado.", "Node {version} isn’t installed."));
        }
    }
    let global_changed = layout.node_global().is_some_and(|g| pins(&g, old));
    if global_changed {
        let text = layout.node_global().unwrap_or_default();
        layout.set_node_global(repin(&text, new).trim())?;
    }
    let mut projects = Vec::new();
    for root in project_roots {
        for name in [select::NVMRC, select::NODE_VERSION_FILE] {
            let file = root.join(name);
            if let Ok(text) = fs::read_to_string(&file)
                && pins(&text, old)
            {
                fs::write(&file, repin(&text, new)).map_err(|error| error.to_string())?;
                projects.push(file.display().to_string());
            }
        }
    }
    remove_version(layout, old)?;
    layout.reshim()?;
    Ok(NodeReplaceReport { old: old.into(), new: new.into(), global_changed, projects })
}

/// Instala la más reciente de la mayor de `old` y la sustituye (ver [`replace_in`]).
pub fn update(old: &str, project_roots: &[PathBuf], on_line: &mut dyn FnMut(&str), on_bytes: &mut dyn FnMut(u64, u64)) -> Result<NodeReplaceReport, String> {
    let major = old.split('.').next().unwrap_or("");
    let new = install(major, on_line, on_bytes)?;
    if new == old {
        return Err(tr!("Node {old} ya es la más reciente de su versión mayor.", "Node {old} is already the newest in its major line."));
    }
    let env = NodeEnv::system()?;
    let _lock = lock(&env)?;
    let report = replace_in(&env.layout(), old, &new, project_roots)?;
    stamp::touch("node");
    Ok(report)
}

// ── Elegir ───────────────────────────────────────────────────────────────────

/// `None` vuelve a «la más reciente».
pub fn set_global(requested: Option<&str>) -> Result<NodeStatus, String> {
    let layout = Layout::from_home(&paths::home()?);
    match requested {
        Some(requested) => layout.set_node_global(requested).map(drop)?,
        None => layout.clear_node_global()?,
    }
    stamp::touch("node");
    Ok(status_of(&layout, &[]))
}

/// Escribe el `.nvmrc` (o `.node-version`) del proyecto. Devuelve el archivo.
pub fn set_project(dir: &Path, requested: &str) -> Result<PathBuf, String> {
    let file = Layout::from_home(&paths::home()?).set_node_project(dir, requested)?;
    stamp::touch("node");
    Ok(file)
}

/// Rehace los selectores de `bin/` (tras `npm i -g` fuera del selector, por ejemplo).
pub fn reshim() -> Result<elephant_php::selectors::Reshim, String> {
    let env = NodeEnv::system()?;
    selectors::ensure_shim(&env.tools, env.shim_source.as_deref())?;
    let report = env.layout().reshim()?;
    stamp::touch("node");
    Ok(report)
}

// ── Estado ───────────────────────────────────────────────────────────────────

pub fn choice_for(layout: &Layout, dir: &Path) -> NodeChoice {
    let file = select::find_project_file(dir);
    let (requested, reason) = match &file {
        Some((_, requested)) => (Some(requested.clone()), NodeReason::ProjectFile),
        None => match layout.node_global() {
            Some(global) => (Some(global), NodeReason::Global),
            None => (None, NodeReason::Newest),
        },
    };
    let resolved = layout.resolve_node(dir);
    NodeChoice {
        dir: dir.display().to_string(),
        requested,
        file: file.map(|(path, _)| path.display().to_string()),
        reason,
        version: resolved.as_ref().ok().map(|r| r.version.id.clone()),
        error: resolved.err(),
    }
}

fn npm_version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("lib/node_modules/npm/package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["version"].as_str().filter(|v| v.len() <= 32).map(String::from)
}

const BUNDLED: [&str; 4] = ["node", "npm", "npx", "corepack"];

pub(crate) fn status_of(layout: &Layout, project_roots: &[PathBuf]) -> NodeStatus {
    let versions = layout
        .node_versions()
        .into_iter()
        .map(|v| {
            let mut globals: Vec<String> = fs::read_dir(v.bin())
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| !BUNDLED.contains(&name.as_str()) && !name.starts_with('.'))
                .collect();
            globals.sort();
            NodeInstalled { major: v.major(), npm: npm_version(&v.dir), path: v.dir.display().to_string(), version: v.id, lts: v.lts, globals }
        })
        .collect();
    let at_root = layout.resolve_node(Path::new("/"));
    let home = layout.data_dir.parent().map(Path::to_path_buf).unwrap_or_default();
    let nvm = home.join(".nvm");
    NodeStatus {
        versions,
        global: layout.node_global(),
        active: at_root.as_ref().ok().map(|r| r.version.id.clone()),
        global_error: match (&at_root, layout.node_global()) {
            (Err(error), Some(_)) => Some(error.clone()),
            _ => None,
        },
        nvm: nvm.is_dir().then(|| nvm.display().to_string()),
        projects: project_roots.iter().map(|root| choice_for(layout, root)).collect(),
        path: Some(selectors::path_advice_in(&home)),
    }
}

/// Lo instalado, la versión global y la de cada proyecto de `project_roots`.
pub fn status(project_roots: &[PathBuf]) -> NodeStatus {
    match paths::home() {
        Ok(home) => status_of(&Layout::from_home(&home), project_roots),
        Err(_) => NodeStatus::default(),
    }
}

/// `NodeSpec` también para quien valida argumentos (la CLI).
pub fn check_spec(requested: &str) -> Result<(), String> {
    select::parse_spec(requested).map(|_: NodeSpec| ())
}

#[cfg(test)]
mod tests;
