//! Proyecto → fuentes → archivos (02 §5.1).
//!
//! | Fuente                 | Archivos                                   | Proyecto        |
//! |------------------------|--------------------------------------------|-----------------|
//! | Laravel                | `<root>/storage/logs/*.log` y `log_globs`  | el del proyecto |
//! | nginx error de Herd    | `Herd/Log/nginx-error.log`                 | `host:` de cada línea |
//! | PHP-FPM de Herd        | `Herd/Log/php-fpm.log`                     | «Sistema»       |
//! | nginx de Elephant      | `~/.elephant-stack/logs/nginx/{access,error}.log` | `h` / `host:` |
//!
//! Las rutas salen de `HOME`; nunca se escriben a mano.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use jiff::tz::TimeZone;

use crate::parse::{FileCtx, Format};
use crate::projects::{self, Config};
use elephant_i18n::tr;

use crate::util::{glob_match, path_key};
use crate::{LogSource, Project, ProjectOverride, SourceKind, SourceState, SourceStatus};

const HERD_DIR: &str = "Library/Application Support/Herd";
const HERD_LOGS: &str = "Library/Application Support/Herd/Log";
const ELEPHANT_NGINX: &str = ".elephant-stack/logs/nginx";

/// Todo lo que se sabe de los logs en un momento: proyectos, fuentes y los
/// archivos de cada una.
#[derive(Debug, Clone)]
pub(crate) struct Catalog {
    pub projects: Arc<Vec<Project>>,
    pub sources: Vec<SourceStatus>,
    /// `(índice de la fuente, ruta)`, en el orden de las fuentes.
    pub files: Vec<LogFile>,
    /// Ajustes de `projects.json` por id de proyecto.
    pub overrides: BTreeMap<String, ProjectOverride>,
}

#[derive(Debug, Clone)]
pub(crate) struct LogFile {
    pub source: usize,
    pub path: PathBuf,
    pub key: u64,
}

impl Catalog {
    pub(crate) fn load(home: &Path) -> Self {
        let config = projects::load_config(home);
        let projects = Arc::new(projects::discover_with(home, &config));
        Self::with_projects(home, projects, &config)
    }

    pub(crate) fn with_projects(home: &Path, projects: Arc<Vec<Project>>, config: &Config) -> Self {
        let sources: Vec<SourceStatus> = source_list(home, &projects, config)
            .into_iter()
            .map(|(source, base)| status(source, &base))
            .collect();
        let files = sources
            .iter()
            .enumerate()
            .flat_map(|(index, status)| {
                status.files.iter().map(move |name| {
                    let path = status.source.dir.join(name);
                    let key = path_key(&fs::canonicalize(&path).unwrap_or_else(|_| path.clone()));
                    LogFile {
                        source: index,
                        path,
                        key,
                    }
                })
            })
            .collect();
        Self {
            projects,
            sources,
            files,
            overrides: config.overrides.clone(),
        }
    }

    pub(crate) fn source(&self, file: &LogFile) -> &LogSource {
        &self.sources[file.source].source
    }

    pub(crate) fn file_by_key(&self, key: u64) -> Option<&LogFile> {
        self.files.iter().find(|file| file.key == key)
    }

    pub(crate) fn project(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// Abre el contexto de un archivo: formato, zona horaria y hora de
    /// modificación. `None` si no se puede leer.
    pub(crate) fn context(&self, file: &LogFile) -> Option<(File, FileCtx)> {
        let source = self.source(file);
        let mut handle = File::open(&file.path).ok()?;
        let metadata = handle.metadata().ok()?;
        let mtime_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let mut head = [0u8; 4096];
        let read = handle.read(&mut head).unwrap_or(0);
        let first = head[..read]
            .split(|b| *b == b'\n')
            .map(|line| line.trim_ascii())
            .find(|line| !line.is_empty())
            .unwrap_or_default();
        let format = Format::detect(source.kind, first);
        let project = source.project.as_deref().and_then(|id| self.project(id));
        let tz = match (source.kind, project) {
            (SourceKind::LaravelApp, Some(project)) => {
                TimeZone::get(&project.log_tz).unwrap_or(TimeZone::UTC)
            }
            (SourceKind::LaravelApp, None) => TimeZone::UTC,
            _ => system_tz(),
        };
        let ctx = FileCtx {
            path: file.path.clone(),
            key: file.key,
            kind: source.kind,
            format,
            project: source.project.clone(),
            root: project.map(|p| p.root.clone()),
            tz,
            projects: Arc::clone(&self.projects),
            mtime_ms,
        };
        Some((handle, ctx))
    }
}

/// nginx y PHP-FPM escriben la hora local del Mac.
fn system_tz() -> TimeZone {
    TimeZone::try_system().unwrap_or(TimeZone::UTC)
}

/// Las de Herd se leen pero nunca se vacían.
pub(crate) fn is_herd(source: &LogSource) -> bool {
    source.id.ends_with(":herd")
}

/// Cada fuente con la carpeta de la que no pueden salir sus archivos: la raíz
/// del proyecto para `storage/logs` y la propia carpeta en las demás.
fn source_list(home: &Path, projects: &[Project], config: &Config) -> Vec<(LogSource, PathBuf)> {
    let mut list = Vec::new();
    for project in projects {
        let logs = project.root.join("storage/logs");
        if project.kind == crate::ProjectKind::Laravel || logs.is_dir() {
            list.push((
                LogSource {
                    id: format!("laravel_app:{}", project.id),
                    kind: SourceKind::LaravelApp,
                    project: Some(project.id.clone()),
                    dir: logs,
                    pattern: "*.log".into(),
                },
                project.root.clone(),
            ));
        }
        let globs = config
            .overrides
            .get(&project.id)
            .map(|o| o.log_globs.as_slice())
            .unwrap_or_default();
        for (index, glob) in globs.iter().enumerate() {
            if projects::validate_glob(glob).is_err() {
                continue;
            }
            let path = Path::new(glob);
            let (Some(dir), Some(pattern)) = (path.parent(), path.file_name()) else {
                continue;
            };
            list.push((
                LogSource {
                    id: format!("laravel_app:{}:{}", project.id, index + 1),
                    kind: SourceKind::LaravelApp,
                    project: Some(project.id.clone()),
                    dir: dir.to_path_buf(),
                    pattern: pattern.to_string_lossy().into_owned(),
                },
                dir.to_path_buf(),
            ));
        }
    }
    if home.join(HERD_DIR).is_dir() {
        let dir = home.join(HERD_LOGS);
        list.push(global(
            "nginx_error:herd",
            SourceKind::NginxError,
            &dir,
            "nginx-error.log",
        ));
        list.push(global(
            "php_fpm:herd",
            SourceKind::PhpFpm,
            &dir,
            "php-fpm.log",
        ));
    }
    let nginx = home.join(ELEPHANT_NGINX);
    if nginx.is_dir() {
        list.push(global(
            "nginx_access:elephant",
            SourceKind::NginxAccess,
            &nginx,
            "access.log",
        ));
        list.push(global(
            "nginx_error:elephant",
            SourceKind::NginxError,
            &nginx,
            "error.log",
        ));
    }
    list
}

fn global(id: &str, kind: SourceKind, dir: &Path, pattern: &str) -> (LogSource, PathBuf) {
    let source = LogSource {
        id: id.into(),
        kind,
        project: None,
        dir: dir.to_path_buf(),
        pattern: pattern.into(),
    };
    (source, dir.to_path_buf())
}

fn unreadable(path: &Path) -> SourceState {
    SourceState::Unreadable {
        message: {
            let path = path.display();
            tr!(
                "Sin permiso para leer {path}. Si está en Documentos, Escritorio o Descargas, \
                 da acceso a Elephant Stack en Ajustes del Sistema ▸ Privacidad y seguridad.",
                "No permission to read {path}. If it’s in Documents, Desktop or Downloads, \
                 give Elephant Stack access in System Settings ▸ Privacy & Security."
            )
        },
    }
}

/// Si `path`, tras resolver los enlaces, queda dentro de `base`. Un proyecto
/// clonado puede traer `storage/logs/laravel.log -> ~/.ssh/id_ed25519`: leerlo
/// se lo daría a un agente y vaciarlo borraría el archivo del usuario.
fn stays_inside(path: &Path, base: &Path) -> bool {
    match (fs::canonicalize(path), fs::canonicalize(base)) {
        (Ok(target), Ok(base)) => target.starts_with(base),
        _ => false,
    }
}

/// Lista los archivos de una fuente y comprueba que se pueden leer. Los
/// archivos (y la carpeta, si es un enlace) tienen que quedar dentro de `base`.
fn status(source: LogSource, base: &Path) -> SourceStatus {
    let entries = match fs::read_dir(&source.dir) {
        Ok(entries) => entries,
        Err(error) => {
            let state = if error.kind() == ErrorKind::PermissionDenied {
                unreadable(&source.dir)
            } else {
                SourceState::Missing
            };
            return SourceStatus {
                source,
                files: Vec::new(),
                state,
            };
        }
    };
    let mut files: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|t| t.is_file() || t.is_symlink())
        })
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.') && glob_match(&source.pattern, name))
        .filter(|name| {
            let path = source.dir.join(name);
            path.is_file() && stays_inside(&path, base)
        })
        .collect();
    files.sort();
    let mut state = if files.is_empty() {
        SourceState::Missing
    } else {
        SourceState::Ok
    };
    if let Some(blocked) = files.iter().find(|name| {
        File::open(source.dir.join(name)).is_err_and(|e| e.kind() == ErrorKind::PermissionDenied)
    }) {
        state = unreadable(&source.dir.join(blocked));
        let blocked = blocked.clone();
        files.retain(|name| *name != blocked);
    }
    SourceStatus {
        source,
        files,
        state,
    }
}

/// Las fuentes de un proyecto (por id, nombre o host) más las globales de
/// nginx, que también pueden traer sus peticiones. Sin proyecto, todas.
pub(crate) fn for_project(catalog: &Catalog, needle: Option<&str>) -> Vec<SourceStatus> {
    let Some(needle) = needle else {
        return catalog.sources.clone();
    };
    let Some(project) = projects::resolve(&catalog.projects, needle) else {
        return Vec::new();
    };
    catalog
        .sources
        .iter()
        .filter(|status| match &status.source.project {
            Some(id) => *id == project.id,
            None => matches!(
                status.source.kind,
                SourceKind::NginxError | SourceKind::NginxAccess
            ),
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectOverride;
    use crate::util::testutil::TempDir;

    fn laravel(home: &Path, name: &str) -> PathBuf {
        let root = home.join("Code").join(name);
        fs::create_dir_all(root.join("storage/logs")).unwrap();
        fs::write(root.join("artisan"), "").unwrap();
        root
    }

    #[test]
    fn sources_cover_projects_herd_and_elephant() {
        let home = TempDir::new("sources");
        let home = home.path();
        let root = laravel(home, "tienda");
        fs::write(root.join("storage/logs/laravel.log"), "x\n").unwrap();
        fs::write(root.join("storage/logs/laravel-2026-11-06.log"), "x\n").unwrap();
        fs::write(root.join("storage/logs/notas.txt"), "x\n").unwrap();
        fs::create_dir_all(home.join(HERD_LOGS)).unwrap();
        fs::write(home.join(HERD_LOGS).join("nginx-error.log"), "").unwrap();
        crate::projects::add_root(home, &home.join("Code")).unwrap();

        let catalog = Catalog::load(home);
        let ids: Vec<&str> = catalog
            .sources
            .iter()
            .map(|s| s.source.id.as_str())
            .collect();
        let project = &catalog.projects[0];
        assert_eq!(
            ids,
            [
                format!("laravel_app:{}", project.id).as_str(),
                "nginx_error:herd",
                "php_fpm:herd"
            ]
        );
        assert_eq!(
            catalog.sources[0].files,
            ["laravel-2026-11-06.log", "laravel.log"]
        );
        assert_eq!(catalog.sources[0].state, SourceState::Ok);
        assert_eq!(catalog.sources[2].state, SourceState::Missing);
        assert_eq!(catalog.files.len(), 3);

        let mine = for_project(&catalog, Some("tienda"));
        assert_eq!(mine.len(), 2, "el suyo y el nginx global");
        assert!(for_project(&catalog, Some("no-existe")).is_empty());
    }

    #[test]
    fn log_globs_add_sources() {
        let home = TempDir::new("globs");
        let home = home.path();
        laravel(home, "tienda");
        let custom = home.join("custom");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join("worker-1.log"), "").unwrap();
        crate::projects::add_root(home, &home.join("Code")).unwrap();
        let id = crate::projects::discover(home)[0].id.clone();
        let glob = format!("{}/worker-*.log", custom.display());
        crate::projects::set_override(
            home,
            &id,
            ProjectOverride {
                log_globs: vec![glob],
                timezone: None,
            },
        )
        .unwrap();

        let catalog = Catalog::load(home);
        let extra = catalog
            .sources
            .iter()
            .find(|s| s.source.id == format!("laravel_app:{id}:1"))
            .unwrap();
        assert_eq!(extra.files, ["worker-1.log"]);
    }

    #[test]
    fn unreadable_folders_are_reported_without_failing() {
        use std::os::unix::fs::PermissionsExt;
        let home = TempDir::new("unreadable");
        let root = laravel(home.path(), "tienda");
        let logs = root.join("storage/logs");
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o000)).unwrap();
        crate::projects::add_root(home.path(), &home.path().join("Code")).unwrap();
        let catalog = Catalog::load(home.path());
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            catalog.sources[0].state,
            SourceState::Unreadable { .. }
        ));
    }
}
