//! Localiza las versiones de PHP y de Node y decide cuál usar. Lo comparten
//! los selectores de `~/.elephant-stack/bin` y la app, para que ambos lleguen
//! siempre a la misma respuesta.

use std::fs;
use std::path::{Path, PathBuf};

use elephant_i18n::tr;

pub mod node;
pub mod selectors;

pub const PROJECT_FILE: &str = ".php-version";
const HERD: &str = "Herd";

#[derive(Debug, Clone, PartialEq)]
pub struct PhpVersion {
    /// `8.4` o `8.4.23`.
    pub id: String,
    pub source: &'static str,
    pub php: PathBuf,
}

impl PhpVersion {
    /// Identifica exactamente esta instalación. El `8.4` de Herd necesita indicar
    /// su origen porque, sin él, `8.4` significa «la 8.4.x más reciente».
    pub fn key(&self) -> String {
        if self.source == HERD {
            format!("{}@{HERD}", self.id)
        } else {
            self.id.clone()
        }
    }
}

/// Rutas donde viven las versiones y los ajustes.
#[derive(Debug, Clone)]
pub struct Layout {
    pub data_dir: PathBuf,
    pub herd_bin: PathBuf,
}

impl Layout {
    pub fn from_home(home: &Path) -> Self {
        Self {
            data_dir: home.join(".elephant-stack"),
            herd_bin: home.join("Library/Application Support/Herd/bin"),
        }
    }

    fn global_file(&self) -> PathBuf {
        self.data_dir.join("global-php")
    }

    pub fn shim_path(&self) -> PathBuf {
        self.data_dir.join("bin/php")
    }

    /// Versiones disponibles, de la más reciente a la más antigua. A igual
    /// versión gana la de Elephant Stack.
    pub fn discover(&self) -> Vec<PhpVersion> {
        let mut versions = self.own_versions();
        versions.extend(self.herd_versions());
        versions.sort_by_key(|version| std::cmp::Reverse(numeric_key(&version.id)));
        versions
    }

    fn own_versions(&self) -> Vec<PhpVersion> {
        let Ok(entries) = fs::read_dir(self.data_dir.join("php")) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let php = entry.path().join("php");
                php.is_file().then(|| PhpVersion {
                    id: entry.file_name().to_string_lossy().into_owned(),
                    source: "Elephant Stack",
                    php,
                })
            })
            .collect()
    }

    fn herd_versions(&self) -> Vec<PhpVersion> {
        let Ok(entries) = fs::read_dir(&self.herd_bin) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let id = herd_binary_version(&name.to_string_lossy())?;
                Some(PhpVersion {
                    id,
                    source: HERD,
                    php: entry.path(),
                })
            })
            .collect()
    }

    pub fn global(&self) -> Option<String> {
        read_version_file(&self.global_file())
    }

    pub fn set_global(&self, requested: &str) -> Result<(), String> {
        self.ensure_available(requested)?;
        fs::create_dir_all(&self.data_dir).map_err(|error| error.to_string())?;
        fs::write(self.global_file(), format!("{}\n", requested.trim()))
            .map_err(|error| error.to_string())
    }

    pub fn set_project(&self, dir: &Path, requested: &str) -> Result<(), String> {
        self.ensure_available(requested)?;
        if !dir.is_dir() {
            let dir = dir.display();
            return Err(tr!("No existe la carpeta {dir}.", "The folder {dir} doesn’t exist."));
        }
        fs::write(dir.join(PROJECT_FILE), format!("{}\n", requested.trim()))
            .map_err(|error| error.to_string())
    }

    fn ensure_available(&self, requested: &str) -> Result<(), String> {
        self.find(requested)
            .map(|_| ())
            .ok_or_else(|| self.not_found(requested, "Elephant Stack"))
    }

    fn find(&self, requested: &str) -> Option<PhpVersion> {
        self.discover()
            .into_iter()
            .find(|version| match requested.trim().split_once('@') {
                Some((id, source)) => version.id == id && version.source == source,
                None => matches_version(requested, &version.id),
            })
    }

    fn not_found(&self, requested: &str, origin: &str) -> String {
        let available: Vec<String> = self.discover().into_iter().map(|v| v.id).collect();
        let requested = requested.trim();
        let list = if available.is_empty() { tr!("ninguna", "none") } else { available.join(", ") };
        tr!(
            "Elephant Stack: PHP {requested} no está disponible (pedido por {origin}). Disponibles: {list}.",
            "Elephant Stack: PHP {requested} isn’t available (requested by {origin}). Available: {list}."
        )
    }

    /// Versión para una carpeta: `.php-version` del proyecto, después la global
    /// y, si no hay ninguna elegida, la más reciente.
    pub fn resolve(&self, cwd: &Path) -> Result<PhpVersion, String> {
        if let Some((file, requested)) = find_project_version(cwd) {
            return self
                .find(&requested)
                .ok_or_else(|| self.not_found(&requested, &file.display().to_string()));
        }
        if let Some(requested) = self.global() {
            return self
                .find(&requested)
                .ok_or_else(|| self.not_found(&requested, &tr!("el ajuste global", "the global setting")));
        }
        self.discover().into_iter().next().ok_or_else(|| {
            tr!(
                "Elephant Stack: no hay ninguna versión de PHP disponible.",
                "Elephant Stack: no PHP version is available."
            )
        })
    }
}

/// `php84` → `8.4`. Descarta `php`, `php84-fpm` y similares.
fn herd_binary_version(name: &str) -> Option<String> {
    let digits = name.strip_prefix("php")?;
    if digits.len() < 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (major, minor) = digits.split_at(1);
    Some(format!("{major}.{minor}"))
}

pub(crate) fn numeric_key(id: &str) -> Vec<u32> {
    id.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

/// `8.4` coincide con `8.4` y `8.4.23`, pero no con `8.40`. También admite `84`.
fn matches_version(requested: &str, id: &str) -> bool {
    let requested = requested.trim().trim_start_matches("php");
    let normalized = if !requested.contains('.') && requested.len() >= 2 {
        let (major, minor) = requested.split_at(1);
        format!("{major}.{minor}")
    } else {
        requested.to_string()
    };
    if normalized.is_empty() {
        return false;
    }
    let wanted: Vec<&str> = normalized.split('.').collect();
    let have: Vec<&str> = id.split('.').collect();
    // Un `8.4.23` pedido también lo sirve el binario `8.4` de Herd.
    wanted.iter().zip(&have).all(|(a, b)| a == b)
}

pub(crate) fn read_version_file(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let version = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(version.to_string())
}

fn find_project_version(cwd: &Path) -> Option<(PathBuf, String)> {
    cwd.ancestors().find_map(|dir| {
        let file = dir.join(PROJECT_FILE);
        let version = read_version_file(&file)?;
        Some((file, version))
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// Carpeta temporal con un HOME falso: nunca toca el real.
    pub(crate) struct Sandbox {
        pub root: PathBuf,
        pub layout: Layout,
    }

    impl Sandbox {
        pub fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "elephant-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&root).unwrap();
            let layout = Layout::from_home(&root);
            Self { root, layout }
        }

        fn herd(&self, name: &str) {
            fs::create_dir_all(&self.layout.herd_bin).unwrap();
            fs::write(self.layout.herd_bin.join(name), "").unwrap();
        }

        fn own(&self, id: &str) {
            let dir = self.layout.data_dir.join("php").join(id);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("php"), "").unwrap();
        }

        /// `node/<id>/bin/{node,npm,npx}`; `lts` escribe la marca (`Some("")`: no es LTS).
        pub fn node(&self, id: &str, lts: Option<&str>) {
            let dir = self.layout.node_root().join(id);
            fs::create_dir_all(dir.join("bin")).unwrap();
            for name in ["node", "npm", "npx"] {
                fs::write(dir.join("bin").join(name), "").unwrap();
            }
            if let Some(lts) = lts {
                fs::write(dir.join(node::LTS_MARKER), lts).unwrap();
            }
        }

        pub fn project(&self, relative: &str, version: Option<&str>) -> PathBuf {
            let dir = self.root.join(relative);
            fs::create_dir_all(&dir).unwrap();
            if let Some(version) = version {
                fs::write(dir.join(PROJECT_FILE), format!("{version}\n")).unwrap();
            }
            dir
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn discovers_herd_binaries_and_ignores_the_rest() {
        let sandbox = Sandbox::new();
        for name in ["php", "php84", "php84-fpm", "php83", "composer"] {
            sandbox.herd(name);
        }
        let ids: Vec<String> = sandbox.layout.discover().into_iter().map(|v| v.id).collect();
        assert_eq!(ids, ["8.4", "8.3"]);
    }

    #[test]
    fn sorts_numerically_across_sources() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        sandbox.own("8.10.1");
        sandbox.own("8.5.8");
        let ids: Vec<String> = sandbox.layout.discover().into_iter().map(|v| v.id).collect();
        assert_eq!(ids, ["8.10.1", "8.5.8", "8.4"]);
    }

    #[test]
    fn own_build_wins_over_herd_for_same_minor_when_newer() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        sandbox.own("8.4.23");
        let cwd = sandbox.project("sitio", Some("8.4"));
        assert_eq!(sandbox.layout.resolve(&cwd).unwrap().source, "Elephant Stack");
    }

    #[test]
    fn every_key_selects_exactly_its_own_installation() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        sandbox.own("8.4.27");
        let cwd = sandbox.project("sitio", None);
        for version in sandbox.layout.discover() {
            sandbox.layout.set_global(&version.key()).unwrap();
            assert_eq!(sandbox.layout.resolve(&cwd).unwrap(), version);
        }
    }

    #[test]
    fn source_suffix_never_matches_another_source() {
        let sandbox = Sandbox::new();
        sandbox.own("8.4");
        assert!(sandbox.layout.set_global("8.4@Herd").is_err());
    }

    #[test]
    fn matching_respects_version_boundaries() {
        assert!(matches_version("8.4", "8.4.23"));
        assert!(matches_version("8.4", "8.4"));
        assert!(matches_version("84", "8.4"));
        assert!(matches_version("php84", "8.4"));
        assert!(matches_version("8.4.23", "8.4"));
        assert!(!matches_version("8.4", "8.40.1"));
        assert!(!matches_version("8.4", "8.5.1"));
        assert!(!matches_version("", "8.4"));
    }

    #[test]
    fn project_file_beats_global_and_applies_to_subfolders() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        sandbox.herd("php83");
        sandbox.layout.set_global("8.4").unwrap();
        sandbox.project("sitio", Some("8.3"));
        let nested = sandbox.project("sitio/app/Models", None);
        assert_eq!(sandbox.layout.resolve(&nested).unwrap().id, "8.3");
    }

    #[test]
    fn falls_back_to_global_then_to_newest() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        sandbox.herd("php83");
        let cwd = sandbox.project("sin-version", None);
        assert_eq!(sandbox.layout.resolve(&cwd).unwrap().id, "8.4");
        sandbox.layout.set_global("8.3").unwrap();
        assert_eq!(sandbox.layout.resolve(&cwd).unwrap().id, "8.3");
    }

    #[test]
    fn missing_version_is_an_error_not_a_silent_fallback() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        let cwd = sandbox.project("viejo", Some("7.4"));
        let error = sandbox.layout.resolve(&cwd).unwrap_err();
        assert!(error.contains("7.4"));
        assert!(error.contains("8.4"));
    }

    #[test]
    fn setters_reject_unknown_versions() {
        let sandbox = Sandbox::new();
        sandbox.herd("php84");
        let cwd = sandbox.project("sitio", None);
        assert!(sandbox.layout.set_global("9.9").is_err());
        assert!(sandbox.layout.set_project(&cwd, "9.9").is_err());
        assert!(!cwd.join(PROJECT_FILE).exists());

        sandbox.layout.set_project(&cwd, "8.4").unwrap();
        assert_eq!(fs::read_to_string(cwd.join(PROJECT_FILE)).unwrap(), "8.4\n");
    }
}
