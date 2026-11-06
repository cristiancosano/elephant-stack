//! Parsers escritos a mano con `memchr`, uno por formato (02 §5.3).
//!
//! Cada formato sabe dos cosas: si una línea empieza una entrada
//! ([`Format::is_header`]) y cómo interpretar una entrada completa (la cabecera
//! más sus líneas de continuación). Lo demás (leer, fusionar, filtrar) no
//! depende del formato.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone};

use crate::util::{fnv1a, hex16, normalize_message, normalize_path, trim_newline, truncate_utf8};
use crate::{ExceptionInfo, HttpInfo, LogEntry, LogLevel, Project, SourceKind, projects};

mod laravel;
mod nginx_access;
mod nginx_error;
mod php_fpm;

pub(crate) use laravel::{context_of, frames_of};

/// Como mucho 2 KB de mensaje.
const MAX_MESSAGE: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    /// `LineFormatter` de Monolog: cabecera y líneas de continuación.
    Laravel,
    /// `JsonFormatter`: una entrada por línea.
    LaravelJson,
    NginxError,
    /// JSON de Elephant o `combined`: una entrada por línea.
    NginxAccess,
    PhpFpm,
}

impl Format {
    /// `first_line` es la primera línea no vacía del archivo: si empieza por
    /// `{`, el log de Laravel es JSON.
    pub(crate) fn detect(kind: SourceKind, first_line: &[u8]) -> Self {
        match kind {
            SourceKind::LaravelApp if first_line.first() == Some(&b'{') => Self::LaravelJson,
            SourceKind::LaravelApp => Self::Laravel,
            SourceKind::NginxError => Self::NginxError,
            SourceKind::NginxAccess => Self::NginxAccess,
            SourceKind::PhpFpm => Self::PhpFpm,
        }
    }

    pub(crate) fn is_header(self, line: &[u8]) -> bool {
        let line = trim_newline(line);
        match self {
            Self::Laravel => laravel::header(line).is_some(),
            Self::LaravelJson | Self::NginxAccess => !line.iter().all(u8::is_ascii_whitespace),
            Self::NginxError => nginx_error::is_header(line),
            Self::PhpFpm => php_fpm::is_header(line),
        }
    }

    /// Solo la hora de la cabecera, para la búsqueda binaria.
    pub(crate) fn header_ts(self, line: &[u8], tz: &TimeZone) -> Option<i64> {
        let line = trim_newline(line);
        match self {
            Self::Laravel => laravel::header(line).and_then(|h| h.time.to_ms(tz)),
            Self::LaravelJson => laravel::parse_json(line, tz).and_then(|p| p.ts_ms),
            Self::NginxError => nginx_error::header_time(line).and_then(|t| t.to_ms(tz)),
            Self::NginxAccess => nginx_access::parse(line, tz).and_then(|p| p.ts_ms),
            Self::PhpFpm => php_fpm::header_time(line).and_then(|t| t.to_ms(tz)),
        }
    }

    fn parse(self, record: &[u8], tz: &TimeZone) -> Option<Parsed> {
        match self {
            Self::Laravel => laravel::parse_line_format(record, tz),
            Self::LaravelJson => laravel::parse_json(trim_newline(record), tz),
            Self::NginxError => nginx_error::parse(record, tz),
            Self::NginxAccess => nginx_access::parse(trim_newline(record), tz),
            Self::PhpFpm => php_fpm::parse(record, tz),
        }
    }
}

/// Lo que hace falta para convertir una entrada de un archivo en `LogEntry`.
#[derive(Debug)]
pub(crate) struct FileCtx {
    pub path: PathBuf,
    pub key: u64,
    pub kind: SourceKind,
    pub format: Format,
    /// `None` = archivo global: el proyecto sale de cada línea.
    pub project: Option<String>,
    /// Raíz del proyecto, para las huellas de las excepciones.
    pub root: Option<PathBuf>,
    pub tz: TimeZone,
    /// Para asignar las líneas de nginx a su proyecto por `host`.
    pub projects: Arc<Vec<Project>>,
    /// Hora de las líneas sueltas sin cabecera.
    pub mtime_ms: i64,
}

/// Una hora tal como viene en el log: sin zona (se aplica la del archivo) o
/// con desfase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LogTime {
    pub civil: DateTime,
    pub offset: Option<Offset>,
}

impl LogTime {
    pub(crate) fn to_ms(self, tz: &TimeZone) -> Option<i64> {
        let ts = match self.offset {
            Some(offset) => offset.to_timestamp(self.civil).ok()?,
            None => tz.to_ambiguous_timestamp(self.civil).compatible().ok()?,
        };
        Some(ts.as_millisecond())
    }
}

/// Resultado de un parser, antes de asignar id, proyecto y huella.
#[derive(Debug, Default)]
pub(crate) struct Parsed {
    pub ts_ms: Option<i64>,
    pub level: Option<LogLevel>,
    pub channel: Option<String>,
    pub message: String,
    pub exception: Option<ExceptionInfo>,
    pub http: Option<HttpInfo>,
    pub host: Option<String>,
}

pub(crate) fn entry_id(key: u64, offset: u64) -> String {
    format!("{}-{offset:x}", hex16(key))
}

/// `<file_key:016x>-<offset:x>` → `(file_key, offset)`.
pub(crate) fn parse_entry_id(id: &str) -> Option<(u64, u64)> {
    let (key, offset) = id.trim().split_once('-')?;
    if key.len() != 16 || offset.is_empty() || offset.len() > 16 {
        return None;
    }
    Some((
        u64::from_str_radix(key, 16).ok()?,
        u64::from_str_radix(offset, 16).ok()?,
    ))
}

/// Convierte una entrada completa en `LogEntry`. Lo que no se reconoce no se
/// pierde: queda como `Info` con la hora de modificación del archivo.
pub(crate) fn build_entry(ctx: &FileCtx, offset: u64, record: &[u8]) -> LogEntry {
    let body = trim_newline(record);
    let parsed = ctx.format.parse(body, &ctx.tz).unwrap_or_else(|| Parsed {
        message: String::from_utf8_lossy(crate::util::first_line(body)).into_owned(),
        ..Parsed::default()
    });
    let level = parsed.level.unwrap_or(LogLevel::Info);
    let project = ctx
        .project
        .clone()
        .or_else(|| match (ctx.kind, &parsed.host) {
            (SourceKind::NginxError | SourceKind::NginxAccess, Some(host)) => {
                projects::project_for_host(&ctx.projects, host).map(|p| p.id.clone())
            }
            _ => None,
        });
    let fingerprint = fingerprint(ctx, level, &parsed, project.as_deref());
    LogEntry {
        id: entry_id(ctx.key, offset),
        kind: ctx.kind,
        project,
        host: parsed.host,
        ts_ms: parsed.ts_ms.unwrap_or(ctx.mtime_ms),
        level,
        channel: parsed.channel,
        message: truncate_utf8(&parsed.message, MAX_MESSAGE).to_string(),
        exception: parsed.exception,
        http: parsed.http,
        fingerprint,
        len: u32::try_from(body.len()).unwrap_or(u32::MAX),
    }
}

fn fingerprint(ctx: &FileCtx, level: LogLevel, parsed: &Parsed, project: Option<&str>) -> String {
    let mut key = String::new();
    if let Some(exception) = &parsed.exception {
        let root = project
            .and_then(|id| ctx.projects.iter().find(|p| p.id == id))
            .map(|p| p.root.as_path())
            .or(ctx.root.as_deref());
        let file = exception
            .file
            .as_deref()
            .map(|f| relative_to(f, root))
            .unwrap_or_default();
        key.push_str(&exception.class);
        key.push('\0');
        key.push_str(&file);
        key.push('\0');
        key.push_str(&exception.line.unwrap_or_default().to_string());
    } else if let (SourceKind::NginxAccess, Some(http)) = (ctx.kind, &parsed.http) {
        key = format!(
            "{} {} {}",
            http.method,
            http.status,
            normalize_path(&http.path)
        );
    } else {
        key = format!("{level:?}\0{}", normalize_message(&parsed.message));
    }
    hex16(fnv1a(key.as_bytes()))
}

fn relative_to(file: &str, root: Option<&Path>) -> String {
    match root.and_then(|root| Path::new(file).strip_prefix(root).ok()) {
        Some(relative) => relative.display().to_string(),
        None => file.to_string(),
    }
}

/// `warn` → `Warning`, `ERROR` → `Error`…; vale para nginx, PHP-FPM y Monolog.
pub(crate) fn level_from(word: &[u8]) -> Option<LogLevel> {
    let lower = word.to_ascii_lowercase();
    Some(match lower.as_slice() {
        b"debug" => LogLevel::Debug,
        b"info" => LogLevel::Info,
        b"notice" => LogLevel::Notice,
        b"warn" | b"warning" => LogLevel::Warning,
        b"error" => LogLevel::Error,
        b"crit" | b"critical" => LogLevel::Critical,
        b"alert" => LogLevel::Alert,
        b"emerg" | b"emergency" => LogLevel::Emergency,
        _ => return None,
    })
}

/// Número de `n` dígitos ASCII exactos en `bytes[at..]`.
pub(crate) fn digits(bytes: &[u8], at: usize, n: usize) -> Option<i64> {
    let slice = bytes.get(at..at + n)?;
    let mut value: i64 = 0;
    for byte in slice {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i64::from(byte - b'0');
    }
    Some(value)
}

pub(crate) fn civil(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    nanos: i64,
) -> Option<DateTime> {
    DateTime::new(
        i16::try_from(year).ok()?,
        i8::try_from(month).ok()?,
        i8::try_from(day).ok()?,
        i8::try_from(hour).ok()?,
        i8::try_from(minute).ok()?,
        i8::try_from(second).ok()?,
        i32::try_from(nanos).ok()?,
    )
    .ok()
}

/// `.f{1,9}` a partir de `at` (sin el punto): `(nanosegundos, bytes leídos)`.
pub(crate) fn fraction(bytes: &[u8], at: usize) -> (i64, usize) {
    let count = bytes[at.min(bytes.len())..]
        .iter()
        .take(9)
        .take_while(|b| b.is_ascii_digit())
        .count();
    let mut nanos: i64 = 0;
    for byte in &bytes[at..at + count] {
        nanos = nanos * 10 + i64::from(byte - b'0');
    }
    for _ in count..9 {
        nanos *= 10;
    }
    (nanos, count)
}

/// `Z`, `±HH:MM` o `±HHMM` en `bytes[at..]`: `(desfase, bytes leídos)`.
pub(crate) fn offset_at(bytes: &[u8], at: usize) -> Option<(Offset, usize)> {
    match bytes.get(at)? {
        b'Z' => Some((Offset::UTC, 1)),
        sign @ (b'+' | b'-') => {
            let hours = digits(bytes, at + 1, 2)?;
            let (minutes, used) = if bytes.get(at + 3) == Some(&b':') {
                (digits(bytes, at + 4, 2)?, 6)
            } else {
                (digits(bytes, at + 3, 2)?, 5)
            };
            let mut seconds = i32::try_from(hours * 3600 + minutes * 60).ok()?;
            if *sign == b'-' {
                seconds = -seconds;
            }
            Some((Offset::from_seconds(seconds).ok()?, used))
        }
        _ => None,
    }
}

/// `Jan` → 1 … `Dec` → 12.
pub(crate) fn month_from(name: &[u8]) -> Option<i64> {
    const MONTHS: [&[u8]; 12] = [
        b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep", b"oct", b"nov",
        b"dec",
    ];
    let lower = name.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|m| *m == lower.as_slice())
        .map(|i| i as i64 + 1)
}

/// `GET /ruta HTTP/1.1` → `("GET", "/ruta")`.
pub(crate) fn split_request(request: &str) -> Option<(String, String)> {
    let mut parts = request.split(' ');
    let method = parts
        .next()
        .filter(|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_uppercase()))?;
    let path = parts.next().unwrap_or("/");
    Some((method.to_string(), path.to_string()))
}

#[cfg(test)]
pub(crate) mod testctx {
    use super::*;

    pub fn ctx(kind: SourceKind, format: Format, tz: &str) -> FileCtx {
        FileCtx {
            path: PathBuf::from("/tmp/x.log"),
            key: 0xabc,
            kind,
            format,
            project: None,
            root: None,
            tz: TimeZone::get(tz).unwrap(),
            projects: Arc::new(Vec::new()),
            mtime_ms: 42,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectKind, ProjectOrigin};

    #[test]
    fn entry_ids_round_trip() {
        let id = entry_id(0x00ab_cdef_0123_4567, 0x1f);
        assert_eq!(id, "00abcdef01234567-1f");
        assert_eq!(parse_entry_id(&id), Some((0x00ab_cdef_0123_4567, 0x1f)));
        assert_eq!(parse_entry_id("../../.env"), None);
        assert_eq!(parse_entry_id("00abcdef01234567-"), None);
        assert_eq!(parse_entry_id("xyz-1"), None);
    }

    #[test]
    fn levels_map_from_every_format() {
        assert_eq!(level_from(b"warn"), Some(LogLevel::Warning));
        assert_eq!(level_from(b"CRITICAL"), Some(LogLevel::Critical));
        assert_eq!(level_from(b"emerg"), Some(LogLevel::Emergency));
        assert_eq!(level_from(b"fatal"), None);
    }

    #[test]
    fn offsets_parse_with_and_without_colon() {
        assert_eq!(
            offset_at(b"+02:00", 0).unwrap(),
            (Offset::from_seconds(7200).unwrap(), 6)
        );
        assert_eq!(
            offset_at(b"-0130", 0).unwrap(),
            (Offset::from_seconds(-5400).unwrap(), 5)
        );
        assert_eq!(offset_at(b"Z", 0).unwrap().0, Offset::UTC);
        assert!(offset_at(b"x", 0).is_none());
    }

    #[test]
    fn unknown_records_are_kept_as_info() {
        let ctx = testctx::ctx(SourceKind::LaravelApp, Format::Laravel, "UTC");
        let entry = build_entry(&ctx, 0, b"una linea suelta\notra\n");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.ts_ms, 42);
        assert_eq!(entry.message, "una linea suelta");
        assert_eq!(entry.len, 21);
    }

    #[test]
    fn nginx_lines_are_assigned_by_host() {
        let mut ctx = testctx::ctx(SourceKind::NginxError, Format::NginxError, "Europe/Madrid");
        ctx.projects = Arc::new(vec![Project {
            id: "p1".into(),
            name: "tienda".into(),
            root: PathBuf::from("/code/tienda"),
            hosts: vec!["tienda.test".into()],
            kind: ProjectKind::Laravel,
            origin: ProjectOrigin::HerdParked,
            php_version: None,
            log_tz: "UTC".into(),
        }]);
        let known = br#"2026/11/06 10:00:00 [error] 1#1: *2 fallo, client: 127.0.0.1, server: , request: "GET / HTTP/1.1", host: "tienda.test""#;
        let unknown = br#"2026/11/06 10:00:00 [error] 1#1: *2 fallo, client: 127.0.0.1, server: , request: "GET / HTTP/1.1", host: "otra.test""#;
        assert_eq!(build_entry(&ctx, 0, known).project.as_deref(), Some("p1"));
        let entry = build_entry(&ctx, 0, unknown);
        assert_eq!(entry.project, None);
        assert_eq!(entry.host.as_deref(), Some("otra.test"));
    }

    #[test]
    fn exception_fingerprints_ignore_the_message_and_the_root() {
        let mut ctx = testctx::ctx(SourceKind::LaravelApp, Format::Laravel, "UTC");
        ctx.root = Some(PathBuf::from("/code/tienda"));
        let a = br#"[2026-11-06 10:00:00] local.ERROR: Pedido 1 {"exception":"[object] (RuntimeException(code: 0): Pedido 1 at /code/tienda/app/X.php:10)"}"#;
        let b = br#"[2026-11-06 11:00:00] local.ERROR: Pedido 2 {"exception":"[object] (RuntimeException(code: 0): Pedido 2 at /code/tienda/app/X.php:10)"}"#;
        assert_eq!(
            build_entry(&ctx, 0, a).fingerprint,
            build_entry(&ctx, 99, b).fingerprint
        );
        let mut other_root = testctx::ctx(SourceKind::LaravelApp, Format::Laravel, "UTC");
        other_root.root = Some(PathBuf::from("/otra/tienda"));
        let c = br#"[2026-11-06 10:00:00] local.ERROR: x {"exception":"[object] (RuntimeException(code: 0): x at /otra/tienda/app/X.php:10)"}"#;
        assert_eq!(
            build_entry(&ctx, 0, a).fingerprint,
            build_entry(&other_root, 0, c).fingerprint
        );
    }

    #[test]
    fn messages_are_capped_at_two_kilobytes() {
        let ctx = testctx::ctx(SourceKind::LaravelApp, Format::Laravel, "UTC");
        let line = format!("[2026-11-06 10:00:00] local.INFO: {}", "ñ".repeat(3000));
        let entry = build_entry(&ctx, 0, line.as_bytes());
        assert!(entry.message.len() <= MAX_MESSAGE);
        assert!(entry.message.starts_with('ñ'));
    }
}
