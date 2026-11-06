//! Validación de lo que llega de fuera (la interfaz, la CLI o un agente) antes
//! de tocar el sistema.

use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;

/// Nombre de proyecto o de sitio: `^[a-z0-9][a-z0-9-]{0,62}$`.
pub fn name(s: &str) -> Result<(), String> {
    let bytes = s.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 63
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if valid {
        Ok(())
    } else {
        Err(crate::tr!(
            "Nombre no válido: «{s}». Usa minúsculas, números y guiones (máximo 63), empezando por letra o número.",
            "Invalid name: “{s}”. Use lowercase letters, numbers and hyphens (up to 63), starting with a letter or number."
        ))
    }
}

/// Carpeta de un proyecto del usuario: absoluta, existente y canónica (con
/// los enlaces resueltos), bajo `$HOME` y fuera de `~/.elephant-stack` y de
/// `~/Library`. Devuelve la ruta canónica, que es la que hay que usar.
pub fn user_dir(p: &Path) -> Result<PathBuf, String> {
    user_dir_in(&paths::home()?, p)
}

pub(crate) fn user_dir_in(home: &Path, p: &Path) -> Result<PathBuf, String> {
    if !p.is_absolute() {
        let p = p.display();
        return Err(crate::tr!("La ruta debe ser absoluta: {p}", "The path must be absolute: {p}"));
    }
    let dir = fs::canonicalize(p).map_err(|_| {
        let p = p.display();
        crate::tr!("La carpeta no existe: {p}", "The folder doesn’t exist: {p}")
    })?;
    if !dir.is_dir() {
        let dir = dir.display();
        return Err(crate::tr!("No es una carpeta: {dir}", "Not a folder: {dir}"));
    }
    let home = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if dir == home || !dir.starts_with(&home) {
        let dir = dir.display();
        return Err(crate::tr!(
            "Solo se permiten carpetas dentro de tu carpeta personal: {dir}",
            "Only folders inside your home folder are allowed: {dir}"
        ));
    }
    for forbidden in [paths::data_dir_in(&home), home.join("Library")] {
        if dir.starts_with(&forbidden) {
            let dir = dir.display();
            return Err(crate::tr!("Esa carpeta no puede ser un proyecto: {dir}", "That folder can’t be a project: {dir}"));
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn accepts_simple_names() {
        for ok in ["shop", "a", "mi-tienda-2", "0x", &"a".repeat(63)] {
            assert!(name(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn rejects_unsafe_names() {
        for bad in ["", "-shop", "Shop", "shop_1", "../x", "a b", "tienda.test", "ñ", &"a".repeat(64)] {
            assert!(name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn user_dirs_must_live_under_home() {
        let home = TempDir::new("validate-home");
        let project = home.path().join("Code/shop");
        fs::create_dir_all(&project).unwrap();
        assert_eq!(user_dir_in(home.path(), &project).unwrap(), project);

        // Con `..` y enlaces se compara la ruta ya resuelta.
        let dotted = home.path().join("Code/shop/../shop");
        assert_eq!(user_dir_in(home.path(), &dotted).unwrap(), project);

        assert!(user_dir_in(home.path(), Path::new("Code/shop")).is_err());
        assert!(user_dir_in(home.path(), home.path()).is_err());
        assert!(user_dir_in(home.path(), Path::new("/tmp")).is_err());
        assert!(user_dir_in(home.path(), &home.path().join("no-existe")).is_err());
    }

    #[test]
    fn user_dirs_cannot_be_inside_private_folders() {
        let home = TempDir::new("validate-private");
        for private in [".elephant-stack/bin", "Library/Application Support"] {
            let dir = home.path().join(private);
            fs::create_dir_all(&dir).unwrap();
            assert!(user_dir_in(home.path(), &dir).is_err(), "{private}");
        }
    }

    #[test]
    fn symlinks_are_resolved_before_checking() {
        let home = TempDir::new("validate-link");
        let outside = TempDir::new("validate-outside");
        let link = home.path().join("enlace");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        assert!(user_dir_in(home.path(), &link).is_err());
    }
}
