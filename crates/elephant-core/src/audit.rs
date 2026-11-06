//! Registro de auditoría del canal de agentes:
//! `~/.elephant-stack/logs/agent-audit.jsonl`, una línea JSON por llamada. Rota
//! al pasar de 1 MB y guarda un único `.1` de respaldo.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{paths, redact};

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AuditEntry {
    /// RFC 3339 en UTC.
    pub at: String,
    pub session: String,
    /// `claude-code 2.1.318`, `mcp:<nombre> <versión>`, `tty` o `script`.
    pub client: Option<String>,
    /// Id de la operación del catálogo (`service_control`, `logs_tail`…).
    pub tool: String,
    pub args: serde_json::Value,
    /// `dialog`, `elicitation`, `trusted`, `tty`…; `None` si no hizo falta o se denegó.
    pub confirmed_by: Option<String>,
    pub ok: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ms: u64,
    pub error: Option<String>,
}

fn dir_in(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("logs")
}

fn file_in(dir: &Path) -> PathBuf {
    dir.join("agent-audit.jsonl")
}

fn backup_in(dir: &Path) -> PathBuf {
    dir.join("agent-audit.jsonl.1")
}

/// Nunca falla hacia fuera: no poder auditar no debe romper la operación.
/// Los argumentos y el error pasan por [`redact`] antes de escribirse.
pub fn record(entry: &AuditEntry) {
    if let Ok(home) = paths::home() {
        let _ = record_in(&dir_in(&home), entry);
    }
}

/// Las más recientes primero.
pub fn recent(limit: usize) -> Vec<AuditEntry> {
    paths::home()
        .map(|home| recent_in(&dir_in(&home), limit))
        .unwrap_or_default()
}

/// La última llamada, venga de la CLI o de `elephant mcp`.
pub fn last_used() -> Option<AuditEntry> {
    recent(1).into_iter().next()
}

fn record_in(dir: &Path, entry: &AuditEntry) -> Result<(), String> {
    let mut entry = entry.clone();
    redact::mask_json(&mut entry.args);
    entry.error = entry.error.as_deref().map(redact::mask);

    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let file = file_in(dir);
    if fs::metadata(&file).is_ok_and(|meta| meta.len() >= MAX_BYTES) {
        // Si dos procesos rotan a la vez, uno de los renombres falla y no pasa nada.
        let _ = fs::rename(&file, backup_in(dir));
    }
    let mut line = serde_json::to_string(&entry).map_err(|error| error.to_string())?;
    line.push('\n');
    // O_APPEND con una sola escritura: las líneas de varios procesos no se mezclan.
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .and_then(|mut handle| handle.write_all(line.as_bytes()))
        .map_err(|error| error.to_string())
}

fn recent_in(dir: &Path, limit: usize) -> Vec<AuditEntry> {
    let mut entries = Vec::new();
    for path in [file_in(dir), backup_in(dir)] {
        if entries.len() >= limit {
            break;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        entries.extend(
            text.lines()
                .rev()
                .filter_map(|line| serde_json::from_str::<AuditEntry>(line).ok())
                .take(limit - entries.len()),
        );
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use serde_json::json;

    fn entry(tool: &str, args: serde_json::Value) -> AuditEntry {
        AuditEntry {
            at: "2026-11-06T10:21:04Z".into(),
            session: "s1".into(),
            client: Some("claude-code 2.1.318".into()),
            tool: tool.into(),
            args,
            confirmed_by: Some("dialog".into()),
            ok: true,
            ms: 1840,
            error: None,
        }
    }

    #[test]
    fn records_newest_first_and_masks_arguments() {
        let dir = TempDir::new("audit");
        record_in(dir.path(), &entry("service_control", json!({"name": "mysql"}))).unwrap();
        let mut failed = entry("logs_search", json!({"query": "password=hunter2"}));
        failed.ok = false;
        failed.error = Some("token=abc".into());
        record_in(dir.path(), &failed).unwrap();

        let recent = recent_in(dir.path(), 10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].tool, "logs_search");
        assert_eq!(recent[0].args["query"], "password=••••");
        assert_eq!(recent[0].error.as_deref(), Some("token=••••"));
        assert_eq!(recent[1].tool, "service_control");
        assert_eq!(recent_in(dir.path(), 1).len(), 1);
    }

    #[test]
    fn rotates_after_one_megabyte_and_keeps_reading_the_backup() {
        let dir = TempDir::new("audit-rotate");
        fs::write(file_in(dir.path()), "x".repeat(MAX_BYTES as usize)).unwrap();
        record_in(dir.path(), &entry("php_set_global", json!({}))).unwrap();
        assert!(backup_in(dir.path()).exists());
        assert!(fs::metadata(file_in(dir.path())).unwrap().len() < 1024);

        // La línea rota del respaldo se ignora; la nueva se lee.
        let recent = recent_in(dir.path(), 5);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].tool, "php_set_global");
    }

    #[test]
    fn missing_log_reads_as_empty() {
        let dir = TempDir::new("audit-empty");
        assert!(recent_in(dir.path(), 5).is_empty());
    }
}
