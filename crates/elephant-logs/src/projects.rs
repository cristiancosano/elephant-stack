//! Descubrimiento de proyectos (02 §4): la única fuente de proyectos de
//! Elephant. Lee Herd y `~/.elephant-stack/projects.json`; solo escribe este
//! último.

use std::path::Path;

use crate::{NOT_AVAILABLE, Project, ProjectOverride};

/// Ordenados por nombre.
pub fn discover(_home: &Path) -> Vec<Project> {
    Vec::new()
}

/// Por id, nombre o host.
pub fn resolve<'a>(_projects: &'a [Project], _needle: &str) -> Option<&'a Project> {
    None
}

/// Sin distinguir mayúsculas; `a.b.tienda.test` también es de `tienda.test`.
pub fn project_for_host<'a>(_projects: &'a [Project], _host: &str) -> Option<&'a Project> {
    None
}

pub fn add_root(_home: &Path, _root: &Path) -> Result<(), String> {
    Err(NOT_AVAILABLE.into())
}

pub fn remove_root(_home: &Path, _root: &Path) -> Result<(), String> {
    Err(NOT_AVAILABLE.into())
}

pub fn set_override(_home: &Path, _project_id: &str, _o: ProjectOverride) -> Result<(), String> {
    Err(NOT_AVAILABLE.into())
}
