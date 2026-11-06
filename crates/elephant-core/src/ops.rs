//! Mutaciones de alto nivel: la única puerta que usan la app, el menú de la
//! barra, la CLI y las tareas de los agentes. Las que llaman a Homebrew toman
//! aquí el cerrojo (C15) y todas avisan con [`stamp::touch`] al terminar bien.

use std::path::{Path, PathBuf};

use elephant_php::Layout;

use crate::brew::{self, Operation, Service};
use crate::{components, launchd, lock, paths, php, services, setup, stamp, validate};

pub fn service_control(name: &str, op: Operation) -> Result<Vec<Service>, String> {
    let lock = lock::brew()?;
    let services = brew::apply(&lock, name, op)?;
    stamp::touch("services");
    if matches!(op, Operation::TurnOn | Operation::Restart | Operation::TurnOnWithAutostart) {
        verify_brew_start(name)?;
    }
    Ok(services)
}

/// Tras arrancar un servicio de brew, durante unos segundos su proceso tiene
/// que seguir vivo. Si termina (también con código 0, como `mysqld_safe`
/// cuando no puede usar sus datos), es un error con la explicación que haya.
fn verify_brew_start(name: &str) -> Result<(), String> {
    use std::time::{Duration, Instant};
    let label = format!("homebrew.mxcl.{name}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let job = launchd::homebrew_jobs()?.into_iter().find(|job| job.label == label);
        // Sin puerto que mirar: basta con que el proceso siga vivo.
        match services::probe(job.as_ref(), true) {
            services::Probe::Exited(code) => return Err(brew_start_error(name, code)),
            _ if Instant::now() >= deadline => return Ok(()),
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

fn brew_start_error(name: &str, code: Option<i64>) -> String {
    let code = code.map_or_else(|| "?".to_string(), |c| c.to_string());
    let prefix = Path::new("/opt/homebrew/var");
    let mut detail = brew_log_line(prefix, name).map(|line| format!(" {line}")).unwrap_or_default();
    if (name == "mysql" || name.starts_with("mysql@"))
        && let Err(owner) = services::check_owner(&prefix.join("mysql"))
    {
        detail = format!(" {owner}");
    }
    crate::tr!(
        "{name} se ha detenido justo después de arrancar (código {code}).{detail}",
        "{name} stopped right after starting (code {code}).{detail}"
    )
}

/// La última línea útil del log de un servicio de brew, si se puede leer.
fn brew_log_line(var: &Path, name: &str) -> Option<String> {
    let base = name.split('@').next().unwrap_or(name);
    let mut candidates = vec![var.join(format!("log/{name}.log")), var.join(format!("log/{base}.log"))];
    if base == "mysql"
        && let Ok(entries) = std::fs::read_dir(var.join("mysql"))
    {
        candidates.extend(
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "err")),
        );
    }
    candidates.into_iter().find_map(|path| {
        let text = std::fs::read_to_string(path).ok()?;
        let tail: String = text.chars().rev().take(16 * 1024).collect::<Vec<_>>().into_iter().rev().collect();
        services::last_useful_line(&tail)
    })
}

/// Reiniciar o detener todos los servicios que están en marcha, con un solo
/// cerrojo. Sigue con los demás si uno falla y devuelve el primer error.
pub fn services_bulk(op: Operation) -> Result<Vec<Service>, String> {
    check_bulk(op)?;
    let lock = lock::brew()?;
    let running: Vec<String> = brew::list_services()?
        .into_iter()
        .filter(|service| service.running)
        .map(|service| service.name)
        .collect();

    let mut errors = Vec::new();
    for name in &running {
        if let Err(error) = brew::apply(&lock, name, op) {
            errors.push(format!("{name}: {error}"));
        }
    }
    if errors.len() < running.len() {
        stamp::touch("services");
    }
    match errors.into_iter().next() {
        Some(error) => Err(error),
        None => brew::list_services(),
    }
}

fn check_bulk(op: Operation) -> Result<(), String> {
    match op {
        Operation::Restart | Operation::TurnOff => Ok(()),
        _ => Err(crate::tr!(
            "Solo se pueden reiniciar o detener todos los servicios a la vez.",
            "Only restart or stop can be applied to all services at once."
        )),
    }
}

pub fn set_global_php(key: &str) -> Result<php::PhpStatus, String> {
    set_global_php_in(&paths::home()?, key)
}

pub fn set_project_php(dir: &Path, key: &str) -> Result<(), String> {
    set_project_php_in(&paths::home()?, dir, key)
}

/// Sustituye una versión propia de PHP por otra más reciente de su rama, ya
/// instalada: ver [`php::replace_own`].
pub fn replace_php(
    old: &str,
    new: &str,
    project_roots: &[PathBuf],
) -> Result<php::PhpReplaceReport, String> {
    let home = paths::home()?;
    let report = php::replace_own(&Layout::from_home(&home), old, new, project_roots)?;
    let _ = stamp::touch_in(&home, "php", std::process::id());
    Ok(report)
}

/// `composer` lo instala el asistente (phar propio, 04 §4.5); el resto,
/// Homebrew, solo si está en la tabla de componentes instalables.
pub fn install_component(id: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    if id == "composer" {
        setup::install_composer(on_line)?;
    } else {
        let lock = lock::brew()?;
        components::install(&lock, id, on_line)?;
    }
    stamp::touch("components");
    Ok(())
}

fn set_global_php_in(home: &Path, key: &str) -> Result<php::PhpStatus, String> {
    let layout = Layout::from_home(home);
    layout.set_global(key)?;
    let _ = stamp::touch_in(home, "php", std::process::id());
    Ok(php::status_of(&layout))
}

fn set_project_php_in(home: &Path, dir: &Path, key: &str) -> Result<(), String> {
    let dir = validate::user_dir_in(home, dir)?;
    Layout::from_home(home).set_project(&dir, key)?;
    let _ = stamp::touch_in(home, "php", std::process::id());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, write_script};
    use std::fs;

    /// Un HOME con una versión propia de PHP (8.4.27).
    fn home_with_php() -> TempDir {
        let home = TempDir::new("ops");
        let php_dir = home.path().join(".elephant-stack/php/8.4.27");
        fs::create_dir_all(&php_dir).unwrap();
        write_script(&php_dir.join("php"), "echo 8.4.27");
        home
    }

    #[test]
    fn brew_logs_give_the_last_useful_line() {
        let var = TempDir::new("ops-brew-log");
        fs::create_dir_all(var.path().join("log")).unwrap();
        fs::write(var.path().join("log/redis.log"), "1:M 06 Nov arrancando\n1:M 06 Nov # Fallo al abrir el puerto\n").unwrap();
        assert_eq!(brew_log_line(var.path(), "redis").as_deref(), Some("1:M 06 Nov # Fallo al abrir el puerto"));
        fs::create_dir_all(var.path().join("mysql")).unwrap();
        fs::write(
            var.path().join("mysql/host.err"),
            "2026-11-06T10:00:00.000000Z 0 [ERROR] [MY-010273] [Server] No se puede crear el socket\n",
        )
        .unwrap();
        assert_eq!(
            brew_log_line(var.path(), "mysql").as_deref(),
            Some("[ERROR] [MY-010273] [Server] No se puede crear el socket")
        );
        assert_eq!(brew_log_line(var.path(), "nginx"), None);
    }

    #[test]
    fn bulk_operations_are_only_restart_or_turn_off() {
        assert!(check_bulk(Operation::Restart).is_ok());
        assert!(check_bulk(Operation::TurnOff).is_ok());
        for op in [Operation::TurnOn, Operation::EnableAutostart, Operation::TurnOnWithAutostart] {
            assert!(services_bulk(op).is_err());
        }
    }

    #[test]
    fn setting_the_global_php_stamps_the_change() {
        let home = home_with_php();
        let status = set_global_php_in(home.path(), "8.4").unwrap();
        assert_eq!(status.active.unwrap().id, "8.4.27");
        assert_eq!(stamp::read_in(home.path()).unwrap().what, "php");
    }

    #[test]
    fn an_unknown_version_changes_nothing() {
        let home = home_with_php();
        assert!(set_global_php_in(home.path(), "7.4").is_err());
        assert!(stamp::read_in(home.path()).is_none());
    }

    #[test]
    fn project_php_only_inside_home() {
        let home = home_with_php();
        let project = home.path().join("Code/shop");
        fs::create_dir_all(&project).unwrap();
        set_project_php_in(home.path(), &project, "8.4").unwrap();
        assert_eq!(fs::read_to_string(project.join(".php-version")).unwrap(), "8.4\n");

        let private = home.path().join(".elephant-stack/php");
        assert!(set_project_php_in(home.path(), &private, "8.4").is_err());
        assert!(!private.join(".php-version").exists());
    }
}
