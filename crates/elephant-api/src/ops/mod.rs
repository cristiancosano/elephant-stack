//! Las 17 operaciones de 00 §5.9. Cada una es un tipo que implementa [`Op`];
//! [`meta`] la convierte en una fila del catálogo con punteros a funciones
//! genéricas, así que no hace falta macro ni `match` a mano.

use std::path::{Path, PathBuf};

use clap::{ArgMatches, Command, FromArgMatches};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ApiError, Ctx, ErrorCode, Op, OpMeta, Output, Prepared};

mod jobs;
pub(crate) mod logs;
mod php;
mod projects;
mod services;
mod status;

/// En el orden de la tabla de 00 §5.9: lecturas primero y después mutaciones.
pub(crate) static CATALOG: &[OpMeta] = &[
    meta::<status::Status>(),
    meta::<status::ComponentsList>(),
    meta::<php::PhpVersions>(),
    meta::<projects::ProjectsList>(),
    meta::<projects::ProjectNeeds>(),
    meta::<logs::LogsSources>(),
    meta::<logs::LogsTail>(),
    meta::<logs::LogsSearch>(),
    meta::<logs::LogsErrors>(),
    meta::<logs::LogsEntry>(),
    meta::<jobs::JobStatus>(),
    meta::<services::ServiceControl>(),
    meta::<php::PhpSetGlobal>(),
    meta::<php::PhpSetProject>(),
    meta::<services::ComponentInstall>(),
    meta::<projects::ProjectStartServices>(),
    meta::<projects::ProjectCreate>(),
];

const fn meta<O: Op>() -> OpMeta {
    OpMeta {
        id: O::ID,
        cli: O::CLI,
        kind: O::KIND,
        idempotent: O::IDEMPOTENT,
        long: O::LONG,
        oldest_first: O::OLDEST_FIRST,
        about: O::about,
        input_schema: schema_of::<O::Args>,
        output_schema: schema_of::<O::Out>,
        augment: augment_of::<O>,
        from_matches: from_matches_of::<O>,
        prepare: prepare_of::<O>,
        execute: execute_of::<O>,
    }
}

fn schema_of<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default()
}

fn augment_of<O: Op>(command: Command) -> Command {
    <O::Args as clap::Args>::augment_args(command)
}

fn from_matches_of<O: Op>(matches: &ArgMatches) -> Result<Value, clap::Error> {
    let args = <O::Args as FromArgMatches>::from_arg_matches(matches)?;
    serde_json::to_value(args).map_err(|error| clap::Error::raw(clap::error::ErrorKind::InvalidValue, error))
}

fn parse<O: Op>(value: Value) -> Result<O::Args, ApiError> {
    // MCP manda `{}` o nada cuando no hay argumentos.
    let value = if value.is_null() { Value::Object(Default::default()) } else { value };
    serde_json::from_value(value).map_err(|error| ApiError::usage(format!("Argumentos no válidos: {error}")))
}

fn prepare_of<O: Op>(cx: &Ctx, value: Value) -> Result<Prepared, ApiError> {
    let mut args = parse::<O>(value)?;
    O::validate(cx, &mut args).map_err(ApiError::usage)?;
    let confirm = O::confirm_text(&args);
    let args = serde_json::to_value(&args).map_err(|error| ApiError::new(ErrorCode::Failed, error.to_string()))?;
    Ok(Prepared { args, confirm })
}

fn execute_of<O: Op>(cx: &Ctx, value: Value, on_line: &mut dyn FnMut(&str)) -> Result<Output, ApiError> {
    let args = parse::<O>(value)?;
    let out = O::run(cx, args, on_line).map_err(ApiError::from_core)?;
    let structured = serde_json::to_value(&out).map_err(|error| ApiError::new(ErrorCode::Failed, error.to_string()))?;
    Ok(Output { structured, text: O::text(&out) })
}

// ── Validación común ─────────────────────────────────────────────────────────

/// Nombre de servicio de Homebrew: `mysql`, `postgresql@18`, `php@8.4`.
pub(crate) fn check_service_name(name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'@' | b'.' | b'_' | b'+' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(format!("Nombre de servicio no válido: «{}».", crate::fit::truncate_chars(name, 64)))
    }
}

/// `8.4`, `8.4.27` o `8.4@Herd`.
pub(crate) fn check_php_version(version: &str) -> Result<(), String> {
    let (number, source) = match version.split_once('@') {
        Some((number, source)) => (number, Some(source)),
        None => (version, None),
    };
    let number_ok = !number.is_empty()
        && number.len() <= 16
        && number.split('.').count() <= 3
        && number.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    let source_ok = source.is_none_or(|s| {
        !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ')
    });
    if number_ok && source_ok {
        Ok(())
    } else {
        Err(format!("Versión de PHP no válida: «{}». Usa, por ejemplo, 8.4 o 8.4.27.", crate::fit::truncate_chars(version, 32)))
    }
}

/// Carpeta de proyecto validada y canónica (bajo `$HOME`, fuera de
/// `~/.elephant-stack` y `~/Library`).
pub(crate) fn user_dir(cx: &Ctx, dir: Option<&Path>) -> Result<PathBuf, String> {
    elephant_core::validate::user_dir(&cx.project_dir(dir))
}

// ── Enumeraciones de los argumentos ──────────────────────────────────────────

/// Nivel mínimo de log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Level {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl From<Level> for elephant_logs::LogLevel {
    fn from(level: Level) -> Self {
        use elephant_logs::LogLevel as L;
        match level {
            Level::Debug => L::Debug,
            Level::Info => L::Info,
            Level::Notice => L::Notice,
            Level::Warning => L::Warning,
            Level::Error => L::Error,
            Level::Critical => L::Critical,
            Level::Alert => L::Alert,
            Level::Emergency => L::Emergency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_names() {
        for good in ["mysql", "postgresql@18", "php@8.4", "redis"] {
            assert!(check_service_name(good).is_ok(), "{good}");
        }
        for bad in ["", "MySQL", "../x", "mysql; rm", "a b", "-x", "x\"y"] {
            assert!(check_service_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn php_versions() {
        for good in ["8", "8.4", "8.4.27", "8.4@Herd", "8.4.27@Elephant Stack"] {
            assert!(check_php_version(good).is_ok(), "{good}");
        }
        for bad in ["", "8.4.", "8..4", "8.4.1.2", "latest", "8.4@../x", "8.4; rm"] {
            assert!(check_php_version(bad).is_err(), "{bad}");
        }
    }
}
