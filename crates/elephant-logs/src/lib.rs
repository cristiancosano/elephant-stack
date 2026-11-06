//! Motor de logs de Elephant Stack (02): descubre los proyectos, lee sus logs
//! de Laravel, nginx y PHP-FPM, y los sirve a la app y al canal de agentes.
//!
//! No depende de `elephant-core` ni de tokio. Solo escribe en
//! `~/.elephant-stack/projects.json`, salvo [`clear`]. No oculta secretos: lo
//! que va a un agente pasa por `elephant_core::redact` (C6).
//!
//! P01 deja los tipos y las firmas; P03 implementa la lógica. Las funciones
//! sin implementar devuelven [`NOT_AVAILABLE`] o un valor vacío.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

pub mod projects;

mod parse;
mod query;
mod reader;
mod sources;
mod tail;

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

pub fn sources(_home: &Path, _project: Option<&str>) -> Vec<SourceStatus> {
    Vec::new()
}

pub fn summaries(_home: &Path) -> Vec<ProjectSummary> {
    Vec::new()
}

pub fn query(_home: &Path, _q: &LogQuery) -> Result<LogPage, String> {
    Err(NOT_AVAILABLE.into())
}

pub fn entry(_home: &Path, _id: &str) -> Result<EntryDetail, String> {
    Err(NOT_AVAILABLE.into())
}

pub fn groups(_home: &Path, _q: &LogQuery) -> Result<Vec<LogGroup>, String> {
    Err(NOT_AVAILABLE.into())
}

/// `source_id` es un id de [`LogSource`], nunca una ruta.
pub fn tail(
    _home: &Path,
    _source_id: &str,
    _lines: u32,
    _min_level: Option<LogLevel>,
    _contains: Option<&str>,
) -> Result<Vec<LogEntry>, String> {
    Err(NOT_AVAILABLE.into())
}

pub fn errors_summary(
    _home: &Path,
    _project: Option<&str>,
    _since_ms: i64,
) -> Result<Vec<ErrorSummary>, String> {
    Err(NOT_AVAILABLE.into())
}

/// `(project_id, errores, última)` desde `since_ms`, para el menú de la barra.
pub fn count_errors(_home: &Path, _since_ms: i64) -> Vec<(String, u32, Option<i64>)> {
    Vec::new()
}

/// `15m`, `2h`, `1d` o una fecha ISO 8601.
pub fn parse_since(_text: &str, _now_ms: i64) -> Result<i64, String> {
    Err(NOT_AVAILABLE.into())
}

/// Vacía un archivo de log de un proyecto o de Elephant; nunca uno de Herd.
pub fn clear(_home: &Path, _file_key: &str) -> Result<(), String> {
    Err(NOT_AVAILABLE.into())
}

/// Envuelve las funciones libres con caché y añade el directo. Con 0
/// suscripciones no queda ningún hilo de sondeo.
pub struct LogHub {
    home: PathBuf,
    next_subscription: AtomicU32,
}

impl LogHub {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            next_subscription: AtomicU32::new(1),
        }
    }

    pub fn summaries(&self) -> Vec<ProjectSummary> {
        summaries(&self.home)
    }

    pub fn query(&self, q: &LogQuery) -> Result<LogPage, String> {
        query(&self.home, q)
    }

    pub fn entry(&self, id: &str) -> Result<EntryDetail, String> {
        entry(&self.home, id)
    }

    pub fn groups(&self, q: &LogQuery) -> Result<Vec<LogGroup>, String> {
        groups(&self.home, q)
    }

    /// `sink` devuelve `false` si el canal se ha roto; entonces se cancela la
    /// suscripción.
    pub fn subscribe(&self, _q: LogQuery, _sink: Box<dyn Fn(LogBatch) -> bool + Send + 'static>) -> u32 {
        self.next_subscription.fetch_add(1, Ordering::Relaxed)
    }

    pub fn unsubscribe(&self, _id: u32) {}

    /// 0 → no queda hilo de sondeo.
    pub fn active_subscriptions(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_ordered_by_severity() {
        assert!(LogLevel::Error > LogLevel::Warning);
        assert!(LogLevel::Emergency > LogLevel::Critical);
        assert_eq!(serde_json::to_string(&LogLevel::Warning).unwrap(), "\"warning\"");
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
        let state = SourceState::Unreadable { message: "Permiso denegado".into() };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["kind"], "unreadable");
        assert_eq!(json["message"], "Permiso denegado");
    }

    #[test]
    fn a_hub_without_subscriptions_has_no_poller() {
        let hub = LogHub::new(PathBuf::from("/nonexistent"));
        assert_eq!(hub.active_subscriptions(), 0);
        let first = hub.subscribe(LogQuery::default(), Box::new(|_| true));
        assert_ne!(first, hub.subscribe(LogQuery::default(), Box::new(|_| true)));
    }
}
