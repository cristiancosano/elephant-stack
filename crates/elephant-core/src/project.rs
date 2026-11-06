//! Crear proyectos en el directorio aparcado de Herd (C23). P09 lo implementa.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::NOT_AVAILABLE;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Template {
    #[default]
    Laravel,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Created {
    pub root: PathBuf,
    /// `<nombre>.<tld>` si Herd lo sirve.
    pub host: Option<String>,
}

/// Primer `paths[i]` de Herd que no acaba en `valet/Sites`.
pub fn parked_dir(_home: &Path) -> Option<PathBuf> {
    None
}

pub fn create(
    _name: &str,
    _template: Template,
    _php: Option<&str>,
    _on_line: &mut dyn FnMut(&str),
) -> Result<Created, String> {
    Err(NOT_AVAILABLE.into())
}
