//! Instalador común de los servicios propios: el mismo camino que PHP
//! (`setup::download`: curl, sha256, tar y `rename` atómico), pero para
//! paquetes con una carpeta raíz y enlaces internos, como el de MySQL
//! (`mysql-8.4.7-macos15-arm64/bin/mysqld`, `lib/libssl.dylib -> libssl.3.dylib`).

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use super::catalog::ServiceRelease;
use super::{Env, ServiceKind};
use crate::setup::download;
use crate::{tr, tr_str};

/// Una entrada del paquete: nombre, tipo (`-`, `d`, `l`) y destino del enlace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub name: String,
    pub kind: char,
    pub link: Option<String>,
}

fn not_allowed(name: &str) -> String {
    tr!(
        "El paquete contiene una ruta no permitida: {name}",
        "The package contains a path that isn’t allowed: {name}"
    )
}

/// `true` si `path` (relativa) baja por debajo de su punto de partida.
fn escapes(path: &Path) -> bool {
    let mut depth = 0i32;
    for part in path.components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            Component::RootDir | Component::Prefix(_) => return true,
        }
    }
    false
}

/// Todo dentro de una única carpeta raíz, sin rutas absolutas ni `..`; solo
/// archivos, carpetas y enlaces relativos que no salen de la raíz.
pub(crate) fn check_entries(entries: &[Entry]) -> Result<(), String> {
    if entries.is_empty() {
        return Err(tr!("No se ha podido leer el contenido del paquete.", "Couldn’t read the contents of the package."));
    }
    let mut root: Option<&str> = None;
    for entry in entries {
        let name = entry.name.trim_start_matches("./");
        let path = Path::new(name);
        if name.is_empty() || entry.name.starts_with('/') || path.components().any(|c| c == Component::ParentDir) {
            return Err(not_allowed(&entry.name));
        }
        let top = name.split('/').next().unwrap_or("");
        match root {
            None => root = Some(top),
            Some(root) if root == top => {}
            Some(_) => return Err(not_allowed(&entry.name)),
        }
        match (entry.kind, &entry.link) {
            ('-' | 'd', None) => {}
            ('l', Some(target)) => {
                // El destino se resuelve desde la carpeta del enlace, sin la raíz.
                let inside = path.parent().unwrap_or(Path::new("")).components().skip(1);
                let joined: PathBuf = inside.collect::<PathBuf>().join(target);
                if target.starts_with('/') || escapes(&joined) {
                    return Err(not_allowed(&entry.name));
                }
            }
            _ => {
                let name = &entry.name;
                return Err(tr!(
                    "El paquete contiene una entrada inesperada: {name}",
                    "The package contains an unexpected entry: {name}"
                ));
            }
        }
    }
    Ok(())
}

/// Lee el paquete con `tar -tzf` (nombres) y `tar -tvzf` (tipo y destino de
/// los enlaces, en el mismo orden).
fn list_archive(tar: &Path, archive: &Path) -> Result<Vec<Entry>, String> {
    let names: Vec<String> = download::tar_output(tar, &["-tzf"], archive)?.lines().map(String::from).collect();
    let verbose = download::tar_output(tar, &["-tvzf"], archive)?;
    let lines: Vec<&str> = verbose.lines().collect();
    if lines.len() != names.len() {
        return Err(tr!("No se ha podido leer el contenido del paquete.", "Couldn’t read the contents of the package."));
    }
    Ok(names
        .into_iter()
        .zip(lines)
        .map(|(name, line)| {
            let kind = line.chars().next().unwrap_or('?');
            let marker = format!(" {name} -> ");
            let link = (kind == 'l')
                .then(|| line.find(&marker).map(|at| line[at + marker.len()..].to_string()))
                .flatten();
            Entry { name, kind, link }
        })
        .collect())
}

/// Descarga, verifica y deja `~/.elephant-stack/services/<servicio>/<versión>`.
/// Si ya existe, no hace nada.
pub(crate) fn install(
    env: &Env,
    release: &ServiceRelease,
    on_line: &mut dyn FnMut(&str),
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let kind = release.service;
    let version = release.version.as_str();
    let name = kind.name();
    if !super::catalog::is_version(version) || !super::catalog::is_branch_of(&release.branch, version) {
        return Err(tr!("Versión de {name} no válida: {version}", "Invalid {name} version: {version}"));
    }
    let dest = env.install_dir(kind, version);
    if kind.daemon(&dest).is_file() {
        on_line(&tr!("{name} {version} ya está instalado.", "{name} {version} is already installed."));
        return Ok(());
    }
    let asset = download::asset_name(&release.url)?;
    let part = env.data().join("cache").join(format!("{asset}.part"));
    download::dir_for(&part)?;
    let _ = fs::remove_file(&part);
    let url = &release.url;
    on_line(&tr!("Descargando {url}", "Downloading {url}"));
    download::download_file(&env.tools.curl, url, &part, release.size, on_bytes)?;
    let result = verify_and_extract(env, kind, &part, &release.sha256, version, &dest, on_line);
    let _ = fs::remove_file(&part);
    result
}

fn verify_and_extract(
    env: &Env,
    kind: ServiceKind,
    part: &Path,
    expected: &str,
    version: &str,
    dest: &Path,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    if download::sha256_file(part)? != expected.to_ascii_lowercase() {
        return Err(download::mismatch());
    }
    on_line(tr_str!("Firma correcta. Comprobando el contenido del paquete", "Signature OK. Checking the package contents"));
    check_entries(&list_archive(&env.tools.tar, part)?)?;

    let root = dest.parent().ok_or_else(|| tr!("Ruta de instalación no válida.", "Invalid install path."))?;
    let tmp = root.join(format!(".tmp-{version}"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|error| {
        let dir = tmp.display();
        tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
    })?;
    let name = kind.name();
    let installed = (|| {
        on_line(tr_str!("Descomprimiendo", "Extracting"));
        let status = Command::new(&env.tools.tar)
            .arg("-xzf")
            .arg(part)
            .args(["--strip-components", "1", "-C"])
            .arg(&tmp)
            .stdin(Stdio::null())
            .status()
            .map_err(|error| tr!("No se pudo ejecutar tar: {error}", "Couldn’t run tar: {error}"))?;
        if !status.success() {
            return Err(tr!("No se ha podido extraer el paquete.", "Couldn’t extract the package."));
        }
        match kind.version_of(&tmp) {
            Some(found) if found == version => {}
            Some(found) => {
                return Err(tr!(
                    "El paquete dice ser {name} {version} pero contiene {name} {found}.",
                    "The package says it’s {name} {version} but contains {name} {found}."
                ));
            }
            None => {
                return Err(tr!(
                    "El {name} {version} descargado no arranca en este Mac.",
                    "The downloaded {name} {version} doesn’t run on this Mac."
                ));
            }
        }
        if dest.exists() {
            return Ok(());
        }
        fs::rename(&tmp, dest)
            .map_err(|error| tr!("No se pudo instalar {name} {version}: {error}", "Couldn’t install {name} {version}: {error}"))
    })();
    let _ = fs::remove_dir_all(&tmp);
    if installed.is_ok() {
        let dir = dest.display();
        on_line(&tr!("{name} {version} instalado en {dir}", "{name} {version} installed in {dir}"));
    }
    installed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: char, link: Option<&str>) -> Entry {
        Entry { name: name.into(), kind, link: link.map(String::from) }
    }

    #[test]
    fn accepts_a_tree_like_mysqls() {
        let entries = [
            entry("mysql-8.4.11-macos15-arm64/", 'd', None),
            entry("mysql-8.4.11-macos15-arm64/bin/mysqld", '-', None),
            entry("mysql-8.4.11-macos15-arm64/bin/libprotobuf.24.4.0.dylib", 'l', Some("../lib/libprotobuf.24.4.0.dylib")),
            entry("mysql-8.4.11-macos15-arm64/lib/libssl.dylib", 'l', Some("libssl.3.dylib")),
            entry("mysql-8.4.11-macos15-arm64/lib/plugin/debug/libcrypto.3.dylib", 'l', Some("../../../lib/libcrypto.3.dylib")),
        ];
        check_entries(&entries).unwrap();
    }

    #[test]
    fn rejects_escapes_absolute_paths_and_odd_entries() {
        let root = |rest: Entry| vec![entry("m/", 'd', None), rest];
        for bad in [
            root(entry("m/lib/x", 'l', Some("../../etc/passwd"))),
            root(entry("m/lib/x", 'l', Some("/etc/passwd"))),
            root(entry("m/../x", '-', None)),
            root(entry("/etc/x", '-', None)),
            root(entry("otra/x", '-', None)),
            root(entry("m/dev", 'c', None)),
            root(entry("m/x", 'h', Some("m/y"))),
        ] {
            assert!(check_entries(&bad).is_err(), "{bad:?}");
        }
        assert!(check_entries(&[]).is_err());
    }
}
