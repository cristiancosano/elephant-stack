//! Motor de logs de Elephant Stack (02): descubre los proyectos, lee sus logs
//! de Laravel, nginx y PHP-FPM, y los sirve a la app y al canal de agentes.
//!
//! No depende de `elephant-core` ni de tokio; los textos usan `elephant-i18n`. Solo escribe en
//! `~/.elephant-stack/projects.json`, salvo [`clear`]. No oculta secretos: lo
//! que va a un agente pasa por `elephant_core::redact` (C6).
//!
//! Los archivos de log son la única fuente de verdad: no se copian ni se
//! indexan. Se leen hacia atrás por bloques, con búsqueda binaria por tiempo,
//! y el directo es un hilo de sondeo que solo existe mientras hay
//! suscripciones ([`LogHub`]).
//!
//! Las funciones libres no guardan estado (las usa la CLI); [`LogHub`]
//! reutiliza el catálogo de proyectos y fuentes durante unos segundos.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use sources::Catalog;

pub mod projects;

mod ops;
mod parse;
mod query;
mod reader;
mod sources;
mod tail;
mod util;

/// Mensaje común de lo que todavía no está disponible. Ya no lo devuelve
/// ninguna función de la crate; se conserva porque es parte del contrato. Es un
/// centinela que se compara (también en la interfaz): no se traduce.
pub const NOT_AVAILABLE: &str = "Todavía no disponible.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SourceKind {
    LaravelApp,
    NginxAccess,
    NginxError,
    PhpFpm,
}

/// Escala de Monolog / RFC 5424, de menos a más grave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ProjectKind {
    Laravel,
    Php,
    Other,
}

/// De dónde sale el proyecto. Al deduplicar gana el primero de esta lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ProjectOrigin {
    Elephant,
    HerdParked,
    HerdLinked,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Project {
    /// FNV-1a 64 de la ruta canónica, en hex (16 caracteres).
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    /// `["tienda.test"]`; vacío si no lo sirve nadie.
    pub hosts: Vec<String>,
    pub kind: ProjectKind,
    pub origin: ProjectOrigin,
    pub php_version: Option<String>,
    /// Zona horaria de los logs de Laravel (`UTC`, `Europe/Madrid`…).
    pub log_tz: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LogSource {
    /// `<kind>:<project_id>`, `nginx_error:herd`, `php_fpm:herd`,
    /// `nginx_access:elephant` o `nginx_error:elephant`. Nunca una ruta.
    pub id: String,
    pub kind: SourceKind,
    /// `None` = global; el proyecto se asigna por entrada.
    pub project: Option<String>,
    pub dir: PathBuf,
    /// Glob simple: solo `*` en el nombre del archivo.
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SourceState {
    Ok,
    Missing,
    Unreadable { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SourceStatus {
    pub source: LogSource,
    pub files: Vec<String>,
    pub state: SourceState,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExceptionInfo {
    pub class: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HttpInfo {
    pub method: String,
    pub path: String,
    pub status: u16,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub bytes: Option<u64>,
    pub duration_ms: Option<u32>,
    pub upstream_ms: Option<u32>,
    pub ip: Option<String>,
    pub ua: Option<String>,
}

/// Una entrada de log (02 §5.2). La traza no se guarda: se relee del disco con
/// [`entry`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LogEntry {
    /// `<file_key:016x>-<offset:x>`: estable entre procesos.
    pub id: String,
    pub kind: SourceKind,
    /// `None` = «Sin proyecto» o «Sistema».
    pub project: Option<String>,
    /// nginx: el host original, aunque no sea de ningún proyecto.
    pub host: Option<String>,
    /// UTC.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ts_ms: i64,
    pub level: LogLevel,
    /// Canal de Monolog (`local`, `production`…).
    pub channel: Option<String>,
    /// Primera línea, como mucho 2 KB.
    pub message: String,
    pub exception: Option<ExceptionInfo>,
    pub http: Option<HttpInfo>,
    /// Hex de 16 caracteres.
    pub fingerprint: String,
    /// Bytes de la entrada completa en el archivo.
    pub len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum StatusFilter {
    /// `Class(5)` = 5xx.
    Class(u8),
    AtLeast(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LogQuery {
    /// Vacío = todos; `none` = sin proyecto; `system` = PHP-FPM.
    pub projects: Vec<String>,
    /// Vacío = todas.
    pub kinds: Vec<SourceKind>,
    pub min_level: Option<LogLevel>,
    pub text: Option<String>,
    pub status: Option<StatusFilter>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub since_ms: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub until_ms: Option<i64>,
    /// Cursor: el id de la última entrada de la página anterior.
    pub before: Option<String>,
    /// Por defecto 200, con un tope de 2000.
    pub limit: u32,
}

impl Default for LogQuery {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            kinds: Vec::new(),
            min_level: None,
            text: None,
            status: None,
            since_ms: None,
            until_ms: None,
            before: None,
            limit: 200,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LogPage {
    pub entries: Vec<LogEntry>,
    pub next_before: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub scanned_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Frame {
    pub index: u32,
    pub file: String,
    pub line: u32,
    pub call: String,
    /// La ruta contiene `/vendor/`.
    pub vendor: bool,
    /// Laravel, Symfony o Debugbar.
    pub framework: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EntryDetail {
    pub entry: LogEntry,
    /// La entrada completa, como mucho 1 MB.
    pub body: String,
    pub frames: Vec<Frame>,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LogGroup {
    pub fingerprint: String,
    pub count: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub first_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_ms: i64,
    pub sample: LogEntry,
    pub projects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectSummary {
    pub project: Project,
    pub sources: Vec<SourceStatus>,
    /// Entradas de nivel Error o superior de las últimas 24 h.
    pub errors_24h: u32,
    /// Se llegó al tope de lectura: la interfaz muestra «500+».
    pub errors_capped: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_ms: Option<i64>,
    /// Ajuste guardado en `projects.json` (zona horaria y rutas extra), para
    /// que Ajustes ▸ Logs parta de los valores actuales.
    #[serde(rename = "override", default)]
    pub log_override: Option<ProjectOverride>,
}

/// Lote del directo: como mucho cada 100 ms y 500 entradas.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LogBatch {
    pub subscription: u32,
    pub entries: Vec<LogEntry>,
    /// Entradas descartadas porque la interfaz no las consumía.
    pub dropped: u32,
    /// Claves de los archivos que se han truncado o rotado.
    pub truncated: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectOverride {
    /// Rutas absolutas con `*` en el nombre del archivo.
    pub log_globs: Vec<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HttpBrief {
    pub method: String,
    pub path: String,
    pub status: u16,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ts_ms: i64,
}

/// Un grupo de errores con lo que un agente necesita para diagnosticarlo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ErrorSummary {
    pub group: LogGroup,
    pub frames: Vec<Frame>,
    /// La petición 5xx de nginx más próxima en el tiempo.
    pub nearest_request: Option<HttpBrief>,
}

fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// Las fuentes de un proyecto (id, nombre o host) y las globales de nginx que
/// pueden traer sus peticiones. Sin proyecto, todas. Un proyecto que no existe
/// no tiene fuentes.
pub fn sources(home: &Path, project: Option<&str>) -> Vec<SourceStatus> {
    ops::sources(&Catalog::load(home), project)
}

/// Un resumen por proyecto, en el orden de [`projects::discover`].
pub fn summaries(home: &Path) -> Vec<ProjectSummary> {
    ops::summaries(&Catalog::load(home), now_ms())
}

/// Una página de entradas, de la más nueva a la más antigua. Para la
/// siguiente, `before = next_before`.
pub fn query(home: &Path, q: &LogQuery) -> Result<LogPage, String> {
    ops::query(&Catalog::load(home), q)
}

/// El detalle de una entrada, releído del disco. Si el archivo ha cambiado y
/// en ese punto ya no empieza una entrada, devuelve un error.
pub fn entry(home: &Path, id: &str) -> Result<EntryDetail, String> {
    ops::entry(&Catalog::load(home), id)
}

/// Entradas agrupadas por huella, de la última vista a la más antigua.
pub fn groups(home: &Path, q: &LogQuery) -> Result<Vec<LogGroup>, String> {
    ops::groups(&Catalog::load(home), q)
}

/// Las últimas `lines` entradas de una fuente (≤ 500; 0 = 100), en orden
/// cronológico. `source_id` es un id de [`LogSource`], nunca una ruta: lo que
/// no es una fuente conocida se rechaza.
pub fn tail(
    home: &Path,
    source_id: &str,
    lines: u32,
    min_level: Option<LogLevel>,
    contains: Option<&str>,
) -> Result<Vec<LogEntry>, String> {
    ops::tail(&Catalog::load(home), source_id, lines, min_level, contains)
}

/// Errores (nivel Error o superior) desde `since_ms`, agrupados, con su traza
/// recortada (8 frames propios y el primero de vendor) y la petición de nginx
/// más próxima. Como mucho 20 grupos.
pub fn errors_summary(
    home: &Path,
    project: Option<&str>,
    since_ms: i64,
) -> Result<Vec<ErrorSummary>, String> {
    ops::errors_summary(&Catalog::load(home), project, since_ms)
}

/// `(project_id, errores, última)` desde `since_ms`, para el menú de la barra.
/// Lo que no es de ningún proyecto va a `none` (nginx) o `system` (PHP-FPM).
pub fn count_errors(home: &Path, since_ms: i64) -> Vec<(String, u32, Option<i64>)> {
    ops::count_errors(&Catalog::load(home), since_ms)
}

/// `30s`, `15m`, `2h`, `1d`, `1w` o una fecha ISO 8601.
pub fn parse_since(text: &str, now_ms: i64) -> Result<i64, String> {
    ops::parse_since(text, now_ms)
}

/// Vacía un archivo de log de un proyecto o de Elephant; nunca uno de Herd.
/// `file_key` es la primera parte del id de una entrada (16 caracteres hex).
pub fn clear(home: &Path, file_key: &str) -> Result<(), String> {
    ops::clear(&Catalog::load(home), file_key)
}

/// El catálogo se reutiliza este tiempo entre llamadas del hub.
const CATALOG_TTL: Duration = Duration::from_secs(3);

/// Envuelve las funciones libres con un catálogo en caché y añade el directo.
/// Con 0 suscripciones no queda ningún hilo de sondeo; al soltarlo se termina
/// el hilo y se cierran los archivos.
pub struct LogHub {
    home: PathBuf,
    catalog: Mutex<Option<(Instant, Arc<Catalog>)>>,
    live: tail::Live,
}

impl LogHub {
    pub fn new(home: PathBuf) -> Self {
        Self {
            live: tail::Live::new(home.clone()),
            home,
            catalog: Mutex::new(None),
        }
    }

    fn catalog(&self, fresh: bool) -> Arc<Catalog> {
        let mut cached = self
            .catalog
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !fresh
            && let Some((at, catalog)) = cached.as_ref()
            && at.elapsed() < CATALOG_TTL
        {
            return Arc::clone(catalog);
        }
        let catalog = Arc::new(Catalog::load(&self.home));
        *cached = Some((Instant::now(), Arc::clone(&catalog)));
        catalog
    }

    pub fn summaries(&self) -> Vec<ProjectSummary> {
        ops::summaries(&self.catalog(false), now_ms())
    }

    pub fn query(&self, q: &LogQuery) -> Result<LogPage, String> {
        ops::query(&self.catalog(false), q)
    }

    /// Si el archivo es nuevo (el catálogo en caché no lo conoce), se vuelve a
    /// mirar el disco antes de dar la entrada por perdida.
    pub fn entry(&self, id: &str) -> Result<EntryDetail, String> {
        ops::entry(&self.catalog(false), id).or_else(|_| ops::entry(&self.catalog(true), id))
    }

    pub fn groups(&self, q: &LogQuery) -> Result<Vec<LogGroup>, String> {
        ops::groups(&self.catalog(false), q)
    }

    /// `sink` devuelve `false` si el canal se ha roto; entonces se cancela la
    /// suscripción. Los lotes llegan desde el hilo de sondeo, como mucho cada
    /// 100 ms y con 500 entradas.
    pub fn subscribe(
        &self,
        q: LogQuery,
        sink: Box<dyn Fn(LogBatch) -> bool + Send + 'static>,
    ) -> u32 {
        self.live.subscribe(q, sink)
    }

    pub fn unsubscribe(&self, id: u32) {
        self.live.unsubscribe(id);
    }

    /// 0 → no queda hilo de sondeo.
    pub fn active_subscriptions(&self) -> usize {
        self.live.active()
    }

    /// Olvida el catálogo en caché: la siguiente consulta vuelve a mirar el
    /// disco. Para después de cambiar `projects.json`.
    pub fn invalidate(&self) {
        *self
            .catalog
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_ordered_by_severity() {
        assert!(LogLevel::Error > LogLevel::Warning);
        assert!(LogLevel::Emergency > LogLevel::Critical);
        assert_eq!(
            serde_json::to_string(&LogLevel::Warning).unwrap(),
            "\"warning\""
        );
    }

    #[test]
    fn queries_accept_partial_json() {
        let q: LogQuery = serde_json::from_str(
            r#"{"kinds":["laravel_app"],"min_level":"error","status":{"kind":"class","value":5}}"#,
        )
        .unwrap();
        assert_eq!(q.limit, 200);
        assert_eq!(q.kinds, [SourceKind::LaravelApp]);
        assert_eq!(q.status, Some(StatusFilter::Class(5)));
    }

    #[test]
    fn source_states_are_tagged() {
        let state = SourceState::Unreadable {
            message: "Permiso denegado".into(),
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["kind"], "unreadable");
        assert_eq!(json["message"], "Permiso denegado");
    }

    #[test]
    fn a_hub_without_subscriptions_has_no_poller() {
        let hub = LogHub::new(PathBuf::from("/nonexistent"));
        assert_eq!(hub.active_subscriptions(), 0);
        let first = hub.subscribe(LogQuery::default(), Box::new(|_| true));
        assert_ne!(
            first,
            hub.subscribe(LogQuery::default(), Box::new(|_| true))
        );
    }
}
