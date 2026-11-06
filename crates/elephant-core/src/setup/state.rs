//! `~/.elephant-stack/setup.json`: qué escenario eligió el usuario y cuándo.
//! Si no existe, la app abre el asistente (C8).

use std::fs;
use std::path::{Path, PathBuf};

use super::SetupFile;
use crate::{fsutil, paths};

fn path_in(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("setup.json")
}

/// Un archivo dañado equivale a uno ausente, como en `settings::load`.
pub fn load() -> Option<SetupFile> {
    load_in(&paths::home().ok()?)
}

pub fn save(f: &SetupFile) -> Result<(), String> {
    save_in(&paths::home()?, f)
}

pub(crate) fn load_in(home: &Path) -> Option<SetupFile> {
    let text = fs::read_to_string(path_in(home)).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn save_in(home: &Path, f: &SetupFile) -> Result<(), String> {
    fsutil::write_json_atomic(&path_in(home), f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{Database, SETUP_VERSION, ScenarioId};
    use crate::testutil::TempDir;

    #[test]
    fn missing_or_damaged_means_not_done() {
        let home = TempDir::new("setup-state");
        assert_eq!(load_in(home.path()), None);
        let path = path_in(home.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{\"version\":").unwrap();
        assert_eq!(load_in(home.path()), None);
    }

    #[test]
    fn round_trips_the_documented_format() {
        let home = TempDir::new("setup-state-ok");
        let file = SetupFile {
            version: SETUP_VERSION,
            scenario: Some(ScenarioId::LaravelServices),
            database: Some(Database::Postgres),
            custom: None,
            completed_at: Some("2026-11-06T10:20:00Z".into()),
        };
        save_in(home.path(), &file).unwrap();
        assert_eq!(load_in(home.path()), Some(file));

        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path_in(home.path())).unwrap()).unwrap();
        assert_eq!(saved["scenario"], "laravel_services");
        assert_eq!(saved["database"], "postgres");
    }

    #[test]
    fn skipping_is_a_null_scenario() {
        let file: SetupFile = serde_json::from_str(
            r#"{"version":1,"scenario":null,"database":null,"custom":null,"completed_at":null}"#,
        )
        .unwrap();
        assert_eq!(file.scenario, None);
    }
}
