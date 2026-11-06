use std::fs;
use std::io::Write;
use std::path::Path;

/// Escribe en un temporal del mismo directorio y lo renombra, para que nadie
/// lea nunca un archivo a medias. Crea el directorio si no existe.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| {
            let path = path.display();
            crate::tr!("Ruta sin directorio: {path}", "Path without a folder: {path}")
        })?;
    fs::create_dir_all(dir).map_err(|error| {
        let dir = dir.display();
        crate::tr!("No se pudo crear {dir}: {error}", "Couldn’t create {dir}: {error}")
    })?;

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = fs::File::create(&tmp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, path));
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        let path = path.display();
        return Err(crate::tr!("No se pudo guardar {path}: {error}", "Couldn’t save {path}: {error}"));
    }
    Ok(())
}

/// JSON con sangría y salto de línea final.
pub(crate) fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    write_atomic(path, format!("{json}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn replaces_the_file_and_leaves_no_temporaries() {
        let dir = TempDir::new("fsutil");
        let path = dir.path().join("sub/settings.json");
        write_atomic(&path, b"uno").unwrap();
        write_atomic(&path, b"dos").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "dos");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
    }
}
