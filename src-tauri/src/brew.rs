use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Service {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub running: bool,
    /// Tiene un plist en LaunchAgents, así que arranca al iniciar sesión.
    #[serde(default)]
    pub registered: bool,
    pub pid: Option<i64>,
    pub exit_code: Option<i64>,
}

/// Lo que pide el usuario. Arrancar o detener y el arranque al iniciar sesión
/// son decisiones independientes, aunque Homebrew las mezcla en sus comandos.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    TurnOn,
    TurnOff,
    Restart,
    EnableAutostart,
    DisableAutostart,
}

/// Comandos de `brew services` que aplican una operación sin alterar el resto
/// del estado. Homebrew no permite cambiar el registro sin tocar el proceso, así
/// que cambiar el arranque automático reinicia el servicio o lo arranca un momento.
fn plan(operation: Operation, service: &Service) -> Vec<&'static [&'static str]> {
    const RUN: &[&str] = &["run"];
    const START: &[&str] = &["start"];
    const STOP: &[&str] = &["stop"];
    const STOP_KEEP: &[&str] = &["stop", "--keep"];
    const RESTART: &[&str] = &["restart"];

    match (operation, service.running, service.registered) {
        (Operation::TurnOn, _, true) => vec![START],
        (Operation::TurnOn, _, false) => vec![RUN],
        (Operation::TurnOff, _, true) => vec![STOP_KEEP],
        (Operation::TurnOff, _, false) => vec![STOP],
        (Operation::Restart, _, _) => vec![RESTART],
        (Operation::EnableAutostart, true, _) => vec![STOP, START],
        (Operation::EnableAutostart, false, _) => vec![START, STOP_KEEP],
        (Operation::DisableAutostart, true, _) => vec![STOP, RUN],
        (Operation::DisableAutostart, false, _) => vec![STOP],
    }
}

fn brew_path() -> Result<PathBuf, String> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .iter()
        .map(Path::new)
        .find(|path| path.is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| "No se ha encontrado Homebrew.".to_string())
}

fn run_brew(args: &[&str]) -> Result<String, String> {
    let output = Command::new(brew_path()?)
        .args(args)
        .env("PATH", paths::child_path())
        .output()
        .map_err(|error| format!("No se pudo ejecutar brew: {error}"))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("brew {} falló: {}", args.join(" "), stderr.trim()))
}

fn parse_services(json: &str) -> Result<Vec<Service>, String> {
    serde_json::from_str(json)
        .map_err(|error| format!("Respuesta inesperada de brew services: {error}"))
}

pub fn list_services() -> Result<Vec<Service>, String> {
    parse_services(&run_brew(&["services", "info", "--all", "--json"])?)
}

pub fn list_formulas() -> Result<String, String> {
    run_brew(&["list", "--formula", "-1"])
}

pub fn install_formula(name: &str) -> Result<(), String> {
    run_brew(&["install", name]).map(|_| ())
}

/// El nombre debe existir en la lista actual, así el frontend no puede pasarle
/// a brew opciones ni servicios arbitrarios.
pub fn apply(name: &str, operation: Operation) -> Result<Vec<Service>, String> {
    let services = list_services()?;
    let service = services
        .iter()
        .find(|service| service.name == name)
        .ok_or_else(|| format!("Servicio desconocido: {name}"))?;

    for step in plan(operation, service) {
        let mut args = vec!["services"];
        args.extend_from_slice(step);
        args.push(name);
        run_brew(&args)?;
    }
    list_services()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
        {"name":"mysql","service_name":"sh.brew.mysql","running":false,"loaded":true,
         "pid":null,"exit_code":0,"status":"stopped","registered":true},
        {"name":"postgresql@18","running":true,"pid":9201,"exit_code":null,
         "status":"started","registered":true}
    ]"#;

    fn service(running: bool, registered: bool) -> Service {
        Service {
            name: "demo".to_string(),
            status: "none".to_string(),
            running,
            registered,
            pid: None,
            exit_code: None,
        }
    }

    fn steps(operation: Operation, running: bool, registered: bool) -> Vec<Vec<&'static str>> {
        plan(operation, &service(running, registered))
            .into_iter()
            .map(<[&str]>::to_vec)
            .collect()
    }

    #[test]
    fn parses_brew_output_and_ignores_unknown_fields() {
        let services = parse_services(SAMPLE).unwrap();
        assert_eq!(services.len(), 2);
        assert!(!services[0].running);
        assert_eq!(services[1].pid, Some(9201));
    }

    #[test]
    fn reports_invalid_json() {
        assert!(parse_services("Warning: algo").is_err());
    }

    #[test]
    fn turning_on_and_off_keeps_the_autostart_choice() {
        assert_eq!(steps(Operation::TurnOn, false, false), [["run"]]);
        assert_eq!(steps(Operation::TurnOn, false, true), [["start"]]);
        assert_eq!(steps(Operation::TurnOff, true, false), [["stop"]]);
        assert_eq!(steps(Operation::TurnOff, true, true), [["stop", "--keep"]]);
    }

    #[test]
    fn changing_autostart_keeps_the_running_state() {
        assert_eq!(
            steps(Operation::EnableAutostart, true, false),
            vec![vec!["stop"], vec!["start"]]
        );
        assert_eq!(
            steps(Operation::EnableAutostart, false, false),
            vec![vec!["start"], vec!["stop", "--keep"]]
        );
        assert_eq!(
            steps(Operation::DisableAutostart, true, true),
            vec![vec!["stop"], vec!["run"]]
        );
        assert_eq!(steps(Operation::DisableAutostart, false, true), [["stop"]]);
    }

    #[test]
    fn operations_deserialize_from_frontend_values() {
        let operation: Operation = serde_json::from_str("\"enable_autostart\"").unwrap();
        assert_eq!(operation, Operation::EnableAutostart);
        assert!(serde_json::from_str::<Operation>("\"cleanup\"").is_err());
    }
}
