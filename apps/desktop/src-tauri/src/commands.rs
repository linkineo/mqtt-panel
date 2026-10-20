//! Tauri commands: one per EngineClient method (packages/engine-client/src/client.ts), snake_case names.

use crate::files;
use crate::state::*;
use crate::types::*;
use base64::Engine as _;
use mqttpanel_client::{msg, ConnectFailure, Outgoing, Props5, UserMessage};
use mqttpanel_engine::*;
use mqttpanel_profiles::model::secret_keys;
use mqttpanel_profiles::{FileField, Profile, ProtoSchema, Settings, StoredProfile, Template};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

type R<T> = Result<T, UserMessage>;

fn conn(state: &AppState, id: &str) -> R<Arc<Conn>> {
    lock(&state.conns).get(id).cloned().ok_or_else(|| msg!("error.notConnected"))
}

// ---------------------------------------------------------------- app and settings

#[tauri::command]
pub fn app_info() -> AppInfo {
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    AppInfo { version: env!("CARGO_PKG_VERSION").into(), platform: platform.into(), native: true }
}

/// Where the master key lives in this build: "file" or "keychain" (change set 10; the ⓘ next to the key fields says it).
#[tauri::command]
pub fn key_store_kind() -> &'static str {
    mqttpanel_profiles::key_store_kind()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    lock(&state.settings).clone()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> R<Settings> {
    let saved = state.store.save_settings(&settings)?;
    *lock(&state.settings) = saved.clone();
    let cfg = engine_config(&saved);
    for c in lock(&state.conns).values() {
        lock(&c.engine).set_config(cfg.clone());
    }
    Ok(saved)
}

// ---------------------------------------------------------------- profiles

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Vec<Profile> {
    state.store.profiles()
}

#[tauri::command]
pub fn new_profile() -> Profile {
    Profile::new_default()
}

#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: Profile) -> R<Profile> {
    let saved = state.store.save(profile)?;
    if let Some(c) = lock(&state.conns).get(&saved.id) {
        *lock(&c.name) = saved.name.clone();
        // "Use as sparkline" applies to the open connection at once (change set 8).
        lock(&c.engine).set_sparkline_paths(saved.sparkline_paths());
    }
    Ok(saved)
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, profile_id: String) -> R<()> {
    close(&state, &profile_id).await;
    state.store.delete(&profile_id)
}

#[tauri::command]
pub fn duplicate_profile(state: State<'_, AppState>, profile_id: String) -> R<Profile> {
    state.store.duplicate(&profile_id)
}

#[tauri::command]
pub fn reorder_profiles(state: State<'_, AppState>, ids: Vec<String>) -> R<()> {
    state.store.reorder(&ids)
}

#[tauri::command]
pub fn set_password(state: State<'_, AppState>, profile_id: String, password: String) -> R<Profile> {
    state.store.set_password(&profile_id, &password)
}

/// The stored password of a profile, for the eye of the password field (change set 11: every profile; the copy icon
/// stays reserved for the password the app generated for "Local broker"). Read from the secret store only when the
/// eye is pressed.
#[tauri::command]
pub fn profile_password(state: State<'_, AppState>, profile_id: String) -> R<Option<String>> {
    stored_password(&state, &profile_id)
}

pub fn stored_password(state: &AppState, profile_id: &str) -> R<Option<String>> {
    stored(state, profile_id)?;
    Ok(state.store.password(profile_id))
}

#[tauri::command]
pub async fn import_profiles(app: AppHandle, state: State<'_, AppState>, path: Option<String>) -> R<Option<ImportSummary>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => match files::pick_path(&app, "import") {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    let bytes = std::fs::read(&path).map_err(|_| msg!("file.unreadable", file = files::file_name(&path)))?;
    let r = mqttpanel_profiles::import::parse_import(&bytes, None)?;
    let imported = state.store.add_imported(r.profiles, r.secrets)?;
    Ok(Some(ImportSummary {
        imported,
        skipped: r.skipped.into_iter().map(|(name, reason)| ImportSkip { name, reason }).collect(),
        notes: r.notes,
    }))
}

#[tauri::command]
pub async fn export_profiles(app: AppHandle, state: State<'_, AppState>, passphrase: Option<String>) -> R<SaveOutcome> {
    let data = state.store.export(passphrase.as_deref());
    Ok(files::save_bytes(&app, "mqttpanel-profiles.json", &data))
}

// ---------------------------------------------------------------- files

#[tauri::command]
pub async fn pick_file(app: AppHandle, kind: String) -> R<Option<FileInspection>> {
    Ok(files::pick_path(&app, &kind).map(|p| files::inspect(&kind, &p, None)))
}

#[tauri::command]
pub fn inspect_file(kind: String, path: String) -> FileInspection {
    files::inspect(&kind, &PathBuf::from(path), None)
}

#[tauri::command]
pub fn set_profile_file(state: State<'_, AppState>, profile_id: String, field: String, path: Option<String>, passphrase: Option<String>) -> R<Profile> {
    let secrets = state.store.secrets().clone();
    let set = |k: String, v: &str| secrets.set(&k, v).map_err(|e| msg!(e.catalog_key(), detail = e.to_string()));
    let del = |k: String| {
        let _ = secrets.delete(&k);
    };
    let id = profile_id.clone();
    let Some(path) = path.map(PathBuf::from) else {
        let p = state.store.update(&profile_id, |p| match field.as_str() {
            "caCert" => {
                p.profile.tls.ca_cert = None;
                p.blobs.ca_pem = None;
            }
            "clientCert" => {
                p.profile.tls.client_cert = None;
                p.blobs.client_cert_pem = None;
            }
            _ => {
                p.profile.tls.client_key = None;
            }
        })?;
        // Change set 11: in the generated way, removing the signed certificate keeps the generated key.
        if field != "caCert" && !(field == "clientCert" && p.profile.tls.client_key_generated) {
            if field == "clientKey" || p.profile.tls.client_cert.is_none() {
                del(secret_keys::client_key(&id));
                del(secret_keys::pkcs12(&id));
                del(secret_keys::passphrase(&id));
            }
        }
        return Ok(p.profile);
    };
    let name = files::file_name(&path);
    let bytes = std::fs::read(&path).map_err(|_| msg!("file.unreadable", file = name.clone()))?;
    let pass = passphrase.as_deref().filter(|p| !p.is_empty());
    let field_of = |s: mqttpanel_tls::CertSummary, kc: bool| FileField { file_name: name.clone(), summary: Some(s), in_keychain: kc };
    if field != "caCert" && mqttpanel_tls::looks_like_pkcs12(&bytes) {
        let id_ = mqttpanel_tls::parse_pkcs12(&bytes, pass).map_err(|e| files::tls_error(&e, &name))?;
        let _ = id_;
        let summary = mqttpanel_tls::inspect(&bytes, mqttpanel_tls::FileExpect::CertificateOrPkcs12, pass).map_err(|e| files::tls_error(&e, &name))?;
        set(secret_keys::pkcs12(&id), &base64::engine::general_purpose::STANDARD.encode(&bytes))?;
        match pass {
            Some(p) => set(secret_keys::passphrase(&id), p)?,
            None => del(secret_keys::passphrase(&id)),
        }
        del(secret_keys::client_key(&id));
        let p = state.store.update(&profile_id, |p| {
            p.blobs.client_cert_pem = None;
            p.profile.tls.client_cert = Some(field_of(summary.clone(), true));
            p.profile.tls.client_key = Some(field_of(summary, true));
        })?;
        return Ok(p.profile);
    }
    match field.as_str() {
        "caCert" | "clientCert" => {
            let certs = mqttpanel_tls::parse_certs(&bytes).map_err(|e| files::tls_error(&e, &name))?;
            let summary = mqttpanel_tls::inspect(&bytes, mqttpanel_tls::FileExpect::Certificate, None).map_err(|e| files::tls_error(&e, &name))?;
            let pem: String = certs.iter().map(|c| mqttpanel_tls::to_pem("CERTIFICATE", c.as_ref())).collect();
            let is_ca = field == "caCert";
            if !is_ca {
                del(secret_keys::pkcs12(&id));
            }
            let p = state.store.update(&profile_id, |p| {
                if is_ca {
                    p.blobs.ca_pem = Some(pem);
                    p.profile.tls.ca_cert = Some(field_of(summary, false));
                } else {
                    p.blobs.client_cert_pem = Some(pem);
                    p.profile.tls.client_cert = Some(field_of(summary, false));
                }
            })?;
            Ok(p.profile)
        }
        _ => {
            let key = mqttpanel_tls::parse_private_key(&bytes, pass).map_err(|e| files::tls_error(&e, &name))?;
            let summary = mqttpanel_tls::inspect(&bytes, mqttpanel_tls::FileExpect::KeyOrPkcs12, pass).map_err(|e| files::tls_error(&e, &name))?;
            // Store the decrypted key as PKCS#8 / SEC1 / PKCS#1 PEM in the keychain.
            let (label, der): (&str, Vec<u8>) = match &key {
                rustls_pki_types::PrivateKeyDer::Pkcs1(k) => ("RSA PRIVATE KEY", k.secret_pkcs1_der().to_vec()),
                rustls_pki_types::PrivateKeyDer::Sec1(k) => ("EC PRIVATE KEY", k.secret_sec1_der().to_vec()),
                rustls_pki_types::PrivateKeyDer::Pkcs8(k) => ("PRIVATE KEY", k.secret_pkcs8_der().to_vec()),
                _ => return Err(msg!("file.notKey")),
            };
            set(secret_keys::client_key(&id), &mqttpanel_tls::to_pem(label, &der))?;
            del(secret_keys::passphrase(&id));
            del(secret_keys::pkcs12(&id));
            let p = state.store.update(&profile_id, |p| p.profile.tls.client_key = Some(field_of(summary, true)))?;
            Ok(p.profile)
        }
    }
}

// ---------------------------------------------------------------- client certificate, second way (change set 11)

fn secret_err(e: mqttpanel_profiles::SecretError) -> UserMessage {
    msg!(e.catalog_key(), detail = e.to_string())
}

/// Clears every piece of client-certificate material of a profile (certificate, key, PKCS#12 bundle, passphrase).
fn clear_client_material(state: &AppState, id: &str) {
    let secrets = state.store.secrets();
    for k in [secret_keys::client_key(id), secret_keys::pkcs12(id), secret_keys::passphrase(id)] {
        let _ = secrets.delete(&k);
    }
}

/// Chooses how the profile gets its client certificate: imported (false) or generated key and CSR (true). Any material
/// of either way is discarded (the UI asked first when there was some); "Start over" is the same call with `true`.
#[tauri::command]
pub fn set_client_cert_way(state: State<'_, AppState>, profile_id: String, generated: bool) -> R<Profile> {
    client_cert_way(&state, &profile_id, generated)
}

pub fn client_cert_way(state: &AppState, profile_id: &str, generated: bool) -> R<Profile> {
    stored(state, profile_id)?;
    clear_client_material(state, profile_id);
    let p = state.store.update(profile_id, |p| {
        p.profile.tls.client_key_generated = generated;
        p.profile.tls.client_cert = None;
        p.profile.tls.client_key = None;
        p.blobs.client_cert_pem = None;
    })?;
    Ok(p.profile)
}

/// "Generate key": a new ECDSA P-256 (default) or RSA 2048 key for the profile, stored in the secret store under the
/// profile's client-key secret; the key never leaves it. Any previous certificate is dropped (it belongs to another
/// key). The profile's `tls.clientKey` carries the algorithm and the public-key fingerprint (summary kind "generated").
#[tauri::command]
pub fn profile_generate_client_key(state: State<'_, AppState>, profile_id: String, algorithm: Option<mqttpanel_tls::kit::KeyAlg>) -> R<Profile> {
    generate_key_for(&state, &profile_id, algorithm.unwrap_or_default())
}

pub fn generate_key_for(state: &AppState, profile_id: &str, algorithm: mqttpanel_tls::kit::KeyAlg) -> R<Profile> {
    stored(state, profile_id)?;
    let k = mqttpanel_tls::kit::generate_client_key(algorithm).map_err(|e| msg!("error.internal", detail = e.to_string()))?;
    clear_client_material(state, profile_id);
    state.store.secrets().set(&secret_keys::client_key(profile_id), &k.key_pem).map_err(secret_err)?;
    let summary = generated_summary(&k.algorithm, &k.fingerprint);
    let p = state.store.update(profile_id, |p| {
        p.profile.tls.client_key_generated = true;
        p.profile.tls.client_cert = None;
        p.blobs.client_cert_pem = None;
        p.profile.tls.client_key = Some(FileField { file_name: String::new(), summary: Some(summary), in_keychain: true });
    })?;
    Ok(p.profile)
}

pub fn generated_summary(algorithm: &str, fingerprint: &str) -> mqttpanel_tls::CertSummary {
    mqttpanel_tls::CertSummary {
        kind: "generated".into(),
        subject: None,
        cn: None,
        issuer: None,
        not_before: None,
        not_after: None,
        key_type: Some(algorithm.to_string()),
        count: 0,
        self_signed: false,
        is_ca: false,
        encrypted: false,
        fingerprint: Some(fingerprint.to_string()),
    }
}

fn generated_key(state: &AppState, profile_id: &str) -> R<String> {
    let p = stored(state, profile_id)?;
    if !p.profile.tls.client_key_generated {
        return Err(msg!("file.noGeneratedKey"));
    }
    state.store.secrets().get(&secret_keys::client_key(profile_id)).map_err(secret_err)?.ok_or_else(|| msg!("file.noGeneratedKey"))
}

/// The certificate request (PEM) for the generated key with this common name.
pub fn client_csr_pem(state: &AppState, profile_id: &str, common_name: &str) -> R<String> {
    let key = generated_key(state, profile_id)?;
    let cn = common_name.trim();
    if cn.is_empty() {
        return Err(msg!("file.csrNoName"));
    }
    mqttpanel_tls::kit::csr_for_key(&key, cn).map_err(|e| msg!("error.internal", detail = e.to_string()))
}

/// "Download CSR…": the request for the generated key, saved through the native save dialog as "<common name>.csr".
#[tauri::command]
pub async fn profile_client_csr(app: AppHandle, state: State<'_, AppState>, profile_id: String, common_name: String) -> R<SaveOutcome> {
    let pem = client_csr_pem(&state, &profile_id, &common_name)?;
    let safe: String = common_name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect();
    Ok(files::save_bytes(&app, &format!("{safe}.csr"), pem.as_bytes()))
}

/// "Import signed certificate…": the certificate (PEM, CRT or DER, with its chain if any) must carry the generated
/// key's public key, otherwise "This certificate does not match the generated key".
#[tauri::command]
pub fn profile_import_signed_cert(state: State<'_, AppState>, profile_id: String, path: String) -> R<Profile> {
    let path = PathBuf::from(path);
    let name = files::file_name(&path);
    let bytes = std::fs::read(&path).map_err(|_| msg!("file.unreadable", file = name.clone()))?;
    import_signed_cert_bytes(&state, &profile_id, &name, &bytes)
}

pub fn import_signed_cert_bytes(state: &AppState, profile_id: &str, name: &str, bytes: &[u8]) -> R<Profile> {
    let key = generated_key(state, profile_id)?;
    let certs = mqttpanel_tls::parse_certs(bytes).map_err(|e| files::tls_error(&e, name))?;
    if !mqttpanel_tls::kit::cert_matches_key(certs[0].as_ref(), &key) {
        return Err(msg!("file.notGeneratedKey"));
    }
    let summary = mqttpanel_tls::inspect(bytes, mqttpanel_tls::FileExpect::Certificate, None).map_err(|e| files::tls_error(&e, name))?;
    let pem: String = certs.iter().map(|c| mqttpanel_tls::to_pem("CERTIFICATE", c.as_ref())).collect();
    let p = state.store.update(profile_id, |p| {
        p.blobs.client_cert_pem = Some(pem);
        p.profile.tls.client_cert = Some(FileField { file_name: name.to_string(), summary: Some(summary), in_keychain: false });
    })?;
    Ok(p.profile)
}

#[tauri::command]
pub fn add_proto_schema(state: State<'_, AppState>, profile_id: String, path: String, topic_filter: String, message_type: Option<String>) -> R<Profile> {
    let path = PathBuf::from(path);
    let files_ = files::proto_files_near(&path);
    let types = proto_message_types(&files_).map_err(files::engine_msg)?;
    let name = files::file_name(&path);
    let chosen = message_type.filter(|t| !t.is_empty()).or_else(|| types.first().cloned()).unwrap_or_default();
    let p = state.store.update(&profile_id, |p| {
        for f in &files_ {
            p.blobs.proto_files.retain(|(n, _)| n != &f.name);
            p.blobs.proto_files.push((f.name.clone(), f.content.clone()));
        }
        p.profile.proto_schemas.retain(|s| s.file_name != name);
        p.profile.proto_schemas.push(ProtoSchema { file_name: name.clone(), topic_filter: if topic_filter.is_empty() { "#".into() } else { topic_filter }, message_type: chosen, message_types: types });
    })?;
    if let Some(c) = lock(&state.conns).get(&profile_id) {
        apply_proto(&mut lock(&c.engine), &p);
    }
    Ok(p.profile)
}

#[tauri::command]
pub fn remove_proto_schema(state: State<'_, AppState>, profile_id: String, file_name: String) -> R<Profile> {
    let p = state.store.update(&profile_id, |p| p.profile.proto_schemas.retain(|s| s.file_name != file_name))?;
    Ok(p.profile)
}

// ---------------------------------------------------------------- connections

fn trust_prompt(state: &AppState, profile_id: &str, t: mqttpanel_client::TrustInfo) -> TrustPrompt {
    lock(&state.pending_trust).insert(profile_id.to_string(), t.clone());
    TrustPrompt { profile_id: profile_id.to_string(), info: t }
}

fn stored(state: &AppState, id: &str) -> R<StoredProfile> {
    state.store.get(id).ok_or_else(|| msg!("profile.notFound"))
}

/// Connects a profile. `skip_once` skips certificate verification for this attempt only ("only this time").
pub async fn connect_stored(state: &AppState, p: StoredProfile, skip_once: bool) -> ConnectOutcome {
    let id = p.profile.id.clone();
    if let Some(c) = lock(&state.conns).get(&id).cloned() {
        if c.is_open() && c.handle().is_some() {
            return ConnectOutcome::Connected { status: c.status(now_ms()) };
        }
    }
    let opts = match mqttpanel_profiles::connect_options(&p, state.store.secrets().as_ref(), skip_once) {
        Ok(o) => o,
        Err(error) => return ConnectOutcome::Failed { error },
    };
    let opts_skip = opts.tls.skip_verify && opts.scheme.is_tls();
    match mqttpanel_client::connect(opts).await {
        Ok((handle, info, rx)) => {
            let existing = lock(&state.conns).get(&id).cloned();
            let c = match existing {
                Some(c) => {
                    lock(&c.engine).reset_counts();
                    c
                }
                None => {
                    let settings = lock(&state.settings).clone();
                    let c = Arc::new(Conn::new(&p, &settings));
                    lock(&state.conns).insert(id.clone(), c.clone());
                    c
                }
            };
            *lock(&c.ever_connected) = true;
            c.skip_verify.store(opts_skip, std::sync::atomic::Ordering::Relaxed);
            on_connected(&c, &info);
            lock(&c.link).sub_errors.clear();
            *lock(&c.handle) = Some(handle);
            tokio::spawn(pump(c.clone(), rx));
            let _ = state.store.update(&id, |s| s.profile.last_used = Some(now_ms()));
            ConnectOutcome::Connected { status: c.status(now_ms()) }
        }
        Err(ConnectFailure::Trust(t)) => ConnectOutcome::NeedsTrust { trust: trust_prompt(state, &id, *t) },
        Err(ConnectFailure::Message(error)) => ConnectOutcome::Failed { error },
    }
}

#[tauri::command]
pub async fn connect_profile(state: State<'_, AppState>, profile_id: String, skip_once: Option<bool>) -> R<ConnectOutcome> {
    let p = stored(&state, &profile_id)?;
    Ok(connect_stored(&state, p, skip_once.unwrap_or(false)).await)
}

#[tauri::command]
pub async fn test_profile(state: State<'_, AppState>, profile_id: String, skip_once: Option<bool>) -> R<TestOutcome> {
    let p = stored(&state, &profile_id)?;
    let opts = mqttpanel_profiles::connect_options(&p, state.store.secrets().as_ref(), skip_once.unwrap_or(false))?;
    let tls = opts.scheme.is_tls();
    let result = match mqttpanel_client::test_connection(&opts).await {
        Ok(r) => TestResult {
            ok: true,
            at: now_ms(),
            latency_ms: Some(r.info.latency_ms),
            negotiated_version: Some(r.info.version.label().into()),
            max_packet_size: r.info.max_packet_size,
            server_cert: r.info.transport.server_cert.clone(),
            tls_version: r.info.transport.tls_version.clone(),
            verified: if tls { r.info.transport.verified } else { None },
            error: None,
        },
        Err(ConnectFailure::Trust(t)) => return Ok(TestOutcome::NeedsTrust { trust: trust_prompt(&state, &profile_id, *t) }),
        Err(ConnectFailure::Message(m)) => TestResult {
            ok: false,
            at: now_ms(),
            latency_ms: None,
            negotiated_version: None,
            max_packet_size: None,
            server_cert: None,
            tls_version: None,
            verified: None,
            error: Some(m),
        },
    };
    let value = serde_json::to_value(&result).ok();
    let _ = state.store.update(&profile_id, |s| s.profile.last_test = value);
    Ok(TestOutcome::Tested { result })
}

#[tauri::command]
pub fn trust_certificate(state: State<'_, AppState>, profile_id: String, mode: String) -> R<Profile> {
    let t = lock(&state.pending_trust).remove(&profile_id).ok_or_else(|| msg!("profile.notFound"))?;
    let p = state.store.update(&profile_id, |p| {
        if mode == "skipOnce" {
            // Nothing is saved: the UI retries with skipOnce.
        } else if mode == "skipAlways" {
            p.profile.tls.skip_verify = true;
        } else if mode == "ca" {
            if let Some(der) = &t.ca_der {
                let pem = mqttpanel_tls::to_pem("CERTIFICATE", der);
                let summary = mqttpanel_tls::inspect(pem.as_bytes(), mqttpanel_tls::FileExpect::Certificate, None).ok();
                let name = summary.as_ref().and_then(|s| s.cn.clone()).unwrap_or_else(|| "broker-ca".into());
                p.blobs.ca_pem = Some(pem);
                p.profile.tls.ca_cert = Some(FileField { file_name: format!("{name}.pem"), summary, in_keychain: false });
            }
        } else {
            if t.reason == mqttpanel_client::TrustReason::FingerprintChanged {
                p.profile.tls.trusted_fingerprints.clear();
            }
            if !p.profile.tls.trusted_fingerprints.contains(&t.leaf.fingerprint) {
                p.profile.tls.trusted_fingerprints.push(t.leaf.fingerprint.clone());
            }
        }
    })?;
    Ok(p.profile)
}

pub async fn close(state: &AppState, id: &str) {
    let c = lock(&state.conns).remove(id);
    if let Some(c) = c {
        halt_repeat(&c, None);
        if let Some(h) = c.handle() {
            h.disconnect().await;
        }
    }
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, connection_id: String) -> R<()> {
    let c = lock(&state.conns).get(&connection_id).cloned();
    if let Some(c) = c {
        halt_repeat(&c, Some(msg!("repeat.stoppedDisconnect")));
        let h = lock(&c.handle).take();
        if let Some(h) = h {
            h.disconnect().await;
        }
        lock(&c.link).state = "disconnected".into();
    }
    Ok(())
}

#[tauri::command]
pub async fn close_connection(state: State<'_, AppState>, connection_id: String) -> R<()> {
    close(&state, &connection_id).await;
    Ok(())
}

#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Vec<ConnectionStatus> {
    let now = now_ms();
    lock(&state.conns).values().map(|c| c.status(now)).collect()
}

// ---------------------------------------------------------------- explorer

#[tauri::command]
pub fn tree_slice(state: State<'_, AppState>, connection_id: String, query: TreeQuery, start: usize, count: usize) -> R<TreeSlice> {
    Ok(lock(&conn(&state, &connection_id)?.engine).tree_slice(&query, start, count.min(2000), now_ms()))
}

#[tauri::command]
pub fn table_slice(state: State<'_, AppState>, connection_id: String, query: TableQuery, start: usize, count: usize) -> R<TableSlice> {
    Ok(lock(&conn(&state, &connection_id)?.engine).table_slice(&query, start, count.min(2000), now_ms()))
}

#[tauri::command]
pub fn set_expanded(state: State<'_, AppState>, connection_id: String, path: String, expanded: bool) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).set_expanded(&path, expanded);
    Ok(())
}

#[tauri::command]
pub fn expand_to_depth(state: State<'_, AppState>, connection_id: String, depth: usize) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).expand_to_depth(depth);
    Ok(())
}

#[tauri::command]
pub fn expand_changed(state: State<'_, AppState>, connection_id: String, within_ms: i64) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).expand_changed(within_ms, now_ms());
    Ok(())
}

#[tauri::command]
pub fn collapse_all(state: State<'_, AppState>, connection_id: String) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).collapse_all();
    Ok(())
}

#[tauri::command]
pub fn get_expanded(state: State<'_, AppState>, connection_id: String) -> R<Vec<String>> {
    Ok(lock(&conn(&state, &connection_id)?.engine).expanded())
}

#[tauri::command]
pub fn set_expanded_set(state: State<'_, AppState>, connection_id: String, paths: Vec<String>) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).set_expanded_set(paths);
    Ok(())
}

#[tauri::command]
pub fn set_pinned(state: State<'_, AppState>, connection_id: String, paths: Vec<String>) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).set_pinned(paths);
    Ok(())
}

#[tauri::command]
pub fn reveal_topic(state: State<'_, AppState>, connection_id: String, topic: String) -> R<i64> {
    let q = TreeQuery::default();
    Ok(lock(&conn(&state, &connection_id)?.engine).reveal_topic(&topic, &q))
}

#[tauri::command]
pub fn next_changed(state: State<'_, AppState>, connection_id: String, from_topic: Option<String>, within_ms: i64) -> R<Option<String>> {
    Ok(lock(&conn(&state, &connection_id)?.engine).next_changed(from_topic.as_deref(), within_ms, now_ms()))
}

#[tauri::command]
pub fn known_topics(state: State<'_, AppState>, connection_id: String, prefix: String, limit: usize) -> R<Vec<String>> {
    Ok(lock(&conn(&state, &connection_id)?.engine).known_topics(&prefix, limit.min(500)))
}

// ---------------------------------------------------------------- inspection

#[tauri::command]
pub fn topic_detail(state: State<'_, AppState>, connection_id: String, topic: String) -> R<TopicDetail> {
    Ok(lock(&conn(&state, &connection_id)?.engine).topic_detail(&topic))
}

#[tauri::command]
pub fn message(state: State<'_, AppState>, connection_id: String, topic: String, seq: u64, format: PayloadFormat, format_anyway: bool) -> R<Option<MessageView>> {
    Ok(lock(&conn(&state, &connection_id)?.engine).message(&topic, seq, format, format_anyway))
}

#[tauri::command]
pub fn history(state: State<'_, AppState>, connection_id: String, topic: String) -> R<Vec<HistoryEntry>> {
    Ok(lock(&conn(&state, &connection_id)?.engine).history(&topic))
}

#[tauri::command]
pub fn diff(state: State<'_, AppState>, connection_id: String, topic: String, baseline_seq: u64, seq: u64) -> R<DiffResult> {
    Ok(lock(&conn(&state, &connection_id)?.engine).diff(&topic, baseline_seq, seq))
}

#[tauri::command]
pub fn set_topic_buffer(state: State<'_, AppState>, connection_id: String, topic: String, size: usize) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).set_topic_buffer(&topic, size.clamp(10, 10_000));
    Ok(())
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, connection_id: String, prefix: Option<String>) -> R<()> {
    lock(&conn(&state, &connection_id)?.engine).clear_history(prefix.as_deref());
    Ok(())
}

#[derive(serde::Serialize)]
pub struct RetainedItem {
    topic: String,
    preview: String,
}

#[tauri::command]
pub fn retained_under(state: State<'_, AppState>, connection_id: String, prefix: String) -> R<Vec<RetainedItem>> {
    Ok(lock(&conn(&state, &connection_id)?.engine).retained_under(&prefix).into_iter().map(|(topic, preview)| RetainedItem { topic, preview }).collect())
}

#[tauri::command]
pub fn memory(state: State<'_, AppState>) -> MemoryEstimate {
    let budget = lock(&state.settings).memory_budget_mb as usize * 1024 * 1024;
    let mut total = MemoryEstimate { bytes: 0, budget_bytes: budget as _, points: 0, messages: 0 };
    for c in lock(&state.conns).values() {
        let m = lock(&c.engine).memory();
        total.bytes += m.bytes;
        total.points += m.points;
        total.messages += m.messages;
    }
    total
}

#[tauri::command]
pub fn series(state: State<'_, AppState>, connection_id: String, topic: String, path: String, since_ms: Option<i64>) -> R<SeriesData> {
    Ok(lock(&conn(&state, &connection_id)?.engine).series(&topic, &path, since_ms))
}

// ---------------------------------------------------------------- export

#[tauri::command]
pub fn export_estimate(state: State<'_, AppState>, connection_id: String, options: ExportOptions) -> R<ExportEstimate> {
    Ok(lock(&conn(&state, &connection_id)?.engine).export_estimate(&options))
}

fn stamp() -> String {
    let t = time::OffsetDateTime::now_utc();
    format!("{:04}{:02}{:02}-{:02}{:02}", t.year(), u8::from(t.month()), t.day(), t.hour(), t.minute())
}

#[tauri::command]
pub async fn export_data(app: AppHandle, state: State<'_, AppState>, connection_id: String, options: ExportOptions) -> R<SaveOutcome> {
    let c = conn(&state, &connection_id)?;
    let ext = match options.format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
        ExportFormat::Ndjson => "ndjson",
    };
    let mut data = Vec::new();
    lock(&c.engine).export(&options, &mut data).map_err(|e| msg!("error.internal", detail = e.to_string()))?;
    Ok(files::save_bytes(&app, &format!("mqtt-export-{}.{ext}", stamp()), &data))
}

#[tauri::command]
pub async fn export_chart_csv(app: AppHandle, state: State<'_, AppState>, connection_id: String, series: Vec<ChartSeriesRef>, since_ms: Option<i64>) -> R<SaveOutcome> {
    let csv = lock(&conn(&state, &connection_id)?.engine).export_chart_csv(&series, since_ms);
    Ok(files::save_bytes(&app, &format!("chart-{}.csv", stamp()), csv.as_bytes()))
}

#[tauri::command]
pub async fn save_png(app: AppHandle, default_name: String, base64: String) -> R<SaveOutcome> {
    let data = base64::engine::general_purpose::STANDARD.decode(base64.trim()).map_err(|_| msg!("error.internal", detail = "image data"))?;
    let name = if default_name.ends_with(".png") { default_name } else { format!("{default_name}.png") };
    Ok(files::save_bytes(&app, &name, &data))
}

#[tauri::command]
pub async fn export_sent_log(app: AppHandle, state: State<'_, AppState>, connection_id: String) -> R<SaveOutcome> {
    let log: Vec<SentEntry> = lock(&conn(&state, &connection_id)?.sent).iter().cloned().collect();
    let data = serde_json::to_vec_pretty(&log).unwrap_or_default();
    Ok(files::save_bytes(&app, &format!("sent-log-{}.json", stamp()), &data))
}

// ---------------------------------------------------------------- publish

fn new_entry(req: &PublishRequest, size: usize, status: &str) -> SentEntry {
    SentEntry {
        id: uuid::Uuid::new_v4().to_string(),
        at: now_ms(),
        topic: req.topic.clone(),
        size,
        qos: req.qos.min(2),
        retain: req.retain,
        status: status.into(),
        reason: None,
        repeat_count: None,
        payload_text: req.payload.clone(),
        format: req.format.clone(),
        request: None,
        reply: None,
        reply_timed_out: false,
    }
}

/// Publishes one request; returns the sent-log entry (pending until the broker answers).
pub fn do_publish(c: &Conn, req: &PublishRequest, status: &str, log: bool) -> R<SentEntry> {
    let handle = c.handle().ok_or_else(|| msg!("error.notConnected"))?;
    if req.topic.is_empty() || req.topic.contains('+') || req.topic.contains('#') {
        return Err(msg!("reason.topicInvalid"));
    }
    let payload = crate::payload::encode(&req.format, &req.payload)?;
    let v5 = c.negotiated_v5();
    let mut entry = new_entry(req, payload.len(), status);
    let mut props: Option<Props5> = None;
    if v5 {
        let mut p = Props5::default();
        if let Some(pp) = &req.props {
            p.content_type = Some(pp.content_type.clone()).filter(|s| !s.is_empty());
            p.payload_format = pp.payload_format;
            p.message_expiry = pp.message_expiry;
            p.user_properties = pp.user_properties.iter().filter(|(k, _)| !k.is_empty()).cloned().collect();
        }
        if let Some(r) = req.request.as_ref().filter(|r| !r.response_topic.is_empty()) {
            let corr = if r.correlation_data.trim().is_empty() || r.correlation_data.starts_with("auto") { c.next_correlation() } else { r.correlation_data.clone() };
            p.response_topic = Some(r.response_topic.clone());
            p.correlation_data = Some(corr.clone().into_bytes());
            let new_sub = lock(&c.response_subs).insert(r.response_topic.clone());
            if new_sub {
                handle.subscribe(vec![(r.response_topic.clone(), 1)]);
            }
            lock(&c.requests).insert(
                corr.clone(),
                PendingRequest { sent_id: entry.id.clone(), at: entry.at, response_topic: r.response_topic.clone(), deadline: entry.at + (r.wait_s.max(1.0) * 1000.0) as i64 },
            );
            entry.request = Some(SentRequest { response_topic: r.response_topic.clone(), correlation_data: corr });
        }
        if p != Props5::default() {
            props = Some(p);
        }
    }
    let token = handle.publish(Outgoing { topic: req.topic.clone(), payload, qos: req.qos.min(2), retain: req.retain, props });
    if log {
        lock(&c.tokens).insert(token, entry.id.clone());
        c.upsert_sent(entry.clone());
    }
    Ok(entry)
}

#[tauri::command]
pub fn publish(state: State<'_, AppState>, connection_id: String, request: PublishRequest) -> R<SentEntry> {
    let c = conn(&state, &connection_id)?;
    do_publish(&c, &request, "pending", true)
}

fn delete_request(topic: &str) -> PublishRequest {
    PublishRequest { topic: topic.to_string(), payload: String::new(), format: "text".into(), qos: 1, retain: true, props: None, request: None }
}

#[tauri::command]
pub fn delete_retained(state: State<'_, AppState>, connection_id: String, topic: String) -> R<SentEntry> {
    let c = conn(&state, &connection_id)?;
    do_publish(&c, &delete_request(&topic), "deletedRetained", true)
}

#[tauri::command]
pub fn delete_retained_many(state: State<'_, AppState>, connection_id: String, topics: Vec<String>) -> R<usize> {
    let c = conn(&state, &connection_id)?;
    let mut n = 0;
    for t in topics {
        do_publish(&c, &delete_request(&t), "deletedRetained", true)?;
        n += 1;
    }
    Ok(n)
}

#[tauri::command]
pub fn sent_log(state: State<'_, AppState>, connection_id: String) -> R<Vec<SentEntry>> {
    Ok(lock(&conn(&state, &connection_id)?.sent).iter().cloned().collect())
}

#[tauri::command]
pub fn clear_sent_log(state: State<'_, AppState>, connection_id: String) -> R<()> {
    lock(&conn(&state, &connection_id)?.sent).clear();
    Ok(())
}

#[tauri::command]
pub fn start_repeat(state: State<'_, AppState>, connection_id: String, request: PublishRequest, interval_s: f64, count: Option<u32>) -> R<()> {
    let c = conn(&state, &connection_id)?;
    start_repeat_on(c, request, interval_s, count)
}

/// Starts repeat publishing on a connection (P6): one sent-log row counts the sends.
pub fn start_repeat_on(c: Arc<Conn>, request: PublishRequest, interval_s: f64, count: Option<u32>) -> R<()> {
    let connection_id = c.id.clone();
    halt_repeat(&c, None);
    // Validate once before starting.
    crate::payload::encode(&request.format, &request.payload)?;
    let interval = interval_s.max(0.1);
    let mut row = new_entry(&request, request.payload.len(), "repeat");
    row.repeat_count = Some(0);
    c.upsert_sent(row.clone());
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    *lock(&c.repeat) = Some(RepeatState {
        status: RepeatStatus { connection_id: connection_id.clone(), running: true, interval_s: interval, sent: 0, count, next_at: Some(now_ms()), topic: request.topic.clone(), stopped_reason: None },
        cancel: tx,
        sent_id: row.id.clone(),
    });
    let c2 = c.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs_f64(interval));
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = rx.changed() => break,
            }
            if *rx.borrow() {
                break;
            }
            let ok = do_publish(&c2, &request, "repeat", false).is_ok();
            let done = {
                let mut rep = lock(&c2.repeat);
                let Some(r) = rep.as_mut() else { break };
                if ok {
                    r.status.sent += 1;
                }
                r.status.next_at = Some(now_ms() + (interval * 1000.0) as i64);
                let sent = r.status.sent;
                let id = r.sent_id.clone();
                let finished = !ok || r.status.count.is_some_and(|n| sent >= n);
                if finished {
                    r.status.running = false;
                    r.status.next_at = None;
                    if !ok {
                        r.status.stopped_reason = Some(msg!("repeat.stoppedDisconnect"));
                    }
                }
                drop(rep);
                c2.update_sent(&id, |e| {
                    e.repeat_count = Some(sent);
                    e.at = now_ms();
                });
                finished
            };
            if done {
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn stop_repeat(state: State<'_, AppState>, connection_id: String) -> R<()> {
    let c = conn(&state, &connection_id)?;
    halt_repeat(&c, None);
    Ok(())
}

#[tauri::command]
pub fn list_templates(state: State<'_, AppState>, profile_id: String) -> Vec<Template> {
    state.store.templates(&profile_id)
}

#[tauri::command]
pub fn save_template(state: State<'_, AppState>, profile_id: String, template: Template) -> R<Template> {
    state.store.save_template(&profile_id, template)
}

#[tauri::command]
pub fn delete_template(state: State<'_, AppState>, profile_id: String, template_id: String) -> R<()> {
    state.store.delete_template(&profile_id, &template_id)
}

#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum EncodeResult {
    Ok { ok: bool, size: usize },
    Err { ok: bool, error: UserMessage },
}

#[tauri::command]
pub fn encode_payload(format: String, payload: String) -> EncodeResult {
    match crate::payload::encode(&format, &payload) {
        Ok(b) => EncodeResult::Ok { ok: true, size: b.len() },
        Err(error) => EncodeResult::Err { ok: false, error },
    }
}
