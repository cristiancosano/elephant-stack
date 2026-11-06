//! Bloque del PATH en `~/.zshrc` (04 §4.7). P04 lo implementa.

use std::path::Path;

use super::ShellPreview;
use crate::NOT_AVAILABLE;

pub fn preview(_home: &Path) -> Result<ShellPreview, String> {
    Err(NOT_AVAILABLE.into())
}

/// Solo aplica el cambio si `hash` coincide con el de la última vista previa.
pub fn apply(_home: &Path, _hash: &str) -> Result<(), String> {
    Err(NOT_AVAILABLE.into())
}
