//! Utilidades pequeñas que comparten los módulos: hash estable, recortes de
//! texto, normalización para las huellas y escritura atómica.

use std::fs;
use std::io::Write;
use std::path::Path;

use elephant_i18n::tr;

/// FNV-1a de 64 bits. No se usa `DefaultHasher` porque su resultado puede
/// cambiar entre versiones de Rust, y los ids tienen que ser estables entre
/// sesiones y procesos (la app y la CLI resuelven los mismos).
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

pub(crate) fn hex16(value: u64) -> String {
    format!("{value:016x}")
}

pub(crate) fn path_key(path: &Path) -> u64 {
    fnv1a(path.as_os_str().as_encoded_bytes())
}

/// Recorta a `max` bytes sin partir un carácter UTF-8.
pub(crate) fn truncate_utf8(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Quita el `\n` (y el `\r`) del final.
pub(crate) fn trim_newline(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && matches!(bytes[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

/// La primera línea, sin el salto.
pub(crate) fn first_line(bytes: &[u8]) -> &[u8] {
    match memchr::memchr(b'\n', bytes) {
        Some(end) => trim_newline(&bytes[..end]),
        None => trim_newline(bytes),
    }
}

/// Para las huellas: dígitos → `#`, UUID → `<uuid>`, hex de 16 o más → `<hex>`,
/// y como mucho 200 bytes.
pub(crate) fn normalize_message(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len().min(256));
    let mut i = 0;
    while i < bytes.len() && out.len() < 200 {
        if is_uuid_at(bytes, i) {
            out.push_str("<uuid>");
            i += 36;
            continue;
        }
        let hex_run = bytes[i..]
            .iter()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        let starts_word = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if hex_run >= 16
            && starts_word
            && !bytes
                .get(i + hex_run)
                .is_some_and(u8::is_ascii_alphanumeric)
        {
            out.push_str("<hex>");
            i += hex_run;
            continue;
        }
        if bytes[i].is_ascii_digit() {
            out.push('#');
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        let ch = text[i..].chars().next().unwrap_or('\u{fffd}');
        out.push(ch);
        i += ch.len_utf8();
    }
    truncate_utf8(&out, 200).to_string()
}

fn is_uuid_at(bytes: &[u8], i: usize) -> bool {
    let Some(candidate) = bytes.get(i..i + 36) else {
        return false;
    };
    candidate
        .iter()
        .enumerate()
        .all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// Ruta HTTP para las huellas: sin query, y con los segmentos numéricos o UUID
/// cambiados por `{id}`.
pub(crate) fn normalize_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    path.split('/')
        .map(|segment| {
            let numeric = !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit());
            let uuid = segment.len() == 36 && is_uuid_at(segment.as_bytes(), 0);
            if numeric || uuid { "{id}" } else { segment }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `*` en el nombre del archivo: cualquier secuencia, también vacía.
pub(crate) fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// Escribe en un temporal junto al destino y lo renombra: quien lee nunca ve
/// un archivo a medias.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| {
            let path = path.display();
            tr!("Ruta no válida: {path}.", "Invalid path: {path}.")
        })?;
    fs::create_dir_all(dir).map_err(|e| {
        let dir = dir.display();
        tr!("No se pudo crear {dir}: {e}.", "Couldn’t create {dir}: {e}.")
    })?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archivo"),
        std::process::id()
    ));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    result.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        let path = path.display();
        tr!("No se pudo guardar {path}: {e}.", "Couldn’t save {path}: {e}.")
    })
}

/// Desescapa lo que Monolog escapa en JSON dentro de una línea: `\\`, `\/`,
/// `\"`, `\n` y `\t`. Lo demás se deja tal cual.
pub(crate) fn unescape_json_fragment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Carpeta temporal que se borra al soltarla. Nunca se toca el `HOME` real.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "elephant-logs-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("debe crear la carpeta temporal");
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_matches_the_reference_values() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(hex16(fnv1a(b"a")).len(), 16);
    }

    #[test]
    fn truncation_respects_utf8() {
        assert_eq!(truncate_utf8("año", 2), "a");
        assert_eq!(truncate_utf8("año", 3), "añ");
        assert_eq!(truncate_utf8("abc", 10), "abc");
    }

    #[test]
    fn messages_are_normalized_for_fingerprints() {
        assert_eq!(
            normalize_message("Pedido 1234 falló en 3 s"),
            "Pedido # falló en # s"
        );
        assert_eq!(
            normalize_message("job 550e8400-e29b-41d4-a716-446655440000 hecho"),
            "job <uuid> hecho"
        );
        assert_eq!(
            normalize_message("token 0123456789abcdef0123 caducado"),
            "token <hex> caducado"
        );
        assert!(normalize_message(&"x".repeat(500)).len() <= 200);
    }

    #[test]
    fn paths_are_normalized_for_fingerprints() {
        assert_eq!(
            normalize_path("/api/orders/42/items?page=2"),
            "/api/orders/{id}/items"
        );
        assert_eq!(
            normalize_path("/u/550e8400-e29b-41d4-a716-446655440000"),
            "/u/{id}"
        );
    }

    #[test]
    fn globs_only_use_star() {
        assert!(glob_match("*.log", "laravel.log"));
        assert!(glob_match("laravel-*.log", "laravel-2026-11-06.log"));
        assert!(!glob_match("*.log", "laravel.log.1"));
        assert!(glob_match("nginx-error.log", "nginx-error.log"));
        assert!(!glob_match("a*b*c", "acb"));
        assert!(glob_match("a*b*c", "axxbyyc"));
    }

    #[test]
    fn json_fragments_are_unescaped() {
        assert_eq!(
            unescape_json_fragment(r#"App\\Models\\User \"x\" a\/b"#),
            r#"App\Models\User "x" a/b"#
        );
    }

    #[test]
    fn atomic_writes_replace_the_file() {
        let dir = testutil::TempDir::new("atomic");
        let path = dir.path().join("sub/projects.json");
        write_atomic(&path, b"uno").unwrap();
        write_atomic(&path, b"dos").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "dos");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
}
