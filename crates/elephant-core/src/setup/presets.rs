//! Los tres escenarios fijos y Personalizado (04 §4.1): de lo que elige la
//! interfaz a un [`Profile`], más las equivalencias entre fórmulas y las cifras
//! de RAM que enseñan las tarjetas.

use std::collections::BTreeMap;

use super::{
    Catalog, Database, DatabaseOption, Detection, PhpWant, Profile, ProfileInput, ScenarioCard,
    ScenarioId, ServiceWant,
};
use crate::brew::Service;
use crate::{components, tr, tr_str};

/// Origen de las versiones de PHP que instala Elephant (`elephant_php`).
pub(crate) const OWN_SOURCE: &str = "Elephant Stack";

/// Servicios que el asistente conoce y enseña en Personalizado.
pub(crate) const WIZARD_SERVICES: [&str; 4] = ["mysql", "postgresql@18", "redis", "mailpit"];

/// Familia de una fórmula: así `mariadb` cuenta como MySQL y `postgresql@17`
/// como PostgreSQL 18.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    Mysql,
    Postgres,
    Redis,
    Mailpit,
    Nginx,
    Dnsmasq,
    Other,
}

pub(crate) fn engine(formula: &str) -> Engine {
    let base = formula.split('@').next().unwrap_or(formula);
    match base {
        "mysql" | "mariadb" => Engine::Mysql,
        "postgresql" => Engine::Postgres,
        "redis" => Engine::Redis,
        "mailpit" => Engine::Mailpit,
        "nginx" => Engine::Nginx,
        "dnsmasq" => Engine::Dnsmasq,
        _ => Engine::Other,
    }
}

/// Estimación en reposo con el servicio encendido. Siempre se presenta con «≈».
pub(crate) fn idle_ram_estimate(formula: &str) -> u32 {
    match engine(formula) {
        Engine::Mysql => 400,
        Engine::Postgres => 20,
        Engine::Redis => 10,
        Engine::Mailpit => 25,
        Engine::Nginx => 10,
        Engine::Dnsmasq => 2,
        Engine::Other => 0,
    }
}

/// Puerto que el servicio necesita libre para arrancar.
pub(crate) fn port_of(formula: &str) -> Option<u16> {
    match engine(formula) {
        Engine::Mysql => Some(3306),
        Engine::Postgres => Some(5432),
        Engine::Redis => Some(6379),
        Engine::Mailpit => Some(1025),
        _ => None,
    }
}

/// Nombre para la interfaz: `postgresql@18` → «PostgreSQL 18».
pub(crate) fn label(formula: &str) -> String {
    let (base, version) = match formula.split_once('@') {
        Some((base, version)) => (base, Some(version)),
        None => (formula, None),
    };
    let name = match base {
        "mysql" => "MySQL",
        "mariadb" => "MariaDB",
        "postgresql" => "PostgreSQL",
        "redis" => "Redis",
        "mailpit" => "Mailpit",
        other => other,
    };
    match version {
        Some(version) => format!("{name} {version}"),
        None => name.to_string(),
    }
}

pub(crate) fn database_formula(database: Database) -> &'static str {
    match database {
        Database::Mysql => "mysql",
        Database::Postgres => "postgresql@18",
    }
}

/// `8.5.12` → `[8, 5, 12]`, para ordenar versiones numéricamente.
pub(crate) fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// `8.5.12` → `8.5`.
pub(crate) fn branch_of(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// `X.Y.Z` con números: lo único que se acepta como versión que viene de la red.
pub(crate) fn is_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 4 && part.bytes().all(|b| b.is_ascii_digit())
        })
}

fn is_branch(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 4 && part.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Versiones propias instaladas, de la más reciente a la más antigua.
pub(crate) fn own_versions(d: &Detection) -> Vec<String> {
    let mut versions: Vec<String> = d
        .php_versions
        .iter()
        .filter(|version| version.source == OWN_SOURCE)
        .map(|version| version.id.clone())
        .collect();
    versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
    versions.dedup();
    versions
}

/// Ramas del catálogo de la más reciente a la más antigua, sin repetir.
pub(crate) fn catalog_branches(c: &Catalog) -> Vec<String> {
    let mut releases: Vec<&str> = c
        .php
        .iter()
        .map(|release| release.version.as_str())
        .collect();
    releases.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
    let mut branches: Vec<String> = Vec::new();
    for version in releases {
        let branch = branch_of(version);
        if !branches.contains(&branch) {
            branches.push(branch);
        }
    }
    branches
}

fn installed_names(d: &Detection) -> Vec<&str> {
    let mut names: Vec<&str> = d.formulas.iter().map(String::as_str).collect();
    names.extend(d.services.iter().map(|service| service.name.as_str()));
    names
}

pub(crate) fn service<'a>(d: &'a Detection, name: &str) -> Option<&'a Service> {
    d.services.iter().find(|service| service.name == name)
}

/// La fórmula instalada que satisface `id`, si hay alguna (04 §4.1):
/// `mysql` vale con `mysql`, `mysql@*` o `mariadb`, y `postgresql@18` con
/// `postgresql` o cualquier `postgresql@*`. Se actúa siempre sobre la instalada.
pub(crate) fn installed_for(id: &str, d: &Detection) -> Option<String> {
    let names = installed_names(d);
    if names.contains(&id) {
        return Some(id.to_string());
    }
    let family = engine(id);
    if !matches!(family, Engine::Mysql | Engine::Postgres) {
        return None;
    }
    let mut candidates: Vec<&str> = names
        .into_iter()
        .filter(|name| engine(name) == family)
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    // Primero la que está en marcha, después la que arranca sola y después la
    // versión más alta. MariaDB queda detrás de cualquier MySQL.
    candidates.sort_by_key(|name| {
        let svc = service(d, name);
        (
            std::cmp::Reverse(svc.is_some_and(|s| s.running)),
            std::cmp::Reverse(svc.is_some_and(|s| s.registered)),
            name.starts_with("mariadb"),
            std::cmp::Reverse(version_key(name.split_once('@').map_or("", |(_, v)| v))),
        )
    });
    candidates.first().map(|name| (*name).to_string())
}

/// En marcha, después instalada y, si no hay ninguna, MySQL (lo que usan por
/// defecto Sail y Forge).
pub(crate) fn preselected_database(d: &Detection) -> Database {
    let state = |database: Database| {
        let installed = installed_for(database_formula(database), d);
        let running = installed
            .as_deref()
            .and_then(|name| service(d, name))
            .is_some_and(|svc| svc.running);
        (running, installed.is_some())
    };
    let (mysql_running, mysql_installed) = state(Database::Mysql);
    let (pg_running, pg_installed) = state(Database::Postgres);
    if mysql_running {
        Database::Mysql
    } else if pg_running || (pg_installed && !mysql_installed) {
        Database::Postgres
    } else {
        Database::Mysql
    }
}

fn base_profile(scenario: ScenarioId) -> Profile {
    Profile {
        scenario,
        php: PhpWant::AnyOwn,
        composer: true,
        shim: true,
        services: BTreeMap::new(),
        launch_at_login: true,
        default_php: None,
    }
}

pub(crate) fn resolve(input: &ProfileInput, d: &Detection, c: &Catalog) -> Result<Profile, String> {
    match input {
        ProfileInput::Laravel => {
            let mut profile = base_profile(ScenarioId::Laravel);
            profile
                .services
                .insert("mailpit".into(), ServiceWant::Installed);
            Ok(profile)
        }
        ProfileInput::LaravelServices { database } => {
            let mut profile = base_profile(ScenarioId::LaravelServices);
            profile
                .services
                .insert("redis".into(), ServiceWant::Installed);
            profile
                .services
                .insert("mailpit".into(), ServiceWant::Autostart);
            profile
                .services
                .insert(database_formula(*database).into(), ServiceWant::Installed);
            Ok(profile)
        }
        ProfileInput::Terminal => {
            let mut profile = base_profile(ScenarioId::Terminal);
            let branches: Vec<String> = catalog_branches(c).into_iter().take(2).collect();
            // Sin catálogo no hay ramas que pedir: vale cualquier versión propia
            // y, si no hay ninguna, el plan lo explica como conflicto.
            profile.php = if branches.is_empty() {
                PhpWant::AnyOwn
            } else {
                PhpWant::Branches(branches)
            };
            profile.launch_at_login = false;
            Ok(profile)
        }
        ProfileInput::Custom { profile } => validate_custom(profile, d, c),
    }
}

fn validate_custom(profile: &Profile, d: &Detection, c: &Catalog) -> Result<Profile, String> {
    for id in profile.services.keys() {
        if !components::is_installable_service(id) {
            return Err(tr!(
                "{id} no es un servicio que Elephant sepa instalar.",
                "{id} isn’t a service Elephant can install."
            ));
        }
    }
    let own = own_versions(d);
    let available = catalog_branches(c);
    if let PhpWant::Branches(branches) = &profile.php {
        for branch in branches {
            let known = is_branch(branch)
                && (available.contains(branch) || own.iter().any(|v| branch_of(v) == *branch));
            if !known {
                return Err(tr!("PHP {branch} no está en el catálogo de Elephant.", "PHP {branch} isn’t in Elephant’s catalog."));
            }
        }
    }
    if let Some(version) = &profile.default_php {
        let detected = d
            .php_versions
            .iter()
            .any(|v| v.key == *version || v.id == *version);
        let downloadable = c.php.iter().any(|release| release.version == *version);
        if !detected && !downloadable {
            return Err(tr!(
                "PHP {version} no está instalado ni en el catálogo.",
                "PHP {version} isn’t installed or in the catalog."
            ));
        }
    }
    Ok(Profile {
        scenario: ScenarioId::Custom,
        ..profile.clone()
    })
}

/// Personalizado sin tocar nada: describe lo que ya hay, así que el plan sale
/// vacío (C10).
pub(crate) fn profile_from_system(d: &Detection) -> Profile {
    let own = own_versions(d);
    let mut branches: Vec<String> = Vec::new();
    for version in &own {
        let branch = branch_of(version);
        if !branches.contains(&branch) {
            branches.push(branch);
        }
    }
    let services = WIZARD_SERVICES
        .iter()
        .map(|id| {
            let want = match installed_for(id, d) {
                None => ServiceWant::Keep,
                Some(name) if service(d, &name).is_some_and(|svc| svc.registered) => {
                    ServiceWant::Autostart
                }
                Some(_) => ServiceWant::Installed,
            };
            ((*id).to_string(), want)
        })
        .collect();
    Profile {
        scenario: ScenarioId::Custom,
        php: if branches.is_empty() {
            PhpWant::None
        } else {
            PhpWant::Branches(branches)
        },
        composer: d.composer_phar,
        shim: d.shim == super::FileState::Same,
        services,
        launch_at_login: d.launch_at_login,
        default_php: d.global_php.clone(),
    }
}

/// Lo que se enseña en la tarjeta: nombre, frase y lista corta.
fn card_text(id: ScenarioId) -> (&'static str, &'static str) {
    match id {
        ScenarioId::Laravel => (
            "Laravel",
            tr_str!("Proyectos con SQLite. Nada en marcha.", "Projects with SQLite. Nothing running."),
        ),
        ScenarioId::LaravelServices => (
            tr_str!("Laravel con servicios", "Laravel with services"),
            tr_str!("MySQL o PostgreSQL, Redis y correo de pruebas.", "MySQL or PostgreSQL, Redis and test mail."),
        ),
        ScenarioId::Terminal => (
            tr_str!("Solo terminal", "Terminal only"),
            tr_str!(
                "PHP y Composer en la terminal. Herd sigue sirviendo tus sitios.",
                "PHP and Composer in the terminal. Herd keeps serving your sites."
            ),
        ),
        ScenarioId::Custom => (
            tr_str!("Personalizado", "Custom"),
            tr_str!("Elige cada pieza. Parte de lo que ya tienes.", "Pick each piece. Starts from what you already have."),
        ),
    }
}

fn components_of(profile: &Profile) -> Vec<String> {
    let mut list = Vec::new();
    match &profile.php {
        PhpWant::AnyOwn => list.push("PHP".to_string()),
        PhpWant::Branches(branches) => list.extend(branches.iter().map(|b| format!("PHP {b}"))),
        PhpWant::None => {}
    }
    if profile.composer {
        list.push("Composer".into());
    }
    if profile.shim {
        list.push(tr!("Selector php", "php selector"));
    }
    for (id, want) in &profile.services {
        match want {
            ServiceWant::Keep => {}
            ServiceWant::Installed => {
                let label = label(id);
                list.push(tr!("{label} (apagado)", "{label} (off)"))
            }
            ServiceWant::Autostart => {
                let label = label(id);
                list.push(tr!("{label} (al iniciar sesión)", "{label} (at login)"))
            }
        }
    }
    list
}

/// Tarjetas de la pantalla de elección. `measured` lleva la RAM real (MB) de
/// los servicios que ya están en marcha, por nombre.
pub(crate) fn scenarios(
    d: &Detection,
    c: &Catalog,
    measured: &BTreeMap<String, u32>,
) -> Vec<ScenarioCard> {
    let preselected = preselected_database(d);
    let ram_of = |formula: &str| -> u32 {
        let installed = installed_for(formula, d).unwrap_or_else(|| formula.to_string());
        measured
            .get(&installed)
            .copied()
            .unwrap_or_else(|| idle_ram_estimate(&installed))
    };
    let inputs = [
        (ScenarioId::Laravel, ProfileInput::Laravel),
        (
            ScenarioId::LaravelServices,
            ProfileInput::LaravelServices {
                database: preselected,
            },
        ),
        (ScenarioId::Terminal, ProfileInput::Terminal),
    ];
    let mut cards: Vec<ScenarioCard> = inputs
        .into_iter()
        .filter_map(|(id, input)| {
            let profile = resolve(&input, d, c).ok()?;
            Some(card(id, &profile, d, c, measured))
        })
        .collect();
    if let Some(services) = cards
        .iter_mut()
        .find(|card| card.id == ScenarioId::LaravelServices)
    {
        services.databases = [Database::Mysql, Database::Postgres]
            .into_iter()
            .map(|database| DatabaseOption {
                database,
                label: label(database_formula(database).split('@').next().unwrap_or("")),
                idle_ram_mb: ram_of(database_formula(database)),
                preselected: database == preselected,
            })
            .collect();
    }
    let system = profile_from_system(d);
    cards.push(card(ScenarioId::Custom, &system, d, c, measured));
    cards
}

fn card(
    id: ScenarioId,
    profile: &Profile,
    d: &Detection,
    c: &Catalog,
    measured: &BTreeMap<String, u32>,
) -> ScenarioCard {
    let plan = super::plan::plan(d, profile, c, measured);
    let (name, phrase) = card_text(id);
    ScenarioCard {
        id,
        name: name.into(),
        phrase: phrase.into(),
        recommended: id == ScenarioId::Laravel,
        components: components_of(profile),
        idle_ram_mb: plan.totals.idle_ram_mb,
        download_bytes: plan.totals.download_bytes,
        steps: u32::try_from(plan.steps.iter().filter(|step| !step.optional).count())
            .unwrap_or(u32::MAX),
        databases: Vec::new(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::php::VersionInfo;
    use crate::setup::{FileState, PhpRelease};

    pub(crate) fn svc(name: &str, running: bool, registered: bool) -> Service {
        Service {
            name: name.into(),
            status: if running { "started" } else { "none" }.into(),
            running,
            registered,
            pid: running.then_some(4242),
            exit_code: None,
        }
    }

    pub(crate) fn own(id: &str) -> VersionInfo {
        VersionInfo {
            key: id.into(),
            id: id.into(),
            source: OWN_SOURCE,
        }
    }

    pub(crate) fn release(version: &str) -> PhpRelease {
        let asset = format!("php-{version}-macos-arm64.tar.gz");
        PhpRelease {
            version: version.into(),
            url: format!("https://example.test/{asset}"),
            size: 65_000_000,
            checksum_url: format!("https://example.test/{asset}.sha256"),
        }
    }

    pub(crate) fn catalog() -> Catalog {
        Catalog {
            php: vec![release("8.5.12"), release("8.4.27"), release("8.3.36")],
            composer: Some(crate::setup::ComposerRelease {
                version: "2.11.1".into(),
                url: "https://example.test/composer.phar".into(),
                checksum_url: "https://example.test/composer.phar.sha256sum".into(),
            }),
            offline: false,
        }
    }

    #[test]
    fn equivalent_formulas_satisfy_the_preset() {
        let d = Detection {
            formulas: vec!["postgresql@17".into(), "mariadb".into()],
            ..Detection::default()
        };
        assert_eq!(
            installed_for("postgresql@18", &d).as_deref(),
            Some("postgresql@17")
        );
        assert_eq!(installed_for("mysql", &d).as_deref(), Some("mariadb"));
        assert_eq!(installed_for("redis", &d), None);
    }

    #[test]
    fn mysql_wins_over_mariadb_and_running_wins_over_newer() {
        let d = Detection {
            formulas: vec![
                "mariadb".into(),
                "mysql@8.4".into(),
                "postgresql@16".into(),
                "postgresql@18".into(),
            ],
            services: vec![svc("postgresql@16", true, false)],
            ..Detection::default()
        };
        assert_eq!(installed_for("mysql", &d).as_deref(), Some("mysql@8.4"));
        // postgresql@18 existe tal cual: se usa ese aunque otro esté en marcha.
        assert_eq!(
            installed_for("postgresql@18", &d).as_deref(),
            Some("postgresql@18")
        );
        let d = Detection {
            formulas: vec!["postgresql@15".into(), "postgresql@17".into()],
            services: vec![svc("postgresql@15", true, false)],
            ..Detection::default()
        };
        assert_eq!(
            installed_for("postgresql@18", &d).as_deref(),
            Some("postgresql@15")
        );
    }

    #[test]
    fn database_preselection_follows_running_then_installed_then_mysql() {
        assert_eq!(preselected_database(&Detection::default()), Database::Mysql);
        let installed_pg = Detection {
            formulas: vec!["postgresql@18".into()],
            ..Detection::default()
        };
        assert_eq!(preselected_database(&installed_pg), Database::Postgres);
        let both = Detection {
            formulas: vec!["postgresql@18".into(), "mysql".into()],
            services: vec![svc("postgresql@18", true, true), svc("mysql", false, false)],
            ..Detection::default()
        };
        assert_eq!(preselected_database(&both), Database::Postgres);
    }

    #[test]
    fn presets_resolve_as_documented() {
        let d = Detection::default();
        let c = catalog();
        let laravel = resolve(&ProfileInput::Laravel, &d, &c).unwrap();
        assert_eq!(laravel.php, PhpWant::AnyOwn);
        assert_eq!(
            laravel.services.get("mailpit"),
            Some(&ServiceWant::Installed)
        );
        assert!(laravel.launch_at_login && laravel.composer && laravel.shim);

        let services = resolve(
            &ProfileInput::LaravelServices {
                database: Database::Postgres,
            },
            &d,
            &c,
        )
        .unwrap();
        assert_eq!(
            services.services.get("postgresql@18"),
            Some(&ServiceWant::Installed)
        );
        assert_eq!(
            services.services.get("redis"),
            Some(&ServiceWant::Installed)
        );
        assert_eq!(
            services.services.get("mailpit"),
            Some(&ServiceWant::Autostart)
        );
        assert!(!services.services.contains_key("mysql"));

        let terminal = resolve(&ProfileInput::Terminal, &d, &c).unwrap();
        assert_eq!(
            terminal.php,
            PhpWant::Branches(vec!["8.5".into(), "8.4".into()])
        );
        assert!(terminal.services.is_empty());
        assert!(!terminal.launch_at_login);

        let offline = resolve(&ProfileInput::Terminal, &d, &Catalog::default()).unwrap();
        assert_eq!(offline.php, PhpWant::AnyOwn);
    }

    #[test]
    fn custom_rejects_what_elephant_cannot_install() {
        let d = Detection::default();
        let c = catalog();
        let custom = |services: &[&str], php: PhpWant| ProfileInput::Custom {
            profile: Profile {
                services: services
                    .iter()
                    .map(|id| ((*id).to_string(), ServiceWant::Installed))
                    .collect(),
                php,
                ..base_profile(ScenarioId::Laravel)
            },
        };
        for id in ["rabbitmq", "php", "composer", "node", "--cask"] {
            assert!(
                resolve(&custom(&[id], PhpWant::AnyOwn), &d, &c).is_err(),
                "{id}"
            );
        }
        assert!(resolve(&custom(&[], PhpWant::Branches(vec!["7.4".into()])), &d, &c).is_err());
        assert!(resolve(&custom(&[], PhpWant::Branches(vec!["../8".into()])), &d, &c).is_err());

        let ok = resolve(
            &custom(&["redis"], PhpWant::Branches(vec!["8.4".into()])),
            &d,
            &c,
        )
        .unwrap();
        assert_eq!(ok.scenario, ScenarioId::Custom);

        let mut bad_default = base_profile(ScenarioId::Custom);
        bad_default.default_php = Some("9.9.9".into());
        assert!(
            resolve(
                &ProfileInput::Custom {
                    profile: bad_default
                },
                &d,
                &c
            )
            .is_err()
        );
    }

    #[test]
    fn custom_accepts_installed_branches_missing_from_the_catalog() {
        let d = Detection {
            php_versions: vec![own("8.2.30")],
            ..Detection::default()
        };
        let profile = Profile {
            php: PhpWant::Branches(vec!["8.2".into()]),
            ..base_profile(ScenarioId::Custom)
        };
        assert!(resolve(&ProfileInput::Custom { profile }, &d, &catalog()).is_ok());
    }

    #[test]
    fn the_system_profile_describes_what_is_there() {
        let d = Detection {
            php_versions: vec![own("8.4.27")],
            global_php: Some("8.4.27".into()),
            formulas: vec!["mailpit".into(), "postgresql@18".into()],
            services: vec![
                svc("mailpit", false, false),
                svc("postgresql@18", true, true),
            ],
            shim: FileState::Same,
            composer_phar: true,
            launch_at_login: true,
            ..Detection::default()
        };
        let profile = profile_from_system(&d);
        assert_eq!(profile.php, PhpWant::Branches(vec!["8.4".into()]));
        assert_eq!(profile.services["mailpit"], ServiceWant::Installed);
        assert_eq!(profile.services["postgresql@18"], ServiceWant::Autostart);
        assert_eq!(profile.services["redis"], ServiceWant::Keep);
        assert_eq!(profile.default_php.as_deref(), Some("8.4.27"));
        assert!(profile.shim && profile.composer && profile.launch_at_login);
        // Lo que devuelve se acepta como Personalizado.
        assert!(resolve(&ProfileInput::Custom { profile }, &d, &catalog()).is_ok());
    }

    #[test]
    fn wizard_services_are_installable_services() {
        for id in WIZARD_SERVICES {
            assert!(components::is_installable_service(id), "{id}");
        }
    }

    #[test]
    fn cards_carry_costs_and_database_options() {
        let d = Detection {
            arch_arm64: true,
            formulas: vec!["postgresql@18".into()],
            services: vec![svc("postgresql@18", true, true)],
            ..Detection::default()
        };
        let measured = BTreeMap::from([("postgresql@18".to_string(), 16)]);
        let cards = scenarios(&d, &catalog(), &measured);
        let ids: Vec<ScenarioId> = cards.iter().map(|card| card.id).collect();
        assert_eq!(
            ids,
            [
                ScenarioId::Laravel,
                ScenarioId::LaravelServices,
                ScenarioId::Terminal,
                ScenarioId::Custom
            ]
        );
        assert!(cards[0].recommended && !cards[1].recommended);
        assert_eq!(cards[0].idle_ram_mb, 0);
        assert_eq!(cards[1].idle_ram_mb, 25);
        assert_eq!(
            cards[0].download_bytes,
            65_000_000 + super::super::plan::COMPOSER_BYTES
        );
        assert_eq!(
            cards[2].download_bytes,
            2 * 65_000_000 + super::super::plan::COMPOSER_BYTES
        );
        let pg = cards[1]
            .databases
            .iter()
            .find(|o| o.database == Database::Postgres)
            .unwrap();
        assert!(pg.preselected);
        assert_eq!(pg.idle_ram_mb, 16);
        assert_eq!(pg.label, "PostgreSQL");
        let mysql = cards[1]
            .databases
            .iter()
            .find(|o| o.database == Database::Mysql)
            .unwrap();
        assert_eq!((mysql.idle_ram_mb, mysql.preselected), (400, false));
        assert_eq!(cards[3].steps, 0);
    }

    #[test]
    fn labels_and_versions() {
        assert_eq!(label("postgresql@18"), "PostgreSQL 18");
        assert_eq!(label("mysql"), "MySQL");
        assert_eq!(branch_of("8.5.12"), "8.5");
        assert!(is_version("8.5.12"));
        assert!(!is_version("8.5") && !is_version("8.5.x") && !is_version("../8.5.1"));
        assert!(version_key("8.10.0") > version_key("8.9.30"));
    }
}
