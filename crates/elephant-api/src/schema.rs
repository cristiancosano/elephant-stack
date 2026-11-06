//! El contrato versionado del catálogo (05 §4.2): `elephant schema` lo vuelca
//! en JSON (`docs/agent-api.json`) o en Markdown (el `reference.md` de la
//! skill), y `--check` compara con el archivo del repositorio.

use serde_json::{Map, Value, json};

use crate::{API_VERSION, Kind, catalog};

/// El catálogo entero, con las claves ordenadas para que la salida no
/// dependa de las features de `serde_json` que active otra crate.
pub fn document() -> Value {
    let operations: Vec<Value> = catalog()
        .iter()
        .map(|op| {
            json!({
                "id": op.id,
                "cli": format!("elephant {}", op.cli_path()),
                "kind": op.kind,
                "idempotent": op.idempotent,
                "long": op.long,
                "about": (op.about)(),
                "input_schema": (op.input_schema)(),
                "output_schema": (op.output_schema)(),
            })
        })
        .collect();
    canonical(json!({ "api": API_VERSION, "operations": operations }))
}

/// `document()` en JSON con sangría y salto de línea final.
pub fn document_text() -> String {
    let mut text = serde_json::to_string_pretty(&document()).unwrap_or_default();
    text.push('\n');
    text
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = Map::new();
            for (key, item) in entries {
                sorted.insert(key, canonical(item));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

/// La referencia para la skill, generada a partir de clap (nombres de la CLI)
/// y del catálogo (ids de MCP).
pub fn markdown() -> String {
    let mut out = String::from(
        "# Referencia de `elephant`\n\nGenerado con `elephant schema --markdown`. No lo edites a mano.\n\n\
         Todas las órdenes aceptan `--json`, que imprime exactamente el `structuredContent` de `elephant mcp`. \
         Los errores van a stderr (en JSON con `--json`).\n",
    );
    for (title, kind) in [("Lecturas", Kind::Read), ("Mutaciones (piden confirmación según el modo)", Kind::Mutate)] {
        out.push_str(&format!("\n## {title}\n"));
        for op in catalog().iter().filter(|op| op.kind == kind) {
            let command = (op.augment)(clap::Command::new("x"));
            let mut usage = format!("elephant {}", op.cli_path());
            let mut rows = Vec::new();
            for arg in command.get_arguments() {
                let id = arg.get_id().as_str();
                let mut help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
                if !help.is_empty() && !help.ends_with(['.', ')', '»']) {
                    help.push('.');
                }
                // Los interruptores (`--brief`) no llevan valores ni valor por defecto.
                let takes_value = arg.get_action().takes_values();
                let default = arg
                    .get_default_values()
                    .first()
                    .filter(|_| takes_value)
                    .map(|d| format!(" Por defecto: {}.", d.to_string_lossy()))
                    .unwrap_or_default();
                let values: Vec<String> = arg.get_possible_values().iter().map(|v| v.get_name().to_string()).collect();
                let values = if values.is_empty() || !takes_value {
                    String::new()
                } else {
                    format!(" Valores: {}.", values.join(", "))
                };
                let name = if arg.is_positional() {
                    let value = arg.get_value_names().and_then(|v| v.first()).map(|v| v.to_string()).unwrap_or_else(|| id.to_uppercase());
                    usage.push_str(&format!(" {value}"));
                    value
                } else {
                    let long = arg.get_long().map(|l| format!("--{l}")).unwrap_or_default();
                    match arg.get_short() {
                        Some(short) => format!("-{short}, {long}"),
                        None => long,
                    }
                };
                rows.push(format!("| `{name}` | `{id}` | {help}{default}{values} |"));
            }
            out.push_str(&format!("\n### `{usage}`\n\nMCP: `{}`", op.id));
            if op.long {
                out.push_str(" · tarea larga (espera 45 s; después, código 75 y `job show`)");
            }
            out.push_str(&format!(".\n\n{}\n", (op.about)()));
            if !rows.is_empty() {
                out.push_str("\n| CLI | MCP | Descripción |\n|---|---|---|\n");
                out.push_str(&rows.join("\n"));
                out.push('\n');
            }
        }
    }
    out.push_str(
        "\n## Solo en la CLI\n\n\
         - `elephant logs watch [--errors] [--project P] --timeout S`: una línea por cada entrada nueva, para Monitor. `--timeout` es obligatorio (como mucho 1800 s).\n\
         - `elephant doctor [--json]`: enlace, versión de la API, modo y número de operaciones.\n\
         - `elephant schema [--check] [--markdown]`: este catálogo en JSON o Markdown.\n\
         - `elephant guide`: la guía para `AGENTS.md` o las reglas de otros agentes.\n\
         - `elephant mcp`: el mismo catálogo como servidor MCP por stdio.\n\
         - `elephant --version`.\n\
         \n## Códigos de salida\n\n\
         | Código | Significado |\n|---|---|\n\
         | 0 | Bien |\n| 1 | Error de ejecución |\n| 2 | Uso incorrecto o validación |\n\
         | 3 | Denegado por el modo o por la persona: no lo reintentes por otra vía |\n\
         | 4 | Todavía no disponible |\n| 5 | Necesita administrador: hazlo desde Elephant Stack |\n\
         | 75 | Tarea larga en marcha: `elephant job show ID --wait 60` |\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Si falla tras cambiar el catálogo: `cargo run -p elephant-cli -- schema > docs/agent-api.json`
    /// (y, si el cambio es incompatible, sube `API_VERSION`).
    #[test]
    fn matches_docs_agent_api_json() {
        assert_eq!(document_text(), include_str!("../../../docs/agent-api.json"));
    }

    /// Si falla: `cargo run -p elephant-cli -- schema --markdown > crates/elephant-api/plugin/elephant/skills/elephant/reference.md`.
    #[test]
    fn matches_the_committed_reference() {
        assert_eq!(markdown(), include_str!("../plugin/elephant/skills/elephant/reference.md"));
    }

    #[test]
    fn keys_are_sorted() {
        let doc = document();
        let keys: Vec<&String> = doc.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["api", "operations"]);
        let first = doc["operations"][0].as_object().unwrap();
        let keys: Vec<&String> = first.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
