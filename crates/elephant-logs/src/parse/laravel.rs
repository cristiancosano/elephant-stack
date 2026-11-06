//! Laravel / Monolog: `LineFormatter` multilínea y `JsonFormatter` (02 §5.3).
//!
//! Laravel activa los saltos de línea en línea, así que el contexto con una
//! excepción (`{"exception":"[object] (…)\n[stacktrace]\n#0 …"}`) lleva saltos
//! reales y no es JSON válido: la excepción se saca buscando en bytes.

use jiff::tz::TimeZone;
use memchr::memmem;
use serde_json::Value;

use super::{LogTime, Parsed, civil, digits, fraction, level_from, offset_at};
use crate::util::{first_line, unescape_json_fragment};
use crate::{ExceptionInfo, Frame, LogLevel};

const EXCEPTION_MARK: &[u8] = br#""exception":"[object] ("#;

pub(crate) struct Header {
    pub time: LogTime,
    pub channel: String,
    pub level: LogLevel,
    /// Donde empieza el mensaje dentro de la línea.
    pub message_at: usize,
}

/// `[YYYY-MM-DD(T| )HH:MM:SS(.f{1,6})?(±HH:MM|Z)?] canal.NIVEL: `.
pub(crate) fn header(line: &[u8]) -> Option<Header> {
    if line.first() != Some(&b'[') {
        return None;
    }
    let year = digits(line, 1, 4)?;
    (line.get(5) == Some(&b'-')).then_some(())?;
    let month = digits(line, 6, 2)?;
    (line.get(8) == Some(&b'-')).then_some(())?;
    let day = digits(line, 9, 2)?;
    matches!(line.get(11), Some(b' ' | b'T')).then_some(())?;
    let hour = digits(line, 12, 2)?;
    (line.get(14) == Some(&b':')).then_some(())?;
    let minute = digits(line, 15, 2)?;
    (line.get(17) == Some(&b':')).then_some(())?;
    let second = digits(line, 18, 2)?;
    let mut at = 20;
    let mut nanos = 0;
    if line.get(at) == Some(&b'.') {
        let (value, used) = fraction(line, at + 1);
        if !(1..=6).contains(&used) {
            return None;
        }
        nanos = value;
        at += 1 + used;
    }
    let mut offset = None;
    if let Some((value, used)) = offset_at(line, at) {
        offset = Some(value);
        at += used;
    }
    if line.get(at..at + 2) != Some(b"] ") {
        return None;
    }
    at += 2;
    let channel_len = line[at..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_' || **b == b'-')
        .count();
    if channel_len == 0 || line.get(at + channel_len) != Some(&b'.') {
        return None;
    }
    let channel = String::from_utf8_lossy(&line[at..at + channel_len]).into_owned();
    at += channel_len + 1;
    let level_len = line[at..]
        .iter()
        .take_while(|b| b.is_ascii_uppercase())
        .count();
    let level = level_from(&line[at..at + level_len])?;
    at += level_len;
    if line.get(at) != Some(&b':') {
        return None;
    }
    at += 1;
    match line.get(at) {
        None => {}
        Some(b' ') => at += 1,
        Some(_) => return None,
    }
    Some(Header {
        time: LogTime {
            civil: civil(year, month, day, hour, minute, second, nanos)?,
            offset,
        },
        channel,
        level,
        message_at: at,
    })
}

pub(crate) fn parse_line_format(record: &[u8], tz: &TimeZone) -> Option<Parsed> {
    let line = first_line(record);
    let header = header(line)?;
    let text = &line[header.message_at..];
    let message = match memmem::find(text, EXCEPTION_MARK) {
        // El contexto empieza en el último ` {` antes de la excepción.
        Some(at) => &text[..memmem::rfind(&text[..at], b" {").unwrap_or(at)],
        None => match json_suffix(record) {
            // Si el contexto está en la primera línea, se quita del mensaje.
            Some((at, _)) if at >= header.message_at && at <= line.len() => {
                &line[header.message_at..at]
            }
            _ => text,
        },
    };
    let message = String::from_utf8_lossy(trim_end(strip_empty_extra(message))).into_owned();
    Some(Parsed {
        ts_ms: header.time.to_ms(tz),
        level: Some(header.level),
        channel: Some(header.channel),
        message,
        exception: exception_from_line_format(record),
        http: None,
        host: None,
    })
}

fn trim_end(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(0, |i| i + 1);
    &bytes[..end]
}

fn strip_empty_extra(bytes: &[u8]) -> &[u8] {
    let mut bytes = trim_end(bytes);
    while let Some(rest) = bytes
        .strip_suffix(b" []")
        .or_else(|| bytes.strip_suffix(b" {}"))
    {
        bytes = trim_end(rest);
    }
    bytes
}

/// El contexto JSON del final de la entrada, si es válido: `(posición, valor)`.
/// Monolog lo escribe detrás del mensaje, separado por un espacio, y a veces
/// seguido del `extra` (` []`).
fn json_suffix(record: &[u8]) -> Option<(usize, Value)> {
    let body = strip_empty_extra(record);
    if !matches!(body.last(), Some(b'}' | b']')) {
        return None;
    }
    // Se prueban los candidatos de izquierda a derecha: gana el JSON más largo.
    memmem::find_iter(body, b" {")
        .chain(memmem::find_iter(body, b" ["))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .take(64)
        .find_map(|at| {
            let value: Value = serde_json::from_slice(&body[at + 1..]).ok()?;
            (value.is_object() || value.is_array()).then_some((at, value))
        })
}

/// `{"exception":"[object] (Clase(code: N): mensaje at /ruta/F.php:123)`.
fn exception_from_line_format(record: &[u8]) -> Option<ExceptionInfo> {
    let start = memmem::find(record, EXCEPTION_MARK)? + EXCEPTION_MARK.len();
    let rest = &record[start..];
    let class_end = memmem::find(rest, b"(code: ")?;
    let class = unescape_json_fragment(&String::from_utf8_lossy(&rest[..class_end]));
    let after_code = &rest[class_end..];
    let message_start = memmem::find(after_code, b"): ")? + 3;
    let tail = &after_code[message_start..];
    // Hasta la traza (o el final): «mensaje at /ruta:línea)».
    let end = memmem::find(tail, b"\n[stacktrace]")
        .or_else(|| memmem::find(tail, b"\n[previous exception]"))
        .unwrap_or(tail.len());
    let segment = trim_end(&tail[..end]);
    let segment = segment.strip_suffix(b"\"}").unwrap_or(segment);
    let segment = segment.strip_suffix(b")").unwrap_or(segment);
    let (message, file, line) = match memmem::rfind(segment, b" at ") {
        Some(at) => {
            let location = String::from_utf8_lossy(&segment[at + 4..]).into_owned();
            let (file, line) = split_location(&location);
            (&segment[..at], file, line)
        }
        None => (segment, None, None),
    };
    Some(ExceptionInfo {
        class: class.trim().to_string(),
        message: unescape_json_fragment(&String::from_utf8_lossy(message)),
        file: file.map(|f| unescape_json_fragment(&f)),
        line,
    })
}

/// `/ruta/F.php:123` → (`/ruta/F.php`, 123).
fn split_location(location: &str) -> (Option<String>, Option<u32>) {
    match location.rsplit_once(':') {
        Some((file, line)) if line.parse::<u32>().is_ok() => {
            (Some(file.to_string()), line.parse().ok())
        }
        _ => (Some(location.to_string()).filter(|l| !l.is_empty()), None),
    }
}

/// `JsonFormatter`: `datetime`, `channel`, `level_name`, `message` y
/// `context.exception`.
pub(crate) fn parse_json(line: &[u8], tz: &TimeZone) -> Option<Parsed> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let object = value.as_object()?;
    let ts_ms = object
        .get("datetime")
        .and_then(Value::as_str)
        .and_then(|t| parse_iso(t, tz));
    let level = object
        .get("level_name")
        .and_then(Value::as_str)
        .and_then(|l| level_from(l.as_bytes()));
    let exception = object
        .get("context")
        .and_then(|c| c.get("exception"))
        .and_then(Value::as_object)
        .map(|e| {
            let text = |key: &str| {
                e.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let (file, line) = split_location(&text("file"));
            ExceptionInfo {
                class: text("class"),
                message: text("message"),
                file,
                line,
            }
        });
    Some(Parsed {
        ts_ms,
        level,
        channel: object
            .get("channel")
            .and_then(Value::as_str)
            .map(str::to_string),
        message: object
            .get("message")
            .and_then(Value::as_str)
            .map(|m| m.lines().next().unwrap_or_default().to_string())
            .unwrap_or_default(),
        exception,
        http: None,
        host: None,
    })
}

/// ISO 8601 con zona; sin zona, en la del archivo.
pub(crate) fn parse_iso(text: &str, tz: &TimeZone) -> Option<i64> {
    if let Ok(ts) = text.parse::<jiff::Timestamp>() {
        return Some(ts.as_millisecond());
    }
    let civil: jiff::civil::DateTime = text.parse().ok()?;
    Some(
        tz.to_ambiguous_timestamp(civil)
            .compatible()
            .ok()?
            .as_millisecond(),
    )
}

/// El contexto para el detalle: JSON válido del final de la línea o el campo
/// `context` del `JsonFormatter`.
pub(crate) fn context_of(format: super::Format, record: &[u8]) -> Option<Value> {
    match format {
        super::Format::Laravel => {
            if memmem::find(record, EXCEPTION_MARK).is_some() {
                return None;
            }
            json_suffix(record).map(|(_, value)| value)
        }
        super::Format::LaravelJson => {
            let mut value: Value = serde_json::from_slice(first_line(record)).ok()?;
            value
                .get_mut("context")
                .map(Value::take)
                .filter(|c| !is_empty_json(c))
        }
        super::Format::NginxAccess => serde_json::from_slice(first_line(record)).ok(),
        _ => None,
    }
}

fn is_empty_json(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(o) => o.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// Frames de la primera traza (hasta `[previous exception]`). En el
/// `JsonFormatter`, de `context.exception.trace`.
pub(crate) fn frames_of(format: super::Format, record: &[u8]) -> Vec<Frame> {
    match format {
        super::Format::Laravel => {
            let end = memmem::find(record, b"\n[previous exception]").unwrap_or(record.len());
            frames_from_text(&String::from_utf8_lossy(&record[..end]))
        }
        super::Format::LaravelJson => {
            let Some(value) = serde_json::from_slice::<Value>(first_line(record)).ok() else {
                return Vec::new();
            };
            match value.pointer("/context/exception/trace") {
                Some(Value::String(text)) => frames_from_text(text),
                Some(Value::Array(lines)) => lines
                    .iter()
                    .filter_map(Value::as_str)
                    .enumerate()
                    .filter_map(|(index, location)| {
                        let (file, line) = split_location(location);
                        Some(frame(
                            u32::try_from(index).ok()?,
                            file?,
                            line.unwrap_or(0),
                            String::new(),
                        ))
                    })
                    .collect(),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// `#N /ruta(línea): llamada` y `#N [internal function]: llamada`.
fn frames_from_text(text: &str) -> Vec<Frame> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix('#')?;
            let (index, rest) = rest.split_once(' ')?;
            let index: u32 = index.parse().ok()?;
            if rest == "{main}" {
                return None;
            }
            if let Some(call) = rest.strip_prefix("[internal function]: ") {
                return Some(frame(index, "[internal function]".into(), 0, call.into()));
            }
            let (location, call) = rest.split_once("): ")?;
            let (file, line) = location.rsplit_once('(')?;
            Some(frame(
                index,
                file.into(),
                line.parse().unwrap_or(0),
                call.into(),
            ))
        })
        .collect()
}

fn frame(index: u32, file: String, line: u32, call: String) -> Frame {
    let vendor = file.contains("/vendor/");
    let framework = [
        "/vendor/laravel/framework/",
        "/vendor/symfony/",
        "/vendor/barryvdh/laravel-debugbar/",
    ]
    .iter()
    .any(|needle| file.contains(needle));
    Frame {
        index,
        file,
        line,
        call,
        vendor,
        framework,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Format;

    fn utc() -> TimeZone {
        TimeZone::UTC
    }

    const EXCEPTION: &str = r#"[2026-11-06 10:20:30] local.ERROR: No query results for model [App\Models\Order] 7 {"userId":3,"exception":"[object] (Illuminate\\Database\\Eloquent\\ModelNotFoundException(code: 0): No query results for model [App\\Models\\Order] 7 at /code/tienda/vendor/laravel/framework/src/Illuminate/Database/Eloquent/Builder.php:621)
[stacktrace]
#0 /code/tienda/app/Http/Controllers/OrderController.php(42): Illuminate\\Database\\Eloquent\\Builder->findOrFail()
#1 [internal function]: App\\Http\\Controllers\\OrderController->show()
#2 /code/tienda/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): call_user_func_array()
#3 {main}

[previous exception] [object] (PDOException(code: 42S02): Tabla at /code/tienda/app/X.php:1)
[stacktrace]
#0 /code/tienda/app/Y.php(9): z()
"} "#;

    #[test]
    fn headers_accept_every_documented_variant() {
        assert!(header(b"[2026-11-06 10:20:30] local.ERROR: x").is_some());
        assert!(header(b"[2026-11-06T10:20:30.123456+02:00] production.INFO: x").is_some());
        assert!(header(b"[2026-11-06 10:20:30Z] my-chan_1.DEBUG:").is_some());
        assert!(header(b"[2026-11-06 10:20:30] local.FATAL: x").is_none());
        assert!(header(b"[2026-11-06 10:20:30] local.error: x").is_none());
        assert!(header(b"[stacktrace]").is_none());
        assert!(header(b"#0 /a.php(1): b()").is_none());
        assert!(header(b"[2026-11-06 10:20:30.1234567] local.INFO: x").is_none());
    }

    #[test]
    fn offsets_in_the_header_win_over_the_project_timezone() {
        let madrid = TimeZone::get("Europe/Madrid").unwrap();
        let with_offset = header(b"[2026-11-06 10:00:00+00:00] local.INFO: x").unwrap();
        let plain = header(b"[2026-11-06 10:00:00] local.INFO: x").unwrap();
        assert_eq!(with_offset.time.to_ms(&madrid), plain.time.to_ms(&utc()));
        assert_eq!(
            plain.time.to_ms(&madrid).unwrap() + 3_600_000,
            plain.time.to_ms(&utc()).unwrap()
        );
    }

    #[test]
    fn exceptions_are_extracted_from_the_inline_context() {
        let parsed = parse_line_format(EXCEPTION.as_bytes(), &utc()).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Error));
        assert_eq!(parsed.channel.as_deref(), Some("local"));
        assert_eq!(
            parsed.message,
            "No query results for model [App\\Models\\Order] 7"
        );
        let exception = parsed.exception.unwrap();
        assert_eq!(
            exception.class,
            "Illuminate\\Database\\Eloquent\\ModelNotFoundException"
        );
        assert_eq!(
            exception.message,
            "No query results for model [App\\Models\\Order] 7"
        );
        assert_eq!(
            exception.file.as_deref(),
            Some(
                "/code/tienda/vendor/laravel/framework/src/Illuminate/Database/Eloquent/Builder.php"
            )
        );
        assert_eq!(exception.line, Some(621));
        assert_eq!(parsed.ts_ms, Some(1_793_960_430_000));
    }

    #[test]
    fn frames_come_from_the_first_trace_and_mark_vendor() {
        let frames = frames_of(Format::Laravel, EXCEPTION.as_bytes());
        assert_eq!(frames.len(), 3);
        assert_eq!(
            frames[0].file,
            "/code/tienda/app/Http/Controllers/OrderController.php"
        );
        assert_eq!(frames[0].line, 42);
        assert!(!frames[0].vendor);
        assert_eq!(frames[1].file, "[internal function]");
        assert!(frames[2].vendor && frames[2].framework);
    }

    #[test]
    fn single_line_context_is_split_from_the_message() {
        let record =
            br#"[2026-11-06 10:20:30] local.INFO: Pedido creado {"order":7,"items":[1,2]} []"#;
        let parsed = parse_line_format(record, &utc()).unwrap();
        assert_eq!(parsed.message, "Pedido creado");
        let context = context_of(Format::Laravel, record).unwrap();
        assert_eq!(context["order"], 7);
        assert!(
            parse_line_format(b"[2026-11-06 10:20:30] local.INFO: Sin contexto", &utc()).is_some()
        );
        assert!(
            context_of(
                Format::Laravel,
                b"[2026-11-06 10:20:30] local.INFO: Sin contexto"
            )
            .is_none()
        );
        assert!(context_of(Format::Laravel, EXCEPTION.as_bytes()).is_none());
    }

    #[test]
    fn json_formatter_entries_are_parsed() {
        let line = br#"{"message":"Fallo\nsegunda","context":{"exception":{"class":"RuntimeException","message":"Fallo","code":0,"file":"/code/app/X.php:12","trace":["/code/app/Y.php:3","/code/vendor/symfony/z.php:9"]}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-11-06T10:20:30.123456+00:00","extra":{}}"#;
        let parsed = parse_json(line, &utc()).unwrap();
        assert_eq!(parsed.message, "Fallo");
        assert_eq!(parsed.level, Some(LogLevel::Error));
        assert_eq!(parsed.ts_ms, Some(1_793_960_430_123));
        let exception = parsed.exception.unwrap();
        assert_eq!(
            (exception.file.as_deref(), exception.line),
            (Some("/code/app/X.php"), Some(12))
        );
        let frames = frames_of(Format::LaravelJson, line);
        assert_eq!(frames.len(), 2);
        assert!(frames[1].framework);
        assert_eq!(
            context_of(Format::LaravelJson, line).unwrap()["exception"]["code"],
            0
        );
    }
}
