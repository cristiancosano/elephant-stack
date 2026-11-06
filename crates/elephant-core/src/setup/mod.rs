//! Asistente inicial (04): detección del sistema, escenarios, plan y
//! ejecución. P01 deja los tipos y las firmas; P04 implementa la lógica.
//! Las funciones sin implementar devuelven [`crate::NOT_AVAILABLE`] o un valor
//! vacío, y la interfaz oculta lo que depende de ellas.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::NOT_AVAILABLE;
use crate::brew::{Operation, Service};
use crate::php::VersionInfo;

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
    Done { report: ApplyReport },
    /// El sistema cambió desde la revisión: hay que revisar este plan nuevo.
    Stale { plan: Plan },
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

pub fn detect(launch_at_login: bool) -> Detection {
    Detection {
        arch_arm64: cfg!(target_arch = "aarch64"),
        launch_at_login,
        ..Detection::default()
    }
}

pub fn catalog(_force_refresh: bool) -> Catalog {
    Catalog {
        offline: true,
        ..Catalog::default()
    }
}

pub fn resolve(_input: &ProfileInput, _d: &Detection, _c: &Catalog) -> Result<Profile, String> {
    Err(NOT_AVAILABLE.into())
}

/// Personalizado sin tocar nada: el plan sale vacío.
pub fn profile_from_system(d: &Detection) -> Profile {
    Profile {
        scenario: ScenarioId::Custom,
        php: PhpWant::None,
        composer: false,
        shim: false,
        services: BTreeMap::new(),
        launch_at_login: d.launch_at_login,
        default_php: None,
    }
}

pub fn scenarios(_d: &Detection, _c: &Catalog) -> Vec<ScenarioCard> {
    Vec::new()
}

/// Pura: no ejecuta nada.
pub fn plan(_d: &Detection, p: &Profile, _c: &Catalog) -> Plan {
    Plan {
        id: String::new(),
        scenario: p.scenario,
        steps: Vec::new(),
        satisfied: Vec::new(),
        notices: Vec::new(),
        conflicts: Vec::new(),
        totals: Totals::default(),
    }
}

pub fn apply(
    _plan: &Plan,
    _accepted: &BTreeSet<String>,
    _ctx: &ApplyContext,
    _cancel: &AtomicBool,
    _on_event: &(dyn Fn(SetupProgress) + Sync),
) -> ApplyReport {
    ApplyReport::default()
}

/// La usa [`crate::ops::install_component`] para `composer`.
pub fn install_composer(_on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    Err(NOT_AVAILABLE.into())
}

pub fn download_php(
    _version: &str,
    _on_event: &(dyn Fn(SetupProgress) + Sync),
) -> Result<ApplyReport, String> {
    Err(NOT_AVAILABLE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_use_the_documented_tags() {
        let input: ProfileInput =
            serde_json::from_str(r#"{"scenario":"laravel_services","database":"postgres"}"#).unwrap();
        assert_eq!(input, ProfileInput::LaravelServices { database: Database::Postgres });

        let want: PhpWant = serde_json::from_str(r#"{"kind":"branches","value":["8.5","8.4"]}"#).unwrap();
        assert_eq!(want, PhpWant::Branches(vec!["8.5".into(), "8.4".into()]));
        assert_eq!(serde_json::to_string(&PhpWant::AnyOwn).unwrap(), r#"{"kind":"any_own"}"#);
    }

    #[test]
    fn outcomes_and_actions_are_tagged_by_kind() {
        let outcome = ApplyOutcome::Done { report: ApplyReport::default() };
        let json = serde_json::to_value(&outcome).unwrap();
        assert_eq!(json["kind"], "done");

        let action = Action::Service { name: "redis".into(), op: Operation::TurnOnWithAutostart };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["kind"], "service");
        assert_eq!(json["op"], "turn_on_with_autostart");
    }

    #[test]
    fn the_system_profile_plans_nothing() {
        let detection = detect(true);
        let profile = profile_from_system(&detection);
        assert!(profile.launch_at_login);
        assert!(plan(&detection, &profile, &catalog(false)).steps.is_empty());
    }
}
