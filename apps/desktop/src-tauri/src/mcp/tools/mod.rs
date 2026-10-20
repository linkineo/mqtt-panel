//! The tool registry. Each group is a table of [`ToolDef`]; [`registry`] concatenates them and the server reads only
//! the table (name, description, schema, `writes`, secret fields, timeout kind, handler). Phase 3 adds the broker,
//! data and chart groups here without touching the server.
//!
//! Handlers call the same functions as the Tauri commands (`crate::commands::*_on`, `crate::broker::*_on`) and return
//! an [`Outcome`]: one or two plain sentences in the app language, plus a JSON block. They never return a password, a
//! private key or a passphrase.

pub mod connections;
pub mod navigation;

use super::text;
use super::AppLink;
use mqttpanel_client::UserMessage;
use serde_json::{json, Map, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub type Handler = fn(Call) -> BoxFuture<Result<Outcome, Fail>>;

/// Which limit applies to a call (docs/MCP.md section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// 30 s.
    Call,
    /// `connect`: 20 s.
    Connect,
    /// A tool with `ask_in_app`: up to 120 s for the user when it asks in the app, 30 s otherwise.
    MaybeUser,
}

pub struct ToolDef {
    pub name: &'static str,
    /// For the model (English, plain words); says when a value travels through the chat.
    pub description: &'static str,
    pub schema: fn() -> Value,
    /// Changes something: needs the acknowledgement, shows a toast, adds a row to the Assistant log.
    pub writes: bool,
    /// Removes something (host hint only).
    pub destructive: bool,
    /// Arguments that carry a secret or the path of a secret file: masked in the log; giving one directly brings the
    /// warning once per session and the log note.
    pub secret_fields: &'static [&'static str],
    pub wait: Wait,
    pub handler: Handler,
}

impl ToolDef {
    #[cfg(test)]
    pub fn touches_secret(&self) -> bool {
        !self.secret_fields.is_empty()
    }
}

/// Every tool of this build, in the order the host lists them.
pub fn registry() -> Vec<ToolDef> {
    let mut all = connections::tools();
    all.extend(navigation::tools());
    all
}

/// One tool call: the app, the arguments, and a way to tell the host that the call waits for the user.
pub struct Call {
    pub link: Arc<dyn AppLink>,
    pub args: Map<String, Value>,
    pub waiting: Arc<dyn Fn() + Send + Sync>,
}

/// What a handler returns: sentences for the chat and the toast, a JSON block, and what changed under the window.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub text: String,
    pub data: Value,
    /// The toast after a writing call (defaults to the text).
    pub toast: Option<String>,
    /// profiles, connections, broker: the window refreshes them.
    pub changed: Vec<&'static str>,
}

impl Outcome {
    pub fn new(text: String, data: Value) -> Outcome {
        Outcome { text, data, toast: None, changed: vec![] }
    }
    pub fn changed(mut self, what: &'static str) -> Outcome {
        self.changed.push(what);
        self
    }
}

/// A failed call; `outcome` is the word in the Assistant log.
#[derive(Debug, Clone)]
pub struct Fail {
    pub msg: UserMessage,
    pub outcome: &'static str,
}

impl From<UserMessage> for Fail {
    fn from(msg: UserMessage) -> Fail {
        Fail { msg, outcome: "failed" }
    }
}

impl Fail {
    pub fn cancelled() -> Fail {
        Fail { msg: text::um("userCancelled", &[]), outcome: "cancelled" }
    }
    pub fn no_answer(seconds: u64) -> Fail {
        Fail { msg: text::um("userTimeout", &[("seconds", json!(seconds))]), outcome: "timeout" }
    }
    pub fn args(detail: &str) -> Fail {
        Fail { msg: text::um("badArgs", &[("detail", json!(detail))]), outcome: "failed" }
    }
}

// ---------------------------------------------------------------- argument helpers

impl Call {
    pub fn state(&self) -> &crate::state::AppState {
        self.link.state()
    }
    pub fn lang(&self) -> mqttpanel_client::catalog::Lang {
        self.state().assistant.lang()
    }
    pub fn t(&self, key: &str, params: &[(&str, Value)]) -> String {
        text::t(self.lang(), key, params)
    }
    pub fn str(&self, k: &str) -> Result<String, Fail> {
        match self.args.get(k) {
            Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
            _ => Err(Fail::args(&format!("{k} is required"))),
        }
    }
    pub fn opt_str(&self, k: &str) -> Option<String> {
        match self.args.get(k) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        }
    }
    pub fn opt_bool(&self, k: &str) -> Option<bool> {
        self.args.get(k).and_then(Value::as_bool)
    }
    pub fn opt_u64(&self, k: &str) -> Result<Option<u64>, Fail> {
        match self.args.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_u64().map(Some).ok_or_else(|| Fail::args(&format!("{k} must be a whole number"))),
        }
    }
    /// `ask_in_app` (default true).
    pub fn ask_in_app(&self) -> bool {
        self.opt_bool("ask_in_app").unwrap_or(true)
    }
    /// The profile named by `key` (an id, or a connection name).
    pub fn profile(&self, key: &str) -> Result<mqttpanel_profiles::StoredProfile, Fail> {
        let needle = self.str(key)?;
        self.state().store.find(&needle).ok_or_else(|| Fail::from(text::um("noConnection", &[("name", json!(needle))])))
    }
}

/// True when a call gives a secret directly (one of the tool's secret fields is present and not empty).
pub fn gives_secret(def: &ToolDef, args: &Map<String, Value>) -> bool {
    def.secret_fields.iter().any(|f| matches!(args.get(*f), Some(Value::String(s)) if !s.is_empty()))
}

/// The arguments for the log, with secret fields replaced by "•••".
pub fn masked(def: &ToolDef, args: &Map<String, Value>) -> Value {
    let mut out = args.clone();
    for f in def.secret_fields {
        if let Some(v) = out.get_mut(*f) {
            if !v.is_null() {
                *v = json!("•••");
            }
        }
    }
    Value::Object(out)
}

/// A JSON schema object for the tool list.
pub fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

/// The `id` property every connection tool takes.
pub fn id_prop() -> Value {
    json!({ "type": "string", "description": "The connection id from list_connections (a connection name also works)." })
}

pub fn ask_prop() -> Value {
    json!({ "type": "boolean", "default": true, "description": "Recommended (default true): MQTT Panel asks the user in its own window, so the value never passes through the chat." })
}
