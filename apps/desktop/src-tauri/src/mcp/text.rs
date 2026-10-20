//! Tool texts in the app language: the `mcp.*` keys of the shared catalog (packages/ui/src/i18n/<lang>/errors.json),
//! next to the error messages the tools reuse.

use mqttpanel_client::catalog::{render_with, Lang};
use mqttpanel_client::UserMessage;

/// Renders `mcp.<key>` with parameters.
pub fn t(lang: Lang, key: &str, params: &[(&str, serde_json::Value)]) -> String {
    let mut map = serde_json::Map::new();
    for (k, v) in params {
        map.insert((*k).to_string(), v.clone());
    }
    render_with(lang, &format!("mcp.{key}"), &map)
}

/// A catalog message (the app's errors) in the app language.
pub fn m(lang: Lang, msg: &UserMessage) -> String {
    msg.render(lang)
}

/// A user message for an `mcp.*` key (errors returned by tools).
pub fn um(key: &str, params: &[(&str, serde_json::Value)]) -> UserMessage {
    UserMessage::new(&format!("mcp.{key}"), params)
}
