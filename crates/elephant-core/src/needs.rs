//! «Arrancar lo que necesita» un proyecto, a partir de su `.env` (04 §4.10).
//! P10 lo implementa.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::NOT_AVAILABLE;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Needs {
    /// Ids de servicios de Homebrew (`mysql`, `redis`…).
    pub services: Vec<String>,
    /// Por qué hace falta cada uno, o por qué no se ha podido saber.
    pub reasons: Vec<String>,
}

/// Solo lee el `.env`; nunca lo escribe.
pub fn project_needs(_dir: &Path) -> Needs {
    Needs::default()
}

/// Arranca (sin arranque automático) lo que falte y devuelve lo arrancado.
pub fn start_project_services(_dir: &Path) -> Result<Vec<String>, String> {
    Err(NOT_AVAILABLE.into())
}
