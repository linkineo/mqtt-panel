//! Embedded broker commands: the Broker screen and the "Connect to this app's broker" flow (Connections).

use crate::files;
use crate::state::{lock, AppState};
use crate::types::*;
use mqttpanel_broker::manager::DESKTOP_CERT_NAME;
use mqttpanel_broker::*;
use mqttpanel_client::{msg, UserMessage};
use mqttpanel_profiles::model::secret_keys;
use mqttpanel_profiles::{FileField, Profile, StoredProfile};
use std::sync::OnceLock;
use tauri::{AppHandle, State};

type R<T> = Result<T, UserMessage>;

/// The broker manager, opened on first use (its folder is shared with `mqttpanel broker`).
#[derive(Default)]
pub struct BrokerState {
    manager: OnceLock<Result<BrokerManager, UserMessage>>,
}

impl BrokerState {
    /// A broker state with an already opened manager (tests).
    #[cfg(test)]
    pub fn with_manager(m: BrokerManager) -> BrokerState {
        let s = BrokerState::default();
        let _ = s.manager.set(Ok(m));
        s
    }
}

pub fn broker_dir() -> std::path::PathBuf {
    mqttpanel_profiles::default_dir().join("broker")
}

fn um(m: mqttpanel_broker::UserMessage) -> UserMessage {
    UserMessage { key: m.key, params: m.params, text: m.text }
}

fn be(e: BrokerError) -> UserMessage {
    um(e.into())
}

/// The manager of a state built with [`BrokerState::with_manager`] (tests).
#[cfg(test)]
pub fn test_manager(s: &BrokerState) -> &BrokerManager {
    s.manager.get().unwrap().as_ref().unwrap()
}

fn mgr<'a>(state: &'a AppState) -> R<&'a BrokerManager> {
    state
        .broker
        .manager
        .get_or_init(|| BrokerManager::open(&broker_dir(), state.store.secrets().clone()).map_err(be))
        .as_ref()
        .map_err(|e| e.clone())
}

/// Status for the tick event (None until the Broker screen or the Connections flow opened the manager).
pub fn status_json(s: &BrokerState) -> Option<serde_json::Value> {
    match s.manager.get() {
        Some(Ok(m)) => serde_json::to_value(m.status()).ok(),
        _ => None,
    }
}

pub fn log_seq(s: &BrokerState) -> u64 {
    match s.manager.get() {
        Some(Ok(m)) => m.log_buffer().last_seq(),
        _ => 0,
    }
}

/// Stops the broker when the app quits.
pub async fn shutdown(s: &BrokerState) {
    if let Some(Ok(m)) = s.manager.get() {
        if m.is_running() {
            m.stop().await;
        }
    }
}

#[tauri::command]
pub fn broker_status(state: State<'_, AppState>) -> R<BrokerStatus> {
    Ok(mgr(&state)?.status())
}

#[tauri::command]
pub fn broker_config(state: State<'_, AppState>) -> R<BrokerConfig> {
    Ok(mgr(&state)?.config())
}

#[tauri::command]
pub async fn broker_start(state: State<'_, AppState>) -> R<BrokerStatus> {
    mgr(&state)?.start().await;
    if mgr(&state)?.take_ca_regenerated() {
        follow_local_profile(&state);
    }
    reconnect_local(&state).await;
    Ok(mgr(&state)?.status())
}

#[tauri::command]
pub async fn broker_stop(state: State<'_, AppState>) -> R<BrokerStatus> {
    Ok(mgr(&state)?.stop().await)
}

#[tauri::command]
pub fn validate_port(state: State<'_, AppState>, port: u32, listener_id: Option<String>) -> R<Option<UserMessage>> {
    Ok(mgr(&state)?.validate_port(port, listener_id.as_deref()).map(um))
}

// Every broker change applies at once while the broker runs (change set 7); the profile "Local broker" follows it.

#[tauri::command]
pub async fn add_listener(state: State<'_, AppState>, request: AddListenerRequest) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.add_listener(request).await;
    followed(&state, before, r).await
}

#[tauri::command]
pub async fn update_listener(state: State<'_, AppState>, listener: Listener) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.update_listener(listener).await;
    followed(&state, before, r).await
}

#[tauri::command]
pub async fn remove_listener(state: State<'_, AppState>, listener_id: String) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.remove_listener(&listener_id).await;
    followed(&state, before, r).await
}

/// Reachability of every listener, broker-wide (change set 4): the whole network or this computer only.
#[tauri::command]
pub async fn set_broker_bind(state: State<'_, AppState>, bind: Bind) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.set_bind(bind).await;
    followed(&state, before, r).await
}

#[tauri::command]
pub async fn set_anonymous(state: State<'_, AppState>, on: bool) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.set_anonymous(on);
    followed(&state, before, r).await
}

/// Client certificate on encrypted listeners, broker-wide: Required (true) or Optional (false).
#[tauri::command]
pub async fn set_require_client_cert(state: State<'_, AppState>, on: bool) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.set_require_client_cert(on);
    followed(&state, before, r).await
}

#[tauri::command]
pub async fn set_verify_against(state: State<'_, AppState>, source: VerifyAgainst) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.set_verify_against(source);
    followed(&state, before, r).await
}

#[tauri::command]
pub fn add_user(state: State<'_, AppState>, username: String, password: String) -> R<BrokerConfig> {
    mgr(&state)?.add_user(&username, &password).map_err(be)
}

#[tauri::command]
pub async fn set_user_password(state: State<'_, AppState>, username: String, password: String) -> R<BrokerConfig> {
    let before = local_fingerprint(&state);
    let r = mgr(&state)?.set_user_password(&username, &password);
    followed(&state, before, r).await
}

#[tauri::command]
pub fn remove_user(state: State<'_, AppState>, username: String) -> R<BrokerConfig> {
    mgr(&state)?.remove_user(&username).map_err(be)
}

/// The plain password of a broker user, for the eye and copy icons of the users table.
#[tauri::command]
pub fn broker_user_password(state: State<'_, AppState>, username: String) -> R<Option<String>> {
    mgr(&state)?.user_password(&username).map_err(be)
}

#[tauri::command]
pub fn issued_certs(state: State<'_, AppState>) -> R<Vec<IssuedCert>> {
    Ok(mgr(&state)?.issued_certs())
}

#[tauri::command]
pub async fn issue_client_cert(app: AppHandle, state: State<'_, AppState>, request: IssueCertRequest) -> R<SaveOutcome> {
    let files_ = mgr(&state)?.issue_client_cert(request).map_err(be)?;
    Ok(files::save_files(&app, &files_))
}

#[tauri::command]
pub async fn import_csr(app: AppHandle, state: State<'_, AppState>, name: Option<String>, valid_years: u32) -> R<SaveOutcome> {
    let Some(path) = files::pick_path(&app, "csr") else { return Ok(SaveOutcome::Cancelled) };
    let bytes = std::fs::read(&path).map_err(|_| msg!("file.unreadable", file = files::file_name(&path)))?;
    let years = valid_years.clamp(1, 5) as u8;
    let (fname, data) = mgr(&state)?.import_csr(&bytes, name.as_deref().filter(|n| !n.is_empty()), years).map_err(be)?;
    Ok(files::save_bytes(&app, &fname, &data))
}

#[tauri::command]
pub async fn export_client_cert(app: AppHandle, state: State<'_, AppState>, name: String, format: CertFormat, passphrase: String) -> R<SaveOutcome> {
    let files_ = mgr(&state)?.export_client_cert(&name, format, &passphrase).map_err(be)?;
    Ok(files::save_files(&app, &files_))
}

#[tauri::command]
pub fn revoke_client_cert(state: State<'_, AppState>, name: String) -> R<Vec<IssuedCert>> {
    mgr(&state)?.revoke_client_cert(&name).map_err(be)
}

#[tauri::command]
pub fn broker_certs(state: State<'_, AppState>) -> R<BrokerCerts> {
    mgr(&state)?.broker_certs().map_err(be)
}

#[tauri::command]
pub async fn export_ca(app: AppHandle, state: State<'_, AppState>) -> R<SaveOutcome> {
    let (name, data) = mgr(&state)?.export_ca();
    Ok(files::save_bytes(&app, &name, &data))
}

#[tauri::command]
pub async fn regenerate_broker_certs(state: State<'_, AppState>, sans: Vec<String>, key_type: mqttpanel_tls::kit::KeyAlg) -> R<BrokerCerts> {
    let before = local_fingerprint(&state);
    let c = mgr(&state)?.regenerate(&sans, key_type).map_err(be)?;
    follow_and_reconnect(&state, before).await;
    Ok(c)
}

#[tauri::command]
pub async fn import_broker_cert(app: AppHandle, state: State<'_, AppState>) -> R<Option<BrokerCerts>> {
    let Some(cert_path) = files::pick_path(&app, "brokerCert") else { return Ok(None) };
    let cert = std::fs::read(&cert_path).map_err(|_| msg!("file.unreadable", file = files::file_name(&cert_path)))?;
    if mqttpanel_tls::looks_like_pkcs12(&cert) {
        return mgr(&state)?.import_server_cert(&cert, None, None).map(Some).map_err(be);
    }
    let Some(key_path) = files::pick_path(&app, "brokerKey") else { return Ok(None) };
    let key = std::fs::read(&key_path).map_err(|_| msg!("file.unreadable", file = files::file_name(&key_path)))?;
    mgr(&state)?.import_server_cert(&cert, Some(&key), None).map(Some).map_err(be)
}

#[tauri::command]
pub async fn import_client_ca(app: AppHandle, state: State<'_, AppState>) -> R<Option<BrokerCerts>> {
    let Some(path) = files::pick_path(&app, "caCert") else { return Ok(None) };
    let bytes = std::fs::read(&path).map_err(|_| msg!("file.unreadable", file = files::file_name(&path)))?;
    mgr(&state)?.import_client_ca(&bytes).map(Some).map_err(be)
}

#[tauri::command]
pub async fn rotate_ca(state: State<'_, AppState>) -> R<BrokerCerts> {
    let before = local_fingerprint(&state);
    let c = mgr(&state)?.rotate_ca().map_err(be)?;
    follow_and_reconnect(&state, before).await;
    Ok(c)
}

#[tauri::command]
pub fn broker_clients(state: State<'_, AppState>) -> R<Vec<BrokerClient>> {
    Ok(mgr(&state)?.clients())
}

#[tauri::command]
pub fn disconnect_client(state: State<'_, AppState>, client_id: String) -> R<()> {
    mgr(&state)?.disconnect_client(&client_id).map_err(be)
}

#[tauri::command]
pub fn broker_log(state: State<'_, AppState>, level: LogLevel, after_seq: u64) -> R<Vec<LogLine>> {
    Ok(mgr(&state)?.log(level, after_seq))
}

/// One step of the "Connect to this app's broker" flow (mockup 02b, B5, B-12). The UI runs the steps in order and
/// ticks them off: `user`, `cert`, `trust`, `broker`, `connect`.
#[tauri::command]
pub async fn app_broker_step(state: State<'_, AppState>, listener_id: String, step: String, profile_name: String) -> R<AppBrokerStepResult> {
    app_broker_step_on(&state, &listener_id, &step, &profile_name).await
}

fn step_result(step: &str, result: &str) -> AppBrokerStepResult {
    AppBrokerStepResult { step: step.into(), result: result.into(), password: None, outcome: None }
}

/// The steps of the "Connect to this app's broker" flow, callable without Tauri (tests).
///
/// - `user`: creates the user "mqtt-panel" with a strong generated password, or keeps it (`skipped` with Anonymous on).
/// - `cert`: issues "mqttpanel-desktop" when the listener requires a client certificate (`kept` when valid, else
///   `skipped`).
/// - `trust`: creates or updates the single profile "Local broker" (CA, certificate, user, password reference);
///   `trusted` for TLS and WSS listeners, `skipped` otherwise. `profile_name` names a new profile only.
/// - `broker`: starts the broker when it is stopped (`started`), else `running`; nothing restarts (change set 7). A
///   running broker whose listener for this profile could not start tries it again.
/// - `connect`: connects the profile (`connected`, or `failed` with the outcome).
pub async fn app_broker_step_on(state: &AppState, listener_id: &str, step: &str, profile_name: &str) -> R<AppBrokerStepResult> {
    let m = mgr(state)?;
    if !m.config().listeners.iter().any(|l| l.id == listener_id) {
        return Err(msg!("broker.noListener"));
    }
    match step {
        "user" => {
            if m.anonymous() {
                return Ok(step_result(step, "skipped"));
            }
            let (created, pw) = m.ensure_app_user().map_err(be)?;
            let mut r = step_result(step, if created { "created" } else { "kept" });
            r.password = Some(pw);
            Ok(r)
        }
        "cert" => {
            if !m.needs_desktop_cert(listener_id) {
                return Ok(step_result(step, "skipped"));
            }
            let (issued, _, _) = m.ensure_desktop_cert().map_err(be)?;
            Ok(step_result(step, if issued { "issued" } else { "kept" }))
        }
        "trust" => {
            let lc = m.local_connection(Some(listener_id)).map_err(be)?;
            let tls = lc.ca_pem.is_some();
            save_local_profile(state, &lc, profile_name)?;
            Ok(step_result(step, if tls { "trusted" } else { "skipped" }))
        }
        "broker" => {
            let action = if m.is_running() { "running" } else { "started" };
            let mut st = m.status();
            let listener_failed = st.listeners.iter().any(|l| l.id == listener_id && l.error.is_some());
            if action == "started" || listener_failed {
                st = m.start().await;
            }
            if !m.is_running() {
                return Err(st.error.map(um).unwrap_or_else(|| msg!("broker.notRunning")));
            }
            if let Some(e) = st.listeners.iter().find(|l| l.id == listener_id).and_then(|l| l.error.clone()) {
                return Err(um(e));
            }
            Ok(step_result(step, action))
        }
        "connect" => {
            let Some(stored) = state.store.list().into_iter().find(|p| p.profile.is_local_broker) else {
                return Err(msg!("broker.noListener"));
            };
            let id = stored.profile.id.clone();
            // The listener may have changed: always start a fresh connection.
            if lock(&state.conns).contains_key(&id) {
                crate::commands::close(state, &id).await;
            }
            let outcome = crate::commands::connect_stored(state, stored, false).await;
            let mut r = step_result(step, if matches!(outcome, ConnectOutcome::Connected { .. }) { "connected" } else { "failed" });
            r.outcome = Some(outcome);
            Ok(r)
        }
        _ => Err(UserMessage { key: "internal".into(), params: Default::default(), text: format!("unknown step {step}") }),
    }
}

/// Creates or updates the single profile "Local broker" for a listener (never a second one).
pub fn save_local_profile(state: &AppState, lc: &LocalConnection, profile_name: &str) -> R<String> {
    let existing = state.store.list().into_iter().find(|p| p.profile.is_local_broker);
    let mut stored = existing.unwrap_or_else(|| {
        let mut p = Profile::new_default();
        p.name = if profile_name.trim().is_empty() { "Local broker".into() } else { profile_name.trim().into() };
        p.is_local_broker = true;
        p.client_id = DESKTOP_CERT_NAME.into();
        StoredProfile { profile: p, blobs: Default::default() }
    });
    let id = stored.profile.id.clone();
    let p = &mut stored.profile;
    p.target = "app".into();
    p.broker_listener_id = Some(lc.listener_id.clone());
    p.host = lc.host.clone();
    p.port = lc.port;
    p.scheme = lc.scheme.clone();
    p.ws_path = lc.ws_path.clone().unwrap_or_else(|| "/mqtt".into());
    p.mqtt_version = "5".into();
    p.tls.skip_verify = false;
    p.tls.trusted_fingerprints.clear();
    p.username = lc.username.clone().unwrap_or_default();
    let secrets = state.store.secrets().clone();
    let set = |k: String, v: &str| secrets.set(&k, v).map_err(|e| msg!(e.catalog_key(), detail = e.to_string()));
    match &lc.password {
        Some(pw) => set(secret_keys::password(&id), pw)?,
        None => {
            let _ = secrets.delete(&secret_keys::password(&id));
        }
    }
    p.has_password = lc.password.is_some();
    stored.blobs.ca_pem = lc.ca_pem.clone();
    stored.profile.tls.ca_cert = lc.ca_pem.as_ref().map(|pem| FileField {
        file_name: "CA.pem".into(),
        summary: mqttpanel_tls::inspect(pem.as_bytes(), mqttpanel_tls::FileExpect::Certificate, None).ok(),
        in_keychain: false,
    });
    let _ = secrets.delete(&secret_keys::pkcs12(&id));
    let _ = secrets.delete(&secret_keys::passphrase(&id));
    match (&lc.client_cert_pem, &lc.client_key_pem) {
        (Some(cert), Some(key)) => {
            set(secret_keys::client_key(&id), key)?;
            stored.blobs.client_cert_pem = Some(cert.clone());
            stored.profile.tls.client_cert = Some(FileField {
                file_name: format!("{DESKTOP_CERT_NAME}.pem"),
                summary: mqttpanel_tls::inspect(cert.as_bytes(), mqttpanel_tls::FileExpect::Certificate, None).ok(),
                in_keychain: false,
            });
            stored.profile.tls.client_key = Some(FileField { file_name: format!("{DESKTOP_CERT_NAME}.key"), summary: None, in_keychain: true });
        }
        _ => {
            let _ = secrets.delete(&secret_keys::client_key(&id));
            stored.blobs.client_cert_pem = None;
            stored.profile.tls.client_cert = None;
            stored.profile.tls.client_key = None;
        }
    }
    state.store.save_stored(stored)?;
    Ok(id)
}

/// A broker change followed by the profile "Local broker" (see [`follow_and_reconnect`]).
async fn followed(state: &AppState, before: Option<String>, r: Result<BrokerConfig, BrokerError>) -> R<BrokerConfig> {
    let c = r.map_err(be)?;
    follow_and_reconnect(state, before).await;
    Ok(mgr(state).map(|m| m.config()).unwrap_or(c))
}

/// What the connection of the profile "Local broker" depends on: its listener (kind, port, reachability) and its
/// login (user and password, client certificate and key, the CA it trusts, Anonymous). None without the profile.
pub fn local_fingerprint(state: &AppState) -> Option<String> {
    let m = mgr(state).ok()?;
    let stored = state.store.list().into_iter().find(|p| p.profile.is_local_broker)?;
    let p = &stored.profile;
    let secrets = state.store.secrets();
    let secret = |k: String| secrets.get(&k).ok().flatten().unwrap_or_default();
    let fp = serde_json::json!([
        p.broker_listener_id,
        p.scheme,
        p.host,
        p.port,
        p.ws_path,
        serde_json::to_value(m.config().bind).ok(),
        p.username,
        p.has_password,
        secret(secret_keys::password(&p.id)),
        stored.blobs.ca_pem,
        stored.blobs.client_cert_pem,
        secret(secret_keys::client_key(&p.id)),
    ]);
    // Compared in memory only, never stored or logged.
    Some(fp.to_string())
}

/// The profile "Local broker" follows the change at once and, when its connection is open, reconnects only when its
/// own listener or login changed (change set 7); any other change leaves it connected.
pub async fn follow_and_reconnect(state: &AppState, before: Option<String>) {
    follow_local_profile(state);
    if before.is_some() && local_fingerprint(state) != before {
        reconnect_local(state).await;
    }
}

/// The profile "Local broker" follows every broker change at once (owner, 13 Oct 2026, change set 2): its
/// listener's kind and port, the client-certificate requirement, a CA rotation, the "mqtt-panel" password and
/// Anonymous. When the profile's listener was removed or switched off, the profile moves to the most secure enabled
/// listener; with none, it stays as it is. Reconnecting is [`follow_and_reconnect`]'s job.
pub fn follow_local_profile(state: &AppState) {
    let Ok(m) = mgr(state) else { return };
    let Some(stored) = state.store.list().into_iter().find(|p| p.profile.is_local_broker) else { return };
    let cfg = m.config();
    let current = stored.profile.broker_listener_id.as_deref().and_then(|id| cfg.listeners.iter().find(|l| l.id == id && l.enabled));
    let listener = match current {
        Some(l) => l.clone(),
        None => match m.preferred_listener() {
            Some(l) => l,
            None => return,
        },
    };
    let Ok(mut lc) = m.local_connection(Some(&listener.id)) else { return };
    lc.port = listener.port;
    let _ = save_local_profile(state, &lc, "");
}

/// After a broker start or a change of its own settings: the profile "Local broker", when its connection was open
/// (connected, connecting or reconnecting), reconnects with its current settings, keeping its topics and history.
pub async fn reconnect_local(state: &AppState) {
    let Some(stored) = state.store.list().into_iter().find(|p| p.profile.is_local_broker) else { return };
    let id = stored.profile.id.clone();
    let Some(c) = lock(&state.conns).get(&id).cloned() else { return };
    if !c.is_open() {
        return;
    }
    let h = lock(&c.handle).take();
    if let Some(h) = h {
        h.disconnect().await;
    }
    lock(&c.link).state = "connecting".into();
    match crate::commands::connect_stored(state, stored, false).await {
        ConnectOutcome::Connected { .. } => {}
        ConnectOutcome::Failed { error } => {
            let mut l = lock(&c.link);
            l.state = "failed".into();
            l.error = Some(error);
        }
        ConnectOutcome::NeedsTrust { .. } => {
            let mut l = lock(&c.link);
            l.state = "failed".into();
            l.error = Some(msg!("tls.untrusted"));
        }
    }
}
