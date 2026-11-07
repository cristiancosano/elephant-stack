//! Asistente inicial (04): detección del sistema, escenarios, plan y
//! ejecución.
//!
//! - [`detect`] lee el Mac sin tocar nada.
//! - [`resolve`] convierte lo que elige la interfaz en un [`Profile`].
//! - [`plan`] compara los dos: es puro, así que se recalcula al aplicar y un
//!   plan obsoleto no se ejecuta ([`apply_if_current`]).
//! - [`apply`] ejecuta el plan en dos carriles (Homebrew y archivos).
//!
//! El plan solo suma: nunca desinstala ni apaga nada sin una casilla opcional
//! que viene desmarcada.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::brew::{Operation, Service};
use crate::{monitor, tr};
use crate::php::VersionInfo;

mod apply;
pub(crate) mod catalog;
pub(crate) mod detect;
pub(crate) mod download;
mod plan;
pub(crate) mod presets;
pub mod shell;
pub mod state;

/// Sube cuando un escenario gana pasos nuevos (`Step::since`).
pub const SETUP_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ScenarioId {
    Laravel,
    LaravelServices,
    Terminal,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Database {
    Mysql,
    Postgres,
}

/// Estado deseado de un servicio. `Keep` es «no tocar», no «desinstalar».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ServiceWant {
    Keep,
    Installed,
    Autostart,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PhpWant {
    /// Cualquier versión propia vale; si no hay ninguna, la última del catálogo.
    AnyOwn,
    /// Ramas concretas («8.5», «8.4»); cada una, al último parche del catálogo.
    Branches(Vec<String>),
    /// No instalar PHP propio.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Profile {
    pub scenario: ScenarioId,
    pub php: PhpWant,
    pub composer: bool,
    /// Instalar el selector `php` en `~/.elephant-stack/bin`.
    pub shim: bool,
    /// Solo ids de servicios instalables de la tabla de componentes.
    pub services: BTreeMap<String, ServiceWant>,
    pub launch_at_login: bool,
    /// Versión global que se quiere (`8.4.27`); `None` = la que decida el plan.
    pub default_php: Option<String>,
}

/// Lo que puede enviar la interfaz: nunca un perfil de escenario ya resuelto.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "scenario")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ProfileInput {
    Laravel,
    LaravelServices { database: Database },
    Terminal,
    Custom { profile: Profile },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FileState {
    #[default]
    Missing,
    Same,
    Different,
}

/// Si `~/.elephant-stack/bin` está en el PATH de la shell del usuario.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PathState {
    Active,
    AfterHerdMissing,
    ShadowedByHerd,
    #[default]
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PortUse {
    pub port: u16,
    pub busy: bool,
    pub owner_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HerdInfo {
    pub running: bool,
    pub tld: Option<String>,
    pub sites: Option<u32>,
}

/// Lo que hay en el Mac (04 §4.2). Solo lectura.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Detection {
    pub arch_arm64: bool,
    /// `brew list --formula -1`.
    pub formulas: Vec<String>,
    pub services: Vec<Service>,
    pub php_versions: Vec<VersionInfo>,
    pub global_php: Option<String>,
    /// `~/.elephant-stack/bin/php` frente al selector empaquetado.
    pub shim: FileState,
    /// `~/.elephant-stack/libexec/composer.phar`.
    pub composer_phar: bool,
    pub path: PathState,
    pub ports: Vec<PortUse>,
    pub herd: Option<HerdInfo>,
    pub launch_at_login: bool,
    /// Datos de instalaciones anteriores (`/opt/homebrew/var/mysql`…).
    pub data_dirs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PhpRelease {
    pub version: String,
    pub url: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: u64,
    pub checksum_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ComposerRelease {
    pub version: String,
    pub url: String,
    pub checksum_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Catalog {
    pub php: Vec<PhpRelease>,
    pub composer: Option<ComposerRelease>,
    /// Sin red ni caché: las descargas pasan a ser conflictos.
    pub offline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum StepGroup {
    Download,
    BrewInstall,
    Configure,
    Start,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Action {
    BrewUpdate,
    BrewInstall { formula: String },
    DownloadPhp { release: PhpRelease },
    SetGlobalPhp { version: String },
    InstallShim,
    InstallComposer { release: ComposerRelease },
    Service { name: String, op: Operation },
    LaunchAtLogin { enabled: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Step {
    /// Estable: `brew-install:redis`, `autostart-off:postgresql@18`.
    pub key: String,
    pub action: Action,
    pub group: StepGroup,
    pub title: String,
    /// El comando equivalente, para «Ver comandos».
    pub command: Option<String>,
    /// Los opcionales vienen desmarcados.
    pub optional: bool,
    /// Claves de las que depende.
    pub after: Vec<String>,
    /// Homebrew reinicia el servicio al cambiar el arranque automático.
    pub restarts_service: bool,
    /// `SETUP_VERSION` en la que apareció el paso.
    pub since: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Conflict {
    pub key: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Totals {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub download_bytes: u64,
    pub brew_formulas: u32,
    pub idle_ram_mb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Plan {
    /// Huella de (detección, perfil, catálogo): detecta planes obsoletos.
    pub id: String,
    pub scenario: ScenarioId,
    pub steps: Vec<Step>,
    /// «Ya lo tienes».
    pub satisfied: Vec<String>,
    pub notices: Vec<String>,
    /// El aviso de `notices` que dice que el selector php no está en el PATH,
    /// si está. La interfaz lo oculta cuando enseña su propio aviso con botón.
    #[serde(default)]
    pub path_notice: Option<String>,
    pub conflicts: Vec<Conflict>,
    pub totals: Totals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum StepState {
    Pending,
    Running,
    Ok,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetupProgress {
    pub key: String,
    pub state: StepState,
    /// Última línea de brew o curl.
    pub line: Option<String>,
    /// Descargas: bytes hechos y totales.
    #[cfg_attr(feature = "ts", ts(type = "[number, number] | null"))]
    pub bytes: Option<(u64, u64)>,
    /// Motivo de `Failed` o `Skipped`, ya traducido.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ApplyReport {
    pub done: Vec<String>,
    /// (clave, motivo).
    pub failed: Vec<(String, String)>,
    pub skipped: Vec<(String, String)>,
    pub log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ApplyOutcome {
    Done {
        report: ApplyReport,
    },
    /// El sistema cambió desde la revisión: hay que revisar este plan nuevo.
    Stale {
        plan: Plan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DatabaseOption {
    pub database: Database,
    pub label: String,
    pub idle_ram_mb: u32,
    pub preselected: bool,
}

/// Tarjeta de un escenario en la pantalla de elección.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ScenarioCard {
    pub id: ScenarioId,
    pub name: String,
    pub phrase: String,
    pub recommended: bool,
    pub components: Vec<String>,
    pub idle_ram_mb: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub download_bytes: u64,
    pub steps: u32,
    pub databases: Vec<DatabaseOption>,
}

/// `~/.elephant-stack/setup.json`: la única fuente de «primera ejecución» (C8).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetupFile {
    pub version: u32,
    /// `None` = «No cambiar nada».
    pub scenario: Option<ScenarioId>,
    pub database: Option<Database>,
    pub custom: Option<Profile>,
    /// RFC 3339 en UTC.
    pub completed_at: Option<String>,
}

/// Bloque que se añadiría a `~/.zshrc` (04 §4.7).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ShellPreview {
    pub path: String,
    pub diff: String,
    /// Hay que devolverlo a `shell::apply`: solo se aplica lo que se ha visto.
    pub hash: String,
    pub needed: bool,
}

/// Lo que solo la app sabe hacer. Core no conoce Tauri.
pub struct ApplyContext<'a> {
    /// Selector `php` empaquetado (sidecar `php-selector`).
    pub shim_source: PathBuf,
    pub launch_at_login: &'a (dyn Fn(bool) -> Result<(), String> + Sync),
}

/// Lo que hay en el Mac. Solo lectura; tarda menos de 1,5 s (brew, `ps` y
/// los puertos en paralelo). `launch_at_login` lo sabe la app.
pub fn detect(launch_at_login: bool) -> Detection {
    detect::detect(launch_at_login)
}

/// Releases de php-builds y Composer estable. Una hora en memoria y copia en
/// `~/.elephant-stack/cache/catalog.json` para trabajar sin red.
pub use catalog::{DEFAULT_SOURCE, SourceCheck, check_source, parse_source};

pub fn catalog(force_refresh: bool) -> Catalog {
    catalog::catalog(force_refresh)
}

/// De la elección de la interfaz a un perfil. Valida Personalizado: solo
/// servicios instalables de la tabla y ramas que existen.
pub fn resolve(input: &ProfileInput, d: &Detection, c: &Catalog) -> Result<Profile, String> {
    presets::resolve(input, d, c)
}

/// Personalizado sin tocar nada: describe el sistema, así que el plan sale vacío.
pub fn profile_from_system(d: &Detection) -> Profile {
    presets::profile_from_system(d)
}

/// Tarjetas de escenario con su RAM en reposo, descarga y número de pasos.
/// Usa la RAM real de los servicios que ya están en marcha.
pub fn scenarios(d: &Detection, c: &Catalog) -> Vec<ScenarioCard> {
    presets::scenarios(d, c, &measured_ram(d))
}

/// Pura: no ejecuta nada. La RAM es la estimada; [`plan_measured`] usa la real.
pub fn plan(d: &Detection, p: &Profile, c: &Catalog) -> Plan {
    plan::plan(d, p, c, &BTreeMap::new())
}

/// Como [`plan`], pero `totals.idle_ram_mb` usa la RAM real de los servicios
/// que ya están en marcha. El `id` es el mismo.
pub fn plan_measured(d: &Detection, p: &Profile, c: &Catalog) -> Plan {
    plan::plan(d, p, c, &measured_ram(d))
}

/// RAM (MB) de cada servicio de Homebrew en marcha, con sus procesos hijos.
fn measured_ram(d: &Detection) -> BTreeMap<String, u32> {
    let pids: Vec<i64> = d
        .services
        .iter()
        .filter(|s| s.running)
        .filter_map(|s| s.pid)
        .collect();
    if pids.is_empty() {
        return BTreeMap::new();
    }
    let usage = monitor::usage_for(&pids).unwrap_or_default();
    d.services
        .iter()
        .filter_map(|service| {
            let pid = service.pid?;
            let used = usage.iter().find(|u| u.pid == pid)?;
            Some((
                service.name.clone(),
                u32::try_from(used.memory_kb / 1024).unwrap_or(u32::MAX),
            ))
        })
        .collect()
}

/// Con un HOME sin definir no se puede ejecutar nada: todos los pasos fallan
/// con ese motivo.
fn failed_report(plan: &Plan, error: &str) -> ApplyReport {
    ApplyReport {
        failed: plan
            .steps
            .iter()
            .map(|s| (s.key.clone(), error.to_string()))
            .collect(),
        ..ApplyReport::default()
    }
}

/// Ejecuta los pasos obligatorios y los opcionales de `accepted` (cualquier
/// otra clave se ignora y queda en el log). Toma el cerrojo de Homebrew en cada
/// paso de brew y avisa con `stamp::touch` al terminar.
pub fn apply(
    plan: &Plan,
    accepted: &BTreeSet<String>,
    ctx: &ApplyContext,
    cancel: &AtomicBool,
    on_event: &(dyn Fn(SetupProgress) + Sync),
) -> ApplyReport {
    match download::Tools::system() {
        Ok(tools) => apply::run(
            plan,
            accepted,
            ctx,
            cancel,
            on_event,
            &apply::Runner {
                tools,
                brew: &apply::SystemBrew,
            },
        ),
        Err(error) => failed_report(plan, &error),
    }
}

/// Solo aplica `fresh` (el plan recalculado ahora) si su `id` es `plan_id`,
/// el que el usuario revisó. Si no, no ejecuta nada y devuelve `Stale`.
pub fn apply_if_current(
    plan_id: &str,
    fresh: Plan,
    accepted: &BTreeSet<String>,
    ctx: &ApplyContext,
    cancel: &AtomicBool,
    on_event: &(dyn Fn(SetupProgress) + Sync),
) -> ApplyOutcome {
    match download::Tools::system() {
        Ok(tools) => {
            let runner = apply::Runner {
                tools,
                brew: &apply::SystemBrew,
            };
            apply::run_if_current(plan_id, fresh, accepted, ctx, cancel, on_event, &runner)
        }
        Err(_) if fresh.id != plan_id => ApplyOutcome::Stale { plan: fresh },
        Err(error) => ApplyOutcome::Done {
            report: failed_report(&fresh, &error),
        },
    }
}

/// La usa [`crate::ops::install_component`] para `composer`: el phar propio
/// y, si falta, el selector `php` que lo ejecuta.
pub fn install_composer(on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    crate::composer::install(crate::composer::ComposerChannel::Stable, on_line)
        .map(drop)
        .map_err(|e| apply::translate(&e).map_or(e, String::from))
}

/// Descarga una versión concreta del catálogo (sección PHP de la app).
pub fn download_php(
    version: &str,
    on_event: &(dyn Fn(SetupProgress) + Sync),
) -> Result<ApplyReport, String> {
    if !cfg!(target_arch = "aarch64") {
        return Err(tr!("Los PHP de Elephant son solo para Apple Silicon.", "Elephant’s PHP builds are Apple Silicon only."));
    }
    let release = catalog(false)
        .php
        .into_iter()
        .find(|release| release.version == version)
        .ok_or_else(|| tr!("PHP {version} no está en el catálogo de Elephant.", "PHP {version} isn’t in Elephant’s catalog."))?;
    let key = plan::download_key(version);
    let single = Plan {
        id: key.clone(),
        scenario: ScenarioId::Custom,
        steps: vec![Step {
            key,
            title: tr!("Descargar PHP {version}", "Download PHP {version}"),
            command: Some(format!("curl -fL {}", release.url)),
            action: Action::DownloadPhp { release },
            group: StepGroup::Download,
            optional: false,
            after: Vec::new(),
            restarts_service: false,
            since: SETUP_VERSION,
        }],
        satisfied: Vec::new(),
        notices: Vec::new(),
        path_notice: None,
        conflicts: Vec::new(),
        totals: Totals::default(),
    };
    let no_launch = |_: bool| Ok(());
    let ctx = ApplyContext {
        shim_source: PathBuf::new(),
        launch_at_login: &no_launch,
    };
    Ok(apply(
        &single,
        &BTreeSet::new(),
        &ctx,
        &AtomicBool::new(false),
        on_event,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_use_the_documented_tags() {
        let input: ProfileInput =
            serde_json::from_str(r#"{"scenario":"laravel_services","database":"postgres"}"#)
                .unwrap();
        assert_eq!(
            input,
            ProfileInput::LaravelServices {
                database: Database::Postgres
            }
        );

        let want: PhpWant =
            serde_json::from_str(r#"{"kind":"branches","value":["8.5","8.4"]}"#).unwrap();
        assert_eq!(want, PhpWant::Branches(vec!["8.5".into(), "8.4".into()]));
        assert_eq!(
            serde_json::to_string(&PhpWant::AnyOwn).unwrap(),
            r#"{"kind":"any_own"}"#
        );
    }

    #[test]
    fn outcomes_and_actions_are_tagged_by_kind() {
        let outcome = ApplyOutcome::Done {
            report: ApplyReport::default(),
        };
        let json = serde_json::to_value(&outcome).unwrap();
        assert_eq!(json["kind"], "done");

        let action = Action::Service {
            name: "redis".into(),
            op: Operation::TurnOnWithAutostart,
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["kind"], "service");
        assert_eq!(json["op"], "turn_on_with_autostart");
    }

    #[test]
    fn the_system_profile_plans_nothing() {
        let detection = Detection {
            arch_arm64: true,
            launch_at_login: true,
            ..Detection::default()
        };
        let profile = profile_from_system(&detection);
        assert!(profile.launch_at_login);
        assert!(
            plan(&detection, &profile, &Catalog::default())
                .steps
                .is_empty()
        );
    }
}

#[cfg(test)]
mod system_tests {
    use super::*;

    /// Imprime el plan real de este Mac para cada escenario (04 §5, fase 1).
    /// Solo lee: el catálogo se consulta sin guardar la caché en disco.
    /// `cargo test -p elephant-core system_tests -- --ignored --nocapture`
    #[test]
    #[ignore = "depende del Mac en el que se ejecuta y de la red"]
    fn prints_the_real_plans() {
        let detection = detect(false);
        let (php, composer) = catalog::fetch(&download::curl_path(), &catalog::configured_source());
        let catalog = catalog::merge(php, composer, None);
        for input in [
            ProfileInput::Laravel,
            ProfileInput::LaravelServices {
                database: presets::preselected_database(&detection),
            },
            ProfileInput::Terminal,
        ] {
            let profile = resolve(&input, &detection, &catalog)
                .expect("los escenarios fijos siempre resuelven");
            let plan = plan_measured(&detection, &profile, &catalog);
            println!("\n### {input:?}");
            for step in &plan.steps {
                let optional = if step.optional { " (opcional)" } else { "" };
                println!("  paso {}: {}{optional}", step.key, step.title);
            }
            println!("  ya lo tienes: {:?}", plan.satisfied);
            println!("  avisos: {:?}", plan.notices);
            println!("  conflictos: {:?}", plan.conflicts);
            println!("  totales: {:?}", plan.totals);
        }
        let system = profile_from_system(&detection);
        assert!(plan(&detection, &system, &catalog).steps.is_empty());
    }
}
