//! nginx access: el JSON de Elephant (02 §8) y, de respaldo, `combined`:
//! `ip - user [dd/Mon/yyyy:HH:MM:SS ±zzzz] "M ruta HTTP/x" status bytes "ref" "ua"`.
//! El nivel sale del estado: 5xx → Error, 4xx → Warning, el resto → Info.

use jiff::tz::TimeZone;
use serde_json::Value;

use super::laravel::parse_iso;
use super::{LogTime, Parsed, civil, digits, month_from, offset_at, split_request};
use crate::{HttpInfo, LogLevel};

pub(crate) fn parse(line: &[u8], tz: &TimeZone) -> Option<Parsed> {
    if line.first() == Some(&b'{') {
        parse_json(line, tz)
    } else {
        parse_combined(line, tz)
    }
}

fn level_for(status: u16) -> LogLevel {
    match status {
        500.. => LogLevel::Error,
        400..=499 => LogLevel::Warning,
        _ => LogLevel::Info,
    }
}

/// Segundos con decimales → milisegundos. `urt` puede ser `-` o una lista
/// separada por comas: se suma.
fn seconds_to_ms(value: Option<&Value>) -> Option<u32> {
    let seconds = match value? {
        Value::Number(n) => n.as_f64()?,
        Value::String(text) => {
            let parts: Vec<f64> = text
                .split([',', ':'])
                .filter_map(|part| part.trim().parse::<f64>().ok())
                .collect();
            if parts.is_empty() {
                return None;
            }
            parts.iter().sum()
        }
        _ => return None,
    };
    (seconds >= 0.0).then(|| (seconds * 1000.0).round().min(f64::from(u32::MAX)) as u32)
}

fn parse_json(line: &[u8], tz: &TimeZone) -> Option<Parsed> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let status = value
        .get("s")
        .and_then(Value::as_u64)
        .and_then(|s| u16::try_from(s).ok())?;
    let method = text("m").unwrap_or_default();
    let path = text("u").unwrap_or_else(|| "/".into());
    Some(Parsed {
        ts_ms: text("t").and_then(|t| parse_iso(&t, tz)),
        level: Some(level_for(status)),
        channel: None,
        message: format!("{method} {path} {status}"),
        exception: None,
        http: Some(HttpInfo {
            method,
            path,
            status,
            bytes: value.get("b").and_then(Value::as_u64),
            duration_ms: seconds_to_ms(value.get("rt")),
            upstream_ms: seconds_to_ms(value.get("urt")),
            ip: text("ip"),
            ua: text("ua").filter(|ua| !ua.is_empty() && ua != "-"),
        }),
        host: text("h").filter(|h| !h.is_empty()),
    })
}

fn parse_combined(line: &[u8], tz: &TimeZone) -> Option<Parsed> {
    let text = String::from_utf8_lossy(line);
    let (ip, rest) = text.split_once(' ')?;
    let open = rest.find('[')?;
    let close = rest[open..].find(']')? + open;
    let time = combined_time(&rest.as_bytes()[open + 1..close])?;
    let rest = rest[close + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let request_end = rest.find('"')?;
    let (method, path) =
        split_request(&rest[..request_end]).unwrap_or_else(|| ("-".into(), "/".into()));
    let mut fields = rest[request_end + 1..].split_whitespace();
    let status: u16 = fields.next()?.parse().ok()?;
    let bytes = fields.next().and_then(|b| b.parse().ok());
    let quoted: Vec<&str> = rest[request_end + 1..]
        .split('"')
        .skip(1)
        .step_by(2)
        .collect();
    let ua = quoted
        .get(1)
        .map(|ua| ua.to_string())
        .filter(|ua| !ua.is_empty() && ua != "-");
    Some(Parsed {
        ts_ms: time.to_ms(tz),
        level: Some(level_for(status)),
        channel: None,
        message: format!("{method} {path} {status}"),
        exception: None,
        http: Some(HttpInfo {
            method,
            path,
            status,
            bytes,
            duration_ms: None,
            upstream_ms: None,
            ip: Some(ip.to_string()),
            ua,
        }),
        host: None,
    })
}

/// `06/Nov/2026:10:20:30 +0100`.
fn combined_time(bytes: &[u8]) -> Option<LogTime> {
    let day = digits(bytes, 0, 2)?;
    let month = month_from(bytes.get(3..6)?)?;
    let year = digits(bytes, 7, 4)?;
    let hour = digits(bytes, 12, 2)?;
    let minute = digits(bytes, 15, 2)?;
    let second = digits(bytes, 18, 2)?;
    let offset = offset_at(bytes, 21).map(|(offset, _)| offset);
    Some(LogTime {
        civil: civil(year, month, day, hour, minute, second, 0)?,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elephant_json_lines_are_parsed() {
        let line = br#"{"t":"2026-11-06T10:20:30+01:00","h":"tienda.test","m":"GET","u":"/orders/7?x=1","s":502,"b":157,"rt":1.234,"urt":"0.500, 0.250","ip":"127.0.0.1","ua":"curl/8"}"#;
        let parsed = parse(line, &TimeZone::UTC).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Error));
        assert_eq!(parsed.host.as_deref(), Some("tienda.test"));
        assert_eq!(parsed.ts_ms, Some(1_793_956_830_000));
        let http = parsed.http.unwrap();
        assert_eq!(
            (http.status, http.bytes, http.duration_ms, http.upstream_ms),
            (502, Some(157), Some(1234), Some(750))
        );
        assert_eq!(parsed.message, "GET /orders/7?x=1 502");
        let missing = br#"{"t":"2026-11-06T10:20:30+01:00","h":"x","m":"GET","u":"/","s":200,"b":0,"rt":0.001,"urt":"-","ip":"::1","ua":""}"#;
        let parsed = parse(missing, &TimeZone::UTC).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Info));
        assert_eq!(parsed.http.unwrap().upstream_ms, None);
    }

    #[test]
    fn combined_lines_are_the_fallback() {
        let line = br#"192.168.1.5 - - [06/Nov/2026:10:20:30 +0100] "POST /login HTTP/1.1" 404 512 "-" "Mozilla/5.0 (Mac)""#;
        let parsed = parse(line, &TimeZone::UTC).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Warning));
        assert_eq!(parsed.ts_ms, Some(1_793_956_830_000));
        let http = parsed.http.unwrap();
        assert_eq!(
            (http.method.as_str(), http.path.as_str(), http.status),
            ("POST", "/login", 404)
        );
        assert_eq!(http.ua.as_deref(), Some("Mozilla/5.0 (Mac)"));
        assert!(parse(b"basura", &TimeZone::UTC).is_none());
    }
}
