//! Lo propio de Mailpit: SMTP y web solo en 127.0.0.1 y su base de datos en
//! `data/mailpit/mailpit.db`, la misma para todas las versiones.

use std::path::Path;
use std::process::{Command, Stdio};

use super::{Env, ServiceKind};

const KIND: ServiceKind = ServiceKind::Mailpit;

pub(crate) fn daemon_args(env: &Env, install: &Path) -> Vec<String> {
    let smtp = env.port(KIND);
    let web = env.web_port(KIND).unwrap_or(8025);
    vec![
        KIND.daemon(install).display().to_string(),
        "--smtp".into(),
        format!("127.0.0.1:{smtp}"),
        "--listen".into(),
        format!("127.0.0.1:{web}"),
        "--database".into(),
        env.data_dir(KIND, "").join("mailpit.db").display().to_string(),
        "--disable-version-check".into(),
    ]
}

/// «mailpit v1.33.0 compiled with go1.27.3 on darwin/arm64» → `1.33.0`.
pub(crate) fn parse_version(text: &str) -> Option<String> {
    let version = text.split_whitespace().find_map(|word| word.strip_prefix('v'))?;
    super::catalog::is_version(version).then(|| version.to_string())
}

pub(crate) fn version_of(install: &Path) -> Option<String> {
    let output = Command::new(KIND.daemon(install)).args(["version", "--no-release-check"]).stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| parse_version(&String::from_utf8_lossy(&output.stdout))).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_from_mailpit() {
        assert_eq!(parse_version("mailpit v1.33.0 compiled with go1.27.3 on darwin/arm64\n").as_deref(), Some("1.33.0"));
        assert_eq!(parse_version("mailpit"), None);
    }
}
