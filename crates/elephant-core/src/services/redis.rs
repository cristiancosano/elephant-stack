//! Lo propio de Redis (lo compila el CI de php-builds): una configuración
//! generada en cada arranque y `redis-server` con ella. Sin importar datos.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{Env, ServiceKind};
use crate::fsutil;

const KIND: ServiceKind = ServiceKind::Redis;

pub(crate) fn config_path(env: &Env, branch: &str) -> PathBuf {
    env.config_dir().join(format!("redis-{branch}.conf"))
}

/// Entre comillas, como las lee `redis.conf`.
fn quoted(path: &Path) -> String {
    let text = path.display().to_string();
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Solo en 127.0.0.1, en primer plano (lo vigila launchd) y con el log en
/// la salida, que launchd guarda en `logs/redis-<rama>.err`.
pub(crate) fn config(env: &Env, branch: &str) -> String {
    let port = env.port(KIND);
    let dir = quoted(&env.data_dir(KIND, branch));
    format!(
        "# Generado por Elephant Stack en cada arranque: los cambios se pierden.\n\
port {port}\n\
bind 127.0.0.1\n\
protected-mode yes\n\
daemonize no\n\
supervised no\n\
dir {dir}\n\
dbfilename dump.rdb\n\
logfile \"\"\n\
save 3600 1 300 100 60 10000\n"
    )
}

pub(crate) fn write_config(env: &Env, branch: &str) -> Result<(), String> {
    fsutil::write_atomic(&config_path(env, branch), config(env, branch).as_bytes())
}

pub(crate) fn daemon_args(env: &Env, install: &Path, branch: &str) -> Vec<String> {
    vec![KIND.daemon(install).display().to_string(), config_path(env, branch).display().to_string()]
}

/// «Redis server v=8.2.11 sha=00000000:0 malloc=libc bits=64 build=…» → `8.2.11`.
pub(crate) fn parse_version(text: &str) -> Option<String> {
    let version = text.split_whitespace().find_map(|word| word.strip_prefix("v="))?;
    super::catalog::is_version(version).then(|| version.to_string())
}

pub(crate) fn version_of(install: &Path) -> Option<String> {
    let output = Command::new(KIND.daemon(install)).arg("--version").stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| parse_version(&String::from_utf8_lossy(&output.stdout))).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_from_redis_server() {
        let out = "Redis server v=8.2.11 sha=00000000:0 malloc=libc bits=64 build=1f2a\n";
        assert_eq!(parse_version(out).as_deref(), Some("8.2.11"));
        assert_eq!(parse_version("redis-server x"), None);
    }

    #[test]
    fn quotes_paths_for_redis_conf() {
        assert_eq!(quoted(Path::new("/a b/\"c\"")), "\"/a b/\\\"c\\\"\"");
    }
}
