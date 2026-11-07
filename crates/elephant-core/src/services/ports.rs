//! Quién ocupa un puerto, para que el error de arranque diga qué hacer:
//! `lsof` ve los procesos de la cuenta (los servicios de brew corren con
//! ella) y, si el proceso es de un servicio de brew, se da la orden para
//! detenerlo. Los de otras cuentas se adivinan como en el asistente.

use std::process::{Command, Stdio};

use crate::launchd::{self, LaunchJob};
use crate::setup::detect;
use crate::setup::presets::{self, Engine};
use crate::tr;

/// Quién escucha en un puerto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Owner {
    /// «PostgreSQL 18 de Homebrew», «redis-server», «mysqld de otra-cuenta».
    pub who: String,
    /// La fórmula de brew, si es uno de sus servicios: `postgresql@18`.
    pub formula: Option<String>,
}

/// `p<pid>` y `c<orden>` de `lsof -F pc`: el primer proceso que escucha.
pub(crate) fn parse_lsof(text: &str) -> Option<(i64, String)> {
    let mut pid = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.trim().parse().ok();
        } else if let (Some(value), Some(pid)) = (line.strip_prefix('c'), pid) {
            return Some((pid, value.trim().to_string()));
        }
    }
    None
}

fn listener(port: u16) -> Option<(i64, String)> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-F", "pc"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    parse_lsof(&String::from_utf8_lossy(&output.stdout))
}

/// `sh.elephant.mysql.9.7` → «MySQL 9.7 de Elephant».
fn own_name(label: &str) -> Option<String> {
    let rest = label.strip_prefix(super::LABEL_PREFIX)?;
    let (id, branch) = rest.split_once('.').map_or((rest, None), |(id, branch)| (id, Some(branch)));
    let name = super::ServiceKind::parse(id)
        .map(super::ServiceKind::name)
        .or_else(|_| super::ToolKind::parse(id).map(super::ToolKind::name))
        .ok()?;
    let name = match branch {
        Some(branch) => format!("{name} {branch}"),
        None => name.to_string(),
    };
    Some(tr!("{name} de Elephant", "Elephant’s {name}"))
}

/// Con el proceso que escucha (si se ve), los servicios de brew y los de
/// Elephant cargados.
pub(crate) fn owner_from(
    listener: Option<(i64, String)>,
    brew: &[LaunchJob],
    own: &[LaunchJob],
    engine: Option<Engine>,
    others: &dyn Fn() -> Option<String>,
) -> Option<Owner> {
    let brew_owner = |job: &LaunchJob| {
        let formula = job.service_name()?.to_string();
        let name = presets::label(&formula);
        Some(Owner { who: tr!("{name} de Homebrew", "{name} from Homebrew"), formula: Some(formula) })
    };
    if let Some((pid, command)) = listener {
        if let Some(owner) = brew.iter().find(|job| job.pid == Some(pid)).and_then(brew_owner) {
            return Some(owner);
        }
        if let Some(who) = own.iter().find(|job| job.pid == Some(pid)).and_then(|job| own_name(&job.label)) {
            return Some(Owner { who, formula: None });
        }
        return Some(Owner { who: format!("{command} (pid {pid})"), formula: None });
    }
    // No se ve el proceso (es de otra cuenta): el servicio de brew del mismo
    // motor en marcha o, si no, una suposición por el nombre del proceso.
    let engine = engine?;
    brew.iter()
        .filter(|job| job.pid.is_some())
        .find(|job| job.service_name().is_some_and(|name| presets::engine(name) == engine))
        .and_then(brew_owner)
        .or_else(|| others().map(|who| Owner { who, formula: None }))
}

pub(crate) fn owner(port: u16, engine: Option<Engine>) -> Option<Owner> {
    let brew = launchd::homebrew_jobs().unwrap_or_default();
    let own = launchd::jobs_with_prefix(super::LABEL_PREFIX).unwrap_or_default();
    owner_from(listener(port), &brew, &own, engine, &|| {
        let user = std::env::var("USER").unwrap_or_default();
        detect::owner_hint(port, &[], &detect::process_table(), &user)
    })
}

pub(crate) fn busy_message(name: &str, port: u16, owner: Option<&Owner>) -> String {
    match owner {
        Some(Owner { who, formula: Some(formula) }) => tr!(
            "El puerto {port} lo usa {who}. Para encender {name} de Elephant, detenlo con «brew services stop {formula}» (sus datos no se tocan) y vuelve a intentarlo.",
            "Port {port} is used by {who}. To turn on Elephant’s {name}, stop it with “brew services stop {formula}” (its data isn’t touched) and try again."
        ),
        Some(Owner { who, formula: None }) => tr!(
            "El puerto {port} lo usa {who}. Detenlo y vuelve a intentarlo.",
            "Port {port} is used by {who}. Stop it and try again."
        ),
        None => tr!(
            "El puerto {port} ya está en uso. Detén lo que lo usa y vuelve a intentarlo.",
            "Port {port} is already in use. Stop whatever is using it and try again."
        ),
    }
}

/// El error de arranque con el puerto ocupado.
pub(crate) fn busy_error(name: &str, port: u16, engine: Option<Engine>) -> String {
    busy_message(name, port, owner(port, engine).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(label: &str, pid: Option<i64>) -> LaunchJob {
        LaunchJob { label: label.into(), pid, last_exit: None }
    }

    #[test]
    fn reads_the_listener_from_lsof() {
        assert_eq!(parse_lsof("p812\ncpostgres\np813\ncpostgres\n"), Some((812, "postgres".into())));
        assert_eq!(parse_lsof(""), None);
    }

    #[test]
    fn a_brew_service_is_named_with_the_command_to_stop_it() {
        let brew = [job("homebrew.mxcl.redis", Some(70)), job("homebrew.mxcl.postgresql@18", Some(812))];
        let owner = owner_from(Some((812, "postgres".into())), &brew, &[], Some(Engine::Postgres), &|| None).unwrap();
        assert_eq!(owner.who, "PostgreSQL 18 de Homebrew");
        assert_eq!(owner.formula.as_deref(), Some("postgresql@18"));
        let text = busy_message("PostgreSQL", 5432, Some(&owner));
        assert!(text.contains("brew services stop postgresql@18") && text.contains("5432"), "{text}");

        // Otro proceso de la cuenta: su nombre y su pid; uno de Elephant, por su nombre.
        let other = owner_from(Some((9, "node".into())), &brew, &[], Some(Engine::Redis), &|| None).unwrap();
        assert_eq!((other.who.as_str(), other.formula), ("node (pid 9)", None));
        let own = [job("sh.elephant.mysql.9.7", Some(66)), job("sh.elephant.phpmyadmin", Some(67))];
        let mine = owner_from(Some((66, "mysqld".into())), &brew, &own, Some(Engine::Mysql), &|| None).unwrap();
        assert_eq!(mine.who, "MySQL 9.7 de Elephant");
        let tool = owner_from(Some((67, "php".into())), &brew, &own, None, &|| None).unwrap();
        assert_eq!(tool.who, "phpMyAdmin de Elephant");

        // Sin ver el proceso: el servicio de brew del mismo motor o la suposición.
        let guessed = owner_from(None, &brew, &[], Some(Engine::Redis), &|| None).unwrap();
        assert_eq!(guessed.formula.as_deref(), Some("redis"));
        let hint = owner_from(None, &[], &[], Some(Engine::Mysql), &|| Some("mysqld de otra".into())).unwrap();
        assert_eq!(hint.who, "mysqld de otra");
        assert!(owner_from(None, &brew, &[], None, &|| None).is_none());
        assert!(busy_message("Redis", 6379, None).contains("6379"));
    }
}

#[cfg(test)]
mod system_tests {
    use super::*;

    /// Solo lee: `lsof`, `launchctl list` y `ps` de este Mac.
    ///
    /// `cargo test -p elephant-core real_port_owners -- --ignored --nocapture`
    #[test]
    #[ignore = "lee los puertos y servicios reales"]
    fn real_port_owners_are_named() {
        for (name, port, engine) in [
            ("MySQL", 3306, Engine::Mysql),
            ("PostgreSQL", 5432, Engine::Postgres),
            ("Redis", 6379, Engine::Redis),
            ("Mailpit", 1025, Engine::Mailpit),
            ("Mailpit", 8025, Engine::Mailpit),
        ] {
            if super::super::port_open(port) {
                println!("{port}: {}", busy_error(name, port, Some(engine)));
            } else {
                println!("{port}: libre");
            }
        }
    }
}
