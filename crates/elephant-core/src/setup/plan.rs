//! Del estado deseado al plan (04 §4.4). Función pura: no lee ni ejecuta
//! nada, así que se puede recalcular al aplicar para detectar planes obsoletos.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::presets;
use super::{
    Action, Catalog, Conflict, Detection, FileState, PathState, PhpRelease, PhpWant, Plan, Profile,
    SETUP_VERSION, ServiceWant, Step, StepGroup, Totals,
};
use crate::brew::Operation;
use crate::components;

/// Tamaño aproximado de `composer.phar` (≈ 3 MB): el catálogo no lo da.
pub(crate) const COMPOSER_BYTES: u64 = 3_200_000;

const KEY_BREW_UPDATE: &str = "brew-update";
const KEY_SHIM: &str = "shim";
const KEY_COMPOSER: &str = "composer";
const KEY_LAUNCH: &str = "launch-at-login";
const KEY_LAUNCH_OFF: &str = "launch-at-login-off";

pub(crate) fn download_key(version: &str) -> String {
    format!("download-php:{version}")
}

fn install_key(formula: &str) -> String {
    format!("brew-install:{formula}")
}

#[derive(Default)]
struct Builder {
    brew: Vec<Step>,
    files: Vec<Step>,
    start: Vec<Step>,
    optional: Vec<Step>,
    satisfied: Vec<String>,
    notices: Vec<String>,
    conflicts: Vec<Conflict>,
}

fn step(
    key: String,
    action: Action,
    group: StepGroup,
    title: String,
    command: Option<String>,
) -> Step {
    Step {
        key,
        action,
        group,
        title,
        command,
        optional: group == StepGroup::Optional,
        after: Vec::new(),
        restarts_service: false,
        since: SETUP_VERSION,
    }
}

impl Builder {
    fn conflict(&mut self, key: impl Into<String>, reason: impl Into<String>) {
        self.conflicts.push(Conflict {
            key: key.into(),
            reason: reason.into(),
        });
    }

    fn satisfied(&mut self, text: impl Into<String>) {
        let text = text.into();
        if !self.satisfied.contains(&text) {
            self.satisfied.push(text);
        }
    }

    fn notice(&mut self, text: impl Into<String>) {
        let text = text.into();
        if !self.notices.contains(&text) {
            self.notices.push(text);
        }
    }
}

/// Texto para la versión global: `8.4@Herd` → «8.4 de Herd».
fn php_label(key: &str) -> String {
    match key.split_once('@') {
        Some((id, source)) => format!("{id} de {source}"),
        None => key.to_string(),
    }
}

/// Descarga una versión del catálogo, o explica por qué no se puede.
/// `what` es la rama o la versión pedida; `None` = la última del catálogo.
fn want_download(
    b: &mut Builder,
    d: &Detection,
    c: &Catalog,
    release: Option<&PhpRelease>,
    what: Option<&str>,
) -> Option<String> {
    let key = format!("php:{}", what.unwrap_or("latest"));
    if !d.arch_arm64 {
        b.conflict(key, "Los PHP de Elephant son solo para Apple Silicon.");
        return None;
    }
    let Some(release) = release else {
        if c.offline {
            b.conflict(key, "Sin conexión: no se puede descargar PHP.");
        } else {
            let reason = match what {
                Some(what) => format!("PHP {what} no está en el catálogo de Elephant."),
                None => "No hay ninguna versión de PHP en el catálogo de Elephant.".to_string(),
            };
            b.conflict(key, reason);
        }
        return None;
    };
    let key = download_key(&release.version);
    if !b.files.iter().any(|s| s.key == key) {
        b.files.push(step(
            key.clone(),
            Action::DownloadPhp {
                release: release.clone(),
            },
            StepGroup::Download,
            format!("Descargar PHP {}", release.version),
            Some(format!("curl -fL {}", release.url)),
        ));
    }
    Some(release.version.clone())
}

fn latest_in_branch<'a>(c: &'a Catalog, branch: &str) -> Option<&'a PhpRelease> {
    c.php
        .iter()
        .filter(|release| presets::branch_of(&release.version) == branch)
        .max_by_key(|release| presets::version_key(&release.version))
}

fn latest(c: &Catalog) -> Option<&PhpRelease> {
    c.php
        .iter()
        .max_by_key(|release| presets::version_key(&release.version))
}

fn plan_php(b: &mut Builder, d: &Detection, p: &Profile, c: &Catalog) {
    let own = presets::own_versions(d);
    // Versiones propias con las que quedará el perfil, ya instaladas o por descargar.
    let mut wanted: Vec<String> = Vec::new();

    match &p.php {
        PhpWant::None => {}
        PhpWant::AnyOwn => match own.first() {
            Some(version) => {
                b.satisfied(format!("PHP {version}"));
                wanted.push(version.clone());
            }
            None => wanted.extend(want_download(b, d, c, latest(c), None)),
        },
        PhpWant::Branches(branches) => {
            for branch in branches {
                match own.iter().find(|v| presets::branch_of(v) == *branch) {
                    Some(version) => {
                        b.satisfied(format!("PHP {version}"));
                        wanted.push(version.clone());
                    }
                    None => wanted.extend(want_download(
                        b,
                        d,
                        c,
                        latest_in_branch(c, branch),
                        Some(branch),
                    )),
                }
            }
        }
    }

    // Una versión por defecto elegida a mano que todavía no está instalada.
    if let Some(version) = &p.default_php {
        let detected = d
            .php_versions
            .iter()
            .any(|v| v.key == *version || v.id == *version);
        if !detected {
            let release = c.php.iter().find(|release| release.version == *version);
            wanted.extend(want_download(b, d, c, release, Some(version)));
        }
    }

    let current = d.global_php.as_deref();
    let explicit = p.default_php.is_some();
    let target = p.default_php.clone().or_else(|| {
        wanted
            .iter()
            .max_by_key(|v| presets::version_key(v))
            .cloned()
    });
    let Some(target) = target else { return };
    // Solo se puede fijar lo que existe o se va a descargar en este plan.
    let downloading = b.files.iter().any(|s| s.key == download_key(&target));
    let exists = downloading
        || d.php_versions
            .iter()
            .any(|v| v.key == target || v.id == target);
    if !exists || current == Some(target.as_str()) {
        return;
    }
    let mut set = step(
        format!("php-global:{target}"),
        Action::SetGlobalPhp {
            version: target.clone(),
        },
        StepGroup::Configure,
        format!("Usar PHP {} por defecto", php_label(&target)),
        None,
    );
    if downloading {
        set.after.push(download_key(&target));
    }
    match current {
        // Sin global (o uno que no resuelve), o uno elegido en Personalizado: obligatorio.
        None => b.files.push(set),
        Some(_) if explicit => b.files.push(set),
        // Hay uno que funciona: cambiarlo es opcional.
        Some(now) => {
            set.group = StepGroup::Optional;
            set.optional = true;
            set.title = format!(
                "Usar PHP {} por defecto (ahora {})",
                php_label(&target),
                php_label(now)
            );
            b.optional.push(set);
        }
    }
}

fn plan_tools(b: &mut Builder, d: &Detection, p: &Profile, c: &Catalog) {
    if p.shim {
        match d.shim {
            FileState::Same => b.satisfied("Selector php"),
            state => b.files.push(step(
                KEY_SHIM.into(),
                Action::InstallShim,
                StepGroup::Configure,
                if state == FileState::Different {
                    "Actualizar el selector php".into()
                } else {
                    "Instalar el selector php".into()
                },
                Some("cp php-selector ~/.elephant-stack/bin/php".into()),
            )),
        }
    }

    if p.composer {
        if d.composer_phar {
            b.satisfied("Composer");
        } else if let Some(release) = &c.composer {
            let mut composer = step(
                KEY_COMPOSER.into(),
                Action::InstallComposer {
                    release: release.clone(),
                },
                StepGroup::Download,
                format!("Instalar Composer {}", release.version),
                Some(format!(
                    "curl -fL {} -o ~/.elephant-stack/libexec/composer.phar",
                    release.url
                )),
            );
            composer.after = b
                .files
                .iter()
                .filter(|s| s.key == KEY_SHIM || matches!(s.action, Action::DownloadPhp { .. }))
                .map(|s| s.key.clone())
                .collect();
            b.files.push(composer);
            if d.herd.is_some() {
                b.notice("Tienes Composer de Herd; Elephant instala el suyo, que usa el PHP de cada proyecto.");
            } else if d.formulas.iter().any(|f| f == "composer") {
                b.notice("Tienes Composer de Homebrew; Elephant instala el suyo, que usa el PHP de cada proyecto.");
            }
        } else if c.offline {
            b.conflict(
                KEY_COMPOSER,
                "Sin conexión: no se puede descargar Composer.",
            );
        } else {
            b.conflict(
                KEY_COMPOSER,
                "No se ha podido consultar la versión estable de Composer.",
            );
        }
    }
}

fn plan_services(b: &mut Builder, d: &Detection, p: &Profile) {
    for (id, want) in &p.services {
        // Lo que no está en la tabla de componentes no genera nada.
        if *want == ServiceWant::Keep || !components::is_installable_service(id) {
            continue;
        }
        let installed = presets::installed_for(id, d);
        let name = installed.clone().unwrap_or_else(|| id.clone());
        let label = presets::label(&name);
        let svc = presets::service(d, &name);
        let running = svc.is_some_and(|s| s.running);
        let registered = svc.is_some_and(|s| s.registered);

        let mut after = Vec::new();
        if installed.is_none() {
            b.brew.push(step(
                install_key(id),
                Action::BrewInstall {
                    formula: id.clone(),
                },
                StepGroup::BrewInstall,
                format!("Instalar {label} con Homebrew"),
                Some(format!("brew install {id}")),
            ));
            after.push(install_key(id));
            let engine = presets::engine(id);
            if d.data_dirs.iter().any(|dir| {
                let last = dir.rsplit('/').next().unwrap_or(dir);
                presets::engine(last) == engine
            }) {
                b.notice(format!(
                    "Hay datos de {} de una instalación anterior: se reutilizarán.",
                    presets::label(id.split('@').next().unwrap_or(id))
                ));
            }
        } else if !(*want == ServiceWant::Autostart && registered) {
            b.satisfied(format!("{label} (Homebrew)"));
        }

        match want {
            ServiceWant::Keep => {}
            ServiceWant::Autostart if registered => {
                b.satisfied(format!("{label} al iniciar sesión"))
            }
            ServiceWant::Autostart => {
                let key = format!("autostart:{name}");
                let port = presets::port_of(&name);
                let busy = port
                    .and_then(|port| d.ports.iter().find(|use_| use_.port == port && use_.busy));
                if let (Some(use_), false) = (busy, running) {
                    let owner = use_.owner_hint.as_deref().map_or_else(
                        || "otro programa".to_string(),
                        |hint| format!("parece que {hint}"),
                    );
                    b.conflict(
                        key,
                        format!("El puerto {} está ocupado ({owner}): no se arranca {label} de Homebrew.", use_.port),
                    );
                    continue;
                }
                let (op, title, command) = if running {
                    (
                        Operation::EnableAutostart,
                        format!("{label} al iniciar sesión"),
                        format!("brew services stop {name} && brew services start {name}"),
                    )
                } else {
                    (
                        Operation::TurnOnWithAutostart,
                        format!("Arrancar {label} y dejarlo al iniciar sesión"),
                        format!("brew services start {name}"),
                    )
                };
                let mut start = step(
                    key,
                    Action::Service {
                        name: name.clone(),
                        op,
                    },
                    StepGroup::Start,
                    title,
                    Some(command),
                );
                start.after = after;
                start.restarts_service = running;
                b.start.push(start);
            }
            ServiceWant::Installed if registered => {
                // Nunca se apaga nada: quitar el arranque es opcional y no detiene el servicio.
                let mut off = step(
                    format!("autostart-off:{name}"),
                    Action::Service {
                        name: name.clone(),
                        op: Operation::DisableAutostart,
                    },
                    StepGroup::Optional,
                    if running {
                        format!("Quitar el arranque automático de {label} (sigue encendido)")
                    } else {
                        format!("Quitar el arranque automático de {label}")
                    },
                    Some(if running {
                        format!("brew services stop {name} && brew services run {name}")
                    } else {
                        format!("brew services stop {name}")
                    }),
                );
                off.restarts_service = running;
                b.optional.push(off);
            }
            ServiceWant::Installed => {}
        }
    }

    if !b.brew.is_empty() {
        b.brew.insert(
            0,
            step(
                KEY_BREW_UPDATE.into(),
                Action::BrewUpdate,
                StepGroup::BrewInstall,
                "Actualizar la lista de fórmulas de Homebrew".into(),
                Some("brew update".into()),
            ),
        );
        b.notice("Homebrew puede actualizar librerías que necesitan las fórmulas que se instalen.");
    }
}

fn plan_launch(b: &mut Builder, d: &Detection, p: &Profile) {
    match (p.launch_at_login, d.launch_at_login) {
        (true, true) => b.satisfied("Elephant al iniciar sesión"),
        (true, false) => b.start.push(step(
            KEY_LAUNCH.into(),
            Action::LaunchAtLogin { enabled: true },
            StepGroup::Start,
            "Abrir Elephant al iniciar sesión, solo en la barra de menús".into(),
            None,
        )),
        (false, true) => b.optional.push(step(
            KEY_LAUNCH_OFF.into(),
            Action::LaunchAtLogin { enabled: false },
            StepGroup::Optional,
            "No abrir Elephant al iniciar sesión".into(),
            None,
        )),
        (false, false) => {}
    }
}

fn plan_notices(b: &mut Builder, d: &Detection, p: &Profile) {
    match &d.herd {
        Some(herd) => {
            b.notice("Herd está instalado. Elephant convive con él: no toca sus sitios, su PHP ni sus servicios.");
            let tld = herd.tld.as_deref().unwrap_or("test");
            b.notice(match herd.sites {
                Some(1) => format!("Herd sigue sirviendo tu sitio .{tld}."),
                Some(n) if n > 0 => format!("Herd sigue sirviendo tus {n} sitios .{tld}."),
                _ => format!("Herd sigue sirviendo tus sitios .{tld}."),
            });
        }
        None => b.notice("Elephant todavía no sirve sitios .test; llegará en una próxima versión."),
    }

    if p.shim || d.shim != FileState::Missing {
        match d.path {
            PathState::Active => b.satisfied("Selector php en el PATH"),
            PathState::ShadowedByHerd => b.notice(
                "Herd va por delante de Elephant en el PATH de tu terminal: el comando php sería el de Herd. Puedes corregirlo en ~/.zshrc.",
            ),
            PathState::Missing | PathState::AfterHerdMissing => {
                b.notice("El selector php no está en el PATH de tu terminal. Puedes añadirlo a ~/.zshrc.")
            }
        }
    }
}

fn plan_id(plan: &Plan) -> String {
    // Huella de lo que se va a ejecutar y explicar, sin los totales: la RAM
    // medida cambia de una vez a otra y no hace obsoleto un plan.
    let payload = serde_json::to_vec(&(
        &plan.scenario,
        &plan.steps,
        &plan.satisfied,
        &plan.notices,
        &plan.conflicts,
    ))
    .unwrap_or_default();
    let digest = Sha256::digest(&payload);
    digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `measured`: RAM real (MB) de los servicios en marcha, por nombre. Vacío =
/// solo estimaciones.
pub(crate) fn plan(
    d: &Detection,
    p: &Profile,
    c: &Catalog,
    measured: &BTreeMap<String, u32>,
) -> Plan {
    let mut b = Builder::default();
    plan_php(&mut b, d, p, c);
    plan_tools(&mut b, d, p, c);
    plan_services(&mut b, d, p);
    plan_launch(&mut b, d, p);
    plan_notices(&mut b, d, p);

    // Orden de ejecución: Homebrew y archivos (dos carriles), los arranques y
    // por último los opcionales.
    let mut steps = b.brew;
    steps.extend(b.files);
    steps.extend(b.start);
    steps.extend(b.optional);

    let download_bytes = steps
        .iter()
        .map(|s| match &s.action {
            Action::DownloadPhp { release } => release.size,
            Action::InstallComposer { .. } => COMPOSER_BYTES,
            _ => 0,
        })
        .sum();
    let brew_formulas = steps
        .iter()
        .filter(|s| matches!(s.action, Action::BrewInstall { .. }))
        .count();
    let idle_ram_mb = p
        .services
        .iter()
        .filter(|(id, want)| {
            **want == ServiceWant::Autostart && components::is_installable_service(id)
        })
        .map(|(id, _)| {
            let name = presets::installed_for(id, d).unwrap_or_else(|| id.clone());
            measured
                .get(&name)
                .copied()
                .unwrap_or_else(|| presets::idle_ram_estimate(&name))
        })
        .sum();

    let mut plan = Plan {
        id: String::new(),
        scenario: p.scenario,
        steps,
        satisfied: b.satisfied,
        notices: b.notices,
        conflicts: b.conflicts,
        totals: Totals {
            download_bytes,
            brew_formulas: u32::try_from(brew_formulas).unwrap_or(u32::MAX),
            idle_ram_mb,
        },
    };
    plan.id = plan_id(&plan);
    plan
}

/// La versión propia que deja una descarga, para simular su efecto en tests.
#[cfg(test)]
pub(crate) fn own_version(version: &str) -> crate::php::VersionInfo {
    crate::php::VersionInfo {
        key: version.into(),
        id: version.into(),
        source: presets::OWN_SOURCE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::presets::tests::{catalog, own, svc};
    use crate::setup::{Database, HerdInfo, PortUse, ProfileInput, ScenarioId};

    /// Aplica a la detección el efecto de cada paso, como si se hubiera ejecutado.
    fn simulate(d: &Detection, plan: &Plan, with_optional: bool) -> Detection {
        let mut d = d.clone();
        for step in plan.steps.iter().filter(|s| with_optional || !s.optional) {
            match &step.action {
                Action::BrewUpdate => {}
                Action::BrewInstall { formula } => {
                    d.formulas.push(formula.clone());
                    d.services.push(svc(formula, false, false));
                }
                Action::DownloadPhp { release } => {
                    d.php_versions.push(own_version(&release.version))
                }
                Action::SetGlobalPhp { version } => d.global_php = Some(version.clone()),
                Action::InstallShim => d.shim = FileState::Same,
                Action::InstallComposer { .. } => d.composer_phar = true,
                Action::Service { name, op } => {
                    let service = d
                        .services
                        .iter_mut()
                        .find(|s| s.name == *name)
                        .expect("servicio simulado");
                    match op {
                        Operation::TurnOnWithAutostart => {
                            service.running = true;
                            service.registered = true;
                        }
                        Operation::EnableAutostart => service.registered = true,
                        Operation::DisableAutostart => service.registered = false,
                        other => panic!("el asistente no debería pedir {other:?}"),
                    }
                }
                Action::LaunchAtLogin { enabled } => d.launch_at_login = *enabled,
            }
        }
        d
    }

    fn empty_mac() -> Detection {
        Detection {
            arch_arm64: true,
            ..Detection::default()
        }
    }

    /// Como el Mac del usuario el 2026-11-06 (04 §6, criterios manuales).
    fn this_mac() -> Detection {
        Detection {
            arch_arm64: true,
            formulas: vec![
                "mailpit".into(),
                "postgresql@18".into(),
                "mysql".into(),
                "rabbitmq".into(),
            ],
            services: vec![
                svc("mailpit", false, false),
                svc("postgresql@18", true, true),
                svc("mysql", false, false),
                svc("rabbitmq", true, true),
            ],
            php_versions: vec![
                own("8.4.27"),
                crate::php::VersionInfo {
                    key: "8.4@Herd".into(),
                    id: "8.4".into(),
                    source: "Herd",
                },
            ],
            global_php: Some("8.4.27".into()),
            shim: FileState::Same,
            composer_phar: false,
            path: PathState::Active,
            ports: vec![
                PortUse {
                    port: 3306,
                    busy: false,
                    owner_hint: None,
                },
                PortUse {
                    port: 5432,
                    busy: true,
                    owner_hint: Some("PostgreSQL 18 de Homebrew".into()),
                },
            ],
            herd: Some(HerdInfo {
                running: true,
                tld: Some("test".into()),
                sites: Some(12),
            }),
            launch_at_login: false,
            data_dirs: vec![
                "/opt/homebrew/var/mysql".into(),
                "/opt/homebrew/var/postgresql@18".into(),
            ],
        }
    }

    fn all_installed() -> Detection {
        Detection {
            arch_arm64: true,
            formulas: vec![
                "mailpit".into(),
                "redis".into(),
                "mysql".into(),
                "postgresql@18".into(),
            ],
            services: vec![
                svc("mailpit", true, true),
                svc("redis", true, true),
                svc("mysql", true, true),
                svc("postgresql@18", true, true),
            ],
            php_versions: vec![own("8.5.12"), own("8.4.27")],
            global_php: Some("8.5.12".into()),
            shim: FileState::Same,
            composer_phar: true,
            path: PathState::Active,
            launch_at_login: true,
            ..Detection::default()
        }
    }

    fn inputs() -> Vec<ProfileInput> {
        vec![
            ProfileInput::Laravel,
            ProfileInput::LaravelServices {
                database: Database::Mysql,
            },
            ProfileInput::LaravelServices {
                database: Database::Postgres,
            },
            ProfileInput::Terminal,
        ]
    }

    fn plan_for(d: &Detection, input: &ProfileInput) -> Plan {
        let c = catalog();
        let profile = presets::resolve(input, d, &c).unwrap();
        plan(d, &profile, &c, &BTreeMap::new())
    }

    fn keys(plan: &Plan) -> Vec<&str> {
        plan.steps.iter().map(|s| s.key.as_str()).collect()
    }

    fn mandatory(plan: &Plan) -> Vec<&Step> {
        plan.steps.iter().filter(|s| !s.optional).collect()
    }

    #[test]
    fn plans_are_idempotent() {
        let c = catalog();
        for d in [empty_mac(), this_mac(), all_installed()] {
            for input in inputs() {
                let profile = presets::resolve(&input, &d, &c).unwrap();
                let first = plan(&d, &profile, &c, &BTreeMap::new());

                let after = simulate(&d, &first, false);
                let second = plan(&after, &profile, &c, &BTreeMap::new());
                assert!(
                    mandatory(&second).is_empty(),
                    "{input:?}: {:?}",
                    keys(&second)
                );

                let after_all = simulate(&d, &first, true);
                let third = plan(&after_all, &profile, &c, &BTreeMap::new());
                assert!(third.steps.is_empty(), "{input:?}: {:?}", keys(&third));
            }
            let system = presets::profile_from_system(&d);
            assert!(
                plan(&d, &system, &c, &BTreeMap::new()).steps.is_empty(),
                "{d:?}"
            );
        }
    }

    #[test]
    fn never_installs_php_or_composer_with_homebrew() {
        let c = catalog();
        for d in [empty_mac(), this_mac(), all_installed()] {
            for input in inputs() {
                for step in plan_for(&d, &input).steps {
                    if let Action::BrewInstall { formula } = &step.action {
                        assert!(formula != "php" && formula != "composer", "{input:?}");
                    }
                }
            }
            let mut custom = presets::profile_from_system(&d);
            custom.composer = true;
            custom.php = PhpWant::AnyOwn;
            custom
                .services
                .insert("redis".into(), ServiceWant::Autostart);
            for step in plan(&d, &custom, &c, &BTreeMap::new()).steps {
                assert!(
                    !matches!(step.action, Action::BrewInstall { ref formula } if formula == "php" || formula == "composer")
                );
            }
        }
    }

    #[test]
    fn equivalents_act_on_the_installed_formula() {
        let d = Detection {
            arch_arm64: true,
            formulas: vec!["postgresql@17".into(), "mariadb".into()],
            services: vec![
                svc("postgresql@17", false, false),
                svc("mariadb", false, false),
            ],
            ..Detection::default()
        };
        let c = catalog();
        let mut profile = presets::profile_from_system(&d);
        profile
            .services
            .insert("postgresql@18".into(), ServiceWant::Autostart);
        profile
            .services
            .insert("mysql".into(), ServiceWant::Autostart);
        let plan = plan(&d, &profile, &c, &BTreeMap::new());
        assert!(
            !plan
                .steps
                .iter()
                .any(|s| matches!(s.action, Action::BrewInstall { .. }))
        );
        let names: Vec<&str> = plan
            .steps
            .iter()
            .filter_map(|s| match &s.action {
                Action::Service { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["mariadb", "postgresql@17"]);
    }

    #[test]
    fn registered_and_installed_only_offers_an_optional_step() {
        for d in [empty_mac(), this_mac(), all_installed()] {
            for input in inputs() {
                for step in mandatory(&plan_for(&d, &input)) {
                    if let Action::Service { op, .. } = &step.action {
                        assert!(
                            !matches!(op, Operation::TurnOff | Operation::DisableAutostart),
                            "{input:?}"
                        );
                    }
                }
            }
        }
        let plan = plan_for(
            &this_mac(),
            &ProfileInput::LaravelServices {
                database: Database::Postgres,
            },
        );
        let off = plan
            .steps
            .iter()
            .find(|s| s.key == "autostart-off:postgresql@18")
            .unwrap();
        assert!(off.optional && off.restarts_service);
        assert_eq!(off.group, StepGroup::Optional);
    }

    #[test]
    fn services_outside_the_table_are_ignored() {
        let d = this_mac();
        let c = catalog();
        let mut profile = presets::profile_from_system(&d);
        profile
            .services
            .insert("rabbitmq".into(), ServiceWant::Installed);
        let plan = plan(&d, &profile, &c, &BTreeMap::new());
        assert!(plan.steps.iter().all(|s| !s.key.contains("rabbitmq")));
        assert!(plan.satisfied.iter().all(|s| !s.contains("rabbitmq")));
    }

    #[test]
    fn a_busy_port_is_a_conflict_not_a_start() {
        let mut d = empty_mac();
        d.formulas.push("mysql".into());
        d.services.push(svc("mysql", false, false));
        d.ports.push(PortUse {
            port: 3306,
            busy: true,
            owner_hint: Some("mysqld de cristian-smw".into()),
        });
        let mut profile = presets::profile_from_system(&d);
        profile
            .services
            .insert("mysql".into(), ServiceWant::Autostart);
        let plan = plan(&d, &profile, &catalog(), &BTreeMap::new());
        assert!(
            plan.steps
                .iter()
                .all(|s| !matches!(s.action, Action::Service { .. }))
        );
        let conflict = plan
            .conflicts
            .iter()
            .find(|c| c.key == "autostart:mysql")
            .unwrap();
        assert!(
            conflict.reason.contains("3306") && conflict.reason.contains("cristian-smw"),
            "{}",
            conflict.reason
        );
    }

    #[test]
    fn a_working_global_is_never_changed_without_asking() {
        let plan = plan_for(&this_mac(), &ProfileInput::Terminal);
        assert!(
            mandatory(&plan)
                .iter()
                .all(|s| !matches!(s.action, Action::SetGlobalPhp { .. }))
        );
        let optional = plan
            .steps
            .iter()
            .find(|s| s.key == "php-global:8.5.12")
            .unwrap();
        assert!(optional.optional);
        assert_eq!(optional.title, "Usar PHP 8.5.12 por defecto (ahora 8.4.27)");
        assert_eq!(optional.after, ["download-php:8.5.12"]);

        // Sin global, se fija la descargada después de descargarla.
        let plan = plan_for(&empty_mac(), &ProfileInput::Laravel);
        let set = plan
            .steps
            .iter()
            .find(|s| s.key == "php-global:8.5.12")
            .unwrap();
        assert!(!set.optional);
        assert_eq!(set.after, ["download-php:8.5.12"]);
    }

    #[test]
    fn offline_turns_downloads_into_conflicts_and_keeps_the_rest() {
        let d = empty_mac();
        let online = plan_for(
            &d,
            &ProfileInput::LaravelServices {
                database: Database::Mysql,
            },
        );
        let offline_catalog = Catalog {
            offline: true,
            ..Catalog::default()
        };
        let profile = presets::resolve(
            &ProfileInput::LaravelServices {
                database: Database::Mysql,
            },
            &d,
            &offline_catalog,
        )
        .unwrap();
        let offline = plan(&d, &profile, &offline_catalog, &BTreeMap::new());

        assert!(offline.steps.iter().all(|s| !matches!(
            s.action,
            Action::DownloadPhp { .. } | Action::InstallComposer { .. }
        )));
        assert!(
            offline
                .conflicts
                .iter()
                .any(|c| c.reason == "Sin conexión: no se puede descargar PHP.")
        );
        assert!(offline.conflicts.iter().any(|c| c.key == "composer"));
        let without_downloads: Vec<&str> = online
            .steps
            .iter()
            .filter(|s| {
                !matches!(
                    s.action,
                    Action::DownloadPhp { .. }
                        | Action::InstallComposer { .. }
                        | Action::SetGlobalPhp { .. }
                )
            })
            .map(|s| s.key.as_str())
            .collect();
        assert_eq!(keys(&offline), without_downloads);
    }

    #[test]
    fn intel_macs_get_a_conflict_instead_of_a_download() {
        let d = Detection::default();
        let plan = plan_for(&d, &ProfileInput::Laravel);
        assert!(
            plan.steps
                .iter()
                .all(|s| !matches!(s.action, Action::DownloadPhp { .. }))
        );
        assert!(
            plan.conflicts
                .iter()
                .any(|c| c.reason.contains("Apple Silicon"))
        );
    }

    #[test]
    fn laravel_on_this_mac_matches_the_manual_criteria() {
        let plan = plan_for(&this_mac(), &ProfileInput::Laravel);
        assert_eq!(keys(&plan), ["composer", "launch-at-login"]);
        for text in [
            "PHP 8.4.27",
            "Mailpit (Homebrew)",
            "Selector php en el PATH",
        ] {
            assert!(
                plan.satisfied.iter().any(|s| s == text),
                "{text}: {:?}",
                plan.satisfied
            );
        }
        assert!(
            plan.notices
                .iter()
                .any(|n| n.starts_with("Herd está instalado"))
        );
        assert!(plan.notices.iter().any(|n| n.contains("Composer de Herd")));
        assert!(
            plan.notices
                .iter()
                .any(|n| n == "Herd sigue sirviendo tus 12 sitios .test.")
        );
        assert!(plan.steps.iter().all(|s| !s.key.contains("postgresql")));
        assert_eq!(plan.totals.download_bytes, COMPOSER_BYTES);
        let composer = &plan.steps[0];
        assert!(composer.after.is_empty());
    }

    #[test]
    fn laravel_services_on_this_mac_matches_the_manual_criteria() {
        let d = this_mac();
        assert_eq!(presets::preselected_database(&d), Database::Postgres);
        let plan = plan_for(
            &d,
            &ProfileInput::LaravelServices {
                database: Database::Postgres,
            },
        );
        assert_eq!(
            keys(&plan),
            [
                "brew-update",
                "brew-install:redis",
                "composer",
                "autostart:mailpit",
                "launch-at-login",
                "autostart-off:postgresql@18"
            ]
        );
        let mailpit = plan
            .steps
            .iter()
            .find(|s| s.key == "autostart:mailpit")
            .unwrap();
        assert!(matches!(
            mailpit.action,
            Action::Service {
                op: Operation::TurnOnWithAutostart,
                ..
            }
        ));
        assert!(plan.steps.iter().all(|s| !s.key.contains("mysql")));
        assert_eq!(plan.totals.brew_formulas, 1);
        assert_eq!(plan.totals.idle_ram_mb, 25);
    }

    #[test]
    fn installs_come_before_their_start_and_after_the_update() {
        let plan = plan_for(
            &empty_mac(),
            &ProfileInput::LaravelServices {
                database: Database::Mysql,
            },
        );
        let order = keys(&plan);
        assert_eq!(order[0], "brew-update");
        let mailpit = plan
            .steps
            .iter()
            .find(|s| s.key == "autostart:mailpit")
            .unwrap();
        assert_eq!(mailpit.after, ["brew-install:mailpit"]);
        let composer = plan.steps.iter().find(|s| s.key == "composer").unwrap();
        assert_eq!(composer.after, ["download-php:8.5.12", "shim"]);
        assert!(
            plan.notices
                .iter()
                .any(|n| n.contains("todavía no sirve sitios"))
        );
    }

    #[test]
    fn old_data_is_announced_when_reinstalling() {
        let mut d = empty_mac();
        d.data_dirs.push("/opt/homebrew/var/mysql".into());
        let plan = plan_for(
            &d,
            &ProfileInput::LaravelServices {
                database: Database::Mysql,
            },
        );
        assert!(
            plan.notices
                .iter()
                .any(|n| n == "Hay datos de MySQL de una instalación anterior: se reutilizarán.")
        );
    }

    #[test]
    fn running_service_to_autostart_restarts_it_once() {
        let mut d = empty_mac();
        d.formulas.push("mailpit".into());
        d.services.push(svc("mailpit", true, false));
        let plan = plan_for(
            &d,
            &ProfileInput::LaravelServices {
                database: Database::Mysql,
            },
        );
        let step = plan
            .steps
            .iter()
            .find(|s| s.key == "autostart:mailpit")
            .unwrap();
        assert!(matches!(
            step.action,
            Action::Service {
                op: Operation::EnableAutostart,
                ..
            }
        ));
        assert!(step.restarts_service);
    }

    #[test]
    fn the_id_tracks_what_would_run_but_not_measured_ram() {
        let d = this_mac();
        let c = catalog();
        let profile = presets::resolve(
            &ProfileInput::LaravelServices {
                database: Database::Postgres,
            },
            &d,
            &c,
        )
        .unwrap();
        let a = plan(&d, &profile, &c, &BTreeMap::new());
        let b = plan(
            &d,
            &profile,
            &c,
            &BTreeMap::from([("mailpit".to_string(), 31)]),
        );
        assert_eq!(a.id, b.id);
        assert_eq!(a.id.len(), 32);
        let mut changed = d.clone();
        changed.formulas.push("redis".into());
        assert_ne!(a.id, plan(&changed, &profile, &c, &BTreeMap::new()).id);
        assert_eq!(a.scenario, ScenarioId::LaravelServices);
    }

    #[test]
    fn launch_at_login_is_only_removed_if_asked() {
        let d = all_installed();
        let plan = plan_for(&d, &ProfileInput::Terminal);
        let off = plan
            .steps
            .iter()
            .find(|s| s.key == "launch-at-login-off")
            .unwrap();
        assert!(off.optional);
    }

    #[test]
    fn path_notices_follow_the_detection() {
        let mut d = this_mac();
        d.path = PathState::ShadowedByHerd;
        let plan = plan_for(&d, &ProfileInput::Laravel);
        assert!(
            plan.notices
                .iter()
                .any(|n| n.starts_with("Herd va por delante"))
        );
        d.path = PathState::Missing;
        let plan = plan_for(&d, &ProfileInput::Laravel);
        assert!(
            plan.notices
                .iter()
                .any(|n| n.starts_with("El selector php no está en el PATH"))
        );
    }
}
