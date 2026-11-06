//! PHP-FPM: `[DD-Mon-YYYY HH:MM:SS] NIVEL: mensaje`. Las líneas
//! `WARNING: [pool x] child N said into stderr: "…"` se conservan tal cual.

use jiff::tz::TimeZone;

use super::{LogTime, Parsed, civil, digits, fraction, level_from, month_from};
use crate::util::first_line;

/// `(hora, posición tras el corchete de cierre)`.
fn header(line: &[u8]) -> Option<(LogTime, usize)> {
    if line.first() != Some(&b'[') {
        return None;
    }
    let day = digits(line, 1, 2)?;
    (line.get(3) == Some(&b'-')).then_some(())?;
    let month = month_from(line.get(4..7)?)?;
    (line.get(7) == Some(&b'-')).then_some(())?;
    let year = digits(line, 8, 4)?;
    (line.get(12) == Some(&b' ')).then_some(())?;
    let hour = digits(line, 13, 2)?;
    let minute = digits(line, 16, 2)?;
    let second = digits(line, 19, 2)?;
    let mut at = 21;
    let mut nanos = 0;
    if line.get(at) == Some(&b'.') {
        let (value, used) = fraction(line, at + 1);
        nanos = value;
        at += 1 + used;
    }
    (line.get(at) == Some(&b']')).then_some(())?;
    Some((
        LogTime {
            civil: civil(year, month, day, hour, minute, second, nanos)?,
            offset: None,
        },
        at + 1,
    ))
}

pub(crate) fn header_time(line: &[u8]) -> Option<LogTime> {
    header(line).map(|(time, _)| time)
}

pub(crate) fn is_header(line: &[u8]) -> bool {
    header(line).is_some()
}

pub(crate) fn parse(record: &[u8], tz: &TimeZone) -> Option<Parsed> {
    let line = first_line(record);
    let (time, at) = header(line)?;
    let rest = String::from_utf8_lossy(&line[at..]).trim().to_string();
    let (level, message) = match rest.split_once(": ") {
        Some((word, message)) if level_from(word.as_bytes()).is_some() => {
            (level_from(word.as_bytes()), message.to_string())
        }
        _ => (None, rest),
    };
    Some(Parsed {
        ts_ms: time.to_ms(tz),
        level,
        message,
        ..Parsed::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogLevel;

    #[test]
    fn fpm_lines_are_parsed() {
        let line = br#"[06-Nov-2026 10:20:30] WARNING: [pool www] child 123 said into stderr: "PHP Warning: x""#;
        let parsed = parse(line, &TimeZone::UTC).unwrap();
        assert_eq!(parsed.level, Some(LogLevel::Warning));
        assert_eq!(
            parsed.message,
            r#"[pool www] child 123 said into stderr: "PHP Warning: x""#
        );
        assert_eq!(parsed.ts_ms, Some(1_793_960_430_000));
        assert!(is_header(b"[06-Nov-2026 10:20:30.123456] NOTICE: ready"));
        assert!(!is_header(b"[2026-11-06 10:20:30] local.INFO: x"));
        let notice = parse(
            b"[06-Nov-2026 10:20:30] NOTICE: fpm is running, pid 1",
            &TimeZone::UTC,
        )
        .unwrap();
        assert_eq!(notice.level, Some(LogLevel::Notice));
    }
}
