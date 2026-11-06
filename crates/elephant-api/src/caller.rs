//! Quién llama (05 §5.2). Es agente si `CLAUDECODE` o `AI_AGENT` están
//! definidas (con cualquier valor) o si stdin no es un TTY. La detección solo
//! endurece: ninguna variable convierte a un agente en persona.

use crate::Caller;

/// `env` lee una variable (en la CLI, `std::env::var_os`); `stdin_tty` dice si
/// stdin es una terminal.
pub fn detect_caller(env: &dyn Fn(&str) -> Option<String>, stdin_tty: bool) -> Caller {
    let session = env("CLAUDE_CODE_SESSION_ID")
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(process_session);
    let claude = env("CLAUDECODE").is_some();
    let ai_agent = env("AI_AGENT");
    if claude || ai_agent.is_some() || !stdin_tty {
        let client = ai_agent
            .as_deref()
            .and_then(client_from_ai_agent)
            .unwrap_or_else(|| if claude { "claude-code".to_string() } else { "script".to_string() });
        Caller::Agent { client, session }
    } else {
        Caller::Person { session }
    }
}

/// `claude-code_2-1-318_agent` → `claude-code 2.1.318`. Otros valores se
/// conservan tal cual, recortados.
fn client_from_ai_agent(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split('_');
    let name = parts.next()?;
    match parts.next() {
        Some(version) if version.chars().all(|c| c.is_ascii_digit() || c == '-') && !version.is_empty() => {
            Some(format!("{name} {}", version.replace('-', ".")))
        }
        _ => Some(value.chars().take(64).collect()),
    }
}

/// Un identificador por proceso cuando el cliente no da uno.
fn process_session() -> String {
    format!("{:x}-{:x}", elephant_core::now_ms(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn claude_code_with_its_session() {
        let env = env_of(&[
            ("CLAUDECODE", "1"),
            ("AI_AGENT", "claude-code_2-1-318_agent"),
            ("CLAUDE_CODE_SESSION_ID", "abc-123"),
        ]);
        let caller = detect_caller(&env, false);
        assert_eq!(caller, Caller::Agent { client: "claude-code 2.1.318".into(), session: "abc-123".into() });
    }

    #[test]
    fn a_person_needs_a_tty_and_no_agent_variables() {
        assert!(!detect_caller(&env_of(&[]), true).is_agent());
        assert_eq!(detect_caller(&env_of(&[]), false).client(), "script");
        assert_eq!(detect_caller(&env_of(&[("CLAUDECODE", "1")]), true).client(), "claude-code");
    }

    #[test]
    fn variables_only_harden() {
        // CLAUDECODE=0 sigue siendo agente, y vaciar AI_AGENT sin TTY también.
        assert!(detect_caller(&env_of(&[("CLAUDECODE", "0")]), true).is_agent());
        assert!(detect_caller(&env_of(&[("AI_AGENT", "")]), true).is_agent());
        assert!(detect_caller(&env_of(&[("AI_AGENT", "")]), false).is_agent());
    }

    #[test]
    fn other_agents_keep_their_name() {
        let caller = detect_caller(&env_of(&[("AI_AGENT", "cursor")]), true);
        assert_eq!(caller.client(), "cursor");
        assert!(!caller.session().is_empty());
    }
}
