//! Composer de Elephant (06, fase 3): el `composer.phar` oficial con su
//! `.sha256sum` en `~/.elephant-stack/tools/composer/<versión>/`, una sola
//! versión a la vez, y el selector `bin/composer` (un enlace al binario de los
//! selectores) que lo ejecuta con el PHP que toca en la carpeta actual.
//!
//! Canales de `getcomposer.org/versions`: `stable` (la 2.x más reciente) y
//! `2.2` (LTS, la última que admite PHP 5.3 a 7.1).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use elephant_php::Layout;
use serde::{Deserialize, Serialize};

use crate::setup::ComposerRelease;
use crate::setup::catalog::MemoryCache;
use crate::setup::download::{self, Tools};
use crate::{fsutil, paths, selectors, stamp, tr};

pub const VERSIONS_URL: &str = "https://getcomposer.org/versions";
/// El envoltorio `#!/bin/sh` que dejaba el asistente antes de la fase 3.
pub(crate) const LEGACY_WRAPPER: &str =
    "#!/bin/sh\nexec \"$HOME/.elephant-stack/bin/php\" \"$HOME/.elephant-stack/libexec/composer.phar\" \"$@\"\n";

static MEMORY: MemoryCache<ComposerCatalog> = MemoryCache::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ComposerChannel {
    /// La versión estable más reciente.
    Stable,
    /// 2.2 LTS: para proyectos con PHP anterior a 7.2.5.
    Lts,
}

impl ComposerChannel {
    pub fn of(version: &str) -> Self {
        if version.starts_with("2.2.") { Self::Lts } else { Self::Stable }
    }
}

/// Una versión que se puede descargar.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ComposerVersion {
    pub version: String,
    pub channel: ComposerChannel,
    /// PHP mínimo: `7.2.5`.
    pub min_php: Option<String>,
    pub url: String,
    pub checksum_url: String,
}

impl ComposerVersion {
    fn release(&self) -> ComposerRelease {
        ComposerRelease { version: self.version.clone(), url: self.url.clone(), checksum_url: self.checksum_url.clone() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ComposerCatalog {
    pub stable: Option<ComposerVersion>,
    pub lts: Option<ComposerVersion>,
    /// Sin red ni copia en disco.
    pub offline: bool,
}

impl ComposerCatalog {
    pub fn get(&self, channel: ComposerChannel) -> Option<&ComposerVersion> {
        match channel {
            ComposerChannel::Stable => self.stable.as_ref(),
            ComposerChannel::Lts => self.lts.as_ref(),
        }
    }
}

/// Lo instalado.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ComposerStatus {
    /// `None` con el `composer.phar` antiguo del asistente (sin versión en la ruta).
    pub version: Option<String>,
    pub installed: bool,
    pub channel: Option<ComposerChannel>,
    pub phar: Option<String>,
    /// El PHP con el que se ejecuta fuera de cualquier proyecto.
    pub php: Option<String>,
    /// `bin/composer` es el selector de Elephant (o el envoltorio antiguo).
    pub selector: bool,
}

/// `70205` → `7.2.5`.
fn php_from_id(id: u64) -> String {
    format!("{}.{}.{}", id / 10_000, (id / 100) % 100, id % 100)
}

fn is_composer_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    (2..=3).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit()))
}

fn entry(value: &serde_json::Value, channel: ComposerChannel) -> Option<ComposerVersion> {
    let version = value["version"].as_str()?;
    if !is_composer_version(version) {
        return None;
    }
    Some(ComposerVersion {
        version: version.to_string(),
        channel,
        min_php: value["min-php"].as_u64().map(php_from_id),
        url: format!("https://getcomposer.org/download/{version}/composer.phar"),
        checksum_url: format!("https://getcomposer.org/download/{version}/composer.phar.sha256sum"),
    })
}

/// `stable[0]` y `2.2[0]` de getcomposer.org/versions.
pub(crate) fn parse_versions(json: &str) -> Option<ComposerCatalog> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let stable = value["stable"].as_array().and_then(|list| {
        list.iter().find(|v| v["lts"].as_bool() != Some(true)).and_then(|v| entry(v, ComposerChannel::Stable))
    });
    let lts = value["2.2"].get(0).and_then(|v| entry(v, ComposerChannel::Lts));
    stable.is_some().then_some(ComposerCatalog { stable, lts, offline: false })
}

fn cache_path(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("cache/composer.json")
}

/// Una hora en memoria y copia en disco para trabajar sin red.
pub fn catalog(force_refresh: bool) -> ComposerCatalog {
    if !force_refresh && let Some(catalog) = MEMORY.get("composer") {
        return catalog;
    }
    let home = paths::home().ok();
    let fetched = download::fetch_text(&download::curl_path(), VERSIONS_URL).ok().and_then(|json| parse_versions(&json));
    match fetched {
        Some(catalog) => {
            if let Some(home) = &home {
                let _ = fsutil::write_json_atomic(&cache_path(home), &catalog);
            }
            MEMORY.put("composer".into(), catalog.clone());
            catalog
        }
        None => home
            .and_then(|home| fs::read_to_string(cache_path(&home)).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(ComposerCatalog { offline: true, ..ComposerCatalog::default() }),
    }
}

fn is_legacy_wrapper(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|text| text == LEGACY_WRAPPER)
}

/// Descarga el phar de `release`, comprueba su sha256 y lo deja en
/// `tools/composer/<versión>/`; después crea el selector `bin/composer` y
/// borra las demás versiones (y el phar antiguo de `libexec/`). Si ya está
/// instalada, solo rehace el selector.
pub(crate) fn install_release(tools: &Tools, release: &ComposerRelease, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    let version = &release.version;
    if !is_composer_version(version) {
        return Err(tr!("Versión de Composer no válida: {version}", "Invalid Composer version: {version}"));
    }
    let layout = Layout::from_home(&tools.home);
    let dir = layout.composer_root().join(version);
    let phar = dir.join("composer.phar");
    if !phar.is_file() {
        on_line(&tr!("Consultando la firma de Composer {version}", "Checking the signature of Composer {version}"));
        let sums = download::fetch_text(&tools.curl, &release.checksum_url)?;
        let expected = download::expected_hash(&sums, "composer.phar")
            .ok_or_else(|| tr!("No se encuentra la firma de Composer.", "Couldn’t find Composer’s signature."))?;
        let part = tools.data().join("cache").join(format!("composer-{version}.phar.part"));
        download::dir_for(&part)?;
        let _ = fs::remove_file(&part);
        let url = &release.url;
        on_line(&tr!("Descargando {url}", "Downloading {url}"));
        download::download_file(&tools.curl, url, &part, 0, &mut |_, _| {})?;
        let placed = (|| {
            if download::sha256_file(&part)? != expected {
                return Err(download::mismatch());
            }
            fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            fs::set_permissions(&part, fs::Permissions::from_mode(0o644))
                .and_then(|()| fs::rename(&part, &phar))
                .map_err(|error| tr!("No se pudo instalar Composer: {error}", "Couldn’t install Composer: {error}"))
        })();
        if let Err(error) = placed {
            let _ = fs::remove_file(&part);
            return Err(error);
        }
    }
    link_selector(&layout)?;
    // Solo una versión: las demás y el phar del asistente antiguo sobran.
    if let Ok(entries) = fs::read_dir(layout.composer_root()) {
        for entry in entries.flatten() {
            if entry.file_name() != version.as_str() && entry.path().join("composer.phar").is_file() {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    let _ = fs::remove_file(layout.legacy_composer_phar());
    on_line(&tr!("Composer {version} instalado", "Composer {version} installed"));
    Ok(())
}

/// `bin/composer` → `php`, sustituyendo el envoltorio antiguo si estaba.
fn link_selector(layout: &Layout) -> Result<(), String> {
    let path = layout.bin_dir().join("composer");
    if is_legacy_wrapper(&path) {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    if layout.link_selector("composer")? {
        Ok(())
    } else {
        let path = path.display();
        Err(tr!(
            "Ya hay un {path} que no es de Elephant; no se ha tocado. Bórralo o renómbralo y vuelve a instalar Composer.",
            "There’s already a {path} that isn’t Elephant’s, so it was left alone. Delete or rename it and install Composer again."
        ))
    }
}

pub(crate) fn install_in(
    tools: &Tools,
    catalog: &ComposerCatalog,
    channel: ComposerChannel,
    shim_source: Option<&Path>,
    on_line: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let release = catalog.get(channel).cloned().ok_or_else(|| {
        tr!(
            "No se ha podido consultar las versiones de Composer (getcomposer.org).",
            "Couldn’t check the Composer versions (getcomposer.org)."
        )
    })?;
    selectors::check_shim(tools, shim_source)?;
    install_release(tools, &release.release(), on_line)?;
    selectors::ensure_shim(tools, shim_source)?;
    Ok(release.version)
}

/// Instala o actualiza Composer a la última versión del canal. Devuelve la versión.
pub fn install(channel: ComposerChannel, on_line: &mut dyn FnMut(&str)) -> Result<String, String> {
    let tools = Tools::system()?;
    let env = crate::services::Env::system()?;
    let _lock = crate::services::lock_named(&env, "composer")?;
    let version = install_in(&tools, &catalog(false), channel, selectors::bundled_source().as_deref(), on_line)?;
    stamp::touch("php");
    Ok(version)
}

pub(crate) fn uninstall_in(home: &Path) -> Result<(), String> {
    let layout = Layout::from_home(home);
    let root = layout.composer_root();
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|error| tr!("No se pudo borrar Composer: {error}", "Couldn’t delete Composer: {error}"))?;
    }
    let _ = fs::remove_file(layout.legacy_composer_phar());
    let path = layout.bin_dir().join("composer");
    if is_legacy_wrapper(&path) {
        let _ = fs::remove_file(&path);
    }
    layout.unlink_selector("composer")?;
    Ok(())
}

/// Borra el Composer de Elephant y su selector (nunca un `composer` ajeno).
pub fn uninstall() -> Result<(), String> {
    uninstall_in(&paths::home()?)?;
    stamp::touch("php");
    Ok(())
}

pub(crate) fn status_in(home: &Path) -> ComposerStatus {
    let layout = Layout::from_home(home);
    let Some((version, phar)) = layout.composer_phar() else {
        return ComposerStatus::default();
    };
    let path = layout.bin_dir().join("composer");
    let selector = fs::read_link(&path).is_ok_and(|t| t == Path::new(elephant_php::selectors::TARGET)) || is_legacy_wrapper(&path);
    ComposerStatus {
        channel: version.as_deref().map(ComposerChannel::of),
        version,
        installed: true,
        phar: Some(phar.display().to_string()),
        php: layout.resolve(Path::new("/")).ok().map(|php| php.id),
        selector,
    }
}

pub fn status() -> ComposerStatus {
    paths::home().map(|home| status_in(&home)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::download::tests::tools;
    use crate::testutil::TempDir;

    /// Recorte de la respuesta real del 2026-11-07.
    const VERSIONS: &str = r#"{"stable":[{"path":"/download/2.11.1/composer.phar","version":"2.11.1","min-php":70205,"lts":false},
        {"path":"/download/2.2.31/composer.phar","version":"2.2.31","min-php":50300,"lts":true}],
        "preview":[{"version":"2.11.1"}],"snapshot":[{"version":"c63c3347b7669960787b43e665f306808f154a10"}],
        "2.2":[{"path":"/download/2.2.31/composer.phar","version":"2.2.31","min-php":50300,"lts":true}]}"#;

    #[test]
    fn reads_the_stable_and_lts_channels() {
        let catalog = parse_versions(VERSIONS).unwrap();
        let stable = catalog.stable.unwrap();
        assert_eq!((stable.version.as_str(), stable.min_php.as_deref()), ("2.11.1", Some("7.2.5")));
        assert_eq!(stable.checksum_url, "https://getcomposer.org/download/2.11.1/composer.phar.sha256sum");
        let lts = catalog.lts.unwrap();
        assert_eq!((lts.version.as_str(), lts.channel, lts.min_php.as_deref()), ("2.2.31", ComposerChannel::Lts, Some("5.3.0")));
        assert!(parse_versions(r#"{"stable":[{"version":"../x"}]}"#).is_none());
        assert_eq!(ComposerChannel::of("2.2.31"), ComposerChannel::Lts);
        assert_eq!(ComposerChannel::of("2.11.1"), ComposerChannel::Stable);
    }

    fn serve_phar(fixtures: &Path, version: &str) {
        let dir = fixtures;
        fs::write(dir.join("composer.phar"), format!("<?php echo '{version}';")).unwrap();
        let hash = download::sha256_file(&dir.join("composer.phar")).unwrap();
        fs::write(dir.join("composer.phar.sha256sum"), format!("{hash}  composer.phar\n")).unwrap();
    }

    fn catalog_for(version: &str) -> ComposerCatalog {
        ComposerCatalog {
            stable: Some(ComposerVersion {
                version: version.into(),
                channel: ComposerChannel::Stable,
                min_php: None,
                url: format!("https://example.test/{version}/composer.phar"),
                checksum_url: format!("https://example.test/{version}/composer.phar.sha256sum"),
            }),
            lts: None,
            offline: false,
        }
    }

    #[test]
    fn installs_one_version_with_its_selector_and_replaces_the_old_wrapper() {
        let (home, bin, fixtures) = (TempDir::new("cmp-home"), TempDir::new("cmp-bin"), TempDir::new("cmp-fix"));
        let tools = tools(&home, bin.path(), fixtures.path());
        let data = home.path().join(".elephant-stack");
        // Lo que dejaba el asistente antes.
        fs::create_dir_all(data.join("libexec")).unwrap();
        fs::create_dir_all(data.join("bin")).unwrap();
        fs::write(data.join("libexec/composer.phar"), "viejo").unwrap();
        fs::write(data.join("bin/composer"), LEGACY_WRAPPER).unwrap();
        let shim = fixtures.path().join("selector");
        fs::write(&shim, "binario").unwrap();

        serve_phar(fixtures.path(), "2.10.0");
        install_in(&tools, &catalog_for("2.10.0"), ComposerChannel::Stable, Some(&shim), &mut |_| {}).unwrap();
        serve_phar(fixtures.path(), "2.11.1");
        let version = install_in(&tools, &catalog_for("2.11.1"), ComposerChannel::Stable, Some(&shim), &mut |_| {}).unwrap();
        assert_eq!(version, "2.11.1");

        assert_eq!(fs::read_link(data.join("bin/composer")).unwrap(), Path::new("php"));
        assert_eq!(fs::read_to_string(data.join("bin/php")).unwrap(), "binario");
        assert!(data.join("tools/composer/2.11.1/composer.phar").is_file());
        assert!(!data.join("tools/composer/2.10.0").exists());
        assert!(!data.join("libexec/composer.phar").exists());
        let status = status_in(home.path());
        assert_eq!((status.version.as_deref(), status.selector), (Some("2.11.1"), true));

        uninstall_in(home.path()).unwrap();
        assert!(!data.join("tools/composer").exists());
        assert!(fs::symlink_metadata(data.join("bin/composer")).is_err());
        assert!(data.join("bin/php").is_file());
        assert_eq!(status_in(home.path()), ComposerStatus::default());
    }

    #[test]
    fn a_bad_checksum_or_a_foreign_composer_installs_nothing_silently() {
        let (home, bin, fixtures) = (TempDir::new("cmp-bad"), TempDir::new("cmp-bad-bin"), TempDir::new("cmp-bad-fix"));
        let tools = tools(&home, bin.path(), fixtures.path());
        let shim = fixtures.path().join("selector");
        fs::write(&shim, "binario").unwrap();
        serve_phar(fixtures.path(), "2.11.1");
        fs::write(fixtures.path().join("composer.phar.sha256sum"), format!("{}  composer.phar\n", "0".repeat(64))).unwrap();
        let error = install_in(&tools, &catalog_for("2.11.1"), ComposerChannel::Stable, Some(&shim), &mut |_| {}).unwrap_err();
        assert_eq!(error, download::mismatch());
        assert!(!home.path().join(".elephant-stack/tools/composer/2.11.1").exists());

        serve_phar(fixtures.path(), "2.11.1");
        let bin_dir = home.path().join(".elephant-stack/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("composer"), "#!/bin/sh\necho mío\n").unwrap();
        let error = install_in(&tools, &catalog_for("2.11.1"), ComposerChannel::Stable, Some(&shim), &mut |_| {}).unwrap_err();
        assert!(error.contains("no es de Elephant"), "{error}");
        assert_eq!(fs::read_to_string(bin_dir.join("composer")).unwrap(), "#!/bin/sh\necho mío\n");
        assert!(install_in(&tools, &ComposerCatalog::default(), ComposerChannel::Lts, Some(&shim), &mut |_| {}).is_err());
    }
}

#[cfg(test)]
mod system_tests {
    use std::process::Command;

    use super::*;
    use crate::testutil::{TempDir, write_script};

    /// Prueba real en un HOME temporal (nunca el de verdad): descarga el
    /// Composer estable de getcomposer.org y lo ejecuta con el selector, que
    /// respeta `.php-version`. Usa en solo lectura el PHP 8.4.27 de Elephant de
    /// este Mac (un enlace a `~/.elephant-stack/php/8.4.27`) y un PHP 7.4 falso.
    /// `cargo build -p elephant-php && cargo test -p elephant-core real_composer -- --ignored --nocapture`.
    #[test]
    #[ignore = "necesita red y el PHP 8.4.27 de Elephant"]
    fn real_composer_runs_with_the_project_php() {
        let real_home = paths::home().unwrap();
        let real_php = real_home.join(".elephant-stack/php/8.4.27");
        assert!(real_php.join("php").is_file(), "hace falta {}", real_php.display());
        let shim = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/php").canonicalize().expect("compila antes elephant-php");
        let home = TempDir::new("real-composer");
        let data = home.path().join(".elephant-stack");
        fs::create_dir_all(data.join("php/7.4.0")).unwrap();
        std::os::unix::fs::symlink(&real_php, data.join("php/8.4.27")).unwrap();
        write_script(&data.join("php/7.4.0/php"), "echo \"PHP falso 7.4 con $1\"");

        let tools = Tools { home: home.path().to_path_buf(), ..Tools::system().unwrap() };
        let catalog = download::fetch_text(&tools.curl, VERSIONS_URL).ok().and_then(|json| parse_versions(&json)).unwrap();
        let version = install_in(&tools, &catalog, ComposerChannel::Stable, Some(&shim), &mut |line| println!("  {line}")).unwrap();

        let modern = home.path().join("moderno");
        let legacy = home.path().join("antiguo");
        for (dir, php) in [(&modern, "8.4"), (&legacy, "7.4")] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(".php-version"), format!("{php}\n")).unwrap();
        }
        let run = |dir: &Path| {
            let output = Command::new(data.join("bin/composer"))
                .arg("--version")
                .current_dir(dir)
                .env_clear()
                .env("HOME", home.path())
                .env("COMPOSER_HOME", home.path().join("composer-home"))
                .env("PATH", format!("{}:/usr/bin:/bin", data.join("bin").display()))
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr);
            println!("{}: {}", dir.display(), text.trim());
            assert!(output.status.success());
            text
        };
        let out = run(&modern);
        assert!(out.contains(&format!("Composer version {version}")) && out.contains("PHP version 8.4.27"), "{out}");
        let out = run(&legacy);
        assert!(out.contains("PHP falso 7.4 con") && out.contains("tools/composer"), "{out}");
    }
}
