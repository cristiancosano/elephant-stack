use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use serde::{Deserialize, Serialize};

use crate::lock::BrewLock;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Service {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub running: bool,
    /// Tiene un plist en LaunchAgents, así que arranca al iniciar sesión.
    #[serde(default)]
    pub registered: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub pid: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub exit_code: Option<i64>,
}

/// Lo que pide el usuario. Arrancar o detener y el arranque al iniciar sesión
/// son decisiones independientes, aunque Homebrew las mezcla en sus comandos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Operation {
    TurnOn,
    TurnOff,
    Restart,
    EnableAutostart,
    DisableAutostart,
    /// Arrancar y dejarlo al iniciar sesión, con una sola llamada a brew.
    TurnOnWithAutostart,
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
        (Operation::TurnOnWithAutostart, _, _) => vec![START],
    }
}

fn brew_path() -> Result<PathBuf, String> {
    if let Some(path) = paths::tool_override("brew") {
        return Ok(path);
    }
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .iter()
        .map(Path::new)
        .find(|path| path.is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| "No se ha encontrado Homebrew.".to_string())
}

/// Lecturas: no necesitan el cerrojo.
fn run_brew_read(args: &[&str]) -> Result<String, String> {
    let output = Command::new(brew_path()?)
        .args(args)
        .env("PATH", paths::child_path())
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("No se pudo ejecutar brew: {error}"))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("brew {} falló: {}", args.join(" "), stderr.trim()))
}

/// Escrituras: exigen el cerrojo y entregan stdout y stderr línea a línea.
fn run_brew_write(
    _lock: &BrewLock,
    args: &[&str],
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    run_streaming(&brew_path()?, args, on_line)
}

fn run_streaming(program: &Path, args: &[&str], on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .env("PATH", paths::child_path())
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("No se pudo ejecutar brew: {error}"))?;

    // `on_line` no es Send: los hilos leen y este hilo entrega las líneas.
    let (sender, receiver) = mpsc::channel::<(bool, String)>();
    let readers: Vec<_> = [
        child.stdout.take().map(|out| (false, Box::new(out) as Box<dyn Read + Send>)),
        child.stderr.take().map(|err| (true, Box::new(err) as Box<dyn Read + Send>)),
    ]
    .into_iter()
    .flatten()
    .map(|(is_err, stream)| {
        let sender = sender.clone();
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if sender.send((is_err, line)).is_err() {
                    break;
                }
            }
        })
    })
    .collect();
    drop(sender);

    let mut last_error = String::new();
    for (is_err, line) in receiver {
        if is_err && !line.trim().is_empty() {
            last_error = line.trim().to_string();
        }
        on_line(&line);
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child
        .wait()
        .map_err(|error| format!("No se pudo esperar a brew: {error}"))?;
    if status.success() {
        Ok(())
    } else if last_error.is_empty() {
        Err(format!("brew {} falló ({status}).", args.join(" ")))
    } else {
        Err(format!("brew {} falló: {last_error}", args.join(" ")))
    }
}

fn parse_services(json: &str) -> Result<Vec<Service>, String> {
    serde_json::from_str(json)
        .map_err(|error| format!("Respuesta inesperada de brew services: {error}"))
}

pub fn list_services() -> Result<Vec<Service>, String> {
    parse_services(&run_brew_read(&["services", "info", "--all", "--json"])?)
}

pub fn list_formulas() -> Result<String, String> {
    run_brew_read(&["list", "--formula", "-1"])
}

pub fn update(lock: &BrewLock, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    run_brew_write(lock, &["update"], on_line)
}

/// Quien llama valida el nombre (ver `components::install`).
pub fn install_formula(
    lock: &BrewLock,
    name: &str,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    if name.starts_with('-') || name.is_empty() {
        return Err(format!("Nombre de fórmula no válido: {name}"));
    }
    run_brew_write(lock, &["install", name], on_line)
}

/// El nombre debe existir en la lista actual, así nadie puede pasarle a brew
/// opciones ni servicios arbitrarios.
pub fn apply(lock: &BrewLock, name: &str, operation: Operation) -> Result<Vec<Service>, String> {
    let services = list_services()?;
    let service = services
        .iter()
        .find(|service| service.name == name)
        .ok_or_else(|| format!("Servicio desconocido: {name}"))?;

    for step in plan(operation, service) {
        let mut args = vec!["services"];
        args.extend_from_slice(step);
        args.push(name);
        run_brew_write(lock, &args, &mut |_| {})?;
    }
    list_services()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, write_script};

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
    fn turn_on_with_autostart_is_a_single_start_in_any_state() {
        for (running, registered) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(steps(Operation::TurnOnWithAutostart, running, registered), [["start"]]);
        }
    }

    #[test]
    fn operations_round_trip_with_frontend_values() {
        let operation: Operation = serde_json::from_str("\"enable_autostart\"").unwrap();
        assert_eq!(operation, Operation::EnableAutostart);
        assert_eq!(
            serde_json::to_string(&Operation::TurnOnWithAutostart).unwrap(),
            "\"turn_on_with_autostart\""
        );
        assert!(serde_json::from_str::<Operation>("\"cleanup\"").is_err());
    }

    #[test]
    fn streaming_delivers_every_line_and_the_last_error() {
        let dir = TempDir::new("brew-stream");
        let fake = dir.path().join("brew");
        write_script(
            &fake,
            "echo \"==> $1 $2\"\necho 'aviso' >&2\n[ \"$HOMEBREW_NO_AUTO_UPDATE\" = 1 ] || exit 3\necho 'Error: sin red' >&2\nexit 1",
        );
        let mut lines = Vec::new();
        let error = run_streaming(&fake, &["install", "redis"], &mut |line| {
            lines.push(line.to_string())
        })
        .unwrap_err();
        assert!(lines.contains(&"==> install redis".to_string()));
        assert!(lines.contains(&"aviso".to_string()));
        assert_eq!(error, "brew install redis falló: Error: sin red");
    }

    #[test]
    fn streaming_succeeds_with_exit_zero() {
        let dir = TempDir::new("brew-ok");
        let fake = dir.path().join("brew");
        write_script(&fake, "echo listo");
        let _lock = BrewLock::for_tests(dir.path());
        let mut count = 0;
        run_streaming(&fake, &["update"], &mut |_| count += 1).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn rejects_formula_names_that_look_like_options() {
        let dir = TempDir::new("brew-name");
        let lock = BrewLock::for_tests(dir.path());
        assert!(install_formula(&lock, "--cask", &mut |_| {}).is_err());
    }
}
