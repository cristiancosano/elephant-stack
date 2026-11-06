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

/// Origen de las versiones que instala Elephant.
pub const OWN_SOURCE: &str = "Elephant Stack";

pub(crate) fn status_of(layout: &Layout) -> PhpStatus {
    // La raíz no tiene `.php-version`, así que esto da la versión global.
    let active = layout.resolve(Path::new("/")).ok().map(VersionInfo::from);
    PhpStatus {
        // Solo las versiones propias: las de otros programas (Herd) no se listan
        // ni se ofrecen. El selector las sigue resolviendo, así que un proyecto
        // o un ajuste global que ya apunte a ellas sigue funcionando.
        versions: layout
            .discover()
            .into_iter()
            .filter(|version| version.source == OWN_SOURCE)
            .map(VersionInfo::from)
            .collect(),
        active,
    }
}

/// Resultado de sustituir una versión propia por otra más reciente de su rama.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PhpReplaceReport {
    pub old: String,
    pub new: String,
    /// La versión por defecto apuntaba a la antigua y ahora apunta a la nueva.
    pub global_changed: bool,
    /// Carpetas de proyecto cuyo `.php-version` fijaba la antigua.
    pub projects: Vec<String>,
}

fn patch_parts(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|part| {
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    });
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

/// Sustituye la versión propia `old` por `new`, de la misma rama y ya
/// instalada: la versión por defecto y los `.php-version` de `project_roots`
/// que fijaban `old` pasan a `new`, y después se borra `old`. Lo que apunta a la
/// rama (`8.4`) no cambia: ya resuelve a la versión más reciente.
pub fn replace_own(
    layout: &Layout,
    old: &str,
    new: &str,
    project_roots: &[std::path::PathBuf],
) -> Result<PhpReplaceReport, String> {
    let (Some(old_parts), Some(new_parts)) = (patch_parts(old), patch_parts(new)) else {
        return Err(crate::tr!("Las versiones deben tener la forma 8.4.27.", "Versions must look like 8.4.27."));
    };
    if (old_parts.0, old_parts.1) != (new_parts.0, new_parts.1) || new_parts.2 <= old_parts.2 {
        return Err(crate::tr!(
            "PHP {new} no es una versión más reciente de la rama de PHP {old}.",
            "PHP {new} isn’t a newer version in the PHP {old} branch."
        ));
    }
    let own: Vec<String> = layout
        .discover()
        .into_iter()
        .filter(|version| version.source == OWN_SOURCE)
        .map(|version| version.id)
        .collect();
    if !own.iter().any(|id| id == old) {
        return Err(crate::tr!(
            "PHP {old} (la versión que se sustituye) no está instalada en Elephant Stack.",
            "PHP {old} (the version being replaced) isn’t installed in Elephant Stack."
        ));
    }
    if !own.iter().any(|id| id == new) {
        return Err(crate::tr!(
            "PHP {new} (la versión nueva) no está instalada en Elephant Stack.",
            "PHP {new} (the new version) isn’t installed in Elephant Stack."
        ));
    }

    let global_changed = layout.global().as_deref() == Some(old);
    if global_changed {
        layout.set_global(new)?;
    }

    let mut projects = Vec::new();
    for root in project_roots {
        let file = root.join(elephant_php::PROJECT_FILE);
        let pinned = std::fs::read_to_string(&file)
            .ok()
            .is_some_and(|text| text.lines().map(str::trim).find(|l| !l.is_empty()) == Some(old));
        if pinned {
            layout.set_project(root, new)?;
            projects.push(root.display().to_string());
        }
    }

    // Solo se borra una carpeta de versión dentro de `php/` de Elephant.
    let php_dir = layout.data_dir.join("php");
    let target = php_dir.join(old);
    match (php_dir.canonicalize(), target.canonicalize()) {
        (Ok(base), Ok(resolved)) if resolved.parent() == Some(base.as_path()) => {
            std::fs::remove_dir_all(&resolved)
                .map_err(|error| crate::tr!("No se pudo borrar PHP {old}: {error}", "Couldn’t delete PHP {old}: {error}"))?;
        }
        _ => return Err(crate::tr!("No se encuentra la carpeta de PHP {old}.", "Couldn’t find the PHP {old} folder.")),
    }

    Ok(PhpReplaceReport {
        old: old.to_string(),
        new: new.to_string(),
        global_changed,
        projects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use std::fs;

    fn install_own(home: &Path, version: &str) {
        let dir = home.join(".elephant-stack/php").join(version);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("php"), "").unwrap();
    }

    fn install_herd(home: &Path, binary: &str) {
        let dir = home.join("Library/Application Support/Herd/bin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(binary), "").unwrap();
    }

    #[test]
    fn status_lists_only_own_versions() {
        let home = TempDir::new("php-status");
        install_own(home.path(), "8.4.27");
        install_herd(home.path(), "php84");
        let status = status_of(&Layout::from_home(home.path()));
        let ids: Vec<&str> = status.versions.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["8.4.27"]);
    }

    #[test]
    fn replace_moves_default_and_pinned_projects_then_removes_old() {
        let home = TempDir::new("php-replace");
        install_own(home.path(), "8.4.27");
        install_own(home.path(), "8.4.28");
        let layout = Layout::from_home(home.path());
        layout.set_global("8.4.27").unwrap();

        let pinned = home.path().join("shop");
        let branch = home.path().join("blog");
        let other = home.path().join("api");
        for (dir, version) in [(&pinned, "8.4.27"), (&branch, "8.4"), (&other, "8.3.36")] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(".php-version"), format!("{version}\n")).unwrap();
        }

        let report =
            replace_own(&layout, "8.4.27", "8.4.28", &[pinned.clone(), branch.clone(), other.clone()])
                .unwrap();

        assert!(report.global_changed);
        assert_eq!(layout.global().as_deref(), Some("8.4.28"));
        assert_eq!(report.projects, [pinned.display().to_string()]);
        assert_eq!(fs::read_to_string(pinned.join(".php-version")).unwrap(), "8.4.28\n");
        assert_eq!(fs::read_to_string(branch.join(".php-version")).unwrap(), "8.4\n");
        assert_eq!(fs::read_to_string(other.join(".php-version")).unwrap(), "8.3.36\n");
        assert!(!home.path().join(".elephant-stack/php/8.4.27").exists());
        assert!(home.path().join(".elephant-stack/php/8.4.28/php").exists());
    }

    #[test]
    fn replace_keeps_a_default_that_points_elsewhere() {
        let home = TempDir::new("php-replace-global");
        install_own(home.path(), "8.4.27");
        install_own(home.path(), "8.4.28");
        let layout = Layout::from_home(home.path());
        layout.set_global("8.4").unwrap();
        let report = replace_own(&layout, "8.4.27", "8.4.28", &[]).unwrap();
        assert!(!report.global_changed);
        assert_eq!(layout.global().as_deref(), Some("8.4"));
    }

    #[test]
    fn replace_rejects_other_branches_downgrades_and_missing_versions() {
        let home = TempDir::new("php-replace-invalid");
        install_own(home.path(), "8.4.27");
        install_own(home.path(), "8.5.12");
        install_own(home.path(), "8.4.20");
        let layout = Layout::from_home(home.path());
        assert!(replace_own(&layout, "8.4.27", "8.5.12", &[]).is_err());
        assert!(replace_own(&layout, "8.4.27", "8.4.20", &[]).is_err());
        assert!(replace_own(&layout, "8.4.27", "8.4.99", &[]).is_err());
        assert!(replace_own(&layout, "../8.4.27", "8.4.28", &[]).is_err());
        assert!(home.path().join(".elephant-stack/php/8.4.27/php").exists());
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
