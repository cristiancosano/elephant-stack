use std::env;
use std::path::{Path, PathBuf};

use elephant_php::{Layout, PhpVersion};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    /// Lo que hay que enviar para elegir exactamente esta instalación.
    pub key: String,
    pub id: String,
    pub source: &'static str,
}

impl From<PhpVersion> for VersionInfo {
    fn from(version: PhpVersion) -> Self {
        Self {
            key: version.key(),
            id: version.id,
            source: version.source,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PhpStatus {
    pub versions: Vec<VersionInfo>,
    /// La que se usa fuera de cualquier proyecto.
    pub active: Option<VersionInfo>,
}

fn layout() -> Result<Layout, String> {
    let home = env::var_os("HOME").ok_or("HOME no está definido.")?;
    Ok(Layout::from_home(&PathBuf::from(home)))
}

pub fn status() -> Result<PhpStatus, String> {
    let layout = layout()?;
    // La raíz no tiene `.php-version`, así que esto da la versión global.
    let active = layout.resolve(Path::new("/")).ok().map(VersionInfo::from);
    Ok(PhpStatus {
        versions: layout.discover().into_iter().map(VersionInfo::from).collect(),
        active,
    })
}

pub fn set_global(version: &str) -> Result<PhpStatus, String> {
    layout()?.set_global(version)?;
    status()
}

pub fn set_project(dir: &str, version: &str) -> Result<(), String> {
    layout()?.set_project(Path::new(dir), version)
}

#[cfg(test)]
mod system_tests {
    /// Consulta el sistema real sin modificarlo: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore = "depende de las versiones instaladas"]
    fn reads_real_php_status() {
        println!("{:#?}", super::status().expect("debe leer el estado de PHP"));
    }
}
