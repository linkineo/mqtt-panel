//! The user-facing message catalog, shared with the UI (packages/ui/src/i18n/<lang>/errors.json).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

const EN: &str = include_str!("../../../packages/ui/src/i18n/en/errors.json");
const FR: &str = include_str!("../../../packages/ui/src/i18n/fr/errors.json");
const DE: &str = include_str!("../../../packages/ui/src/i18n/de/errors.json");

/// Languages with a catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
    De,
}

impl Lang {
    /// From a locale string such as "fr_CH.UTF-8" or "de-DE"; English otherwise.
    pub fn from_locale(s: &str) -> Lang {
        let s = s.to_ascii_lowercase();
        if s.starts_with("fr") {
            Lang::Fr
        } else if s.starts_with("de") {
            Lang::De
        } else {
            Lang::En
        }
    }
    /// From the environment (LC_ALL, LC_MESSAGES, LANG).
    pub fn from_env() -> Lang {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if !v.is_empty() && v != "C" && v != "POSIX" {
                    return Lang::from_locale(&v);
                }
            }
        }
        Lang::En
    }
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    static T: OnceLock<[HashMap<String, String>; 3]> = OnceLock::new();
    let t = T.get_or_init(|| {
        let p = |s: &str| serde_json::from_str::<HashMap<String, String>>(s).unwrap_or_default();
        [p(EN), p(FR), p(DE)]
    });
    match lang {
        Lang::En => &t[0],
        Lang::Fr => &t[1],
        Lang::De => &t[2],
    }
}

/// Every key of the English catalog (for completeness tests).
pub fn keys(lang: Lang) -> Vec<String> {
    table(lang).keys().cloned().collect()
}

/// A user-facing message: catalog key, parameters, and the English text as a fallback.
/// A string parameter that starts with "@" names another catalog key and is translated when rendered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserMessage {
    pub key: String,
    pub params: serde_json::Map<String, serde_json::Value>,
    pub text: String,
}

impl std::fmt::Display for UserMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl UserMessage {
    /// Builds a message and renders its English text.
    pub fn new(key: &str, params: &[(&str, serde_json::Value)]) -> UserMessage {
        let mut map = serde_json::Map::new();
        for (k, v) in params {
            map.insert((*k).to_string(), v.clone());
        }
        let text = render_with(Lang::En, key, &map);
        UserMessage { key: key.to_string(), params: map, text }
    }
    /// Renders in the given language (falls back to English, then to the key).
    pub fn render(&self, lang: Lang) -> String {
        render_with(lang, &self.key, &self.params)
    }
}

/// `msg!("error.dns", host = h)` builds a [`UserMessage`].
#[macro_export]
macro_rules! msg {
    ($key:expr) => { $crate::catalog::UserMessage::new($key, &[]) };
    ($key:expr, $($name:ident = $val:expr),+ $(,)?) => {
        $crate::catalog::UserMessage::new($key, &[$((stringify!($name), serde_json::json!($val))),+])
    };
}

fn lookup(lang: Lang, key: &str) -> Option<&'static String> {
    table(lang).get(key).or_else(|| table(Lang::En).get(key))
}

/// Renders a key with `{name}` placeholders.
pub fn render_with(lang: Lang, key: &str, params: &serde_json::Map<String, serde_json::Value>) -> String {
    let Some(template) = lookup(lang, key) else { return key.to_string() };
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template.as_str();
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match params.get(name) {
                    Some(serde_json::Value::String(s)) if s.starts_with('@') => {
                        out.push_str(lookup(lang, &s[1..]).map(|t| t.as_str()).unwrap_or(&s[1..]))
                    }
                    Some(serde_json::Value::String(s)) => out.push_str(s),
                    Some(v) => out.push_str(&v.to_string()),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Human size for messages such as "at most 1 MB per message".
pub fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 && bytes.is_multiple_of(1024 * 1024) {
        format!("{} MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 && bytes.is_multiple_of(1024) {
        format!("{} KB", bytes / 1024)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} bytes")
    }
}
