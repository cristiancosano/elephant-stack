use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    /// Al cerrar o minimizar la ventana, la app sigue en la barra de menús y
    /// desaparece del Dock.
    pub keep_in_menu_bar: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keep_in_menu_bar: true,
        }
    }
}

fn path() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".elephant-stack/settings.json"))
}

/// Un archivo ausente o dañado no debe impedir arrancar: se usan los valores por defecto.
pub fn load() -> Settings {
    path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or("HOME no está definido.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, json + "\n").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_running_in_the_menu_bar_by_default() {
        assert!(Settings::default().keep_in_menu_bar);
        let parsed: Settings = serde_json::from_str("{}").unwrap();
        assert!(parsed.keep_in_menu_bar);
    }

    #[test]
    fn reads_a_saved_choice() {
        let parsed: Settings = serde_json::from_str(r#"{"keep_in_menu_bar": false}"#).unwrap();
        assert!(!parsed.keep_in_menu_bar);
    }
}
