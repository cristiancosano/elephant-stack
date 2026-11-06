//! Filtros, fusión por tiempo y agrupación (02 §5.4 y §6).
//!
//! Cada archivo se lee hacia atrás con su propio cursor y las entradas se
//! fusionan por hora, de la más nueva a la más antigua (k-way merge con
//! `BinaryHeap`). El orden total es `(ts_ms, file_key, offset)`, así que un
//! cursor (el id de una entrada) basta para seguir donde se quedó la página
//! anterior sin guardar estado: sirve igual a la app y a la CLI.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::io;

use memchr::memmem;

use crate::parse::{self, FileCtx, build_entry, entry_id, parse_entry_id};
use crate::reader::{BackReader, MAX_RECORD, Record, Step, offset_after, record_at};
use crate::sources::{Catalog, LogFile};
use crate::{LogEntry, LogLevel, LogQuery, LogSource, SourceKind, StatusFilter, projects};

/// Primera página sin filtros: como mucho 4 MB por archivo (02 §5.4).
pub(crate) const PAGE_BUDGET: u64 = 4 * 1024 * 1024;
/// Con filtros que descartan entradas (texto, nivel, estado) se lee más antes
/// de devolver la página.
pub(crate) const FILTERED_BUDGET: u64 = 32 * 1024 * 1024;
/// Una entrada más grande que el presupuesto se lee igual, hasta este tope,
/// para que la paginación siempre avance.
const HARD_BUDGET: u64 = 64 * 1024 * 1024;
/// Un salto atrás de más de una hora se toma como desorden.
const DISORDER_MS: i64 = 3_600_000;
pub(crate) const DEFAULT_LIMIT: u32 = 200;
pub(crate) const MAX_LIMIT: u32 = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProjectSel {
    Id(String),
    /// Hosts de nginx que no son de ningún proyecto.
    Unassigned,
    /// PHP-FPM.
    System,
}

/// `LogQuery` ya resuelto contra el catálogo.
#[derive(Debug, Clone, Default)]
pub(crate) struct Filter {
    projects: Option<Vec<ProjectSel>>,
    kinds: Vec<SourceKind>,
    /// Solo los archivos de esta fuente (`logs tail`).
    source: Option<usize>,
    pub min_level: Option<LogLevel>,
    /// En minúsculas ASCII.
    text: Option<Vec<u8>>,
    status: Option<StatusFilter>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
}

impl Filter {
    /// Nivel Error o superior desde `since_ms`, en todas las fuentes.
    pub(crate) fn errors_since(since_ms: i64) -> Self {
        Self {
            min_level: Some(LogLevel::Error),
            since_ms: Some(since_ms),
            ..Self::default()
        }
    }
}

impl Filter {
    /// Los proyectos se aceptan por id, nombre o host, además de `none` y
    /// `system`. Uno que no existe no coincide con nada.
    pub(crate) fn new(q: &LogQuery, catalog: &Catalog) -> Self {
        let projects = (!q.projects.is_empty()).then(|| {
            q.projects
                .iter()
                .map(|needle| match needle.as_str() {
                    "none" => ProjectSel::Unassigned,
                    "system" => ProjectSel::System,
                    other => ProjectSel::Id(
                        projects::resolve(&catalog.projects, other)
                            .map_or_else(|| other.to_string(), |p| p.id.clone()),
                    ),
                })
                .collect()
        });
        let text = q
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.as_bytes().to_ascii_lowercase());
        Self {
            projects,
            kinds: q.kinds.clone(),
            source: None,
            min_level: q.min_level,
            text,
            status: q.status,
            since_ms: q.since_ms,
            until_ms: q.until_ms,
        }
    }

    pub(crate) fn for_source(source: usize) -> Self {
        Self {
            source: Some(source),
            ..Self::default()
        }
    }

    pub(crate) fn with_text(mut self, text: Option<&str>) -> Self {
        self.text = text
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.as_bytes().to_ascii_lowercase());
        self
    }

    /// Hay filtros que descartan entradas: se lee más por página.
    fn is_selective(&self) -> bool {
        self.text.is_some()
            || self.status.is_some()
            || self.min_level.is_some_and(|l| l > LogLevel::Debug)
    }

    pub(crate) fn budget(&self) -> u64 {
        if self.is_selective() {
            FILTERED_BUDGET
        } else {
            PAGE_BUDGET
        }
    }

    pub(crate) fn wants_file(&self, index: usize, source: &LogSource) -> bool {
        if self.source.is_some_and(|wanted| wanted != index) {
            return false;
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&source.kind) {
            return false;
        }
        let Some(selected) = &self.projects else {
            return true;
        };
        match (&source.project, source.kind) {
            (Some(id), _) => selected.contains(&ProjectSel::Id(id.clone())),
            (None, SourceKind::PhpFpm) => selected.contains(&ProjectSel::System),
            // nginx global: sus líneas pueden ser de cualquier proyecto.
            (None, _) => selected.iter().any(|s| *s != ProjectSel::System),
        }
    }

    /// Lo que se puede decidir sobre los bytes, antes de interpretar nada.
    fn raw_matches(&self, raw: &[u8], scratch: &mut Vec<u8>) -> bool {
        let Some(needle) = &self.text else {
            return true;
        };
        scratch.clear();
        scratch.extend(raw.iter().map(u8::to_ascii_lowercase));
        memmem::find(scratch, needle).is_some()
    }

    pub(crate) fn entry_matches(&self, entry: &LogEntry) -> bool {
        if self.min_level.is_some_and(|min| entry.level < min) {
            return false;
        }
        if self.since_ms.is_some_and(|since| entry.ts_ms < since)
            || self.until_ms.is_some_and(|until| entry.ts_ms > until)
        {
            return false;
        }
        if let Some(status) = self.status {
            let Some(http) = &entry.http else {
                return false;
            };
            let ok = match status {
                StatusFilter::Class(class) => http.status / 100 == u16::from(class),
                StatusFilter::AtLeast(min) => http.status >= min,
            };
            if !ok {
                return false;
            }
        }
        if let Some(selected) = &self.projects {
            let sel = match (&entry.project, entry.kind) {
                (Some(id), _) => ProjectSel::Id(id.clone()),
                (None, SourceKind::PhpFpm) => ProjectSel::System,
                (None, _) => ProjectSel::Unassigned,
            };
            if !selected.contains(&sel) {
                return false;
            }
        }
        true
    }

    /// Para el directo: los bytes y la entrada.
    pub(crate) fn matches(&self, entry: &LogEntry, raw: &[u8], scratch: &mut Vec<u8>) -> bool {
        self.entry_matches(entry) && self.raw_matches(raw, scratch)
    }
}

/// Orden total de las entradas.
type Key = (i64, u64, u64);

enum Pull {
    Entry(Box<(LogEntry, Record)>),
    End,
    Budget,
}

/// Lee un archivo hacia atrás y entrega solo lo que pasa el filtro.
struct FileCursor {
    reader: BackReader,
    ctx: FileCtx,
    /// Entradas con clave mayor o igual ya se entregaron en otra página.
    skip_from: Option<Key>,
    last_ts: Option<i64>,
    disordered: bool,
    examined_any: bool,
    /// La última entrada examinada, aceptada o no.
    frontier: Option<Key>,
    /// La hora de la primera entrada examinada: la más nueva del archivo.
    first_ts: Option<i64>,
    scratch: Vec<u8>,
}

impl FileCursor {
    fn new(reader: BackReader, ctx: FileCtx, skip_from: Option<Key>) -> Self {
        Self {
            reader,
            ctx,
            skip_from,
            last_ts: None,
            disordered: false,
            examined_any: false,
            frontier: None,
            first_ts: None,
            scratch: Vec::new(),
        }
    }

    fn pull(&mut self, filter: &Filter) -> io::Result<Pull> {
        loop {
            let record = match self.reader.next_record()? {
                Step::Record(record) => record,
                Step::End => return Ok(Pull::End),
                Step::Budget if !self.examined_any && self.reader.scanned() < HARD_BUDGET => {
                    self.reader.raise_budget(HARD_BUDGET);
                    continue;
                }
                Step::Budget => return Ok(Pull::Budget),
            };
            self.examined_any = true;
            if !filter.raw_matches(&record.bytes, &mut self.scratch) {
                let ts = self
                    .ctx
                    .format
                    .header_ts(crate::util::first_line(&record.bytes), &self.ctx.tz)
                    .unwrap_or(self.ctx.mtime_ms);
                self.frontier = Some((ts, self.ctx.key, record.offset));
                if self.observe(ts, filter) {
                    return Ok(Pull::End);
                }
                continue;
            }
            let entry = build_entry(&self.ctx, record.offset, &record.bytes);
            let key = (entry.ts_ms, self.ctx.key, record.offset);
            self.frontier = Some(key);
            if self.observe(entry.ts_ms, filter) {
                return Ok(Pull::End);
            }
            if self.skip_from.is_some_and(|skip| key >= skip) || !filter.entry_matches(&entry) {
                continue;
            }
            return Ok(Pull::Entry(Box::new((entry, record))));
        }
    }

    /// Detecta desorden y dice si ya se ha pasado de `since` (en un archivo
    /// ordenado, lo que queda es más antiguo).
    fn observe(&mut self, ts: i64, filter: &Filter) -> bool {
        self.first_ts.get_or_insert(ts);
        if self.last_ts.is_some_and(|newer| ts > newer + DISORDER_MS) {
            self.disordered = true;
        }
        self.last_ts = Some(ts);
        !self.disordered && filter.since_ms.is_some_and(|since| ts < since)
    }
}

struct Head {
    key: Key,
    file: usize,
    entry: LogEntry,
    record: Record,
}

impl PartialEq for Head {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for Head {}
impl PartialOrd for Head {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Head {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

pub(crate) struct ScanEnd {
    pub next_before: Option<String>,
    pub scanned: u64,
}

/// Prepara un cursor por archivo: offset final (por el cursor o `until`) y
/// presupuesto.
fn open_cursors(
    catalog: &Catalog,
    filter: &Filter,
    before: Option<&str>,
    budget: u64,
) -> Result<(Vec<FileCursor>, u64), String> {
    let cursor = match before {
        Some(id) => Some(resolve_cursor(catalog, id)?),
        None => None,
    };
    let mut cursors = Vec::new();
    let mut scanned = 0;
    for file in &catalog.files {
        if !filter.wants_file(file.source, catalog.source(file)) {
            continue;
        }
        let Some((handle, ctx)) = catalog.context(file) else {
            continue;
        };
        let size = handle.metadata().map(|m| m.len()).unwrap_or(0);
        let mut skip_from = None;
        let mut until = filter.until_ms;
        let end = match cursor {
            Some(key) if key.1 == file.key => key.2.min(size),
            other => {
                if let Some(key) = other {
                    skip_from = Some(key);
                    until = Some(until.map_or(key.0, |u| u.min(key.0)));
                }
                match until {
                    Some(until) => {
                        let (offset, read, _) =
                            offset_after(&handle, ctx.format, &ctx.tz, size, until)
                                .map_err(|e| read_error(&ctx, &e))?;
                        scanned += read;
                        offset
                    }
                    None => size,
                }
            }
        };
        let reader = BackReader::new(handle, ctx.format, end, budget);
        cursors.push(FileCursor::new(reader, ctx, skip_from));
    }
    Ok((cursors, scanned))
}

fn read_error(ctx: &FileCtx, error: &io::Error) -> String {
    format!("No se pudo leer {}: {error}.", ctx.path.display())
}

/// La clave de la entrada a la que apunta un cursor.
fn resolve_cursor(catalog: &Catalog, id: &str) -> Result<Key, String> {
    const STALE: &str =
        "El cursor ya no es válido: el archivo de log ha cambiado. Vuelve a la primera página.";
    let (key, offset) = parse_entry_id(id).ok_or_else(|| format!("Cursor no válido: {id}."))?;
    let file = catalog.file_by_key(key).ok_or(STALE)?;
    let (handle, ctx) = catalog.context(file).ok_or(STALE)?;
    let record = record_at(&handle, ctx.format, offset, MAX_RECORD)
        .map_err(|e| read_error(&ctx, &e))?
        .ok_or(STALE)?;
    let ts = ctx
        .format
        .header_ts(crate::util::first_line(&record.bytes), &ctx.tz)
        .unwrap_or(ctx.mtime_ms);
    Ok((ts, key, offset))
}

/// Recorre las entradas que pasan el filtro, de la más nueva a la más
/// antigua, fusionando todos los archivos. `visit` devuelve `false` para
/// parar. Si un archivo agota su presupuesto, se entregan solo las entradas
/// que seguro son más nuevas que lo que falta por leer de él, y el cursor
/// apunta a ese punto.
pub(crate) fn scan(
    catalog: &Catalog,
    filter: &Filter,
    before: Option<&str>,
    budget: u64,
    mut visit: impl FnMut(&LogEntry, &Record, &FileCtx) -> bool,
) -> Result<ScanEnd, String> {
    let (mut cursors, mut scanned) = open_cursors(catalog, filter, before, budget)?;
    let mut heap = BinaryHeap::new();
    // La clave más alta de lo que aún falta por leer de los archivos parados.
    let mut floor: Option<Key> = None;
    for index in 0..cursors.len() {
        pull_into(&mut cursors, index, filter, &mut heap, &mut floor)?;
    }
    let next_before = loop {
        let Some(top) = heap.peek() else {
            break floor.map(|key| entry_id(key.1, key.2));
        };
        if floor.is_some_and(|floor| top.key < floor) {
            break floor.map(|key| entry_id(key.1, key.2));
        }
        let Some(head) = heap.pop() else { break None };
        pull_into(&mut cursors, head.file, filter, &mut heap, &mut floor)?;
        if !visit(&head.entry, &head.record, &cursors[head.file].ctx) {
            let more = !heap.is_empty() || floor.is_some();
            break more.then(|| head.entry.id.clone());
        }
    };
    scanned += cursors.iter().map(|c| c.reader.scanned()).sum::<u64>();
    Ok(ScanEnd {
        next_before,
        scanned,
    })
}

fn pull_into(
    cursors: &mut [FileCursor],
    index: usize,
    filter: &Filter,
    heap: &mut BinaryHeap<Head>,
    floor: &mut Option<Key>,
) -> Result<(), String> {
    let cursor = &mut cursors[index];
    match cursor
        .pull(filter)
        .map_err(|e| read_error(&cursor.ctx, &e))?
    {
        Pull::Entry(found) => {
            let (entry, record) = *found;
            heap.push(Head {
                key: (entry.ts_ms, cursor.ctx.key, record.offset),
                file: index,
                entry,
                record,
            });
        }
        Pull::End => {}
        Pull::Budget => {
            // Sin nada examinado no hay frontera: lo que queda empieza en el final.
            let frontier = cursor
                .frontier
                .unwrap_or((i64::MAX, cursor.ctx.key, u64::MAX));
            *floor = Some(floor.map_or(frontier, |f| f.max(frontier)));
        }
    }
    Ok(())
}

/// Recorre un solo archivo, sin fusionar: para contar.
pub(crate) fn scan_file(
    catalog: &Catalog,
    file: &LogFile,
    filter: &Filter,
    budget: u64,
    mut visit: impl FnMut(&LogEntry) -> bool,
) -> Option<FileScan> {
    let (handle, ctx) = catalog.context(file)?;
    let size = handle.metadata().ok()?.len();
    let mut cursor = FileCursor::new(BackReader::new(handle, ctx.format, size, budget), ctx, None);
    let capped = loop {
        match cursor.pull(filter).ok()? {
            Pull::Entry(found) if !visit(&found.0) => break true,
            Pull::Entry(..) => {}
            Pull::End => break false,
            Pull::Budget => break true,
        }
    };
    Some(FileScan {
        capped,
        newest_any_ms: cursor.first_ts,
    })
}

pub(crate) struct FileScan {
    /// Se paró antes del final (presupuesto o tope del llamante).
    pub capped: bool,
    /// La más nueva del archivo, pase o no el filtro.
    pub newest_any_ms: Option<i64>,
}

/// Clamp del límite de una página.
pub(crate) fn page_limit(limit: u32) -> usize {
    let limit = if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    };
    limit as usize
}

/// Agrupa por huella las entradas que pasan el filtro.
pub(crate) fn group(
    catalog: &Catalog,
    filter: &Filter,
    max_entries: usize,
) -> Result<Vec<crate::LogGroup>, String> {
    let mut groups: HashMap<String, crate::LogGroup> = HashMap::new();
    let mut seen = 0usize;
    scan(catalog, filter, None, FILTERED_BUDGET, |entry, _, _| {
        seen += 1;
        let group = groups
            .entry(entry.fingerprint.clone())
            .or_insert_with(|| crate::LogGroup {
                fingerprint: entry.fingerprint.clone(),
                count: 0,
                first_ms: entry.ts_ms,
                last_ms: entry.ts_ms,
                sample: entry.clone(),
                projects: Vec::new(),
            });
        group.count += 1;
        group.first_ms = group.first_ms.min(entry.ts_ms);
        group.last_ms = group.last_ms.max(entry.ts_ms);
        if let Some(project) = &entry.project
            && !group.projects.contains(project)
        {
            group.projects.push(project.clone());
        }
        seen < max_entries
    })?;
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by(|a, b| {
        b.last_ms
            .cmp(&a.last_ms)
            .then_with(|| b.count.cmp(&a.count))
    });
    Ok(groups)
}

pub(crate) fn read_detail(catalog: &Catalog, id: &str) -> Result<crate::EntryDetail, String> {
    const GONE: &str = "La entrada ya no existe: el archivo ha cambiado.";
    let (key, offset) =
        parse_entry_id(id).ok_or_else(|| format!("Identificador de entrada no válido: {id}."))?;
    let file = catalog
        .file_by_key(key)
        .ok_or("La entrada ya no existe: no hay ningún archivo de log con ese identificador.")?;
    let (handle, ctx) = catalog.context(file).ok_or(GONE)?;
    let record = record_at(&handle, ctx.format, offset, MAX_RECORD)
        .map_err(|e| read_error(&ctx, &e))?
        .ok_or(GONE)?;
    Ok(detail_of(&ctx, &record))
}

pub(crate) fn detail_of(ctx: &FileCtx, record: &Record) -> crate::EntryDetail {
    let entry = build_entry(ctx, record.offset, &record.bytes);
    let body = String::from_utf8_lossy(crate::util::trim_newline(&record.bytes)).into_owned();
    crate::EntryDetail {
        entry,
        body,
        frames: parse::frames_of(ctx.format, &record.bytes),
        context: parse::context_of(ctx.format, &record.bytes),
    }
}
