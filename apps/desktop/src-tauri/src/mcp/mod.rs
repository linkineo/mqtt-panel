//! The assistant interface (docs/MCP.md): MQTT Panel is an MCP server on a local socket owned by the running app; a
//! host reaches it through the stdio shim (`mqttpanel mcp`, or this executable with the argument `mcp`).
//!
//! - `server`: the socket, one host session at a time, the rmcp handler (tools, resources, prompts).
//! - `tools`: the tool registry; each group (`connections`, `navigation`, later the broker, data and charts) is a table
//!   of [`tools::ToolDef`] the server reads without knowing the groups.
//! - `hosts`: "Add to Claude" and "Add to ChatGPT" (configuration files of the hosts).
//! - `commands`: the Tauri commands of Settings, Assistant.
//!
//! This file holds the [`Hub`]: the acknowledgement, the session, the Assistant log, the requests waiting for the user
//! in the app, and the [`AppLink`] through which the server reaches the window (events, raising it).

pub mod commands;
pub mod hosts;
pub mod prompts;
pub mod server;
pub mod text;
#[allow(clippy::result_large_err)]
pub mod tools;
#[cfg(test)]
mod tests;

use crate::state::{lock, AppState};
use mqttpanel_client::catalog::Lang;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::{oneshot, watch};

/// How long things may take (docs/MCP.md section 3, "Limits"); shorter in tests.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Any tool call.
    pub call: Duration,
    /// `connect`.
    pub connect: Duration,
    /// A call that waits for the user in the app (password prompt, file picker).
    pub user: Duration,
    /// A writing call that waits for the acknowledgement.
    pub ack: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { call: Duration::from_secs(30), connect: Duration::from_secs(20), user: Duration::from_secs(120), ack: Duration::from_secs(60) }
    }
}

/// What the app shows for the assistant (sent to the window as `EngineEvent::Assistant`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AssistantEvent {
    /// A toast for a writing call ("Assistant: connected Plant broker").
    Toast { text: String },
    /// Navigation: open a place in the window (the UI also shows "The assistant moved the view").
    #[serde(rename_all = "camelCase")]
    Navigate {
        screen: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        profile_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        topic: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        chart_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        page: Option<String>,
    },
    /// Data changed under the window (profiles, connections, broker): the UI refreshes them.
    Changed { what: String },
    /// The Assistant log has a new row or was cleared.
    LogChanged,
    /// A host connected or left.
    Session { connected: bool, host: Option<String> },
    /// Show the acknowledgement dialog (a host connected, or a writing call waits for it).
    Acknowledge,
    /// The acknowledgement was answered (the dialog closes in every window).
    AcknowledgeDone,
    /// Ask the user in the app: `password`, `caCert`, `clientCert` (certificate, key, passphrase) or `signedCert`.
    #[serde(rename_all = "camelCase")]
    Request { id: String, request: String, profile_id: String, profile_name: String },
    /// A request was answered, cancelled or timed out: the UI closes its prompt.
    RequestDone { id: String },
}

/// The answer to an [`AssistantEvent::Request`], from the app's prompt or picker. Secrets in it stay in the app.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantAnswer {
    pub cancelled: bool,
    pub password: Option<String>,
    /// The picked certificate (or PKCS#12 bundle, or CA, or signed certificate).
    pub path: Option<String>,
    /// The picked key file (client certificate request).
    pub key_path: Option<String>,
    pub passphrase: Option<String>,
}

/// One row of the Assistant log (Settings, Assistant); kept for the session of the app, never written to disk.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub at: i64,
    pub host: String,
    pub tool: String,
    /// The arguments with every secret field replaced by "•••".
    pub arguments: serde_json::Value,
    /// ok, refused, failed, cancelled or timeout.
    pub outcome: String,
    /// One sentence in the app language.
    pub detail: String,
    /// "secret given through the assistant", in the app language, when a secret came as an argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub const LOG_MAX: usize = 1000;

/// The window side of the assistant: the app's state, its events and its window. The Tauri app and the tests each
/// have one.
pub trait AppLink: Send + Sync + 'static {
    fn state(&self) -> &AppState;
    fn emit(&self, event: AssistantEvent);
    /// Brings the window forward without taking the focus from another app (docs/MCP.md section 9).
    fn raise_window(&self);
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AckAnswer {
    None,
    Accepted,
    Declined,
}

/// The current host session.
#[derive(Debug, Clone, Default)]
pub struct Session {
    pub host: String,
    /// The direct-secret warning was returned once in this session.
    pub warned: bool,
}

/// Assistant state shared by the server and the Tauri commands (a field of [`AppState`]).
pub struct Hub {
    lang: Mutex<Lang>,
    session: Mutex<Option<Session>>,
    log: Mutex<VecDeque<LogEntry>>,
    ack: watch::Sender<AckAnswer>,
    pending: Mutex<HashMap<String, oneshot::Sender<AssistantAnswer>>>,
    next_id: AtomicU64,
    pub limits: Mutex<Limits>,
}

impl Default for Hub {
    fn default() -> Self {
        Hub {
            lang: Mutex::new(Lang::from_env()),
            session: Mutex::new(None),
            log: Mutex::new(VecDeque::new()),
            ack: watch::channel(AckAnswer::None).0,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            limits: Mutex::new(Limits::default()),
        }
    }
}

impl Hub {
    /// The app language (the UI reports it with `set_menu_language` at start and on every change).
    pub fn lang(&self) -> Lang {
        *lock(&self.lang)
    }

    pub fn set_lang(&self, code: &str) {
        *lock(&self.lang) = Lang::from_locale(code);
    }

    pub fn limits(&self) -> Limits {
        *lock(&self.limits)
    }

    /// Opens a session for a host; false when another one is open ("another assistant is connected").
    pub fn open_session(&self) -> bool {
        let mut s = lock(&self.session);
        if s.is_some() {
            return false;
        }
        *s = Some(Session::default());
        true
    }

    pub fn set_host(&self, host: &str) {
        if let Some(s) = lock(&self.session).as_mut() {
            s.host = host.to_string();
        }
    }

    pub fn session(&self) -> Option<Session> {
        lock(&self.session).clone()
    }

    /// Ends the session; requests still waiting for the user are cancelled.
    pub fn close_session(&self) {
        *lock(&self.session) = None;
        for (_, tx) in lock(&self.pending).drain() {
            let _ = tx.send(AssistantAnswer { cancelled: true, ..Default::default() });
        }
    }

    /// True the first time it is called in a session (the direct-secret warning is returned once per session).
    pub fn take_warning(&self) -> bool {
        match lock(&self.session).as_mut() {
            Some(s) if !s.warned => {
                s.warned = true;
                true
            }
            Some(_) => false,
            None => true,
        }
    }

    pub fn host(&self) -> String {
        lock(&self.session).as_ref().map(|s| s.host.clone()).unwrap_or_default()
    }

    // ------------------------------------------------------------ log

    pub fn push_log(&self, e: LogEntry) {
        let mut log = lock(&self.log);
        log.push_back(e);
        while log.len() > LOG_MAX {
            log.pop_front();
        }
    }

    pub fn log(&self) -> Vec<LogEntry> {
        lock(&self.log).iter().cloned().collect()
    }

    pub fn clear_log(&self) {
        lock(&self.log).clear();
    }

    // ------------------------------------------------------------ acknowledgement

    /// The user accepted (Settings or the dialog): writing calls waiting for it go on.
    pub fn notify_ack(&self, accepted: bool) {
        let _ = self.ack.send(if accepted { AckAnswer::Accepted } else { AckAnswer::Declined });
    }

    // ------------------------------------------------------------ requests for the user

    fn new_request(&self) -> (String, oneshot::Receiver<AssistantAnswer>) {
        let id = format!("r{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id.clone(), tx);
        (id, rx)
    }

    /// The app answered a request; false when it is no longer waiting.
    pub fn answer(&self, id: &str, answer: AssistantAnswer) -> bool {
        match lock(&self.pending).remove(id) {
            Some(tx) => tx.send(answer).is_ok(),
            None => false,
        }
    }

}

/// Today's date, YYYY-MM-DD (UTC).
pub fn today() -> String {
    let d = time::OffsetDateTime::now_utc().date();
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

/// True once the acknowledgement was accepted.
pub fn acknowledged(state: &AppState) -> bool {
    lock(&state.settings).assistant_acknowledged.is_some()
}

/// Accepts the acknowledgement (persisted in the settings) and lets waiting writing calls go on.
pub fn accept(state: &AppState) -> Result<mqttpanel_profiles::Settings, mqttpanel_client::UserMessage> {
    let mut s = lock(&state.settings).clone();
    if s.assistant_acknowledged.is_none() {
        s.assistant_acknowledged = Some(today());
    }
    let saved = state.store.save_settings(&s)?;
    *lock(&state.settings) = saved.clone();
    state.assistant.notify_ack(true);
    Ok(saved)
}

/// Waits for the acknowledgement for a writing call: shows the dialog and waits up to `limits.ack`; false when it was
/// declined or not answered in time.
pub async fn wait_for_ack(link: &dyn AppLink) -> bool {
    let state = link.state();
    if acknowledged(state) {
        return true;
    }
    let hub = &state.assistant;
    let mut rx = hub.ack.subscribe();
    rx.borrow_and_update();
    link.emit(AssistantEvent::Acknowledge);
    link.raise_window();
    let limit = hub.limits().ack;
    let wait = async {
        loop {
            if rx.changed().await.is_err() {
                return false;
            }
            match *rx.borrow_and_update() {
                AckAnswer::Accepted => return true,
                AckAnswer::Declined => return false,
                AckAnswer::None => {}
            }
        }
    };
    let ok = tokio::time::timeout(limit, wait).await.unwrap_or(false);
    ok && acknowledged(state)
}

/// Asks the user in the app (a prompt or a picker) and waits up to `limits.user`. Err(true) when cancelled, Err(false)
/// when nobody answered in time.
pub async fn ask_user(link: &dyn AppLink, request: &str, profile_id: &str, profile_name: &str) -> Result<AssistantAnswer, bool> {
    let hub = &link.state().assistant;
    let (id, rx) = hub.new_request();
    link.emit(AssistantEvent::Request { id: id.clone(), request: request.into(), profile_id: profile_id.into(), profile_name: profile_name.into() });
    link.raise_window();
    let r = tokio::time::timeout(hub.limits().user, rx).await;
    lock(&hub.pending).remove(&id);
    link.emit(AssistantEvent::RequestDone { id });
    match r {
        Ok(Ok(a)) if !a.cancelled => Ok(a),
        Ok(_) => Err(true),
        Err(_) => Err(false),
    }
}
