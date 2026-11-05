use std::fs;
use std::process::Command;

use crate::paths;

/// Lo que se sabe de un ejecutable encontrado en el sistema.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub path: String,
    pub version: Option<String>,
    pub origin: &'static str,
}

pub fn probe(binary: &str, version_flag: &str) -> Option<Probe> {
    let path = paths::find_executable(binary)?;
    let version = Command::new(&path)
        .arg(version_flag)
        .env("PATH", paths::child_path())
        .output()
        .ok()
        .and_then(|output| {
            // nginx escribe su versión en stderr.
            short_version(&String::from_utf8_lossy(&output.stdout))
                .or_else(|| short_version(&String::from_utf8_lossy(&output.stderr)))
        });

    // El origen se deduce de la ruta real: los bin de Homebrew son enlaces.
    let real_path = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    Some(Probe {
        path: path.to_string_lossy().into_owned(),
        version,
        origin: origin_of(&real_path.to_string_lossy()),
    })
}

fn origin_of(path: &str) -> &'static str {
    if path.contains("/Cellar/") || path.starts_with("/opt/homebrew/") {
        "Homebrew"
    } else if path.contains("/Herd/") {
        "Herd"
    } else if path.contains("/.nvm/") {
        "nvm"
    } else if ["/usr/bin/", "/bin/", "/usr/sbin/", "/sbin/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        "Sistema"
    } else {
        "Otro"
    }
}

/// Primer número de versión de la salida de `--version`.
fn short_version(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    line.split(|c: char| c.is_whitespace() || c == '/' || c == '=' || c == ',')
        .map(|token| token.trim_start_matches('v'))
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()) && token.contains('.'))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_versions_from_real_formats() {
        let cases = [
            ("PHP 8.4.23 (cli) (built: Jul  6 2026)", "8.4.23"),
            ("Composer version 2.10.1 2026-06-04 10:25:59", "2.10.1"),
            ("v24.18.0", "24.18.0"),
            ("11.20.0", "11.20.0"),
            ("nginx version: nginx/1.31.8", "1.31.8"),
            ("psql (PostgreSQL) 18.6", "18.6"),
            ("mysql  Ver 26.10.0 for macos", "26.10.0"),
            ("Redis server v=8.10.3 sha=0", "8.10.3"),
        ];
        for (output, expected) in cases {
            assert_eq!(short_version(output).as_deref(), Some(expected), "{output}");
        }
        assert_eq!(short_version("  \n"), None);
    }

    #[test]
    fn classifies_origin_from_real_paths() {
        assert_eq!(origin_of("/opt/homebrew/Cellar/mysql/26.10.0_1/bin/mysql"), "Homebrew");
        assert_eq!(
            origin_of("/Users/c/Library/Application Support/Herd/bin/php84"),
            "Herd"
        );
        assert_eq!(origin_of("/Users/c/.nvm/versions/node/v24.18.0/bin/node"), "nvm");
        assert_eq!(origin_of("/usr/bin/php"), "Sistema");
        assert_eq!(origin_of("/Users/c/bin/php"), "Otro");
    }
}
