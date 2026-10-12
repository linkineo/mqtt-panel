//! Structural JSON diff and text line diff.

use crate::cast::{cast_value, Cast};
use crate::path;
use crate::types::{DiffKind, DiffLine, TextLine};
use serde_json::Value;

/// JSON diff as pretty-printed lines. Returns the lines and the number of changed, added and
/// removed leaves.
pub fn json_diff(old: &Value, new: &Value) -> (Vec<DiffLine>, u64) {
    let mut lines = Vec::new();
    let mut changes = 0u64;
    let mut p = String::from("$");
    diff_value(
        Some(old),
        Some(new),
        None,
        &mut p,
        0,
        &mut lines,
        &mut changes,
    );
    (lines, changes)
}

fn plottable(v: &Value) -> bool {
    matches!(cast_value(v), Cast::Value { .. })
}

fn is_container(v: &Value) -> bool {
    matches!(v, Value::Object(_) | Value::Array(_))
}

fn brackets(v: &Value) -> (&'static str, &'static str) {
    if matches!(v, Value::Array(_)) {
        ("[", "]")
    } else {
        ("{", "}")
    }
}

fn line(indent: u32, key: Option<&str>, kind: DiffKind) -> DiffLine {
    DiffLine {
        indent,
        key: key.map(str::to_string),
        path: None,
        old_value: None,
        new_value: None,
        open: None,
        close: None,
        kind,
        plottable: false,
    }
}

/// Emits a whole subtree with one kind (added, removed or same).
fn emit(
    v: &Value,
    key: Option<&str>,
    path: &mut String,
    indent: u32,
    kind: DiffKind,
    out: &mut Vec<DiffLine>,
    leaves: &mut u64,
) {
    let empty = match v {
        Value::Object(m) => m.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => true,
    };
    if !is_container(v) || empty {
        let mut l = line(indent, key, kind);
        l.path = Some(path.clone());
        let text = v.to_string();
        if kind == DiffKind::Removed {
            l.old_value = Some(text);
        } else {
            l.new_value = Some(text);
        }
        l.plottable = plottable(v);
        out.push(l);
        *leaves += 1;
        return;
    }
    let (o, c) = brackets(v);
    let mut open = line(indent, key, kind);
    open.open = Some(o.to_string());
    open.path = Some(path.clone());
    out.push(open);
    match v {
        Value::Object(m) => {
            for (k, child) in m {
                let len = path.len();
                path::push_key(path, k);
                emit(child, Some(k), path, indent + 1, kind, out, leaves);
                path.truncate(len);
            }
        }
        Value::Array(a) => {
            for (i, child) in a.iter().enumerate() {
                let len = path.len();
                path::push_index(path, i);
                emit(child, None, path, indent + 1, kind, out, leaves);
                path.truncate(len);
            }
        }
        _ => {}
    }
    let mut close = line(indent, None, kind);
    close.close = Some(c.to_string());
    out.push(close);
}

fn diff_value(
    old: Option<&Value>,
    new: Option<&Value>,
    key: Option<&str>,
    path: &mut String,
    indent: u32,
    out: &mut Vec<DiffLine>,
    changes: &mut u64,
) {
    match (old, new) {
        (None, None) => {}
        (None, Some(n)) => emit(n, key, path, indent, DiffKind::Added, out, changes),
        (Some(o), None) => emit(o, key, path, indent, DiffKind::Removed, out, changes),
        (Some(Value::Object(a)), Some(Value::Object(b))) if !a.is_empty() || !b.is_empty() => {
            let mut open = line(indent, key, DiffKind::Same);
            open.open = Some("{".into());
            open.path = Some(path.clone());
            out.push(open);
            // Union of keys: new order, removed keys inserted at their old position.
            let mut keys: Vec<&String> = b.keys().collect();
            for (i, k) in a.keys().enumerate() {
                if !b.contains_key(k) {
                    let at = i.min(keys.len());
                    keys.insert(at, k);
                }
            }
            for k in keys {
                let len = path.len();
                path::push_key(path, k);
                diff_value(a.get(k), b.get(k), Some(k), path, indent + 1, out, changes);
                path.truncate(len);
            }
            let mut close = line(indent, None, DiffKind::Same);
            close.close = Some("}".into());
            out.push(close);
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) if !a.is_empty() || !b.is_empty() => {
            let mut open = line(indent, key, DiffKind::Same);
            open.open = Some("[".into());
            open.path = Some(path.clone());
            out.push(open);
            for i in 0..a.len().max(b.len()) {
                let len = path.len();
                path::push_index(path, i);
                diff_value(a.get(i), b.get(i), None, path, indent + 1, out, changes);
                path.truncate(len);
            }
            let mut close = line(indent, None, DiffKind::Same);
            close.close = Some("]".into());
            out.push(close);
        }
        (Some(o), Some(n)) => {
            if is_container(o) != is_container(n) || (is_container(o) && brackets(o) != brackets(n))
            {
                // Type change between a container and something else: removed then added.
                let mut dummy = 0;
                emit(o, key, path, indent, DiffKind::Removed, out, &mut dummy);
                emit(n, key, path, indent, DiffKind::Added, out, &mut dummy);
                *changes += 1;
                return;
            }
            let mut l = line(
                indent,
                key,
                if o == n {
                    DiffKind::Same
                } else {
                    DiffKind::Changed
                },
            );
            l.path = Some(path.clone());
            l.new_value = Some(n.to_string());
            if o != n {
                l.old_value = Some(o.to_string());
                *changes += 1;
            }
            l.plottable = plottable(n);
            out.push(l);
        }
    }
}

/// Line diff of two texts. Returns lines and the number of added plus removed lines.
pub fn text_diff(old: &str, new: &str) -> (Vec<TextLine>, u64) {
    // A missing final newline must not count as a changed last line.
    let with_nl = |s: &str| {
        if s.is_empty() || s.ends_with('\n') {
            s.to_string()
        } else {
            format!("{s}\n")
        }
    };
    let (old, new) = (with_nl(old), with_nl(new));
    let diff = similar::TextDiff::from_lines(&old, &new);
    let mut out = Vec::new();
    let mut changes = 0;
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            similar::ChangeTag::Equal => DiffKind::Same,
            similar::ChangeTag::Insert => {
                changes += 1;
                DiffKind::Added
            }
            similar::ChangeTag::Delete => {
                changes += 1;
                DiffKind::Removed
            }
        };
        let text = change.value().trim_end_matches(['\n', '\r']).to_string();
        out.push(TextLine { kind, text });
    }
    (out, changes)
}

/// Dotted paths of the leaves that differ between two documents (for the history list).
pub fn changed_keys(old: &Value, new: &Value) -> Vec<String> {
    let mut a: Vec<(String, &Value)> = Vec::new();
    path::walk_leaves(old, 32, 2000, &mut |p, v| a.push((p.to_string(), v)));
    let mut b: Vec<(String, &Value)> = Vec::new();
    path::walk_leaves(new, 32, 2000, &mut |p, v| b.push((p.to_string(), v)));
    let old_map: std::collections::HashMap<&str, &Value> =
        a.iter().map(|(p, v)| (p.as_str(), *v)).collect();
    let new_set: std::collections::HashSet<&str> = b.iter().map(|(p, _)| p.as_str()).collect();
    let mut out = Vec::new();
    for (p, v) in &b {
        match old_map.get(p.as_str()) {
            Some(o) if *o == *v => {}
            _ => out.push(path::label(p)),
        }
    }
    for (p, _) in &a {
        if !new_set.contains(p.as_str()) {
            out.push(path::label(p));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_changed() {
        let a = json!({"a": 1, "b": {"c": 2}, "d": 3});
        let b = json!({"a": 1, "b": {"c": 5}, "e": 4});
        assert_eq!(changed_keys(&a, &b), vec!["b.c", "e", "d"]);
    }
}
