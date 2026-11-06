//! `~/.elephant-stack/settings.json`. Solo la app lo escribe; la CLI y
//! `elephant mcp` lo leen en cada llamada y nunca lo modifican.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{fsutil, paths};

/// Apariencia de la ventana: la del sistema o una forzada.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AppearanceMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Editor con el que se abren los archivos de los logs y de los proyectos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Editor {
    #[default]
    Vscode,
    Phpstorm,
    Cursor,
}

/// Qué pueden hacer los agentes (CLI `elephant` y `elephant mcp`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum McpMode {
    /// Solo lecturas: las mutaciones se deniegan.
    ReadOnly,
    /// Las mutaciones que pide un agente muestran un diálogo.
    #[default]
    Confirm,
    /// Las mutaciones se ejecutan sin preguntar.
    Trusted,
}

/// Ajustes del canal de agentes. Conserva el nombre `mcp` (C26).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpSettings {
    pub mode: McpMode,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Settings {
    /// Al cerrar o minimizar la ventana, la app sigue en la barra de menús y
    /// desaparece del Dock.
    pub keep_in_menu_bar: bool,
    pub appearance: AppearanceMode,
    /// Muestra la versión de PHP por defecto junto al icono de la barra.
    pub show_php_in_menu_bar: bool,
    /// Vigila los servicios de Homebrew cada 20 s.
    pub watch_services: bool,
    pub editor: Editor,
    pub mcp: McpSettings,
    /// Fuente de las versiones de PHP descargables: `usuario/repo` de GitHub o
    /// la URL https de un `index.json`. Sin valor, la de php-builds.
    pub php_source: Option<String>,
    /// Claves que esta versión no conoce: se conservan al guardar para que una
    /// app antigua no borre lo que escribió una más nueva.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(skip))]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keep_in_menu_bar: true,
            appearance: AppearanceMode::System,
            show_php_in_menu_bar: false,
            watch_services: true,
            editor: Editor::Vscode,
            mcp: McpSettings::default(),
            php_source: None,
            extra: serde_json::Map::new(),
        }
    }
}

fn path_in(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("settings.json")
}

/// Un archivo ausente o dañado no debe impedir arrancar: se usan los valores
/// por defecto.
pub fn load() -> Settings {
    paths::home().map(|home| load_in(&home)).unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    save_in(&paths::home()?, settings)
}

pub(crate) fn load_in(home: &Path) -> Settings {
    fs::read_to_string(path_in(home))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Guardado atómico: nadie lee nunca un `settings.json` a medias.
pub(crate) fn save_in(home: &Path, settings: &Settings) -> Result<(), String> {
    fsutil::write_json_atomic(&path_in(home), settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn defaults_match_the_design() {
        let parsed: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, Settings::default());
        assert!(parsed.keep_in_menu_bar);
        assert!(parsed.watch_services);
        assert!(!parsed.show_php_in_menu_bar);
        assert_eq!(parsed.mcp.mode, McpMode::Confirm);
        assert_eq!(parsed.editor, Editor::Vscode);
    }

    #[test]
    fn reads_the_old_format() {
        let parsed: Settings = serde_json::from_str(r#"{"keep_in_menu_bar": false}"#).unwrap();
        assert!(!parsed.keep_in_menu_bar);
        assert_eq!(parsed.appearance, AppearanceMode::System);
    }

    #[test]
    fn uses_snake_case_values() {
        let parsed: Settings =
            serde_json::from_str(r#"{"appearance":"dark","editor":"phpstorm","mcp":{"mode":"read_only"}}"#)
                .unwrap();
        assert_eq!(parsed.appearance, AppearanceMode::Dark);
        assert_eq!(parsed.editor, Editor::Phpstorm);
        assert_eq!(parsed.mcp.mode, McpMode::ReadOnly);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let dir = TempDir::new("settings");
        let path = path_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"keep_in_menu_bar":false,"futuro":{"a":1}}"#).unwrap();

        let mut settings = load_in(dir.path());
        assert_eq!(settings.extra["futuro"]["a"], 1);
        settings.watch_services = false;
        save_in(dir.path(), &settings).unwrap();

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["futuro"]["a"], 1);
        assert_eq!(saved["watch_services"], false);
        assert_eq!(load_in(dir.path()), settings);
    }

    #[test]
    fn a_damaged_file_gives_the_defaults() {
        let dir = TempDir::new("settings-bad");
        let path = path_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{no es json").unwrap();
        assert_eq!(load_in(dir.path()), Settings::default());
    }
}
