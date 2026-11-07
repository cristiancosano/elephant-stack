//! Los selectores de `~/.elephant-stack/bin/`: un único binario (`php`) y
//! enlaces simbólicos a él con otros nombres (`node`, `npm`, `composer`,
//! `pnpm`…). El binario decide qué hacer por el nombre con el que se le
//! llama (`argv[0]`), así que actualizar `php` actualiza todos.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use elephant_i18n::tr;

use crate::{Layout, numeric_key};

/// A dónde apuntan los enlaces: el binario `php`, en la misma carpeta.
pub const TARGET: &str = "php";
/// Nombres que nunca son de Node: el propio binario, Composer y la orden `elephant`.
pub const RESERVED: [&str; 4] = ["php", "php-selector", "composer", "elephant"];

/// Qué hace el binario según su nombre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Php,
    Composer,
    /// `node`, `npm`, `npx`, `corepack` o un paquete global (`pnpm`).
    Node(String),
}

pub fn role_of(argv0: &str) -> Role {
    let name = Path::new(argv0).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match name.as_str() {
        "" | "php" | "php-selector" => Role::Php,
        "composer" => Role::Composer,
        _ => Role::Node(name),
    }
}

/// Un nombre que se puede usar en `bin/` sin riesgo.
fn is_selector_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && !RESERVED.contains(&name)
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-+@".contains(&b))
}

/// Resultado de regenerar los selectores.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reshim {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Ya hay en `bin/` un archivo con ese nombre que no es de Elephant: no se toca.
    pub skipped: Vec<String>,
}

impl Layout {
    pub fn bin_dir(&self) -> PathBuf {
        self.data_dir.join("bin")
    }

    /// Los ejecutables de `bin/` de todas las versiones de Node instaladas.
    pub fn node_executables(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for version in self.node_versions() {
            let Ok(entries) = fs::read_dir(version.bin()) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_selector_name(&name) && entry.path().is_file() {
                    names.insert(name);
                }
            }
        }
        names
    }

    /// `true` si `path` es un enlace de Elephant (apunta a [`TARGET`]).
    fn is_our_link(path: &Path) -> bool {
        fs::read_link(path).is_ok_and(|target| target == Path::new(TARGET))
    }

    /// Crea `bin/<nombre>` → `php`. `Ok(false)` si ya existía otro archivo.
    pub fn link_selector(&self, name: &str) -> Result<bool, String> {
        let path = self.bin_dir().join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => Ok(Self::is_our_link(&path)),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir_all(self.bin_dir()).map_err(|error| error.to_string())?;
                std::os::unix::fs::symlink(TARGET, &path).map_err(|error| {
                    let path = path.display();
                    tr!("No se pudo crear {path}: {error}", "Couldn’t create {path}: {error}")
                })?;
                Ok(true)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// Quita `bin/<nombre>` solo si es un enlace de Elephant.
    pub fn unlink_selector(&self, name: &str) -> Result<bool, String> {
        let path = self.bin_dir().join(name);
        if !Self::is_our_link(&path) {
            return Ok(false);
        }
        fs::remove_file(&path).map_err(|error| error.to_string())?;
        Ok(true)
    }

    /// Un selector en `bin/` por cada ejecutable de las versiones de Node
    /// (`node`, `npm`, `npx`, `corepack` y los paquetes globales), y quita los
    /// que ya no tiene ninguna. Nunca toca un archivo que no sea un enlace de
    /// Elephant.
    pub fn reshim(&self) -> Result<Reshim, String> {
        let wanted = self.node_executables();
        let mut report = Reshim::default();
        if let Ok(entries) = fs::read_dir(self.bin_dir()) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_selector_name(&name) && !wanted.contains(&name) && self.unlink_selector(&name)? {
                    report.removed.push(name);
                }
            }
        }
        for name in wanted {
            let path = self.bin_dir().join(&name);
            let existed = fs::symlink_metadata(&path).is_ok();
            if self.link_selector(&name)? {
                if !existed {
                    report.added.push(name);
                }
            } else {
                report.skipped.push(name);
            }
        }
        report.removed.sort();
        Ok(report)
    }

    pub fn composer_root(&self) -> PathBuf {
        self.data_dir.join("tools/composer")
    }

    /// El `composer.phar` de Elephant: la versión más reciente de
    /// `tools/composer/<versión>/` o, si no hay, el que dejaba el asistente
    /// antes en `libexec/`. Con su versión, si se sabe.
    pub fn composer_phar(&self) -> Option<(Option<String>, PathBuf)> {
        let mut own: Vec<(String, PathBuf)> = fs::read_dir(self.composer_root())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let id = entry.file_name().to_string_lossy().into_owned();
                let phar = entry.path().join("composer.phar");
                (!id.starts_with('.') && phar.is_file()).then_some((id, phar))
            })
            .collect();
        own.sort_by_key(|(id, _)| std::cmp::Reverse(numeric_key(id)));
        if let Some((id, phar)) = own.into_iter().next() {
            return Some((Some(id), phar));
        }
        let legacy = self.legacy_composer_phar();
        legacy.is_file().then_some((None, legacy))
    }

    pub fn legacy_composer_phar(&self) -> PathBuf {
        self.data_dir.join("libexec/composer.phar")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Sandbox;

    #[test]
    fn the_name_decides_the_role() {
        assert_eq!(role_of("php"), Role::Php);
        assert_eq!(role_of("/Users/a/.elephant-stack/bin/php"), Role::Php);
        assert_eq!(role_of("composer"), Role::Composer);
        assert_eq!(role_of("/x/bin/node"), Role::Node("node".into()));
        assert_eq!(role_of("pnpm"), Role::Node("pnpm".into()));
    }

    #[test]
    fn reshim_links_every_executable_and_never_touches_foreign_files() {
        let sandbox = Sandbox::new();
        sandbox.node("24.11.1", Some("krypton"));
        sandbox.node("22.21.0", Some("jod"));
        let bin = |version: &str| sandbox.layout.node_root().join(version).join("bin");
        fs::write(bin("22.21.0").join("pnpm"), "").unwrap();
        // Lo que ya hay en bin/: el binario, la orden elephant y un `npx` ajeno.
        let ours = sandbox.layout.bin_dir();
        fs::create_dir_all(&ours).unwrap();
        fs::write(ours.join("php"), "").unwrap();
        fs::write(ours.join("npx"), "ajeno").unwrap();
        std::os::unix::fs::symlink("/otro/sitio", ours.join("elephant")).unwrap();

        let report = sandbox.layout.reshim().unwrap();
        assert_eq!(report.added, ["node", "npm", "pnpm"]);
        assert_eq!(report.skipped, ["npx"]);
        assert_eq!(fs::read_link(ours.join("pnpm")).unwrap(), Path::new("php"));
        assert_eq!(fs::read_to_string(ours.join("npx")).unwrap(), "ajeno");
        assert_eq!(fs::read_link(ours.join("elephant")).unwrap(), Path::new("/otro/sitio"));

        // Sin la versión que traía pnpm, su selector se va; los demás se quedan.
        fs::remove_dir_all(sandbox.layout.node_root().join("22.21.0")).unwrap();
        let report = sandbox.layout.reshim().unwrap();
        assert_eq!(report.removed, ["pnpm"]);
        assert!(report.added.is_empty());
        assert!(ours.join("node").symlink_metadata().is_ok());
        assert!(fs::symlink_metadata(ours.join("pnpm")).is_err());
    }

    #[test]
    fn composer_prefers_the_newest_own_phar_then_the_legacy_one() {
        let sandbox = Sandbox::new();
        assert_eq!(sandbox.layout.composer_phar(), None);
        let legacy = sandbox.layout.legacy_composer_phar();
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "").unwrap();
        assert_eq!(sandbox.layout.composer_phar(), Some((None, legacy)));
        for version in ["2.8.12", "2.10.1"] {
            let dir = sandbox.layout.composer_root().join(version);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("composer.phar"), "").unwrap();
        }
        let (version, phar) = sandbox.layout.composer_phar().unwrap();
        assert_eq!(version.as_deref(), Some("2.10.1"));
        assert!(phar.ends_with("tools/composer/2.10.1/composer.phar"));
    }
}
