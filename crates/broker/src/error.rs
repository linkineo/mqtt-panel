//! User-facing errors of the broker: a catalog key, parameters and the English rendering.
//!
//! The English catalog is the UI's `errors.json`; this crate never invents sentences of its own.
//! Raw library errors never reach [`UserMessage::text`]; a short technical reason may be passed as a
//! `detail` parameter where the catalog sentence has one, and goes to the log otherwise.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

const EN_ERRORS: &str = include_str!("../../../packages/ui/src/i18n/en/errors.json");

fn catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(EN_ERRORS).unwrap_or_default())
}

/// True when the key exists in the English catalog.
pub fn catalog_has(key: &str) -> bool {
    catalog().contains_key(key)
}

/// A user-facing message (TS `UserMessage`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    pub key: String,
    pub params: serde_json::Map<String, serde_json::Value>,
    pub text: String,
}

impl UserMessage {
    /// Builds a message and renders the English text. Parameters are strings or numbers.
    pub fn new(key: &str, params: &[(&str, serde_json::Value)]) -> Self {
        let mut map = serde_json::Map::new();
        for (k, v) in params {
            map.insert((*k).to_string(), v.clone());
        }
        let text = render(key, &map);
        UserMessage { key: key.to_string(), params: map, text }
    }

    /// A message without parameters.
    pub fn plain(key: &str) -> Self {
        Self::new(key, &[])
    }
}

impl std::fmt::Display for UserMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

fn render(key: &str, params: &serde_json::Map<String, serde_json::Value>) -> String {
    let value_text = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let Some(tpl) = catalog().get(key) else {
        let mut out = key.to_string();
        for (k, v) in params {
            out.push_str(&format!(" {k}={}", value_text(v)));
        }
        return out;
    };
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl.as_str();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match params.get(name) {
                    Some(v) => out.push_str(&value_text(v)),
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

/// Shorthand for a string parameter.
pub fn s(v: impl Into<String>) -> serde_json::Value {
    serde_json::Value::String(v.into())
}

/// Shorthand for a number parameter.
pub fn n(v: impl Into<i64>) -> serde_json::Value {
    serde_json::Value::from(v.into())
}

/// An error of the broker crate: the user-facing message plus an optional technical detail for logs.
#[derive(Debug, Clone, PartialEq)]
pub struct BrokerError {
    pub message: UserMessage,
    /// Short technical reason (library error text), for logs only.
    pub detail: Option<String>,
}

impl BrokerError {
    pub fn new(key: &str, params: &[(&str, serde_json::Value)]) -> Self {
        BrokerError { message: UserMessage::new(key, params), detail: None }
    }
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    /// The user-facing message.
    pub fn user_message(&self) -> &UserMessage {
        &self.message
    }
    pub fn key(&self) -> &str {
        &self.message.key
    }
    /// "MQTT Panel hit an internal problem" with a short detail.
    pub fn internal(detail: impl Into<String>) -> Self {
        let d = detail.into();
        BrokerError::new("error.internal", &[("detail", s(d.clone()))]).with_detail(d)
    }
    /// The keychain could not be used.
    pub fn keychain(e: &mqttpanel_profiles::SecretError) -> Self {
        let d = e.to_string();
        BrokerError::new(e.catalog_key(), &[("detail", s(short_detail(&d)))]).with_detail(d)
    }
    /// A certificate or key file could not be used.
    pub fn tls_file(e: &mqttpanel_tls::TlsFileError, file: &str) -> Self {
        BrokerError::new(e.catalog_key(), &[("file", s(file))]).with_detail(e.to_string())
    }
    /// Certificate generation failed (should not happen with valid inputs).
    pub fn kit(e: &mqttpanel_tls::kit::KitError) -> Self {
        match e {
            mqttpanel_tls::kit::KitError::NotCsr => BrokerError::new("file.notCsr", &[]).with_detail(e.to_string()),
            _ => BrokerError::internal("certificate generation failed").with_detail(e.to_string()),
        }
    }
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message.text)
    }
}

impl std::error::Error for BrokerError {}

impl From<BrokerError> for UserMessage {
    fn from(e: BrokerError) -> Self {
        e.message
    }
}

/// Keeps a technical reason short and on one line (for `{detail}` parameters and log lines).
pub fn short_detail(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default().trim();
    if line.chars().count() > 120 {
        let cut: String = line.chars().take(117).collect();
        format!("{cut}...")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_catalog_sentence() {
        let m = UserMessage::new("broker.portInUseBy", &[("port", n(1883)), ("process", s("mosquitto (pid 42)"))]);
        assert_eq!(m.text, "Port 1883 is already in use by mosquitto (pid 42).");
        assert!(catalog_has("broker.portPrivileged"));
    }
}
