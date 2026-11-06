//! Ayudas para la salida compacta de la CLI: columnas alineadas y tiempos
//! relativos, que gastan pocos tokens y no dependen de la zona horaria.

use crate::{UNTRUSTED_BEGIN, UNTRUSTED_END};

/// Columnas separadas por dos espacios, sin espacios al final de línea.
pub(crate) fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| rows.iter().filter_map(|r| r.get(c)).map(|s| s.chars().count()).max().unwrap_or(0))
        .collect();
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (c, cell) in row.iter().enumerate() {
                if c + 1 < row.len() {
                    line.push_str(cell);
                    line.extend(std::iter::repeat_n(' ', widths[c] - cell.chars().count() + 2));
                } else {
                    line.push_str(cell);
                }
            }
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// «hace 5 s», «hace 3 min», «hace 2 h», «hace 4 d».
pub(crate) fn ago(ts_ms: i64, now_ms: i64) -> String {
    let secs = (now_ms - ts_ms).max(0) / 1000;
    match secs {
        0..=59 => format!("hace {secs} s"),
        60..=3599 => format!("hace {} min", secs / 60),
        3600..=86_399 => format!("hace {} h", secs / 3600),
        _ => format!("hace {} d", secs / 86_400),
    }
}

/// Encierra líneas de log entre las dos marcas fijas (05 §4.2).
pub(crate) fn log_block(lines: &[String]) -> String {
    let mut out = String::from(UNTRUSTED_BEGIN);
    for line in lines {
        out.push('\n');
        out.push_str(line);
    }
    out.push('\n');
    out.push_str(UNTRUSTED_END);
    out
}

/// Una sola línea: los saltos de línea de un mensaje de log se convierten en
/// espacios para que cada entrada ocupe una fila.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn opt(value: Option<&str>) -> String {
    value.unwrap_or("—").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_columns() {
        let rows = vec![
            vec!["mysql".to_string(), "en marcha".to_string(), "".to_string()],
            vec!["postgresql@18".to_string(), "parado".to_string(), "".to_string()],
        ];
        assert_eq!(table(&rows), "mysql          en marcha\npostgresql@18  parado");
    }

    #[test]
    fn relative_times() {
        assert_eq!(ago(0, 5_000), "hace 5 s");
        assert_eq!(ago(0, 180_000), "hace 3 min");
        assert_eq!(ago(0, 7_200_000), "hace 2 h");
        assert_eq!(ago(10, 0), "hace 0 s");
    }

    #[test]
    fn log_blocks_have_fixed_markers() {
        let block = log_block(&["a".into()]);
        assert!(block.starts_with(UNTRUSTED_BEGIN) && block.ends_with(UNTRUSTED_END));
        assert_eq!(one_line("a\n  b\tc"), "a b c");
    }
}
