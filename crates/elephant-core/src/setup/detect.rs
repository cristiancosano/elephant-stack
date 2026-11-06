//! Qué hay en el Mac (04 §4.2). Solo lectura: lanza las sondas en paralelo y
//! nunca ejecuta una shell del usuario ni escribe nada.

use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use elephant_php::Layout;
use sha2::{Digest, Sha256};

use super::presets::{self, Engine};
use super::{Detection, FileState, HerdInfo, PathState, PortUse};
use crate::brew::{self, Service};
use crate::paths;
use crate::php::VersionInfo;

/// Puertos que miran los escenarios: MySQL, PostgreSQL, Redis y Mailpit.
const PORTS: [u16; 5] = [3306, 5432, 6379, 1025, 8025];
const PORT_TIMEOUT: Duration = Duration::from_millis(150);

/// Selector `php` que viaja con la app (sidecar `php-selector`), buscado junto
/// al ejecutable actual como hace la app. En debug vale el `php` que deja cargo
/// en `target/<perfil>/`.
pub(crate) fn default_shim_source() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    let dir = exe.parent()?;
    let mut candidates = vec![dir.join("php-selector")];
    if cfg!(debug_assertions) {
        candidates.push(dir.join("php"));
    }
    candidates
        .into_iter()
        .find(|path| paths::is_usable_sidecar(path))
}

pub(crate) fn detect(launch_at_login: bool) -> Detection {
    let home = paths::home().ok();
    let (formulas, services, ps, ports) = thread::scope(|scope| {
        let formulas = scope.spawn(|| {
            brew::list_formulas()
                .map(|text| {
                    text.lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        });
        let services = scope.spawn(|| brew::list_services().unwrap_or_default());
        let ps = scope.spawn(process_table);
        let ports = scope.spawn(|| PORTS.map(|port| (port, port_busy(port))));
        (
            formulas.join().unwrap_or_default(),
            services.join().unwrap_or_default(),
            ps.join().unwrap_or_default(),
            ports.join().unwrap_or(PORTS.map(|port| (port, false))),
        )
    });
    let user = std::env::var("USER").unwrap_or_default();

    let mut detection = Detection {
        arch_arm64: cfg!(target_arch = "aarch64"),
        formulas,
        ports: ports
            .iter()
            .map(|&(port, busy)| PortUse {
                port,
                busy,
                owner_hint: busy
                    .then(|| owner_hint(port, &services, &ps, &user))
                    .flatten(),
            })
            .collect(),
        services,
        launch_at_login,
        data_dirs: data_dirs(&brew_prefix()),
        ..Detection::default()
    };
    if let Some(home) = home {
        let layout = Layout::from_home(&home);
        detection.php_versions = layout
            .discover()
            .into_iter()
            .map(VersionInfo::from)
            .collect();
        detection.global_php = global_php(&layout);
        detection.shim = file_state(&layout.shim_path(), default_shim_source().as_deref());
        detection.composer_phar = paths::data_dir_in(&home)
            .join("libexec/composer.phar")
            .is_file();
        detection.path = path_state(&home);
        detection.herd = herd_info(&home, &ps);
    }
    detection
}

/// La global elegida, solo si resuelve a una instalación que existe.
fn global_php(layout: &Layout) -> Option<String> {
    layout.global()?;
    // La raíz no tiene `.php-version`: esto resuelve la global.
    layout
        .resolve(Path::new("/"))
        .ok()
        .map(|version| version.key())
}

fn sha256_of(path: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(path).ok()?;
    Some(Sha256::digest(&bytes).to_vec())
}

/// Sin el selector empaquetado no se puede comparar: lo instalado se da por bueno.
pub(crate) fn file_state(installed: &Path, source: Option<&Path>) -> FileState {
    if !installed.is_file() {
        return FileState::Missing;
    }
    let Some(source) = source.and_then(sha256_of) else {
        return FileState::Same;
    };
    if sha256_of(installed).as_deref() == Some(source.as_slice()) {
        FileState::Same
    } else {
        FileState::Different
    }
}

/// `connect()` en lugar de `lsof`: ve también los puertos de otros usuarios.
fn port_busy(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, PORT_TIMEOUT).is_ok()
}

/// `(usuario, comando)` de cada proceso.
fn process_table() -> Vec<(String, String)> {
    let Ok(output) = Command::new("/bin/ps")
        .args(["-axo", "user=,comm="])
        .env("LC_ALL", "C")
        .output()
    else {
        return Vec::new();
    };
    parse_ps(&String::from_utf8_lossy(&output.stdout))
}

fn parse_ps(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (user, comm) = line.split_once(char::is_whitespace)?;
            Some((user.to_string(), comm.trim().to_string()))
        })
        .collect()
}

fn port_engine(port: u16) -> Engine {
    match port {
        3306 => Engine::Mysql,
        5432 => Engine::Postgres,
        6379 => Engine::Redis,
        _ => Engine::Mailpit,
    }
}

/// De quién parece ser un puerto ocupado. Si el servicio de Homebrew
/// equivalente está en marcha, es suyo; si no, se adivina por el nombre del
/// proceso, así que el plan lo presenta con «parece».
pub(crate) fn owner_hint(
    port: u16,
    services: &[Service],
    ps: &[(String, String)],
    user: &str,
) -> Option<String> {
    let engine = port_engine(port);
    if let Some(service) = services
        .iter()
        .find(|s| s.running && presets::engine(&s.name) == engine)
    {
        return Some(format!("{} de Homebrew", presets::label(&service.name)));
    }
    let known: &[&str] = match engine {
        Engine::Mysql => &["mysqld", "mariadbd"],
        Engine::Postgres => &["postgres"],
        Engine::Redis => &["redis-server"],
        _ => &["mailpit"],
    };
    let mentions = |owner: &str, name: &str| {
        if owner == user || owner.is_empty() {
            name.to_string()
        } else {
            format!("{name} de {owner}")
        }
    };
    // Primero un proceso con el nombre del motor; después las apps que lo
    // empaquetan (DBngin y Herd), que son una suposición más débil.
    let by_name = ps.iter().find_map(|(owner, comm)| {
        let base = comm.rsplit('/').next().unwrap_or(comm);
        known
            .iter()
            .find(|name| base == **name || base.starts_with(&format!("{name}:")))
            .map(|name| mentions(owner, name))
    });
    by_name
        .or_else(|| {
            ps.iter()
                .find(|(_, comm)| comm.contains("DBngin"))
                .map(|(owner, _)| mentions(owner, "DBngin"))
        })
        .or_else(|| {
            (engine != Engine::Postgres)
                .then(|| ps.iter().find(|(_, comm)| comm.contains("Herd.app")))
                .flatten()
                .map(|(owner, _)| mentions(owner, "Herd"))
        })
}

fn brew_prefix() -> PathBuf {
    if cfg!(target_arch = "aarch64") {
        PathBuf::from("/opt/homebrew")
    } else {
        PathBuf::from("/usr/local")
    }
}

/// Datos de instalaciones anteriores que reutilizaría una fórmula nueva.
pub(crate) fn data_dirs(prefix: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(prefix.join("var")) else {
        return Vec::new();
    };
    let mut dirs: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name == "mysql" || name == "postgresql" || name.starts_with("postgresql@")
        })
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    dirs
}

fn herd_info(home: &Path, ps: &[(String, String)]) -> Option<HerdInfo> {
    let installed = [
        PathBuf::from("/Applications/Herd.app"),
        home.join("Applications/Herd.app"),
    ]
    .iter()
    .any(|path| path.is_dir());
    if !installed {
        return None;
    }
    let own_config = home.join("Library/Application Support/Herd");
    let own_config = own_config.to_string_lossy();
    let running = ps
        .iter()
        .any(|(_, comm)| comm.contains("Herd.app") || comm.contains(own_config.as_ref()));
    let config = home.join("Library/Application Support/Herd/config/valet/config.json");
    let (tld, sites) = fs::read_to_string(config)
        .ok()
        .map_or((None, None), |text| herd_config(&text));
    Some(HerdInfo {
        running,
        tld,
        sites,
    })
}

/// En el mejor caso: si el formato cambia, sale `None` y el aviso va sin cifras.
pub(crate) fn herd_config(text: &str) -> (Option<String>, Option<u32>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return (None, None);
    };
    let tld = value["tld"].as_str().map(String::from);
    let sites = value["paths"].as_array().map(|paths| {
        paths
            .iter()
            .filter_map(|path| path.as_str())
            .map(|path| count_sites(Path::new(path)))
            .sum()
    });
    (tld, sites)
}

fn count_sites(dir: &Path) -> u32 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let count = entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        // `is_dir` sigue los enlaces: los sitios enlazados también cuentan.
        .filter(|entry| entry.path().is_dir())
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Si `~/.elephant-stack/bin` está en el PATH de las shells nuevas (04 §4.2).
pub(crate) fn path_state(home: &Path) -> PathState {
    let scripts: Vec<String> = [".zprofile", ".zshrc"]
        .iter()
        .filter_map(|name| fs::read_to_string(home.join(name)).ok())
        .collect();
    path_state_of(home, &scripts)
}

/// `scripts` en el orden en que zsh los lee. Cada `export PATH="X:$PATH"`
/// pone X delante, así que gana la última línea.
pub(crate) fn path_state_of(home: &Path, scripts: &[String]) -> PathState {
    let ours = canonical(&paths::data_dir_in(home).join("bin"));
    let mut index = 0usize;
    let mut last_ours = None;
    let mut last_herd = None;
    for line in scripts.iter().flat_map(|text| text.lines()) {
        index += 1;
        let Some(first) = path_first_entry(line) else {
            continue;
        };
        if first.contains("Herd/bin") {
            last_herd = Some(index);
        }
        if canonical(&expand(&first, home)) == ours {
            last_ours = Some(index);
        }
    }
    match (last_ours, last_herd) {
        (Some(ours), Some(herd)) if ours < herd => PathState::ShadowedByHerd,
        (Some(_), _) => PathState::Active,
        (None, Some(_)) => PathState::AfterHerdMissing,
        (None, None) => PathState::Missing,
    }
}

/// Primera entrada de una asignación `export PATH=…` o `PATH=…`, sin comillas.
pub(crate) fn path_first_entry(line: &str) -> Option<String> {
    let code = line.trim();
    if code.starts_with('#') {
        return None;
    }
    let start = code
        .match_indices("PATH=")
        .find(|(at, _)| {
            let before = &code[..*at];
            before.is_empty()
                || before.ends_with("export ")
                || before.ends_with(' ')
                || before.ends_with(')')
        })
        .map(|(at, _)| at + "PATH=".len())?;
    // Antes de PATH= solo puede haber `export`, un `case` o nada.
    let prefix = code[..start - "PATH=".len()].trim_end();
    if !(prefix.is_empty() || prefix.ends_with("export") || prefix.ends_with(')')) {
        return None;
    }
    let value = shell_word(&code[start..]);
    let first = value.split(':').next().unwrap_or("").trim();
    (!first.is_empty() && !first.starts_with("$PATH") && !first.starts_with("${PATH"))
        .then(|| first.to_string())
}

/// Una palabra de shell sin comillas: termina en el primer espacio o `;` fuera
/// de comillas.
fn shell_word(text: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => out.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() || c == ';' => break,
            (None, c) => out.push(c),
        }
    }
    out
}

fn expand(entry: &str, home: &Path) -> PathBuf {
    let home_text = home.to_string_lossy();
    let expanded = if let Some(rest) = entry.strip_prefix("~/") {
        format!("{home_text}/{rest}")
    } else {
        entry
            .replace("${HOME}", &home_text)
            .replace("$HOME", &home_text)
    };
    PathBuf::from(expanded.trim_end_matches('/'))
}

/// Ruta canónica, aunque el último componente no exista todavía.
fn canonical(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => canonical(parent).join(name),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::presets::tests::svc;
    use crate::testutil::TempDir;

    const HERD_LINE: &str =
        r#"export PATH="/Users/demo/Library/Application Support/Herd/bin/":$PATH"#;

    #[test]
    fn reads_the_first_entry_of_real_path_lines() {
        assert_eq!(
            path_first_entry(HERD_LINE).as_deref(),
            Some("/Users/demo/Library/Application Support/Herd/bin/")
        );
        assert_eq!(
            path_first_entry(r#"export PATH="$HOME/.localdeck/bin:$PATH""#).as_deref(),
            Some("$HOME/.localdeck/bin")
        );
        assert_eq!(
            path_first_entry(r#"  *) export PATH="$PNPM_HOME/bin:$PATH" ;;"#).as_deref(),
            Some("$PNPM_HOME/bin")
        );
        assert_eq!(
            path_first_entry("PATH=~/bin:$PATH").as_deref(),
            Some("~/bin")
        );
        assert_eq!(
            path_first_entry(r#"export PATH="$PATH:/Users/demo/.local/bin""#),
            None
        );
        assert_eq!(path_first_entry("# export PATH=/x:$PATH"), None);
        assert_eq!(path_first_entry("export MANPATH=/x:$MANPATH"), None);
        assert_eq!(path_first_entry("echo PATH=/x"), None);
    }

    #[test]
    fn a_legacy_symlinked_line_after_herd_is_active() {
        let home = TempDir::new("path-localdeck");
        let data = home.path().join(".elephant-stack/bin");
        fs::create_dir_all(&data).unwrap();
        std::os::unix::fs::symlink(
            home.path().join(".elephant-stack"),
            home.path().join(".localdeck"),
        )
        .unwrap();
        let zshrc = format!("{HERD_LINE}\nexport PATH=\"$HOME/.localdeck/bin:$PATH\"\n");
        assert_eq!(path_state_of(home.path(), &[zshrc]), PathState::Active);
    }

    #[test]
    fn a_line_before_herd_is_shadowed() {
        let home = TempDir::new("path-shadow");
        let zshrc = format!("export PATH=\"$HOME/.elephant-stack/bin:$PATH\"\n{HERD_LINE}\n");
        assert_eq!(
            path_state_of(home.path(), &[zshrc]),
            PathState::ShadowedByHerd
        );
        // .zprofile se lee antes que .zshrc.
        let profile = "export PATH=\"$HOME/.elephant-stack/bin:$PATH\"".to_string();
        assert_eq!(
            path_state_of(home.path(), &[profile, HERD_LINE.to_string()]),
            PathState::ShadowedByHerd
        );
    }

    #[test]
    fn missing_lines() {
        let home = TempDir::new("path-missing");
        assert_eq!(path_state_of(home.path(), &[]), PathState::Missing);
        assert_eq!(
            path_state_of(home.path(), &[HERD_LINE.to_string()]),
            PathState::AfterHerdMissing
        );
        let block = "# >>> elephant-stack >>>\nexport PATH=\"$HOME/.elephant-stack/bin:$PATH\"\n# <<< elephant-stack <<<\n";
        assert_eq!(
            path_state_of(home.path(), &[block.to_string()]),
            PathState::Active
        );
    }

    #[test]
    fn compares_the_installed_selector_by_content() {
        let dir = TempDir::new("shim-state");
        let installed = dir.path().join("php");
        let source = dir.path().join("php-selector");
        assert_eq!(file_state(&installed, Some(&source)), FileState::Missing);
        fs::write(&installed, "uno").unwrap();
        fs::write(&source, "uno").unwrap();
        assert_eq!(file_state(&installed, Some(&source)), FileState::Same);
        fs::write(&source, "dos").unwrap();
        assert_eq!(file_state(&installed, Some(&source)), FileState::Different);
        assert_eq!(file_state(&installed, None), FileState::Same);
    }

    #[test]
    fn port_owner_prefers_homebrew_and_names_other_users() {
        let ps = parse_ps(
            "nobody           /Applications/Herd.app/Contents/Resources/dnsmasq-arm64\n\
             cristian-smw     /usr/local/mysql/bin/mysqld\n\
             cristian         /opt/homebrew/opt/postgresql@18/bin/postgres\n",
        );
        assert_eq!(
            owner_hint(3306, &[], &ps, "cristian").as_deref(),
            Some("mysqld de cristian-smw")
        );
        let services = [svc("postgresql@18", true, true)];
        assert_eq!(
            owner_hint(5432, &services, &ps, "cristian").as_deref(),
            Some("PostgreSQL 18 de Homebrew")
        );
        assert_eq!(
            owner_hint(5432, &[], &ps, "cristian").as_deref(),
            Some("postgres")
        );
        assert_eq!(
            owner_hint(6379, &[], &ps, "cristian").as_deref(),
            Some("Herd de nobody")
        );
        assert_eq!(owner_hint(6379, &[], &[], "cristian"), None);
    }

    #[test]
    fn herd_config_is_read_in_the_best_case() {
        let dir = TempDir::new("herd");
        let parked = dir.path().join("Herd");
        for site in ["shop", "blog", ".hidden"] {
            fs::create_dir_all(parked.join(site)).unwrap();
        }
        fs::write(parked.join("notas.txt"), "").unwrap();
        let linked = dir.path().join("Sites");
        fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(parked.join("shop"), linked.join("tienda")).unwrap();
        let text = serde_json::json!({
            "tld": "test",
            "paths": [format!("{}/", linked.display()), parked.display().to_string()]
        })
        .to_string();
        assert_eq!(herd_config(&text), (Some("test".into()), Some(3)));
        assert_eq!(herd_config("no es json"), (None, None));
        assert_eq!(
            herd_config(r#"{"tld":"local"}"#),
            (Some("local".into()), None)
        );
    }

    #[test]
    fn finds_previous_database_data() {
        let prefix = TempDir::new("brew-var");
        for name in [
            "mysql",
            "postgresql@18",
            "mysql.failed-initialization-1",
            "redis",
        ] {
            fs::create_dir_all(prefix.path().join("var").join(name)).unwrap();
        }
        let dirs = data_dirs(prefix.path());
        let names: Vec<&str> = dirs.iter().map(|d| d.rsplit('/').next().unwrap()).collect();
        assert_eq!(names, ["mysql", "postgresql@18"]);
    }
}

#[cfg(test)]
mod system_tests {
    /// Lee el sistema real sin modificarlo: `cargo test system_tests -- --ignored --nocapture`.
    #[test]
    #[ignore = "depende del Mac en el que se ejecuta"]
    fn detects_this_mac_quickly() {
        let started = std::time::Instant::now();
        let detection = super::detect(false);
        let elapsed = started.elapsed();
        println!("{detection:#?}");
        println!("detect() tardó {elapsed:?}");
        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "{elapsed:?}"
        );
    }
}
