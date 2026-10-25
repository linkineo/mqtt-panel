//! The tool registry. Each group is a table of [`ToolDef`]; [`registry`] concatenates them and the server reads only
//! the table (name, description, schema, `writes`, secret fields, timeout kind, handler): Connections, Broker, Data
//! (Explorer), Charts and Navigation.
//!
//! Handlers call the same functions as the Tauri commands (`crate::commands::*_on`, `crate::broker::*_on`) and return
//! an [`Outcome`]: one or two plain sentences in the app language, plus a JSON block. They never return a password, a
//! private key or a passphrase.

pub mod broker;
pub mod charts;
pub mod connections;
pub mod data;
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
    /// A tool that always waits for the user in the app (a folder for new key files): 120 s plus 30 s.
    User,
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
    all.extend(broker::tools());
    all.extend(data::tools());
    all.extend(charts::tools());
    all.extend(navigation::tools());
    all
}

/// Tools that may stop for the user's Allow or Deny in the app ([`Call::confirm`], docs/MCP.md section 3): their time
/// limit grows by `limits.confirm`. Only the cases listed there ask; every other call of these tools runs as before.
pub const CONFIRMING: &[&str] = &[
    "broker_set_anonymous",
    "broker_set_reachability",
    "trust_certificate",
    "update_connection",
    "export_data",
    "export_chart",
    "broker_export_ca",
    "broker_export_server_cert",
    "broker_issue_client_cert",
    "broker_set_allowed_origins",
];

/// One tool call: the app, the arguments, and a way to tell the host that the call waits for the user.
pub struct Call {
    pub link: Arc<dyn AppLink>,
    pub args: Map<String, Value>,
    pub waiting: Arc<dyn Fn() + Send + Sync>,
    /// Set when the user allowed a confirmation during this call (the Assistant log notes it).
    pub confirmed: Arc<std::sync::atomic::AtomicBool>,
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
    /// The user chose Deny, or nobody answered the confirmation in time (log outcome "denied").
    pub fn denied(no_answer: Option<u64>) -> Fail {
        let msg = match no_answer {
            Some(seconds) => text::um("deniedNoAnswer", &[("seconds", json!(seconds))]),
            None => text::um("denied", &[]),
        };
        Fail { msg, outcome: "denied" }
    }
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
    pub fn opt_i64(&self, k: &str) -> Result<Option<i64>, Fail> {
        match self.args.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_i64().map(Some).ok_or_else(|| Fail::args(&format!("{k} must be a whole number"))),
        }
    }
    /// A string argument that must be one of `allowed` (None when absent).
    pub fn opt_enum(&self, k: &str, allowed: &[&str]) -> Result<Option<String>, Fail> {
        match self.opt_str(k).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) if allowed.contains(&v.as_str()) => Ok(Some(v)),
            Some(_) => Err(Fail::args(&format!("{k} must be one of {}", allowed.join(", ")))),
        }
    }
    /// The open (or recently open) connection named by `key`: its profile and its engine.
    pub fn connection(&self, key: &str) -> Result<(mqttpanel_profiles::Profile, Arc<crate::state::Conn>), Fail> {
        let p = self.profile(key)?.profile;
        let c = crate::state::lock(&self.state().conns).get(&p.id).cloned();
        match c {
            Some(c) => Ok((p, c)),
            None => Err(Fail::from(text::um("notConnected", &[("name", json!(p.name))]))),
        }
    }
    /// Asks the user in the app; maps a cancel or no answer to the failures of the log.
    pub async fn ask(&self, a: super::Ask) -> Result<super::AssistantAnswer, Fail> {
        (self.waiting)();
        match super::ask_with(self.link.as_ref(), a).await {
            Ok(a) => Ok(a),
            Err(true) => Err(Fail::cancelled()),
            Err(false) => Err(Fail::no_answer(self.state().assistant.limits().user.as_secs())),
        }
    }
    /// Asks the user in the app to Allow or Deny (see [`super::confirm`]); Ok when allowed, the "denied" failure
    /// otherwise. Call it only for the cases listed in docs/MCP.md section 3, and add the tool to [`CONFIRMING`].
    pub async fn confirm(&self, action: &'static str, name: &str, params: &[(&'static str, Value)]) -> Result<(), Fail> {
        (self.waiting)();
        let req = super::ConfirmRequest { action, params: params.to_vec(), name: name.to_string() };
        match super::confirm(self.link.as_ref(), req).await {
            super::Confirmation::Allowed => {
                self.confirmed.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
            super::Confirmation::Denied => Err(Fail::denied(None)),
            super::Confirmation::NoAnswer => Err(Fail::denied(Some(self.state().assistant.limits().confirm.as_secs()))),
        }
    }
    /// Before writing `path`: an existing file is overwritten only after the user allows it; a new file at a `direct`
    /// path (given by the host, not picked in the app) outside the usual folders ([`super::safe_roots`]) too.
    pub async fn check_write(&self, path: &std::path::Path, direct: bool) -> Result<(), Fail> {
        let file = json!(shown(path));
        if path.exists() {
            return self.confirm("overwrite", &crate::files::file_name(path), &[("file", file)]).await;
        }
        if direct && !super::inside(path, &super::safe_roots(self.state())) {
            return self.confirm("writeOutside", &crate::files::file_name(path), &[("file", file)]).await;
        }
        Ok(())
    }
    /// The file an export goes to: the direct `file` argument (an absolute path; not a secret), or the save dialog in
    /// the app with `default_name`. A direct path is checked with [`Call::check_write`] (the save dialog asks itself
    /// before replacing a file).
    pub async fn save_target(&self, default_name: &str, what: &str) -> Result<std::path::PathBuf, Fail> {
        if let Some(f) = self.opt_str("file").filter(|f| !f.trim().is_empty()) {
            let path = absolute(&f, false)?;
            self.check_write(&path, true).await?;
            return Ok(path);
        }
        if !self.ask_in_app() {
            return Err(need("file"));
        }
        let a = self.ask(super::Ask { request: "saveFile".into(), name: what.into(), default_name: Some(default_name.into()), ..Default::default() }).await?;
        a.path.filter(|p| !p.is_empty()).map(std::path::PathBuf::from).ok_or_else(Fail::cancelled)
    }
    /// The folder files go to: the direct `folder` argument, or the folder dialog in the app; true when direct (check
    /// each file with [`Call::check_write`]).
    pub async fn folder_target(&self, default_name: &str, what: &str) -> Result<(std::path::PathBuf, bool), Fail> {
        if let Some(f) = self.opt_str("folder").filter(|f| !f.trim().is_empty()) {
            return Ok((absolute(&f, true)?, true));
        }
        if !self.ask_in_app() {
            return Err(need("folder"));
        }
        let a = self.ask(super::Ask { request: "folder".into(), name: what.into(), default_name: Some(default_name.into()), ..Default::default() }).await?;
        a.path.filter(|p| !p.is_empty()).map(|p| (std::path::PathBuf::from(p), false)).ok_or_else(Fail::cancelled)
    }
    /// The profile named by `key` (an id, or a connection name).
    pub fn profile(&self, key: &str) -> Result<mqttpanel_profiles::StoredProfile, Fail> {
        let needle = self.str(key)?;
        self.state().store.find(&needle).ok_or_else(|| Fail::from(text::um("noConnection", &[("name", json!(needle))])))
    }
}

/// "Give {field}, or set ask_in_app to true so MQTT Panel asks the user."
pub fn need(field: &str) -> Fail {
    Fail::from(text::um("needSecret", &[("field", json!(field))]))
}

/// A path given by the host: absolute, its folder existing (a folder itself when `dir`).
pub fn absolute(p: &str, dir: bool) -> Result<std::path::PathBuf, Fail> {
    let path = std::path::PathBuf::from(p.trim());
    if !path.is_absolute() {
        return Err(Fail::args(if dir { "folder must be an absolute path" } else { "file must be an absolute path" }));
    }
    let ok = if dir { path.is_dir() } else { path.parent().is_some_and(|d| d.is_dir()) && !path.is_dir() };
    if !ok {
        return Err(Fail::args(if dir { "folder does not exist" } else { "the folder of file does not exist" }));
    }
    Ok(path)
}

/// A path for the chat and the log.
pub fn shown(p: &std::path::Path) -> String {
    p.to_string_lossy().to_string()
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

/// The `connection` property of the data and chart tools.
pub fn conn_prop() -> Value {
    json!({ "type": "string", "description": "The connection id or name from list_connections; it must be connected (or was connected in this session)." })
}

pub fn file_prop() -> Value {
    json!({ "type": "string", "description": "Direct way: an absolute file path on this computer. Leave it out to let the user choose in MQTT Panel's save dialog. Replacing an existing file, or a path outside the user's Documents, Downloads, Desktop and temporary folders, waits until the user allows it in MQTT Panel." })
}

pub fn folder_prop() -> Value {
    json!({ "type": "string", "description": "Direct way: an absolute folder path on this computer. Leave it out to let the user choose the folder in MQTT Panel. Replacing an existing file, or a folder outside the user's Documents, Downloads, Desktop and temporary folders, waits until the user allows it in MQTT Panel." })
}

pub fn ask_prop() -> Value {
    json!({ "type": "boolean", "default": true, "description": "Recommended (default true): MQTT Panel asks the user in its own window, so the value never passes through the chat." })
}
