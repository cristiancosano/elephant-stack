//! Sidecars (C21): `tauri-build` copia cada `bundle.externalBin` a
//! `target/<perfil>/<nombre>` y falla si el archivo no existe. Los compila
//! `scripts/build-sidecars.sh` (lo lanzan `pnpm tauri dev` y `pnpm tauri build`).
//!
//! - En debug, si falta uno, se crea un marcador ejecutable que avisa y sale
//!   con el código 1: así `cargo test --workspace` funciona sin compilarlos.
//!   La app reconoce el marcador (`elephant_core::paths::is_sidecar_marker`)
//!   y usa entonces el binario de cargo de `target/debug/`.
//! - En release, que falte o que sea un marcador es un error.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const SIDECARS: [&str; 2] = ["cli", "php-selector"];
/// Lo mismo que `elephant_core::paths::SIDECAR_MARKER` (un test lo comprueba).
const MARKER_TAG: &str = "# elephant-stack: marcador de sidecar";

fn main() {
    let triple = std::env::var("TARGET").expect("cargo define TARGET");
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    let dir = Path::new("binaries");

    for name in SIDECARS {
        let path = dir.join(format!("{name}-{triple}"));
        println!("cargo:rerun-if-changed={}", path.display());
        let marker = is_marker(&path);
        if release {
            if !path.is_file() || marker {
                panic!(
                    "Falta el sidecar {} para release: ejecuta scripts/build-sidecars.sh release.",
                    path.display()
                );
            }
        } else if !path.is_file() {
            write_marker(&path, name);
        }
    }

    tauri_build::build()
}

fn is_marker(path: &Path) -> bool {
    fs::read(path).is_ok_and(|bytes| {
        let head = &bytes[..bytes.len().min(128)];
        String::from_utf8_lossy(head).contains(MARKER_TAG)
    })
}

fn write_marker(path: &Path, name: &str) {
    let script = format!(
        "#!/bin/sh\n{MARKER_TAG} (lo crea src-tauri/build.rs)\n\
         echo 'Falta el sidecar {name}. Ejecuta scripts/build-sidecars.sh' >&2\nexit 1\n"
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("crear src-tauri/binaries");
    }
    fs::write(path, script).expect("escribir el marcador del sidecar");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("hacer ejecutable el marcador");
}
