//! Canal de agentes (C26): conserva los nombres `mcp_*` aunque ahora el canal
//! principal es la CLI `elephant` con su plugin de Claude Code. Stub de P01;
//! P09 lo implementa.

use elephant_core::audit::{self, AuditEntry};
use elephant_core::settings::{self, McpMode};
use elephant_core::{NOT_AVAILABLE, paths};
use serde::Serialize;

use super::blocking;
use crate::state::McpBrief;

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

/// Barato: se llama en cada refresco del estado.
pub fn brief() -> McpBrief {
    let linked = paths::data_dir()
        .map(|dir| dir.join("bin/elephant").symlink_metadata().is_ok())
        .unwrap_or(false);
    McpBrief {
        linked,
        last_used_ms: None,
        mode: settings::load().mcp.mode,
    }
}

/// Repara el enlace `elephant` si apunta a otro sitio y reescribe el plugin
/// si cambió la versión (03 §8.2, 05 §6.2). P09.
pub fn repair_link_on_startup() {}

#[tauri::command]
pub async fn mcp_status() -> Result<McpStatus, String> {
    Err(NOT_AVAILABLE.into())
}

#[tauri::command]
pub async fn mcp_enable() -> Result<McpStatus, String> {
    Err(NOT_AVAILABLE.into())
}

#[tauri::command]
pub async fn mcp_disable() -> Result<McpStatus, String> {
    Err(NOT_AVAILABLE.into())
}

#[tauri::command]
pub async fn mcp_connect_claude() -> Result<McpStatus, String> {
    Err(NOT_AVAILABLE.into())
}

#[tauri::command]
pub async fn mcp_test() -> Result<McpTestResult, String> {
    Err(NOT_AVAILABLE.into())
}

#[tauri::command]
pub async fn mcp_snippets() -> Result<McpSnippets, String> {
    Err(NOT_AVAILABLE.into())
}

/// Las más recientes primero, de `agent-audit.jsonl`.
#[tauri::command]
pub async fn mcp_activity(limit: u32) -> Result<Vec<AuditEntry>, String> {
    blocking(move || Ok(audit::recent(limit as usize))).await
}
