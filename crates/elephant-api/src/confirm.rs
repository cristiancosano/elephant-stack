//! Diálogo nativo de confirmación (03 §7.2, 05 §5.4): `osascript` con el
//! mensaje como argumento (nunca interpolado en el script), «Denegar» como
//! botón por defecto y 60 s antes de denegar solo.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Caller, Confirmation, Confirmer};

const SCRIPT: [&str; 3] = [
    "on run argv",
    "display dialog (item 1 of argv) with title \"Elephant Stack\" buttons {\"Denegar\", \"Permitir\"} default button \"Denegar\" cancel button \"Denegar\" with icon caution giving up after 60",
    "end run",
];

/// Margen sobre los 60 s del propio diálogo antes de matar `osascript`.
const HARD_TIMEOUT: Duration = Duration::from_secs(70);

pub const CANNOT_ASK: &str = "No se pudo pedir confirmación; cambia el modo en Ajustes > Agentes.";
pub const DENIED_BY_PERSON: &str =
    "La persona lo ha denegado en el diálogo de Elephant Stack. No lo intentes por otra vía.";

/// El diálogo de Elephant: funciona en cualquier modo del cliente.
#[derive(Debug, Clone)]
pub struct DialogConfirmer {
    osascript: PathBuf,
}

impl Default for DialogConfirmer {
    fn default() -> Self {
        Self {
            // ELEPHANT_STACK_OSASCRIPT solo existe en debug (tests).
            osascript: elephant_core::paths::tool_override("osascript")
                .unwrap_or_else(|| PathBuf::from("/usr/bin/osascript")),
        }
    }
}

impl DialogConfirmer {
    pub fn with_program(osascript: PathBuf) -> Self {
        Self { osascript }
    }
}

impl Confirmer for DialogConfirmer {
    fn confirm(&self, _caller: &Caller, text: &str) -> Confirmation {
        let mut command = Command::new(&self.osascript);
        for line in SCRIPT {
            command.arg("-e").arg(line);
        }
        command.arg("--").arg(text);
        match run_with_timeout(&mut command, HARD_TIMEOUT) {
            Some((true, stdout)) => interpret(&stdout),
            // Cancelar sale con error («User canceled», -128).
            Some((false, _)) => Confirmation::Denied { reason: DENIED_BY_PERSON.to_string() },
            None => Confirmation::Denied { reason: CANNOT_ASK.to_string() },
        }
    }
}

/// Solo «Permitir» sin agotar el tiempo permite la acción.
fn interpret(stdout: &str) -> Confirmation {
    let allowed = stdout.contains("button returned:Permitir") && !stdout.contains("gave up:true");
    if allowed {
        Confirmation::Allowed { by: "dialog".to_string() }
    } else {
        Confirmation::Denied { reason: DENIED_BY_PERSON.to_string() }
    }
}

/// `None` si no se pudo ejecutar, si no hay sesión gráfica (código distinto
/// de «cancelado») o si se pasa del tiempo. `Some((éxito, stdout))` si no.
fn run_with_timeout(command: &mut Command, timeout: Duration) -> Option<(bool, String)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    if status.success() {
        return Some((true, stdout));
    }
    // -128 es «User canceled»: la persona pulsó «Denegar». Cualquier otro error
    // (sin sesión gráfica, permisos…) significa que no se pudo preguntar.
    if stderr.contains("-128") {
        Some((false, stdout))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake(body: &str) -> (PathBuf, PathBuf) {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "elephant-api-osa-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("osascript");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    fn caller() -> Caller {
        Caller::Agent { client: "claude-code".into(), session: "s".into() }
    }

    #[test]
    fn only_permitir_allows() {
        assert_eq!(interpret("button returned:Permitir, gave up:false\n"), Confirmation::Allowed { by: "dialog".into() });
        assert!(matches!(interpret("button returned:, gave up:true"), Confirmation::Denied { .. }));
        assert!(matches!(interpret("button returned:Denegar"), Confirmation::Denied { .. }));
    }

    #[test]
    fn the_message_is_the_last_argument() {
        // Devuelve «Permitir» solo si el mensaje llega intacto como argumento.
        let (dir, path) = fake(
            "for last; do :; done\n[ \"$last\" = 'Claude Code quiere \"reiniciar\" mysql. ¿Permitir?' ] && echo 'button returned:Permitir, gave up:false'",
        );
        let confirmer = DialogConfirmer::with_program(path);
        let result = confirmer.confirm(&caller(), "Claude Code quiere \"reiniciar\" mysql. ¿Permitir?");
        assert_eq!(result, Confirmation::Allowed { by: "dialog".into() });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failures_deny_with_a_hint() {
        let (dir, path) = fake("echo 'execution error: No user interaction allowed. (-1713)' >&2; exit 1");
        let result = DialogConfirmer::with_program(path).confirm(&caller(), "x");
        assert_eq!(result, Confirmation::Denied { reason: CANNOT_ASK.into() });
        let missing = DialogConfirmer::with_program(dir.join("no-existe")).confirm(&caller(), "x");
        assert_eq!(missing, Confirmation::Denied { reason: CANNOT_ASK.into() });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cancel_is_a_denial_by_the_person() {
        let (dir, path) = fake("echo 'execution error: User canceled. (-128)' >&2; exit 1");
        let result = DialogConfirmer::with_program(path).confirm(&caller(), "x");
        assert_eq!(result, Confirmation::Denied { reason: DENIED_BY_PERSON.into() });
        let _ = std::fs::remove_dir_all(dir);
    }
}
