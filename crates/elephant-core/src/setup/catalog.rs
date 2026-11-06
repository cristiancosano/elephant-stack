//! Versiones que se pueden descargar (04 §4.3): releases de php-builds y la
//! versión estable de Composer. Caché de una hora en memoria y copia en disco
//! para trabajar sin red.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::download;
use super::presets;
use super::{Catalog, ComposerRelease, PhpRelease};
use crate::{fsutil, paths};

/// Repositorio por defecto de las versiones de PHP.
pub const DEFAULT_SOURCE: &str = "cristiancosano/php-builds";
const COMPOSER_VERSIONS_URL: &str = "https://getcomposer.org/versions";
const MEMORY_TTL: Duration = Duration::from_secs(60 * 60);

/// Caché en memoria de la última fuente consultada.
static MEMORY: Mutex<Option<(Instant, String, Catalog)>> = Mutex::new(None);

/// De dónde salen las versiones de PHP que se pueden descargar (Ajustes ▸
/// General). Las dos formas dan un índice con la forma de la API de releases
/// de GitHub: `[{tag_name, draft, prerelease, assets: [{name,
/// browser_download_url, size}]}]`, con releases `php-X.Y.Z`, paquetes
/// `php-X.Y.Z-macos-arm64.tar.gz` y su `.sha256` (o `SHA256SUMS`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhpSource {
    /// Un repositorio de GitHub que publica como php-builds.
    GitHub { owner: String, repo: String },
    /// La URL https de un `index.json`.
    Index(String),
}

impl Default for PhpSource {
    fn default() -> Self {
        parse_source(DEFAULT_SOURCE).expect("la fuente por defecto es válida")
    }
}

impl PhpSource {
    /// Dónde se busca el índice, por orden. Para GitHub, primero el
    /// `index.json` de la release `catalog`, que no cuenta para el límite de la
    /// API anónima (60 consultas por hora y por IP, que una VPN compartida como
    /// Cloudflare WARP agota enseguida), y la API como respaldo.
    pub fn urls(&self) -> Vec<String> {
        match self {
            Self::GitHub { owner, repo } => vec![
                format!("https://github.com/{owner}/{repo}/releases/download/catalog/index.json"),
                format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=50"),
            ],
            Self::Index(url) => vec![url.clone()],
        }
    }

    /// Identifica la fuente en las cachés.
    fn key(&self) -> String {
        match self {
            Self::GitHub { owner, repo } => format!("{owner}/{repo}"),
            Self::Index(url) => url.clone(),
        }
    }
}

fn is_github_name(name: &str, max: usize) -> bool {
    !name.is_empty()
        && name.len() <= max
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// `usuario/repo` de GitHub o la URL https de un `index.json`.
pub fn parse_source(input: &str) -> Result<PhpSource, String> {
    let input = input.trim();
    if input.starts_with("https://") {
        let valid = input.len() <= 2048
            && input.len() > "https://".len()
            && !input.chars().any(|c| c.is_whitespace() || c.is_control());
        return if valid {
            Ok(PhpSource::Index(input.to_string()))
        } else {
            Err("La URL del índice no es válida.".to_string())
        };
    }
    if input.contains("://") {
        return Err("Solo se admiten URL https.".to_string());
    }
    match input.split_once('/') {
        Some((owner, repo)) if is_github_name(owner, 39) && is_github_name(repo, 100) => {
            Ok(PhpSource::GitHub {
                owner: owner.to_string(),
                repo: repo.to_string(),
            })
        }
        _ => Err("Escribe un repositorio de GitHub (usuario/repo) o la URL https de un index.json.".to_string()),
    }
}

/// La fuente guardada en los ajustes, o la por defecto si no hay o no es válida.
pub fn configured_source() -> PhpSource {
    crate::settings::load()
        .php_source
        .as_deref()
        .and_then(|input| parse_source(input).ok())
        .unwrap_or_default()
}

/// Resultado de «Probar» una fuente antes de guardarla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SourceCheck {
    /// Versiones con paquete para macOS arm64, de la más reciente a la más antigua.
    pub versions: Vec<String>,
    /// Releases descartadas y el motivo.
    pub skipped: Vec<String>,
    /// URL de la que se ha leído el índice.
    pub url: String,
}

/// Lee y valida una fuente sin guardarla ni tocar las cachés.
pub fn check_source(input: &str) -> Result<SourceCheck, String> {
    let source = parse_source(input)?;
    let curl = download::curl_path();
    let mut last_error = String::from("No se ha podido leer el índice.");
    for url in source.urls() {
        match download::fetch_text(&curl, &url).and_then(|json| parse_releases(&json)) {
            Ok((php, skipped)) if !php.is_empty() => {
                return Ok(SourceCheck {
                    versions: php.into_iter().map(|release| release.version).collect(),
                    skipped,
                    url,
                });
            }
            Ok((_, skipped)) => {
                last_error = if skipped.is_empty() {
                    "El índice no tiene ninguna release php-X.Y.Z.".to_string()
                } else {
                    format!("Ninguna versión sirve en este Mac: {}", skipped.join("; "))
                };
            }
            Err(error) => last_error = error,
        }
    }
    // El error de curl solo (por ejemplo, el 403 de la API sin cupo) no dice
    // qué falta; se explica qué se ha buscado.
    Err(match &source {
        PhpSource::GitHub { owner, repo } => format!(
            "No se encuentra un índice de versiones en {owner}/{repo}: falta la release «catalog» con index.json, o el repositorio no existe o es privado. Detalle: {last_error}"
        ),
        PhpSource::Index(_) => format!("No se ha podido leer el índice. Detalle: {last_error}"),
    })
}

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

/// FNV-1a de 64 bits: nombre de archivo estable para la caché de cada fuente.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// La fuente por defecto conserva `catalog.json`; cada fuente propia tiene la suya.
fn cache_path(home: &Path, source: &PhpSource) -> PathBuf {
    let name = if *source == PhpSource::default() {
        "catalog.json".to_string()
    } else {
        format!("catalog-{:016x}.json", fnv1a(&source.key()))
    };
    paths::data_dir_in(home).join("cache").join(name)
}

fn load_disk(home: &Path, source: &PhpSource) -> Option<Catalog> {
    let text = std::fs::read_to_string(cache_path(home, source)).ok()?;
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

pub(crate) fn fetch(
    curl: &Path,
    source: &PhpSource,
) -> (Option<Vec<PhpRelease>>, Option<ComposerRelease>) {
    std::thread::scope(|scope| {
        let php = scope.spawn(|| {
            source.urls().into_iter().find_map(|url| {
                let json = download::fetch_text(curl, &url).ok()?;
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
    let source = configured_source();
    let key = source.key();
    if !force_refresh {
        let memory = MEMORY.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, cached_key, catalog)) = memory.as_ref()
            && at.elapsed() < MEMORY_TTL
            && *cached_key == key
        {
            return catalog.clone();
        }
    }
    let home = paths::home().ok();
    let (php, composer) = fetch(&download::curl_path(), &source);
    let fetched = php.is_some();
    let catalog = merge(
        php,
        composer,
        home.as_deref().and_then(|home| load_disk(home, &source)),
    );
    if fetched {
        if let Some(home) = &home {
            let _ = fsutil::write_json_atomic(&cache_path(home, &source), &catalog);
        }
        *MEMORY.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((Instant::now(), key, catalog.clone()));
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
mod source_tests {
    use super::*;

    #[test]
    fn github_repositories_use_the_index_first_and_the_api_as_fallback() {
        let source = parse_source(" mi-org/php.builds_2 ").unwrap();
        assert_eq!(
            source.urls(),
            [
                "https://github.com/mi-org/php.builds_2/releases/download/catalog/index.json",
                "https://api.github.com/repos/mi-org/php.builds_2/releases?per_page=50",
            ]
        );
        assert_eq!(PhpSource::default().key(), DEFAULT_SOURCE);
    }

    #[test]
    fn index_urls_must_be_https_without_spaces() {
        let url = "https://cdn.example.test/php/index.json";
        assert_eq!(parse_source(url).unwrap().urls(), [url]);
        assert!(parse_source("http://example.test/index.json").is_err());
        assert!(parse_source("file:///etc/passwd").is_err());
        assert!(parse_source("https://example.test/a b.json").is_err());
        assert!(parse_source("https://").is_err());
    }

    #[test]
    fn rejects_anything_that_is_not_a_repository() {
        for input in ["", "php-builds", "a/b/c", "../x", "a/..", "org/repo name", "org/re;po"] {
            assert!(parse_source(input).is_err(), "{input}");
        }
    }

    #[test]
    fn each_custom_source_has_its_own_cache_file() {
        let home = Path::new("/tmp/elephant-home");
        let default = cache_path(home, &PhpSource::default());
        assert!(default.ends_with("cache/catalog.json"));
        let one = cache_path(home, &parse_source("a/b").unwrap());
        let two = cache_path(home, &parse_source("a/c").unwrap());
        assert_ne!(one, two);
        assert_ne!(one, default);
    }
}

#[cfg(test)]
mod system_tests {
    /// Consulta la red sin escribir nada: `cargo test system_tests -- --ignored --nocapture`.
    #[test]
    #[ignore = "necesita red"]
    fn reads_the_real_catalog() {
        let curl = super::download::curl_path();
        let (php, composer) = super::fetch(&curl, &super::PhpSource::default());
        println!("{php:#?}\n{composer:#?}");
        assert!(php.is_some_and(|php| !php.is_empty()));
    }

    /// Lo que hace «Probar» en Ajustes, contra la red real.
    #[test]
    #[ignore = "necesita red"]
    fn checks_real_sources() {
        let ok = super::check_source(super::DEFAULT_SOURCE).expect("php-builds debe servir");
        println!("{ok:#?}");
        assert!(!ok.versions.is_empty());
        let error = super::check_source("cristiancosano/no-existe-este-repo").unwrap_err();
        println!("{error}");
    }
}
