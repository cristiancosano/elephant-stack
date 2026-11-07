//! Lo propio de PostgreSQL (binarios de theseus-rs): argumentos de `postgres`,
//! primer arranque (`initdb`), apagado rápido y los datos de Homebrew
//! (`/opt/homebrew/var/postgresql@<mayor>`) que se pueden copiar.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{Env, ImportOffer, ServiceKind, last_useful_line, readable_size};
use crate::{launchd, tr};

const KIND: ServiceKind = ServiceKind::Postgresql;

/// `postgres -D … -p … -k …`. La línea de órdenes manda sobre el
/// `postgresql.conf` de los datos (los copiados de brew traen el suyo): solo
/// escucha en 127.0.0.1 y su socket va a una carpeta propia, no a `/tmp`.
pub(crate) fn daemon_args(env: &Env, install: &Path, branch: &str) -> Vec<String> {
    vec![
        KIND.daemon(install).display().to_string(),
        "-D".into(),
        env.data_dir(KIND, branch).display().to_string(),
        "-p".into(),
        env.port(KIND).to_string(),
        "-k".into(),
        env.socket_path(KIND, branch).display().to_string(),
        "-c".into(),
        "listen_addresses=127.0.0.1".into(),
        "-c".into(),
        "unix_socket_permissions=0700".into(),
    ]
}

/// «postgres (PostgreSQL) 18.6» → `18.6`.
pub(crate) fn parse_version(text: &str) -> Option<String> {
    let word = text.split_whitespace().last()?;
    super::catalog::is_version(word).then(|| word.to_string())
}

pub(crate) fn version_of(install: &Path) -> Option<String> {
    let output = Command::new(KIND.daemon(install)).arg("--version").stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| parse_version(&String::from_utf8_lossy(&output.stdout))).flatten()
}

/// `en_US.UTF-8` si el sistema lo tiene (macOS siempre); si no, `C`.
fn locale() -> &'static str {
    if Path::new("/usr/share/locale/en_US.UTF-8").exists() { "en_US.UTF-8" } else { "C" }
}

/// Primer arranque: superusuario `postgres` sin contraseña desde esta
/// máquina, como en Herd. `initdb` deja la carpeta en 0700.
pub(crate) fn initialize(env: &Env, install: &Path, branch: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    let data = env.data_dir(KIND, branch);
    let dir = data.display().to_string();
    on_line(&tr!("Preparando los datos en {dir}", "Preparing the data in {dir}"));
    let output = Command::new(install.join("bin/initdb"))
        .arg("-D")
        .arg(&data)
        .args(["-U", "postgres", "--auth=trust", "-E", "UTF8"])
        .arg(format!("--locale={}", locale()))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| tr!("No se pudo ejecutar initdb: {error}", "Couldn’t run initdb: {error}"))?;
    let log = env.log_path(KIND, branch);
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log) {
        use std::io::Write;
        let _ = file.write_all(text.as_bytes());
    }
    if output.status.success() {
        return Ok(());
    }
    // Sin dejar datos a medias: el siguiente intento empieza de cero.
    let _ = fs::remove_dir_all(&data);
    let detail = last_useful_line(&text).unwrap_or_else(|| output.status.to_string());
    Err(tr!(
        "No se han podido preparar los datos de PostgreSQL {branch}: {detail}",
        "Couldn’t prepare the PostgreSQL {branch} data: {detail}"
    ))
}

/// `postgres` no arranca si su carpeta de datos se puede leer o escribir por
/// otros (solo admite 0700 o 0750): se corrige, porque es la de Elephant.
/// También se crea la carpeta de su socket.
pub(crate) fn before_start(env: &Env, branch: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let data = env.data_dir(KIND, branch);
    let mode = fs::metadata(&data).map(|m| m.permissions().mode() & 0o777).unwrap_or(0o700);
    if mode & 0o027 != 0 {
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).map_err(|error| {
            let dir = data.display();
            tr!("No se pudieron cambiar los permisos de {dir}: {error}", "Couldn’t change the permissions of {dir}: {error}")
        })?;
    }
    super::create_private_dir(&env.socket_path(KIND, branch))
}

/// Apagado rápido: corta las conexiones y cierra bien (sin recuperación al
/// arrancar). Si falla, `bootout` lo apaga igual.
pub(crate) fn fast_stop(env: &Env, install: &Path, branch: &str) {
    let _ = Command::new(install.join("bin/pg_ctl"))
        .arg("stop")
        .arg("-D")
        .arg(env.data_dir(KIND, branch))
        .args(["-m", "fast", "-w", "-t", "20"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Lo que no se copia: el `postmaster.pid` del servidor de brew.
pub(crate) fn skipped(name: &str) -> bool {
    name == "postmaster.pid"
}

/// Las carpetas de datos de brew: `var/postgresql@18` y, de la fórmula sin
/// versión, `var/postgresql` o `var/postgres`. Con su mayor y su fórmula.
pub(crate) fn brew_sources(env: &Env) -> Vec<(PathBuf, String, String)> {
    let Ok(entries) = fs::read_dir(env.brew_prefix.join("var")) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let (formula, from_name) = match name.strip_prefix("postgresql@") {
                Some(major) => (name.clone(), Some(major.to_string())),
                None if name == "postgresql" || name == "postgres" => ("postgresql".to_string(), None),
                None => return None,
            };
            let major = fs::read_to_string(path.join("PG_VERSION"))
                .ok()
                .map(|text| text.trim().to_string())
                .or(from_name)
                .filter(|major| !major.is_empty() && major.len() <= 4 && major.bytes().all(|b| b.is_ascii_digit()))?;
            Some((path, major, formula))
        })
        .collect();
    found.sort_by_key(|(_, major, _)| std::cmp::Reverse(major.parse::<u32>().unwrap_or(0)));
    found
}

/// Un `postgres` usa `source`, o el servicio de brew de esa fórmula está en marcha.
pub(crate) fn source_in_use(source: &Path, formula: &str, ps: &str, brew_jobs: &[launchd::LaunchJob]) -> bool {
    let shown = source.display().to_string();
    let by_process = ps.lines().any(|line| line.contains("postgres") && line.contains(&shown));
    let by_brew = brew_jobs.iter().any(|job| job.service_name() == Some(formula) && job.pid.is_some());
    by_process || by_brew
}

pub(crate) fn brew_offer(env: &Env) -> Option<ImportOffer> {
    brew_offer_with(env, &|source, formula| {
        env.brew_in_use.unwrap_or_else(|| {
            source_in_use(source, formula, &super::mysql::ps_commands(), &launchd::homebrew_jobs().unwrap_or_default())
        })
    })
}

/// Cómo pasar a mano unos datos que no se pueden copiar: un volcado con
/// `pg_dumpall` desde el servidor de brew y cargarlo en el de Elephant.
fn manual_steps(env: &Env, owner: &str, branch: &str, formula: &str) -> String {
    let home = env.tools.home.display().to_string();
    let tilde = |path: PathBuf| {
        let text = path.display().to_string();
        text.strip_prefix(&home).map_or(text.clone(), |rest| format!("~{rest}"))
    };
    let dump = tilde(env.brew_prefix.join("opt").join(formula).join("bin/pg_dumpall"));
    let psql = match super::version_for(env, KIND, branch) {
        Some(version) => tilde(env.install_dir(KIND, &version).join("bin/psql")),
        None => tilde(env.versions_dir(KIND).join("<versión>/bin/psql")),
    };
    let file = format!("~/postgresql-{branch}.sql");
    tr!(
        "Son de la cuenta {owner} y no se pueden copiar desde la tuya. Pásalos con un volcado: con el PostgreSQL de Homebrew en marcha, «{dump} -h 127.0.0.1 -p 5432 -U {owner} -f {file}»; después «brew services stop {formula}», enciende aquí PostgreSQL {branch} y ejecuta «{psql} -h 127.0.0.1 -U postgres -f {file} postgres».",
        "They belong to the {owner} account and can’t be copied from yours. Move them with a dump: with Homebrew’s PostgreSQL running, run “{dump} -h 127.0.0.1 -p 5432 -U {owner} -f {file}”; then “brew services stop {formula}”, turn on PostgreSQL {branch} here and run “{psql} -h 127.0.0.1 -U postgres -f {file} postgres”."
    )
}

/// La oferta para la mayor más reciente que haya en brew, si no se ha copiado
/// ya. Si no se puede leer entera, en vez del botón se explica el volcado.
pub(crate) fn brew_offer_with(env: &Env, in_use: &dyn Fn(&Path, &str) -> bool) -> Option<ImportOffer> {
    let (source, branch, formula) = brew_sources(env).into_iter().next()?;
    if env.import_marker(KIND, &branch).exists() {
        return None;
    }
    let (size, unreadable) = match readable_size(&source, &skipped) {
        Ok(size) => (size, None),
        Err(path) => (0, Some(path)),
    };
    let (blocked, manual) = if unreadable.is_some() {
        use std::os::unix::fs::MetadataExt;
        let me = crate::fsutil::current_uid();
        let uid = fs::metadata(&source).map_or(me, |m| m.uid());
        let owner = if uid == me { std::env::var("USER").unwrap_or_default() } else { super::owner_name(uid) };
        (None, Some(manual_steps(env, &owner, &branch, &formula)))
    } else if in_use(&source, &formula) {
        (
            Some(tr!(
                "El PostgreSQL de Homebrew está usando estos datos: detenlo con «brew services stop {formula}» antes de copiarlos.",
                "Homebrew’s PostgreSQL is using this data. Stop it with “brew services stop {formula}” before copying."
            )),
            None,
        )
    } else {
        (None, None)
    };
    let target = env.data_dir(KIND, &branch);
    let replaces = fs::read_dir(&target).is_ok_and(|mut d| d.next().is_some());
    Some(ImportOffer {
        source: source.display().to_string(),
        version: branch.clone(),
        branch,
        size,
        blocked,
        manual,
        replaces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_from_postgres() {
        assert_eq!(parse_version("postgres (PostgreSQL) 18.6\n").as_deref(), Some("18.6"));
        assert_eq!(parse_version("algo raro"), None);
    }

    #[test]
    fn data_in_use_by_a_postgres_or_by_brew_is_detected() {
        let source = Path::new("/opt/homebrew/var/postgresql@18");
        let ps = "/opt/homebrew/opt/postgresql@18/bin/postgres -D /opt/homebrew/var/postgresql@18\n";
        assert!(source_in_use(source, "postgresql@18", ps, &[]));
        assert!(!source_in_use(source, "postgresql@18", "/usr/sbin/cfprefsd\n", &[]));
        let job = |label: &str, pid| launchd::LaunchJob { label: label.into(), pid, last_exit: Some(0) };
        assert!(source_in_use(source, "postgresql@18", "", &[job("homebrew.mxcl.postgresql@18", Some(10))]));
        assert!(!source_in_use(source, "postgresql@18", "", &[job("homebrew.mxcl.postgresql@17", Some(10))]));
        assert!(!source_in_use(source, "postgresql@18", "", &[job("homebrew.mxcl.postgresql@18", None)]));
    }
}
