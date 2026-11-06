//! `elephant logs watch` (solo CLI): una línea por cada entrada nueva, pensada
//! para la herramienta Monitor de Claude Code. Usa el directo del motor de
//! logs, que sondea los archivos solo mientras dura la orden, y termina al
//! cumplirse `--timeout`.

use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use elephant_logs::{LogHub, LogLevel, LogQuery, projects};

use crate::ops::logs::{EntryLine, entry_text};
use elephant_core::tr;

use crate::{UNTRUSTED_BEGIN, UNTRUSTED_END};

/// El plazo máximo de Monitor.
pub const MAX_TIMEOUT_SECS: u64 = 1800;

pub fn check_timeout(secs: u64) -> Result<Duration, String> {
    if secs == 0 || secs > MAX_TIMEOUT_SECS {
        Err(tr!("--timeout va de 1 a {MAX_TIMEOUT_SECS} segundos.", "--timeout goes from 1 to {MAX_TIMEOUT_SECS} seconds."))
    } else {
        Ok(Duration::from_secs(secs))
    }
}

/// Bloquea hasta `timeout`. `project` es un id, nombre o host.
pub fn watch(
    home: &Path,
    project: Option<&str>,
    errors: bool,
    timeout: Duration,
    out: &mut dyn Write,
) -> Result<(), String> {
    let mut q = LogQuery::default();
    if let Some(needle) = project {
        let all = projects::discover(home);
        let id = projects::resolve(&all, needle)
            .map(|p| p.id.clone())
            .ok_or_else(|| tr!("No conozco el proyecto «{needle}». Mira `elephant project list`.", "Unknown project “{needle}”. See `elephant project list`."))?;
        q.projects.push(id);
    }
    if errors {
        q.min_level = Some(LogLevel::Error);
    }

    let hub = LogHub::new(home.to_path_buf());
    let (sender, receiver) = mpsc::channel();
    let subscription = hub.subscribe(q, Box::new(move |batch| sender.send(batch.entries).is_ok()));
    let deadline = Instant::now() + timeout;
    let write = |out: &mut dyn Write, line: &str| -> Result<(), String> {
        writeln!(out, "{line}").and_then(|()| out.flush()).map_err(|error| error.to_string())
    };
    write(out, UNTRUSTED_BEGIN)?;
    let result = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Ok(());
        }
        match receiver.recv_timeout(left) {
            Ok(entries) => {
                let now = elephant_core::now_ms();
                let mut failed = None;
                for entry in entries {
                    let line = elephant_core::redact::mask(&entry_text(&EntryLine::from(entry), now));
                    if let Err(error) = write(out, &line) {
                        failed = Some(error);
                        break;
                    }
                }
                // Si nadie lee (Monitor terminó), se para.
                if let Some(error) = failed {
                    break Err(error);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break Ok(()),
            Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(()),
        }
    };
    hub.unsubscribe(subscription);
    let _ = write(out, UNTRUSTED_END);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_mandatory_and_bounded() {
        assert!(check_timeout(0).is_err());
        assert!(check_timeout(1801).is_err());
        assert_eq!(check_timeout(600).unwrap(), Duration::from_secs(600));
    }

    #[test]
    fn ends_when_the_timeout_expires() {
        let home = std::env::temp_dir().join(format!("elephant-api-watch-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let mut out = Vec::new();
        let start = Instant::now();
        watch(&home, None, true, Duration::from_millis(300), &mut out).unwrap();
        assert!(start.elapsed() < Duration::from_secs(3));
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, format!("{UNTRUSTED_BEGIN}\n{UNTRUSTED_END}\n"));
        let _ = std::fs::remove_dir_all(home);
    }
}
