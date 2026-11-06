use std::path::Path;

use elephant_php::{Layout, PhpVersion};
use serde::Serialize;

use crate::paths;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PhpStatus {
    pub versions: Vec<VersionInfo>,
    /// La que se usa fuera de cualquier proyecto.
    pub active: Option<VersionInfo>,
}

pub(crate) fn layout() -> Result<Layout, String> {
    Ok(Layout::from_home(&paths::home()?))
}

pub fn status() -> Result<PhpStatus, String> {
    Ok(status_of(&layout()?))
}

pub(crate) fn status_of(layout: &Layout) -> PhpStatus {
    // La raíz no tiene `.php-version`, así que esto da la versión global.
    let active = layout.resolve(Path::new("/")).ok().map(VersionInfo::from);
    PhpStatus {
        versions: layout.discover().into_iter().map(VersionInfo::from).collect(),
        active,
    }
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
