//! Binarios que viajan con la app (C21): el selector `php` y la CLI `elephant`.
//! En el bundle están junto al ejecutable (`Contents/MacOS/<nombre>`).

use std::path::{Path, PathBuf};

/// `php-selector` o `cli`. En debug, si falta junto al ejecutable, se usa el
/// binario que deja cargo en `target/<perfil>/` (`php` o `elephant`).
// Lo usan el asistente (P04) y el canal de agentes (P09).
#[allow(dead_code)]
pub fn path(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    candidates(exe.parent()?, name, cfg!(debug_assertions))
        .into_iter()
        .find(|path| path.is_file())
}

fn candidates(exe_dir: &Path, name: &str, debug: bool) -> Vec<PathBuf> {
    let cargo_name = match name {
        "php-selector" => "php",
        "cli" => "elephant",
        _ => return Vec::new(),
    };
    let mut candidates = vec![exe_dir.join(name)];
    if debug {
        // El ejecutable de la app ya está en target/<perfil>/.
        candidates.push(exe_dir.join(cargo_name));
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_first_and_cargo_output_only_in_debug() {
        let dir = Path::new("/x/target/debug");
        assert_eq!(candidates(dir, "cli", true), [dir.join("cli"), dir.join("elephant")]);
        assert_eq!(candidates(dir, "php-selector", false), [dir.join("php-selector")]);
        assert!(candidates(dir, "../../bin/sh", true).is_empty());
    }
}
