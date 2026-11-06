//! Ocultación de secretos en todo lo que sale hacia un agente (C6): la unión
//! de los patrones de 02 §7 y 03 §5.1, sustituidos por `••••`.
//!
//! - Valores de claves que contienen `password`, `passwd`, `secret`, `token`,
//!   `api_key`, `apikey`, `authorization`, `cookie`, `session`, `app_key` o
//!   `private_key`, sin distinguir mayúsculas, en las formas `clave=valor`,
//!   `clave: valor`, `"clave": "valor"` y `'clave' => 'valor'`.
//! - `Bearer <token>`.
//! - Credenciales en URLs: `scheme://usuario:clave@host`.
//! - Cadenas `base64:` de 32 caracteres o más (como `APP_KEY`).
//!
//! Se escribe a mano, sin `regex`. Tiende a ocultar de más: perder un dato de
//! un log es mejor que filtrar una contraseña.

use std::ops::Range;

pub const MASK: &str = "••••";

const SENSITIVE: [&str; 11] = [
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "authorization",
    "cookie",
    "session",
    "app_key",
    "private_key",
];

/// Una clave cuyo valor nunca debe salir en claro. `X-Api-Key` y `api.key`
/// cuentan igual que `api_key`.
pub fn is_sensitive_key(key: &str) -> bool {
    let key: String = key
        .chars()
        .map(|c| match c {
            '-' | '.' => '_',
            c => c.to_ascii_lowercase(),
        })
        .collect();
    SENSITIVE.iter().any(|word| key.contains(word))
}

pub fn mask(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    key_values(bytes, &mut ranges);
    bearer_tokens(bytes, &mut ranges);
    url_credentials(bytes, &mut ranges);
    base64_keys(bytes, &mut ranges);
    if ranges.is_empty() {
        return text.to_string();
    }

    ranges.sort_by_key(|range| range.start);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for range in ranges {
        if range.end <= cursor {
            continue;
        }
        let start = range.start.max(cursor);
        out.push_str(&text[cursor..start]);
        out.push_str(MASK);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Recorre un JSON: oculta los valores de las claves sensibles y aplica
/// [`mask`] a todas las cadenas.
pub fn mask_json(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::String(text) => *text = mask(text),
        Value::Array(items) => items.iter_mut().for_each(mask_json),
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if is_sensitive_key(key) {
                    mask_all(item);
                } else {
                    mask_json(item);
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Todo lo que cuelga de una clave sensible se oculta, salvo `null`.
fn mask_all(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Null => {}
        Value::Array(items) => items.iter_mut().for_each(mask_all),
        Value::Object(map) => map.values_mut().for_each(mask_all),
        _ => *value = Value::String(MASK.to_string()),
    }
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    memchr_line(bytes, from).unwrap_or(bytes.len())
}

fn memchr_line(bytes: &[u8], from: usize) -> Option<usize> {
    bytes[from..]
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .map(|offset| from + offset)
}

/// Fin de un valor sin comillas.
fn word_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|&b| {
            b.is_ascii_whitespace()
                || matches!(b, b',' | b';' | b'&' | b')' | b'}' | b']' | b'"' | b'\'' | b'<' | b'>')
        })
        .map_or(bytes.len(), |offset| from + offset)
}

fn key_values(bytes: &[u8], ranges: &mut Vec<Range<usize>>) {
    let mut i = 0;
    while i < bytes.len() {
        if !is_ident(bytes[i]) || (i > 0 && is_ident(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_ident(bytes[i]) {
            i += 1;
        }
        // Los nombres de clave son ASCII: el corte es válido en UTF-8.
        let key = std::str::from_utf8(&bytes[start..i]).unwrap_or_default();
        if !is_sensitive_key(key) {
            continue;
        }
        if let Some(value) = value_after_key(bytes, i, key) {
            i = i.max(value.end);
            ranges.push(value);
        }
    }
}

/// El valor que sigue a una clave sensible, si lo que viene después es un
/// separador (`=`, `:`, `=>`) y no código como `Token::class` o `$token == x`.
fn value_after_key(bytes: &[u8], key_end: usize, key: &str) -> Option<Range<usize>> {
    let mut j = key_end;
    if j < bytes.len() && matches!(bytes[j], b'"' | b'\'') {
        j += 1;
    }
    while j < bytes.len() && is_blank(bytes[j]) {
        j += 1;
    }
    let rest = &bytes[j..];
    let header_style = if rest.starts_with(b"=>") {
        j += 2;
        false
    } else if rest.starts_with(b"::") || rest.starts_with(b"==") {
        return None;
    } else if rest.starts_with(b"=") {
        j += 1;
        false
    } else if rest.starts_with(b":") {
        j += 1;
        true
    } else {
        return None;
    };
    while j < bytes.len() && is_blank(bytes[j]) {
        j += 1;
    }
    let first = *bytes.get(j)?;

    if first == b'"' || first == b'\'' {
        let open = j + 1;
        let mut k = open;
        while k < bytes.len() {
            match bytes[k] {
                b'\\' => k += 2,
                b if b == first => break,
                b'\n' => break,
                _ => k += 1,
            }
        }
        let end = k.min(bytes.len());
        return (end > open).then_some(open..end);
    }
    if first == b'{' || first == b'[' {
        // Objeto o lista: sus claves se revisan por separado.
        return None;
    }

    let lower = key.to_ascii_lowercase();
    let whole_line = header_style && (lower.contains("authorization") || lower.contains("cookie"));
    let mut end = if whole_line { line_end(bytes, j) } else { word_end(bytes, j) };
    let word = &bytes[j..end];
    if word.eq_ignore_ascii_case(b"null") {
        return None;
    }
    // `Basic dXNlcjpwYXNz` o `Bearer …` sin comillas: el esquema y el token.
    if !whole_line
        && ["basic", "bearer", "digest", "token"]
            .iter()
            .any(|scheme| word.eq_ignore_ascii_case(scheme.as_bytes()))
        && bytes.get(end) == Some(&b' ')
    {
        end = word_end(bytes, end + 1);
    }
    (end > j).then_some(j..end)
}

fn bearer_tokens(bytes: &[u8], ranges: &mut Vec<Range<usize>>) {
    const MIN_TOKEN: usize = 8;
    let lower = bytes.to_ascii_lowercase();
    let mut from = 0;
    while let Some(offset) = find(&lower[from..], b"bearer") {
        let at = from + offset;
        let mut j = at + "bearer".len();
        from = j;
        if at > 0 && is_ident(bytes[at - 1]) {
            continue;
        }
        if j >= bytes.len() || !is_blank(bytes[j]) {
            continue;
        }
        while j < bytes.len() && is_blank(bytes[j]) {
            j += 1;
        }
        let end = word_end(bytes, j);
        if end - j >= MIN_TOKEN {
            ranges.push(j..end);
            from = end;
        }
    }
}

fn url_credentials(bytes: &[u8], ranges: &mut Vec<Range<usize>>) {
    let mut from = 0;
    while let Some(offset) = find(&bytes[from..], b"://") {
        let start = from + offset + 3;
        let end = bytes[start..]
            .iter()
            .position(|&b| {
                b.is_ascii_whitespace() || matches!(b, b'/' | b'?' | b'#' | b'"' | b'\'' | b'<' | b'>')
            })
            .map_or(bytes.len(), |offset| start + offset);
        let authority = &bytes[start..end];
        if let Some(at) = authority.iter().rposition(|&b| b == b'@')
            && let Some(colon) = authority[..at].iter().position(|&b| b == b':')
            && colon + 1 < at
        {
            ranges.push(start + colon + 1..start + at);
        }
        from = end.max(start);
    }
}

fn base64_keys(bytes: &[u8], ranges: &mut Vec<Range<usize>>) {
    const MIN_LEN: usize = 32;
    let mut from = 0;
    while let Some(offset) = find(&bytes[from..], b"base64:") {
        let start = from + offset + "base64:".len();
        let end = bytes[start..]
            .iter()
            .position(|&b| !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')))
            .map_or(bytes.len(), |offset| start + offset);
        if end - start >= MIN_LEN {
            ranges.push(start..end);
        }
        from = end;
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn env_style_assignments() {
        assert_eq!(mask("DB_PASSWORD=hunter2"), "DB_PASSWORD=••••");
        assert_eq!(mask("STRIPE_SECRET=sk_live_123 next"), "STRIPE_SECRET=•••• next");
        assert_eq!(mask("token=abc&page=2"), "token=••••&page=2");
        assert_eq!(mask("DB_HOST=127.0.0.1"), "DB_HOST=127.0.0.1");
    }

    #[test]
    fn app_key_and_base64_strings() {
        let key = "base64:Q2xhdmVTdXBlclNlY3JldGFEZUVqZW1wbG8xMjM0NTY3OA==";
        assert_eq!(mask(&format!("APP_KEY={key}")), "APP_KEY=••••");
        assert_eq!(mask(&format!("la clave es {key}.")), "la clave es base64:••••.");
        assert_eq!(mask("base64:corto"), "base64:corto");
    }

    #[test]
    fn json_and_php_arrays() {
        assert_eq!(
            mask(r#"{"user":"ana","password":"p\"a ss","remember_token":"x1"}"#),
            r#"{"user":"ana","password":"••••","remember_token":"••••"}"#
        );
        assert_eq!(mask("'api_key' => 'abc123',"), "'api_key' => '••••',");
        assert_eq!(mask(r#""session": null"#), r#""session": null"#);
    }

    #[test]
    fn headers_and_bearer_tokens() {
        assert_eq!(
            mask("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.e30.abc\nHost: x"),
            "Authorization: ••••\nHost: x"
        );
        assert_eq!(
            mask("curl -H 'Bearer abcdefghijkl' https://api"),
            "curl -H 'Bearer ••••' https://api"
        );
        assert_eq!(mask("Cookie: a=1; laravel_session=xyz"), "Cookie: ••••");
        assert_eq!(mask("auth=Basic dXNlcjpwYXNz rest"), "auth=Basic dXNlcjpwYXNz rest");
        assert_eq!(mask("authorization=Basic dXNlcjpwYXNz rest"), "authorization=•••• rest");
    }

    #[test]
    fn credentials_in_urls() {
        assert_eq!(
            mask("DATABASE_URL=mysql://root:secreta@127.0.0.1:3306/app"),
            "DATABASE_URL=mysql://root:••••@127.0.0.1:3306/app"
        );
        assert_eq!(mask("ver https://laravel.com/docs"), "ver https://laravel.com/docs");
        assert_eq!(mask("redis://:pw@localhost"), "redis://:••••@localhost");
    }

    #[test]
    fn leaves_code_and_prose_alone() {
        assert_eq!(mask("Token::class"), "Token::class");
        assert_eq!(mask("if ($token == null)"), "if ($token == null)");
        assert_eq!(mask("the bearer of bad news"), "the bearer of bad news");
        assert_eq!(mask("sin secretos aquí"), "sin secretos aquí");
    }

    #[test]
    fn keeps_utf8_intact() {
        assert_eq!(mask("contraseña password=ñandú€ fin"), "contraseña password=•••• fin");
        assert_eq!(mask("ñ"), "ñ");
    }

    #[test]
    fn masks_json_values_by_key_and_by_content() {
        let mut value = json!({
            "name": "mysql",
            "Password": "x",
            "nested": { "X-Api-Key": 1234, "note": "token=abc" },
            "headers": [{ "cookie": ["a", "b"] }],
            "session": null
        });
        mask_json(&mut value);
        assert_eq!(
            value,
            json!({
                "name": "mysql",
                "Password": "••••",
                "nested": { "X-Api-Key": "••••", "note": "token=••••" },
                "headers": [{ "cookie": ["••••", "••••"] }],
                "session": null
            })
        );
    }
}
