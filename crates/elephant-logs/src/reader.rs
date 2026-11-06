//! Lectura hacia atrás por bloques, paginación y búsqueda binaria por tiempo
//! (02 §5.4).
//!
//! Nunca se lee un archivo entero: se lee desde el final en bloques de 64 KB, y
//! una entrada que cruza el borde de un bloque se arrastra al siguiente.

use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;

use jiff::tz::TimeZone;

use crate::parse::Format;

pub(crate) const BLOCK: u64 = 64 * 1024;
/// Al alinear a la siguiente cabecera se exploran como mucho 256 KB.
const ALIGN_LIMIT: u64 = 256 * 1024;
/// El detalle de una entrada, como mucho 1 MB.
pub(crate) const MAX_RECORD: usize = 1024 * 1024;

/// Una entrada completa tal como está en el archivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Record {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Record(Record),
    /// Se llegó al principio del archivo.
    End,
    /// Se agotó el presupuesto de bytes antes del principio.
    Budget,
}

/// Lee entradas de un archivo de la más nueva a la más antigua.
pub(crate) struct BackReader {
    file: File,
    format: Format,
    /// `buf` contiene los bytes `[pos, pos + buf.len())`, aún sin entregar.
    pos: u64,
    buf: Vec<u8>,
    ready: VecDeque<Record>,
    budget: u64,
    scanned: u64,
}

impl BackReader {
    /// `end` tiene que ser el final del archivo o el principio de una entrada.
    pub(crate) fn new(file: File, format: Format, end: u64, budget: u64) -> Self {
        Self {
            file,
            format,
            pos: end,
            buf: Vec::new(),
            ready: VecDeque::new(),
            budget,
            scanned: 0,
        }
    }

    pub(crate) fn scanned(&self) -> u64 {
        self.scanned
    }

    pub(crate) fn raise_budget(&mut self, budget: u64) {
        self.budget = self.budget.max(budget);
    }

    pub(crate) fn next_record(&mut self) -> io::Result<Step> {
        loop {
            if let Some(record) = self.ready.pop_front() {
                return Ok(Step::Record(record));
            }
            if self.pos == 0 {
                if self.buf.iter().all(u8::is_ascii_whitespace) {
                    self.buf.clear();
                    return Ok(Step::End);
                }
                // Líneas sueltas al principio, sin cabecera: no se pierden.
                let mut bytes = std::mem::take(&mut self.buf);
                bytes.truncate(MAX_RECORD);
                return Ok(Step::Record(Record { offset: 0, bytes }));
            }
            if self.scanned >= self.budget {
                return Ok(Step::Budget);
            }
            self.read_block()?;
        }
    }

    fn read_block(&mut self) -> io::Result<()> {
        let start = self.pos.saturating_sub(BLOCK);
        let len = usize::try_from(self.pos - start).unwrap_or(0);
        let mut block = vec![0; len];
        self.file.read_exact_at(&mut block, start)?;
        self.scanned += len as u64;
        // Del resto anterior, solo la primera línea (que estaba incompleta)
        // falta por mirar: las demás ya se vio que no son cabeceras.
        let checked_from = memchr::memchr(b'\n', &self.buf).map_or(self.buf.len(), |i| i + 1);
        block.extend_from_slice(&self.buf);
        self.buf = block;
        self.pos = start;

        // Inicios de línea, de atrás adelante. El primero solo cuenta si el
        // bloque empieza en 0: si no, esa línea puede seguir más atrás.
        let mut end = self.buf.len();
        let mut line_end = len + checked_from;
        let mut cursor = line_end;
        while cursor > 0 {
            let newline = memchr::memrchr(b'\n', &self.buf[..cursor - 1]);
            let line_start = newline.map_or(0, |i| i + 1);
            if line_start == 0 && self.pos != 0 {
                break;
            }
            if self.format.is_header(&self.buf[line_start..line_end]) {
                self.ready.push_back(Record {
                    offset: self.pos + line_start as u64,
                    bytes: self.buf[line_start..end.min(line_start + MAX_RECORD)].to_vec(),
                });
                end = line_start;
            }
            line_end = line_start;
            cursor = line_start;
            if line_start == 0 {
                break;
            }
        }
        self.buf.truncate(end);
        // Lo que sigue sin cabecera es el final de una entrada que empieza más
        // atrás (o líneas sueltas): de ella solo se entrega el primer MB, así
        // que el resto se suelta para que la memoria no crezca con el archivo.
        // Se guarda un bloque más para no partir la línea que hay que mirar.
        self.buf.truncate(MAX_RECORD + BLOCK as usize);
        Ok(())
    }
}

/// Lee la entrada que empieza en `offset`. Comprueba que ahí empieza una
/// línea con cabecera (o que es el principio del archivo); si no, `None`.
pub(crate) fn record_at(
    file: &File,
    format: Format,
    offset: u64,
    max: usize,
) -> io::Result<Option<Record>> {
    let size = file.metadata()?.len();
    if offset >= size {
        return Ok(None);
    }
    if offset > 0 {
        let mut previous = [0u8; 1];
        file.read_exact_at(&mut previous, offset - 1)?;
        if previous[0] != b'\n' {
            return Ok(None);
        }
    }
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
    let mut pos = offset;
    let mut checked_first = false;
    // Inicio de la línea que se está mirando, relativo a `bytes`.
    let mut line_start = 0usize;
    while pos < size && bytes.len() < max {
        let want = usize::try_from((size - pos).min(chunk.len() as u64)).unwrap_or(chunk.len());
        let read = file.read_at(&mut chunk[..want], pos)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        pos += read as u64;
        while let Some(newline) = memchr::memchr(b'\n', &bytes[line_start..]) {
            let line = &bytes[line_start..line_start + newline + 1];
            if !checked_first {
                checked_first = true;
                if !format.is_header(line) && offset != 0 {
                    return Ok(None);
                }
            } else if format.is_header(line) {
                bytes.truncate(line_start);
                return Ok(Some(Record { offset, bytes }));
            }
            line_start += newline + 1;
        }
    }
    if !checked_first && !format.is_header(&bytes) && offset != 0 {
        return Ok(None);
    }
    // Una última línea sin salto que es otra cabecera no es de esta entrada.
    if checked_first && line_start < bytes.len() && format.is_header(&bytes[line_start..]) {
        bytes.truncate(line_start);
    }
    bytes.truncate(max);
    Ok(Some(Record { offset, bytes }))
}

/// La primera cabecera que empieza en `from` o después, con su hora,
/// explorando como mucho 256 KB. Devuelve también los bytes leídos.
fn header_after(
    file: &File,
    format: Format,
    tz: &TimeZone,
    from: u64,
    size: u64,
) -> io::Result<(Option<(u64, i64)>, u64)> {
    let mut scanned = 0u64;
    // Se empieza un byte antes para saber si `from` ya es principio de línea.
    let mut pos = from.saturating_sub(1);
    let mut buf: Vec<u8> = Vec::new();
    let mut buf_start = pos;
    let mut first_line_done = from == 0;
    let mut chunk = vec![0u8; 8 * 1024];
    while pos < size && scanned < ALIGN_LIMIT {
        let want = usize::try_from((size - pos).min(chunk.len() as u64)).unwrap_or(chunk.len());
        let read = file.read_at(&mut chunk[..want], pos)?;
        if read == 0 {
            break;
        }
        scanned += read as u64;
        pos += read as u64;
        buf.extend_from_slice(&chunk[..read]);
        let at_eof = pos >= size;
        let mut line_start = 0usize;
        loop {
            let newline = memchr::memchr(b'\n', &buf[line_start..]);
            let line_end = match newline {
                Some(n) => line_start + n + 1,
                None if at_eof => buf.len(),
                None => break,
            };
            if !first_line_done {
                // Lo que hay hasta el primer salto es el final de otra línea.
                first_line_done = true;
            } else if line_end > line_start {
                let line = &buf[line_start..line_end];
                if format.is_header(line)
                    && let Some(ts) = format.header_ts(line, tz)
                {
                    return Ok((Some((buf_start + line_start as u64, ts)), scanned));
                }
            }
            if newline.is_none() {
                break;
            }
            line_start = line_end;
        }
        buf.drain(..line_start);
        buf_start += line_start as u64;
    }
    Ok((None, scanned))
}

/// Offset de la primera entrada con hora posterior a `until_ms`, por búsqueda
/// binaria: saltar, alinear a la siguiente cabecera y comparar. Si las horas
/// no van en orden (por ejemplo, tras un cambio de zona), devuelve el final del
/// archivo y quien llama filtra entrada a entrada.
/// Devuelve `(offset, bytes leídos, ordenado)`.
pub(crate) fn offset_after(
    file: &File,
    format: Format,
    tz: &TimeZone,
    size: u64,
    until_ms: i64,
) -> io::Result<(u64, u64, bool)> {
    let (mut lo, mut hi) = (0u64, size);
    let mut scanned = 0u64;
    let mut probes: Vec<(u64, i64)> = Vec::new();
    while hi - lo > BLOCK {
        let mid = lo + (hi - lo) / 2;
        let (found, read) = header_after(file, format, tz, mid, size)?;
        scanned += read;
        match found {
            Some((offset, ts)) if offset < hi => {
                probes.push((offset, ts));
                if ts > until_ms {
                    hi = offset;
                } else {
                    lo = offset + 1;
                }
            }
            _ => hi = mid,
        }
    }
    probes.sort_unstable();
    if probes.windows(2).any(|pair| pair[1].1 < pair[0].1) {
        return Ok((size, scanned, false));
    }
    // Tramo final, lineal y de una sola lectura: como mucho 64 KB más lo que
    // haga falta para terminar la última línea.
    let (offset, read) = first_after_in_window(file, format, tz, lo, hi, size, until_ms)?;
    Ok((offset, scanned + read, true))
}

/// La primera cabecera que empieza en `[lo, hi)` con hora posterior a
/// `until_ms`; si no hay, `hi`.
fn first_after_in_window(
    file: &File,
    format: Format,
    tz: &TimeZone,
    lo: u64,
    hi: u64,
    size: u64,
    until_ms: i64,
) -> io::Result<(u64, u64)> {
    let start = lo.saturating_sub(1);
    let end = hi.saturating_add(ALIGN_LIMIT).min(size);
    let mut buf = vec![0u8; usize::try_from(end - start).unwrap_or(0)];
    file.read_exact_at(&mut buf, start)?;
    let read = buf.len() as u64;
    // Si `lo` no es principio de línea, la primera no cuenta.
    let mut line_start = if lo == 0 {
        0
    } else {
        match memchr::memchr(b'\n', &buf) {
            Some(newline) => newline + 1,
            None => return Ok((hi, read)),
        }
    };
    while start + (line_start as u64) < hi {
        let line_end =
            memchr::memchr(b'\n', &buf[line_start..]).map_or(buf.len(), |n| line_start + n + 1);
        let line = &buf[line_start..line_end];
        if format.is_header(line) && format.header_ts(line, tz).is_some_and(|ts| ts > until_ms) {
            return Ok((start + line_start as u64, read));
        }
        if line_end == buf.len() {
            break;
        }
        line_start = line_end;
    }
    Ok((hi, read))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testutil::TempDir;
    use std::fs;
    use std::io::Write;

    fn open(dir: &TempDir, name: &str, text: &str) -> File {
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        File::open(path).unwrap()
    }

    fn collect(mut reader: BackReader) -> Vec<Record> {
        let mut out = Vec::new();
        while let Step::Record(record) = reader.next_record().unwrap() {
            out.push(record);
        }
        out
    }

    fn line(i: usize) -> String {
        format!(
            "[2026-11-06 10:{:02}:{:02}] local.INFO: entrada {i}\n",
            i / 60 % 60,
            i % 60
        )
    }

    #[test]
    fn records_are_read_backwards_with_their_continuations() {
        let dir = TempDir::new("back");
        let text = "suelta\n[2026-11-06 10:00:00] local.ERROR: uno\n#0 traza\n#1 más\n[2026-11-06 10:00:01] local.INFO: dos\n";
        let file = open(&dir, "a.log", text);
        let size = text.len() as u64;
        let records = collect(BackReader::new(file, Format::Laravel, size, u64::MAX));
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].bytes, b"[2026-11-06 10:00:01] local.INFO: dos\n");
        assert_eq!(
            records[1].bytes,
            b"[2026-11-06 10:00:00] local.ERROR: uno\n#0 traza\n#1 m\xc3\xa1s\n"
        );
        assert_eq!(records[1].offset, 7);
        assert_eq!(
            records[2],
            Record {
                offset: 0,
                bytes: b"suelta\n".to_vec()
            }
        );
    }

    #[test]
    fn records_crossing_block_borders_are_kept_whole() {
        let dir = TempDir::new("borders");
        let mut text = String::new();
        let trace: String = (0..4000)
            .map(|i| format!("#{i} /app/F.php({i}): f()\n"))
            .collect();
        for i in 0..20 {
            text.push_str(&line(i));
            if i % 5 == 0 {
                text.push_str(&trace);
            }
        }
        let file = open(&dir, "big.log", &text);
        let records = collect(BackReader::new(
            file,
            Format::Laravel,
            text.len() as u64,
            u64::MAX,
        ));
        assert_eq!(records.len(), 20);
        let rebuilt: Vec<u8> = records.iter().rev().flat_map(|r| r.bytes.clone()).collect();
        assert_eq!(rebuilt, text.as_bytes());
    }

    #[test]
    fn huge_records_are_capped_and_memory_stays_bounded() {
        let dir = TempDir::new("huge");
        let trace = "#0 /app/F.php(1): f()\n".repeat(200_000);
        let text = format!("{}{trace}{}", line(0), line(1));
        let file = open(&dir, "a.log", &text);
        let mut reader = BackReader::new(file, Format::Laravel, text.len() as u64, u64::MAX);
        let mut records = Vec::new();
        while let Step::Record(record) = reader.next_record().unwrap() {
            assert!(reader.buf.len() <= MAX_RECORD + 2 * BLOCK as usize);
            records.push(record);
        }
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].offset, 0);
        assert_eq!(records[1].bytes.len(), MAX_RECORD);
        assert!(records[1].bytes.starts_with(line(0).as_bytes()));
    }

    #[test]
    fn the_budget_stops_the_reader() {
        let dir = TempDir::new("budget");
        let text: String = (0..20_000).map(line).collect();
        let file = open(&dir, "a.log", &text);
        let mut reader = BackReader::new(file, Format::Laravel, text.len() as u64, BLOCK);
        let mut count = 0;
        let last = loop {
            match reader.next_record().unwrap() {
                Step::Record(_) => count += 1,
                other => break other,
            }
        };
        assert_eq!(last, Step::Budget);
        assert!(count > 100);
        assert_eq!(reader.scanned(), BLOCK);
    }

    #[test]
    fn record_at_checks_that_an_entry_starts_there() {
        let dir = TempDir::new("at");
        let text = "[2026-11-06 10:00:00] local.ERROR: uno\n#0 traza\n[2026-11-06 10:00:01] local.INFO: dos";
        let file = open(&dir, "a.log", text);
        let record = record_at(&file, Format::Laravel, 0, MAX_RECORD)
            .unwrap()
            .unwrap();
        assert_eq!(
            record.bytes,
            b"[2026-11-06 10:00:00] local.ERROR: uno\n#0 traza\n"
        );
        let second = record_at(&file, Format::Laravel, 48, MAX_RECORD)
            .unwrap()
            .unwrap();
        assert_eq!(second.bytes, b"[2026-11-06 10:00:01] local.INFO: dos");
        assert!(
            record_at(&file, Format::Laravel, 39, MAX_RECORD)
                .unwrap()
                .is_none(),
            "«#0 traza» no es cabecera"
        );
        assert!(
            record_at(&file, Format::Laravel, 5, MAX_RECORD)
                .unwrap()
                .is_none(),
            "mitad de línea"
        );
        assert!(
            record_at(&file, Format::Laravel, 10_000, MAX_RECORD)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn binary_search_finds_the_first_entry_after_a_time() {
        let dir = TempDir::new("bsearch");
        let path = dir.path().join("a.log");
        let mut out = fs::File::create(&path).unwrap();
        let mut offsets = Vec::new();
        let mut size = 0u64;
        for i in 0..30_000 {
            let text = line(i % 3600);
            offsets.push(size);
            size += text.len() as u64;
            out.write_all(text.as_bytes()).unwrap();
            if i == 3599 {
                break;
            }
        }
        drop(out);
        let file = File::open(&path).unwrap();
        let tz = TimeZone::UTC;
        let base = jiff::civil::datetime(2026, 11, 6, 10, 0, 0, 0)
            .to_zoned(tz.clone())
            .unwrap()
            .timestamp()
            .as_millisecond();
        let (offset, scanned, sorted) =
            offset_after(&file, Format::Laravel, &tz, size, base + 1_000 * 1800).unwrap();
        assert!(sorted);
        assert_eq!(offset, offsets[1801]);
        assert!(scanned < 1024 * 1024, "leídos {scanned}");
        let (end, _, _) =
            offset_after(&file, Format::Laravel, &tz, size, base + 1_000_000_000).unwrap();
        assert_eq!(end, size);
        let (start, _, _) = offset_after(&file, Format::Laravel, &tz, size, base - 1).unwrap();
        assert_eq!(start, 0);
    }

    #[test]
    fn disordered_files_fall_back_to_a_linear_scan() {
        let dir = TempDir::new("disorder");
        let mut text = String::new();
        for i in 0..4000 {
            // La segunda mitad «vuelve atrás» una hora.
            let minute = if i < 2000 { 30 + i / 120 } else { i / 200 };
            text.push_str(&format!(
                "[2026-11-06 10:{minute:02}:00] local.INFO: {i} {}\n",
                "x".repeat(40)
            ));
        }
        let file = open(&dir, "a.log", &text);
        let tz = TimeZone::UTC;
        let until = jiff::civil::datetime(2026, 11, 6, 10, 5, 0, 0)
            .to_zoned(tz.clone())
            .unwrap()
            .timestamp()
            .as_millisecond();
        let (offset, _, sorted) =
            offset_after(&file, Format::Laravel, &tz, text.len() as u64, until).unwrap();
        assert!(!sorted);
        assert_eq!(offset, text.len() as u64);
    }
}
