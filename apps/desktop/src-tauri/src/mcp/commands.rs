//! Tauri commands of Settings, Assistant (docs/MCP.md section 6), and the window side of the server ([`TauriLink`]).

use super::hosts::{self, HostFile, Homes, WriteReport};
use super::{AppLink, AssistantAnswer, AssistantEvent, LogEntry};
use crate::state::{lock, AppState};
use crate::types::{EngineEvent, SaveOutcome};
use mqttpanel_client::UserMessage;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

type R<T> = Result<T, UserMessage>;

/// The app as the server sees it: its state, its events, its main window.
pub struct TauriLink(pub AppHandle);

impl AppLink for TauriLink {
    fn state(&self) -> &AppState {
        self.0.state::<AppState>().inner()
    }

    fn emit(&self, event: AssistantEvent) {
        let _ = self.0.emit("engine-event", EngineEvent::Assistant { event });
    }

    /// Shows the window; when another app is frontmost, the window comes forward for a moment above the others
    /// without being activated (the keyboard stays with the other app).
    fn raise_window(&self) {
        let Some(w) = self.0.get_webview_window("main") else { return };
        let _ = w.unminimize();
        let _ = w.show();
        if !w.is_focused().unwrap_or(false) {
            let _ = w.set_always_on_top(true);
            let _ = w.set_always_on_top(false);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStatus {
    /// The day the acknowledgement was accepted (YYYY-MM-DD), or null.
    pub acknowledged: Option<String>,
    pub connected: bool,
    pub host: Option<String>,
    /// The executable the hosts start (with the argument `mcp`).
    pub executable: String,
    pub snippet: String,
    pub hosts: Vec<HostFile>,
    /// App Sandbox (Mac App Store variant): the buttons cannot write the hosts' files; the snippet is the way.
    pub sandboxed: bool,
}

pub fn status_of(state: &AppState) -> AssistantStatus {
    let exe = hosts::executable();
    let session = state.assistant.session();
    let homes = Homes::detect();
    AssistantStatus {
        acknowledged: lock(&state.settings).assistant_acknowledged.clone(),
        connected: session.is_some(),
        host: session.map(|s| s.host).filter(|h| !h.is_empty()),
        snippet: hosts::snippet(&exe),
        hosts: hosts::host_files(&homes, &exe),
        sandboxed: homes.sandboxed,
        executable: exe,
    }
}

#[tauri::command]
pub fn assistant_status(state: State<'_, AppState>) -> AssistantStatus {
    status_of(&state)
}

/// "Accept" (Settings or the dialog): persisted; writing calls waiting for it go on.
#[tauri::command]
pub fn assistant_accept(app: AppHandle, state: State<'_, AppState>) -> R<AssistantStatus> {
    super::accept(&state)?;
    let _ = app.emit("engine-event", EngineEvent::Assistant { event: AssistantEvent::AcknowledgeDone });
    Ok(status_of(&state))
}

/// "Not now" in the dialog: writing calls waiting for it are refused at once.
#[tauri::command]
pub fn assistant_decline(app: AppHandle, state: State<'_, AppState>) {
    state.assistant.notify_ack(false);
    let _ = app.emit("engine-event", EngineEvent::Assistant { event: AssistantEvent::AcknowledgeDone });
}

#[tauri::command]
pub fn assistant_add_to_claude() -> WriteReport {
    hosts::add_to_claude(&Homes::detect(), &hosts::executable())
}

#[tauri::command]
pub fn assistant_add_to_chatgpt() -> WriteReport {
    hosts::add_to_chatgpt(&Homes::detect(), &hosts::executable())
}

#[tauri::command]
pub fn assistant_log(state: State<'_, AppState>) -> Vec<LogEntry> {
    state.assistant.log()
}

#[tauri::command]
pub fn assistant_clear_log(app: AppHandle, state: State<'_, AppState>) {
    state.assistant.clear_log();
    let _ = app.emit("engine-event", EngineEvent::Assistant { event: AssistantEvent::LogChanged });
}

/// "Export…": the log as a text file through the save dialog.
#[tauri::command]
pub async fn assistant_export_log(app: AppHandle, state: State<'_, AppState>) -> R<SaveOutcome> {
    let text = super::prompts::log_text(state.assistant.lang(), &state.assistant.log());
    Ok(crate::files::save_bytes(&app, "mqttpanel-assistant-log.txt", text.as_bytes()))
}

/// "Choose…" in an assistant request to save an export: the native save dialog with the default name; the path.
#[tauri::command]
pub async fn assistant_pick_save(app: AppHandle, default_name: String) -> Option<String> {
    crate::files::save_path(&app, &default_name).map(|p| p.to_string_lossy().to_string())
}

/// "Choose…" in an assistant request for a folder (CA.pem, server certificate, a new client certificate).
#[tauri::command]
pub async fn assistant_pick_folder(app: AppHandle) -> Option<String> {
    crate::files::pick_folder(&app).map(|p| p.to_string_lossy().to_string())
}

/// The app's answer to a request of the assistant (password prompt, file picker); false when nobody waits any more.
#[tauri::command]
pub fn assistant_answer(state: State<'_, AppState>, request_id: String, answer: AssistantAnswer) -> bool {
    state.assistant.answer(&request_id, answer)
}
