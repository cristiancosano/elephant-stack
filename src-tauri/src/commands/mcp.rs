//! Canal de agentes (C26): conserva los nombres `mcp_*` aunque ahora el canal
//! principal es la CLI `elephant` con su plugin de Claude Code (05 §6.4).
//!
//! - El enlace `~/.elephant-stack/bin/elephant` apunta al sidecar `cli`.
//! - El plugin lo escribe el propio binario (`elephant __write-plugin`), así
//!   que siempre corresponde a su versión.
//! - «Conectar» ejecuta `claude plugin …` con argumentos separados y sin shell.
//! - La app nunca escribe la configuración de Claude Code ni de otros clientes:
//!   solo muestra fragmentos para copiar.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use elephant_core::audit::{self, AuditEntry};
use elephant_core::settings::{self, McpMode};
use elephant_core::{paths, tr, tr_str};
use serde::Serialize;
use serde_json::{Value, json};

use super::blocking;
use crate::sidecar;
use crate::state::McpBrief;

/// El plugin en el marketplace local.
const PLUGIN_ID: &str = "elephant@elephant-stack";
const MARKETPLACE: &str = "elephant-stack";
/// Lo que puede tardar una orden de `claude plugin` antes de darla por fallida.
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(60);
const CLI_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct McpState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct McpStatus {
    /// Existe el enlace `~/.elephant-stack/bin/elephant`.
    pub linked: bool,
    pub link_path: String,
    /// A dónde apunta el enlace.
    pub target: Option<String>,
    pub mode: McpMode,
    pub claude_path: Option<String>,
    /// El plugin `elephant@elephant-stack` está instalado en Claude Code.
    pub claude_registered: bool,
    #[cfg_attr(test, ts(type = "number | null"))]
    pub last_used_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct McpTestResult {
    /// Operaciones del catálogo.
    pub tools: u32,
    pub mode: McpMode,
    pub latency_ms: u32,
}

/// Fragmentos para copiar en cada cliente. Los MCP van vacíos si no existe
/// `elephant mcp`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct McpSnippets {
    pub project_json: String,
    pub cursor: String,
    pub claude_desktop: String,
    pub vscode: String,
    /// Los dos comandos que instalan el plugin.
    pub claude_command: String,
    /// La salida de `elephant guide`.
    pub shell_guide: String,
}

// ── Rutas ────────────────────────────────────────────────────────────────────

fn link_path() -> Result<PathBuf, String> {
    Ok(paths::data_dir()?.join("bin/elephant"))
}

fn plugin_dir() -> Result<PathBuf, String> {
    Ok(paths::data_dir()?.join("share/claude-plugin"))
}

/// El binario que ejecuta la app: el sidecar del bundle o, si falta, el enlace.
fn cli_binary() -> Result<PathBuf, String> {
    sidecar::path("cli")
        .or_else(|| link_path().ok().filter(|link| link.is_file()))
        .ok_or_else(|| {
            tr!(
                "No se encuentra la orden elephant de esta copia de Elephant Stack.",
                "Couldn’t find the elephant command in this copy of Elephant Stack."
            )
        })
}

fn claude_path() -> Option<PathBuf> {
    paths::tool_override("claude").or_else(|| paths::find_executable("claude"))
}

/// `~/.elephant-stack/…` con la ruta personal sustituida, para mostrarlo.
fn tilde(path: &Path) -> String {
    match paths::home().ok().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

// ── Procesos ─────────────────────────────────────────────────────────────────

/// Sin shell, con el PATH de la app, stdin cerrado y un plazo máximo.
fn run(program: &Path, args: &[&str], timeout: Duration) -> Result<Output, String> {
    let name = program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut child = Command::new(program)
        .args(args)
        .env("PATH", paths::child_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| tr!("No se pudo ejecutar {name}: {error}", "Couldn’t run {name}: {error}"))?;
    // Leer en hilos: una salida grande no debe bloquear al hijo.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let secs = timeout.as_secs();
                return Err(tr!("{name} no ha respondido en {secs} s.", "{name} didn’t respond in {secs} s."));
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    Ok(Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

fn checked(output: Output, what: &str) -> Result<String, String> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = stderr
        .trim()
        .lines()
        .last()
        .or_else(|| stdout.trim().lines().last())
        .unwrap_or(tr_str!("sin detalles", "no details"));
    Err(tr!("{what} ha fallado: {detail}", "{what} failed: {detail}"))
}

// ── Estado ───────────────────────────────────────────────────────────────────

/// `2026-11-06T10:21:04Z` (lo que escribe la auditoría) en milisegundos.
fn rfc3339_ms(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !text.ends_with('Z') {
        return None;
    }
    // Días desde 1970 (algoritmo de Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000)
}

fn last_used_ms() -> Option<i64> {
    audit::last_used().and_then(|entry| rfc3339_ms(&entry.at))
}

/// ¿Está `elephant@elephant-stack` en la salida de `claude plugin list --json`?
/// Acepta una lista o un objeto con `plugins`, y entradas como texto u objeto.
fn plugin_installed(json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let list = match &value {
        Value::Array(items) => items.as_slice(),
        Value::Object(map) => map.get("plugins").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]),
        _ => &[],
    };
    list.iter().any(|item| match item {
        Value::String(id) => id == PLUGIN_ID,
        Value::Object(map) => {
            let text = |key: &str| map.get(key).and_then(Value::as_str);
            text("id") == Some(PLUGIN_ID)
                || text("plugin") == Some(PLUGIN_ID)
                || (text("name") == Some("elephant")
                    && [text("marketplace"), text("marketplaceName"), text("source")].contains(&Some(MARKETPLACE)))
        }
        _ => false,
    })
}

fn claude_registered(claude: Option<&Path>) -> bool {
    claude
        .and_then(|claude| run(claude, &["plugin", "list", "--json"], CLAUDE_TIMEOUT).ok())
        .filter(|out| out.status.success())
        .is_some_and(|out| plugin_installed(&String::from_utf8_lossy(&out.stdout)))
}

fn status() -> Result<McpStatus, String> {
    let link = link_path()?;
    let claude = claude_path();
    Ok(McpStatus {
        linked: link.exists(),
        link_path: link.display().to_string(),
        target: std::fs::read_link(&link).ok().map(|t| t.display().to_string()),
        mode: settings::load().mcp.mode,
        claude_registered: claude_registered(claude.as_deref()),
        claude_path: claude.map(|p| p.display().to_string()),
        last_used_ms: last_used_ms(),
    })
}

/// Barato: se llama en cada refresco del estado.
pub fn brief() -> McpBrief {
    let linked = link_path().map(|link| link.exists()).unwrap_or(false);
    McpBrief {
        linked,
        last_used_ms: last_used_ms(),
        mode: settings::load().mcp.mode,
    }
}

// ── Enlace y plugin ──────────────────────────────────────────────────────────

/// Qué hacer con el enlace, según lo que hay en su sitio.
#[derive(Debug, PartialEq, Eq)]
enum LinkAction {
    Keep,
    Create,
    Replace,
    /// Un archivo normal (por ejemplo, el que deja `install-shim.sh` en
    /// desarrollo): no se toca.
    LeaveFile,
}

fn link_action(current: Option<Result<PathBuf, ()>>, target: &Path) -> LinkAction {
    match current {
        None => LinkAction::Create,
        Some(Ok(points_to)) if points_to == target => LinkAction::Keep,
        Some(Ok(_)) => LinkAction::Replace,
        Some(Err(())) => LinkAction::LeaveFile,
    }
}

/// `None` si no existe; `Err` si existe y no es un enlace.
fn inspect_link(link: &Path) -> Option<Result<PathBuf, ()>> {
    let meta = link.symlink_metadata().ok()?;
    if meta.file_type().is_symlink() {
        Some(std::fs::read_link(link).map_err(|_| ()))
    } else {
        Some(Err(()))
    }
}

fn ensure_link() -> Result<(), String> {
    let link = link_path()?;
    let target = sidecar::path("cli").ok_or_else(|| {
        tr!(
            "No se encuentra la orden elephant dentro de Elephant Stack.",
            "Couldn’t find the elephant command inside Elephant Stack."
        )
    })?;
    match link_action(inspect_link(&link), &target) {
        LinkAction::Keep | LinkAction::LeaveFile => Ok(()),
        LinkAction::Create | LinkAction::Replace => {
            if let Some(dir) = link.parent() {
                std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
            }
            // Se crea al lado y se renombra encima: nunca hay un momento sin enlace.
            let tmp = link.with_extension(format!("tmp-{}", std::process::id()));
            let _ = std::fs::remove_file(&tmp);
            let failed = |error: std::io::Error| tr!("No se pudo crear el enlace: {error}", "Couldn’t create the link: {error}");
            std::os::unix::fs::symlink(&target, &tmp).map_err(failed)?;
            std::fs::rename(&tmp, &link).map_err(failed)
        }
    }
}

fn remove_link() -> Result<(), String> {
    let link = link_path()?;
    match inspect_link(&link) {
        Some(Ok(_)) => std::fs::remove_file(&link)
            .map_err(|error| tr!("No se pudo quitar el enlace: {error}", "Couldn’t remove the link: {error}")),
        _ => Ok(()),
    }
}

fn write_plugin() -> Result<PathBuf, String> {
    let dir = plugin_dir()?;
    let dir_text = dir.display().to_string();
    checked(
        run(&cli_binary()?, &["__write-plugin", &dir_text], CLI_TIMEOUT)?,
        tr_str!("Escribir el plugin", "Write the plugin"),
    )?;
    Ok(dir)
}

/// Lo que dice `elephant --version` del binario de la app.
fn cli_version() -> Option<String> {
    let out = run(&cli_binary().ok()?, &["--version"], CLI_TIMEOUT).ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 03 §8.2 y 05 §6.2: con la integración activa, corrige el enlace si apunta
/// a otra copia de la app y reescribe el plugin si cambió la versión. Va en
/// otro hilo para no retrasar el arranque.
pub fn repair_link_on_startup() {
    std::thread::spawn(|| {
        let Ok(link) = link_path() else { return };
        if matches!(inspect_link(&link), Some(Ok(_))) {
            let _ = ensure_link();
        }
        let Ok(dir) = plugin_dir() else { return };
        let marker = dir.join(".elephant-version");
        let Ok(written) = std::fs::read_to_string(&marker) else { return };
        if cli_version().is_some_and(|current| current != written.trim()) {
            let _ = write_plugin();
        }
    });
}

// ── Prueba de conexión ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct Doctor {
    operations: u32,
    mode: McpMode,
    mcp: bool,
}

fn parse_doctor(text: &str) -> Result<Doctor, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| tr!("elephant doctor ha devuelto algo inesperado.", "elephant doctor returned something unexpected."))?;
    Ok(Doctor {
        operations: value["operations"].as_u64().unwrap_or(0) as u32,
        mode: serde_json::from_value(value["mode"].clone()).unwrap_or_default(),
        mcp: value["mcp"].as_bool().unwrap_or(false),
    })
}

fn doctor(binary: &Path) -> Result<(Doctor, Duration), String> {
    let start = Instant::now();
    let out = checked(run(binary, &["doctor", "--json"], CLI_TIMEOUT)?, "elephant doctor")?;
    Ok((parse_doctor(&out)?, start.elapsed()))
}

/// `initialize` y `tools/list` contra `elephant mcp`: cuántas herramientas.
fn mcp_handshake(binary: &Path) -> Result<u32, String> {
    let mut child = Command::new(binary)
        .arg("mcp")
        .env("PATH", paths::child_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| tr!("No se pudo ejecutar elephant mcp: {error}", "Couldn’t run elephant mcp: {error}"))?;
    let result = (|| {
        let mut stdin = child.stdin.take().ok_or("Sin stdin.")?;
        let stdout = child.stdout.take().ok_or("Sin stdout.")?;
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        // Con este nombre, `elephant mcp` no lo anota como uso de un agente
        // (elephant_mcp::PROBE_CLIENT).
        let messages = [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "elephant-stack", "version": env!("CARGO_PKG_VERSION")}}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        ];
        for message in messages {
            writeln!(stdin, "{message}").map_err(|error| error.to_string())?;
        }
        stdin.flush().map_err(|error| error.to_string())?;
        let deadline = Instant::now() + CLI_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = lines
                .recv_timeout(left)
                .map_err(|_| tr!("elephant mcp no ha respondido.", "elephant mcp didn’t respond."))?;
            let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
            if message["id"] == 2 {
                return message["result"]["tools"]
                    .as_array()
                    .map(|tools| tools.len() as u32)
                    .ok_or_else(|| tr!("elephant mcp no ha devuelto herramientas.", "elephant mcp returned no tools."));
            }
        }
    })();
    let _ = child.kill();
    let _ = child.wait();
    result
}

// ── Fragmentos ───────────────────────────────────────────────────────────────

fn mcp_snippets_for(command: &str) -> (String, String, String, String) {
    let server = json!({"command": command, "args": ["mcp"]});
    let pretty = |value: Value| serde_json::to_string_pretty(&value).unwrap_or_default();
    (
        // .mcp.json de un proyecto: sin rutas personales (Claude Code expande ${HOME}).
        pretty(json!({"mcpServers": {"elephant": {"type": "stdio", "command": "${HOME}/.elephant-stack/bin/elephant", "args": ["mcp"]}}})),
        pretty(json!({"mcpServers": {"elephant": server.clone()}})),
        pretty(json!({"mcpServers": {"elephant": server}})),
        pretty(json!({"servers": {"elephant": {"type": "stdio", "command": command, "args": ["mcp"]}}})),
    )
}

fn claude_commands(dir: &Path) -> String {
    format!(
        "claude plugin marketplace add {}\nclaude plugin install {PLUGIN_ID} -s user",
        tilde(dir)
    )
}

fn snippets() -> Result<McpSnippets, String> {
    let link = link_path()?;
    let binary = cli_binary()?;
    let shell_guide = checked(run(&binary, &["guide"], CLI_TIMEOUT)?, "elephant guide")?.trim_end().to_string();
    let with_mcp = doctor(&binary).map(|(d, _)| d.mcp).unwrap_or(false);
    let (project_json, cursor, claude_desktop, vscode) = if with_mcp {
        mcp_snippets_for(&link.display().to_string())
    } else {
        Default::default()
    };
    Ok(McpSnippets {
        project_json,
        cursor,
        claude_desktop,
        vscode,
        claude_command: claude_commands(&plugin_dir()?),
        shell_guide,
    })
}

// ── Conectar y desconectar ───────────────────────────────────────────────────

/// Un fallo que significa «ya estaba hecho» no impide seguir.
fn already_done(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    ["already", "ya existe", "ya está"].iter().any(|word| error.contains(word))
}

fn connect_claude() -> Result<(), String> {
    let claude = claude_path().ok_or_else(|| {
        tr!(
            "No se encuentra Claude Code (la orden claude). Instálalo y vuelve a intentarlo.",
            "Couldn’t find Claude Code (the claude command). Install it and try again."
        )
    })?;
    ensure_link()?;
    let dir = write_plugin()?;
    let dir_text = dir.display().to_string();
    if let Err(error) = checked(
        run(&claude, &["plugin", "marketplace", "add", &dir_text], CLAUDE_TIMEOUT)?,
        tr_str!("Añadir el marketplace", "Add the marketplace"),
    )
        && !already_done(&error)
    {
        return Err(error);
    }
    if let Err(error) = checked(
        run(&claude, &["plugin", "install", PLUGIN_ID, "-s", "user"], CLAUDE_TIMEOUT)?,
        tr_str!("Instalar el plugin", "Install the plugin"),
    )
        && !already_done(&error)
    {
        return Err(error);
    }
    Ok(())
}

fn disable() -> Result<(), String> {
    remove_link()?;
    let claude = claude_path();
    if claude_registered(claude.as_deref())
        && let Some(claude) = claude
    {
        checked(
            run(&claude, &["plugin", "uninstall", PLUGIN_ID], CLAUDE_TIMEOUT)?,
            tr_str!("Desinstalar el plugin", "Uninstall the plugin"),
        )?;
    }
    Ok(())
}

// ── Comandos ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn mcp_status() -> Result<McpStatus, String> {
    blocking(status).await
}

#[tauri::command]
pub async fn mcp_enable() -> Result<McpStatus, String> {
    blocking(|| {
        ensure_link()?;
        status()
    })
    .await
}

/// Quita el enlace y, si está instalado, el plugin de Claude Code.
#[tauri::command]
pub async fn mcp_disable() -> Result<McpStatus, String> {
    blocking(|| {
        disable()?;
        status()
    })
    .await
}

/// La interfaz enseña antes los comandos (`mcp_snippets().claude_command`) y
/// pide confirmación.
#[tauri::command]
pub async fn mcp_connect_claude() -> Result<McpStatus, String> {
    blocking(|| {
        connect_claude()?;
        status()
    })
    .await
}

#[tauri::command]
pub async fn mcp_test() -> Result<McpTestResult, String> {
    blocking(|| {
        let link = link_path()?;
        // Se prueba lo que usan los agentes: el enlace, si existe.
        let binary = if link.exists() { link } else { cli_binary()? };
        let (doctor, latency) = doctor(&binary)?;
        let tools = if doctor.mcp { mcp_handshake(&binary)? } else { doctor.operations };
        Ok(McpTestResult {
            tools,
            mode: doctor.mode,
            latency_ms: latency.as_millis().min(u128::from(u32::MAX)) as u32,
        })
    })
    .await
}

#[tauri::command]
pub async fn mcp_snippets() -> Result<McpSnippets, String> {
    blocking(snippets).await
}

/// Las más recientes primero, de `agent-audit.jsonl`.
#[tauri::command]
pub async fn mcp_activity(limit: u32) -> Result<Vec<AuditEntry>, String> {
    blocking(move || Ok(audit::recent(limit.min(500) as usize))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_audit_timestamps() {
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_ms("2026-11-06T10:21:04Z"), Some(1_793_960_464_000));
        assert_eq!(rfc3339_ms("2024-02-29T23:59:59Z"), Some(1_709_251_199_000));
        assert_eq!(rfc3339_ms("ayer"), None);
        assert_eq!(rfc3339_ms("2026-13-06T10:21:04Z"), None);
    }

    #[test]
    fn detects_the_installed_plugin() {
        assert!(!plugin_installed("[]"));
        assert!(!plugin_installed("no es json"));
        assert!(plugin_installed(r#"[{"id":"elephant@elephant-stack","enabled":true}]"#));
        assert!(plugin_installed(r#"{"plugins":[{"name":"elephant","marketplace":"elephant-stack"}]}"#));
        assert!(plugin_installed(r#"["otro@x","elephant@elephant-stack"]"#));
        assert!(!plugin_installed(r#"[{"name":"elephant","marketplace":"otro"}]"#));
    }

    #[test]
    fn the_link_is_only_replaced_when_it_is_ours() {
        let target = Path::new("/Applications/Elephant Stack.app/Contents/MacOS/cli");
        assert_eq!(link_action(None, target), LinkAction::Create);
        assert_eq!(link_action(Some(Ok(target.to_path_buf())), target), LinkAction::Keep);
        assert_eq!(link_action(Some(Ok(PathBuf::from("/viejo/cli"))), target), LinkAction::Replace);
        assert_eq!(link_action(Some(Err(())), target), LinkAction::LeaveFile);
    }

    #[test]
    fn snippets_point_to_the_stable_link() {
        let (project, cursor, desktop, vscode) = mcp_snippets_for("/Users/demo/.elephant-stack/bin/elephant");
        assert!(project.contains("${HOME}/.elephant-stack/bin/elephant") && !project.contains("/Users/demo"));
        for snippet in [&cursor, &desktop, &vscode] {
            let value: Value = serde_json::from_str(snippet).unwrap();
            let server = value.get("mcpServers").or_else(|| value.get("servers")).unwrap()["elephant"].clone();
            assert_eq!(server["command"], "/Users/demo/.elephant-stack/bin/elephant");
            assert_eq!(server["args"], json!(["mcp"]));
        }
        let commands = claude_commands(Path::new("/x/.elephant-stack/share/claude-plugin"));
        assert!(commands.ends_with("claude plugin install elephant@elephant-stack -s user"));
    }

    #[test]
    fn doctor_output_is_read() {
        let doctor = parse_doctor(r#"{"operations":17,"mode":"trusted","mcp":true}"#).unwrap();
        assert_eq!(doctor, Doctor { operations: 17, mode: McpMode::Trusted, mcp: true });
        assert!(parse_doctor("x").is_err());
        assert!(already_done("Marketplace 'elephant-stack' already exists"));
        assert!(!already_done("Permission denied"));
    }
}
