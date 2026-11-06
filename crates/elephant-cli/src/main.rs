//! CLI `elephant`: el canal principal entre los agentes y Elephant Stack
//! (05 §4). El árbol de órdenes se genera recorriendo el catálogo de
//! `elephant-api`, y cada orden acaba en `elephant_api::invoke_with`, que
//! aplica toda la política. Aquí solo se traduce la entrada y la salida.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Arg, ArgAction, ArgMatches, Command};
use elephant_api::{ApiError, DialogConfirmer, ErrorCode, OpMeta, SystemRuntime, catalog, plugin, schema, watch};
use elephant_api::elephant_core::audit::{self, AuditEntry};
use elephant_api::elephant_core::{self, paths, settings, tr, tr_str};

/// `elephant --version` → `elephant 0.1.0 (api 1)`.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (api 1)");
const DOCS_SCHEMA: &str = "docs/agent-api.json";
const DOCS_REFERENCE: &str = "crates/elephant-api/plugin/elephant/skills/elephant/reference.md";

fn main() -> ExitCode {
    let matches = match cli().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            // --help y --version salen con 0; el uso incorrecto, con 2.
            let _ = error.print();
            return ExitCode::from(error.exit_code() as u8);
        }
    };
    let json = matches.get_flag("json");
    let Some((name, sub)) = matches.subcommand() else {
        return ExitCode::from(2);
    };
    let result = match name {
        "doctor" => doctor(json),
        "schema" => schema_command(sub),
        "guide" => print_text(&plugin::guide()),
        "mcp" => mcp(),
        "__job" => hidden_job(sub),
        "__write-plugin" => hidden_write_plugin(sub),
        "logs" if sub.subcommand_name() == Some("watch") => {
            logs_watch(sub.subcommand_matches("watch").unwrap_or(sub))
        }
        _ => return run_operation(name, sub, json),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&error, json),
    }
}

/// `elephant <recurso> <verbo>`, generado del catálogo, más las órdenes que
/// solo existen en la CLI.
fn cli() -> Command {
    let mut root = Command::new("elephant")
        .bin_name("elephant")
        .version(VERSION)
        .about(tr_str!(
            "Elephant Stack para agentes y para la terminal: logs, servicios, PHP y proyectos",
            "Elephant Stack for agents and the terminal: logs, services, PHP and projects"
        ))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help(tr_str!(
                    "Imprime JSON: exactamente el structuredContent de elephant mcp",
                    "Print JSON: exactly the structuredContent of elephant mcp"
                )),
        );

    let mut groups: Vec<(&'static str, Command)> = Vec::new();
    for op in catalog() {
        let leaf = (op.augment)(Command::new(op.cli[op.cli.len() - 1]).about((op.about)()));
        if op.cli.len() == 1 {
            root = root.subcommand(leaf);
            continue;
        }
        let group = op.cli[0];
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, command)) => *command = command.clone().subcommand(leaf),
            None => groups.push((group, Command::new(group).about(group_about(group)).subcommand_required(true).subcommand(leaf))),
        }
    }
    for (name, mut command) in groups {
        if name == "logs" {
            command = command.subcommand(watch_command());
        }
        root = root.subcommand(command);
    }

    root.subcommand(
        Command::new("doctor").about(tr_str!(
            "Estado del enlace, versión de la API, modo y número de operaciones",
            "Link status, API version, mode and number of operations"
        )),
    )
    .subcommand(
        Command::new("schema")
            .about(tr_str!(
                "Vuelca el catálogo (argumentos y salidas como JSON Schema)",
                "Dump the catalog (arguments and outputs as JSON Schema)"
            ))
            .arg(
                Arg::new("check")
                    .long("check")
                    .action(ArgAction::SetTrue)
                    .help(tr_str!("Compara con el archivo del repositorio", "Compare with the repository file")),
            )
            .arg(
                Arg::new("markdown")
                    .long("markdown")
                    .action(ArgAction::SetTrue)
                    .help(tr_str!("La referencia en Markdown de la skill", "The skill’s Markdown reference")),
            )
            .arg(
                Arg::new("file")
                    .long("file")
                    .value_name(tr_str!("RUTA", "PATH"))
                    .help(tr_str!("El archivo con el que comparar", "The file to compare with")),
            ),
    )
    .subcommand(Command::new("guide").about(tr_str!(
        "Imprime la guía para AGENTS.md o las reglas de otros agentes",
        "Print the guide for AGENTS.md or other agents’ rules"
    )))
    .subcommand(Command::new("mcp").about(tr_str!(
        "Sirve el catálogo como servidor MCP por stdio",
        "Serve the catalog as an MCP server over stdio"
    )))
    .subcommand(Command::new("__job").hide(true).arg(Arg::new("id").required(true)))
    .subcommand(Command::new("__write-plugin").hide(true).arg(Arg::new("dir").required(true)))
}

fn group_about(group: &str) -> &'static str {
    match group {
        "component" => tr_str!(
            "Componentes: mysql, postgresql, redis, mailpit, composer…",
            "Components: mysql, postgresql, redis, mailpit, composer…"
        ),
        "php" => tr_str!("Versiones de PHP: la global y la de cada proyecto", "PHP versions: the global one and each project’s"),
        "project" => tr_str!("Proyectos locales servidos por Herd", "Local projects served by Herd"),
        "logs" => tr_str!("Logs de Laravel, nginx y PHP-FPM (datos no fiables)", "Laravel, nginx and PHP-FPM logs (untrusted data)"),
        "job" => tr_str!("Tareas largas", "Jobs"),
        _ => "",
    }
}

fn watch_command() -> Command {
    Command::new("watch")
        .about(tr_str!(
            "Una línea por cada entrada nueva, para Monitor; termina al cumplirse --timeout",
            "One line per new entry, for Monitor. Ends when --timeout is up"
        ))
        .arg(Arg::new("errors").long("errors").action(ArgAction::SetTrue).help(tr_str!("Solo errores", "Errors only")))
        .arg(
            Arg::new("project")
                .long("project")
                .value_name("P")
                .help(tr_str!("Id, nombre o host del proyecto", "Project ID, name or host")),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .value_name("S")
                .required(true)
                .value_parser(clap::value_parser!(u64))
                .help(tr_str!("Segundos (como mucho 1800)", "Seconds (at most 1800)")),
        )
}

fn find_op(name: &str, sub: &ArgMatches) -> Option<(&'static OpMeta, ArgMatches)> {
    let verb = sub.subcommand();
    catalog().iter().find_map(|op| match (op.cli, verb) {
        ([group], _) if *group == name => Some((op, sub.clone())),
        ([group, leaf], Some((verb, matches))) if *group == name && *leaf == verb => Some((op, matches.clone())),
        _ => None,
    })
}

fn caller() -> elephant_api::Caller {
    let env = |key: &str| std::env::var_os(key).map(|v| v.to_string_lossy().into_owned());
    elephant_api::detect_caller(&env, std::io::stdin().is_terminal())
}

fn run_operation(name: &str, sub: &ArgMatches, json: bool) -> ExitCode {
    let Some((op, matches)) = find_op(name, sub) else {
        return fail(&ApiError::usage(tr!("Orden desconocida: {name}", "Unknown command: {name}")), json);
    };
    let args = match (op.from_matches)(&matches) {
        Ok(args) => args,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(2);
        }
    };
    let runtime = match SystemRuntime::new(caller()) {
        Ok(runtime) => runtime,
        Err(error) => return fail(&error, json),
    };
    let runtime = match std::env::current_exe() {
        Ok(exe) => runtime.with_job_exe(exe),
        Err(_) => runtime,
    };
    let confirmer = DialogConfirmer::default();
    // El progreso de las tareas largas va a stderr, para no mezclarlo con la salida.
    let mut progress = |line: &str| eprintln!("{line}");
    match elephant_api::invoke_with(&runtime, op.id, args, &confirmer, &mut progress) {
        Ok(reply) => {
            let out = if json { reply.structured.to_string() } else { reply.text };
            match print_text(&out) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => fail(&error, false),
            }
        }
        Err(error) if error.code == ErrorCode::Running => {
            // El id en stdout para poder usarlo en un script; la explicación, en stderr.
            let id = error.job_id.clone().unwrap_or_default();
            let line = if json { serde_json::json!({"job_id": id, "state": "running"}).to_string() } else { id };
            let _ = print_text(&line);
            fail(&error, json)
        }
        Err(error) => fail(&error, json),
    }
}

fn print_text(text: &str) -> Result<(), ApiError> {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{text}")
        .and_then(|()| stdout.flush())
        .map_err(|error| ApiError::new(ErrorCode::Failed, error.to_string()))
}

fn fail(error: &ApiError, json: bool) -> ExitCode {
    if json {
        eprintln!("{}", serde_json::json!({ "error": error }));
    } else {
        eprintln!("{}", error.message);
    }
    ExitCode::from(error.code.exit_code())
}

// ── Órdenes solo de la CLI ───────────────────────────────────────────────────

fn link_path() -> Option<PathBuf> {
    paths::data_dir().ok().map(|dir| dir.join("bin/elephant"))
}

fn plugin_dir() -> Option<PathBuf> {
    paths::data_dir().ok().map(|dir| dir.join("share/claude-plugin"))
}

fn doctor(json: bool) -> Result<(), ApiError> {
    let link = link_path();
    let target = link.as_ref().and_then(|l| std::fs::read_link(l).ok());
    let linked = link.as_ref().is_some_and(|l| l.exists());
    let plugin = plugin_dir();
    let plugin_version = plugin
        .as_ref()
        .and_then(|dir| std::fs::read_to_string(dir.join(plugin::VERSION_FILE)).ok())
        .map(|v| v.trim().to_string());
    let mode = settings::load().mcp.mode;
    let report = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "api": elephant_api::API_VERSION,
        "mode": mode,
        "operations": catalog().len(),
        "mcp": cfg!(feature = "mcp"),
        "linked": linked,
        "link": link,
        "link_target": target,
        "exe": std::env::current_exe().ok(),
        "plugin_dir": plugin,
        "plugin_version": plugin_version,
    });
    if json {
        return print_text(&report.to_string());
    }
    print_text(&doctor_text(mode, link.as_deref(), linked, target.as_deref(), plugin_version.as_deref()))
}

/// La salida de texto de `elephant doctor`, en el idioma de la persona (la
/// de `--json` no cambia).
fn doctor_text(
    mode: settings::McpMode,
    link: Option<&Path>,
    linked: bool,
    target: Option<&Path>,
    plugin_version: Option<&str>,
) -> String {
    let mode = match mode {
        settings::McpMode::ReadOnly => tr_str!("solo lectura", "read-only"),
        settings::McpMode::Confirm => tr_str!("confirmar", "confirm"),
        settings::McpMode::Trusted => tr_str!("de confianza", "trusted"),
    };
    let version = plugin::version_line();
    let count = catalog().len();
    let mut lines = vec![
        if count == 1 {
            tr!("{version} · modo {mode} · 1 operación", "{version} · {mode} mode · 1 operation")
        } else {
            tr!("{version} · modo {mode} · {count} operaciones", "{version} · {mode} mode · {count} operations")
        },
        match (link, linked) {
            (Some(link), true) => {
                let link = link.display();
                let target = target.map(|t| format!(" → {}", t.display())).unwrap_or_default();
                tr!("Enlace: {link}{target}", "Link: {link}{target}")
            }
            (Some(link), false) => {
                let link = link.display();
                tr!(
                    "Enlace: falta {link} (actívalo en Elephant Stack > Ajustes > Agentes)",
                    "Link: {link} not found (turn it on in Elephant Stack > Settings > Agents)"
                )
            }
            (None, _) => tr!("Enlace: HOME no está definido", "Link: HOME is not set"),
        },
        match plugin_version {
            Some(version) => tr!("Plugin de Claude Code: instalado ({version})", "Claude Code plugin: installed ({version})"),
            None => tr!(
                "Plugin de Claude Code: sin instalar (Ajustes > Agentes > Conectar)",
                "Claude Code plugin: not installed (Settings > Agents > Connect)"
            ),
        },
    ];
    lines.push(if cfg!(feature = "mcp") {
        tr!("elephant mcp: disponible", "elephant mcp: available")
    } else {
        tr!("elephant mcp: no incluido", "elephant mcp: not included")
    });
    lines.join("\n")
}

fn schema_command(sub: &ArgMatches) -> Result<(), ApiError> {
    let markdown = sub.get_flag("markdown");
    let current = if markdown { schema::markdown() } else { schema::document_text() };
    if !sub.get_flag("check") {
        return print_text(current.trim_end_matches('\n'));
    }
    let file = sub
        .get_one::<String>("file")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(if markdown { DOCS_REFERENCE } else { DOCS_SCHEMA }));
    let committed = std::fs::read_to_string(&file)
        .map_err(|error| {
            let file = file.display();
            ApiError::new(ErrorCode::Failed, tr!("No se pudo leer {file}: {error}", "Couldn’t read {file}: {error}"))
        })?;
    let file = file.display();
    if committed == current {
        print_text(&tr!("{file} está al día.", "{file} is up to date."))
    } else {
        let regenerate = if markdown { "elephant schema --markdown" } else { "elephant schema" };
        Err(ApiError::new(
            ErrorCode::Failed,
            tr!(
                "{file} no coincide con el catálogo. Regenera con `{regenerate} > {file}` y, si el cambio es incompatible, sube la versión de la API.",
                "{file} doesn’t match the catalog. Regenerate it with `{regenerate} > {file}` and, if the change is incompatible, bump the API version."
            ),
        ))
    }
}

#[cfg(feature = "mcp")]
fn mcp() -> Result<(), ApiError> {
    elephant_mcp::serve().map_err(|error| ApiError::new(ErrorCode::Failed, error))
}

#[cfg(not(feature = "mcp"))]
fn mcp() -> Result<(), ApiError> {
    Err(ApiError::new(
        ErrorCode::Unavailable,
        tr!("Esta compilación de elephant no incluye elephant mcp.", "This build of elephant doesn’t include elephant mcp."),
    ))
}

fn logs_watch(sub: &ArgMatches) -> Result<(), ApiError> {
    let timeout = sub.get_one::<u64>("timeout").copied().unwrap_or(0);
    let timeout = watch::check_timeout(timeout).map_err(ApiError::usage)?;
    let project = sub.get_one::<String>("project").cloned();
    let errors = sub.get_flag("errors");
    let home = paths::home().map_err(ApiError::from_core)?;
    let caller = caller();
    // Una lectura de logs: se audita sin su contenido.
    audit::record(&AuditEntry {
        at: elephant_core::now_rfc3339(),
        session: caller.session().to_string(),
        client: Some(caller.client().to_string()),
        tool: "logs_watch".into(),
        args: serde_json::json!({"project": project, "errors": errors, "timeout": timeout.as_secs()}),
        confirmed_by: None,
        ok: true,
        ms: 0,
        error: None,
    });
    let mut stdout = std::io::stdout().lock();
    watch::watch(&home, project.as_deref(), errors, timeout, &mut stdout).map_err(|error| ApiError::new(ErrorCode::Failed, error))
}

fn hidden_job(sub: &ArgMatches) -> Result<(), ApiError> {
    let id = sub.get_one::<String>("id").map(String::as_str).unwrap_or_default();
    elephant_api::run_job(id).map_err(|error| ApiError::new(ErrorCode::Failed, error))
}

fn hidden_write_plugin(sub: &ArgMatches) -> Result<(), ApiError> {
    let dir = sub.get_one::<String>("dir").map(Path::new).ok_or_else(|| ApiError::usage(tr!("Falta la carpeta.", "Missing folder.")))?;
    let written = plugin::write_plugin(dir).map_err(|error| ApiError::new(ErrorCode::Failed, error))?;
    let (path, n) = (dir.display(), written.len());
    print_text(&tr!("Plugin escrito en {path} ({n} archivos)", "Plugin written to {path} ({n} files)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_the_api() {
        assert_eq!(format!("elephant {VERSION}"), plugin::version_line());
    }

    #[test]
    fn doctor_text_is_translated() {
        use elephant_core::i18n::{Locale, with_locale};
        let link = Path::new("/h/.elephant-stack/bin/elephant");
        let text = |locale| {
            with_locale(locale, || doctor_text(settings::McpMode::Confirm, Some(link), false, None, None))
        };
        let es = text(Locale::Es);
        assert!(es.contains("· modo confirmar ·"), "{es}");
        assert!(es.contains("Enlace: falta /h/.elephant-stack/bin/elephant"), "{es}");
        let en = text(Locale::En);
        assert!(en.contains("· confirm mode ·"), "{en}");
        assert!(en.contains("Link: /h/.elephant-stack/bin/elephant not found"), "{en}");
        assert!(en.contains("Claude Code plugin: not installed"), "{en}");
    }

    #[test]
    fn the_command_tree_is_valid() {
        cli().debug_assert();
    }

    #[test]
    fn every_operation_is_reachable_by_its_cli_path() {
        for op in catalog() {
            let mut argv = vec!["elephant".to_string()];
            argv.extend(op.cli.iter().map(|s| s.to_string()));
            argv.extend(sample_args(op.id).iter().map(|s| s.to_string()));
            let matches = cli().try_get_matches_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            let (name, sub) = matches.subcommand().unwrap();
            let (found, _) = find_op(name, sub).unwrap();
            assert_eq!(found.id, op.id);
        }
    }

    /// Ida y vuelta clap → serde → clap sin cambios (05 §8).
    #[test]
    fn clap_and_serde_round_trip() {
        for op in catalog() {
            let mut argv = vec!["elephant".to_string()];
            argv.extend(op.cli.iter().map(|s| s.to_string()));
            argv.extend(sample_args(op.id).iter().map(|s| s.to_string()));
            let matches = cli().try_get_matches_from(&argv).unwrap();
            let (name, sub) = matches.subcommand().unwrap();
            let (op, leaf) = find_op(name, sub).unwrap();
            let first = (op.from_matches)(&leaf).unwrap();
            // serde → argumentos de la CLI → clap otra vez.
            let mut again = vec!["elephant".to_string()];
            again.extend(op.cli.iter().map(|s| s.to_string()));
            again.extend(to_argv(op, &first));
            let matches = cli().try_get_matches_from(&again).unwrap_or_else(|e| panic!("{again:?}: {e}"));
            let (name, sub) = matches.subcommand().unwrap();
            let (_, leaf) = find_op(name, sub).unwrap();
            assert_eq!((op.from_matches)(&leaf).unwrap(), first, "{}", op.id);
        }
    }

    fn sample_args(id: &str) -> Vec<&'static str> {
        match id {
            "elephant_status" => vec!["--brief"],
            "components_list" => vec!["--kind", "service", "--installed"],
            "php_versions" | "project_needs" | "project_start_services" => vec!["--dir", "/tmp/x"],
            "logs_sources" => vec!["--project", "shop"],
            "logs_tail" => vec!["laravel_app:shop", "-n", "20", "--level", "error", "--contains", "SQL"],
            "logs_search" => vec!["timeout", "--status", "5xx", "--since", "2h", "--limit", "10", "--cursor", "abc"],
            "logs_errors_summary" => vec!["--project", "shop", "--since", "30m"],
            "logs_entry" => vec!["abc:12"],
            "job_status" => vec!["job-12345678", "--wait", "5"],
            "service_control" => vec!["autostart-on", "postgresql@18"],
            "php_set_global" => vec!["8.4"],
            "php_set_project" => vec!["8.3", "--dir", "/tmp/x"],
            "component_install" => vec!["redis", "--detach"],
            "project_create" => vec!["shop", "--template", "blank", "--php", "8.4", "--detach"],
            _ => vec![],
        }
    }

    /// Reconstruye la línea de órdenes a partir del JSON de serde, usando la
    /// definición de clap (posicionales en orden y opciones largas).
    fn to_argv(op: &OpMeta, value: &serde_json::Value) -> Vec<String> {
        let command = (op.augment)(Command::new("x"));
        let mut argv = Vec::new();
        for arg in command.get_arguments() {
            let id = arg.get_id().as_str();
            let Some(item) = value.get(id).filter(|v| !v.is_null()) else { continue };
            let text = match item {
                serde_json::Value::String(s) => s.replace('_', "-").to_string(),
                other => other.to_string(),
            };
            // Los valores libres se pasan tal cual; los enumerados, en kebab-case.
            let text = if arg.get_possible_values().is_empty() { item.as_str().map(str::to_string).unwrap_or(text) } else { text };
            if arg.is_positional() {
                argv.push(text);
            } else if !arg.get_action().takes_values() {
                if item.as_bool() == Some(true) {
                    argv.push(format!("--{}", arg.get_long().unwrap_or(id)));
                }
            } else {
                argv.push(format!("--{}", arg.get_long().unwrap_or(id)));
                argv.push(text);
            }
        }
        argv
    }
}
