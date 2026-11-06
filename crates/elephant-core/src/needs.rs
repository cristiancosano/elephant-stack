//! «Arrancar lo que necesita» un proyecto, a partir de su `.env` (04 §4.10).
//!
//! Los servicios pueden ir apagados porque el proyecto dice lo que usa: se lee
//! su `.env` (nunca se escribe) y se arranca con `brew services run`, sin
//! registrar el arranque al iniciar sesión, lo que falte.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::brew::{self, Operation, Service};
use crate::{lock, stamp};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Needs {
    /// Ids de servicios de Homebrew (`mysql`, `redis`…).
    pub services: Vec<String>,
    /// Por qué hace falta cada uno, o por qué no se ha podido saber.
    pub reasons: Vec<String>,
}

/// Un `.env` normal ocupa unos pocos KB; esto evita leer un archivo enorme.
const MAX_ENV_BYTES: u64 = 1024 * 1024;

/// Hosts que cuentan como «este Mac». Vacío equivale a no definido: Laravel
/// usa entonces `127.0.0.1`.
const LOCAL_HOSTS: &[&str] = &["", "127.0.0.1", "localhost"];

/// Variables que eligen un driver de Redis. `CACHE_DRIVER` y `BROADCAST_DRIVER`
/// son los nombres anteriores a Laravel 11.
const REDIS_DRIVERS: &[&str] = &[
    "CACHE_STORE",
    "CACHE_DRIVER",
    "QUEUE_CONNECTION",
    "SESSION_DRIVER",
    "BROADCAST_CONNECTION",
    "BROADCAST_DRIVER",
];

/// Solo lee el `.env`; nunca lo escribe. Si no se puede leer, devuelve
/// `Needs` vacío con el motivo.
pub fn project_needs(dir: &Path) -> Needs {
    read_needs(dir).unwrap_or_else(|reason| Needs {
        services: Vec::new(),
        reasons: vec![reason],
    })
}

/// Arranca (sin arranque automático) lo que falte y devuelve lo arrancado, con
/// el nombre de Homebrew (`postgresql@18`). Vacío si no hacía falta nada.
///
/// Es un error que no haya `.env` legible, que falte algún servicio por
/// instalar o que brew falle; en esos casos se arranca igualmente lo que se
/// pueda y el mensaje dice qué se arrancó.
pub fn start_project_services(dir: &Path) -> Result<Vec<String>, String> {
    let needs = read_needs(dir)?;
    if needs.services.is_empty() {
        return Ok(Vec::new());
    }
    let lock = lock::brew()?;
    let plan = plan_start(&needs.services, &brew::list_services()?);
    let outcome = execute(&plan, &mut |name| {
        brew::apply(&lock, name, Operation::TurnOn).map(drop)
    });
    if !outcome.started.is_empty() {
        stamp::touch("services");
    }
    outcome.into_result()
}

// ---------------------------------------------------------------------------
// Lectura del `.env`
// ---------------------------------------------------------------------------

fn read_needs(dir: &Path) -> Result<Needs, String> {
    Ok(needs_from_env(&parse_env(&read_env(dir)?)))
}

fn read_env(dir: &Path) -> Result<String, String> {
    let path = dir.join(".env");
    let file = fs::File::open(&path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => format!("No hay .env en {}.", dir.display()),
        _ => format!("No se ha podido leer {}: {error}", path.display()),
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_ENV_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("No se ha podido leer {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_ENV_BYTES {
        return Err(format!("{} es demasiado grande para ser un .env.", path.display()));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Lee el formato de phpdotenv lo justo para estas variables: comentarios,
/// `export`, comillas simples y dobles (también de varias líneas), comentarios
/// al final de un valor sin comillas e interpolación `${VAR}`.
///
/// Si una clave se repite gana la primera, como en Laravel, que carga el
/// `.env` con un repositorio inmutable.
fn parse_env(text: &str) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = HashMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !is_key(key) {
            continue;
        }
        let raw = raw.trim_start();
        let value = match raw.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let mut body = raw[1..].to_string();
                // Si la comilla no se cierra en esta línea, el valor sigue.
                while closing_quote(&body, quote).is_none() {
                    match lines.next() {
                        Some(next) => {
                            body.push('\n');
                            body.push_str(next);
                        }
                        None => break,
                    }
                }
                let end = closing_quote(&body, quote).unwrap_or(body.len());
                let inner = &body[..end];
                if quote == '"' {
                    interpolate(&unescape(inner), &env)
                } else {
                    inner.to_string()
                }
            }
            _ => interpolate(strip_inline_comment(raw).trim_end(), &env),
        };
        env.entry(key.to_string()).or_insert(value);
    }
    env
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Posición de la comilla que cierra el valor. Con comillas dobles, `\"` no
/// cierra.
fn closing_quote(body: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        match c {
            '\\' if quote == '"' && !escaped => escaped = true,
            c if c == quote && !escaped => return Some(i),
            _ => escaped = false,
        }
    }
    None
}

fn unescape(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other @ ('"' | '\\' | '$')) => out.push(other),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `VALOR # comentario` → `VALOR`. Un `#` pegado al valor es parte de él.
fn strip_inline_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    (1..bytes.len())
        .find(|&i| bytes[i] == b'#' && bytes[i - 1].is_ascii_whitespace())
        .map_or(raw, |i| &raw[..i])
}

/// Sustituye `${VAR}` por su valor ya leído, o por nada si no existe.
fn interpolate(value: &str, env: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start + 2..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let name = &rest[start + 2..start + 2 + len];
        if let Some(found) = env.get(name) {
            out.push_str(found);
        }
        rest = &rest[start + 3 + len..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// De variables a servicios
// ---------------------------------------------------------------------------

fn get<'a>(env: &'a HashMap<String, String>, key: &str) -> &'a str {
    env.get(key).map_or("", |value| value.trim())
}

fn is_local(env: &HashMap<String, String>, key: &str) -> bool {
    let host = get(env, key).to_ascii_lowercase();
    LOCAL_HOSTS.contains(&host.as_str())
}

fn needs_from_env(env: &HashMap<String, String>) -> Needs {
    let mut needs = Needs::default();
    let mut need = |service: &str, reason: String| {
        needs.services.push(service.to_string());
        needs.reasons.push(reason);
    };
    let mut reasons = Vec::new();

    let connection = get(env, "DB_CONNECTION");
    let database = match connection.to_ascii_lowercase().as_str() {
        "mysql" | "mariadb" => Some("mysql"),
        "pgsql" => Some("postgresql"),
        _ => None,
    };
    if let Some(service) = database {
        if is_local(env, "DB_HOST") {
            need(service, format!("{service}: DB_CONNECTION={connection}"));
        } else {
            reasons.push(format!(
                "Sin {service}: la base de datos está en {} (DB_HOST).",
                get(env, "DB_HOST")
            ));
        }
    }

    // `REDIS_HOST` por sí solo no cuenta: el skeleton de Laravel lo trae siempre.
    let drivers: Vec<String> = REDIS_DRIVERS
        .iter()
        .filter(|key| get(env, key).eq_ignore_ascii_case("redis"))
        .map(|key| format!("{key}=redis"))
        .collect();
    if !drivers.is_empty() {
        if is_local(env, "REDIS_HOST") {
            need("redis", format!("redis: {}", drivers.join(", ")));
        } else {
            reasons.push(format!(
                "Sin redis: Redis está en {} (REDIS_HOST).",
                get(env, "REDIS_HOST")
            ));
        }
    }

    let mailer_key = if env.contains_key("MAIL_MAILER") { "MAIL_MAILER" } else { "MAIL_DRIVER" };
    if get(env, mailer_key).eq_ignore_ascii_case("smtp") && get(env, "MAIL_PORT") == "1025" {
        if is_local(env, "MAIL_HOST") {
            need("mailpit", format!("mailpit: {mailer_key}=smtp y MAIL_PORT=1025"));
        } else {
            reasons.push(format!(
                "Sin mailpit: el SMTP está en {} (MAIL_HOST).",
                get(env, "MAIL_HOST")
            ));
        }
    }

    needs.reasons.extend(reasons);
    needs
}

// ---------------------------------------------------------------------------
// De servicios a `brew services`
// ---------------------------------------------------------------------------

/// Fórmulas que cubren cada necesidad, de la preferida a la menos.
fn formulas(need: &str) -> &'static [&'static str] {
    match need {
        "mysql" => &["mysql", "mariadb"],
        "postgresql" => &["postgresql"],
        "redis" => &["redis"],
        "mailpit" => &["mailpit"],
        _ => &[],
    }
}

fn display_name(need: &str) -> &str {
    match need {
        "mysql" => "MySQL",
        "postgresql" => "PostgreSQL",
        "redis" => "Redis",
        "mailpit" => "Mailpit",
        other => other,
    }
}

/// `mysql` → `Some(None)`, `postgresql@18` → `Some(Some([18]))`; `None` si el
/// servicio no es de esa fórmula.
fn formula_version(formula: &str, name: &str) -> Option<Option<Vec<u32>>> {
    let rest = name.strip_prefix(formula)?;
    if rest.is_empty() {
        return Some(None);
    }
    let version = rest.strip_prefix('@')?;
    Some(Some(
        version
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect(),
    ))
}

#[derive(Debug, Default, PartialEq, Eq)]
struct StartPlan {
    /// Nombres de brew que hay que arrancar.
    start: Vec<String>,
    /// Necesidades que no tienen ningún servicio instalado.
    missing: Vec<String>,
}

/// Una necesidad queda cubierta si alguno de sus servicios ya está en marcha.
/// Si no, se arranca uno: primero el que arranca al iniciar sesión (es el que
/// el usuario ha elegido), después por orden de fórmula y la versión más
/// reciente.
fn plan_start(needs: &[String], services: &[Service]) -> StartPlan {
    let mut plan = StartPlan::default();
    for need in needs {
        let mut candidates: Vec<(usize, Option<Vec<u32>>, &Service)> = services
            .iter()
            .filter_map(|service| {
                formulas(need).iter().enumerate().find_map(|(rank, formula)| {
                    formula_version(formula, &service.name).map(|version| (rank, version, service))
                })
            })
            .collect();
        if candidates.is_empty() {
            plan.missing.push(need.clone());
            continue;
        }
        if candidates.iter().any(|(_, _, service)| service.running) {
            continue;
        }
        candidates.sort_by(|a, b| {
            b.2.registered
                .cmp(&a.2.registered)
                .then(a.0.cmp(&b.0))
                .then_with(|| newest_first(&a.1, &b.1))
        });
        let name = candidates[0].2.name.clone();
        if !plan.start.contains(&name) {
            plan.start.push(name);
        }
    }
    plan
}

/// Sin versión (`mysql`) es la fórmula principal y va primero.
fn newest_first(a: &Option<Vec<u32>>, b: &Option<Vec<u32>>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => b.cmp(a),
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Outcome {
    started: Vec<String>,
    problems: Vec<String>,
}

/// Arranca todo lo que pueda aunque algo falle.
fn execute(plan: &StartPlan, start: &mut dyn FnMut(&str) -> Result<(), String>) -> Outcome {
    let mut outcome = Outcome {
        started: Vec::new(),
        problems: plan
            .missing
            .iter()
            .map(|need| format!("{} no está instalado", display_name(need)))
            .collect(),
    };
    for name in &plan.start {
        match start(name) {
            Ok(()) => outcome.started.push(name.clone()),
            Err(error) => outcome.problems.push(format!("{name}: {error}")),
        }
    }
    outcome
}

impl Outcome {
    fn into_result(self) -> Result<Vec<String>, String> {
        if self.problems.is_empty() {
            return Ok(self.started);
        }
        let mut message = format!("{}.", self.problems.join(". "));
        if !self.started.is_empty() {
            message.push_str(&format!(" Se ha arrancado {}.", self.started.join(", ")));
        }
        Err(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    /// `.env` del skeleton de Laravel 12, tal cual lo deja `laravel new`.
    const SKELETON: &str = r#"APP_NAME=Laravel
APP_ENV=local
APP_KEY=
APP_DEBUG=true
APP_URL=http://localhost

LOG_CHANNEL=stack

DB_CONNECTION=sqlite
# DB_HOST=127.0.0.1
# DB_PORT=3306

SESSION_DRIVER=database
BROADCAST_CONNECTION=log
QUEUE_CONNECTION=database
CACHE_STORE=database

REDIS_CLIENT=phpredis
REDIS_HOST=127.0.0.1
REDIS_PASSWORD=null
REDIS_PORT=6379

MAIL_MAILER=log
MAIL_HOST=127.0.0.1
MAIL_PORT=2525
"#;

    /// Un proyecto con MySQL, Redis y Mailpit en este Mac.
    const FULL: &str = r#"DB_CONNECTION=mysql
DB_HOST=127.0.0.1
DB_DATABASE=shop
DB_PASSWORD="se#cre\"to"

CACHE_STORE=redis
QUEUE_CONNECTION=redis
SESSION_DRIVER=file
REDIS_HOST=localhost

MAIL_MAILER=smtp
MAIL_HOST=127.0.0.1
MAIL_PORT=1025
"#;

    fn needs_of(text: &str) -> Needs {
        needs_from_env(&parse_env(text))
    }

    fn project(env: Option<&str>) -> TempDir {
        let dir = TempDir::new("needs");
        if let Some(env) = env {
            fs::write(dir.path().join(".env"), env).unwrap();
        }
        dir
    }

    fn service(name: &str, running: bool, registered: bool) -> Service {
        Service {
            name: name.into(),
            status: if running { "started" } else { "none" }.into(),
            running,
            registered,
            pid: None,
            exit_code: None,
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // --- .env ---------------------------------------------------------------

    #[test]
    fn the_laravel_skeleton_needs_nothing() {
        // REDIS_HOST y MAIL_HOST vienen siempre: no bastan.
        assert_eq!(needs_of(SKELETON), Needs::default());
    }

    #[test]
    fn a_full_project_needs_its_three_services() {
        let needs = needs_of(FULL);
        assert_eq!(needs.services, ["mysql", "redis", "mailpit"]);
        assert_eq!(
            needs.reasons,
            [
                "mysql: DB_CONNECTION=mysql",
                "redis: CACHE_STORE=redis, QUEUE_CONNECTION=redis",
                "mailpit: MAIL_MAILER=smtp y MAIL_PORT=1025",
            ]
        );
    }

    #[test]
    fn database_drivers_map_to_formulas() {
        assert_eq!(needs_of("DB_CONNECTION=mariadb").services, ["mysql"]);
        assert_eq!(needs_of("DB_CONNECTION=pgsql\nDB_HOST=localhost").services, ["postgresql"]);
        assert_eq!(needs_of("DB_CONNECTION=MySQL").services, ["mysql"]);
        assert!(needs_of("DB_CONNECTION=sqlite").services.is_empty());
        assert!(needs_of("DB_CONNECTION=sqlsrv").services.is_empty());
        assert!(needs_of("").services.is_empty());
    }

    #[test]
    fn remote_hosts_are_not_started_and_say_why() {
        let needs = needs_of(
            "DB_CONNECTION=pgsql\nDB_HOST=db.internal\n\
             CACHE_STORE=redis\nREDIS_HOST=10.0.0.5\n\
             MAIL_MAILER=smtp\nMAIL_HOST=mailpit\nMAIL_PORT=1025\n",
        );
        assert!(needs.services.is_empty());
        assert_eq!(
            needs.reasons,
            [
                "Sin postgresql: la base de datos está en db.internal (DB_HOST).",
                "Sin redis: Redis está en 10.0.0.5 (REDIS_HOST).",
                "Sin mailpit: el SMTP está en mailpit (MAIL_HOST).",
            ]
        );
    }

    #[test]
    fn redis_counts_for_any_driver_but_not_for_the_host_alone() {
        for key in REDIS_DRIVERS {
            assert_eq!(needs_of(&format!("{key}=redis")).services, ["redis"], "{key}");
        }
        assert!(needs_of("REDIS_HOST=127.0.0.1\nREDIS_PORT=6379").services.is_empty());
    }

    #[test]
    fn mailpit_needs_smtp_and_port_1025() {
        assert_eq!(needs_of("MAIL_MAILER=smtp\nMAIL_PORT=1025").services, ["mailpit"]);
        assert_eq!(needs_of("MAIL_DRIVER=smtp\nMAIL_PORT=1025").services, ["mailpit"]);
        assert!(needs_of("MAIL_MAILER=smtp\nMAIL_PORT=2525").services.is_empty());
        assert!(needs_of("MAIL_MAILER=log\nMAIL_PORT=1025").services.is_empty());
        // MAIL_MAILER manda sobre el nombre antiguo.
        assert!(needs_of("MAIL_MAILER=log\nMAIL_DRIVER=smtp\nMAIL_PORT=1025").services.is_empty());
    }

    #[test]
    fn dotenv_syntax() {
        let env = parse_env(
            "# comentario\n\
             export DB_CONNECTION=pgsql\n\
             DB_HOST = 127.0.0.1   # al final\n\
             DB_DATABASE=a#b\n\
             DB_PASSWORD=\"x \\\"y\\\" # z\"\n\
             DB_USERNAME='${NO} # literal'\n\
             APP_HOST=localhost\n\
             MAIL_HOST=\"${APP_HOST}\"\n\
             REDIS_HOST=${APP_HOST}\n\
             MULTI=\"uno\n\
             dos\"\n\
             AFTER=ok\n\
             not a line\n\
             BAD KEY=1\n\
             DB_CONNECTION=mysql\n",
        );
        assert_eq!(env["DB_CONNECTION"], "pgsql", "gana la primera");
        assert_eq!(env["DB_HOST"], "127.0.0.1");
        assert_eq!(env["DB_DATABASE"], "a#b");
        assert_eq!(env["DB_PASSWORD"], "x \"y\" # z");
        assert_eq!(env["DB_USERNAME"], "${NO} # literal");
        assert_eq!(env["MAIL_HOST"], "localhost");
        assert_eq!(env["REDIS_HOST"], "localhost");
        assert_eq!(env["MULTI"], "uno\ndos");
        assert_eq!(env["AFTER"], "ok");
        assert!(!env.contains_key("BAD KEY"));
    }

    #[test]
    fn an_unclosed_quote_does_not_panic() {
        let env = parse_env("A=\"sin cerrar\nB=1");
        assert_eq!(env["A"], "sin cerrar\nB=1");
        assert_eq!(parse_env("A='")["A"], "");
        assert_eq!(parse_env("A=${SIN")["A"], "${SIN");
    }

    #[test]
    fn needs_come_from_the_project_env_file() {
        let dir = project(Some(FULL));
        let before = fs::read(dir.path().join(".env")).unwrap();
        assert_eq!(project_needs(dir.path()).services, ["mysql", "redis", "mailpit"]);
        assert_eq!(fs::read(dir.path().join(".env")).unwrap(), before, "no se escribe nunca");
    }

    #[test]
    fn without_env_needs_is_empty_with_the_reason() {
        let dir = project(None);
        let needs = project_needs(dir.path());
        assert!(needs.services.is_empty());
        assert_eq!(needs.reasons, [format!("No hay .env en {}.", dir.path().display())]);
        assert!(start_project_services(dir.path()).is_err());
    }

    #[test]
    fn an_unreadable_env_is_reported() {
        let dir = project(None);
        fs::create_dir(dir.path().join(".env")).unwrap();
        let needs = project_needs(dir.path());
        assert!(needs.services.is_empty());
        assert!(needs.reasons[0].starts_with("No se ha podido leer"), "{:?}", needs.reasons);
    }

    #[test]
    fn a_huge_env_is_not_read() {
        let dir = project(None);
        let file = fs::File::create(dir.path().join(".env")).unwrap();
        file.set_len(MAX_ENV_BYTES + 1).unwrap();
        assert!(project_needs(dir.path()).reasons[0].contains("demasiado grande"));
    }

    #[test]
    fn nothing_to_start_does_not_call_brew() {
        // Sin necesidades no se toma el cerrojo ni se llama a brew.
        let dir = project(Some(SKELETON));
        assert_eq!(start_project_services(dir.path()), Ok(Vec::new()));
    }

    // --- brew services ------------------------------------------------------

    #[test]
    fn only_what_is_stopped_is_started() {
        let services = [
            service("mysql", false, false),
            service("redis", true, false),
            service("mailpit", false, true),
        ];
        let plan = plan_start(&strings(&["mysql", "redis", "mailpit"]), &services);
        assert_eq!(plan.start, ["mysql", "mailpit"]);
        assert!(plan.missing.is_empty());
    }

    #[test]
    fn versioned_formulas_count() {
        let services = [service("postgresql@17", false, false), service("postgresql@18", false, false)];
        let plan = plan_start(&strings(&["postgresql"]), &services);
        assert_eq!(plan.start, ["postgresql@18"], "la más reciente");

        let services = [service("postgresql@17", false, true), service("postgresql@18", false, false)];
        let plan = plan_start(&strings(&["postgresql"]), &services);
        assert_eq!(plan.start, ["postgresql@17"], "la que arranca al iniciar sesión");

        let services = [service("postgresql@17", true, false), service("postgresql@18", false, false)];
        assert_eq!(plan_start(&strings(&["postgresql"]), &services), StartPlan::default());
    }

    #[test]
    fn mariadb_covers_mysql_and_similar_names_do_not() {
        let services = [service("mariadb", false, false), service("mysql-client", false, false)];
        assert_eq!(plan_start(&strings(&["mysql"]), &services).start, ["mariadb"]);

        let services = [service("mysql@8.4", false, false), service("mysql", false, false)];
        assert_eq!(plan_start(&strings(&["mysql"]), &services).start, ["mysql"]);

        let services = [service("mysql@8.0", false, false), service("mysql@8.4", false, false)];
        assert_eq!(plan_start(&strings(&["mysql"]), &services).start, ["mysql@8.4"]);

        let services = [service("redis-stack", false, false)];
        assert_eq!(plan_start(&strings(&["redis"]), &services).missing, ["redis"]);
    }

    #[test]
    fn everything_possible_is_started_even_if_something_fails() {
        let plan = StartPlan {
            start: strings(&["mysql", "redis"]),
            missing: strings(&["postgresql"]),
        };
        let mut calls = Vec::new();
        let outcome = execute(&plan, &mut |name| {
            calls.push(name.to_string());
            if name == "mysql" { Err("brew services run mysql falló: x".into()) } else { Ok(()) }
        });
        assert_eq!(calls, ["mysql", "redis"]);
        assert_eq!(
            outcome.into_result(),
            Err("PostgreSQL no está instalado. mysql: brew services run mysql falló: x. \
                 Se ha arrancado redis."
                .into())
        );
    }

    #[test]
    fn a_clean_run_returns_what_was_started() {
        let plan = StartPlan {
            start: strings(&["mysql", "mailpit"]),
            missing: Vec::new(),
        };
        let outcome = execute(&plan, &mut |_| Ok(()));
        assert_eq!(outcome.into_result(), Ok(strings(&["mysql", "mailpit"])));
        assert_eq!(execute(&StartPlan::default(), &mut |_| Ok(())).into_result(), Ok(Vec::new()));
    }
}
