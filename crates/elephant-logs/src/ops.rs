//! Las operaciones de la API pública sobre un [`Catalog`] ya cargado. Las
//! funciones libres de la crate cargan uno nuevo en cada llamada (CLI, sin
//! estado); `LogHub` reutiliza uno durante unos segundos.

use std::collections::HashMap;
use std::fs::OpenOptions;

use crate::query::{self, FILTERED_BUDGET, Filter, PAGE_BUDGET, page_limit, scan, scan_file};
use crate::sources::{self, Catalog};
use crate::{
    EntryDetail, ErrorSummary, Frame, HttpBrief, LogEntry, LogGroup, LogLevel, LogPage, LogQuery,
    ProjectSummary, SourceKind, SourceStatus, projects,
};

/// El contador de errores de 24 h se corta aquí; la interfaz muestra «500+».
const ERRORS_CAP: u32 = 500;
/// `errors_summary` devuelve como mucho estos grupos.
const MAX_ERROR_GROUPS: usize = 20;
/// Entradas que se agrupan como mucho en una llamada.
const MAX_GROUPED: usize = 50_000;
/// La petición más próxima a un error se busca en ±5 min.
const REQUEST_WINDOW_MS: i64 = 5 * 60_000;
/// Frames que se conservan en un resumen (C14): 8 propios y el primero de vendor.
const OWN_FRAMES: usize = 8;
const DAY_MS: i64 = 24 * 3_600_000;

pub(crate) fn sources(catalog: &Catalog, project: Option<&str>) -> Vec<SourceStatus> {
    sources::for_project(catalog, project)
}

pub(crate) fn summaries(catalog: &Catalog, now_ms: i64) -> Vec<ProjectSummary> {
    let filter = Filter::errors_since(now_ms - DAY_MS);
    #[derive(Default)]
    struct Tally {
        errors: u32,
        capped: bool,
        last_ms: Option<i64>,
    }
    let mut tallies: HashMap<String, Tally> = HashMap::new();
    for file in &catalog.files {
        let source = catalog.source(file);
        match &source.project {
            Some(project) => {
                let mut count = 0u32;
                let Some(result) = scan_file(catalog, file, &filter, PAGE_BUDGET, |_| {
                    count += 1;
                    count < ERRORS_CAP
                }) else {
                    continue;
                };
                let tally = tallies.entry(project.clone()).or_default();
                tally.errors = (tally.errors + count).min(ERRORS_CAP);
                tally.capped |= result.capped || tally.errors >= ERRORS_CAP;
                tally.last_ms = tally.last_ms.max(result.newest_any_ms);
            }
            // nginx global: cada línea suma a su proyecto.
            None if matches!(
                source.kind,
                SourceKind::NginxError | SourceKind::NginxAccess
            ) =>
            {
                let mut per_project: HashMap<String, (u32, i64)> = HashMap::new();
                let result = scan_file(catalog, file, &filter, PAGE_BUDGET, |entry| {
                    if let Some(project) = &entry.project {
                        let slot = per_project
                            .entry(project.clone())
                            .or_insert((0, entry.ts_ms));
                        slot.0 += 1;
                        slot.1 = slot.1.max(entry.ts_ms);
                    }
                    true
                });
                for (project, (count, last)) in per_project {
                    let tally = tallies.entry(project).or_default();
                    tally.errors = (tally.errors + count).min(ERRORS_CAP);
                    tally.capped |=
                        result.as_ref().is_some_and(|r| r.capped) || tally.errors >= ERRORS_CAP;
                    tally.last_ms = tally.last_ms.max(Some(last));
                }
            }
            None => {}
        }
    }
    catalog
        .projects
        .iter()
        .map(|project| {
            let tally = tallies.remove(&project.id).unwrap_or_default();
            ProjectSummary {
                project: project.clone(),
                sources: catalog
                    .sources
                    .iter()
                    .filter(|s| s.source.project.as_deref() == Some(project.id.as_str()))
                    .cloned()
                    .collect(),
                errors_24h: tally.errors,
                errors_capped: tally.capped,
                last_ms: tally.last_ms,
                log_override: catalog.overrides.get(&project.id).cloned(),
            }
        })
        .collect()
}

pub(crate) fn query(catalog: &Catalog, q: &LogQuery) -> Result<LogPage, String> {
    let filter = Filter::new(q, catalog);
    let limit = page_limit(q.limit);
    let mut entries = Vec::with_capacity(limit.min(512));
    let end = scan(
        catalog,
        &filter,
        q.before.as_deref(),
        filter.budget(),
        |entry, _, _| {
            entries.push(entry.clone());
            entries.len() < limit
        },
    )?;
    Ok(LogPage {
        entries,
        next_before: end.next_before,
        scanned_bytes: end.scanned,
    })
}

pub(crate) fn entry(catalog: &Catalog, id: &str) -> Result<EntryDetail, String> {
    query::read_detail(catalog, id)
}

pub(crate) fn groups(catalog: &Catalog, q: &LogQuery) -> Result<Vec<LogGroup>, String> {
    let filter = Filter::new(q, catalog);
    let mut groups = query::group(catalog, &filter, MAX_GROUPED)?;
    groups.truncate(page_limit(q.limit));
    Ok(groups)
}

pub(crate) fn tail(
    catalog: &Catalog,
    source_id: &str,
    lines: u32,
    min_level: Option<LogLevel>,
    contains: Option<&str>,
) -> Result<Vec<LogEntry>, String> {
    let index = catalog
        .sources
        .iter()
        .position(|s| s.source.id == source_id)
        .ok_or_else(|| {
            format!("No hay ninguna fuente de logs «{source_id}». Consulta la lista de fuentes.")
        })?;
    let mut filter = Filter::for_source(index).with_text(contains);
    filter.min_level = min_level;
    let limit = match lines {
        0 => 100,
        n => n.min(500) as usize,
    };
    let mut entries = Vec::new();
    scan(catalog, &filter, None, filter.budget(), |entry, _, _| {
        entries.push(entry.clone());
        entries.len() < limit
    })?;
    entries.reverse();
    Ok(entries)
}

pub(crate) fn errors_summary(
    catalog: &Catalog,
    project: Option<&str>,
    since_ms: i64,
) -> Result<Vec<ErrorSummary>, String> {
    let mut q = LogQuery {
        min_level: Some(LogLevel::Error),
        since_ms: Some(since_ms),
        ..LogQuery::default()
    };
    if let Some(needle) = project {
        let found = projects::resolve(&catalog.projects, needle)
            .ok_or_else(|| format!("No hay ningún proyecto «{needle}»."))?;
        q.projects = vec![found.id.clone()];
    }
    let filter = Filter::new(&q, catalog);
    let mut groups = query::group(catalog, &filter, MAX_GROUPED)?;
    groups.truncate(MAX_ERROR_GROUPS);
    let requests = nearby_requests(catalog, &q, &groups)?;
    Ok(groups
        .into_iter()
        .map(|group| {
            let frames = query::read_detail(catalog, &group.sample.id)
                .map(|detail| trim_frames(detail.frames))
                .unwrap_or_default();
            let nearest_request = nearest(&requests, &group.sample);
            ErrorSummary {
                group,
                frames,
                nearest_request,
            }
        })
        .collect())
}

/// Los 8 primeros frames propios y el primero de vendor, en su orden.
pub(crate) fn trim_frames(frames: Vec<Frame>) -> Vec<Frame> {
    let mut own = 0;
    let mut vendor_kept = false;
    frames
        .into_iter()
        .filter(|frame| {
            if frame.vendor {
                let keep = !vendor_kept;
                vendor_kept = true;
                keep
            } else {
                own += 1;
                own <= OWN_FRAMES
            }
        })
        .collect()
}

/// Peticiones de nginx en el periodo de los grupos.
fn nearby_requests(
    catalog: &Catalog,
    q: &LogQuery,
    groups: &[LogGroup],
) -> Result<Vec<LogEntry>, String> {
    let (Some(first), Some(last)) = (
        groups.iter().map(|g| g.first_ms).min(),
        groups.iter().map(|g| g.last_ms).max(),
    ) else {
        return Ok(Vec::new());
    };
    let nginx = LogQuery {
        projects: q.projects.clone(),
        kinds: vec![SourceKind::NginxAccess, SourceKind::NginxError],
        since_ms: Some(first - REQUEST_WINDOW_MS),
        until_ms: Some(last + REQUEST_WINDOW_MS),
        ..LogQuery::default()
    };
    let filter = Filter::new(&nginx, catalog);
    let mut found = Vec::new();
    scan(catalog, &filter, None, FILTERED_BUDGET, |entry, _, _| {
        if entry.http.is_some() {
            found.push(entry.clone());
        }
        found.len() < 5000
    })?;
    Ok(found)
}

/// La petición más próxima en el tiempo al error, del mismo proyecto: primero
/// un 5xx del log de acceso y, si no hay (Herd no lo guarda), una petición del
/// log de errores de nginx.
fn nearest(requests: &[LogEntry], sample: &LogEntry) -> Option<HttpBrief> {
    let distance = |r: &LogEntry| (r.ts_ms - sample.ts_ms).abs();
    let candidates = || {
        requests
            .iter()
            .filter(move |r| r.project == sample.project && distance(r) <= REQUEST_WINDOW_MS)
    };
    let server_error = candidates()
        .filter(|r| {
            r.kind == SourceKind::NginxAccess && r.http.as_ref().is_some_and(|h| h.status >= 500)
        })
        .min_by_key(|r| distance(r));
    let request = server_error.or_else(|| {
        candidates()
            .filter(|r| r.kind == SourceKind::NginxError)
            .min_by_key(|r| distance(r))
    })?;
    let http = request.http.as_ref()?;
    Some(HttpBrief {
        method: http.method.clone(),
        path: http.path.clone(),
        status: http.status,
        ts_ms: request.ts_ms,
    })
}

/// `(proyecto, errores, último)` desde `since_ms`. Las líneas sin proyecto van
/// a `none` (nginx) o `system` (PHP-FPM), los mismos valores que acepta
/// `LogQuery.projects`. De más reciente a más antiguo.
pub(crate) fn count_errors(catalog: &Catalog, since_ms: i64) -> Vec<(String, u32, Option<i64>)> {
    let filter = Filter::errors_since(since_ms);
    let mut counts: HashMap<String, (u32, Option<i64>)> = HashMap::new();
    for file in &catalog.files {
        scan_file(catalog, file, &filter, PAGE_BUDGET, |entry| {
            let key = match (&entry.project, entry.kind) {
                (Some(project), _) => project.clone(),
                (None, SourceKind::PhpFpm) => "system".into(),
                (None, _) => "none".into(),
            };
            let slot = counts.entry(key).or_default();
            slot.0 += 1;
            slot.1 = slot.1.max(Some(entry.ts_ms));
            true
        });
    }
    let mut list: Vec<_> = counts
        .into_iter()
        .map(|(id, (n, last))| (id, n, last))
        .collect();
    list.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    list
}

/// Vacía un archivo de un proyecto o del nginx de Elephant. Los de Herd nunca.
pub(crate) fn clear(catalog: &Catalog, file_key: &str) -> Result<(), String> {
    let key = (file_key.len() == 16)
        .then(|| u64::from_str_radix(file_key, 16).ok())
        .flatten()
        .ok_or_else(|| format!("Identificador de archivo no válido: {file_key}."))?;
    let file = catalog
        .file_by_key(key)
        .ok_or("No hay ningún archivo de log con ese identificador.")?;
    if sources::is_herd(catalog.source(file)) {
        return Err("Los logs de Herd son de Herd: Elephant Stack no los vacía.".into());
    }
    OpenOptions::new()
        .write(true)
        .open(&file.path)
        .and_then(|f| f.set_len(0))
        .map_err(|e| format!("No se pudo vaciar {}: {e}.", file.path.display()))
}

/// `30s`, `15m`, `2h`, `1d`, `1w` o una fecha ISO 8601 (con o sin hora y zona;
/// sin zona, la del Mac).
pub(crate) fn parse_since(text: &str, now_ms: i64) -> Result<i64, String> {
    let text = text.trim();
    let invalid = || format!("No entiendo «{text}»: usa 15m, 2h, 1d o una fecha ISO 8601.");
    if let Some(unit) = text.chars().last().filter(char::is_ascii_alphabetic) {
        let number = &text[..text.len() - 1];
        if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
            let amount: i64 = number.parse().map_err(|_| invalid())?;
            let unit_ms = match unit.to_ascii_lowercase() {
                's' => 1_000,
                'm' => 60_000,
                'h' => 3_600_000,
                'd' => DAY_MS,
                'w' => 7 * DAY_MS,
                _ => return Err(invalid()),
            };
            return amount
                .checked_mul(unit_ms)
                .and_then(|ms| now_ms.checked_sub(ms))
                .ok_or_else(invalid);
        }
    }
    if let Ok(ts) = text.parse::<jiff::Timestamp>() {
        return Ok(ts.as_millisecond());
    }
    let tz = jiff::tz::TimeZone::try_system().unwrap_or(jiff::tz::TimeZone::UTC);
    let civil = text
        .parse::<jiff::civil::DateTime>()
        .ok()
        .or_else(|| {
            text.parse::<jiff::civil::Date>()
                .ok()
                .map(|d| d.to_datetime(jiff::civil::Time::midnight()))
        })
        .ok_or_else(invalid)?;
    tz.to_ambiguous_timestamp(civil)
        .compatible()
        .map(|ts| ts.as_millisecond())
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(index: u32, vendor: bool) -> Frame {
        Frame {
            index,
            file: format!("/f{index}.php"),
            line: 1,
            call: String::new(),
            vendor,
            framework: vendor,
        }
    }

    #[test]
    fn frames_are_trimmed_to_eight_own_and_the_first_vendor() {
        let mut frames: Vec<Frame> = (0..3).map(|i| frame(i, true)).collect();
        frames.extend((3..15).map(|i| frame(i, false)));
        let trimmed = trim_frames(frames);
        let indexes: Vec<u32> = trimmed.iter().map(|f| f.index).collect();
        assert_eq!(indexes, [0, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn relative_and_absolute_since_values_are_parsed() {
        let now = 1_793_960_430_000;
        assert_eq!(parse_since("15m", now).unwrap(), now - 900_000);
        assert_eq!(parse_since(" 2h ", now).unwrap(), now - 7_200_000);
        assert_eq!(parse_since("1d", now).unwrap(), now - DAY_MS);
        assert_eq!(parse_since("30s", now).unwrap(), now - 30_000);
        assert_eq!(parse_since("2026-11-06T10:20:30Z", now).unwrap(), now);
        assert_eq!(parse_since("2026-11-06T11:20:30+01:00", now).unwrap(), now);
        assert!(parse_since("2026-11-06", now).is_ok());
        assert!(parse_since("2026-11-06T10:00", now).is_ok());
        let error = parse_since("ayer", now).unwrap_err();
        assert!(error.contains("ayer"));
        assert!(parse_since("5y", now).is_err());
        assert!(parse_since("", now).is_err());
    }
}
