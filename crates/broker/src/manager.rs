//! Persistent broker management for the desktop app and the CLI: `broker.json` in a caller-chosen directory,
//! secrets (CA and server keys, user passwords, issued client keys and passphrases) in a [`SecretStore`].

use crate::config::*;
use crate::error::{n, s, BrokerError};
use crate::runtime::{
    hash_password, normalize_serial, AuthConfig, BindAddr, Broker, BrokerHandle, Credential,
    ListenerSpec, LogBuffer, RevocationList, RuntimeConfig, TlsMaterial,
};
use mqttpanel_profiles::SecretStore;
use mqttpanel_tls::kit::{self, Ca, KeyAlg, Usage, Validity};
use mqttpanel_tls::{cert_details, parse_certs, parse_private_key, to_pem};
use rustls_pki_types::PrivateKeyDer;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// File name of the broker configuration inside the state directory.
pub const CONFIG_FILE: &str = "broker.json";
/// Common name of the local CA.
pub const LOCAL_CA_NAME: &str = "MQTT Panel Local CA";
/// Common name of the generated server certificate.
pub const SERVER_CERT_NAME: &str = "mqttpanel-local";
/// Name of the client certificate used by the profile "Local broker".
pub const DESKTOP_CERT_NAME: &str = "mqttpanel-desktop";
/// Name of the broker user created for the profile "Local broker".
pub const APP_USER_NAME: &str = "mqtt-panel";
/// Length of the generated password of [`APP_USER_NAME`] (letters and digits, about 143 bits).
const APP_PASSWORD_LEN: usize = 24;

const SECRET_CA_KEY: &str = "broker/ca-key";
const SECRET_SERVER_KEY: &str = "broker/server-key";

/// The plain password of a shared user, kept so the UI can reveal and copy it (broker.json holds the argon2 hash).
fn secret_user_password(name: &str) -> String {
    format!("broker/user/{name}/password")
}

/// A strong random password of letters and digits (no look-alike characters).
pub fn generate_password(len: usize) -> String {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let x = OsRng.next_u32() as usize;
        // rejection sampling keeps the distribution uniform
        if x < (u32::MAX as usize / A.len()) * A.len() {
            out.push(A[x % A.len()] as char);
        }
    }
    out
}
fn secret_client_key(name: &str) -> String {
    format!("broker/client/{name}/key")
}
fn secret_client_passphrase(name: &str) -> String {
    format!("broker/client/{name}/passphrase")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredListener {
    id: String,
    kind: ListenerKind,
    bind: Bind,
    port: u16,
    enabled: bool,
    preset: Preset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredUser {
    username: String,
    /// argon2id PHC string ("argon2-hashed at rest"). The plain password is in the secret store.
    password_hash: String,
    /// Created by the app for the profile "Local broker".
    #[serde(default)]
    system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCert {
    name: String,
    serial: String,
    issued_at: String,
    expires_at: String,
    key_type: String,
    revoked: bool,
    system: bool,
    /// The private key was generated here and is kept in the secret store (exportable later).
    has_key: bool,
    /// SHA-256 fingerprint of the CA that issued it.
    ca_fingerprint: String,
    cert_pem: String,
    last_seen: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    version: u32,
    listeners: Vec<StoredListener>,
    anonymous: bool,
    users: Vec<StoredUser>,
    /// Client certificate on encrypted listeners, broker-wide (change set 2). Older files carried it per listener;
    /// pre-release, that field is ignored.
    #[serde(default)]
    require_client_cert: bool,
    verify_against: VerifyAgainst,
    max_clients: u32,
    issued: Vec<StoredCert>,
    revoked_serials: Vec<String>,
    /// Local CA certificate (public).
    ca_pem: String,
    /// Server certificate chain (public).
    server_pem: String,
    server_imported: bool,
    imported_ca_pem: Option<String>,
}

fn default_listener(id: &str, kind: ListenerKind, port: u16, enabled: bool) -> StoredListener {
    StoredListener {
        id: id.into(),
        kind,
        bind: Bind::Network,
        port,
        enabled,
        preset: if kind.encrypted() {
            Preset::Encrypted
        } else {
            Preset::Plain
        },
    }
}

struct Inner {
    stored: Stored,
    handle: Option<BrokerHandle>,
    state: BrokerState,
    error: Option<crate::error::UserMessage>,
    restart_needed: bool,
}

/// Manages the broker configuration, certificates and the running broker.
pub struct BrokerManager {
    dir: PathBuf,
    secrets: Arc<dyn SecretStore>,
    inner: Mutex<Inner>,
    log: LogBuffer,
    revoked: RevocationList,
    /// The server key, read once per session from the secret store and kept in memory, so a broker start or restart
    /// never touches the secret store (and the OS keychain) again (SPEC 6, change set 2).
    server_key: Mutex<Option<String>>,
}

impl std::fmt::Debug for BrokerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerManager")
            .field("dir", &self.dir)
            .finish()
    }
}

fn valid_username(u: &str) -> bool {
    !u.is_empty() && !u.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn valid_cert_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn key_pem_of(key: &PrivateKeyDer<'_>) -> String {
    match key {
        PrivateKeyDer::Pkcs8(k) => to_pem("PRIVATE KEY", k.secret_pkcs8_der()),
        PrivateKeyDer::Sec1(k) => to_pem("EC PRIVATE KEY", k.secret_sec1_der()),
        PrivateKeyDer::Pkcs1(k) => to_pem("RSA PRIVATE KEY", k.secret_pkcs1_der()),
        _ => String::new(),
    }
}

fn certs_pem(certs: &[rustls_pki_types::CertificateDer<'_>]) -> String {
    certs
        .iter()
        .map(|c| to_pem("CERTIFICATE", c.as_ref()))
        .collect()
}

fn details_of_pem(pem: &str) -> Result<CertDetails, BrokerError> {
    let certs =
        parse_certs(pem.as_bytes()).map_err(|e| BrokerError::tls_file(&e, "certificate"))?;
    let first = certs
        .first()
        .ok_or_else(|| BrokerError::new("file.notCertificate", &[]))?;
    cert_details(first.as_ref()).map_err(|e| BrokerError::tls_file(&e, "certificate"))
}

fn fingerprint_of_pem(pem: &str) -> String {
    details_of_pem(pem)
        .map(|d| d.fingerprint)
        .unwrap_or_default()
}

fn stored_cert_from(issued: &kit::Issued, system: bool, has_key: bool, ca_fp: &str) -> StoredCert {
    let d = cert_details(&issued.cert_der).ok();
    StoredCert {
        name: issued.cn.clone(),
        serial: issued.serial.clone(),
        issued_at: d.as_ref().map(|d| d.not_before.clone()).unwrap_or_default(),
        expires_at: d.as_ref().map(|d| d.not_after.clone()).unwrap_or_default(),
        key_type: d.as_ref().map(|d| d.key_type.clone()).unwrap_or_default(),
        revoked: false,
        system,
        has_key,
        ca_fingerprint: ca_fp.to_string(),
        cert_pem: issued.cert_pem.clone(),
        last_seen: None,
    }
}

/// Finds the process listening on a TCP port with `lsof` (macOS, Linux), as "name (pid N)".
fn process_on_port(port: u16) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let out = std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let mut parts = line.split_whitespace();
    let cmd = parts.next()?;
    let pid = parts.next()?;
    Some(format!("{cmd} (pid {pid})"))
}

/// True when binding the port fails because it is in use.
fn port_in_use(port: u16) -> bool {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    let addrs = [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port),
    ];
    for a in addrs {
        let domain = if a.is_ipv6() {
            socket2::Domain::IPV6
        } else {
            socket2::Domain::IPV4
        };
        let Ok(sock) =
            socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))
        else {
            continue;
        };
        if a.is_ipv6() {
            let _ = sock.set_only_v6(true);
        }
        // Same socket options as the broker's own listeners, so connections in TIME_WAIT do not count as "in use".
        #[cfg(unix)]
        let _ = sock.set_reuse_address(true);
        if let Err(e) = sock.bind(&a.into()) {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                return true;
            }
            continue;
        }
        if let Err(e) = sock.listen(1) {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                return true;
            }
        }
    }
    false
}

/// Every secret key the broker state in `dir` may use (CA and server keys, user passwords, issued client keys and
/// passphrases), read from `broker.json` without opening the manager. Used to migrate per-secret keychain entries of
/// earlier builds into the secrets file.
pub fn secret_keys_in(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join(CONFIG_FILE)) else {
        return Vec::new();
    };
    let Ok(st) = serde_json::from_str::<Stored>(&text) else {
        return Vec::new();
    };
    let mut keys = vec![SECRET_CA_KEY.to_string(), SECRET_SERVER_KEY.to_string()];
    keys.extend(st.users.iter().map(|u| secret_user_password(&u.username)));
    for c in &st.issued {
        keys.push(secret_client_key(&c.name));
        keys.push(secret_client_passphrase(&c.name));
    }
    keys.sort();
    keys.dedup();
    keys
}

impl BrokerManager {
    /// Opens (or creates with defaults) the broker state in `dir`.
    ///
    /// Defaults on first use: TCP 1883, TLS 8883 and WS 8083 on the network, WSS 8084 off; anonymous off; a local CA
    /// "MQTT Panel Local CA" (ECDSA P-256, 5 years) and a server certificate "mqttpanel-local" for localhost and the
    /// host name (2 years).
    pub fn open(dir: &Path, secrets: Arc<dyn SecretStore>) -> Result<BrokerManager, BrokerError> {
        std::fs::create_dir_all(dir).map_err(|e| {
            BrokerError::new(
                "file.writeFailed",
                &[
                    ("file", s(CONFIG_FILE)),
                    ("detail", s(e.kind().to_string())),
                ],
            )
        })?;
        let path = dir.join(CONFIG_FILE);
        let stored = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|_| BrokerError::new("file.unreadable", &[("file", s(CONFIG_FILE))]))?;
            serde_json::from_str::<Stored>(&text).map_err(|e| {
                BrokerError::new("file.unreadable", &[("file", s(CONFIG_FILE))])
                    .with_detail(e.to_string())
            })?
        } else {
            let ca = Ca::new_root(LOCAL_CA_NAME, KeyAlg::Ecdsa, Validity::years(5))
                .map_err(|e| BrokerError::kit(&e))?;
            let server = ca
                .issue(
                    SERVER_CERT_NAME,
                    &kit::local_sans(),
                    Usage::Server,
                    KeyAlg::Ecdsa,
                    Validity::years(2),
                )
                .map_err(|e| BrokerError::kit(&e))?;
            secrets
                .set(SECRET_CA_KEY, &ca.key_pem)
                .map_err(|e| BrokerError::keychain(&e))?;
            secrets
                .set(SECRET_SERVER_KEY, &server.key_pem)
                .map_err(|e| BrokerError::keychain(&e))?;
            Stored {
                version: 1,
                listeners: vec![
                    default_listener("tcp", ListenerKind::Tcp, 1883, true),
                    default_listener("tls", ListenerKind::Tls, 8883, true),
                    default_listener("ws", ListenerKind::Ws, 8083, true),
                    default_listener("wss", ListenerKind::Wss, 8084, false),
                ],
                anonymous: false,
                users: Vec::new(),
                require_client_cert: false,
                verify_against: VerifyAgainst::Local,
                max_clients: 1000,
                issued: Vec::new(),
                revoked_serials: Vec::new(),
                ca_pem: ca.cert_pem.clone(),
                server_pem: format!("{}{}", server.cert_pem, ca.cert_pem),
                server_imported: false,
                imported_ca_pem: None,
            }
        };
        let revoked = RevocationList::new(stored.revoked_serials.iter());
        let m = BrokerManager {
            dir: dir.to_path_buf(),
            secrets,
            inner: Mutex::new(Inner {
                stored,
                handle: None,
                state: BrokerState::Stopped,
                error: None,
                restart_needed: false,
            }),
            log: LogBuffer::new(),
            revoked,
            server_key: Mutex::new(None),
        };
        if !path.exists() {
            m.save(&m.lock().stored)?;
        }
        Ok(m)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, stored: &Stored) -> Result<(), BrokerError> {
        let path = self.dir.join(CONFIG_FILE);
        let tmp = self.dir.join(format!("{CONFIG_FILE}.tmp"));
        let fail = |e: std::io::Error| {
            BrokerError::new(
                "file.writeFailed",
                &[
                    ("file", s(CONFIG_FILE)),
                    ("detail", s(e.kind().to_string())),
                ],
            )
            .with_detail(e.to_string())
        };
        let text = serde_json::to_string_pretty(stored)
            .map_err(|e| BrokerError::internal(e.to_string()))?;
        std::fs::write(&tmp, text).map_err(fail)?;
        std::fs::rename(&tmp, &path).map_err(fail)
    }

    /// Saves and, when running, applies live settings or flags a restart.
    fn commit(&self, inner: &mut Inner, needs_restart: bool) -> Result<(), BrokerError> {
        self.save(&inner.stored)?;
        if let Some(h) = &inner.handle {
            h.update_auth(self.auth_config(&inner.stored));
            if needs_restart {
                inner.restart_needed = true;
            }
        }
        Ok(())
    }

    /// The server key: from memory after the first read of the session.
    fn server_key(&self) -> Option<String> {
        let mut cached = self.server_key.lock().unwrap_or_else(|p| p.into_inner());
        if cached.is_none() {
            *cached = self.secrets.get(SECRET_SERVER_KEY).ok().flatten();
        }
        cached.clone()
    }

    fn set_server_key(&self, pem: &str) -> Result<(), BrokerError> {
        self.secrets
            .set(SECRET_SERVER_KEY, pem)
            .map_err(|e| BrokerError::keychain(&e))?;
        *self.server_key.lock().unwrap_or_else(|p| p.into_inner()) = Some(pem.to_string());
        Ok(())
    }

    fn secret(&self, key: &str) -> Result<Option<String>, BrokerError> {
        self.secrets.get(key).map_err(|e| BrokerError::keychain(&e))
    }

    fn auth_config(&self, st: &Stored) -> AuthConfig {
        let mut a = AuthConfig {
            anonymous: st.anonymous,
            ..Default::default()
        };
        for u in &st.users {
            a.users.insert(
                u.username.clone(),
                Credential::Argon2(u.password_hash.clone()),
            );
        }
        a
    }

    fn view(st: &Stored) -> BrokerConfig {
        BrokerConfig {
            listeners: st
                .listeners
                .iter()
                .map(|l| Listener {
                    id: l.id.clone(),
                    kind: l.kind,
                    bind: l.bind,
                    port: l.port,
                    enabled: l.enabled,
                    preset: l.preset,
                })
                .collect(),
            anonymous: st.anonymous,
            require_client_cert: st.require_client_cert,
            users: st
                .users
                .iter()
                .map(|u| UserView {
                    username: u.username.clone(),
                    system: u.system,
                    has_password: false,
                })
                .collect(),
            verify_against: st.verify_against,
            max_clients: st.max_clients,
        }
    }

    /// The directory holding `broker.json`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The configuration as shown to the UI.
    pub fn config(&self) -> BrokerConfig {
        let inner = self.lock();
        self.view_with_secrets(&inner.stored)
    }

    fn view_with_secrets(&self, st: &Stored) -> BrokerConfig {
        let mut c = Self::view(st);
        for u in &mut c.users {
            u.has_password = matches!(
                self.secrets.contains(&secret_user_password(&u.username)),
                Ok(true)
            );
        }
        c
    }

    /// The live log (kept across restarts).
    pub fn log_buffer(&self) -> &LogBuffer {
        &self.log
    }

    /// Log lines after `after_seq` at `level` or above.
    pub fn log(&self, level: LogLevel, after_seq: u64) -> Vec<LogLine> {
        self.log.lines(level, after_seq)
    }

    /// Current status.
    pub fn status(&self) -> BrokerStatus {
        let inner = self.lock();
        let st = &inner.stored;
        let (stats, listeners) = match &inner.handle {
            Some(h) => {
                let rt = h.listeners();
                let ls = st
                    .listeners
                    .iter()
                    .map(|l| match rt.iter().find(|r| r.id == l.id) {
                        Some(r) => ListenerStatus {
                            id: l.id.clone(),
                            up: r.error.is_none() && !r.addrs.is_empty(),
                            error: r.error.as_ref().map(|e| e.message.clone()),
                        },
                        None => ListenerStatus {
                            id: l.id.clone(),
                            up: false,
                            error: None,
                        },
                    })
                    .collect();
                (Some(h.stats()), ls)
            }
            None => (
                None,
                st.listeners
                    .iter()
                    .map(|l| ListenerStatus {
                        id: l.id.clone(),
                        up: false,
                        error: None,
                    })
                    .collect(),
            ),
        };
        BrokerStatus {
            state: inner.state,
            started_at: stats.map(|s| s.started_at),
            clients: stats.map(|s| s.clients).unwrap_or(0),
            max_clients: st.max_clients,
            msg_in_rate: stats.map(|s| s.msg_in_rate).unwrap_or(0.0),
            msg_out_rate: stats.map(|s| s.msg_out_rate).unwrap_or(0.0),
            retained: stats.map(|s| s.retained).unwrap_or(0),
            restart_needed: inner.restart_needed,
            error: inner.error.clone(),
            listeners,
        }
    }

    /// True while the broker runs.
    pub fn is_running(&self) -> bool {
        self.lock().handle.is_some()
    }

    /// The runtime port of a listener while running (equal to the configured port).
    pub fn running_port(&self, listener_id: &str) -> Option<u16> {
        self.lock()
            .handle
            .as_ref()
            .and_then(|h| h.port(listener_id))
    }

    /// Builds the runtime configuration from the stored state and the secret store.
    pub(crate) fn runtime_config(&self) -> Result<RuntimeConfig, BrokerError> {
        let inner = self.lock();
        let st = &inner.stored;
        let tls = self.tls_material(st);
        let listeners = st
            .listeners
            .iter()
            .filter(|l| l.enabled)
            .map(|l| ListenerSpec {
                id: l.id.clone(),
                kind: l.kind,
                bind: match l.bind {
                    Bind::Network => BindAddr::Network,
                    Bind::Localhost => BindAddr::Localhost,
                },
                port: l.port,
                tls: if l.kind.encrypted() {
                    tls.clone()
                } else {
                    None
                },
            })
            .collect::<Vec<_>>();
        if listeners.is_empty() {
            return Err(BrokerError::new("broker.noListener", &[]));
        }
        Ok(RuntimeConfig {
            listeners,
            auth: self.auth_config(st),
            require_client_cert: st.require_client_cert,
            max_clients: st.max_clients as usize,
            max_packet_size: crate::runtime::DEFAULT_MAX_PACKET,
            revoked: self.revoked.clone(),
            log: self.log.clone(),
        })
    }

    fn tls_material(&self, st: &Stored) -> Option<Arc<TlsMaterial>> {
        let chain = parse_certs(st.server_pem.as_bytes()).ok()?;
        let key_pem = self.server_key()?;
        let key = parse_private_key(key_pem.as_bytes(), None).ok()?;
        let roots_pem = match st.verify_against {
            VerifyAgainst::Local => Some(st.ca_pem.clone()),
            VerifyAgainst::Imported => st.imported_ca_pem.clone(),
        };
        let client_roots = roots_pem
            .and_then(|p| parse_certs(p.as_bytes()).ok())
            .unwrap_or_default();
        Some(Arc::new(TlsMaterial {
            chain,
            key,
            client_roots,
        }))
    }

    /// Starts the broker (no-op when running).
    pub async fn start(&self) -> BrokerStatus {
        if self.is_running() {
            return self.status();
        }
        {
            let mut inner = self.lock();
            inner.state = BrokerState::Starting;
            inner.error = None;
        }
        let cfg = self.runtime_config();
        let result = match cfg {
            Ok(cfg) => Broker::start(cfg).await,
            Err(e) => Err(e),
        };
        {
            let mut inner = self.lock();
            match result {
                Ok(h) => {
                    inner.handle = Some(h);
                    inner.state = BrokerState::Running;
                    inner.restart_needed = false;
                }
                Err(e) => {
                    if let Some(d) = &e.detail {
                        self.log.error(format!("broker could not start: {d}"));
                    }
                    inner.state = BrokerState::Failed;
                    inner.error = Some(e.message);
                }
            }
        }
        self.status()
    }

    /// Stops the broker and remembers when issued certificates were last seen.
    pub async fn stop(&self) -> BrokerStatus {
        let handle = {
            let mut inner = self.lock();
            let h = inner.handle.take();
            if let Some(h) = &h {
                let seen = h.cert_seen();
                for c in &mut inner.stored.issued {
                    if let Some(sn) = seen.get(&normalize_serial(&c.serial)) {
                        if let Some(t) = sn.last_seen {
                            c.last_seen = Some(c.last_seen.map_or(t, |old| old.max(t)));
                        }
                    }
                }
                let _ = self.save(&inner.stored);
            }
            inner.state = BrokerState::Stopped;
            inner.restart_needed = false;
            inner.error = None;
            h
        };
        if let Some(h) = handle {
            h.stop().await;
        }
        self.status()
    }

    /// Stops and starts again (applies listener and certificate changes).
    pub async fn restart(&self) -> BrokerStatus {
        self.stop().await;
        self.start().await
    }

    /// Validates a listener port: range, duplicates among listeners, and use by another program.
    pub fn validate_port(
        &self,
        port: u32,
        listener_id: Option<&str>,
    ) -> Option<crate::error::UserMessage> {
        if port == 0 || port > 65535 {
            return Some(crate::error::UserMessage::new("broker.portRange", &[]));
        }
        if port < 1024 {
            return Some(crate::error::UserMessage::new("broker.portPrivileged", &[]));
        }
        let port = port as u16;
        let inner = self.lock();
        if inner
            .stored
            .listeners
            .iter()
            .any(|l| l.port == port && Some(l.id.as_str()) != listener_id)
        {
            return Some(crate::error::UserMessage::new(
                "broker.portDuplicate",
                &[("port", n(port))],
            ));
        }
        if inner.handle.as_ref().is_some_and(|h| h.uses_port(port)) {
            return None;
        }
        drop(inner);
        if port_in_use(port) {
            return Some(match process_on_port(port) {
                Some(p) => crate::error::UserMessage::new(
                    "broker.portInUseBy",
                    &[("port", n(port)), ("process", s(p))],
                ),
                None => crate::error::UserMessage::new("broker.portInUse", &[("port", n(port))]),
            });
        }
        None
    }

    fn check_port(&self, port: u32, listener_id: Option<&str>) -> Result<u16, BrokerError> {
        match self.validate_port(port, listener_id) {
            Some(m) => Err(BrokerError {
                message: m,
                detail: None,
            }),
            None => Ok(port as u16),
        }
    }

    /// Adds a listener with a setup preset.
    ///
    /// - plain: TCP (or WS), no encryption, labelled "Plain, lab only". Global anonymous stays as it is.
    /// - encrypted: TLS (or WSS) with the local CA's server certificate; client certificate optional.
    /// - passwords: encrypted; clients log in with a shared user (there is no per-listener login).
    /// - mtls: TLS (or WSS); sets the broker-wide "client certificate on encrypted listeners" to Required, verified by
    ///   the local CA (or the imported CA when that is the current choice).
    pub fn add_listener(&self, req: AddListenerRequest) -> Result<BrokerConfig, BrokerError> {
        let port = self.check_port(req.port, None)?;
        let kind = match (req.preset, req.kind) {
            (Preset::Plain, ListenerKind::Tls) => ListenerKind::Tcp,
            (Preset::Plain, ListenerKind::Wss) => ListenerKind::Ws,
            (Preset::Plain, k) => k,
            (_, ListenerKind::Tcp) => ListenerKind::Tls,
            (_, ListenerKind::Ws) => ListenerKind::Wss,
            (_, k) => k,
        };
        let mut inner = self.lock();
        let prefix = kind.id_prefix();
        let mut id = prefix.to_string();
        let mut i = 2;
        while inner.stored.listeners.iter().any(|l| l.id == id) {
            id = format!("{prefix}-{i}");
            i += 1;
        }
        inner.stored.listeners.push(StoredListener {
            id: id.clone(),
            kind,
            bind: req.bind,
            port,
            enabled: true,
            preset: req.preset,
        });
        if req.preset == Preset::Mtls {
            inner.stored.require_client_cert = true;
            inner.stored.verify_against = if inner.stored.imported_ca_pem.is_some() {
                inner.stored.verify_against
            } else {
                VerifyAgainst::Local
            };
        }
        self.log
            .info(format!("{} listener added on port {port}", kind.label()));
        self.commit(&mut inner, true)?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Updates kind, bind, port, enabled and preset of a listener.
    pub fn update_listener(&self, l: Listener) -> Result<BrokerConfig, BrokerError> {
        let current_port = self
            .lock()
            .stored
            .listeners
            .iter()
            .find(|x| x.id == l.id)
            .map(|x| x.port);
        let Some(current_port) = current_port else {
            return Err(BrokerError::internal(format!("no listener {}", l.id)));
        };
        if l.port != current_port {
            self.check_port(u32::from(l.port), Some(&l.id))?;
        }
        let mut inner = self.lock();
        if let Some(x) = inner.stored.listeners.iter_mut().find(|x| x.id == l.id) {
            x.kind = l.kind;
            x.bind = l.bind;
            x.port = l.port;
            x.enabled = l.enabled;
            x.preset = l.preset;
        }
        self.commit(&mut inner, true)?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Removes a listener.
    pub fn remove_listener(&self, listener_id: &str) -> Result<BrokerConfig, BrokerError> {
        let mut inner = self.lock();
        inner.stored.listeners.retain(|l| l.id != listener_id);
        self.commit(&mut inner, true)?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Turns anonymous access on or off; applies immediately when running.
    pub fn set_anonymous(&self, on: bool) -> Result<BrokerConfig, BrokerError> {
        let mut inner = self.lock();
        inner.stored.anonymous = on;
        self.log.warn(if on {
            "anonymous access on: users and client certificates are not checked"
        } else {
            "anonymous access off"
        });
        self.commit(&mut inner, false)?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Client certificate on encrypted listeners: Required (true) or Optional (false), for the whole broker. Plain
    /// listeners are never affected. Takes effect at the next start (`restartNeeded` while running).
    pub fn set_require_client_cert(&self, on: bool) -> Result<BrokerConfig, BrokerError> {
        let mut inner = self.lock();
        let changed = inner.stored.require_client_cert != on;
        inner.stored.require_client_cert = on;
        if changed {
            self.log.info(if on {
                "client certificate required on encrypted listeners"
            } else {
                "client certificate optional on encrypted listeners"
            });
        }
        self.commit(&mut inner, changed)?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Chooses the CA that verifies client certificates. "imported" needs an imported CA.
    pub fn set_verify_against(&self, source: VerifyAgainst) -> Result<BrokerConfig, BrokerError> {
        let mut inner = self.lock();
        if source == VerifyAgainst::Imported && inner.stored.imported_ca_pem.is_none() {
            // Closest catalog sentence until a dedicated key exists (see report).
            return Err(BrokerError::new("file.notCertificate", &[]));
        }
        let changed = inner.stored.verify_against != source;
        inner.stored.verify_against = source;
        self.commit(&mut inner, changed)?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Adds a shared user (password argon2-hashed in broker.json).
    pub fn add_user(&self, username: &str, password: &str) -> Result<BrokerConfig, BrokerError> {
        if !valid_username(username) {
            return Err(BrokerError::new("broker.userInvalid", &[]));
        }
        if self
            .lock()
            .stored
            .users
            .iter()
            .any(|u| u.username == username)
        {
            return Err(BrokerError::new(
                "broker.userExists",
                &[("user", s(username))],
            ));
        }
        self.insert_user(username, password, false)?;
        Ok(self.config())
    }

    fn insert_user(&self, username: &str, password: &str, system: bool) -> Result<(), BrokerError> {
        let hash = hash_password(password).map_err(BrokerError::internal)?;
        let mut inner = self.lock();
        if inner.stored.users.iter().any(|u| u.username == username) {
            return Err(BrokerError::new(
                "broker.userExists",
                &[("user", s(username))],
            ));
        }
        self.secrets
            .set(&secret_user_password(username), password)
            .map_err(|e| BrokerError::keychain(&e))?;
        inner.stored.users.push(StoredUser {
            username: username.to_string(),
            password_hash: hash,
            system,
        });
        self.commit(&mut inner, false)
    }

    /// Changes a shared user's password.
    pub fn set_user_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<BrokerConfig, BrokerError> {
        if !self
            .lock()
            .stored
            .users
            .iter()
            .any(|u| u.username == username)
        {
            return Err(BrokerError::new("broker.userInvalid", &[]));
        }
        let hash = hash_password(password).map_err(BrokerError::internal)?;
        {
            let mut inner = self.lock();
            self.secrets
                .set(&secret_user_password(username), password)
                .map_err(|e| BrokerError::keychain(&e))?;
            if let Some(u) = inner
                .stored
                .users
                .iter_mut()
                .find(|u| u.username == username)
            {
                u.password_hash = hash;
            }
            self.commit(&mut inner, false)?;
        }
        Ok(self.config())
    }

    /// Removes a shared user and its password.
    pub fn remove_user(&self, username: &str) -> Result<BrokerConfig, BrokerError> {
        {
            let mut inner = self.lock();
            inner.stored.users.retain(|u| u.username != username);
            let _ = self.secrets.delete(&secret_user_password(username));
            self.commit(&mut inner, false)?;
        }
        Ok(self.config())
    }

    /// The plain password of a shared user, for the eye and copy icons (None when it was never kept).
    pub fn user_password(&self, username: &str) -> Result<Option<String>, BrokerError> {
        if !self.lock().stored.users.iter().any(|u| u.username == username) {
            return Ok(None);
        }
        self.secret(&secret_user_password(username))
    }

    /// Makes sure the user "mqtt-panel" exists with a strong generated password kept in the secret store.
    /// Returns `(created, password)`: an existing user keeps its password; when its password was never kept, a new
    /// one is generated (`created` stays false). Applies live, no restart.
    pub fn ensure_app_user(&self) -> Result<(bool, String), BrokerError> {
        let exists = self
            .lock()
            .stored
            .users
            .iter()
            .any(|u| u.username == APP_USER_NAME);
        if !exists {
            let pw = generate_password(APP_PASSWORD_LEN);
            self.insert_user(APP_USER_NAME, &pw, true)?;
            self.log
                .info(format!("user {APP_USER_NAME} created for the profile \"Local broker\""));
            return Ok((true, pw));
        }
        {
            let mut inner = self.lock();
            if let Some(u) = inner.stored.users.iter_mut().find(|u| u.username == APP_USER_NAME) {
                if !u.system {
                    u.system = true;
                    self.commit(&mut inner, false)?;
                }
            }
        }
        if let Some(pw) = self.secret(&secret_user_password(APP_USER_NAME))? {
            return Ok((false, pw));
        }
        let pw = generate_password(APP_PASSWORD_LEN);
        self.set_user_password(APP_USER_NAME, &pw)?;
        Ok((false, pw))
    }

    /// Issued client certificates with "last seen".
    pub fn issued_certs(&self) -> Vec<IssuedCert> {
        let inner = self.lock();
        let seen = inner
            .handle
            .as_ref()
            .map(|h| h.cert_seen())
            .unwrap_or_default();
        let ca_fp = fingerprint_of_pem(&inner.stored.ca_pem);
        inner
            .stored
            .issued
            .iter()
            .map(|c| {
                let rt = seen.get(&normalize_serial(&c.serial));
                let last_seen = if rt.is_some_and(|r| r.connected > 0) {
                    LastSeen::Connected
                } else {
                    match (c.last_seen, rt.and_then(|r| r.last_seen)) {
                        (None, None) => LastSeen::Never,
                        (a, b) => LastSeen::At(a.unwrap_or(0).max(b.unwrap_or(0))),
                    }
                };
                IssuedCert {
                    name: c.name.clone(),
                    serial: c.serial.clone(),
                    issued_at: c.issued_at.clone(),
                    expires_at: c.expires_at.clone(),
                    key_type: c.key_type.clone(),
                    last_seen,
                    revoked: c.revoked,
                    system: c.system,
                    superseded: c.ca_fingerprint != ca_fp,
                }
            })
            .collect()
    }

    fn load_ca(&self, st: &Stored) -> Result<Ca, BrokerError> {
        let key = self.secret(SECRET_CA_KEY)?.ok_or_else(|| {
            BrokerError::new(
                "keychain.unavailable",
                &[("detail", s("the local CA key is missing"))],
            )
        })?;
        Ca::from_pem(&st.ca_pem, &key).map_err(|e| BrokerError::kit(&e))
    }

    fn active_cert_exists(st: &Stored, name: &str) -> bool {
        let ca_fp = fingerprint_of_pem(&st.ca_pem);
        st.issued
            .iter()
            .any(|c| c.name == name && !c.revoked && c.ca_fingerprint == ca_fp)
    }

    #[allow(clippy::too_many_arguments)]
    fn files_for(
        &self,
        st: &Stored,
        name: &str,
        cert_pem: &str,
        key_pem: Option<&str>,
        format: CertFormat,
        passphrase: &str,
        save_ca: bool,
    ) -> Result<Vec<SaveFile>, BrokerError> {
        let mut files = Vec::new();
        match (format, key_pem) {
            (CertFormat::Pkcs12, Some(key_pem)) => {
                let cert = parse_certs(cert_pem.as_bytes())
                    .map_err(|e| BrokerError::tls_file(&e, name))?;
                let key = parse_private_key(key_pem.as_bytes(), None)
                    .map_err(|e| BrokerError::tls_file(&e, name))?;
                let PrivateKeyDer::Pkcs8(k8) = key else {
                    return Err(BrokerError::internal("key is not PKCS#8"));
                };
                let chain: Vec<Vec<u8>> = parse_certs(st.ca_pem.as_bytes())
                    .map(|c| c.iter().map(|c| c.as_ref().to_vec()).collect())
                    .unwrap_or_default();
                let first = cert
                    .first()
                    .ok_or_else(|| BrokerError::new("file.notCertificate", &[]))?;
                let p12 = kit::pkcs12(
                    first.as_ref(),
                    &chain,
                    k8.secret_pkcs8_der(),
                    passphrase,
                    name,
                )
                .map_err(|e| BrokerError::kit(&e))?;
                files.push((format!("{name}.p12"), p12));
            }
            (_, Some(key_pem)) => {
                let key = parse_private_key(key_pem.as_bytes(), None)
                    .map_err(|e| BrokerError::tls_file(&e, name))?;
                let PrivateKeyDer::Pkcs8(k8) = key else {
                    return Err(BrokerError::internal("key is not PKCS#8"));
                };
                let key_out = kit::key_pem_with_passphrase(k8.secret_pkcs8_der(), passphrase)
                    .map_err(|e| BrokerError::kit(&e))?;
                files.push((format!("{name}.pem"), cert_pem.as_bytes().to_vec()));
                files.push((format!("{name}.key"), key_out.into_bytes()));
            }
            (_, None) => files.push((format!("{name}.pem"), cert_pem.as_bytes().to_vec())),
        }
        if save_ca {
            files.push(("CA.pem".to_string(), st.ca_pem.as_bytes().to_vec()));
        }
        Ok(files)
    }

    /// Issues a client certificate from the local CA. Returns the files to save; the key and the passphrase are
    /// also kept in the secret store so the certificate can be exported again later.
    pub fn issue_client_cert(&self, req: IssueCertRequest) -> Result<Vec<SaveFile>, BrokerError> {
        if !valid_cert_name(&req.name) {
            return Err(BrokerError::new("broker.nameInvalid", &[]));
        }
        let mut inner = self.lock();
        if Self::active_cert_exists(&inner.stored, &req.name) {
            return Err(BrokerError::new(
                "broker.certExists",
                &[("name", s(req.name.clone()))],
            ));
        }
        let ca = self.load_ca(&inner.stored)?;
        let years = i64::from(req.valid_years.clamp(1, 10));
        let issued = ca
            .issue(
                &req.name,
                &[],
                Usage::Client,
                req.key_type,
                Validity::years(years),
            )
            .map_err(|e| BrokerError::kit(&e))?;
        self.secrets
            .set(&secret_client_key(&req.name), &issued.key_pem)
            .map_err(|e| BrokerError::keychain(&e))?;
        if req.passphrase.is_empty() {
            let _ = self.secrets.delete(&secret_client_passphrase(&req.name));
        } else {
            self.secrets
                .set(&secret_client_passphrase(&req.name), &req.passphrase)
                .map_err(|e| BrokerError::keychain(&e))?;
        }
        let ca_fp = fingerprint_of_pem(&inner.stored.ca_pem);
        inner.stored.issued.retain(|c| c.name != req.name);
        inner
            .stored
            .issued
            .push(stored_cert_from(&issued, false, true, &ca_fp));
        let files = self.files_for(
            &inner.stored,
            &req.name,
            &issued.cert_pem,
            Some(&issued.key_pem),
            req.format,
            &req.passphrase,
            req.save_ca,
        )?;
        self.log
            .info(format!("client certificate {} issued", req.name));
        self.commit(&mut inner, false)?;
        Ok(files)
    }

    /// Signs a certificate request (the key stays on the device). `name` overrides the request's common name.
    pub fn import_csr(
        &self,
        csr: &[u8],
        name: Option<&str>,
        valid_years: u8,
    ) -> Result<SaveFile, BrokerError> {
        if let Some(nm) = name {
            if !valid_cert_name(nm) {
                return Err(BrokerError::new("broker.nameInvalid", &[]));
            }
        }
        let mut inner = self.lock();
        let ca = self.load_ca(&inner.stored)?;
        let issued = ca
            .sign_csr(
                csr,
                name,
                Validity::years(i64::from(valid_years.clamp(1, 10))),
            )
            .map_err(|e| BrokerError::kit(&e))?;
        if !valid_cert_name(&issued.cn) {
            return Err(BrokerError::new("broker.nameInvalid", &[]));
        }
        if Self::active_cert_exists(&inner.stored, &issued.cn) {
            return Err(BrokerError::new(
                "broker.certExists",
                &[("name", s(issued.cn.clone()))],
            ));
        }
        let ca_fp = fingerprint_of_pem(&inner.stored.ca_pem);
        inner.stored.issued.retain(|c| c.name != issued.cn);
        let _ = self.secrets.delete(&secret_client_key(&issued.cn));
        inner
            .stored
            .issued
            .push(stored_cert_from(&issued, false, false, &ca_fp));
        self.log
            .info(format!("certificate request {} signed", issued.cn));
        self.commit(&mut inner, false)?;
        Ok((format!("{}.pem", issued.cn), issued.cert_pem.into_bytes()))
    }

    /// Exports an issued certificate again. Certificates signed from a CSR export the certificate only.
    pub fn export_client_cert(
        &self,
        name: &str,
        format: CertFormat,
        passphrase: &str,
    ) -> Result<Vec<SaveFile>, BrokerError> {
        let inner = self.lock();
        let c = inner
            .stored
            .issued
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| BrokerError::internal(format!("no certificate {name}")))?;
        let key = if c.has_key {
            self.secret(&secret_client_key(name))?
        } else {
            None
        };
        if !passphrase.is_empty() && key.is_some() {
            let _ = self
                .secrets
                .set(&secret_client_passphrase(name), passphrase);
        }
        self.files_for(
            &inner.stored,
            name,
            &c.cert_pem,
            key.as_deref(),
            format,
            passphrase,
            false,
        )
    }

    /// Revokes an issued certificate: refused from the next TLS handshake on, also while running.
    pub fn revoke_client_cert(&self, name: &str) -> Result<Vec<IssuedCert>, BrokerError> {
        {
            let mut inner = self.lock();
            let mut serials = Vec::new();
            for c in inner
                .stored
                .issued
                .iter_mut()
                .filter(|c| c.name == name && !c.revoked)
            {
                c.revoked = true;
                serials.push(c.serial.clone());
            }
            for sn in serials {
                self.revoked.revoke(&sn);
                if !inner.stored.revoked_serials.contains(&sn) {
                    inner.stored.revoked_serials.push(sn);
                }
            }
            let _ = self.secrets.delete(&secret_client_key(name));
            let _ = self.secrets.delete(&secret_client_passphrase(name));
            self.log.warn(format!("client certificate {name} revoked"));
            self.commit(&mut inner, false)?;
        }
        Ok(self.issued_certs())
    }

    /// The CA, the server certificate and the imported client CA.
    pub fn broker_certs(&self) -> Result<BrokerCerts, BrokerError> {
        let inner = self.lock();
        let st = &inner.stored;
        Ok(BrokerCerts {
            ca: details_of_pem(&st.ca_pem)?,
            server: details_of_pem(&st.server_pem)?,
            imported: st.server_imported,
            imported_ca: st
                .imported_ca_pem
                .as_deref()
                .map(details_of_pem)
                .transpose()?,
        })
    }

    /// The local CA certificate as "CA.pem".
    pub fn export_ca(&self) -> SaveFile {
        (
            "CA.pem".to_string(),
            self.lock().stored.ca_pem.clone().into_bytes(),
        )
    }

    /// The local CA certificate (PEM).
    pub fn ca_pem(&self) -> String {
        self.lock().stored.ca_pem.clone()
    }

    /// Regenerates the server certificate from the local CA (empty `sans`: localhost and the host name).
    pub fn regenerate(
        &self,
        sans: &[String],
        key_type: KeyAlg,
    ) -> Result<BrokerCerts, BrokerError> {
        {
            let mut inner = self.lock();
            let ca = self.load_ca(&inner.stored)?;
            let sans: Vec<String> = sans
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let sans = if sans.is_empty() {
                kit::local_sans()
            } else {
                sans
            };
            let server = ca
                .issue(
                    SERVER_CERT_NAME,
                    &sans,
                    Usage::Server,
                    key_type,
                    Validity::years(2),
                )
                .map_err(|e| BrokerError::kit(&e))?;
            self.set_server_key(&server.key_pem)?;
            inner.stored.server_pem = format!("{}{}", server.cert_pem, inner.stored.ca_pem);
            inner.stored.server_imported = false;
            self.log.info("server certificate regenerated");
            self.commit(&mut inner, true)?;
        }
        self.broker_certs()
    }

    /// Imports a server certificate and key: PEM files, or one PKCS#12 bundle as `cert` with `key = None`.
    pub fn import_server_cert(
        &self,
        cert: &[u8],
        key: Option<&[u8]>,
        passphrase: Option<&str>,
    ) -> Result<BrokerCerts, BrokerError> {
        let id = mqttpanel_tls::load_identity(cert, key, passphrase)
            .map_err(|e| BrokerError::tls_file(&e, "certificate"))?;
        let provider = rustls::crypto::ring::default_provider();
        rustls::sign::CertifiedKey::from_der(id.certs.clone(), id.key.clone_key(), &provider)
            .map_err(|e| BrokerError::new("tls.keyMismatch", &[]).with_detail(e.to_string()))?;
        let key_pem = key_pem_of(&id.key);
        if key_pem.is_empty() {
            return Err(BrokerError::new("file.notKey", &[]));
        }
        {
            let mut inner = self.lock();
            self.set_server_key(&key_pem)?;
            inner.stored.server_pem = certs_pem(&id.certs);
            inner.stored.server_imported = true;
            self.log.info("server certificate imported");
            self.commit(&mut inner, true)?;
        }
        self.broker_certs()
    }

    /// Imports a CA used to verify client certificates when "Verify against" is "imported".
    pub fn import_client_ca(&self, bytes: &[u8]) -> Result<BrokerCerts, BrokerError> {
        let certs = parse_certs(bytes).map_err(|e| BrokerError::tls_file(&e, "CA"))?;
        {
            let mut inner = self.lock();
            inner.stored.imported_ca_pem = Some(certs_pem(&certs));
            let needs = inner.stored.verify_against == VerifyAgainst::Imported;
            self.commit(&mut inner, needs)?;
        }
        self.broker_certs()
    }

    /// Replaces the local CA and the server certificate. Client certificates issued before no longer verify.
    pub fn rotate_ca(&self) -> Result<BrokerCerts, BrokerError> {
        {
            let mut inner = self.lock();
            let sans = if inner.stored.server_imported {
                kit::local_sans()
            } else {
                details_of_pem(&inner.stored.server_pem)
                    .map(|d| d.san)
                    .unwrap_or_else(|_| kit::local_sans())
            };
            let ca = Ca::new_root(LOCAL_CA_NAME, KeyAlg::Ecdsa, Validity::years(5))
                .map_err(|e| BrokerError::kit(&e))?;
            let server = ca
                .issue(
                    SERVER_CERT_NAME,
                    &sans,
                    Usage::Server,
                    KeyAlg::Ecdsa,
                    Validity::years(2),
                )
                .map_err(|e| BrokerError::kit(&e))?;
            self.secrets
                .set(SECRET_CA_KEY, &ca.key_pem)
                .map_err(|e| BrokerError::keychain(&e))?;
            self.set_server_key(&server.key_pem)?;
            inner.stored.ca_pem = ca.cert_pem.clone();
            inner.stored.server_pem = format!("{}{}", server.cert_pem, ca.cert_pem);
            inner.stored.server_imported = false;
            self.log
                .warn("CA rotated: issued client certificates must be re-issued");
            self.commit(&mut inner, true)?;
        }
        self.broker_certs()
    }

    /// Connected clients (empty when stopped).
    pub fn clients(&self) -> Vec<BrokerClient> {
        self.lock()
            .handle
            .as_ref()
            .map(|h| h.clients())
            .unwrap_or_default()
    }

    /// Disconnects a client.
    pub fn disconnect_client(&self, client_id: &str) -> Result<(), BrokerError> {
        let inner = self.lock();
        let h = inner
            .handle
            .as_ref()
            .ok_or_else(|| BrokerError::new("broker.notRunning", &[]))?;
        h.disconnect_client(client_id);
        Ok(())
    }

    /// Security rank of a listener for the "Connect to this app's broker" preselection: mutual TLS, then TLS, then
    /// secure WebSocket, then TCP, then WebSocket (higher is more secure).
    /// `require_client_cert` is the broker-wide setting ([`BrokerConfig::require_client_cert`]).
    pub fn listener_rank(l: &Listener, require_client_cert: bool) -> u8 {
        let mtls = l.kind.encrypted() && require_client_cert;
        match (mtls, l.kind) {
            (true, ListenerKind::Tls) => 6,
            (true, _) => 5,
            (false, ListenerKind::Tls) => 4,
            (false, ListenerKind::Wss) => 3,
            (false, ListenerKind::Tcp) => 2,
            (false, ListenerKind::Ws) => 1,
        }
    }

    /// The most secure enabled listener (see [`BrokerManager::listener_rank`]).
    pub fn preferred_listener(&self) -> Option<Listener> {
        let cfg = self.config();
        let req = cfg.require_client_cert;
        let mut best: Option<Listener> = None;
        for l in cfg.listeners.into_iter().filter(|l| l.enabled) {
            if best
                .as_ref()
                .is_none_or(|b| Self::listener_rank(&l, req) > Self::listener_rank(b, req))
            {
                best = Some(l);
            }
        }
        best
    }

    /// True when this listener asks the app for its client certificate "mqttpanel-desktop": it requires one and
    /// Anonymous is off (with Anonymous on no client certificate is requested).
    pub fn needs_desktop_cert(&self, listener_id: &str) -> bool {
        let inner = self.lock();
        !inner.stored.anonymous
            && inner.stored.require_client_cert
            && inner
                .stored
                .listeners
                .iter()
                .any(|l| l.id == listener_id && l.kind.encrypted())
    }

    /// Makes sure the client certificate "mqttpanel-desktop" exists, issued by the current CA, not revoked, with its
    /// key in the secret store. Returns `(issued, cert_pem, key_pem)`; `issued` is false when the existing one is
    /// kept. Re-issued after a CA rotation or a revocation.
    pub fn ensure_desktop_cert(&self) -> Result<(bool, String, String), BrokerError> {
        let mut inner = self.lock();
        let ca_fp = fingerprint_of_pem(&inner.stored.ca_pem);
        let existing = inner
            .stored
            .issued
            .iter()
            .find(|c| {
                c.name == DESKTOP_CERT_NAME
                    && c.system
                    && !c.revoked
                    && c.ca_fingerprint == ca_fp
                    && c.has_key
            })
            .cloned();
        if let Some(c) = existing {
            if let Some(k) = self.secret(&secret_client_key(DESKTOP_CERT_NAME))? {
                return Ok((false, c.cert_pem, k));
            }
        }
        let ca = self.load_ca(&inner.stored)?;
        let issued = ca
            .issue(
                DESKTOP_CERT_NAME,
                &[],
                Usage::Client,
                KeyAlg::Ecdsa,
                Validity::years(2),
            )
            .map_err(|e| BrokerError::kit(&e))?;
        self.secrets
            .set(&secret_client_key(DESKTOP_CERT_NAME), &issued.key_pem)
            .map_err(|e| BrokerError::keychain(&e))?;
        inner.stored.issued.retain(|c| c.name != DESKTOP_CERT_NAME);
        inner
            .stored
            .issued
            .push(stored_cert_from(&issued, true, true, &ca_fp));
        self.log.info(format!(
            "client certificate {DESKTOP_CERT_NAME} issued for the profile \"Local broker\""
        ));
        self.commit(&mut inner, false)?;
        Ok((true, issued.cert_pem, issued.key_pem))
    }

    /// Connection settings for the profile "Local broker": the given listener or the most secure enabled one.
    /// With Anonymous off it carries the user "mqtt-panel" (created when missing) and, when the listener requires
    /// one, the client certificate "mqttpanel-desktop" (issued when missing, see [`BrokerManager::ensure_desktop_cert`]).
    /// TLS and WSS listeners carry the broker's own CA.
    pub fn local_connection(
        &self,
        listener_id: Option<&str>,
    ) -> Result<LocalConnection, BrokerError> {
        let l = match listener_id {
            Some(id) => self.config().listeners.into_iter().find(|l| l.id == id),
            None => self.preferred_listener(),
        }
        .ok_or_else(|| BrokerError::new("broker.noListener", &[]))?;
        let (port, anonymous, ca_pem) = {
            let inner = self.lock();
            (
                inner
                    .handle
                    .as_ref()
                    .and_then(|h| h.port(&l.id))
                    .unwrap_or(l.port),
                inner.stored.anonymous,
                inner.stored.ca_pem.clone(),
            )
        };
        let mut lc = LocalConnection {
            listener_id: l.id.clone(),
            host: "localhost".into(),
            port,
            scheme: l.kind.scheme().into(),
            ws_path: l
                .kind
                .websocket()
                .then(|| crate::runtime::ws::WS_PATH.to_string()),
            ca_pem: l.kind.encrypted().then_some(ca_pem),
            client_cert_pem: None,
            client_key_pem: None,
            username: None,
            password: None,
        };
        if !anonymous {
            let (_, pw) = self.ensure_app_user()?;
            lc.username = Some(APP_USER_NAME.to_string());
            lc.password = Some(pw);
        }
        if self.needs_desktop_cert(&l.id) {
            let (_, cert, key) = self.ensure_desktop_cert()?;
            lc.client_cert_pem = Some(cert);
            lc.client_key_pem = Some(key);
        }
        Ok(lc)
    }

    /// True when Anonymous is on.
    pub fn anonymous(&self) -> bool {
        self.lock().stored.anonymous
    }

    /// True when a saved change waits for a restart of the running broker.
    pub fn restart_needed(&self) -> bool {
        self.lock().restart_needed
    }
}

impl RuntimeConfig {
    /// The runtime configuration of the desktop app's broker (CLI `mqttpanel broker` without `--config`).
    pub fn from_manager(manager: &BrokerManager) -> Result<RuntimeConfig, BrokerError> {
        manager.runtime_config()
    }
}
