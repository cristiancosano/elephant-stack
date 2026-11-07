//! Herramientas (06, fase 2): phpMyAdmin y pgAdmin. No son servicios de
//! datos: no tienen ramas ni datos propios, y solo hay una versión instalada.
//!
//! - **phpMyAdmin:** el zip oficial en `~/.elephant-stack/tools/phpmyadmin/<v>/`
//!   con un `config.inc.php` generado (root sin contraseña en 127.0.0.1:3306,
//!   sin servidores arbitrarios). Lo sirve `php -S 127.0.0.1:8081` con el PHP
//!   de Elephant más reciente que admita, como agente de launchd
//!   (`sh.elephant.phpmyadmin`) que se enciende al abrirlo.
//! - **pgAdmin:** el DMG oficial. Se monta en una carpeta temporal, se copia
//!   la app a `~/Applications` (sin sudo), se comprueba su firma (y el Team
//!   ID) y se desmonta. Solo se desinstala si la instaló Elephant.
//!
//! Lo instalado se apunta en `tools/<herramienta>/installed.json`.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::catalog::ToolRelease;
use super::{Env, LABEL_PREFIX, install as package, job, last_useful_line, lock_named, port_open, ports, read_from};
use crate::launchd::{self, AgentPlist};
use crate::setup::{download, presets};
use crate::{fsutil, stamp, tr, tr_str};

/// Puerto de phpMyAdmin: uno fijo, para que los marcadores del navegador sigan valiendo.
const PHPMYADMIN_PORT: u16 = 8081;
const MYSQL_PORT: u16 = 3306;
/// Team ID de la firma de pgAdmin («Developer ID Application: David Page»),
/// el de todas sus versiones para macOS.
pub(crate) const PGADMIN_TEAM_ID: &str = "TCHGL2R7C5";
const PGADMIN_APP: &str = "pgAdmin 4.app";

/// Una herramienta que Elephant sabe instalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ToolKind {
    Phpmyadmin,
    Pgadmin,
}

impl ToolKind {
    pub const ALL: [Self; 2] = [Self::Phpmyadmin, Self::Pgadmin];

    pub fn id(self) -> &'static str {
        match self {
            Self::Phpmyadmin => "phpmyadmin",
            Self::Pgadmin => "pgadmin",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Phpmyadmin => "phpMyAdmin",
            Self::Pgadmin => "pgAdmin",
        }
    }

    pub fn parse(id: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|tool| tool.id() == id)
            .ok_or_else(|| tr!("Herramienta desconocida: {id}", "Unknown tool: {id}"))
    }

    /// Objetivo de `busy` y de los problemas en la app: `sh.elephant.phpmyadmin`.
    pub fn target(self) -> String {
        format!("{LABEL_PREFIX}{}", self.id())
    }
}

/// Estado de una herramienta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolStatus {
    pub tool: ToolKind,
    pub name: String,
    /// La versión que ha instalado Elephant.
    pub version: Option<String>,
    /// Su carpeta (phpMyAdmin) o su app (pgAdmin).
    pub path: Option<String>,
    /// Una copia que no ha instalado Elephant (pgAdmin en `/Applications`):
    /// se puede abrir, pero no actualizar ni desinstalar.
    pub external: Option<String>,
    /// Objetivo de `busy` y de los problemas en la app.
    pub target: String,
    /// phpMyAdmin: su servidor está en marcha.
    pub running: bool,
    /// phpMyAdmin: dónde se abre (solo en 127.0.0.1).
    pub url: Option<String>,
    /// phpMyAdmin: el PHP de Elephant con el que se sirve.
    pub php: Option<String>,
    /// phpMyAdmin: las versiones de PHP que admite (`>=7.2,<8.4`).
    pub php_range: Option<String>,
    /// Lo que falta o conviene saber (no hay PHP compatible, MySQL está
    /// apagado), ya traducido.
    pub warning: Option<String>,
    /// No se puede abrir hasta resolver `warning`.
    pub blocked: bool,
}

/// Lo que se apunta al instalar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Installed {
    version: String,
    path: String,
    #[serde(default)]
    php: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
}

fn tool_dir(env: &Env, tool: ToolKind) -> PathBuf {
    env.data().join("tools").join(tool.id())
}

fn marker_path(env: &Env, tool: ToolKind) -> PathBuf {
    tool_dir(env, tool).join("installed.json")
}

fn read_marker(env: &Env, tool: ToolKind) -> Option<Installed> {
    let text = fs::read_to_string(marker_path(env, tool)).ok()?;
    serde_json::from_str(&text).ok()
}

// ── Rangos de PHP ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

fn numbers(text: &str) -> Option<Vec<u32>> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    parts.iter().map(|p| if p.len() <= 4 { p.parse().ok() } else { None }).collect()
}

/// `>=7.2,<8.4`: comparaciones separadas por comas (todas a la vez).
pub(crate) fn parse_constraint(text: &str) -> Option<Vec<(Cmp, Vec<u32>)>> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        let (cmp, rest) = [(">=", Cmp::Ge), ("<=", Cmp::Le), ("==", Cmp::Eq), (">", Cmp::Gt), ("<", Cmp::Lt), ("=", Cmp::Eq)]
            .into_iter()
            .find_map(|(prefix, cmp)| part.strip_prefix(prefix).map(|rest| (cmp, rest)))
            .unwrap_or((Cmp::Eq, part));
        out.push((cmp, numbers(rest.trim())?));
    }
    (!out.is_empty()).then_some(out)
}

/// Compara rellenando con ceros: `8.4` es `8.4.0`.
fn compare(a: &[u32], b: &[u32]) -> Ordering {
    let len = a.len().max(b.len());
    (0..len)
        .map(|i| a.get(i).copied().unwrap_or(0).cmp(&b.get(i).copied().unwrap_or(0)))
        .find(|o| *o != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

pub(crate) fn satisfies(version: &str, range: &str) -> bool {
    let (Some(version), Some(range)) = (numbers(version), parse_constraint(range)) else {
        return false;
    };
    range.iter().all(|(cmp, bound)| {
        let order = compare(&version, bound);
        match cmp {
            Cmp::Lt => order == Ordering::Less,
            Cmp::Le => order != Ordering::Greater,
            Cmp::Gt => order == Ordering::Greater,
            Cmp::Ge => order != Ordering::Less,
            Cmp::Eq => order == Ordering::Equal,
        }
    })
}

/// La rama de PHP que conviene instalar: la anterior a un `<8.4`.
pub(crate) fn suggested_php(range: &str) -> Option<String> {
    parse_constraint(range)?.into_iter().find_map(|(cmp, bound)| match (cmp, bound.as_slice()) {
        (Cmp::Lt, [major, minor, ..]) if *minor > 0 => Some(format!("{major}.{}", minor - 1)),
        (Cmp::Le, [major, minor, ..]) => Some(format!("{major}.{minor}")),
        _ => None,
    })
}

/// Las versiones de PHP de Elephant, de la más reciente a la más antigua.
fn own_phps(env: &Env) -> Vec<(String, PathBuf)> {
    let mut list: Vec<(String, PathBuf)> = elephant_php::Layout::from_home(&env.tools.home)
        .discover()
        .into_iter()
        .filter(|v| v.source == crate::php::OWN_SOURCE)
        .map(|v| (v.id, v.php))
        .collect();
    list.sort_by_key(|(id, _)| std::cmp::Reverse(presets::version_key(id)));
    list
}

/// La más reciente que admite `range` (cualquiera si no hay rango).
fn php_for(env: &Env, range: Option<&str>) -> Option<(String, PathBuf)> {
    own_phps(env).into_iter().find(|(id, _)| range.is_none_or(|r| satisfies(id, r)))
}

fn no_php_message(version: &str, range: &str) -> String {
    match suggested_php(range) {
        Some(branch) => tr!(
            "phpMyAdmin {version} necesita PHP {range} y no tienes ninguna versión de PHP de Elephant que valga. Instala PHP {branch} en la vista PHP.",
            "phpMyAdmin {version} needs PHP {range} and none of your Elephant PHP versions fits. Install PHP {branch} in the PHP view."
        ),
        None => tr!(
            "phpMyAdmin {version} necesita PHP {range} y no tienes ninguna versión de PHP de Elephant que valga. Instala una en la vista PHP.",
            "phpMyAdmin {version} needs PHP {range} and none of your Elephant PHP versions fits. Install one in the PHP view."
        ),
    }
}

// ── Estado ───────────────────────────────────────────────────────────────────

fn pma_port(env: &Env) -> u16 {
    env.tool_port.unwrap_or(PHPMYADMIN_PORT)
}

fn mysql_port(env: &Env) -> u16 {
    env.mysql_port.unwrap_or(MYSQL_PORT)
}

fn pma_url(env: &Env) -> String {
    let port = pma_port(env);
    format!("http://127.0.0.1:{port}/")
}

fn pma_label(env: &Env) -> String {
    format!("{}{}", ToolKind::Phpmyadmin.target(), env.label_suffix)
}

fn pma_log(env: &Env) -> PathBuf {
    env.data().join("logs").join("phpmyadmin.log")
}

fn empty_status(tool: ToolKind) -> ToolStatus {
    ToolStatus {
        tool,
        name: tool.name().to_string(),
        version: None,
        path: None,
        external: None,
        target: tool.target(),
        running: false,
        url: None,
        php: None,
        php_range: None,
        warning: None,
        blocked: false,
    }
}

fn phpmyadmin_status(env: &Env, running: bool) -> ToolStatus {
    let mut status = empty_status(ToolKind::Phpmyadmin);
    status.url = Some(pma_url(env));
    let Some(installed) = read_marker(env, ToolKind::Phpmyadmin).filter(|i| Path::new(&i.path).join("index.php").is_file())
    else {
        return status;
    };
    status.running = running;
    status.php_range = installed.php.clone();
    match php_for(env, installed.php.as_deref()) {
        Some((php, _)) => status.php = Some(php),
        None => {
            status.warning = Some(no_php_message(&installed.version, installed.php.as_deref().unwrap_or("?")));
            status.blocked = true;
        }
    }
    if status.warning.is_none() && !port_open(mysql_port(env)) {
        let port = mysql_port(env);
        status.warning = Some(tr!(
            "MySQL no está encendido (puerto {port}): phpMyAdmin se abrirá, pero no podrá conectar hasta que lo enciendas.",
            "MySQL isn’t on (port {port}). phpMyAdmin will open, but it can’t connect until you turn it on."
        ));
    }
    status.version = Some(installed.version);
    status.path = Some(installed.path);
    status
}

/// La versión de una app (`CFBundleShortVersionString`).
fn app_version(app: &Path) -> Option<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}

fn pgadmin_status(env: &Env) -> ToolStatus {
    let mut status = empty_status(ToolKind::Pgadmin);
    let ours = env.apps_dir.join(PGADMIN_APP);
    match read_marker(env, ToolKind::Pgadmin) {
        Some(installed) if Path::new(&installed.path).is_dir() => {
            status.version = Some(installed.version);
            status.path = Some(installed.path);
        }
        _ => {
            status.external = [ours, PathBuf::from("/Applications").join(PGADMIN_APP)]
                .into_iter()
                .find(|app| app.is_dir())
                .map(|app| app.display().to_string());
        }
    }
    status
}

fn status_in(env: &Env) -> Vec<ToolStatus> {
    let running = job(&pma_label(env)).is_some_and(|job| job.pid.is_some());
    vec![phpmyadmin_status(env, running), pgadmin_status(env)]
}

/// Lo instalado y su estado. Solo lee: `launchctl list`, carpetas y, con
/// phpMyAdmin instalado, si el puerto de MySQL responde.
pub fn status() -> Vec<ToolStatus> {
    match Env::system() {
        Ok(env) => status_in(&env),
        Err(_) => ToolKind::ALL.into_iter().map(empty_status).collect(),
    }
}

// ── Instalar ─────────────────────────────────────────────────────────────────

/// Descarga a `cache/` y comprueba el sha256. Devuelve el archivo.
fn download_verified(
    env: &Env,
    release: &ToolRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let asset = download::asset_name(&release.url)?;
    let part = env.data().join("cache").join(format!("{asset}.part"));
    download::dir_for(&part)?;
    let _ = fs::remove_file(&part);
    let url = &release.url;
    on_line(&tr!("Descargando {url}", "Downloading {url}"));
    download::download_file(&env.tools.curl, url, &part, release.size, on_bytes)?;
    if download::sha256_file(&part)? != release.sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(&part);
        return Err(download::mismatch());
    }
    on_line(tr_str!("Firma correcta.", "Signature OK."));
    Ok(part)
}

/// Instala (o actualiza a) la versión más reciente del catálogo. Devuelve la versión.
pub fn install(tool: ToolKind, on_line: &mut dyn FnMut(&str), on_bytes: &mut dyn FnMut(u64, u64)) -> Result<String, String> {
    let env = Env::system()?;
    let name = tool.name();
    let release = super::catalog(false).latest_tool(tool).cloned().ok_or_else(|| {
        tr!(
            "{name} no está en el catálogo de Elephant para este Mac.",
            "{name} isn’t in Elephant’s catalog for this Mac."
        )
    })?;
    let _lock = lock_named(&env, tool.id())?;
    install_in(&env, &release, on_line, on_bytes)?;
    stamp::touch("services");
    Ok(release.version)
}

pub(crate) fn install_in(
    env: &Env,
    release: &ToolRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    match release.tool {
        ToolKind::Phpmyadmin => install_phpmyadmin(env, release, on_line, on_bytes),
        ToolKind::Pgadmin => install_pgadmin(env, release, on_line, on_bytes, PGADMIN_TEAM_ID),
    }
}

/// Entre comillas simples de PHP.
fn php_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// 32 caracteres al azar: phpMyAdmin pide un `blowfish_secret` de 32 bytes.
fn random_secret() -> Result<String, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = [0u8; 32];
    use std::io::Read;
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|error| tr!("No se pudo generar una clave: {error}", "Couldn’t generate a key: {error}"))?;
    Ok(bytes.iter().map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char).collect())
}

/// `config.inc.php`: entra solo como root sin contraseña en el MySQL de
/// 127.0.0.1, sin elegir otro servidor, sin avisos de versión ni informes.
pub(crate) fn phpmyadmin_config(env: &Env, temp_dir: &Path, secret: &str) -> String {
    let port = mysql_port(env);
    let temp = php_string(&temp_dir.display().to_string());
    let secret = php_string(secret);
    format!(
        "<?php\n\
// Generado por Elephant Stack al instalar phpMyAdmin.\n\
declare(strict_types=1);\n\
\n\
$cfg['blowfish_secret'] = {secret};\n\
$i = 1;\n\
$cfg['Servers'][$i]['auth_type'] = 'config';\n\
$cfg['Servers'][$i]['host'] = '127.0.0.1';\n\
$cfg['Servers'][$i]['port'] = '{port}';\n\
$cfg['Servers'][$i]['user'] = 'root';\n\
$cfg['Servers'][$i]['password'] = '';\n\
$cfg['Servers'][$i]['AllowNoPassword'] = true;\n\
$cfg['Servers'][$i]['compress'] = false;\n\
$cfg['AllowArbitraryServer'] = false;\n\
$cfg['TempDir'] = {temp};\n\
$cfg['UploadDir'] = '';\n\
$cfg['SaveDir'] = '';\n\
$cfg['VersionCheck'] = false;\n\
$cfg['SendErrorReports'] = 'never';\n"
    )
}

/// Dentro de `root` y justo debajo: lo único que se borra.
fn remove_inside(root: &Path, target: &Path) {
    if let (Ok(base), Ok(resolved)) = (root.canonicalize(), target.canonicalize())
        && resolved.parent() == Some(base.as_path())
    {
        let _ = fs::remove_dir_all(resolved);
    }
}

fn install_phpmyadmin(
    env: &Env,
    release: &ToolRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let version = release.version.as_str();
    let root = tool_dir(env, ToolKind::Phpmyadmin);
    let previous = read_marker(env, ToolKind::Phpmyadmin);
    if previous.as_ref().is_some_and(|p| p.version == version && Path::new(&p.path).join("index.php").is_file()) {
        on_line(&tr!("phpMyAdmin {version} ya está instalado.", "phpMyAdmin {version} is already installed."));
        return Ok(());
    }
    let part = download_verified(env, release, on_line, on_bytes)?;
    let tmp = root.join(format!(".tmp-{version}"));
    let dest = root.join(version);
    let result = (|| {
        on_line(tr_str!("Comprobando el contenido del paquete", "Checking the package contents"));
        package::check_entries(&package::list_archive(&env.tools.tar, &part)?, true)?;
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|error| {
            let dir = tmp.display();
            tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
        })?;
        on_line(tr_str!("Descomprimiendo", "Extracting"));
        let status = Command::new(&env.tools.tar)
            .arg("-xzf")
            .arg(&part)
            .args(["--strip-components", "1", "-C"])
            .arg(&tmp)
            .stdin(Stdio::null())
            .status()
            .map_err(|error| tr!("No se pudo ejecutar tar: {error}", "Couldn’t run tar: {error}"))?;
        if !status.success() {
            return Err(tr!("No se ha podido extraer el paquete.", "Couldn’t extract the package."));
        }
        let readme = fs::read_to_string(tmp.join("README")).unwrap_or_default();
        if !tmp.join("index.php").is_file() || !readme.lines().any(|l| l.trim() == format!("Version {version}")) {
            return Err(tr!(
                "El paquete no contiene phpMyAdmin {version}.",
                "The package doesn’t contain phpMyAdmin {version}."
            ));
        }
        let temp_dir = root.join("tmp");
        super::create_private_dir(&temp_dir)?;
        let config = tmp.join("config.inc.php");
        fsutil::write_atomic(&config, phpmyadmin_config(env, &temp_dir, &random_secret()?).as_bytes())?;
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&config, fs::Permissions::from_mode(0o600));
        if dest.exists() {
            remove_inside(&root, &dest);
        }
        fs::rename(&tmp, &dest)
            .map_err(|error| tr!("No se pudo instalar phpMyAdmin {version}: {error}", "Couldn’t install phpMyAdmin {version}: {error}"))
    })();
    let _ = fs::remove_file(&part);
    let _ = fs::remove_dir_all(&tmp);
    result?;
    let installed = Installed { version: version.into(), path: dest.display().to_string(), php: release.php.clone(), team_id: None };
    fsutil::write_json_atomic(&marker_path(env, ToolKind::Phpmyadmin), &installed)?;
    let dir = dest.display();
    on_line(&tr!("phpMyAdmin {version} instalado en {dir}", "phpMyAdmin {version} installed in {dir}"));
    // Al actualizar: el servidor pasa a la versión nueva y la anterior se borra.
    if let Some(previous) = previous.filter(|p| p.version != version) {
        if job(&pma_label(env)).is_some_and(|job| job.pid.is_some()) {
            start_phpmyadmin(env)?;
        }
        remove_inside(&root, Path::new(&previous.path));
    }
    Ok(())
}

/// `TeamIdentifier=TCHGL2R7C5` de `codesign -dv`.
pub(crate) fn parse_team_id(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|id| !id.is_empty() && *id != "not set")
        .map(String::from)
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<std::process::Output, String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo ejecutar {program}: {error}", "Couldn’t run {program}: {error}"))
}

fn last_line(output: &std::process::Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// La firma completa (`--deep --strict`), del equipo esperado, y que
/// Gatekeeper la acepta. Devuelve el Team ID.
pub(crate) fn verify_app(app: &Path, expected_team: &str) -> Result<String, String> {
    let path = app.as_os_str();
    let verify = run("/usr/bin/codesign", &["--verify".as_ref(), "--deep".as_ref(), "--strict".as_ref(), path])?;
    if !verify.status.success() {
        let detail = last_line(&verify);
        return Err(tr!("La firma de pgAdmin no es válida: {detail}", "pgAdmin’s signature isn’t valid: {detail}"));
    }
    let info = run("/usr/bin/codesign", &["-dv".as_ref(), "--verbose=2".as_ref(), path])?;
    let team = parse_team_id(&String::from_utf8_lossy(&info.stderr)).unwrap_or_else(|| "-".into());
    if team != expected_team {
        return Err(tr!(
            "pgAdmin está firmado por otro equipo ({team}); se esperaba {expected_team}. No se ha instalado.",
            "pgAdmin is signed by another team ({team}); expected {expected_team}. It wasn’t installed."
        ));
    }
    // `spctl` puede no estar disponible (o fallar por otros motivos): solo
    // cuenta un rechazo explícito.
    if let Ok(check) = run("/usr/sbin/spctl", &["-a".as_ref(), "-t".as_ref(), "exec".as_ref(), "-vv".as_ref(), path]) {
        let text = format!("{}{}", String::from_utf8_lossy(&check.stderr), String::from_utf8_lossy(&check.stdout));
        if !check.status.success() && text.contains("rejected") {
            let detail = last_line(&check);
            return Err(tr!("Gatekeeper rechaza pgAdmin: {detail}", "Gatekeeper rejects pgAdmin: {detail}"));
        }
    }
    Ok(team)
}

/// Desmonta al salir, pase lo que pase.
struct Mounted(PathBuf);

impl Drop for Mounted {
    fn drop(&mut self) {
        let detach = |force: bool| {
            let mut command = Command::new("/usr/bin/hdiutil");
            command.arg("detach").arg(&self.0).arg("-quiet");
            if force {
                command.arg("-force");
            }
            command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
        };
        if !detach(false) {
            let _ = detach(true);
        }
        let _ = fs::remove_dir(&self.0);
    }
}

fn attach(image: &Path, mountpoint: &Path) -> Result<Mounted, String> {
    let output = Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(mountpoint)
        .arg(image)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo ejecutar hdiutil: {error}", "Couldn’t run hdiutil: {error}"))?;
    if !output.status.success() {
        let detail = last_line(&output);
        let _ = fs::remove_dir(mountpoint);
        return Err(tr!("No se pudo abrir la imagen de pgAdmin: {detail}", "Couldn’t open the pgAdmin image: {detail}"));
    }
    Ok(Mounted(mountpoint.to_path_buf()))
}

pub(crate) fn install_pgadmin(
    env: &Env,
    release: &ToolRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
    expected_team: &str,
) -> Result<(), String> {
    let version = release.version.as_str();
    let target = env.apps_dir.join(PGADMIN_APP);
    let previous = read_marker(env, ToolKind::Pgadmin);
    let apps = env.apps_dir.display().to_string();
    if target.exists() && previous.is_none() {
        return Err(tr!(
            "Ya hay un pgAdmin 4 en {apps} que no ha instalado Elephant: no se sustituye. Bórralo si quieres que lo gestione Elephant.",
            "There’s already a pgAdmin 4 in {apps} that Elephant didn’t install, so it isn’t replaced. Delete it if you want Elephant to manage it."
        ));
    }
    if previous.as_ref().is_some_and(|p| p.version == version) && target.is_dir() {
        on_line(&tr!("pgAdmin {version} ya está instalado.", "pgAdmin {version} is already installed."));
        return Ok(());
    }
    let part = download_verified(env, release, on_line, on_bytes)?;
    let mountpoint = env.data().join("tmp").join(format!("pgadmin-{}", std::process::id()));
    let staging = env.apps_dir.join(format!(".{PGADMIN_APP}.elephant-tmp"));
    let result = (|| {
        let _ = fs::remove_dir(&mountpoint);
        super::create_private_dir(&mountpoint)?;
        on_line(tr_str!("Abriendo la imagen", "Opening the image"));
        let mounted = attach(&part, &mountpoint)?;
        let app = mounted.0.join(PGADMIN_APP);
        if !app.is_dir() {
            return Err(tr!("La imagen no contiene {PGADMIN_APP}.", "The image doesn’t contain {PGADMIN_APP}."));
        }
        fs::create_dir_all(&env.apps_dir).map_err(|error| tr!("No se pudo crear {apps}: {error}", "Couldn’t create {apps}: {error}"))?;
        let _ = fs::remove_dir_all(&staging);
        on_line(&tr!("Copiando pgAdmin a {apps}", "Copying pgAdmin to {apps}"));
        let copy = run("/usr/bin/ditto", &["--noqtn".as_ref(), app.as_os_str(), staging.as_os_str()])?;
        if !copy.status.success() {
            let detail = last_line(&copy);
            return Err(tr!("No se pudo copiar pgAdmin: {detail}", "Couldn’t copy pgAdmin: {detail}"));
        }
        drop(mounted);
        // El DMG trae un FinderInfo en la app (que `--strict` no admite) y
        // atributos de cuarentena de quien lo empaquetó: ninguno es de la firma.
        for attr in ["com.apple.FinderInfo", "com.apple.quarantine"] {
            let _ = run("/usr/bin/xattr", &["-dr".as_ref(), attr.as_ref(), staging.as_os_str()]);
        }
        on_line(tr_str!("Comprobando la firma", "Checking the signature"));
        let team = verify_app(&staging, expected_team)?;
        if let Some(old) = previous.as_ref().and_then(|p| p.team_id.as_deref())
            && old != team
        {
            return Err(tr!(
                "El pgAdmin nuevo está firmado por otro equipo ({team}) que el instalado ({old}). No se ha actualizado.",
                "The new pgAdmin is signed by a different team ({team}) than the installed one ({old}). It wasn’t updated."
            ));
        }
        match app_version(&staging) {
            Some(found) if found == version => {}
            found => {
                let found = found.unwrap_or_else(|| "?".into());
                return Err(tr!(
                    "La imagen dice ser pgAdmin {version} pero contiene pgAdmin {found}.",
                    "The image says it’s pgAdmin {version} but contains pgAdmin {found}."
                ));
            }
        }
        let old = env.apps_dir.join(format!(".{PGADMIN_APP}.elephant-old"));
        let _ = fs::remove_dir_all(&old);
        if target.exists() {
            fs::rename(&target, &old).map_err(|error| tr!("No se pudo apartar el pgAdmin anterior: {error}", "Couldn’t move the previous pgAdmin aside: {error}"))?;
        }
        fs::rename(&staging, &target).map_err(|error| tr!("No se pudo instalar pgAdmin: {error}", "Couldn’t install pgAdmin: {error}"))?;
        let _ = fs::remove_dir_all(&old);
        Ok(team)
    })();
    let _ = fs::remove_file(&part);
    let _ = fs::remove_dir_all(&staging);
    let team = result?;
    let installed = Installed { version: version.into(), path: target.display().to_string(), php: None, team_id: Some(team) };
    fsutil::write_json_atomic(&marker_path(env, ToolKind::Pgadmin), &installed)?;
    on_line(&tr!("pgAdmin {version} instalado en {apps}", "pgAdmin {version} installed in {apps}"));
    Ok(())
}

// ── Desinstalar ──────────────────────────────────────────────────────────────

/// Borra lo que instaló Elephant (nunca un pgAdmin que no instaló). Si el
/// servidor de phpMyAdmin está en marcha, lo apaga antes.
pub fn uninstall(tool: ToolKind) -> Result<(), String> {
    let env = Env::system()?;
    let _lock = lock_named(&env, tool.id())?;
    uninstall_in(&env, tool)?;
    stamp::touch("services");
    Ok(())
}

pub(crate) fn uninstall_in(env: &Env, tool: ToolKind) -> Result<(), String> {
    let name = tool.name();
    let installed = read_marker(env, tool)
        .ok_or_else(|| tr!("{name} no lo ha instalado Elephant: no se toca.", "Elephant didn’t install {name}, so it isn’t touched."))?;
    match tool {
        ToolKind::Phpmyadmin => {
            let label = pma_label(env);
            launchd::bootout(&label)?;
            let _ = fs::remove_file(env.agents_dir.join(format!("{label}.plist")));
            remove_inside(&tool_dir(env, tool), Path::new(&installed.path));
        }
        ToolKind::Pgadmin => {
            let target = env.apps_dir.join(PGADMIN_APP);
            if Path::new(&installed.path) != target {
                let path = installed.path;
                return Err(tr!("No se encuentra el pgAdmin que instaló Elephant ({path}).", "Couldn’t find the pgAdmin that Elephant installed ({path})."));
            }
            if target.exists() {
                fs::remove_dir_all(&target).map_err(|error| tr!("No se pudo borrar pgAdmin: {error}", "Couldn’t delete pgAdmin: {error}"))?;
            }
        }
    }
    let _ = fs::remove_file(marker_path(env, tool));
    Ok(())
}

// ── Servidor de phpMyAdmin y abrir ───────────────────────────────────────────

pub(crate) fn phpmyadmin_plist(env: &Env, php: &Path, dir: &Path) -> AgentPlist {
    let port = pma_port(env);
    let temp = tool_dir(env, ToolKind::Phpmyadmin).join("tmp").display().to_string();
    AgentPlist {
        label: pma_label(env),
        program: vec![
            php.display().to_string(),
            "-S".into(),
            format!("127.0.0.1:{port}"),
            "-t".into(),
            dir.display().to_string(),
            "-d".into(),
            format!("session.save_path={temp}"),
            "-d".into(),
            format!("upload_tmp_dir={temp}"),
            "-d".into(),
            "display_errors=0".into(),
        ],
        log: pma_log(env).display().to_string(),
        run_at_load: false,
    }
}

fn start_phpmyadmin(env: &Env) -> Result<(), String> {
    let installed = read_marker(env, ToolKind::Phpmyadmin)
        .filter(|i| Path::new(&i.path).join("index.php").is_file())
        .ok_or_else(|| tr!("phpMyAdmin no está instalado.", "phpMyAdmin isn’t installed."))?;
    let range = installed.php.as_deref();
    let (_, php) = php_for(env, range).ok_or_else(|| no_php_message(&installed.version, range.unwrap_or("?")))?;
    let plist = phpmyadmin_plist(env, &php, Path::new(&installed.path));
    let xml = plist.to_xml();
    let path = env.agents_dir.join(format!("{}.plist", plist.label));
    let running = job(&plist.label).is_some_and(|job| job.pid.is_some());
    if running && fs::read_to_string(&path).is_ok_and(|old| old == xml) {
        return Ok(());
    }
    launchd::bootout(&plist.label)?;
    let port = pma_port(env);
    super::wait_port_free(port);
    if port_open(port) {
        return Err(ports::busy_error("phpMyAdmin", port, None));
    }
    super::create_private_dir(&tool_dir(env, ToolKind::Phpmyadmin).join("tmp"))?;
    fsutil::write_atomic(&path, xml.as_bytes())?;
    let log = pma_log(env);
    let offset = fs::metadata(&log).map_or(0, |m| m.len());
    launchd::bootstrap(&path)?;
    launchd::kickstart(&plist.label)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let job = job(&plist.label);
        if job.as_ref().is_some_and(|j| j.pid.is_some()) && port_open(port) {
            return Ok(());
        }
        let exited = job.as_ref().is_none_or(|j| j.pid.is_none() && j.last_exit.is_some());
        if exited || Instant::now() >= deadline {
            let _ = launchd::bootout(&plist.label);
            let detail = last_useful_line(&read_from(&log, offset)).map(|l| format!(": {l}")).unwrap_or_default();
            return Err(tr!(
                "El servidor de phpMyAdmin no ha arrancado{detail}",
                "phpMyAdmin’s server didn’t start{detail}"
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Enciende o apaga el servidor de phpMyAdmin (pgAdmin no tiene servidor).
pub fn set_running(tool: ToolKind, on: bool) -> Result<(), String> {
    if tool != ToolKind::Phpmyadmin {
        return Err(tr!("pgAdmin no tiene servidor: se abre como app.", "pgAdmin has no server; it opens as an app."));
    }
    let env = Env::system()?;
    let _lock = lock_named(&env, tool.id())?;
    let result = if on { start_phpmyadmin(&env) } else { launchd::bootout(&pma_label(&env)) };
    stamp::touch("services");
    result
}

/// Abre la herramienta: phpMyAdmin en el navegador (encendiendo su
/// servidor si hace falta) y pgAdmin como app. Devuelve lo que se ha abierto.
pub fn open(tool: ToolKind) -> Result<String, String> {
    let env = Env::system()?;
    let target = match tool {
        ToolKind::Phpmyadmin => {
            set_running(tool, true)?;
            pma_url(&env)
        }
        ToolKind::Pgadmin => {
            let status = pgadmin_status(&env);
            status.path.or(status.external).ok_or_else(|| tr!("pgAdmin no está instalado.", "pgAdmin isn’t installed."))?
        }
    };
    let output = run("/usr/bin/open", &[target.as_ref()])?;
    if !output.status.success() {
        let detail = last_line(&output);
        return Err(tr!("No se pudo abrir {target}: {detail}", "Couldn’t open {target}: {detail}"));
    }
    Ok(target)
}

#[cfg(test)]
mod tests;
