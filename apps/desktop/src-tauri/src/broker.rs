//! Embedded broker commands: the Broker screen and "Connect Explorer here".

use crate::files;
use crate::state::{lock, AppState};
use crate::types::*;
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

pub fn broker_dir() -> std::path::PathBuf {
    mqttpanel_profiles::default_dir().join("broker")
}

fn um(m: mqttpanel_broker::UserMessage) -> UserMessage {
    UserMessage { key: m.key, params: m.params, text: m.text }
}

fn be(e: BrokerError) -> UserMessage {
    um(e.into())
}

fn mgr<'a>(state: &'a AppState) -> R<&'a BrokerManager> {
    state
        .broker
        .manager
        .get_or_init(|| BrokerManager::open(&broker_dir(), state.store.secrets().clone()).map_err(be))
        .as_ref()
        .map_err(|e| e.clone())
}

/// Status for the tick event (None until the Broker screen or Connect Explorer here opened the manager).
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
    Ok(mgr(&state)?.start().await)
}

#[tauri::command]
pub async fn broker_stop(state: State<'_, AppState>) -> R<BrokerStatus> {
    Ok(mgr(&state)?.stop().await)
}

#[tauri::command]
pub async fn broker_restart(state: State<'_, AppState>) -> R<BrokerStatus> {
    Ok(mgr(&state)?.restart().await)
}

#[tauri::command]
pub fn validate_port(state: State<'_, AppState>, port: u32, listener_id: Option<String>) -> R<Option<UserMessage>> {
    Ok(mgr(&state)?.validate_port(port, listener_id.as_deref()).map(um))
}

#[tauri::command]
pub fn add_listener(state: State<'_, AppState>, request: AddListenerRequest) -> R<BrokerConfig> {
    mgr(&state)?.add_listener(request).map_err(be)
}

#[tauri::command]
pub fn update_listener(state: State<'_, AppState>, listener: Listener) -> R<BrokerConfig> {
    mgr(&state)?.update_listener(listener).map_err(be)
}

#[tauri::command]
pub fn remove_listener(state: State<'_, AppState>, listener_id: String) -> R<BrokerConfig> {
    mgr(&state)?.remove_listener(&listener_id).map_err(be)
}

#[tauri::command]
pub fn set_listener_login(state: State<'_, AppState>, listener_id: String, username: String, password: Option<String>) -> R<BrokerConfig> {
    mgr(&state)?.set_listener_login(&listener_id, &username, password.as_deref()).map_err(be)
}

#[tauri::command]
pub fn set_anonymous(state: State<'_, AppState>, on: bool) -> R<BrokerConfig> {
    mgr(&state)?.set_anonymous(on).map_err(be)
}

#[tauri::command]
pub fn set_verify_against(state: State<'_, AppState>, source: VerifyAgainst) -> R<BrokerConfig> {
    mgr(&state)?.set_verify_against(source).map_err(be)
}

#[tauri::command]
pub fn add_user(state: State<'_, AppState>, username: String, password: String) -> R<BrokerConfig> {
    mgr(&state)?.add_user(&username, &password).map_err(be)
}

#[tauri::command]
pub fn set_user_password(state: State<'_, AppState>, username: String, password: String) -> R<BrokerConfig> {
    mgr(&state)?.set_user_password(&username, &password).map_err(be)
}

#[tauri::command]
pub fn remove_user(state: State<'_, AppState>, username: String) -> R<BrokerConfig> {
    mgr(&state)?.remove_user(&username).map_err(be)
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
pub fn regenerate_broker_certs(state: State<'_, AppState>, sans: Vec<String>, key_type: mqttpanel_tls::kit::KeyAlg) -> R<BrokerCerts> {
    mgr(&state)?.regenerate(&sans, key_type).map_err(be)
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
pub fn rotate_ca(state: State<'_, AppState>) -> R<BrokerCerts> {
    mgr(&state)?.rotate_ca().map_err(be)
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

/// Creates or updates the single "Local broker" profile for a listener and connects (B5, B-12).
#[tauri::command]
pub async fn connect_explorer_here(state: State<'_, AppState>, listener_id: Option<String>) -> R<ConnectOutcome> {
    let m = mgr(&state)?;
    if !m.is_running() {
        let st = m.start().await;
        if st.error.is_some() && !m.is_running() {
            return Ok(ConnectOutcome::Failed { error: st.error.map(um).unwrap_or_else(|| msg!("broker.notRunning")) });
        }
    }
    let lc = m.local_connection(listener_id.as_deref()).map_err(be)?;
    let existing = state.store.list().into_iter().find(|p| p.profile.is_local_broker);
    let mut stored = existing.unwrap_or_else(|| {
        let mut p = Profile::new_default();
        p.name = "Local broker".into();
        p.is_local_broker = true;
        p.client_id = "mqttpanel-desktop".into();
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
    p.mqtt_version = "auto".into();
    p.tls.verify_server = true;
    p.tls.trusted_fingerprints.clear();
    p.username = lc.username.clone().unwrap_or_default();
    let secrets = state.store.secrets().clone();
    let set = |k: String, v: &str| secrets.set(&k, v).map_err(|e| msg!("keychain.unavailable", detail = e.to_string()));
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
                file_name: "mqttpanel-desktop.pem".into(),
                summary: mqttpanel_tls::inspect(cert.as_bytes(), mqttpanel_tls::FileExpect::Certificate, None).ok(),
                in_keychain: false,
            });
            stored.profile.tls.client_key = Some(FileField { file_name: "mqttpanel-desktop.key".into(), summary: None, in_keychain: true });
        }
        _ => {
            let _ = secrets.delete(&secret_keys::client_key(&id));
            stored.blobs.client_cert_pem = None;
            stored.profile.tls.client_cert = None;
            stored.profile.tls.client_key = None;
        }
    }
    state.store.save_stored(stored.clone())?;
    // A changed listener means a new connection: close the old one first.
    let open = lock(&state.conns).get(&id).cloned();
    if open.is_some() {
        crate::commands::close(&state, &id).await;
    }
    Ok(crate::commands::connect_stored(&state, state.store.get(&id).unwrap_or(stored)).await)
}
