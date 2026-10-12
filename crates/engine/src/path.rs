//! Field paths: `$`, `$.a.b`, `$.a[2].c`, `$["key with.dot"]`.

use serde_json::Value;

/// One step of a field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// Parses a path. A missing leading `$` is accepted (`supply.temp` is `$.supply.temp`).
/// Returns `None` for malformed paths.
pub fn parse(path: &str) -> Option<Vec<Step>> {
    let p = path.trim();
    let mut rest = p.strip_prefix('$').unwrap_or(p);
    let mut steps = Vec::new();
    let mut first = p.starts_with('$');
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('[') {
            if let Some(r2) = r.strip_prefix('"') {
                // JSON-quoted key
                let mut end = None;
                let bytes = r2.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'"' => {
                            end = Some(i);
                            break;
                        }
                        _ => i += 1,
                    }
                }
                let end = end?;
                let key: String = serde_json::from_str(&format!("\"{}\"", &r2[..end])).ok()?;
                let after = r2[end + 1..].strip_prefix(']')?;
                steps.push(Step::Key(key));
                rest = after;
            } else {
                let close = r.find(']')?;
                let idx: usize = r[..close].trim().parse().ok()?;
                steps.push(Step::Index(idx));
                rest = &r[close + 1..];
            }
        } else {
            let r = if let Some(r) = rest.strip_prefix('.') {
                r
            } else if !first {
                rest
            } else {
                return None;
            };
            let end = r.find(['.', '[']).unwrap_or(r.len());
            if end == 0 {
                return None;
            }
            steps.push(Step::Key(r[..end].to_string()));
            rest = &r[end..];
        }
        first = true;
    }
    Some(steps)
}

/// Normalizes a path to its canonical text (`$.a[2]`).
pub fn normalize(path: &str) -> Option<String> {
    parse(path).map(|s| format_steps(&s))
}

fn needs_brackets(key: &str) -> bool {
    key.is_empty() || key.contains(['.', '[', ']', '"', '\\']) || key.trim() != key
}

/// Appends a key step to a path text.
pub fn push_key(path: &mut String, key: &str) {
    if needs_brackets(key) {
        path.push('[');
        path.push_str(&serde_json::to_string(key).unwrap_or_default());
        path.push(']');
    } else {
        path.push('.');
        path.push_str(key);
    }
}

/// Appends an index step to a path text.
pub fn push_index(path: &mut String, i: usize) {
    path.push('[');
    path.push_str(&i.to_string());
    path.push(']');
}

/// Formats steps as a canonical path.
pub fn format_steps(steps: &[Step]) -> String {
    let mut s = String::from("$");
    for st in steps {
        match st {
            Step::Key(k) => push_key(&mut s, k),
            Step::Index(i) => push_index(&mut s, *i),
        }
    }
    s
}

/// The label of a path: the path without `$.`; `value` for the whole payload.
pub fn label(path: &str) -> String {
    if path == "$" {
        "value".to_string()
    } else if let Some(r) = path.strip_prefix("$.") {
        r.to_string()
    } else if let Some(r) = path.strip_prefix('$') {
        r.to_string()
    } else {
        path.to_string()
    }
}

/// Resolves steps in a value.
pub fn resolve<'a>(v: &'a Value, steps: &[Step]) -> Option<&'a Value> {
    let mut cur = v;
    for st in steps {
        cur = match (st, cur) {
            (Step::Key(k), Value::Object(m)) => m.get(k)?,
            (Step::Index(i), Value::Array(a)) => a.get(*i)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Visits leaves (scalars, empty objects and arrays) in document order, up to `max_depth`
/// levels and `max_leaves` leaves. The callback gets the path text and the value.
pub fn walk_leaves<'a>(
    v: &'a Value,
    max_depth: usize,
    max_leaves: usize,
    f: &mut dyn FnMut(&str, &'a Value),
) {
    let mut path = String::from("$");
    let mut count = 0usize;
    walk(v, &mut path, 0, max_depth, max_leaves, &mut count, f);
}

fn walk<'a>(
    v: &'a Value,
    path: &mut String,
    depth: usize,
    max_depth: usize,
    max_leaves: usize,
    count: &mut usize,
    f: &mut dyn FnMut(&str, &'a Value),
) {
    if *count >= max_leaves {
        return;
    }
    match v {
        Value::Object(m) if !m.is_empty() && depth < max_depth => {
            for (k, child) in m {
                let len = path.len();
                push_key(path, k);
                walk(child, path, depth + 1, max_depth, max_leaves, count, f);
                path.truncate(len);
                if *count >= max_leaves {
                    return;
                }
            }
        }
        Value::Array(a) if !a.is_empty() && depth < max_depth => {
            for (i, child) in a.iter().enumerate() {
                let len = path.len();
                push_index(path, i);
                walk(child, path, depth + 1, max_depth, max_leaves, count, f);
                path.truncate(len);
                if *count >= max_leaves {
                    return;
                }
            }
        }
        Value::Object(_) | Value::Array(_) if depth >= max_depth => {}
        _ => {
            *count += 1;
            f(path, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_and_format() {
        assert_eq!(normalize("$.supply.temp").unwrap(), "$.supply.temp");
        assert_eq!(normalize("supply.temp").unwrap(), "$.supply.temp");
        assert_eq!(normalize("$.readings[2].v").unwrap(), "$.readings[2].v");
        assert_eq!(normalize("$").unwrap(), "$");
        assert_eq!(normalize("$[\"a.b\"].c").unwrap(), "$[\"a.b\"].c");
        assert_eq!(
            normalize("$.Node Control/Rebirth").unwrap(),
            "$.Node Control/Rebirth"
        );
        assert!(parse("$.").is_none());
        assert!(parse("$[x]").is_none());
        assert_eq!(label("$.a.b"), "a.b");
        assert_eq!(label("$"), "value");
    }

    #[test]
    fn walk_in_order() {
        let v = json!({"b": 1, "a": {"x": [1, {"y": 2}], "e": {}}});
        let mut seen = Vec::new();
        walk_leaves(&v, 8, 100, &mut |p, _| seen.push(p.to_string()));
        assert_eq!(seen, vec!["$.b", "$.a.x[0]", "$.a.x[1].y", "$.a.e"]);
        let steps = parse("$.a.x[1].y").unwrap();
        assert_eq!(resolve(&v, &steps), Some(&json!(2)));
    }
}
