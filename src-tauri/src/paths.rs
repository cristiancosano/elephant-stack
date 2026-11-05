use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Directorios donde buscar herramientas.
///
/// Una app abierta desde Finder no hereda el PATH de la terminal, así que hay
/// que indicar a mano las rutas habituales de Homebrew, Herd y nvm.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();

    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".composer/vendor/bin"));
        dirs.push(home.join("Library/Application Support/Herd/bin"));
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
}
