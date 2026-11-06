//! Descubrimiento de proyectos (02 §4): la única fuente de proyectos de
//! Elephant. Lee Herd y `~/.elephant-stack/projects.json`; solo escribe este
//! último.
//!
//! Fuentes, por prioridad (al deduplicar por ruta canónica gana la primera):
//! 1. sitios de Elephant (cuando existan, `~/.elephant-stack/sites.json`);
//! 2. Herd: cada entrada directa de cada `paths[i]` de `valet/config.json`;
//! 3. raíces del usuario, con profundidad máxima 2.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use elephant_i18n::tr;

use crate::util::{fnv1a, hex16, write_atomic};
use crate::{Project, ProjectKind, ProjectOrigin, ProjectOverride};

const HERD_CONFIG: &str = "Library/Application Support/Herd/config/valet/config.json";
const LINKED_SUFFIX: &str = "config/valet/Sites";
const CONFIG_FILE: &str = ".elephant-stack/projects.json";
const SKIPPED_DIRS: [&str; 2] = ["vendor", "node_modules"];

/// `~/.elephant-stack/projects.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Config {
    pub roots: Vec<PathBuf>,
    pub overrides: BTreeMap<String, ProjectOverride>,
}

pub(crate) fn config_path(home: &Path) -> PathBuf {
    home.join(CONFIG_FILE)
}

/// Si no existe o está dañado, los valores por defecto.
pub(crate) fn load_config(home: &Path) -> Config {
    fs::read(config_path(home))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_config(home: &Path, config: &Config) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    write_atomic(&config_path(home), text.as_bytes())
}

#[derive(Debug, Deserialize)]
struct HerdConfig {
    #[serde(default)]
    paths: Vec<PathBuf>,
    #[serde(default)]
    tld: Option<String>,
}

/// Ordenados por nombre.
pub fn discover(home: &Path) -> Vec<Project> {
    discover_with(home, &load_config(home))
}

pub(crate) fn discover_with(home: &Path, config: &Config) -> Vec<Project> {
    let mut found = Found::default();
    discover_herd(home, &mut found);
    for root in &config.roots {
        discover_user_root(root, &mut found);
    }
    let layout = elephant_php::Layout::from_home(home);
    let mut projects: Vec<Project> = found
        .list
        .into_iter()
        .map(|candidate| {
            let id = hex16(fnv1a(candidate.root.as_os_str().as_encoded_bytes()));
            let log_tz = log_timezone(&candidate.root, candidate.kind, config.overrides.get(&id));
            let php_version = layout
                .resolve(&candidate.root)
                .ok()
                .map(|version| version.id);
            Project {
                id,
                name: candidate.name,
                root: candidate.root,
                hosts: candidate.hosts,
                kind: candidate.kind,
                origin: candidate.origin,
                php_version,
                log_tz,
            }
        })
        .collect();
    projects.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.root.cmp(&b.root))
    });
    projects
}

struct Candidate {
    name: String,
    root: PathBuf,
    hosts: Vec<String>,
    kind: ProjectKind,
    origin: ProjectOrigin,
}

#[derive(Default)]
struct Found {
    list: Vec<Candidate>,
    by_root: HashMap<PathBuf, usize>,
}

impl Found {
    /// Gana el primero; si se repite la ruta, solo se suman sus hosts (un sitio
    /// aparcado y su enlace en `Sites/` son el mismo proyecto).
    fn add(&mut self, candidate: Candidate) {
        if let Some(&index) = self.by_root.get(&candidate.root) {
            let existing = &mut self.list[index];
            for host in candidate.hosts {
                if !existing.hosts.contains(&host) {
                    existing.hosts.push(host);
                }
            }
            return;
        }
        self.by_root.insert(candidate.root.clone(), self.list.len());
        self.list.push(candidate);
    }
}

fn discover_herd(home: &Path, found: &mut Found) {
    let Some(config) = fs::read(home.join(HERD_CONFIG))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<HerdConfig>(&bytes).ok())
    else {
        return;
    };
    let tld = config
        .tld
        .filter(|tld| !tld.trim().is_empty())
        .unwrap_or_else(|| "test".into());
    for dir in &config.paths {
        let origin = if dir.ends_with(LINKED_SUFFIX) {
            ProjectOrigin::HerdLinked
        } else {
            ProjectOrigin::HerdParked
        };
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            // Sigue los enlaces: los sitios enlazados son symlinks en Sites/.
            let Ok(root) = fs::canonicalize(entry.path()) else {
                continue;
            };
            if !root.is_dir() {
                continue;
            }
            let kind = classify(&root).unwrap_or(ProjectKind::Other);
            found.add(Candidate {
                hosts: vec![format!("{}.{tld}", name.to_lowercase())],
                name,
                root,
                kind,
                origin,
            });
        }
    }
}

fn discover_user_root(root: &Path, found: &mut Found) {
    let Ok(root) = fs::canonicalize(root) else {
        return;
    };
    if let Some(kind) = classify(&root) {
        found.add(user_candidate(&root, kind));
        return;
    }
    scan_children(&root, 2, found);
}

/// Busca proyectos debajo de `dir` sin seguir enlaces y sin entrar en uno ya
/// encontrado, en `vendor`, `node_modules` ni en las carpetas ocultas.
fn scan_children(dir: &Path, depth: u32, found: &mut Found) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIPPED_DIRS.contains(&name.as_ref()) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        match classify(&path) {
            Some(kind) => found.add(user_candidate(&path, kind)),
            None => scan_children(&path, depth - 1, found),
        }
    }
}

fn user_candidate(root: &Path, kind: ProjectKind) -> Candidate {
    Candidate {
        name: root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string()),
        root: root.to_path_buf(),
        hosts: Vec::new(),
        kind,
        origin: ProjectOrigin::User,
    }
}

/// Solo con `metadata`, sin leer contenido.
fn classify(root: &Path) -> Option<ProjectKind> {
    if root.join("artisan").is_file() {
        return Some(ProjectKind::Laravel);
    }
    let php = root.join("composer.json").is_file()
        || root.join("public/index.php").is_file()
        || root.join(elephant_php::PROJECT_FILE).is_file();
    php.then_some(ProjectKind::Php)
}

/// Zona horaria de los logs de Laravel (02 §5.2): el ajuste del usuario, el
/// literal de `config/app.php`, `APP_TIMEZONE` en `.env`, el valor por defecto
/// de `env('APP_TIMEZONE', 'X')` y, si no hay nada, UTC.
fn log_timezone(root: &Path, kind: ProjectKind, overrides: Option<&ProjectOverride>) -> String {
    if let Some(tz) = overrides
        .and_then(|o| o.timezone.clone())
        .filter(|tz| !tz.trim().is_empty())
    {
        return tz;
    }
    if kind == ProjectKind::Other {
        return "UTC".into();
    }
    let app = fs::read_to_string(root.join("config/app.php")).ok();
    let setting = app.as_deref().and_then(app_timezone_setting);
    match setting {
        Some(AppTimezone::Literal(tz)) => tz,
        Some(AppTimezone::Env(default)) => env_timezone(root)
            .or(default)
            .unwrap_or_else(|| "UTC".into()),
        None => env_timezone(root).unwrap_or_else(|| "UTC".into()),
    }
}

#[derive(Debug, PartialEq)]
enum AppTimezone {
    Literal(String),
    /// `env('APP_TIMEZONE', 'X')`, con su valor por defecto si lo tiene.
    Env(Option<String>),
}

fn app_timezone_setting(php: &str) -> Option<AppTimezone> {
    for quote in ['\'', '"'] {
        let key = format!("{quote}timezone{quote}");
        let Some(at) = php.find(&key) else { continue };
        let rest = php[at + key.len()..].trim_start();
        let rest = rest.strip_prefix("=>")?.trim_start();
        if let Some(literal) = quoted(rest) {
            return Some(AppTimezone::Literal(literal));
        }
        let args = rest.strip_prefix("env(")?;
        let args = &args[..args.find(')')?];
        let default = args
            .split_once(',')
            .and_then(|(_, value)| quoted(value.trim()));
        return Some(AppTimezone::Env(default));
    }
    None
}

fn quoted(text: &str) -> Option<String> {
    let quote = text.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let end = text[1..].find(quote)?;
    let value = text[1..=end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Solo se lee la línea `APP_TIMEZONE`; nada más del `.env` se guarda ni se
/// expone.
fn env_timezone(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join(".env")).ok()?;
    text.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("APP_TIMEZONE")?
            .trim_start()
            .strip_prefix('=')?;
        let value = value.split('#').next().unwrap_or_default().trim();
        let value = value.trim_matches(|c| c == '"' || c == '\'').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Por id, nombre (sin distinguir mayúsculas) o host.
pub fn resolve<'a>(projects: &'a [Project], needle: &str) -> Option<&'a Project> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    projects
        .iter()
        .find(|p| p.id == needle)
        .or_else(|| {
            projects
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(needle))
        })
        .or_else(|| project_for_host(projects, needle))
}

/// Sin distinguir mayúsculas; `a.b.tienda.test` también es de `tienda.test`,
/// como hace Valet con los comodines. Gana el host más largo que encaja.
pub fn project_for_host<'a>(projects: &'a [Project], host: &str) -> Option<&'a Project> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = host.split(':').next().unwrap_or_default();
    if host.is_empty() {
        return None;
    }
    projects
        .iter()
        .flat_map(|project| project.hosts.iter().map(move |h| (project, h)))
        .filter(|(_, candidate)| {
            let candidate = candidate.to_ascii_lowercase();
            host == candidate
                || host
                    .strip_suffix(candidate.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
        .max_by_key(|(_, candidate)| candidate.len())
        .map(|(project, _)| project)
}

/// Carpetas raíz que ha añadido el usuario (`projects.json`), en el orden en
/// que se añadieron.
pub fn roots(home: &Path) -> Vec<PathBuf> {
    load_config(home).roots
}

pub fn add_root(home: &Path, root: &Path) -> Result<(), String> {
    if !root.is_absolute() {
        let root = root.display();
        return Err(tr!("La carpeta tiene que ser una ruta absoluta: {root}.", "The folder must be an absolute path: {root}."));
    }
    let root = fs::canonicalize(root).map_err(|_| {
        let root = root.display();
        tr!("No existe la carpeta {root}.", "The folder {root} doesn’t exist.")
    })?;
    if !root.is_dir() {
        let root = root.display();
        return Err(tr!("{root} no es una carpeta.", "{root} isn’t a folder."));
    }
    let mut config = load_config(home);
    if config.roots.contains(&root) {
        return Ok(());
    }
    config.roots.push(root);
    save_config(home, &config)
}

pub fn remove_root(home: &Path, root: &Path) -> Result<(), String> {
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut config = load_config(home);
    let before = config.roots.len();
    config.roots.retain(|r| r != root && r != &canonical);
    if config.roots.len() == before {
        let root = root.display();
        return Err(tr!("{root} no está en la lista de carpetas.", "{root} isn’t in the folder list."));
    }
    save_config(home, &config)
}

/// Un ajuste vacío borra el que hubiera.
pub fn set_override(home: &Path, project_id: &str, o: ProjectOverride) -> Result<(), String> {
    if project_id.len() != 16 || !project_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(tr!("Identificador de proyecto no válido: {project_id}.", "Invalid project ID: {project_id}."));
    }
    let timezone = o
        .timezone
        .map(|tz| tz.trim().to_string())
        .filter(|tz| !tz.is_empty());
    if let Some(tz) = &timezone {
        jiff::tz::TimeZone::get(tz).map_err(|_| format!("Zona horaria desconocida: {tz}."))?;
    }
    let mut log_globs = Vec::new();
    for glob in o
        .log_globs
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
    {
        validate_glob(glob)?;
        if !log_globs.iter().any(|g: &String| g == glob) {
            log_globs.push(glob.to_string());
        }
    }
    let mut config = load_config(home);
    if timezone.is_none() && log_globs.is_empty() {
        config.overrides.remove(project_id);
    } else {
        config.overrides.insert(
            project_id.to_string(),
            ProjectOverride {
                log_globs,
                timezone,
            },
        );
    }
    save_config(home, &config)
}

/// Ruta absoluta, con `*` solo en el nombre del archivo.
pub(crate) fn validate_glob(glob: &str) -> Result<(), String> {
    let path = Path::new(glob);
    let valid = path.is_absolute()
        && path.file_name().is_some()
        && path
            .parent()
            .is_some_and(|dir| !dir.to_string_lossy().contains('*'))
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
    if valid {
        Ok(())
    } else {
        Err(tr!(
            "Patrón no válido: {glob}. Usa una ruta absoluta con `*` solo en el nombre del archivo.",
            "Invalid pattern: {glob}. Use an absolute path with `*` only in the file name."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testutil::TempDir;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    fn herd_config(home: &Path, json: &str) {
        let path = home.join(HERD_CONFIG);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, json).unwrap();
    }

    #[test]
    fn herd_parked_and_linked_sites_are_found_without_duplicates() {
        let home = TempDir::new("herd");
        let home = home.path();
        let parked = home.join("Herd");
        let sites = home.join("Library/Application Support/Herd/config/valet/Sites");
        touch(&parked.join("tienda/artisan"));
        touch(&parked.join("blog/public/index.php"));
        fs::create_dir_all(parked.join("vacia")).unwrap();
        touch(&parked.join("nota.txt"));
        fs::create_dir_all(&sites).unwrap();
        touch(&home.join("Code/api/composer.json"));
        std::os::unix::fs::symlink(home.join("Code/api"), sites.join("api")).unwrap();
        // El mismo proyecto aparcado y enlazado con otro nombre.
        std::os::unix::fs::symlink(parked.join("tienda"), sites.join("shop")).unwrap();
        herd_config(
            home,
            &format!(
                r#"{{"paths":["{}/","{}/"],"tld":"local"}}"#,
                sites.display(),
                parked.display()
            ),
        );

        let projects = discover(home);
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["api", "blog", "shop", "vacia"]);
        let api = &projects[0];
        assert_eq!(api.origin, ProjectOrigin::HerdLinked);
        assert_eq!(api.hosts, ["api.local"]);
        assert_eq!(api.kind, ProjectKind::Php);
        assert_eq!(api.root, fs::canonicalize(home.join("Code/api")).unwrap());
        let shop = &projects[2];
        assert_eq!(shop.kind, ProjectKind::Laravel);
        assert_eq!(shop.hosts, ["shop.local", "tienda.local"]);
        assert_eq!(projects[3].kind, ProjectKind::Other);
        assert_eq!(projects[1].origin, ProjectOrigin::HerdParked);
        assert_eq!(api.id.len(), 16);
        assert_eq!(api.id, discover(home)[0].id, "el id es estable");
    }

    #[test]
    fn broken_herd_config_is_ignored() {
        let home = TempDir::new("herd-broken");
        herd_config(home.path(), "{ no es json");
        assert!(discover(home.path()).is_empty());
    }

    #[test]
    fn user_roots_are_scanned_two_levels_deep() {
        let home = TempDir::new("roots");
        let code = home.path().join("Code");
        touch(&code.join("uno/artisan"));
        touch(&code.join("cliente/dos/composer.json"));
        touch(&code.join("a/b/tres/artisan")); // demasiado hondo
        touch(&code.join("node_modules/x/composer.json"));
        touch(&code.join(".oculto/composer.json"));
        touch(&code.join("uno/packages/interno/composer.json")); // dentro de un proyecto
        add_root(home.path(), &code).unwrap();

        let projects = discover(home.path());
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["dos", "uno"]);
        assert!(
            projects
                .iter()
                .all(|p| p.origin == ProjectOrigin::User && p.hosts.is_empty())
        );
    }

    #[test]
    fn a_root_that_is_a_project_counts_as_one() {
        let home = TempDir::new("root-project");
        let root = home.path().join("tienda");
        touch(&root.join("artisan"));
        add_root(home.path(), &root).unwrap();
        add_root(home.path(), &root).unwrap();
        assert_eq!(load_config(home.path()).roots.len(), 1);
        assert_eq!(roots(home.path()), [fs::canonicalize(&root).unwrap()]);
        assert_eq!(discover(home.path()).len(), 1);
        remove_root(home.path(), &root).unwrap();
        assert!(discover(home.path()).is_empty());
        assert!(remove_root(home.path(), &root).is_err());
    }

    #[test]
    fn roots_must_be_absolute_existing_folders() {
        let home = TempDir::new("root-errors");
        assert!(add_root(home.path(), Path::new("relativa")).is_err());
        assert!(add_root(home.path(), &home.path().join("no-existe")).is_err());
    }

    #[test]
    fn hosts_match_case_insensitively_and_with_subdomains() {
        let project = |id: &str, host: &str| Project {
            id: id.into(),
            name: id.into(),
            root: PathBuf::from("/x"),
            hosts: vec![host.into()],
            kind: ProjectKind::Laravel,
            origin: ProjectOrigin::HerdParked,
            php_version: None,
            log_tz: "UTC".into(),
        };
        let projects = [project("a", "tienda.test"), project("b", "api.tienda.test")];
        assert_eq!(project_for_host(&projects, "Tienda.TEST").unwrap().id, "a");
        assert_eq!(
            project_for_host(&projects, "x.y.tienda.test").unwrap().id,
            "a"
        );
        assert_eq!(
            project_for_host(&projects, "v1.api.tienda.test")
                .unwrap()
                .id,
            "b"
        );
        assert_eq!(
            project_for_host(&projects, "tienda.test:443").unwrap().id,
            "a"
        );
        assert!(project_for_host(&projects, "otratienda.test").is_none());
        assert_eq!(resolve(&projects, "B").unwrap().id, "b");
        assert_eq!(resolve(&projects, "api.tienda.test").unwrap().id, "b");
        assert!(resolve(&projects, "").is_none());
    }

    #[test]
    fn timezone_follows_the_documented_order() {
        let dir = TempDir::new("tz");
        let root = dir.path();
        touch(&root.join("artisan"));
        assert_eq!(log_timezone(root, ProjectKind::Laravel, None), "UTC");

        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config/app.php"),
            "'timezone' => env('APP_TIMEZONE', 'Europe/Lisbon'),",
        )
        .unwrap();
        assert_eq!(
            log_timezone(root, ProjectKind::Laravel, None),
            "Europe/Lisbon"
        );

        fs::write(
            root.join(".env"),
            "APP_KEY=secreto\nAPP_TIMEZONE=\"America/Mexico_City\" # local\n",
        )
        .unwrap();
        assert_eq!(
            log_timezone(root, ProjectKind::Laravel, None),
            "America/Mexico_City"
        );

        fs::write(
            root.join("config/app.php"),
            "    'timezone' => 'Europe/Madrid',",
        )
        .unwrap();
        assert_eq!(
            log_timezone(root, ProjectKind::Laravel, None),
            "Europe/Madrid"
        );

        let o = ProjectOverride {
            log_globs: vec![],
            timezone: Some("Asia/Tokyo".into()),
        };
        assert_eq!(
            log_timezone(root, ProjectKind::Laravel, Some(&o)),
            "Asia/Tokyo"
        );
    }

    #[test]
    fn overrides_are_validated_and_saved() {
        let home = TempDir::new("override");
        let id = "0123456789abcdef";
        assert!(set_override(home.path(), "nope", ProjectOverride::default()).is_err());
        let bad_tz = ProjectOverride {
            log_globs: vec![],
            timezone: Some("Marte/Olympus".into()),
        };
        assert!(set_override(home.path(), id, bad_tz).is_err());
        let bad_glob = ProjectOverride {
            log_globs: vec!["/a/*/b.log".into()],
            timezone: None,
        };
        assert!(set_override(home.path(), id, bad_glob).is_err());

        let good = ProjectOverride {
            log_globs: vec!["/var/log/app/*.log".into(), "/var/log/app/*.log".into()],
            timezone: Some(" Europe/Madrid ".into()),
        };
        set_override(home.path(), id, good).unwrap();
        let saved = &load_config(home.path()).overrides[id];
        assert_eq!(saved.log_globs, ["/var/log/app/*.log"]);
        assert_eq!(saved.timezone.as_deref(), Some("Europe/Madrid"));

        set_override(home.path(), id, ProjectOverride::default()).unwrap();
        assert!(load_config(home.path()).overrides.is_empty());
    }

    #[test]
    fn damaged_config_falls_back_to_defaults() {
        let home = TempDir::new("config-broken");
        touch(&config_path(home.path()));
        fs::write(config_path(home.path()), "[").unwrap();
        assert_eq!(load_config(home.path()), Config::default());
    }
}
