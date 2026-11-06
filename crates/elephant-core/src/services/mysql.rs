//! Lo propio de MySQL: argumentos de `mysqld`, primer arranque
//! (`--initialize-insecure`) y los datos de Homebrew que se pueden copiar.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use super::{Env, ServiceKind, last_useful_line};
use crate::{launchd, tr};

/// Opciones de `mysqld`. `--no-defaults` va primero: así no lee el `my.cnf`
/// de Homebrew ni el de `/etc`. Sin X Protocol: su puerto (33060) y su socket
/// en `/tmp` chocarían con otras versiones y con el MySQL de brew.
pub(crate) fn daemon_args(env: &Env, install: &Path, branch: &str) -> Vec<String> {
    let kind = ServiceKind::Mysql;
    vec![
        kind.daemon(install).display().to_string(),
        "--no-defaults".into(),
        format!("--basedir={}", install.display()),
        format!("--datadir={}", env.data_dir(kind, branch).display()),
        format!("--port={}", env.port(kind)),
        "--bind-address=127.0.0.1".into(),
        format!("--socket={}", env.socket_path(kind, branch).display()),
        "--mysqlx=OFF".into(),
        format!("--pid-file={}", env.run_dir().join(format!("mysql-{branch}.pid")).display()),
        format!("--log-error={}", env.log_path(kind, branch).display()),
    ]
}

/// `mysqld --version` → `8.4.11` (de «… Ver 8.4.11 for macos15 on arm64 …»).
pub(crate) fn version_of(install: &Path) -> Option<String> {
    let output = Command::new(ServiceKind::Mysql.daemon(install))
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&output.stdout))
}

fn parse_version(text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    words.find(|w| *w == "Ver")?;
    let version = words.next()?;
    // Las de MySQL 8.0 llevaban sufijo: «8.0.33-something».
    let version = version.split(['-', '_']).next()?;
    super::catalog::is_version(version).then(|| version.to_string())
}

/// Primer arranque: `root` sin contraseña, como Herd y brew.
pub(crate) fn initialize(env: &Env, install: &Path, branch: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    let kind = ServiceKind::Mysql;
    let datadir = env.data_dir(kind, branch);
    let log = env.log_path(kind, branch);
    let offset = fs::metadata(&log).map_or(0, |m| m.len());
    let dir = datadir.display().to_string();
    on_line(&tr!("Preparando los datos en {dir}", "Preparing the data in {dir}"));
    let output = Command::new(kind.daemon(install))
        .arg("--no-defaults")
        .arg("--initialize-insecure")
        .arg(format!("--basedir={}", install.display()))
        .arg(format!("--datadir={dir}"))
        .arg(format!("--log-error={}", log.display()))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo ejecutar mysqld: {error}", "Couldn’t run mysqld: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = last_useful_line(&super::read_from(&log, offset))
        .or_else(|| last_useful_line(&String::from_utf8_lossy(&output.stderr)))
        .unwrap_or_else(|| output.status.to_string());
    Err(tr!(
        "No se han podido preparar los datos de MySQL {branch}: {detail}",
        "Couldn’t prepare the MySQL {branch} data: {detail}"
    ))
}

/// Datos de MySQL de Homebrew que se pueden copiar a una rama propia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImportOffer {
    /// `/opt/homebrew/var/mysql`.
    pub source: String,
    /// Última versión que usó esos datos.
    pub version: String,
    /// Rama a la que se pueden copiar (la misma que la de `version`).
    pub branch: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: u64,
    /// Por qué no se puede copiar ahora (un mysqld los está usando, un
    /// archivo no se puede leer…).
    pub blocked: Option<String>,
    /// La rama ya tiene datos: se guardarían aparte antes de copiar.
    pub replaces: bool,
}

pub(crate) fn brew_source(env: &Env) -> PathBuf {
    env.brew_prefix.join("var/mysql")
}

/// Versión de unos datos: la última de `mysql_upgrade_history` (8.4 en
/// adelante) o `mysql_upgrade_info` (8.0).
pub(crate) fn data_version(dir: &Path) -> Option<String> {
    if let Ok(text) = fs::read_to_string(dir.join("mysql_upgrade_history")) {
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let last = value["upgrade_history"].as_array()?.last()?;
        let version = last["version"].as_str()?;
        return super::catalog::is_version(version).then(|| version.to_string());
    }
    let text = fs::read_to_string(dir.join("mysql_upgrade_info")).ok()?;
    let version = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    let version = version.split(['-', '_']).next()?;
    super::catalog::is_version(version).then(|| version.to_string())
}

/// Algún `mysqld` usa `source`, o el servicio `mysql` de brew está en marcha.
pub(crate) fn source_in_use(source: &Path, ps: &str, brew_jobs: &[launchd::LaunchJob]) -> bool {
    let source = source.display().to_string();
    let by_process = ps.lines().any(|line| line.contains("mysqld") && line.contains(&source));
    let by_brew = brew_jobs
        .iter()
        .any(|job| job.service_name().is_some_and(|n| n == "mysql" || n.starts_with("mysql@")) && job.pid.is_some());
    by_process || by_brew
}

fn ps_commands() -> String {
    Command::new("/bin/ps")
        .args(["-axo", "command="])
        .stdin(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Lo que ocupa `dir` y el primer archivo que no se puede leer, si lo hay.
pub(crate) fn readable_size(dir: &Path) -> Result<u64, String> {
    let mut total = 0;
    let entries = fs::read_dir(dir).map_err(|_| dir.display().to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            return Err(path.display().to_string());
        };
        if meta.is_dir() {
            total += readable_size(&path)?;
        } else if meta.is_file() {
            fs::File::open(&path).map_err(|_| path.display().to_string())?;
            total += meta.len();
        }
    }
    Ok(total)
}

/// La oferta, si hay datos de brew legibles y con versión conocida y no se
/// han copiado ya a su rama.
pub(crate) fn brew_offer(env: &Env) -> Option<ImportOffer> {
    brew_offer_with(env, &|source| {
        env.brew_in_use
            .unwrap_or_else(|| source_in_use(source, &ps_commands(), &launchd::homebrew_jobs().unwrap_or_default()))
    })
}

pub(crate) fn brew_offer_with(env: &Env, in_use: &dyn Fn(&Path) -> bool) -> Option<ImportOffer> {
    let source = brew_source(env);
    if !source.is_dir() {
        return None;
    }
    let version = data_version(&source)?;
    let branch = ServiceKind::Mysql.branch_of(&version);
    if env.import_marker(ServiceKind::Mysql, &branch).exists() {
        return None;
    }
    let (size, unreadable) = match readable_size(&source) {
        Ok(size) => (size, None),
        Err(path) => (0, Some(path)),
    };
    let blocked = if let Some(path) = unreadable {
        Some(tr!("No se puede leer {path}.", "Can’t read {path}."))
    } else if in_use(&source) {
        Some(tr!(
            "El MySQL de Homebrew está usando estos datos: detenlo antes de copiarlos.",
            "Homebrew’s MySQL is using this data. Stop it before copying."
        ))
    } else {
        None
    };
    let target = env.data_dir(ServiceKind::Mysql, &branch);
    let replaces = fs::read_dir(&target).is_ok_and(|mut d| d.next().is_some());
    Some(ImportOffer {
        source: source.display().to_string(),
        version,
        branch,
        size,
        blocked,
        replaces,
    })
}

/// Copia de `from` a `to` (que no existe): carpetas y archivos normales,
/// sin sockets, enlaces ni `.pid`. Lo copiado es del usuario actual.
pub(crate) fn copy_tree(from: &Path, to: &Path, done: &mut u64, on_bytes: &mut dyn FnMut(u64)) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new().mode(0o750).create(to).map_err(|error| {
        let dir = to.display();
        tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
    })?;
    let entries = fs::read_dir(from).map_err(|error| {
        let dir = from.display();
        tr!("No se pudo leer {dir}: {error}", "Couldn’t read {dir}: {error}")
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let text = name.to_string_lossy();
        let meta = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if meta.is_dir() {
            copy_tree(&path, &to.join(&name), done, on_bytes)?;
        } else if meta.is_file() && !text.ends_with(".pid") && !text.starts_with("mysql.sock") {
            let target = to.join(&name);
            fs::copy(&path, &target).map_err(|error| {
                let file = path.display();
                tr!("No se pudo copiar {file}: {error}", "Couldn’t copy {file}: {error}")
            })?;
            let mode = meta.permissions().mode() & 0o777 | 0o600;
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(mode));
            *done += meta.len();
            on_bytes(*done);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn reads_the_version_from_mysqld() {
        let out = "/x/bin/mysqld  Ver 8.4.11 for macos15 on arm64 (MySQL Community Server - GPL)\n";
        assert_eq!(parse_version(out).as_deref(), Some("8.4.11"));
        assert_eq!(parse_version("mysqld  Ver 8.0.33-commercial for x").as_deref(), Some("8.0.33"));
        assert_eq!(parse_version("algo raro"), None);
    }

    #[test]
    fn reads_the_version_of_existing_data() {
        let dir = TempDir::new("mysql-data-version");
        assert_eq!(data_version(dir.path()), None);
        fs::write(dir.path().join("mysql_upgrade_info"), "8.0.33\0").unwrap();
        assert_eq!(data_version(dir.path()).as_deref(), Some("8.0.33"));
        // El de este Mac: creado con 9.7.1 y actualizado hasta 26.10.0.
        fs::write(
            dir.path().join("mysql_upgrade_history"),
            r#"{"file_format":"1","upgrade_history":[{"date":"2026-07-23 08:01:57","version":"9.7.1","maturity":"LTS","initialize":true},{"date":"2026-08-24 12:52:50","version":"26.7.0","maturity":"INNOVATION"},{"date":"2026-10-26 06:18:38","version":"26.10.0","maturity":"INNOVATION"}]}"#,
        )
        .unwrap();
        assert_eq!(data_version(dir.path()).as_deref(), Some("26.10.0"));
    }

    #[test]
    fn data_in_use_by_a_mysqld_or_by_brew_is_not_offered() {
        let source = Path::new("/opt/homebrew/var/mysql");
        let ps = "/bin/sh /opt/homebrew/opt/mysql/bin/mysqld_safe --datadir=/opt/homebrew/var/mysql\n";
        assert!(source_in_use(source, ps, &[]));
        assert!(!source_in_use(source, "/usr/sbin/cfprefsd\n", &[]));
        let job = |pid| launchd::LaunchJob { label: "homebrew.mxcl.mysql".into(), pid, last_exit: Some(0) };
        assert!(source_in_use(source, "", &[job(Some(10))]));
        assert!(!source_in_use(source, "", &[job(None)]));
    }

    #[test]
    fn copies_regular_files_and_skips_sockets_and_pids() {
        let dir = TempDir::new("mysql-copy");
        let from = dir.path().join("from");
        fs::create_dir_all(from.join("shop")).unwrap();
        fs::write(from.join("ibdata1"), "1234").unwrap();
        fs::write(from.join("shop/orders.ibd"), "56").unwrap();
        fs::write(from.join("host.pid"), "99").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", from.join("enlace")).unwrap();
        let to = dir.path().join("to");
        let mut done = 0;
        copy_tree(&from, &to, &mut done, &mut |_| {}).unwrap();
        assert_eq!(done, 6);
        assert!(to.join("shop/orders.ibd").is_file());
        assert!(!to.join("host.pid").exists() && !to.join("enlace").exists());
        assert_eq!(readable_size(&to).unwrap(), 6);
    }
}
