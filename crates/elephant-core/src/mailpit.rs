//! Estado de Mailpit para el menú de la barra y el panel: si está instalado,
//! si está en marcha y cuántos correos ha capturado. Vale el de Elephant
//! (06, `services::status`) y el de Homebrew: los dos usan el puerto 8025.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::brew::Service;
use crate::paths;
use crate::services::{ManagedService, RunState, ServiceKind};

const URL: &str = "http://127.0.0.1:8025";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MailpitInfo {
    pub installed: bool,
    pub running: bool,
    /// Interfaz web de Mailpit.
    pub url: String,
    /// Correos guardados; `None` si no está en marcha o no responde.
    pub messages: Option<u32>,
}

impl Default for MailpitInfo {
    fn default() -> Self {
        Self {
            installed: false,
            running: false,
            url: URL.to_string(),
            messages: None,
        }
    }
}

/// Usa las listas de servicios que ya tiene quien llama (los de brew y los
/// propios); solo consulta la API de Mailpit (con 1 s como mucho) si está en
/// marcha. Si están los dos, manda el propio.
pub fn info(services: &[Service], managed: &[ManagedService]) -> MailpitInfo {
    let own = managed.iter().find(|m| m.service == ServiceKind::Mailpit && !m.versions.is_empty());
    let own_running = own.is_some_and(|m| m.branches.iter().any(|b| b.state == RunState::Running));
    let brew = services.iter().find(|service| service.name == "mailpit");
    let brew_running = brew.is_some_and(|service| service.running);
    let url = match own.and_then(|m| m.web_port) {
        Some(port) if own_running || !brew_running => format!("http://127.0.0.1:{port}"),
        _ => URL.to_string(),
    };
    let running = own_running || brew_running;
    MailpitInfo {
        installed: own.is_some() || brew.is_some(),
        running,
        messages: if running { count_messages(&url) } else { None },
        url,
    }
}

fn curl_path() -> PathBuf {
    paths::tool_override("curl").unwrap_or_else(|| PathBuf::from("/usr/bin/curl"))
}

fn count_messages(url: &str) -> Option<u32> {
    let output = Command::new(curl_path())
        .args(["-sS", "-f", "--max-time", "1"])
        .arg(format!("{url}/api/v1/messages?limit=1"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_total(&String::from_utf8_lossy(&output.stdout))
}

/// La API devuelve `{"total": N, "unread": M, "messages": […]}`.
fn parse_total(json: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value.get("total")?.as_u64()?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(name: &str, running: bool) -> Service {
        Service {
            name: name.into(),
            status: if running { "started" } else { "none" }.into(),
            running,
            registered: false,
            pid: None,
            exit_code: None,
        }
    }

    #[test]
    fn reads_the_total_from_the_api() {
        assert_eq!(parse_total(r#"{"total":12,"unread":3,"messages":[]}"#), Some(12));
        assert_eq!(parse_total(r#"{"messages":[]}"#), None);
        assert_eq!(parse_total("<html>"), None);
    }

    fn own(state: Option<RunState>) -> ManagedService {
        use crate::services::{BranchStatus, InstalledVersion};
        ManagedService {
            service: ServiceKind::Mailpit,
            name: "Mailpit".into(),
            port: 1025,
            web_port: Some(8025),
            versions: vec![InstalledVersion { version: "1.33.0".into(), branch: "1".into(), path: "/x".into() }],
            branches: state
                .map(|state| BranchStatus {
                    branch: "1".into(),
                    version: "1.33.0".into(),
                    label: "sh.elephant.mailpit.1".into(),
                    state,
                    pid: None,
                    autostart: false,
                    error: None,
                    data_dir: "/d".into(),
                    data_ready: true,
                    log_path: "/l".into(),
                })
                .into_iter()
                .collect(),
            import: None,
        }
    }

    #[test]
    fn without_the_service_it_is_not_installed() {
        let info = info(&[service("redis", true)], &[]);
        assert_eq!(info, MailpitInfo::default());
    }

    #[test]
    fn a_stopped_service_does_not_call_the_api() {
        let info = info(&[service("mailpit", false)], &[]);
        assert!(info.installed);
        assert!(!info.running);
        assert_eq!(info.messages, None);
        // El propio, instalado y parado, también cuenta como instalado.
        let info = super::info(&[], &[own(Some(RunState::Stopped))]);
        assert!(info.installed && !info.running);
        assert_eq!(info.url, URL);
    }
}
