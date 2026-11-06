//! Crear proyectos en el directorio aparcado de Herd (C23). Herd los sirve como
//! `<nombre>.<tld>` sin que Elephant toque su configuración: solo se lee
//! `valet/config.json`.
//!
//! El proyecto se prepara en una carpeta oculta junto al destino y se mueve al
//! final, así que un fallo nunca deja un proyecto a medias en el sitio servido.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use elephant_php::{Layout, PROJECT_FILE};
use serde::{Deserialize, Serialize};

use crate::{lock, paths, stamp, validate};

const HERD_CONFIG: &str = "Library/Application Support/Herd/config/valet/config.json";
/// La carpeta de los sitios enlazados de Herd: no es un directorio aparcado.
const LINKED_SUFFIX: &str = "valet/Sites";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Template {
    /// `composer create-project laravel/laravel`.
    #[default]
    Laravel,
    /// Una carpeta con `public/index.php`.
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Created {
    pub root: PathBuf,
    /// `<nombre>.<tld>` si Herd lo sirve.
    pub host: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct HerdConfig {
    #[serde(default)]
    paths: Vec<PathBuf>,
    #[serde(default)]
    tld: Option<String>,
}

fn herd_config(home: &Path) -> Option<HerdConfig> {
    let bytes = fs::read(home.join(HERD_CONFIG)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Primer `paths[i]` de Herd que no acaba en `valet/Sites` y existe.
pub fn parked_dir(home: &Path) -> Option<PathBuf> {
    herd_config(home)?
        .paths
        .into_iter()
        .find(|dir| !dir.ends_with(LINKED_SUFFIX) && dir.is_dir())
}

fn tld(home: &Path) -> Option<String> {
    let config = herd_config(home)?;
    Some(
        config
            .tld
            .filter(|tld| !tld.trim().is_empty())
            .unwrap_or_else(|| "test".into()),
    )
}

/// Crea `<aparcado>/<name>`. Toma el cerrojo de Homebrew (C15) y avisa a la
/// app con `stamp::touch("projects")`.
pub fn create(
    name: &str,
    template: Template,
    php: Option<&str>,
    on_line: &mut dyn FnMut(&str),
) -> Result<Created, String> {
    let home = paths::home()?;
    let _lock = lock::brew()?;
    let composer = composer_path(&home);
    let created = create_in(&home, name, template, php, composer.as_deref(), on_line)?;
    stamp::touch("projects");
    Ok(created)
}

/// `ELEPHANT_STACK_COMPOSER` (solo en debug) o el `composer` de Elephant.
fn composer_path(home: &Path) -> Option<PathBuf> {
    paths::tool_override("composer").or_else(|| {
        let own = paths::data_dir_in(home).join("bin/composer");
        own.is_file().then_some(own)
    })
}

pub(crate) fn create_in(
    home: &Path,
    name: &str,
    template: Template,
    php: Option<&str>,
    composer: Option<&Path>,
    on_line: &mut dyn FnMut(&str),
) -> Result<Created, String> {
    validate::name(name)?;
    let layout = Layout::from_home(home);
    if let Some(version) = php {
        check_php(&layout, version)?;
    }
    let parked = parked_dir(home).ok_or(
        "No hay ningún directorio aparcado en Herd. Añade uno con `herd park` en la carpeta donde guardas tus proyectos.",
    )?;
    let root = parked.join(name);
    if root.symlink_metadata().is_ok() {
        return Err(format!("Ya existe {}.", root.display()));
    }
    // Composer exige que el destino no exista; la versión pedida se aplica
    // desde la carpeta de trabajo, que es la que ve el selector `php`.
    let staging = parked.join(format!(".elephant-new-{name}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| format!("No se pudo crear {}: {error}", staging.display()))?;
    let result = (|| {
        if let Some(version) = php {
            fs::write(staging.join(PROJECT_FILE), format!("{}\n", version.trim()))
                .map_err(|error| error.to_string())?;
        }
        let work = staging.join(name);
        match template {
            Template::Laravel => {
                let composer = composer.ok_or(
                    "Falta el Composer de Elephant Stack: instálalo con `elephant component install composer`.",
                )?;
                on_line(&format!("composer create-project laravel/laravel {name}"));
                run_streaming(
                    Command::new(composer)
                        .args(["create-project", "laravel/laravel", name, "--no-interaction", "--prefer-dist"])
                        .current_dir(&staging)
                        .env("PATH", paths::child_path())
                        .env("COMPOSER_NO_INTERACTION", "1"),
                    on_line,
                )?;
                if !work.join("artisan").is_file() {
                    return Err("Composer terminó sin crear el proyecto de Laravel.".to_string());
                }
            }
            Template::Blank => write_blank(&work, name)?,
        }
        if let Some(version) = php {
            layout.set_project(&work, version)?;
        }
        fs::rename(&work, &root).map_err(|error| format!("No se pudo mover el proyecto a {}: {error}", root.display()))
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;

    on_line(&format!("Proyecto creado en {}", root.display()));
    Ok(Created {
        root,
        host: tld(home).map(|tld| format!("{name}.{tld}")),
    })
}

/// Por qué una carpeta usa la versión de PHP que usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PhpReason {
    /// Un `.php-version` en la carpeta o en una superior.
    ProjectFile,
    /// El ajuste global de Elephant Stack.
    Global,
    /// No hay nada elegido: la más reciente.
    Newest,
}

/// La versión de PHP que el selector `php` usaría en `dir`, con el motivo.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectPhp {
    pub dir: PathBuf,
    /// Lo que se pide (`8.3`), si viene de un archivo o del ajuste global.
    pub requested: Option<String>,
    pub reason: PhpReason,
    /// El `.php-version` que manda, si lo hay.
    pub file: Option<PathBuf>,
    pub version: Option<crate::php::VersionInfo>,
    /// Si lo pedido no está instalado.
    pub error: Option<String>,
}

/// Lo mismo que hace el selector `php` al ejecutarse en `dir`, sin ejecutarlo.
pub fn php_for_dir(home: &Path, dir: &Path) -> ProjectPhp {
    let layout = Layout::from_home(home);
    let project = dir.ancestors().find_map(|ancestor| {
        let file = ancestor.join(PROJECT_FILE);
        let text = fs::read_to_string(&file).ok()?;
        let version = text.lines().map(str::trim).find(|line| !line.is_empty())?.to_string();
        Some((file, version))
    });
    let global = layout.global();
    let (reason, requested, file) = match (project, global) {
        (Some((file, version)), _) => (PhpReason::ProjectFile, Some(version), Some(file)),
        (None, Some(global)) => (PhpReason::Global, Some(global), None),
        (None, None) => (PhpReason::Newest, None, None),
    };
    let (version, error) = match layout.resolve(dir) {
        Ok(version) => (Some(version.into()), None),
        Err(error) => (None, Some(error)),
    };
    ProjectPhp { dir: dir.to_path_buf(), requested, reason, file, version, error }
}

fn check_php(layout: &Layout, version: &str) -> Result<(), String> {
    let available = layout.discover();
    let wanted = version.trim();
    let found = available.iter().any(|v| {
        v.key() == wanted || v.id == wanted || v.id.starts_with(&format!("{wanted}."))
    });
    if found {
        Ok(())
    } else {
        let ids: Vec<String> = available.into_iter().map(|v| v.key()).collect();
        Err(format!(
            "PHP {wanted} no está instalado. Disponibles: {}.",
            if ids.is_empty() { "ninguna".to_string() } else { ids.join(", ") }
        ))
    }
}

fn write_blank(dir: &Path, name: &str) -> Result<(), String> {
    let public = dir.join("public");
    fs::create_dir_all(&public).map_err(|error| error.to_string())?;
    let index = format!(
        "<?php\n\n// {name}: proyecto creado por Elephant Stack.\n\necho '<h1>{name}</h1><p>PHP ' . PHP_VERSION . '</p>';\n"
    );
    fs::write(public.join("index.php"), index).map_err(|error| error.to_string())
}

/// Ejecuta sin shell y entrega cada línea de stdout y stderr.
fn run_streaming(command: &mut Command, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("No se pudo ejecutar composer: {error}"))?;
    let (sender, receiver) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child.stdout.take().map(|out| Box::new(out) as Box<dyn Read + Send>),
        child.stderr.take().map(|err| Box::new(err) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let sender = sender.clone();
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        })
    })
    .collect();
    drop(sender);
    let mut last = String::new();
    for line in receiver {
        if !line.trim().is_empty() {
            last = line.trim().to_string();
        }
        on_line(&line);
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else if last.is_empty() {
        Err(format!("composer falló ({status})."))
    } else {
        Err(format!("composer falló: {last}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, write_script};

    /// Un HOME con Herd aparcando `~/Herd` (y la carpeta de enlazados primero).
    fn home_with_herd(tld: &str) -> TempDir {
        let home = TempDir::new("project");
        let config = home.path().join(HERD_CONFIG);
        fs::create_dir_all(config.parent().unwrap().join("Sites")).unwrap();
        fs::create_dir_all(home.path().join("Herd")).unwrap();
        fs::write(
            &config,
            format!(
                r#"{{"paths":["{}","{}"],"tld":"{tld}"}}"#,
                config.parent().unwrap().join("Sites").display(),
                home.path().join("Herd").display()
            ),
        )
        .unwrap();
        home
    }

    fn with_php(home: &Path, version: &str) {
        let dir = home.join(".elephant-stack/php").join(version);
        fs::create_dir_all(&dir).unwrap();
        write_script(&dir.join("php"), &format!("echo {version}"));
    }

    #[test]
    fn parked_dir_skips_linked_sites() {
        let home = home_with_herd("test");
        assert_eq!(parked_dir(home.path()), Some(home.path().join("Herd")));
        let empty = TempDir::new("project-empty");
        assert_eq!(parked_dir(empty.path()), None);
    }

    #[test]
    fn blank_project_with_php_version() {
        let home = home_with_herd("local");
        with_php(home.path(), "8.3.14");
        let mut lines = Vec::new();
        let created = create_in(home.path(), "demo", Template::Blank, Some("8.3"), None, &mut |l| {
            lines.push(l.to_string())
        })
        .unwrap();
        assert_eq!(created.root, home.path().join("Herd/demo"));
        assert_eq!(created.host.as_deref(), Some("demo.local"));
        assert!(created.root.join("public/index.php").is_file());
        assert_eq!(fs::read_to_string(created.root.join(".php-version")).unwrap(), "8.3\n");
        assert!(!home.path().join("Herd/.elephant-new-demo").exists());
        assert!(lines.last().unwrap().contains("Proyecto creado"));
    }

    #[test]
    fn laravel_uses_composer_from_the_staging_folder() {
        let home = home_with_herd("test");
        let fake = home.path().join("composer");
        // Comprueba que la versión pedida está a la vista del selector.
        write_script(
            &fake,
            "test -f .php-version || exit 9\nmkdir -p \"$3\" && touch \"$3/artisan\" && echo \"Creating $3\"",
        );
        with_php(home.path(), "8.4.27");
        let mut lines = Vec::new();
        let created =
            create_in(home.path(), "shop", Template::Laravel, Some("8.4"), Some(&fake), &mut |l| {
                lines.push(l.to_string())
            })
            .unwrap();
        assert!(created.root.join("artisan").is_file());
        assert!(lines.iter().any(|l| l == "Creating shop"));
    }

    #[test]
    fn failures_leave_nothing_behind() {
        let home = home_with_herd("test");
        let fake = home.path().join("composer");
        write_script(&fake, "echo 'Could not find package' >&2; exit 1");
        let error = create_in(home.path(), "shop", Template::Laravel, None, Some(&fake), &mut |_| {}).unwrap_err();
        assert!(error.contains("Could not find package"), "{error}");
        assert!(!home.path().join("Herd/shop").exists());
        assert!(!home.path().join("Herd/.elephant-new-shop").exists());
    }

    #[test]
    fn rejects_bad_names_existing_projects_and_missing_tools() {
        let home = home_with_herd("test");
        assert!(create_in(home.path(), "../evil", Template::Blank, None, None, &mut |_| {}).is_err());
        fs::create_dir_all(home.path().join("Herd/taken")).unwrap();
        let taken = create_in(home.path(), "taken", Template::Blank, None, None, &mut |_| {}).unwrap_err();
        assert!(taken.contains("Ya existe"), "{taken}");
        let no_composer = create_in(home.path(), "app", Template::Laravel, None, None, &mut |_| {}).unwrap_err();
        assert!(no_composer.contains("Composer"), "{no_composer}");
        let no_php = create_in(home.path(), "app", Template::Blank, Some("7.4"), None, &mut |_| {}).unwrap_err();
        assert!(no_php.contains("7.4"), "{no_php}");
    }

    #[test]
    fn php_for_dir_explains_the_choice() {
        let home = TempDir::new("project-php");
        with_php(home.path(), "8.4.27");
        with_php(home.path(), "8.3.14");
        let project = home.path().join("Code/shop");
        fs::create_dir_all(project.join("app")).unwrap();

        let newest = php_for_dir(home.path(), &project);
        assert_eq!(newest.reason, PhpReason::Newest);
        assert_eq!(newest.version.unwrap().id, "8.4.27");

        Layout::from_home(home.path()).set_global("8.3").unwrap();
        let global = php_for_dir(home.path(), &project);
        assert_eq!((global.reason, global.requested.as_deref()), (PhpReason::Global, Some("8.3")));
        assert_eq!(global.version.unwrap().id, "8.3.14");

        fs::write(project.join(".php-version"), "7.4\n").unwrap();
        let file = php_for_dir(home.path(), &project.join("app"));
        assert_eq!(file.reason, PhpReason::ProjectFile);
        assert_eq!(file.file, Some(project.join(".php-version")));
        assert!(file.version.is_none());
        assert!(file.error.unwrap().contains("7.4"));
    }

    #[test]
    fn without_herd_explains_what_to_do() {
        let home = TempDir::new("project-noherd");
        let error = create_in(home.path(), "app", Template::Blank, None, None, &mut |_| {}).unwrap_err();
        assert!(error.contains("herd park"), "{error}");
    }
}
