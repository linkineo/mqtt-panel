//! User-facing messages: a catalog key, parameters and the English rendering.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The English catalog shared with the UI.
const EN_ERRORS: &str = include_str!("../../../packages/ui/src/i18n/en/errors.json");

fn catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(EN_ERRORS).unwrap_or_default())
}

/// Returns the English template of a catalog key, if it exists.
pub fn template(key: &str) -> Option<&'static str> {
    catalog().get(key).map(String::as_str)
}

/// A user-facing message: a catalog key plus parameters. `text` is the English rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    pub key: String,
    pub params: serde_json::Map<String, serde_json::Value>,
    pub text: String,
}

/// A parameter value of a [`UserMessage`] (string or number).
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Str(String),
    Num(f64),
}

impl From<&str> for Param {
    fn from(s: &str) -> Self {
        Param::Str(s.to_string())
    }
}
impl From<String> for Param {
    fn from(s: String) -> Self {
        Param::Str(s)
    }
}
impl From<i64> for Param {
    fn from(n: i64) -> Self {
        Param::Num(n as f64)
    }
}
impl From<u64> for Param {
    fn from(n: u64) -> Self {
        Param::Num(n as f64)
    }
}
impl From<f64> for Param {
    fn from(n: f64) -> Self {
        Param::Num(n)
    }
}

impl UserMessage {
    /// Builds a message for `key` and renders the English text from the catalog template.
    /// Unknown keys render as the key followed by the parameters.
    pub fn new(key: &str, params: Vec<(&str, Param)>) -> Self {
        let mut map = serde_json::Map::new();
        for (name, value) in params {
            let v = match value {
                Param::Str(s) => serde_json::Value::String(s),
                Param::Num(n) => serde_json::Number::from_f64(n)
                    .map(|num| {
                        if n.fract() == 0.0 && n.abs() < 9.0e15 {
                            serde_json::Value::from(n as i64)
                        } else {
                            serde_json::Value::Number(num)
                        }
                    })
                    .unwrap_or(serde_json::Value::Null),
            };
            map.insert(name.to_string(), v);
        }
        let text = render(key, &map);
        UserMessage {
            key: key.to_string(),
            params: map,
            text,
        }
    }
}

/// Renders `key` with `{name}` placeholders replaced by `params`.
pub fn render(key: &str, params: &serde_json::Map<String, serde_json::Value>) -> String {
    let param_text = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let Some(tpl) = template(key) else {
        let mut out = key.to_string();
        for (k, v) in params {
            out.push_str(&format!(" {k}={}", param_text(v)));
        }
        return out;
    };
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match params.get(name) {
                    Some(v) => out.push_str(&param_text(v)),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_protobuf_error() {
        let m = UserMessage::new(
            "decode.protobuf",
            vec![
                ("type", "plant.Telemetry".into()),
                ("detail", "field 3 wire type mismatch".into()),
            ],
        );
        assert_eq!(
            m.text,
            "Cannot decode as Protobuf plant.Telemetry: field 3 wire type mismatch"
        );
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["params"]["type"], "plant.Telemetry");
    }

    #[test]
    fn decode_keys_exist_in_catalog() {
        for key in [
            "decode.protobuf",
            "decode.sparkplug",
            "decode.cbor",
            "decode.msgpack",
            "decode.json",
            "decode.base64",
            "decode.notImage",
            "file.notProto",
        ] {
            assert!(template(key).is_some(), "{key} missing from the catalog");
        }
    }

    #[test]
    fn numbers_render_without_fraction() {
        let m = UserMessage::new("error.packetTooLarge", vec![("size", Param::from(1024u64))]);
        assert!(m.text.contains("1024"));
    }
}
