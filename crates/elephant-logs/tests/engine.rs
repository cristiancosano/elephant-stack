//! Criterios de aceptación de 02 §10 sobre un `HOME` temporal con fixtures
//! anonimizados. Nunca se toca el `HOME` real.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use elephant_logs::{
    LogHub, LogLevel, LogQuery, ProjectKind, ProjectOrigin, SourceKind, StatusFilter, count_errors,
    entry, errors_summary, groups, parse_since, projects, query, sources, summaries, tail,
};
use jiff::tz::TimeZone;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
const HERD: &str = "Library/Application Support/Herd";

/// Carpeta temporal que se borra al soltarla.
struct Home(PathBuf);

impl Home {
    fn empty(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "elephant-logs-it-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(&path).unwrap())
    }

    /// Herd con `tienda` (Laravel, UTC), `blog` (JsonFormatter) y `api`
    /// (enlazado), los logs de Herd y el access log de Elephant.
    fn herd(label: &str, daily: bool) -> Self {
        let home = Self::empty(label);
        let h = home.path();
        home.fixture(
            "valet-config.json",
            &format!("{HERD}/config/valet/config.json"),
        );
        let tienda = h.join("Herd/tienda");
        write(&tienda.join("artisan"), "");
        write(
            &tienda.join("config/app.php"),
            "<?php return [\n    'timezone' => 'UTC',\n];\n",
        );
        home.fixture("laravel.log", "Herd/tienda/storage/logs/laravel.log");
        if daily {
            home.fixture(
                "laravel-2026-11-06.log",
                "Herd/tienda/storage/logs/laravel-2026-11-06.log",
            );
            home.fixture(
                "laravel-2026-11-07.log",
                "Herd/tienda/storage/logs/laravel-2026-11-07.log",
            );
        }
        write(&h.join("Herd/blog/composer.json"), "{}");
        home.fixture("laravel-json.log", "Herd/blog/storage/logs/laravel.log");
        write(&h.join("Code/api/public/index.php"), "<?php");
        let sites = h.join(HERD).join("config/valet/Sites");
        fs::create_dir_all(&sites).unwrap();
        std::os::unix::fs::symlink(h.join("Code/api"), sites.join("api")).unwrap();
        home.fixture("nginx-error.log", &format!("{HERD}/Log/nginx-error.log"));
        home.fixture("php-fpm.log", &format!("{HERD}/Log/php-fpm.log"));
        home.fixture("nginx-access.log", ".elephant-stack/logs/nginx/access.log");
        home
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Copia un fixture cambiando `/HOME` por la carpeta temporal.
    fn fixture(&self, name: &str, to: &str) {
        let text = fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap();
        write(
            &self.0.join(to),
            &text.replace("/HOME", &self.0.display().to_string()),
        );
    }

    fn project_id(&self, name: &str) -> String {
        projects::discover(self.path())
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
            .id
    }

    fn tienda_log(&self) -> PathBuf {
        self.0.join("Herd/tienda/storage/logs/laravel.log")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn append(path: &Path, text: &str) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

fn utc_ms(text: &str) -> i64 {
    text.parse::<jiff::Timestamp>().unwrap().as_millisecond()
}

/// La hora local del Mac como la escribe nginx.
fn nginx_local(ms: i64) -> String {
    let ts = jiff::Timestamp::from_millisecond(ms).unwrap();
    ts.to_zoned(TimeZone::try_system().unwrap_or(TimeZone::UTC))
        .strftime("%Y/%m/%d %H:%M:%S")
        .to_string()
}

fn messages(entries: &[elephant_logs::LogEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.message.as_str()).collect()
}

fn only(project: &str, kinds: &[SourceKind]) -> LogQuery {
    LogQuery {
        projects: vec![project.into()],
        kinds: kinds.to_vec(),
        limit: 2000,
        ..LogQuery::default()
    }
}

// 8 · Descubrimiento.
#[test]
fn herd_sites_are_discovered_with_hosts_and_origins() {
    let home = Home::herd("discover", false);
    let found = projects::discover(home.path());
    let summary: Vec<(&str, ProjectKind, ProjectOrigin, &[String])> = found
        .iter()
        .map(|p| (p.name.as_str(), p.kind, p.origin, p.hosts.as_slice()))
        .collect();
    assert_eq!(
        summary,
        [
            (
                "api",
                ProjectKind::Php,
                ProjectOrigin::HerdLinked,
                &["api.test".to_string()][..]
            ),
            (
                "blog",
                ProjectKind::Php,
                ProjectOrigin::HerdParked,
                &["blog.test".to_string()][..]
            ),
            (
                "tienda",
                ProjectKind::Laravel,
                ProjectOrigin::HerdParked,
                &["tienda.test".to_string()][..]
            ),
        ]
    );
    assert_eq!(found[2].log_tz, "UTC");
}

// 1 · Laravel multilínea, excepción previa y entrada a medias.
#[test]
fn laravel_entries_are_parsed_and_the_half_written_one_completes() {
    let home = Home::herd("laravel", false);
    let tienda = home.project_id("tienda");
    let q = only(&tienda, &[SourceKind::LaravelApp]);
    let page = query(home.path(), &q).unwrap();
    assert_eq!(page.next_before, None);
    assert_eq!(
        messages(&page.entries),
        [
            "Pasarela de pago caída",
            "Undefined variable $cliente",
            "Petición externa",
            "SQLSTATE[42S02]: Base table or view not found: 1146 Table 'tienda.cupones' doesn't exist",
            "Stock bajo para el producto 77",
            "Pedido creado",
        ]
    );
    let sql = &page.entries[3];
    assert_eq!(sql.level, LogLevel::Error);
    let exception = sql.exception.as_ref().unwrap();
    assert_eq!(exception.class, "Illuminate\\Database\\QueryException");
    assert!(
        exception.message.ends_with("where `codigo` = OTOÑO)"),
        "{}",
        exception.message
    );
    assert!(
        exception
            .file
            .as_deref()
            .unwrap()
            .ends_with("Illuminate/Database/Connection.php")
    );
    assert_eq!(exception.line, Some(829));
    assert_eq!(page.entries[1].exception.as_ref().unwrap().line, Some(22));

    // El detalle: frames de la primera traza, vendor marcado.
    let detail = entry(home.path(), &sql.id).unwrap();
    assert_eq!(detail.frames.len(), 6);
    assert!(detail.frames[0].vendor && detail.frames[0].framework);
    assert!(!detail.frames[1].vendor);
    assert!(detail.body.contains("[previous exception]"));
    let info = entry(home.path(), &page.entries[5].id).unwrap();
    assert_eq!(info.context.unwrap()["order"], 1042);

    let partial = page.entries[0].clone();
    let rest = fs::read_to_string(Path::new(FIXTURES).join("laravel-tail.log")).unwrap();
    append(&home.tienda_log(), &rest);
    let page = query(home.path(), &q).unwrap();
    assert_eq!(page.entries.len(), 7);
    let completed = page.entries.iter().find(|e| e.id == partial.id).unwrap();
    assert!(completed.len > partial.len);
    assert_eq!(entry(home.path(), &partial.id).unwrap().frames.len(), 2);
}

// 2 · Archivos diarios fusionados por fecha.
#[test]
fn daily_files_are_merged_by_time() {
    let home = Home::herd("daily", true);
    let tienda = home.project_id("tienda");
    let page = query(home.path(), &only(&tienda, &[SourceKind::LaravelApp])).unwrap();
    let times: Vec<i64> = page.entries.iter().map(|e| e.ts_ms).collect();
    assert!(times.windows(2).all(|w| w[0] >= w[1]), "orden descendente");
    let top: Vec<&str> = messages(&page.entries).into_iter().take(5).collect();
    assert_eq!(
        top,
        [
            "Cierre de caja fallido: descuadre de 7 euros",
            "Nuevo día",
            "Último aviso del día",
            "Escrita en el archivo del día siguiente por la zona horaria",
            "Cierre de caja fallido: descuadre de 12 euros",
        ]
    );
    let source = sources(home.path(), Some("tienda"));
    assert_eq!(
        source[0].files,
        [
            "laravel-2026-11-06.log",
            "laravel-2026-11-07.log",
            "laravel.log"
        ]
    );
}

// 3 · nginx por host.
#[test]
fn nginx_lines_go_to_their_project_by_host() {
    let home = Home::herd("hosts", false);
    let tienda = home.project_id("tienda");
    let q = LogQuery {
        kinds: vec![SourceKind::NginxError],
        ..LogQuery::default()
    };
    let page = query(home.path(), &q).unwrap();
    let by_host: BTreeMap<Option<&str>, Option<&str>> = page
        .entries
        .iter()
        .map(|e| (e.host.as_deref(), e.project.as_deref()))
        .collect();
    assert_eq!(by_host[&Some("tienda.test")], Some(tienda.as_str()));
    assert_eq!(
        by_host[&Some("api.tienda.test")],
        Some(tienda.as_str()),
        "subdominio"
    );
    assert_eq!(by_host[&Some("desconocido.test")], None);
    assert_eq!(by_host[&None], None, "línea de arranque");

    let none = query(
        home.path(),
        &LogQuery {
            projects: vec!["none".into()],
            ..LogQuery::default()
        },
    )
    .unwrap();
    assert!(
        none.entries
            .iter()
            .all(|e| e.project.is_none() && e.kind != SourceKind::PhpFpm)
    );
    assert!(
        none.entries
            .iter()
            .any(|e| e.host.as_deref() == Some("desconocido.test"))
    );
    let system = query(
        home.path(),
        &LogQuery {
            projects: vec!["system".into()],
            ..LogQuery::default()
        },
    )
    .unwrap();
    assert_eq!(system.entries.len(), 4);
    assert!(system.entries.iter().all(|e| e.kind == SourceKind::PhpFpm));

    // Por nombre también vale, e incluye sus líneas de nginx y del access log.
    let mine = query(
        home.path(),
        &LogQuery {
            projects: vec!["tienda".into()],
            ..LogQuery::default()
        },
    )
    .unwrap();
    let kinds: HashSet<SourceKind> = mine.entries.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        HashSet::from([
            SourceKind::LaravelApp,
            SourceKind::NginxError,
            SourceKind::NginxAccess
        ])
    );
    let errors = query(
        home.path(),
        &LogQuery {
            status: Some(StatusFilter::Class(5)),
            ..LogQuery::default()
        },
    )
    .unwrap();
    assert_eq!(messages(&errors.entries), ["GET /cuenta 500"]);
}

// Paginación: el cursor sigue donde se quedó, sin repetir ni perder.
#[test]
fn pages_follow_the_cursor_without_gaps_or_duplicates() {
    let home = Home::herd("pages", true);
    let all = query(
        home.path(),
        &LogQuery {
            limit: 2000,
            ..LogQuery::default()
        },
    )
    .unwrap();
    assert!(all.entries.len() > 20);
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = query(
            home.path(),
            &LogQuery {
                limit: 3,
                before: before.clone(),
                ..LogQuery::default()
            },
        )
        .unwrap();
        seen.extend(page.entries.iter().map(|e| e.id.clone()));
        match page.next_before {
            Some(next) => before = Some(next),
            None => break,
        }
    }
    let expected: Vec<String> = all.entries.iter().map(|e| e.id.clone()).collect();
    assert_eq!(seen, expected);
    assert!(
        query(
            home.path(),
            &LogQuery {
                before: Some("zz".into()),
                ..LogQuery::default()
            }
        )
        .is_err()
    );
}

// 5 · Archivo grande: primera página con 4 MB como mucho y `since` por búsqueda binaria.
#[test]
fn big_files_are_read_from_the_end_and_by_time() {
    let home = Home::empty("big");
    let root = home.path().join("Code/grande");
    write(&root.join("artisan"), "");
    let log = root.join("storage/logs/laravel.log");
    write(&log, "");
    let start = utc_ms("2026-10-01T00:00:00Z");
    let mut file = std::io::BufWriter::new(fs::OpenOptions::new().append(true).open(&log).unwrap());
    let lines = 400_000;
    for i in 0..lines {
        let ts = jiff::Timestamp::from_millisecond(start + i * 6_000).unwrap();
        let level = if i % 1000 == 0 { "ERROR" } else { "INFO" };
        writeln!(
            file,
            "[{}] local.{level}: Petición {i} servida {}",
            ts.strftime("%Y-%m-%d %H:%M:%S"),
            "·".repeat(20)
        )
        .unwrap();
    }
    drop(file);
    assert!(fs::metadata(&log).unwrap().len() > 30 * 1024 * 1024);
    projects::add_root(home.path(), &home.path().join("Code")).unwrap();

    let first = query(home.path(), &LogQuery::default()).unwrap();
    assert_eq!(first.entries.len(), 200);
    assert!(
        first.scanned_bytes <= 4 * 1024 * 1024 + 64 * 1024,
        "leídos {}",
        first.scanned_bytes
    );
    assert!(first.next_before.is_some());

    let last = start + (lines - 1) * 6_000;
    let since = last - 3_600_000;
    let q = LogQuery {
        since_ms: Some(since),
        limit: 2000,
        ..LogQuery::default()
    };
    let page = query(home.path(), &q).unwrap();
    assert_eq!(page.entries.len(), 601);
    assert!(page.entries.iter().all(|e| e.ts_ms >= since));
    assert!(page.scanned_bytes < 10 * 1024 * 1024);

    // `until` lejos del final: búsqueda binaria, no lectura entera.
    let until = start + 3_600_000;
    let q = LogQuery {
        until_ms: Some(until),
        limit: 50,
        ..LogQuery::default()
    };
    let page = query(home.path(), &q).unwrap();
    assert_eq!(page.entries[0].ts_ms, until);
    assert!(
        page.scanned_bytes < 10 * 1024 * 1024,
        "leídos {}",
        page.scanned_bytes
    );

    // Texto y nivel sobre todo el archivo, paginando con el cursor.
    let mut errors = 0;
    let mut before = None;
    loop {
        // Sin distinguir mayúsculas ASCII (la Ó no se pliega: 02 §5.4).
        let q = LogQuery {
            text: Some("PETICión 39".into()),
            min_level: Some(LogLevel::Error),
            before,
            limit: 2000,
            ..LogQuery::default()
        };
        let page = query(home.path(), &q).unwrap();
        errors += page.entries.len();
        match page.next_before {
            Some(next) => before = Some(next),
            None => break,
        }
    }
    // Errores (i % 1000 == 0) cuyo número empieza por 39: 39000 y 390000..=399000.
    assert_eq!(errors, 11);
}

// 6 · El id de una entrada no apunta a contenido equivocado.
#[test]
fn entries_of_changed_files_are_reported_as_gone() {
    let home = Home::herd("changed", false);
    let page = query(
        home.path(),
        &only(&home.project_id("tienda"), &[SourceKind::LaravelApp]),
    )
    .unwrap();
    let id = page.entries[4].id.clone();
    assert!(entry(home.path(), &id).is_ok());
    write(
        &home.tienda_log(),
        "[2026-11-06 09:00:00] local.INFO: x\nlínea de continuación larga, larga, larga\n",
    );
    let error = entry(home.path(), &id).unwrap_err();
    assert_eq!(error, "La entrada ya no existe: el archivo ha cambiado.");
    assert!(
        entry(home.path(), "0000000000000000-0")
            .unwrap_err()
            .contains("ya no existe")
    );
    assert!(
        entry(home.path(), "../../.env")
            .unwrap_err()
            .contains("no válido")
    );
}

// 7 · Zona horaria del proyecto frente a la hora local de nginx.
#[test]
fn project_timezones_order_correctly_against_nginx() {
    let home = Home::herd("tz", false);
    let error_ms = utc_ms("2026-11-06T09:10:30Z");
    write(
        &home.path().join(HERD).join("Log/nginx-error.log"),
        &format!(
            "{} [error] 1#1: *1 upstream timed out, client: 127.0.0.1, server: , request: \"POST /carrito HTTP/1.1\", host: \"tienda.test\"\n",
            nginx_local(error_ms + 1_000)
        ),
    );
    let tienda = home.project_id("tienda");
    let q = LogQuery {
        projects: vec![tienda],
        since_ms: Some(error_ms - 1),
        until_ms: Some(error_ms + 5_000),
        ..LogQuery::default()
    };
    let page = query(home.path(), &q).unwrap();
    assert_eq!(page.entries.len(), 2);
    assert_eq!(page.entries[0].kind, SourceKind::NginxError);
    assert_eq!(page.entries[0].ts_ms, error_ms + 1_000);
    assert_eq!(page.entries[1].ts_ms, error_ms);
}

#[test]
fn errors_are_summarised_with_trimmed_traces_and_the_nearest_request() {
    let home = Home::herd("summary", false);
    let sql_ms = utc_ms("2026-11-06T09:10:30Z");
    write(
        &home.path().join(HERD).join("Log/nginx-error.log"),
        &format!(
            "{} [error] 1#1: *1 upstream timed out, client: 127.0.0.1, server: , request: \"POST /carrito HTTP/1.1\", host: \"tienda.test\"\n",
            nginx_local(sql_ms + 2_000)
        ),
    );
    let since = utc_ms("2026-11-06T00:00:00Z");
    let summary = errors_summary(home.path(), Some("tienda.test"), since).unwrap();
    let classes: Vec<Option<&str>> = summary
        .iter()
        .map(|s| s.group.sample.exception.as_ref().map(|e| e.class.as_str()))
        .collect();
    assert!(classes.contains(&Some("Illuminate\\Database\\QueryException")));
    let sql = summary
        .iter()
        .find(|s| s.group.sample.ts_ms == sql_ms)
        .unwrap();
    assert_eq!(sql.frames.len(), 5, "4 propios y el primero de vendor");
    let request = sql.nearest_request.as_ref().unwrap();
    assert_eq!(
        (request.method.as_str(), request.path.as_str()),
        ("POST", "/carrito")
    );
    // El 500 del access log de Elephant gana frente a nginx error.
    let undefined = summary
        .iter()
        .find(|s| s.group.sample.message == "Undefined variable $cliente")
        .unwrap();
    assert_eq!(undefined.nearest_request.as_ref().unwrap().status, 500);
    assert!(errors_summary(home.path(), Some("no-existe"), since).is_err());
}

#[test]
fn groups_join_messages_that_differ_only_in_numbers() {
    let home = Home::herd("groups", true);
    let q = LogQuery {
        min_level: Some(LogLevel::Error),
        projects: vec!["tienda".into()],
        ..LogQuery::default()
    };
    let groups = groups(home.path(), &q).unwrap();
    let cierre = groups
        .iter()
        .find(|g| g.sample.message.starts_with("Cierre de caja"))
        .unwrap();
    assert_eq!(cierre.count, 2);
    assert_eq!(
        cierre.sample.message, "Cierre de caja fallido: descuadre de 7 euros",
        "la más reciente"
    );
    assert!(cierre.first_ms < cierre.last_ms);
}

#[test]
fn errors_are_counted_per_project_with_pseudo_projects() {
    let home = Home::herd("count", false);
    let counts = count_errors(home.path(), utc_ms("2026-11-06T00:00:00Z"));
    let map: BTreeMap<&str, u32> = counts.iter().map(|(id, n, _)| (id.as_str(), *n)).collect();
    let tienda = home.project_id("tienda");
    // 3 de Laravel (ERROR, ERROR, CRITICAL), 1 de nginx error y el 500 del access log.
    assert_eq!(map[tienda.as_str()], 5);
    assert_eq!(map["none"], 1);
    assert_eq!(map["system"], 1);
    assert_eq!(map[home.project_id("blog").as_str()], 1);
}

#[test]
fn summaries_count_the_last_24_hours() {
    let home = Home::herd("summaries", false);
    // Sin nginx: sus fixtures tienen fecha fija y podrían caer en las últimas 24 h.
    write(&home.path().join(HERD).join("Log/nginx-error.log"), "");
    write(
        &home.path().join(".elephant-stack/logs/nginx/access.log"),
        "",
    );
    let now = jiff::Timestamp::now().as_millisecond();
    let recent = jiff::Timestamp::from_millisecond(now - 60_000)
        .unwrap()
        .strftime("%Y-%m-%d %H:%M:%S");
    write(
        &home.tienda_log(),
        &format!(
            "[2020-01-01 00:00:00] local.ERROR: viejo\n[{recent}] local.ERROR: reciente\n[{recent}] local.INFO: info\n"
        ),
    );
    let list = summaries(home.path());
    let tienda = list.iter().find(|s| s.project.name == "tienda").unwrap();
    assert_eq!(tienda.errors_24h, 1);
    assert!(!tienda.errors_capped);
    assert_eq!(tienda.sources.len(), 1);
    assert_eq!(
        tienda.last_ms.map(|ms| ms / 1000),
        Some((now - 60_000) / 1000)
    );
    assert_eq!(list.len(), 3);
}

#[test]
fn tail_reads_a_source_by_id_and_rejects_paths() {
    let home = Home::herd("tail", false);
    let id = format!("laravel_app:{}", home.project_id("tienda"));
    let last = tail(home.path(), &id, 2, None, None).unwrap();
    assert_eq!(
        messages(&last),
        ["Undefined variable $cliente", "Pasarela de pago caída"],
        "cronológico"
    );
    let errors = tail(
        home.path(),
        &id,
        100,
        Some(LogLevel::Error),
        Some("cupones"),
    )
    .unwrap();
    assert_eq!(errors.len(), 1);
    let fpm = tail(home.path(), "php_fpm:herd", 0, None, None).unwrap();
    assert_eq!(fpm.len(), 4);
    assert!(tail(home.path(), "../../.env", 10, None, None).is_err());
    assert!(tail(home.path(), "/etc/passwd", 10, None, None).is_err());
}

#[test]
fn clear_empties_project_logs_but_never_herd_ones() {
    let home = Home::herd("clear", false);
    let page = query(home.path(), &LogQuery::default()).unwrap();
    let key_of = |kind: SourceKind| {
        page.entries.iter().find(|e| e.kind == kind).unwrap().id[..16].to_string()
    };
    let herd = key_of(SourceKind::NginxError);
    assert!(
        elephant_logs::clear(home.path(), &herd)
            .unwrap_err()
            .contains("Herd")
    );
    assert!(
        fs::metadata(home.path().join(HERD).join("Log/nginx-error.log"))
            .unwrap()
            .len()
            > 0
    );
    let laravel = page
        .entries
        .iter()
        .find(|e| e.message == "Pasarela de pago caída")
        .unwrap()
        .id[..16]
        .to_string();
    elephant_logs::clear(home.path(), &laravel).unwrap();
    assert_eq!(fs::metadata(home.tienda_log()).unwrap().len(), 0);
    assert!(elephant_logs::clear(home.path(), "nope").is_err());
}

// 10 · Nada escribe fuera de `~/.elephant-stack/` (ni dentro, salvo `projects.json`).
#[test]
fn reads_never_write_to_the_home() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home::herd("readonly", true);
    let snapshot = |root: &Path| -> BTreeMap<PathBuf, (u64, std::time::SystemTime)> {
        let mut out = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap().flatten() {
                let meta = fs::symlink_metadata(entry.path()).unwrap();
                if meta.is_dir() {
                    stack.push(entry.path());
                }
                out.insert(entry.path(), (meta.len(), meta.modified().unwrap()));
            }
        }
        out
    };
    let before = snapshot(home.path());
    let dirs: Vec<PathBuf> = before
        .keys()
        .filter(|p| p.is_dir() && !p.is_symlink())
        .cloned()
        .collect();
    for dir in dirs.iter().chain([home.path().to_path_buf()].iter()) {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
    }

    let tienda = home.project_id("tienda");
    let _ = projects::discover(home.path());
    let _ = summaries(home.path());
    let page = query(home.path(), &LogQuery::default()).unwrap();
    let _ = entry(home.path(), &page.entries[0].id).unwrap();
    let _ = groups(home.path(), &LogQuery::default()).unwrap();
    let _ = errors_summary(home.path(), None, 0).unwrap();
    let _ = count_errors(home.path(), 0);
    let _ = tail(
        home.path(),
        &format!("laravel_app:{tienda}"),
        10,
        None,
        None,
    )
    .unwrap();
    let _ = sources(home.path(), None);
    let _ = parse_since("1h", 0);
    let hub = LogHub::new(home.path().to_path_buf());
    let _ = hub.summaries();
    drop(hub);

    for dir in dirs.iter().chain([home.path().to_path_buf()].iter()) {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn the_hub_serves_queries_and_live_batches_and_stops_with_no_subscriptions() {
    let home = Home::herd("hub", false);
    let hub = LogHub::new(home.path().to_path_buf());
    assert_eq!(hub.summaries().len(), 3);
    let tienda = home.project_id("tienda");
    let first = hub
        .query(&only(&tienda, &[SourceKind::LaravelApp]))
        .unwrap();
    assert!(hub.entry(&first.entries[0].id).is_ok());

    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let q = LogQuery {
        projects: vec![tienda],
        ..LogQuery::default()
    };
    let id = hub.subscribe(
        q,
        Box::new(move |batch| tx.lock().unwrap().send(batch).is_ok()),
    );
    assert_eq!(hub.active_subscriptions(), 1);
    std::thread::sleep(Duration::from_millis(300));
    let rest = fs::read_to_string(Path::new(FIXTURES).join("laravel-tail.log")).unwrap();
    let written = Instant::now();
    append(&home.tienda_log(), &rest);
    // Se suscribió a mitad de línea: lo primero llega como línea suelta, y
    // después la entrada nueva.
    let mut got = Vec::new();
    let found =
        |got: &[elephant_logs::LogEntry]| got.iter().any(|e| e.message == "Reintento programado");
    while !found(&got) && written.elapsed() < Duration::from_secs(3) {
        if let Ok(batch) = rx.recv_timeout(Duration::from_millis(50)) {
            got.extend(batch.entries);
        }
    }
    assert!(found(&got));
    assert!(
        written.elapsed() < Duration::from_secs(1),
        "la última entrada en menos de 1 s"
    );
    hub.unsubscribe(id);
    assert_eq!(hub.active_subscriptions(), 0);
}
