//! Estado de los servicios de Homebrew a partir de `launchctl list`: menos de
//! 10 ms y sin ejecutar `brew`. Lo usan `elephant status --brief` y la
//! vigilancia de la app.

use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

const HOMEBREW_PREFIX: &str = "homebrew.mxcl.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LaunchJob {
    pub label: String,
    pub pid: Option<i64>,
    /// Último código de salida; negativo si terminó por una señal.
    pub last_exit: Option<i64>,
}

impl LaunchJob {
    /// `homebrew.mxcl.postgresql@18` → `postgresql@18`.
    pub fn service_name(&self) -> Option<&str> {
        self.label.strip_prefix(HOMEBREW_PREFIX)
    }
}

/// Trabajos de launchd de los servicios de Homebrew (`homebrew.mxcl.*`).
pub fn homebrew_jobs() -> Result<Vec<LaunchJob>, String> {
    let output = Command::new("/bin/launchctl")
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| crate::tr!("No se pudo ejecutar launchctl: {error}", "Couldn’t run launchctl: {error}"))?;
    if !output.status.success() {
        return Err("launchctl list ha fallado.".to_string());
    }
    Ok(parse_list(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .filter(|job| job.label.starts_with(HOMEBREW_PREFIX))
        .collect())
}

/// Columnas `PID  Status  Label`, separadas por tabuladores. Un `-` significa
/// que no hay proceso o que no hay código de salida. La cabecera y las líneas
/// mal formadas se ignoran.
pub fn parse_list(text: &str) -> Vec<LaunchJob> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let pid = fields.next()?.trim();
            let status = fields.next()?.trim();
            let label = fields.next()?.trim();
            if label.is_empty() || fields.next().is_some() {
                return None;
            }
            let pid = match pid {
                "-" => None,
                value => Some(value.parse::<i64>().ok()?),
            };
            let last_exit = match status {
                "-" => None,
                value => Some(value.parse::<i64>().ok()?),
            };
            Some(LaunchJob {
                label: label.to_string(),
                pid,
                last_exit,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracto real de `launchctl list` en macOS 26, con los PIDs cambiados.
    const SAMPLE: &str = "PID\tStatus\tLabel
-\t0\tcom.apple.SafariHistoryServiceAgent
4521\t0\thomebrew.mxcl.postgresql@18
-\t1\thomebrew.mxcl.mysql
-\t-9\thomebrew.mxcl.redis
812\t-\tcom.apple.Finder
basura sin tabuladores
x\t0\thomebrew.mxcl.raro
";

    #[test]
    fn parses_pids_and_exit_codes() {
        let jobs = parse_list(SAMPLE);
        assert_eq!(jobs.len(), 5);
        assert_eq!(
            jobs[1],
            LaunchJob {
                label: "homebrew.mxcl.postgresql@18".into(),
                pid: Some(4521),
                last_exit: Some(0),
            }
        );
        assert_eq!(jobs[2].pid, None);
        assert_eq!(jobs[2].last_exit, Some(1));
        assert_eq!(jobs[3].last_exit, Some(-9));
        assert_eq!(jobs[4].last_exit, None);
    }

    #[test]
    fn skips_header_and_malformed_lines() {
        let jobs = parse_list(SAMPLE);
        assert!(jobs.iter().all(|job| job.label != "Label"));
        assert!(jobs.iter().all(|job| job.label != "homebrew.mxcl.raro"));
    }

    #[test]
    fn service_name_strips_the_homebrew_prefix() {
        let jobs = parse_list(SAMPLE);
        assert_eq!(jobs[1].service_name(), Some("postgresql@18"));
        assert_eq!(jobs[0].service_name(), None);
    }

    #[test]
    #[ignore = "lee el launchd real"]
    fn reads_real_launchd() {
        println!("{:#?}", homebrew_jobs().unwrap());
    }
}
