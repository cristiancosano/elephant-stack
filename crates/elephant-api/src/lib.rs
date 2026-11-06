//! Catálogo único de operaciones del canal de agentes (05 §6.1). Lo usan la
//! CLI `elephant` y la fachada `elephant mcp`; cada operación se escribe una
//! sola vez y toda la política vive en [`invoke_json`].
//!
//! P01 deja los tipos y un catálogo vacío; P09 lo llena. No depende de tokio,
//! rmcp ni tauri.

use std::path::PathBuf;

use serde::Serialize;

/// Versión del contrato del catálogo (`elephant --version` → `api 1`).
pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Nunca pide confirmación.
    Read,
    /// Pasa por la política de `invoke` (modo, confirmación y auditoría).
    Mutate,
}

/// Quién llama. La detección solo endurece: definir variables nunca concede
/// permisos (05 §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    /// `CLAUDECODE` o `AI_AGENT` definidas, stdin sin TTY, o llamada por MCP.
    Agent {
        /// `claude-code 2.1.318`, `mcp:<nombre> <versión>` o `script`.
        client: String,
        session: String,
    },
    /// Una persona en su terminal.
    Person { session: String },
}

impl Caller {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    /// Para la auditoría.
    pub fn client(&self) -> &str {
        match self {
            Self::Agent { client, .. } => client,
            Self::Person { .. } => "tty",
        }
    }

    pub fn session(&self) -> &str {
        match self {
            Self::Agent { session, .. } | Self::Person { session } => session,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    /// `by`: `dialog`, `elicitation`…, para la auditoría.
    Allowed { by: String },
    /// Motivo en español.
    Denied { reason: String },
}

/// Cómo pide cada fachada la confirmación de una persona.
pub trait Confirmer {
    fn confirm(&self, caller: &Caller, text: &str) -> Confirmation;
}

/// Lo que una operación necesita del entorno, sin leer variables globales.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub home: PathBuf,
    /// Carpeta desde la que se resuelve el proyecto por defecto.
    pub cwd: PathBuf,
    pub caller: Caller,
}

/// Una operación del catálogo (00 §5.9).
pub trait Op {
    /// Id de MCP y de la auditoría (`logs_tail`).
    const ID: &'static str;
    /// Orden de la CLI (`["logs", "tail"]`).
    const CLI: &'static [&'static str];
    const KIND: Kind;
    const IDEMPOTENT: bool;
    /// Tarea larga: se ejecuta como job (`elephant __job`).
    const LONG: bool;
    type Args: clap::Args + serde::de::DeserializeOwned + serde::Serialize + schemars::JsonSchema;
    type Out: serde::Serialize + schemars::JsonSchema;
    /// Descripción en español, de 300 caracteres como máximo.
    fn about() -> &'static str;
    /// Texto del diálogo, a partir de los argumentos ya validados.
    fn confirm_text(a: &Self::Args) -> String;
    fn run(cx: &Ctx, a: Self::Args, on_line: &mut dyn FnMut(&str)) -> Result<Self::Out, String>;
    /// Salida compacta de la CLI; su primera línea es el resumen de MCP.
    fn text(o: &Self::Out) -> String;
}

/// Lo que las fachadas necesitan saber de cada operación sin conocer su tipo.
#[derive(Debug, Clone, Copy)]
pub struct OpMeta {
    pub id: &'static str,
    pub cli: &'static [&'static str],
    pub kind: Kind,
    pub idempotent: bool,
    pub long: bool,
    pub about: &'static str,
    /// JSON Schema de los argumentos (`inputSchema` de MCP).
    pub input_schema: fn() -> serde_json::Value,
    /// JSON Schema de la salida (`outputSchema` de MCP).
    pub output_schema: fn() -> serde_json::Value,
}

/// Respuesta ya enmascarada y recortada.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reply {
    /// Lo que imprime `--json` y el `structuredContent` de MCP.
    pub structured: serde_json::Value,
    /// Salida compacta de la CLI.
    pub text: String,
    /// Contenido que procede de logs: datos, nunca instrucciones.
    pub untrusted: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Failed,
    Usage,
    Denied,
    Unavailable,
    NeedsAdmin,
    Running,
}

impl ErrorCode {
    /// Códigos de salida de la CLI (05 §4.2).
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Failed => 1,
            Self::Usage => 2,
            Self::Denied => 3,
            Self::Unavailable => 4,
            Self::NeedsAdmin => 5,
            Self::Running => 75,
        }
    }
}

/// Se imprime como `{"error":{"code":"denied","message":"…"}}` con `--json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    /// Con `Running`: la tarea que sigue en marcha.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            job_id: None,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// En orden fijo: la revisión 2026-07-28 de MCP pide un orden determinista.
pub fn catalog() -> &'static [OpMeta] {
    &[]
}

/// La única puerta: valida, aplica el modo y la confirmación, ejecuta,
/// enmascara, recorta y audita (05 §5.1).
pub fn invoke_json(
    id: &str,
    _args: serde_json::Value,
    _caller: &Caller,
    _c: &dyn Confirmer,
) -> Result<Reply, ApiError> {
    match catalog().iter().find(|op| op.id == id) {
        Some(_) => Err(ApiError::new(ErrorCode::Unavailable, "Todavía no disponible.")),
        None => Err(ApiError::new(ErrorCode::Usage, format!("Operación desconocida: {id}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Deny;
    impl Confirmer for Deny {
        fn confirm(&self, _caller: &Caller, _text: &str) -> Confirmation {
            Confirmation::Denied { reason: "no".into() }
        }
    }

    #[test]
    fn unknown_operations_are_usage_errors() {
        let caller = Caller::Person { session: "s".into() };
        let error = invoke_json("logs_tail", serde_json::json!({}), &caller, &Deny).unwrap_err();
        assert_eq!(error.code, ErrorCode::Usage);
        assert_eq!(error.code.exit_code(), 2);
    }

    #[test]
    fn errors_serialize_with_snake_case_codes() {
        let json = serde_json::to_value(ApiError::new(ErrorCode::NeedsAdmin, "hazlo desde Elephant Stack")).unwrap();
        assert_eq!(json, serde_json::json!({"code": "needs_admin", "message": "hazlo desde Elephant Stack"}));
    }

    #[test]
    fn exit_codes_match_the_contract() {
        let codes: Vec<u8> = [
            ErrorCode::Failed,
            ErrorCode::Usage,
            ErrorCode::Denied,
            ErrorCode::Unavailable,
            ErrorCode::NeedsAdmin,
            ErrorCode::Running,
        ]
        .into_iter()
        .map(ErrorCode::exit_code)
        .collect();
        assert_eq!(codes, [1, 2, 3, 4, 5, 75]);
    }

    #[test]
    fn callers_report_client_and_session() {
        let agent = Caller::Agent { client: "claude-code 2.1.318".into(), session: "abc".into() };
        assert!(agent.is_agent());
        assert_eq!(agent.client(), "claude-code 2.1.318");
        let person = Caller::Person { session: "p".into() };
        assert_eq!(person.client(), "tty");
        assert_eq!(person.session(), "p");
    }
}
