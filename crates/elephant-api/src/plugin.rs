//! El plugin local de Claude Code (05 §6.3), embebido en el binario para que
//! siempre corresponda a la versión de `elephant`. La app lo escribe con
//! `elephant __write-plugin ~/.elephant-stack/share/claude-plugin`.
//!
//! Las skills `diagnose-500` y `new-site` y los prompts de `elephant mcp`
//! salen de las mismas plantillas.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use elephant_core::tr;

const MARKETPLACE: &str = include_str!("../plugin/.claude-plugin/marketplace.json");
const PLUGIN: &str = include_str!("../plugin/elephant/.claude-plugin/plugin.json");
const WRAPPER: &str = include_str!("../plugin/elephant/bin/elephant");
const SKILL: &str = include_str!("../plugin/elephant/skills/elephant/SKILL.md");
const DIAGNOSE: &str = include_str!("../plugin/templates/diagnose-500.md");
const NEW_SITE: &str = include_str!("../plugin/templates/new-site.md");

/// Marca que identifica una carpeta escrita por `__write-plugin`. Contiene la
/// salida de `elephant --version`, para que la app sepa si debe reescribirla.
pub const VERSION_FILE: &str = ".elephant-version";

/// `elephant 0.1.0 (api 1)`.
pub fn version_line() -> String {
    format!("elephant {} (api {})", env!("CARGO_PKG_VERSION"), crate::API_VERSION)
}

/// Un prompt de `elephant mcp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// `(nombre, descripción, obligatorio)`.
    pub arguments: &'static [(&'static str, &'static str, bool)],
}

pub const PROMPTS: [PromptSpec; 2] = [
    PromptSpec {
        name: "diagnose-500",
        description: "Diagnostica un error 500 de un proyecto local con los logs de Laravel y nginx.",
        arguments: &[("project", "Id, nombre o host del proyecto", true), ("path", "Ruta que falla, como /checkout", false)],
    },
    PromptSpec {
        name: "new-laravel-site",
        description: "Crea un sitio Laravel nuevo servido por Herd como <nombre>.test.",
        arguments: &[("name", "Nombre del proyecto (minúsculas, números y guiones)", true)],
    },
];

/// El texto de un prompt con sus argumentos. Un argumento obligatorio vacío
/// es un error.
pub fn render_prompt(name: &str, args: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let spec = PROMPTS.iter().find(|p| p.name == name).ok_or_else(|| tr!("No existe el prompt «{name}».", "There’s no prompt “{name}”."))?;
    for (arg, _, required) in spec.arguments {
        if *required && args(arg).is_none_or(|v| v.trim().is_empty()) {
            return Err(tr!("Falta el argumento «{arg}».", "Missing argument “{arg}”."));
        }
    }
    let value = |arg: &str| args(arg).map(|v| one_token(&v)).unwrap_or_default();
    Ok(match name {
        "diagnose-500" => {
            let path = value("path");
            render(DIAGNOSE, &value("project"), &if path.is_empty() { String::new() } else { format!(" (ruta {path})") }, "")
        }
        _ => render(NEW_SITE, "", "", &value("name")),
    })
}

/// Un argumento de prompt se usa como una sola palabra dentro de órdenes de
/// ejemplo: sin espacios ni comillas.
fn one_token(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
        .take(100)
        .collect()
}

fn render(template: &str, project: &str, path: &str, name: &str) -> String {
    template.replace("{{project}}", project).replace("{{path}}", path).replace("{{name}}", name)
}

/// El cuerpo de la skill sin la cabecera, para `AGENTS.md` o las reglas de
/// Cursor (`elephant guide`).
pub fn guide() -> String {
    let body = SKILL.splitn(3, "---\n").nth(2).unwrap_or(SKILL);
    let body = body.replace(
        "!`elephant status --brief || true`",
        "Ejecuta `elephant status --brief` para ver los servicios, el PHP y las tareas en curso.",
    );
    format!("# Elephant Stack\n{}", body.trim_start_matches('\n').replacen("## Estado ahora", "## Estado", 1))
}

/// `plugin.json` con la versión del binario. Con un marketplace en una ruta
/// local no fija nada, pero `claude plugin validate --strict` la exige.
fn plugin_json() -> String {
    let mut value: serde_json::Value = serde_json::from_str(PLUGIN).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert("version".into(), env!("CARGO_PKG_VERSION").into());
    }
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| PLUGIN.to_string());
    text.push('\n');
    text
}

/// Archivos del plugin: `(ruta relativa, contenido, ejecutable)`.
pub fn files() -> Vec<(&'static str, String, bool)> {
    let skill_head = |name: &str, description: &str, hint: &str| {
        format!(
            "---\nname: {name}\ndescription: {description}\nargument-hint: {hint}\ndisable-model-invocation: true\nallowed-tools: Bash(elephant *)\n---\n"
        )
    };
    vec![
        (".claude-plugin/marketplace.json", MARKETPLACE.to_string(), false),
        ("elephant/.claude-plugin/plugin.json", plugin_json(), false),
        ("elephant/bin/elephant", WRAPPER.to_string(), true),
        ("elephant/skills/elephant/SKILL.md", SKILL.to_string(), false),
        ("elephant/skills/elephant/reference.md", crate::schema::markdown(), false),
        (
            "elephant/skills/diagnose-500/SKILL.md",
            skill_head(
                "diagnose-500",
                "Diagnostica un error 500 de un proyecto local con Elephant Stack (logs de Laravel y nginx).",
                "<proyecto>",
            ) + &render(DIAGNOSE, "$ARGUMENTS", "", ""),
            false,
        ),
        (
            "elephant/skills/new-site/SKILL.md",
            skill_head("new-site", "Crea un sitio Laravel nuevo servido por Herd como <nombre>.test.", "<nombre>")
                + &render(NEW_SITE, "", "", "$ARGUMENTS"),
            false,
        ),
        (VERSION_FILE, format!("{}\n", version_line()), false),
    ]
}

/// Escribe el plugin en `dir` de una vez: prepara una carpeta hermana y la
/// cambia por la anterior, así que nunca queda medio escrito ni con archivos
/// de una versión vieja. Se niega a sustituir una carpeta que no sea suya.
pub fn write_plugin(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.is_absolute() {
        let dir = dir.display();
        return Err(tr!("La carpeta del plugin debe ser una ruta absoluta: {dir}", "The plugin folder must be an absolute path: {dir}"));
    }
    if dir.exists() && !dir.join(VERSION_FILE).is_file() {
        let empty = fs::read_dir(dir).map(|mut entries| entries.next().is_none()).unwrap_or(false);
        if !empty {
            let dir = dir.display();
            return Err(tr!("{dir} ya existe y no es un plugin de Elephant Stack.", "{dir} already exists and isn’t an Elephant Stack plugin."));
        }
    }
    let invalid = || tr!("Carpeta del plugin no válida.", "Invalid plugin folder.");
    let parent = dir.parent().ok_or_else(invalid)?;
    let name = dir.file_name().ok_or_else(invalid)?.to_string_lossy().into_owned();
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = parent.join(format!(".{name}.new-{}", std::process::id()));
    let old = parent.join(format!(".{name}.old-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);

    let mut written = Vec::new();
    for (path, content, executable) in files() {
        let target = staging.join(path);
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        fs::write(&target, content).map_err(|error| {
            let path = target.display();
            tr!("No se pudo escribir {path}: {error}", "Couldn’t write {path}: {error}")
        })?;
        if executable {
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())?;
        }
        written.push(dir.join(path));
    }

    if dir.exists() {
        fs::rename(dir, &old).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staging, dir) {
        let _ = fs::rename(&old, dir);
        let _ = fs::remove_dir_all(&staging);
        return Err(error.to_string());
    }
    let _ = fs::remove_dir_all(&old);
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elephant-api-plugin-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_skill_follows_the_contract() {
        let lines = SKILL.lines().count();
        assert!(lines <= 150, "{lines} líneas");
        assert!(SKILL.contains("allowed-tools: Bash(elephant *)"));
        assert!(SKILL.contains("!`elephant status --brief || true`"));
        let description = SKILL.lines().find_map(|l| l.strip_prefix("description: ")).unwrap();
        assert!(description.chars().count() <= 400, "{}", description.chars().count());
        assert!(description.starts_with("Entorno PHP/Laravel local"));
        // Sin servidor MCP en el plugin (05 §3).
        assert!(files().iter().all(|(path, _, _)| !path.ends_with(".mcp.json") && !path.contains("hooks")));
    }

    #[test]
    fn writes_and_replaces_the_plugin() {
        let root = temp("write");
        let dir = root.join("claude-plugin");
        let written = write_plugin(&dir).unwrap();
        assert!(written.contains(&dir.join("elephant/skills/elephant/SKILL.md")));
        let wrapper = dir.join("elephant/bin/elephant");
        assert_eq!(fs::metadata(&wrapper).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read_to_string(dir.join(VERSION_FILE)).unwrap().trim(), version_line());
        let marketplace: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(".claude-plugin/marketplace.json")).unwrap()).unwrap();
        assert_eq!(marketplace["name"], "elephant-stack");
        assert_eq!(marketplace["plugins"][0]["source"], "./elephant");
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("elephant/.claude-plugin/plugin.json")).unwrap()).unwrap();
        assert_eq!(manifest["name"], "elephant");
        assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));

        // Un archivo que ya no forma parte del plugin desaparece al reescribirlo.
        fs::write(dir.join("elephant/sobra.txt"), "x").unwrap();
        write_plugin(&dir).unwrap();
        assert!(!dir.join("elephant/sobra.txt").exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_replace_a_folder_that_is_not_ours() {
        let root = temp("foreign");
        fs::write(root.join("importante.txt"), "no borrar").unwrap();
        assert!(write_plugin(&root).is_err());
        assert!(root.join("importante.txt").exists());
        assert!(write_plugin(Path::new("relativa")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prompts_and_skills_share_templates() {
        let text = render_prompt("diagnose-500", &|a| match a {
            "project" => Some("shop".into()),
            "path" => Some("/checkout; rm -rf ~".into()),
            _ => None,
        })
        .unwrap();
        assert!(text.contains("elephant logs errors --project shop --since 1h"));
        assert!(text.contains("(ruta /checkoutrm-rf)"));
        assert!(render_prompt("new-laravel-site", &|_| None).is_err());
        let skill = files().into_iter().find(|(p, _, _)| p.contains("diagnose-500")).unwrap().1;
        assert!(skill.contains("disable-model-invocation: true"));
        assert!(skill.contains("--project $ARGUMENTS"));
    }

    #[test]
    fn the_guide_has_no_frontmatter_or_injection() {
        let guide = guide();
        assert!(guide.starts_with("# Elephant Stack\n"));
        assert!(!guide.contains("allowed-tools"));
        assert!(!guide.contains("!`"));
        assert!(guide.contains("## Recetas"));
    }
}
