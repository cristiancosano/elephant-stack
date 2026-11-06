//! Versiones que se pueden descargar (04 §4.3): releases de php-builds y la
//! versión estable de Composer. Caché de una hora en memoria y copia en disco
//! para trabajar sin red.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::download;
use super::presets;
use super::{Catalog, ComposerRelease, PhpRelease};
use crate::{fsutil, paths};

/// Índice que publica el workflow de php-builds, con la misma forma que la
/// respuesta de la API. Es un archivo de una release y no cuenta para el límite
/// de la API anónima de GitHub: 60 consultas por hora y por IP, que una VPN
/// compartida (Cloudflare WARP, por ejemplo) agota enseguida.
const INDEX_URL: &str =
    "https://github.com/cristiancosano/php-builds/releases/download/catalog/index.json";
/// Respaldo por si el índice todavía no existe o no se puede leer.
const RELEASES_URL: &str =
    "https://api.github.com/repos/cristiancosano/php-builds/releases?per_page=50";
const COMPOSER_VERSIONS_URL: &str = "https://getcomposer.org/versions";
const MEMORY_TTL: Duration = Duration::from_secs(60 * 60);

static MEMORY: Mutex<Option<(Instant, Catalog)>> = Mutex::new(None);

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Releases `php-X.Y.Z` con su paquete de macOS arm64 y un checksum. Las que
/// no lo tienen se descartan (y se cuentan en `skipped`).
pub(crate) fn parse_releases(json: &str) -> Result<(Vec<PhpRelease>, Vec<String>), String> {
    let releases: Vec<Release> = serde_json::from_str(json)
        .map_err(|error| format!("Respuesta inesperada de GitHub: {error}"))?;
    let mut php = Vec::new();
    let mut skipped = Vec::new();
    for release in releases {
        let Some(version) = release.tag_name.strip_prefix("php-") else {
            continue;
        };
        if release.draft || release.prerelease || !presets::is_version(version) {
            continue;
        }
        let name = format!("php-{version}-macos-arm64.tar.gz");
        let Some(asset) = release.assets.iter().find(|a| a.name == name) else {
            skipped.push(format!(
                "{}: sin paquete para macOS arm64",
                release.tag_name
            ));
            continue;
        };
        // Cada release trae `<paquete>.sha256`, salvo la de 8.4.27, que solo
        // tiene SHA256SUMS. Los dos se buscan por nombre de archivo.
        let checksum = release
            .assets
            .iter()
            .find(|a| a.name == format!("{name}.sha256"))
            .or_else(|| release.assets.iter().find(|a| a.name == "SHA256SUMS"));
        let Some(checksum) = checksum else {
            skipped.push(format!("{}: sin checksum", release.tag_name));
            continue;
        };
        if !is_https(&asset.browser_download_url) || !is_https(&checksum.browser_download_url) {
            skipped.push(format!("{}: URL que no es https", release.tag_name));
            continue;
        }
        php.push(PhpRelease {
            version: version.to_string(),
            url: asset.browser_download_url.clone(),
            size: asset.size,
            checksum_url: checksum.browser_download_url.clone(),
        });
    }
    php.sort_by_key(|release| std::cmp::Reverse(presets::version_key(&release.version)));
    Ok((php, skipped))
}

/// `stable[0].version` de getcomposer.org, con las URL versionadas del phar.
pub(crate) fn parse_composer(json: &str) -> Option<ComposerRelease> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let version = value["stable"].get(0)?["version"].as_str()?;
    let parts: Vec<&str> = version.split('.').collect();
    let valid = (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return None;
    }
    Some(ComposerRelease {
        version: version.to_string(),
        url: format!("https://getcomposer.org/download/{version}/composer.phar"),
        checksum_url: format!("https://getcomposer.org/download/{version}/composer.phar.sha256sum"),
    })
}

fn is_https(url: &str) -> bool {
    url.starts_with("https://")
}

fn cache_path(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("cache/catalog.json")
}

fn load_disk(home: &Path) -> Option<Catalog> {
    let text = std::fs::read_to_string(cache_path(home)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Lo descargado gana; lo que falte se completa con la copia en disco. Sin
/// red ni copia, `offline`: el plan convierte las descargas en conflictos.
pub(crate) fn merge(
    php: Option<Vec<PhpRelease>>,
    composer: Option<ComposerRelease>,
    cached: Option<Catalog>,
) -> Catalog {
    let cached = cached.unwrap_or_default();
    let offline = php.is_none() && cached.php.is_empty();
    Catalog {
        php: php.unwrap_or(cached.php),
        composer: composer.or(cached.composer),
        offline,
    }
}

pub(crate) fn fetch(curl: &Path) -> (Option<Vec<PhpRelease>>, Option<ComposerRelease>) {
    std::thread::scope(|scope| {
        let php = scope.spawn(|| {
            [INDEX_URL, RELEASES_URL].into_iter().find_map(|url| {
                let json = download::fetch_text(curl, url).ok()?;
                parse_releases(&json).ok().map(|(php, _)| php)
            })
        });
        let composer = scope.spawn(|| {
            let json = download::fetch_text(curl, COMPOSER_VERSIONS_URL).ok()?;
            parse_composer(&json)
        });
        (php.join().ok().flatten(), composer.join().ok().flatten())
    })
}

pub(crate) fn catalog(force_refresh: bool) -> Catalog {
    if !force_refresh {
        let memory = MEMORY.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, catalog)) = memory.as_ref()
            && at.elapsed() < MEMORY_TTL
        {
            return catalog.clone();
        }
    }
    let home = paths::home().ok();
    let (php, composer) = fetch(&download::curl_path());
    let fetched = php.is_some();
    let catalog = merge(php, composer, home.as_deref().and_then(load_disk));
    if fetched {
        if let Some(home) = &home {
            let _ = fsutil::write_json_atomic(&cache_path(home), &catalog);
        }
        *MEMORY.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), catalog.clone()));
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorte de la respuesta real de GitHub del 2026-11-06.
    const RELEASES: &str = r#"[
      {"tag_name":"php-8.5.12","draft":false,"prerelease":false,"assets":[
        {"name":"php-8.5.12-linux-arm64.tar.gz","size":70285991,"browser_download_url":"https://github.com/cristiancosano/php-builds/releases/download/php-8.5.12/php-8.5.12-linux-arm64.tar.gz"},
        {"name":"php-8.5.12-macos-arm64.tar.gz","size":67413472,"browser_download_url":"https://github.com/cristiancosano/php-builds/releases/download/php-8.5.12/php-8.5.12-macos-arm64.tar.gz"},
        {"name":"php-8.5.12-macos-arm64.tar.gz.sha256","size":96,"browser_download_url":"https://github.com/cristiancosano/php-builds/releases/download/php-8.5.12/php-8.5.12-macos-arm64.tar.gz.sha256"}]},
      {"tag_name":"php-8.4.27","draft":false,"prerelease":false,"assets":[
        {"name":"php-8.4.27-macos-arm64.tar.gz","size":66592142,"browser_download_url":"https://github.com/cristiancosano/php-builds/releases/download/php-8.4.27/php-8.4.27-macos-arm64.tar.gz"},
        {"name":"SHA256SUMS","size":200,"browser_download_url":"https://github.com/cristiancosano/php-builds/releases/download/php-8.4.27/SHA256SUMS"}]},
      {"tag_name":"php-8.3.36","draft":false,"prerelease":false,"assets":[
        {"name":"php-8.3.36-macos-arm64.tar.gz","size":64000000,"browser_download_url":"https://github.com/cristiancosano/php-builds/releases/download/php-8.3.36/php-8.3.36-macos-arm64.tar.gz"}]},
      {"tag_name":"php-8.6.0","draft":false,"prerelease":true,"assets":[]},
      {"tag_name":"php-8.2.x","draft":false,"prerelease":false,"assets":[]},
      {"tag_name":"tools-1","draft":false,"prerelease":false,"assets":[]}
    ]"#;

    #[test]
    fn keeps_macos_releases_with_a_checksum() {
        let (php, skipped) = parse_releases(RELEASES).unwrap();
        let versions: Vec<&str> = php.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["8.5.12", "8.4.27"]);
        assert!(
            php[0]
                .checksum_url
                .ends_with("php-8.5.12-macos-arm64.tar.gz.sha256")
        );
        assert_eq!(php[0].size, 67_413_472);
        assert_eq!(skipped, ["php-8.3.36: sin checksum"]);
    }

    #[test]
    fn falls_back_to_sha256sums_when_the_sidecar_checksum_is_missing() {
        let (php, _) = parse_releases(RELEASES).unwrap();
        let release = php.iter().find(|r| r.version == "8.4.27").unwrap();
        assert!(release.checksum_url.ends_with("/php-8.4.27/SHA256SUMS"));
    }

    #[test]
    fn rejects_non_https_urls_and_bad_json() {
        let json = r#"[{"tag_name":"php-8.5.1","assets":[
            {"name":"php-8.5.1-macos-arm64.tar.gz","size":1,"browser_download_url":"http://x/php.tgz"},
            {"name":"SHA256SUMS","size":1,"browser_download_url":"https://x/SHA256SUMS"}]}]"#;
        assert!(parse_releases(json).unwrap().0.is_empty());
        assert!(parse_releases("{\"message\":\"API rate limit exceeded\"}").is_err());
    }

    #[test]
    fn composer_uses_the_versioned_phar() {
        let json = r#"{"stable":[{"path":"/download/2.11.1/composer.phar","version":"2.11.1","min-php":70205}],
                       "preview":[{"version":"2.12.0-RC1"}]}"#;
        let composer = parse_composer(json).unwrap();
        assert_eq!(composer.version, "2.11.1");
        assert_eq!(
            composer.url,
            "https://getcomposer.org/download/2.11.1/composer.phar"
        );
        assert_eq!(
            composer.checksum_url,
            "https://getcomposer.org/download/2.11.1/composer.phar.sha256sum"
        );
        assert!(parse_composer(r#"{"stable":[{"version":"../2"}]}"#).is_none());
        assert!(parse_composer("[]").is_none());
    }

    #[test]
    fn without_network_the_disk_copy_is_used_and_without_it_offline() {
        let (php, _) = parse_releases(RELEASES).unwrap();
        let cached = Catalog {
            php: php.clone(),
            composer: None,
            offline: false,
        };
        let merged = merge(None, None, Some(cached));
        assert!(!merged.offline);
        assert_eq!(merged.php, php);

        let nothing = merge(None, None, None);
        assert!(nothing.offline && nothing.php.is_empty());

        let fresh = merge(Some(Vec::new()), None, None);
        assert!(!fresh.offline);
    }
}

#[cfg(test)]
mod system_tests {
    /// Consulta la red sin escribir nada: `cargo test system_tests -- --ignored --nocapture`.
    #[test]
    #[ignore = "necesita red"]
    fn reads_the_real_catalog() {
        let curl = super::download::curl_path();
        let (php, composer) = super::fetch(&curl);
        println!("{php:#?}\n{composer:#?}");
        assert!(php.is_some_and(|php| !php.is_empty()));
    }
}
