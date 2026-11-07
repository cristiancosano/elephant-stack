//! Los selectores de `~/.elephant-stack/bin` (06, fase 3): el binario `php`
//! que viaja con la app y los enlaces a él (`node`, `npm`, `composer`…), y si
//! la shell del usuario los encuentra antes que los de nvm, Herd o Homebrew.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use elephant_php::Layout;
use serde::Serialize;

use crate::setup::download::{self, Tools};
use crate::setup::{FileState, detect};
use crate::{paths, tr};

/// La línea que hay que tener al final de `~/.zshrc`.
pub const PATH_LINE: &str = "export PATH=\"$HOME/.elephant-stack/bin:$PATH\"";

/// El binario de los selectores que viaja con la app (sidecar `php-selector`).
pub(crate) fn bundled_source() -> Option<PathBuf> {
    detect::default_shim_source()
}

/// Antes de crear enlaces a `bin/php`, el binario tiene que existir.
pub(crate) fn check_shim(tools: &Tools, source: Option<&Path>) -> Result<(), String> {
    if source.is_some() || Layout::from_home(&tools.home).shim_path().is_file() {
        return Ok(());
    }
    Err(tr!(
        "Falta el selector de Elephant (~/.elephant-stack/bin/php) y esta copia de Elephant no lo trae. Abre la app y vuelve a intentarlo.",
        "Elephant’s selector (~/.elephant-stack/bin/php) is missing and this copy of Elephant doesn’t include it. Open the app and try again."
    ))
}

/// Deja en `bin/php` el binario de la app si falta o es otro: los enlaces
/// `node`, `npm`, `composer`… necesitan uno que sepa hacer de todos.
pub(crate) fn ensure_shim(tools: &Tools, source: Option<&Path>) -> Result<(), String> {
    check_shim(tools, source)?;
    let shim = Layout::from_home(&tools.home).shim_path();
    match source {
        Some(source) if detect::file_state(&shim, Some(source)) != FileState::Same => download::install_shim(tools, source),
        _ => Ok(()),
    }
}

/// Si `~/.elephant-stack/bin` va delante en la shell (lo que lee `~/.zprofile`
/// y `~/.zshrc`, sin ejecutarlos).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PathAdvice {
    /// `~/.elephant-stack/bin`.
    pub bin: String,
    /// Alguna línea pone esa carpeta en el PATH.
    pub configured: bool,
    /// `Herd` o `nvm`, si se cargan después y quedan delante.
    pub shadowed_by: Option<String>,
    /// La línea que hay que poner al final de `~/.zshrc`.
    pub line: String,
}

impl PathAdvice {
    /// La terminal encuentra antes los selectores de Elephant.
    pub fn ok(&self) -> bool {
        self.configured && self.shadowed_by.is_none()
    }
}

pub fn path_advice() -> PathAdvice {
    match paths::home() {
        Ok(home) => path_advice_in(&home),
        Err(_) => PathAdvice { bin: String::new(), configured: false, shadowed_by: None, line: PATH_LINE.into() },
    }
}

pub(crate) fn path_advice_in(home: &Path) -> PathAdvice {
    let scripts: Vec<String> =
        [".zprofile", ".zshrc"].iter().filter_map(|name| fs::read_to_string(home.join(name)).ok()).collect();
    let (configured, shadowed_by) = detect::path_shadow(home, &scripts);
    PathAdvice {
        bin: paths::data_dir_in(home).join("bin").display().to_string(),
        configured,
        shadowed_by: shadowed_by.map(String::from),
        line: PATH_LINE.into(),
    }
}

/// Qué encuentra un `PATH` concreto para una orden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandCheck {
    pub command: String,
    /// La primera del PATH.
    pub found: Option<PathBuf>,
    /// Es el selector de Elephant.
    pub ours: bool,
    /// Elephant tiene el selector (`~/.elephant-stack/bin/<orden>`).
    pub available: bool,
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `php`, `composer`, `node`, `npm` y `npx` en `path` (el de la terminal en
/// `elephant doctor`). Compara carpetas canónicas: `~/.localdeck/bin`, si es
/// un enlace a `~/.elephant-stack/bin`, cuenta como la de Elephant.
pub fn command_checks(path: &OsStr, home: &Path) -> Vec<CommandCheck> {
    let ours = canonical(&paths::data_dir_in(home).join("bin"));
    ["php", "composer", "node", "npm", "npx"]
        .into_iter()
        .map(|command| {
            let found = std::env::split_paths(path).map(|dir| dir.join(command)).find(|candidate| {
                fs::metadata(candidate).is_ok_and(|meta| {
                    use std::os::unix::fs::PermissionsExt;
                    meta.is_file() && meta.permissions().mode() & 0o111 != 0
                })
            });
            let is_ours = found.as_ref().and_then(|f| f.parent()).is_some_and(|dir| canonical(dir) == ours);
            CommandCheck {
                command: command.into(),
                available: fs::metadata(ours.join(command)).is_ok(),
                ours: is_ours,
                found,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, write_script};

    #[test]
    fn the_first_match_in_path_decides_and_links_to_our_bin_count() {
        let home = TempDir::new("checks");
        let ours = home.path().join(".elephant-stack/bin");
        let nvm = home.path().join(".nvm/versions/node/v24.18.0/bin");
        fs::create_dir_all(&ours).unwrap();
        fs::create_dir_all(&nvm).unwrap();
        write_script(&ours.join("php"), "");
        std::os::unix::fs::symlink("php", ours.join("node")).unwrap();
        write_script(&nvm.join("node"), "");
        write_script(&nvm.join("npm"), "");
        std::os::unix::fs::symlink(home.path().join(".elephant-stack"), home.path().join(".localdeck")).unwrap();

        let path = std::env::join_paths([home.path().join(".localdeck/bin"), nvm.clone()]).unwrap();
        let checks = command_checks(&path, home.path());
        let by = |name: &str| checks.iter().find(|c| c.command == name).unwrap().clone();
        assert!(by("php").ours && by("node").ours && by("node").available);
        assert!(!by("npm").ours && !by("npm").available);
        assert_eq!(by("npm").found, Some(nvm.join("npm")));
        assert_eq!(by("composer").found, None);

        let path = std::env::join_paths([nvm.clone(), ours.clone()]).unwrap();
        let node = command_checks(&path, home.path()).into_iter().find(|c| c.command == "node").unwrap();
        assert!(!node.ours && node.available);
    }

    #[test]
    fn advice_sees_nvm_and_herd_loaded_after_our_line() {
        let home = TempDir::new("advice");
        let zshrc = home.path().join(".zshrc");
        fs::write(&zshrc, format!("{PATH_LINE}\nexport NVM_DIR=\"$HOME/.nvm\"\n[ -s \"$NVM_DIR/nvm.sh\" ] && \\. \"$NVM_DIR/nvm.sh\"\n")).unwrap();
        let advice = path_advice_in(home.path());
        assert!(advice.configured && !advice.ok());
        assert_eq!(advice.shadowed_by.as_deref(), Some("nvm"));

        fs::write(&zshrc, format!("[ -s \"/opt/homebrew/opt/nvm/nvm.sh\" ] && \\. \"/opt/homebrew/opt/nvm/nvm.sh\"\nexport PATH=\"/Users/x/Library/Application Support/Herd/bin/\":$PATH\n# {PATH_LINE}\n{PATH_LINE}\n")).unwrap();
        assert!(path_advice_in(home.path()).ok());

        fs::write(&zshrc, "export PATH=\"/opt/homebrew/bin:$PATH\"\n").unwrap();
        let advice = path_advice_in(home.path());
        assert!(!advice.configured && advice.shadowed_by.is_none());
    }
}
