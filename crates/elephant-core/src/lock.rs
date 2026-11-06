//! Cerrojo entre procesos para Homebrew: la app, la CLI y las tareas de los
//! agentes nunca ejecutan dos escrituras de `brew` a la vez.
//!
//! `flock` se aplica por descripción de archivo abierto: si un mismo proceso
//! pide el cerrojo dos veces se bloquea para siempre. Por eso solo lo toman las
//! funciones de alto nivel ([`crate::ops`]) y las de bajo nivel lo exigen como
//! argumento (`&BrewLock`).

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::paths;

/// Cerrojo exclusivo. Se libera al soltarlo o al morir el proceso.
#[derive(Debug)]
pub struct BrewLock(#[allow(dead_code)] File);

fn lock_path() -> Result<PathBuf, String> {
    Ok(paths::data_dir()?.join("locks/brew.lock"))
}

fn open(path: &Path) -> Result<File, String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| {
                let dir = dir.display();
                crate::tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
            })?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|error| crate::tr!("No se pudo abrir el cerrojo de Homebrew: {error}", "Couldn’t open the Homebrew lock: {error}"))
}

/// Bloquea hasta conseguir el cerrojo.
pub fn brew() -> Result<BrewLock, String> {
    brew_at(&lock_path()?)
}

/// Lo consigue solo si está libre; `None` si otro proceso lo tiene.
pub fn try_brew() -> Result<Option<BrewLock>, String> {
    try_brew_at(&lock_path()?)
}

pub(crate) fn brew_at(path: &Path) -> Result<BrewLock, String> {
    let file = open(path)?;
    file.lock()
        .map_err(|error| crate::tr!("No se pudo tomar el cerrojo de Homebrew: {error}", "Couldn’t take the Homebrew lock: {error}"))?;
    Ok(BrewLock(file))
}

pub(crate) fn try_brew_at(path: &Path) -> Result<Option<BrewLock>, String> {
    let file = open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(BrewLock(file))),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => {
            Err(crate::tr!("No se pudo tomar el cerrojo de Homebrew: {error}", "Couldn’t take the Homebrew lock: {error}"))
        }
    }
}

#[cfg(test)]
impl BrewLock {
    /// Cerrojo en una carpeta temporal, para probar funciones de bajo nivel.
    pub(crate) fn for_tests(dir: &Path) -> Self {
        brew_at(&dir.join("brew.lock")).expect("debe tomar el cerrojo de prueba")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn a_second_holder_waits_until_the_first_releases() {
        let dir = TempDir::new("lock");
        let path = dir.path().join("locks/brew.lock");
        let first = brew_at(&path).unwrap();
        assert!(try_brew_at(&path).unwrap().is_none());
        drop(first);
        assert!(try_brew_at(&path).unwrap().is_some());
    }
}
