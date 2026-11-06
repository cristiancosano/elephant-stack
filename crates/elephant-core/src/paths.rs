use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Carpeta personal del usuario.
pub fn home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| crate::tr!("HOME no está definido.", "HOME isn’t set."))
}

/// `~/.elephant-stack`: ajustes, versiones de PHP, tareas, cerrojos y logs.
pub fn data_dir() -> Result<PathBuf, String> {
    home().map(|home| data_dir_in(&home))
}

pub(crate) fn data_dir_in(home: &Path) -> PathBuf {
    home.join(".elephant-stack")
}

/// Directorios donde buscar herramientas.
///
/// Una app abierta desde Finder no hereda el PATH de la terminal, así que hay
/// que indicar a mano las rutas habituales de Homebrew, Herd y nvm. Las de
/// Elephant Stack van primero, como en la shell del usuario.
pub fn search_dirs() -> Vec<PathBuf> {
    search_dirs_in(env::var_os("HOME").map(PathBuf::from).as_deref())
}

fn search_dirs_in(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home {
        dirs.push(data_dir_in(home).join("bin"));
    }
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
        ]
        .iter()
        .map(PathBuf::from),
    );
    if let Some(home) = home {
        dirs.push(home.join(".composer/vendor/bin"));
        dirs.push(home.join("Library/Application Support/Herd/bin"));
        // El instalador nativo de Claude Code deja aquí `claude`.
        dirs.push(home.join(".local/bin"));
        dirs.extend(nvm_bin_dirs(&home.join(".nvm/versions/node")));
    }
    dirs.extend(["/usr/bin", "/bin", "/usr/sbin", "/sbin"].iter().map(PathBuf::from));
    dirs
}

/// PATH para los procesos hijos, con el mismo orden de búsqueda.
pub fn child_path() -> OsString {
    env::join_paths(search_dirs()).unwrap_or_default()
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Etiqueta de la segunda línea de los marcadores que crea `src-tauri/build.rs`
/// en debug cuando falta un sidecar (C21). `tauri-build` los copia junto al
/// ejecutable como si fueran el binario de verdad.
pub const SIDECAR_MARKER: &str = "# elephant-stack: marcador de sidecar";

/// `true` si `path` es uno de esos marcadores: un sidecar que no sirve.
pub fn is_sidecar_marker(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 128];
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let n = file.read(&mut head).unwrap_or(0);
    String::from_utf8_lossy(&head[..n]).contains(SIDECAR_MARKER)
}

/// Un sidecar que se puede ejecutar: existe y no es un marcador.
pub fn is_usable_sidecar(path: &Path) -> bool {
    path.is_file() && !is_sidecar_marker(path)
}

/// Ejecutables que los tests pueden sustituir por uno falso.
const OVERRIDABLE: [&str; 5] = ["BREW", "CURL", "OSASCRIPT", "COMPOSER", "CLAUDE"];

/// Solo en debug: `ELEPHANT_STACK_<NAME>` (BREW, CURL, OSASCRIPT, COMPOSER,
/// CLAUDE) sustituye a la herramienta real. En release siempre es `None`.
pub fn tool_override(name: &str) -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let name = name.to_ascii_uppercase();
    if !OVERRIDABLE.contains(&name.as_str()) {
        return None;
    }
    env::var_os(format!("ELEPHANT_STACK_{name}"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Carpetas `bin` de nvm, de la versión de Node más reciente a la más antigua.
fn nvm_bin_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut versions: Vec<(Vec<u32>, PathBuf)> = entries
        .flatten()
        .map(|entry| {
            let key = version_key(&entry.file_name().to_string_lossy());
            (key, entry.path().join("bin"))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, path)| path).collect()
}

fn version_key(name: &str) -> Vec<u32> {
    name.trim_start_matches('v')
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_versions_numerically() {
        assert!(version_key("v24.18.0") > version_key("v9.11.2"));
    }

    #[test]
    fn elephant_bin_comes_first_and_local_bin_is_searched() {
        let home = Path::new("/Users/demo");
        let dirs = search_dirs_in(Some(home));
        assert_eq!(dirs[0], home.join(".elephant-stack/bin"));
        assert!(dirs.contains(&home.join(".local/bin")));
        let herd = dirs.iter().position(|d| d.ends_with("Herd/bin")).unwrap();
        let brew = dirs.iter().position(|d| d == Path::new("/opt/homebrew/bin")).unwrap();
        assert!(brew < herd);
        assert_eq!(dirs.last().unwrap(), Path::new("/sbin"));
    }

    #[test]
    fn without_home_only_system_dirs_remain() {
        let dirs = search_dirs_in(None);
        assert_eq!(dirs[0], Path::new("/opt/homebrew/bin"));
    }

    #[test]
    fn only_known_tools_can_be_overridden() {
        assert_eq!(tool_override("php"), None);
        assert_eq!(tool_override("../BREW"), None);
    }

    #[test]
    fn sidecar_markers_are_not_usable() {
        let dir = crate::testutil::TempDir::new("sidecar-marker");
        let marker = dir.path().join("cli");
        fs::write(&marker, format!("#!/bin/sh\n{SIDECAR_MARKER}\nexit 1\n")).unwrap();
        let real = dir.path().join("elephant");
        fs::write(&real, b"\xcf\xfa\xed\xfe binario").unwrap();
        assert!(is_sidecar_marker(&marker));
        assert!(!is_usable_sidecar(&marker));
        assert!(is_usable_sidecar(&real));
        assert!(!is_usable_sidecar(&dir.path().join("no-existe")));
    }
}
