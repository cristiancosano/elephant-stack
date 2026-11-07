//! Versiones de Node de Elephant (06, fase 3): `~/.elephant-stack/node/<versión>/`
//! con la carpeta del paquete oficial (`bin/node`, `bin/npm`…). Elige la versión
//! como nvm: `.nvmrc` (o `.node-version`) del proyecto subiendo por las
//! carpetas, después la global y, si no hay nada elegido, la más reciente.

use std::fs;
use std::path::{Path, PathBuf};

use elephant_i18n::tr;

use crate::{Layout, numeric_key, read_version_file};

/// El de nvm: gana si una carpeta tiene los dos.
pub const NVMRC: &str = ".nvmrc";
/// El de nodenv, fnm, asdf y volta.
pub const NODE_VERSION_FILE: &str = ".node-version";
/// Nombre en clave de la LTS de esa versión (`krypton`) o vacío si no es LTS.
/// Lo escribe Elephant al instalar, con lo que dice `index.json`.
pub const LTS_MARKER: &str = ".elephant-lts";

/// Nombres en clave de las LTS, por versión mayor. Solo para versiones sin
/// [`LTS_MARKER`] (copiadas a mano): las que instala Elephant lo traen.
const LTS_NAMES: [(u32, &str); 11] = [
    (4, "argon"),
    (6, "boron"),
    (8, "carbon"),
    (10, "dubnium"),
    (12, "erbium"),
    (14, "fermium"),
    (16, "gallium"),
    (18, "hydrogen"),
    (20, "iron"),
    (22, "jod"),
    (24, "krypton"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeVersion {
    /// `24.11.1`, sin la `v`.
    pub id: String,
    pub dir: PathBuf,
    /// Nombre en clave de la LTS en minúsculas (`krypton`), si lo es.
    pub lts: Option<String>,
}

impl NodeVersion {
    pub fn bin(&self) -> PathBuf {
        self.dir.join("bin")
    }

    pub fn major(&self) -> u32 {
        numeric_key(&self.id).first().copied().unwrap_or(0)
    }
}

/// Lo que pide un `.nvmrc`, un `.node-version` o el ajuste global.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeSpec {
    /// `node`, `latest`, `current` o `stable`: la más reciente instalada.
    Newest,
    /// `lts/*` (cualquier LTS) o `lts/iron`.
    Lts(Option<String>),
    /// `20`, `20.11` o `20.11.1` (también con `v` o con `.x`).
    Prefix(Vec<u32>),
}

impl NodeSpec {
    pub fn matches(&self, version: &NodeVersion) -> bool {
        self.matches_id(&version.id, version.lts.as_deref())
    }

    /// Lo mismo con la versión (`24.11.1`) y su LTS: sirve también para el catálogo.
    pub fn matches_id(&self, id: &str, lts: Option<&str>) -> bool {
        match self {
            Self::Newest => true,
            Self::Lts(None) => lts.is_some(),
            Self::Lts(Some(name)) => lts.is_some_and(|lts| lts.eq_ignore_ascii_case(name)),
            Self::Prefix(wanted) => {
                let have = numeric_key(id);
                wanted.len() <= have.len() && wanted.iter().zip(&have).all(|(a, b)| a == b)
            }
        }
    }

    /// Una versión exacta (`20.11.1`).
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Prefix(parts) if parts.len() == 3)
    }
}

/// Lo que hay que escribir para usar una versión: `20.11.1` o, para no
/// fijar el parche, `20`.
pub fn parse_spec(raw: &str) -> Result<NodeSpec, String> {
    // nvm admite comentarios al final de la línea.
    let text = raw.split('#').next().unwrap_or("").trim();
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "node" | "latest" | "current" | "stable" => return Ok(NodeSpec::Newest),
        "lts" | "lts/*" => return Ok(NodeSpec::Lts(None)),
        _ => {}
    }
    if let Some(name) = lower.strip_prefix("lts/") {
        if !name.is_empty() && name.len() <= 32 && name.bytes().all(|b| b.is_ascii_lowercase()) {
            return Ok(NodeSpec::Lts(Some(name.to_string())));
        }
        return Err(not_understood(text));
    }
    let number = lower.strip_prefix('v').unwrap_or(&lower);
    let number = number.strip_suffix(".x").or_else(|| number.strip_suffix(".*")).unwrap_or(number);
    let parts: Option<Vec<u32>> = number
        .split('.')
        .map(|part| {
            (!part.is_empty() && part.len() <= 5 && part.bytes().all(|b| b.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        })
        .collect();
    match parts {
        Some(parts) if (1..=3).contains(&parts.len()) => Ok(NodeSpec::Prefix(parts)),
        _ => Err(not_understood(text)),
    }
}

fn not_understood(text: &str) -> String {
    let text: String = text.chars().take(40).collect();
    tr!(
        "Elephant Stack: no entiendo la versión de Node «{text}». Usa, por ejemplo, 24, v24.11.1, 24.11, lts/*, lts/krypton o node (la más reciente).",
        "Elephant Stack: can’t understand the Node version “{text}”. Use, for example, 24, v24.11.1, 24.11, lts/*, lts/krypton or node (the newest)."
    )
}

/// De dónde sale la versión elegida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeOrigin {
    /// El `.nvmrc` o `.node-version` de esta ruta.
    File(PathBuf),
    Global,
    /// No hay nada elegido: la más reciente instalada.
    Newest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeResolved {
    pub version: NodeVersion,
    /// Lo que se ha pedido (`20`, `lts/*`), salvo con [`NodeOrigin::Newest`].
    pub requested: Option<String>,
    pub origin: NodeOrigin,
}

/// `.nvmrc` o `.node-version` de la carpeta más cercana que tenga uno; en la
/// misma carpeta gana `.nvmrc`.
pub fn find_project_file(cwd: &Path) -> Option<(PathBuf, String)> {
    cwd.ancestors().find_map(|dir| {
        [NVMRC, NODE_VERSION_FILE].iter().find_map(|name| {
            let file = dir.join(name);
            let version = read_version_file(&file)?;
            Some((file, version))
        })
    })
}

fn lts_of(dir: &Path, id: &str) -> Option<String> {
    match fs::read_to_string(dir.join(LTS_MARKER)) {
        Ok(text) => {
            let name = text.trim().to_ascii_lowercase();
            (!name.is_empty()).then_some(name)
        }
        Err(_) => {
            let major = numeric_key(id).first().copied()?;
            LTS_NAMES.iter().find(|(m, _)| *m == major).map(|(_, name)| (*name).to_string())
        }
    }
}

/// `24.11.1`: tres números. Descarta `.tmp-…` y cualquier otra carpeta.
pub fn is_version_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()))
}

impl Layout {
    pub fn node_root(&self) -> PathBuf {
        self.data_dir.join("node")
    }

    fn node_global_file(&self) -> PathBuf {
        self.data_dir.join("global-node")
    }

    /// Versiones instaladas, de la más reciente a la más antigua.
    pub fn node_versions(&self) -> Vec<NodeVersion> {
        let Ok(entries) = fs::read_dir(self.node_root()) else {
            return Vec::new();
        };
        let mut versions: Vec<NodeVersion> = entries
            .flatten()
            .filter_map(|entry| {
                let id = entry.file_name().to_string_lossy().into_owned();
                let dir = entry.path();
                (is_version_id(&id) && dir.join("bin/node").is_file()).then(|| NodeVersion {
                    lts: lts_of(&dir, &id),
                    id,
                    dir,
                })
            })
            .collect();
        versions.sort_by_key(|version| std::cmp::Reverse(numeric_key(&version.id)));
        versions
    }

    pub fn node_global(&self) -> Option<String> {
        read_version_file(&self.node_global_file())
    }

    /// La más reciente que cumple `requested`. `Err` si no se entiende.
    pub fn find_node(&self, requested: &str) -> Result<Option<NodeVersion>, String> {
        let spec = parse_spec(requested)?;
        Ok(self.node_versions().into_iter().find(|version| spec.matches(version)))
    }

    fn node_missing(&self, requested: &str, origin: &str) -> String {
        let available: Vec<String> = self.node_versions().into_iter().map(|v| v.id).collect();
        let requested = requested.trim();
        let list = if available.is_empty() { tr!("ninguna", "none") } else { available.join(", ") };
        tr!(
            "Elephant Stack: Node {requested} no está instalado (lo pide {origin}). Instaladas: {list}. Instálalo con `elephant node install {requested}` o en la vista Node de la app.",
            "Elephant Stack: Node {requested} isn’t installed (requested by {origin}). Installed: {list}. Install it with `elephant node install {requested}` or in the app’s Node view."
        )
    }

    fn ensure_node(&self, requested: &str) -> Result<NodeVersion, String> {
        self.find_node(requested)?.ok_or_else(|| self.node_missing(requested, "Elephant Stack"))
    }

    pub fn set_node_global(&self, requested: &str) -> Result<NodeVersion, String> {
        let version = self.ensure_node(requested)?;
        fs::create_dir_all(&self.data_dir).map_err(|error| error.to_string())?;
        fs::write(self.node_global_file(), format!("{}\n", requested.trim())).map_err(|error| error.to_string())?;
        Ok(version)
    }

    /// Vuelve a «la más reciente».
    pub fn clear_node_global(&self) -> Result<(), String> {
        match fs::remove_file(self.node_global_file()) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        }
    }

    /// Escribe `.nvmrc`, o `.node-version` si la carpeta ya usa ese y no el
    /// otro. Devuelve el archivo escrito.
    pub fn set_node_project(&self, dir: &Path, requested: &str) -> Result<PathBuf, String> {
        self.ensure_node(requested)?;
        if !dir.is_dir() {
            let dir = dir.display();
            return Err(tr!("No existe la carpeta {dir}.", "The folder {dir} doesn’t exist."));
        }
        let file = if !dir.join(NVMRC).exists() && dir.join(NODE_VERSION_FILE).exists() {
            dir.join(NODE_VERSION_FILE)
        } else {
            dir.join(NVMRC)
        };
        fs::write(&file, format!("{}\n", requested.trim())).map_err(|error| error.to_string())?;
        Ok(file)
    }

    /// La versión para una carpeta. Lo que se pide y no está instalado es un
    /// error, nunca otra versión en silencio.
    pub fn resolve_node(&self, cwd: &Path) -> Result<NodeResolved, String> {
        if let Some((file, requested)) = find_project_file(cwd) {
            let version = self
                .find_node(&requested)
                .map_err(|error| format!("{error} ({})", file.display()))?
                .ok_or_else(|| self.node_missing(&requested, &file.display().to_string()))?;
            return Ok(NodeResolved { version, requested: Some(requested), origin: NodeOrigin::File(file) });
        }
        if let Some(requested) = self.node_global() {
            let origin = tr!("el ajuste global", "the global setting");
            let version = self
                .find_node(&requested)?
                .ok_or_else(|| self.node_missing(&requested, &origin))?;
            return Ok(NodeResolved { version, requested: Some(requested), origin: NodeOrigin::Global });
        }
        let version = self.node_versions().into_iter().next().ok_or_else(|| {
            tr!(
                "Elephant Stack: no hay ninguna versión de Node instalada. Instala una con `elephant node install lts` o en la vista Node de la app.",
                "Elephant Stack: no Node version is installed. Install one with `elephant node install lts` or in the app’s Node view."
            )
        })?;
        Ok(NodeResolved { version, requested: None, origin: NodeOrigin::Newest })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Sandbox;

    fn spec(text: &str) -> NodeSpec {
        parse_spec(text).unwrap()
    }

    #[test]
    fn understands_the_usual_nvmrc_formats() {
        assert_eq!(spec("20"), NodeSpec::Prefix(vec![20]));
        assert_eq!(spec("v20.11.1\n"), NodeSpec::Prefix(vec![20, 11, 1]));
        assert_eq!(spec(" 20.11 "), NodeSpec::Prefix(vec![20, 11]));
        assert_eq!(spec("20.x"), NodeSpec::Prefix(vec![20]));
        assert_eq!(spec("lts/*"), NodeSpec::Lts(None));
        assert_eq!(spec("lts/Iron"), NodeSpec::Lts(Some("iron".into())));
        assert_eq!(spec("node"), NodeSpec::Newest);
        assert_eq!(spec("22 # la de producción"), NodeSpec::Prefix(vec![22]));
        for bad in ["", "system", ">=18", "lts/-1", "20.1.2.3", "v", "iojs", "20..1", "lts/"] {
            let error = parse_spec(bad).unwrap_err();
            assert!(error.contains("no entiendo"), "{bad}: {error}");
        }
    }

    #[test]
    fn prefixes_respect_number_boundaries() {
        let v = |id: &str| NodeVersion { id: id.into(), dir: PathBuf::new(), lts: None };
        assert!(spec("20").matches(&v("20.11.1")));
        assert!(spec("20.11").matches(&v("20.11.1")));
        assert!(!spec("2").matches(&v("20.11.1")));
        assert!(!spec("20.1").matches(&v("20.11.1")));
        assert!(!spec("20.11.1").matches(&v("20.11.10")));
    }

    #[test]
    fn lts_comes_from_the_marker_or_the_table() {
        let sandbox = Sandbox::new();
        sandbox.node("24.11.1", Some("krypton"));
        sandbox.node("25.2.0", Some(""));
        sandbox.node("20.19.5", None);
        let versions = sandbox.layout.node_versions();
        let ids: Vec<(&str, Option<&str>)> = versions.iter().map(|v| (v.id.as_str(), v.lts.as_deref())).collect();
        assert_eq!(ids, [("25.2.0", None), ("24.11.1", Some("krypton")), ("20.19.5", Some("iron"))]);
        assert_eq!(sandbox.layout.find_node("lts/*").unwrap().unwrap().id, "24.11.1");
        assert_eq!(sandbox.layout.find_node("lts/iron").unwrap().unwrap().id, "20.19.5");
        assert_eq!(sandbox.layout.find_node("node").unwrap().unwrap().id, "25.2.0");
        assert!(sandbox.layout.find_node("lts/hydrogen").unwrap().is_none());
    }

    #[test]
    fn nearest_folder_wins_and_nvmrc_beats_node_version() {
        let sandbox = Sandbox::new();
        sandbox.node("24.11.1", Some("krypton"));
        sandbox.node("22.21.0", Some("jod"));
        sandbox.node("20.19.5", Some("iron"));
        let app = sandbox.project("app", None);
        fs::write(app.join(NVMRC), "v22\n").unwrap();
        fs::write(app.join(NODE_VERSION_FILE), "20\n").unwrap();
        let nested = sandbox.project("app/web/src", None);
        let resolved = sandbox.layout.resolve_node(&nested).unwrap();
        assert_eq!(resolved.version.id, "22.21.0");
        assert_eq!(resolved.origin, NodeOrigin::File(app.join(NVMRC)));

        let web = sandbox.root.join("app/web");
        fs::write(web.join(NODE_VERSION_FILE), "20.19\n").unwrap();
        assert_eq!(sandbox.layout.resolve_node(&nested).unwrap().version.id, "20.19.5");
    }

    #[test]
    fn global_then_newest_and_missing_is_an_error() {
        let sandbox = Sandbox::new();
        let cwd = sandbox.project("sin-version", None);
        assert!(sandbox.layout.resolve_node(&cwd).unwrap_err().contains("ninguna versión de Node"));
        sandbox.node("24.11.1", Some("krypton"));
        sandbox.node("22.21.0", Some("jod"));
        let resolved = sandbox.layout.resolve_node(&cwd).unwrap();
        assert_eq!((resolved.version.id.as_str(), &resolved.origin), ("24.11.1", &NodeOrigin::Newest));
        sandbox.layout.set_node_global("22").unwrap();
        let resolved = sandbox.layout.resolve_node(&cwd).unwrap();
        assert_eq!((resolved.version.id.as_str(), &resolved.origin), ("22.21.0", &NodeOrigin::Global));

        let old = sandbox.project("viejo", None);
        fs::write(old.join(NVMRC), "18\n").unwrap();
        let error = sandbox.layout.resolve_node(&old).unwrap_err();
        assert!(error.contains("Node 18") && error.contains("24.11.1, 22.21.0") && error.contains(".nvmrc"), "{error}");

        fs::write(old.join(NVMRC), "system\n").unwrap();
        assert!(sandbox.layout.resolve_node(&old).unwrap_err().contains(".nvmrc"));
        sandbox.layout.clear_node_global().unwrap();
        sandbox.layout.clear_node_global().unwrap();
        assert_eq!(sandbox.layout.node_global(), None);
    }

    #[test]
    fn project_setter_keeps_the_file_the_project_already_uses() {
        let sandbox = Sandbox::new();
        sandbox.node("24.11.1", Some("krypton"));
        let fresh = sandbox.project("nuevo", None);
        assert_eq!(sandbox.layout.set_node_project(&fresh, "24").unwrap(), fresh.join(NVMRC));
        assert_eq!(fs::read_to_string(fresh.join(NVMRC)).unwrap(), "24\n");

        let asdf = sandbox.project("asdf", None);
        fs::write(asdf.join(NODE_VERSION_FILE), "20\n").unwrap();
        assert_eq!(sandbox.layout.set_node_project(&asdf, "24.11.1").unwrap(), asdf.join(NODE_VERSION_FILE));
        assert!(!asdf.join(NVMRC).exists());

        assert!(sandbox.layout.set_node_project(&fresh, "18").is_err());
        assert!(sandbox.layout.set_node_global("18").is_err());
        assert_eq!(fs::read_to_string(fresh.join(NVMRC)).unwrap(), "24\n");
    }
}
