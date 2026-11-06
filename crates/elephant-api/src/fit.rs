//! Lo que se hace con toda respuesta antes de que salga hacia un agente
//! (05 §5.1, paso 7): ocultar secretos y recortar a 24 KB.

use serde_json::Value;

use crate::{MAX_REPLY_BYTES, Output, Reply, UNTRUSTED_BEGIN, UNTRUSTED_END};

/// Enmascara, recorta y marca el contenido no fiable. Con `oldest_first`
/// (`logs tail`), lo que sobra se quita del principio: las entradas más
/// recientes son las que importan.
pub(crate) fn finish(out: Output, oldest_first: bool) -> Reply {
    finish_with_cap(out, MAX_REPLY_BYTES, oldest_first)
}

pub(crate) fn finish_with_cap(out: Output, cap: usize, oldest_first: bool) -> Reply {
    let mut structured = out.structured;
    elephant_core::redact::mask_json(&mut structured);
    let text = elephant_core::redact::mask(&out.text);
    let (structured, omitted, cursor) = fit_json(structured, cap, oldest_first);
    let text = if oldest_first {
        fit_text_keeping_end(&text, cap)
    } else {
        fit_text(&text, cap, omitted, cursor.as_deref())
    };
    Reply {
        untrusted: structured.get("untrusted").and_then(Value::as_bool).unwrap_or(false),
        next_cursor: structured.get("next_cursor").and_then(Value::as_str).map(str::to_string),
        structured,
        text,
    }
}

fn size(value: &Value) -> usize {
    serde_json::to_string(value).map(|s| s.len()).unwrap_or(usize::MAX)
}

/// Quita elementos del final del array más grande hasta que quepa. Si se
/// recortan `entries` y hay `next_cursor`, el cursor pasa a ser la última
/// entrada que queda, para que la página siguiente empiece justo después.
/// Con `oldest_first`, `entries` se recorta por el principio y sin cursor.
fn fit_json(mut value: Value, cap: usize, oldest_first: bool) -> (Value, usize, Option<String>) {
    // Margen para `omitted` y el cursor.
    let budget = cap.saturating_sub(128);
    let mut omitted = 0;
    let mut entries_cut = false;
    loop {
        let total = size(&value);
        if total <= budget {
            break;
        }
        let Some(path) = largest_array(&value, &mut Vec::new()) else {
            shorten_longest_string(&mut value, total - budget);
            break;
        };
        let is_entries = path.len() == 1 && path[0] == "entries";
        entries_cut |= is_entries;
        if let Some(Value::Array(items)) = pointer_mut(&mut value, &path) {
            // Lo que sobra, según el tamaño medio de cada elemento; al menos uno.
            let per_item = (size(&Value::Array(items.clone())) / items.len()).max(1);
            let remove = (total - budget).div_ceil(per_item).clamp(1, items.len());
            if is_entries && oldest_first {
                items.drain(..remove);
            } else {
                items.truncate(items.len() - remove);
            }
            omitted += remove;
        }
    }
    let mut cursor = None;
    if omitted > 0
        && let Value::Object(map) = &mut value
    {
        map.insert("omitted".into(), Value::from(omitted));
        if entries_cut && !oldest_first && map.contains_key("next_cursor") {
            let last = map
                .get("entries")
                .and_then(Value::as_array)
                .and_then(|entries| entries.last())
                .and_then(|entry| entry.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(last) = last {
                map.insert("next_cursor".into(), Value::String(last.clone()));
                cursor = Some(last);
            }
        }
    }
    (value, omitted, cursor)
}

fn largest_array(value: &Value, path: &mut Vec<String>) -> Option<Vec<String>> {
    let mut best: Option<(usize, Vec<String>)> = None;
    visit(value, path, &mut best);
    best.map(|(_, path)| path)
}

fn visit(value: &Value, path: &mut Vec<String>, best: &mut Option<(usize, Vec<String>)>) {
    match value {
        Value::Array(items) if !items.is_empty() => {
            let bytes = size(value);
            if best.as_ref().is_none_or(|(b, _)| bytes > *b) {
                *best = Some((bytes, path.clone()));
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                path.push(key.clone());
                visit(item, path, best);
                path.pop();
            }
        }
        _ => {}
    }
}

fn pointer_mut<'a>(value: &'a mut Value, path: &[String]) -> Option<&'a mut Value> {
    path.iter().try_fold(value, |current, key| current.get_mut(key.as_str()))
}

fn shorten_longest_string(value: &mut Value, excess: usize) {
    fn longest(value: &mut Value) -> Option<&mut String> {
        match value {
            Value::String(text) => Some(text),
            Value::Array(items) => items.iter_mut().filter_map(longest).max_by_key(|s| s.len()),
            Value::Object(map) => map.values_mut().filter_map(longest).max_by_key(|s| s.len()),
            _ => None,
        }
    }
    if let Some(text) = longest(value) {
        let keep = text.len().saturating_sub(excess + 16);
        *text = truncate_bytes(text, keep);
    }
}

/// Recorta a una frontera de línea y deja claro que falta algo.
fn fit_text(text: &str, cap: usize, omitted: usize, cursor: Option<&str>) -> String {
    let budget = cap.saturating_sub(256);
    if text.len() <= budget {
        return text.to_string();
    }
    let cut = truncate_bytes(text, budget);
    let mut out = match cut.rfind('\n') {
        Some(at) => cut[..at].to_string(),
        None => cut,
    };
    if let Some(begin) = out.rfind(UNTRUSTED_BEGIN)
        && !out[begin..].contains(UNTRUSTED_END)
    {
        out.push('\n');
        out.push_str(UNTRUSTED_END);
    }
    out.push('\n');
    match cursor {
        Some(cursor) => out.push_str(&format!(
            "{} resultados más: usa `--cursor {cursor}`.",
            omitted.max(1)
        )),
        None => out.push_str("Salida recortada a 24 KB: filtra con las opciones de la orden o con `--json | jq`."),
    }
    out
}

/// Como [`fit_text`], pero quita líneas del principio del bloque no fiable y
/// conserva el título y el final, que es lo más reciente.
fn fit_text_keeping_end(text: &str, cap: usize) -> String {
    let budget = cap.saturating_sub(256);
    if text.len() <= budget {
        return text.to_string();
    }
    const NOTE: &str =
        "Salida recortada a 24 KB: faltan las entradas más antiguas. Acota con --lines, --level o --contains.";
    // Lo que va hasta la marca de inicio (incluida) se queda.
    let head_end = text
        .find(UNTRUSTED_BEGIN)
        .map(|at| text[at..].find('\n').map_or(text.len(), |n| at + n + 1))
        .unwrap_or(0);
    let (head, body) = text.split_at(head_end);
    let head_before_marker = head.strip_suffix('\n').unwrap_or(head);
    let (title, marker) = match head_before_marker.rfind('\n') {
        Some(at) => (&head_before_marker[..at], &head_before_marker[at + 1..]),
        None => ("", head_before_marker),
    };
    let fixed = title.len() + NOTE.len() + marker.len() + 3;
    let room = budget.saturating_sub(fixed);
    let mut start = body.len().saturating_sub(room);
    while !body.is_char_boundary(start) {
        start += 1;
    }
    // Desde el principio de una línea.
    if start > 0 {
        start = body[start..].find('\n').map_or(body.len(), |n| start + n + 1);
    }
    let mut out = String::new();
    if !title.is_empty() {
        out.push_str(title);
        out.push('\n');
    }
    out.push_str(NOTE);
    out.push('\n');
    if !marker.is_empty() {
        out.push_str(marker);
        out.push('\n');
    }
    out.push_str(&body[start..]);
    out
}

/// Como mucho `max` bytes, sin partir un carácter. Añade «…» si recorta.
pub(crate) fn truncate_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Como mucho `max` caracteres (C14: los mensajes, 500). Añade «…».
pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn small_replies_pass_untouched_but_masked() {
        let reply = finish(
            Output {
                structured: json!({"line": "DB_PASSWORD=secreto", "untrusted": true, "next_cursor": null}),
                text: "DB_PASSWORD=secreto".into(),
            },
            false,
        );
        assert_eq!(reply.structured["line"], "DB_PASSWORD=••••");
        assert_eq!(reply.text, "DB_PASSWORD=••••");
        assert!(reply.untrusted);
        assert_eq!(reply.next_cursor, None);
    }

    #[test]
    fn long_lists_are_cut_and_the_cursor_follows_the_last_kept_entry() {
        let entries: Vec<Value> = (0..2000).map(|i| json!({"id": format!("e{i}"), "message": "x".repeat(40)})).collect();
        let mut text = format!("{UNTRUSTED_BEGIN}\n");
        for i in 0..2000 {
            text.push_str(&format!("e{i} {}\n", "x".repeat(40)));
        }
        text.push_str(UNTRUSTED_END);
        let reply = finish(
            Output { structured: json!({"entries": entries, "next_cursor": "e1999", "untrusted": true}), text },
            false,
        );
        let json = serde_json::to_string(&reply.structured).unwrap();
        assert!(json.len() <= MAX_REPLY_BYTES, "{}", json.len());
        let kept = reply.structured["entries"].as_array().unwrap();
        let last = kept.last().unwrap()["id"].as_str().unwrap();
        assert_eq!(reply.next_cursor.as_deref(), Some(last));
        assert_eq!(reply.structured["omitted"], 2000 - kept.len());
        assert!(reply.text.len() <= MAX_REPLY_BYTES);
        assert!(reply.text.contains(UNTRUSTED_END));
        assert!(reply.text.ends_with(&format!("`--cursor {last}`.")), "{}", &reply.text[reply.text.len() - 80..]);
    }

    #[test]
    fn tail_keeps_the_newest_entries_and_offers_no_cursor() {
        // 100 entradas de ~1,2 KB en orden cronológico, como `logs tail`.
        let entries: Vec<Value> = (0..100)
            .map(|i| json!({"id": format!("e{i}"), "message": "m".repeat(500), "exception": "x".repeat(500)}))
            .collect();
        let mut text = format!("100 entradas, de la más antigua a la más reciente\n{UNTRUSTED_BEGIN}\n");
        for i in 0..100 {
            text.push_str(&format!("e{i} {} {}\n", "m".repeat(500), "x".repeat(500)));
        }
        text.push_str(UNTRUSTED_END);
        let reply = finish(
            Output { structured: json!({"entries": entries, "untrusted": true, "next_cursor": null}), text },
            true,
        );
        let kept = reply.structured["entries"].as_array().unwrap();
        assert!(kept.len() < 100 && !kept.is_empty());
        assert_eq!(kept.last().unwrap()["id"], "e99", "se conservan las más recientes");
        assert_eq!(reply.structured["next_cursor"], Value::Null);
        assert_eq!(reply.next_cursor, None);
        assert_eq!(reply.structured["omitted"], 100 - kept.len());
        assert!(serde_json::to_string(&reply.structured).unwrap().len() <= MAX_REPLY_BYTES);

        assert!(reply.text.len() <= MAX_REPLY_BYTES);
        assert!(!reply.text.contains("--cursor"));
        assert!(reply.text.starts_with("100 entradas, de la más antigua a la más reciente\nSalida recortada"));
        assert!(reply.text.contains(&format!("\n{UNTRUSTED_BEGIN}\ne")), "{}", &reply.text[..300]);
        assert!(reply.text.ends_with(UNTRUSTED_END));
        assert!(reply.text.contains("e99 "));
        assert!(!reply.text.contains("\ne0 "));
    }

    #[test]
    fn a_single_huge_string_is_shortened() {
        let reply = finish(Output { structured: json!({"body": "y".repeat(100_000)}), text: "ok".into() }, false);
        assert!(serde_json::to_string(&reply.structured).unwrap().len() <= MAX_REPLY_BYTES);
        assert!(reply.structured["body"].as_str().unwrap().ends_with('…'));
    }

    #[test]
    fn truncation_respects_characters() {
        assert_eq!(truncate_chars("ñandú", 3), "ñan…");
        assert_eq!(truncate_chars("abc", 3), "abc");
        assert_eq!(truncate_bytes("ñ", 1), "…");
    }
}
