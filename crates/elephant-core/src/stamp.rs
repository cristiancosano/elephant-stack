//! `~/.elephant-stack/state.stamp`: aviso de que algo ha cambiado fuera de la
//! app (la CLI, `elephant mcp` o una tarea larga). La app lo vigila con
//! `notify` y refresca su estado; ignora los que lleven su propio `pid`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{fsutil, paths};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Stamp {
    pub at_ms: i64,
    /// `services` | `php` | `components` | `projects`.
    pub what: String,
    pub pid: u32,
}

pub fn path() -> Option<PathBuf> {
    paths::home().ok().map(|home| path_in(&home))
}

fn path_in(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("state.stamp")
}

/// Lo llama core al terminar bien una mutación. Nunca falla hacia fuera: un
/// aviso perdido solo retrasa el refresco hasta la siguiente vigilancia.
pub fn touch(what: &str) {
    if let Ok(home) = paths::home() {
        let _ = touch_in(&home, what, std::process::id());
    }
}

pub fn read() -> Option<Stamp> {
    read_in(&paths::home().ok()?)
}

pub(crate) fn touch_in(home: &Path, what: &str, pid: u32) -> Result<(), String> {
    let stamp = Stamp {
        at_ms: crate::now_ms(),
        what: what.to_string(),
        pid,
    };
    fsutil::write_json_atomic(&path_in(home), &stamp)
}

pub(crate) fn read_in(home: &Path) -> Option<Stamp> {
    let text = fs::read_to_string(path_in(home)).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn touch_writes_what_changed_and_who_did_it() {
        let dir = TempDir::new("stamp");
        assert_eq!(read_in(dir.path()), None);
        touch_in(dir.path(), "services", 4242).unwrap();
        let stamp = read_in(dir.path()).unwrap();
        assert_eq!(stamp.what, "services");
        assert_eq!(stamp.pid, 4242);
        assert!(stamp.at_ms > 1_700_000_000_000);

        touch_in(dir.path(), "php", 7).unwrap();
        assert_eq!(read_in(dir.path()).unwrap().what, "php");
    }

    #[test]
    fn a_damaged_stamp_reads_as_none() {
        let dir = TempDir::new("stamp-bad");
        let path = path_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "basura").unwrap();
        assert_eq!(read_in(dir.path()), None);
    }
}
