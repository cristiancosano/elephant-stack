//! nginx error (Herd y, más adelante, el de Elephant):
//! `YYYY/MM/DD HH:MM:SS [nivel] pid#tid: *cid mensaje, client: …, server: …,
//! request: "…", upstream: "…", host: "…"`.
//!
//! Los campos se quitan desde el final, porque el mensaje puede tener comas.

use jiff::tz::TimeZone;

use super::{LogTime, Parsed, civil, digits, level_from, split_request};
use crate::HttpInfo;
use crate::util::first_line;

const FIELDS: [&str; 7] = [
    "host",
    "referrer",
    "upstream",
    "request",
    "server",
    "client",
    "subrequest",
];

pub(crate) fn header_time(line: &[u8]) -> Option<LogTime> {
    let year = digits(line, 0, 4)?;
    (line.get(4) == Some(&b'/')).then_some(())?;
    let month = digits(line, 5, 2)?;
    (line.get(7) == Some(&b'/')).then_some(())?;
    let day = digits(line, 8, 2)?;
    (line.get(10) == Some(&b' ')).then_some(())?;
    let hour = digits(line, 11, 2)?;
    (line.get(13) == Some(&b':')).then_some(())?;
    let minute = digits(line, 14, 2)?;
    (line.get(16) == Some(&b':')).then_some(())?;
    let second = digits(line, 17, 2)?;
    Some(LogTime {
        civil: civil(year, month, day, hour, minute, second, 0)?,
        offset: None,
    })
}

pub(crate) fn is_header(line: &[u8]) -> bool {
    header_time(line).is_some() && line.get(19..21) == Some(b" [")
}

pub(crate) fn parse(record: &[u8], tz: &TimeZone) -> Option<Parsed> {
    let line = first_line(record);
    let time = header_time(line)?;
    let rest = line.get(21..)?;
    let close = memchr::memchr(b']', rest)?;
    let level = level_from(&rest[..close]);
    let mut text = String::from_utf8_lossy(rest.get(close + 1..).unwrap_or_default())
        .trim_start()
        .to_string();
    // `pid#tid: ` y `*cid `.
    if let Some((prefix, after)) = text.split_once(": ")
        && prefix.contains('#')
        && prefix.bytes().all(|b| b.is_ascii_digit() || b == b'#')
    {
        text = after.to_string();
    }
    if let Some(after) = text.strip_prefix('*')
        && let Some((cid, after)) = after.split_once(' ')
        && cid.bytes().all(|b| b.is_ascii_digit())
    {
        text = after.to_string();
    }
    let (message, fields) = split_fields(&text);
    let field = |name: &str| {
        fields
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
    };
    let http = field("request")
        .and_then(|r| split_request(&r))
        .map(|(method, path)| HttpInfo {
            method,
            path,
            // El log de errores no trae el estado: 0 = desconocido.
            status: 0,
            bytes: None,
            duration_ms: None,
            upstream_ms: None,
            ip: field("client"),
            ua: None,
        });
    Some(Parsed {
        ts_ms: time.to_ms(tz),
        level,
        channel: None,
        message,
        exception: None,
        http,
        host: field("host").filter(|h| !h.is_empty()),
    })
}

/// Quita `, clave: valor` del final mientras la clave sea conocida. Se busca
/// el marcador completo (`, request:`) y no cualquier coma, así que las comas
/// del mensaje y de los valores no cortan nada.
fn split_fields(text: &str) -> (String, Vec<(&'static str, String)>) {
    let mut fields = Vec::new();
    let mut end = text.len();
    loop {
        let last = FIELDS
            .iter()
            .filter_map(|name| {
                text[..end]
                    .rfind(&format!(", {name}:"))
                    .map(|at| (at, *name))
            })
            .max_by_key(|(at, _)| *at);
        let Some((at, name)) = last else { break };
        let value = text[at + name.len() + 3..end]
            .trim()
            .trim_matches('"')
            .to_string();
        fields.push((name, value));
        end = at;
    }
    (text[..end].trim().to_string(), fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogLevel;

    const HERD: &str = r#"2026/08/11 19:25:41 [error] 18493#1061392: *88 connect() to unix:/Users/u/Library/Application Support/Herd/herd.sock failed (61: Connection refused) while connecting to upstream, client: 127.0.0.1, server: , request: "POST /api/v1/auth/validate HTTP/1.1", upstream: "fastcgi://unix:/Users/u/Library/Application Support/Herd/herd.sock:", host: "tienda.test""#;

    #[test]
    fn herd_lines_are_parsed_from_the_end() {
        let tz = TimeZone::get("Europe/Madrid").unwrap();
        let parsed = parse(HERD.as_bytes(), &tz).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Error));
        assert_eq!(parsed.host.as_deref(), Some("tienda.test"));
        assert!(parsed.message.starts_with("connect() to unix:"));
        assert!(parsed.message.ends_with("while connecting to upstream"));
        let http = parsed.http.unwrap();
        assert_eq!(
            (http.method.as_str(), http.path.as_str()),
            ("POST", "/api/v1/auth/validate")
        );
        assert_eq!(http.ip.as_deref(), Some("127.0.0.1"));
        // 19:25:41 en Madrid (verano) = 17:25:41 UTC.
        assert_eq!(parsed.ts_ms, Some(1_786_469_141_000));
    }

    #[test]
    fn startup_lines_without_connection_are_valid() {
        let line = b"2026/11/06 10:00:00 [notice] 123#0: signal process started";
        assert!(is_header(line));
        let parsed = parse(line, &TimeZone::UTC).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Notice));
        assert_eq!(parsed.message, "signal process started");
        assert!(parsed.http.is_none() && parsed.host.is_none());
    }

    #[test]
    fn commas_in_the_message_are_kept() {
        let line = br#"2026/11/06 10:00:00 [warn] 1#1: *5 a, b, c, client: 10.0.0.1, server: x, request: "GET /a,b HTTP/1.1", host: "h.test""#;
        let parsed = parse(line, &TimeZone::UTC).unwrap();
        assert_eq!(parsed.message, "a, b, c");
        assert_eq!(parsed.http.unwrap().path, "/a,b");
        assert_eq!(parsed.host.as_deref(), Some("h.test"));
        assert!(!is_header(b"2026/11/06 10:00:00 sin nivel"));
    }
}
