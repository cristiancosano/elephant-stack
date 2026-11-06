//! Versiones de los servicios propios (06): `services.json` de la release
//! `catalog` de php-builds, que genera un job diario a partir de las fuentes
//! oficiales. Se queda con lo de esta plataforma. Una hora en memoria y copia
//! en `~/.elephant-stack/cache/services.json` para trabajar sin red.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ServiceKind;
use crate::setup::catalog::MemoryCache;
use crate::setup::{download, presets};
use crate::{fsutil, paths, tr};

pub const CATALOG_URL: &str =
    "https://github.com/cristiancosano/php-builds/releases/download/catalog/services.json";
const SCHEMA: u32 = 1;

static MEMORY: MemoryCache<ServicesCatalog> = MemoryCache::new();

/// Una versión que se puede descargar.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ServiceRelease {
    pub service: ServiceKind,
    pub version: String,
    /// Rama que comparte los datos: `8.4` para MySQL 8.4.7.
    pub branch: String,
    /// URL oficial del proyecto (no redistribuimos binarios).
    pub url: String,
    pub sha256: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: u64,
    pub lts: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ServicesCatalog {
    /// De esta plataforma, de la más reciente a la más antigua por servicio.
    pub releases: Vec<ServiceRelease>,
    /// Sin red ni copia en disco.
    pub offline: bool,
}

impl ServicesCatalog {
    pub fn find(&self, kind: ServiceKind, version: &str) -> Option<&ServiceRelease> {
        self.releases.iter().find(|r| r.service == kind && r.version == version)
    }
}

#[derive(Deserialize)]
struct RawCatalog {
    schema: u32,
    #[serde(default)]
    components: BTreeMap<String, Vec<RawRelease>>,
}

#[derive(Deserialize)]
struct RawRelease {
    version: String,
    branch: String,
    platform: String,
    url: String,
    sha256: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    lts: bool,
}

/// `macos-arm64`, `linux-x86_64`…: la plataforma de este binario, con los
/// nombres del índice.
pub fn current_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        ("linux", "x86_64") => "linux-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => "unknown",
    }
}

/// `8.4.7` o `18.1`: entre dos y cuatro números.
pub(crate) fn is_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    (2..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// La rama es el principio de la versión: `8.4` de `8.4.7`, `18` de `18.1`.
pub(crate) fn is_branch_of(branch: &str, version: &str) -> bool {
    let parts = branch.split('.').count();
    (1..=2).contains(&parts)
        && branch
            .split('.')
            .all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_digit()))
        && version.strip_prefix(branch).is_some_and(|rest| rest.starts_with('.'))
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Las entradas de `platform` de los servicios que conoce Elephant. Lo que no
/// cuadra (versión, rama, https, checksum) se descarta.
pub(crate) fn parse(json: &str, platform: &str) -> Result<ServicesCatalog, String> {
    let raw: RawCatalog = serde_json::from_str(json).map_err(|error| {
        tr!("El índice de servicios no es válido: {error}", "The services index isn’t valid: {error}")
    })?;
    if raw.schema != SCHEMA {
        let schema = raw.schema;
        return Err(tr!(
            "El índice de servicios usa un formato que esta versión de Elephant no conoce ({schema}).",
            "The services index uses a format this version of Elephant doesn’t know ({schema})."
        ));
    }
    let mut releases = Vec::new();
    for (name, list) in raw.components {
        let Ok(kind) = ServiceKind::parse(&name) else {
            continue;
        };
        releases.extend(
            list.into_iter()
                .filter(|r| r.platform == platform)
                .filter(|r| is_version(&r.version) && is_branch_of(&r.branch, &r.version))
                .filter(|r| r.url.starts_with("https://") && download::asset_name(&r.url).is_ok())
                .filter(|r| is_sha256(&r.sha256))
                .map(|r| ServiceRelease {
                    service: kind,
                    version: r.version,
                    branch: r.branch,
                    url: r.url,
                    sha256: r.sha256.to_ascii_lowercase(),
                    size: r.size,
                    lts: r.lts,
                }),
        );
    }
    releases.sort_by(|a, b| {
        a.service
            .id()
            .cmp(b.service.id())
            .then_with(|| presets::version_key(&b.version).cmp(&presets::version_key(&a.version)))
    });
    releases.dedup_by(|a, b| a.service == b.service && a.version == b.version);
    Ok(ServicesCatalog { releases, offline: false })
}

fn cache_path(home: &Path) -> PathBuf {
    paths::data_dir_in(home).join("cache").join("services.json")
}

fn load_disk(home: &Path) -> Option<ServicesCatalog> {
    let text = std::fs::read_to_string(cache_path(home)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Lo descargado o, si no hay red, la copia en disco; sin ninguna, `offline`.
pub(crate) fn catalog(force_refresh: bool) -> ServicesCatalog {
    if !force_refresh && let Some(catalog) = MEMORY.get(CATALOG_URL) {
        return catalog;
    }
    let home = paths::home().ok();
    let fetched = download::fetch_text(&download::curl_path(), CATALOG_URL)
        .and_then(|json| parse(&json, current_platform()));
    match fetched {
        Ok(catalog) => {
            if let Some(home) = &home {
                let _ = fsutil::write_json_atomic(&cache_path(home), &catalog);
            }
            MEMORY.put(CATALOG_URL.to_string(), catalog.clone());
            catalog
        }
        Err(_) => home.as_deref().and_then(load_disk).unwrap_or(ServicesCatalog {
            releases: Vec::new(),
            offline: true,
        }),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// El contrato de `services.json` (06), con entradas que hay que descartar.
    pub(crate) const INDEX: &str = r#"{"schema":1,"generated_at":"2026-11-06T06:00:00Z","components":{
      "mysql":[
        {"version":"8.4.7","branch":"8.4","platform":"macos-arm64","url":"https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.7-macos15-arm64.tar.gz","sha256":"B96E00493BC3499B9FFD7F08D65C5D64933AF0383A8287D9873B64F94C2D6009","size":167977240,"lts":true},
        {"version":"8.4.11","branch":"8.4","platform":"macos-arm64","url":"https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.11-macos15-arm64.tar.gz","sha256":"b96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009","size":167977240,"lts":true},
        {"version":"8.4.11","branch":"8.4","platform":"linux-x86_64","url":"https://cdn.mysql.com/mysql-8.4.11-linux-glibc2.28-x86_64.tar.xz","sha256":"b96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009","size":1,"lts":true},
        {"version":"26.10.0","branch":"26.10","platform":"macos-arm64","url":"https://cdn.mysql.com/Downloads/MySQL-26.10/mysql-26.10.0-macos15-arm64.tar.gz","sha256":"a96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009","size":192417435,"lts":false},
        {"version":"9.7.1","branch":"9.6","platform":"macos-arm64","url":"https://x/mysql.tar.gz","sha256":"a96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009","size":1,"lts":true},
        {"version":"9.7.2","branch":"9.7","platform":"macos-arm64","url":"http://x/mysql.tar.gz","sha256":"a96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009","size":1,"lts":true},
        {"version":"9.7.3","branch":"9.7","platform":"macos-arm64","url":"https://x/mysql.tar.gz","sha256":"nope","size":1,"lts":true}
      ],
      "postgresql":[{"version":"18.1.0","branch":"18","platform":"macos-arm64","url":"https://x/pg.tar.gz","sha256":"a96e00493bc3499b9ffd7f08d65c5d64933af0383a8287d9873b64f94c2d6009","size":1,"lts":false}],
      "mailpit":[]
    }}"#;

    #[test]
    fn keeps_valid_releases_of_this_platform_newest_first() {
        let catalog = parse(INDEX, "macos-arm64").unwrap();
        let versions: Vec<&str> = catalog.releases.iter().map(|r| r.version.as_str()).collect();
        // PostgreSQL llega en la fase 2: por ahora se ignora.
        assert_eq!(versions, ["26.10.0", "8.4.11", "8.4.7"]);
        assert!(catalog.releases.iter().all(|r| r.service == ServiceKind::Mysql));
        let first = catalog.find(ServiceKind::Mysql, "8.4.7").unwrap();
        assert_eq!(first.sha256, first.sha256.to_ascii_lowercase());
        assert!(first.lts && !catalog.releases[0].lts);
        assert_eq!(parse(INDEX, "linux-x86_64").unwrap().releases.len(), 1);
    }

    #[test]
    fn rejects_other_schemas_and_bad_json() {
        assert!(parse(r#"{"schema":2,"components":{}}"#, "macos-arm64").is_err());
        assert!(parse("<html>", "macos-arm64").is_err());
    }

    #[test]
    fn versions_and_branches() {
        assert!(is_version("8.4.7") && is_version("18.1") && !is_version("8") && !is_version("8.x.1"));
        assert!(is_branch_of("8.4", "8.4.7") && is_branch_of("18", "18.1.0"));
        assert!(!is_branch_of("8.4", "8.40.1") && !is_branch_of("8.4", "8.4") && !is_branch_of("../", "../.1"));
    }

    #[test]
    fn this_platform_has_a_name() {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(current_platform(), "macos-arm64");
        }
    }
}
