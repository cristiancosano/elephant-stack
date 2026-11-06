//! Presupuestos de rendimiento de 02 §10 que se pueden medir sin la app, sobre
//! datos generados en una carpeta temporal. Son lentos de preparar (escriben
//! ~120 MB) y dependen de la máquina, así que van con `#[ignore]`:
//!
//! ```sh
//! cargo test --release -p elephant-logs --test perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Imprimen las medidas para anotarlas en el PR y fallan si se pasan del
//! presupuesto con un margen de ×3 (la caché de disco y la máquina varían).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use elephant_logs::{LogHub, LogLevel, LogQuery, errors_summary, projects};

/// Carpeta temporal que se borra al soltarla.
struct Home(PathBuf);

impl Home {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("elephant-logs-perf-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(&path).unwrap())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Un `laravel.log` de unos `bytes`, con la hora actual hacia atrás (una entrada
/// por segundo) y un error con traza cada 20 entradas.
fn laravel_log(bytes: usize) -> String {
    let now = jiff::Zoned::now();
    let mut lines = Vec::new();
    let mut size = 0;
    let mut i = 0u64;
    while size < bytes {
        let ts = now
            .checked_sub(jiff::SignedDuration::from_secs(i as i64))
            .unwrap()
            .strftime("%Y-%m-%d %H:%M:%S")
            .to_string();
        let mut entry = String::new();
        if i.is_multiple_of(20) {
            let _ = write!(
                entry,
                "[{ts}] local.ERROR: Undefined variable $pedido{} {{\"exception\":\"[object] (ErrorException(code: 0): Undefined variable $pedido at /app/Http/Controllers/PedidoController.php:{})\n[stacktrace]\n#0 /app/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): fn()\n#1 {{main}}\n\"}} \n",
                i % 7,
                40 + i % 9
            );
        } else {
            let _ = writeln!(
                entry,
                "[{ts}] local.INFO: Pedido {i} confirmado para el cliente {} con {} artículos",
                i % 977,
                i % 13
            );
        }
        size += entry.len();
        lines.push(entry);
        i += 1;
    }
    lines.reverse();
    lines.concat()
}

/// `count` proyectos Laravel en `~/Code`, cada uno con un log de `bytes`.
fn projects_with_logs(home: &Path, count: usize, bytes: usize) {
    let code = home.join("Code");
    let log = laravel_log(bytes);
    for n in 0..count {
        let root = code.join(format!("proyecto{n:02}"));
        fs::create_dir_all(root.join("storage/logs")).unwrap();
        fs::write(root.join("artisan"), "").unwrap();
        fs::write(root.join("storage/logs/laravel.log"), &log).unwrap();
    }
    projects::add_root(home, &code).unwrap();
}

fn report(label: &str, took: Duration, budget_ms: u64) {
    let ms = took.as_secs_f64() * 1000.0;
    println!("{label}: {ms:.1} ms (presupuesto {budget_ms} ms)");
    assert!(
        ms <= (budget_ms * 3) as f64,
        "{label}: {ms:.1} ms, más de tres veces el presupuesto"
    );
}

#[test]
#[ignore = "rendimiento: escribe ~20 MB y mide tiempos"]
fn opening_the_view_with_20_projects() {
    let home = Home::new("20");
    projects_with_logs(home.path(), 20, 1_000_000);

    // La vista pide a la vez los proyectos (selector y contadores) y la primera
    // página; la lista aparece con la página.
    let start = Instant::now();
    let hub = LogHub::new(home.path().to_path_buf());
    let page = hub.query(&LogQuery::default()).unwrap();
    report(
        "Abrir Logs con 20 proyectos (primera página, catálogo en frío)",
        start.elapsed(),
        150,
    );
    let start = Instant::now();
    let summaries = hub.summaries();
    println!(
        "  resumen de los 20 proyectos (errores de 24 h): {:.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(summaries.len(), 20);
    assert_eq!(page.entries.len(), 200);

    // `elephant logs errors` en frío: catálogo nuevo en cada llamada (la CLI).
    let since = jiff::Timestamp::now().as_millisecond() - 3_600_000;
    let start = Instant::now();
    let groups = errors_summary(home.path(), None, since).unwrap();
    report(
        "Resumen de errores en frío con 20 proyectos (1 h)",
        start.elapsed(),
        300,
    );
    assert!(!groups.is_empty());
}

#[test]
#[ignore = "rendimiento: escribe ~100 MB y mide tiempos"]
fn text_search_over_100_mb() {
    let home = Home::new("100mb");
    projects_with_logs(home.path(), 1, 100_000_000);
    let hub = LogHub::new(home.path().to_path_buf());
    // Calienta la caché de disco y el catálogo, como en la vista abierta.
    let _ = hub.query(&LogQuery::default()).unwrap();

    let q = LogQuery {
        text: Some("texto que no aparece".into()),
        ..LogQuery::default()
    };
    let start = Instant::now();
    let page = hub.query(&q).unwrap();
    let took = start.elapsed();
    println!(
        "  leídos {:.1} MB, {} entradas, sigue: {}",
        page.scanned_bytes as f64 / 1e6,
        page.entries.len(),
        page.next_before.is_some()
    );
    report("Texto sobre un log de 100 MB (primera página)", took, 150);

    let q = LogQuery {
        min_level: Some(LogLevel::Error),
        limit: 50,
        ..LogQuery::default()
    };
    let start = Instant::now();
    let page = hub.query(&q).unwrap();
    report(
        "Errores en un log de 100 MB (50 entradas)",
        start.elapsed(),
        150,
    );
    assert_eq!(page.entries.len(), 50);
}
