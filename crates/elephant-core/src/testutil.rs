//! Ayudas para los tests: nunca se toca el `HOME` real.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Carpeta temporal que se borra al soltarla.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "elephant-core-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("debe crear la carpeta temporal");
        // Canónica: en macOS /var es un enlace a /private/var.
        Self(fs::canonicalize(&path).expect("debe resolver la carpeta temporal"))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Escribe un script ejecutable, para usarlo como herramienta falsa.
pub fn write_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("debe escribir el script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("debe dar permisos");
}
