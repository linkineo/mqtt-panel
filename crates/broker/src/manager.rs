//! Persistent broker management for the desktop app and the CLI: `broker.json` in a caller-chosen directory,
//! secrets (CA and server keys, user passwords, issued client keys and passphrases) in a [`SecretStore`].

use crate::config::*;
use crate::error::{n, s, BrokerError};
use crate::runtime::{
    hash_password, normalize_serial, sessions_text, AuthConfig, BindAddr, Broker, BrokerHandle,
    Credential, ListenerSpec, LogBuffer, RevocationList, RuntimeConfig, TlsMaterial,
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
    /// Per-listener reachability of files written before change set 4: read once to derive the broker-wide
    /// [`Stored::bind`] (network wins), never written again.
    #[serde(default, rename = "bind", skip_serializing)]
    legacy_bind: Option<Bind>,
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
    /// Reachability of every listener (change set 4). Missing in older files: derived from their per-listener values
    /// when the file is opened (network when any listener had it, else localhost; network without listeners).
    #[serde(default)]
    bind: Option<Bind>,
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
    /// Extra names of the generated server certificate (change set 11), typed in the Regenerate dialog.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extra_sans: Vec<String>,
}

fn default_listener(id: &str, kind: ListenerKind, port: u16, enabled: bool) -> StoredListener {
    StoredListener {
        id: id.into(),
        kind,
        legacy_bind: None,
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
    /// The running broker. Shared so a live change can await listener tasks without holding the lock.
    handle: Option<Arc<BrokerHandle>>,
    state: BrokerState,
    error: Option<crate::error::UserMessage>,
    /// Set when a start regenerated the local CA because its key was missing; taken by the shell, which then lets the
    /// "Local broker" profile follow the new CA.
    ca_regenerated: bool,
}

/// Manages the broker configuration, certificates and the running broker.
pub struct BrokerManager {
    dir: PathBuf,
    secrets: Arc<dyn SecretStore>,
    inner: Mutex<Inner>,
    log: LogBuffer,
    revoked: RevocationList,
    /// Serialises live changes, start and stop (a listener change awaits its tasks).
    apply: tokio::sync::Mutex<()>,
    /// The server key, read once per session from the secret store and kept in memory, so a later start or a live
    /// certificate change never touches the secret store (and the OS keychain) again (SPEC 6, change set 2).
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

/// The trust a client needs for the server identity the broker serves: `(CA PEM, leaf fingerprint)`.
/// Generated identity: the local CA, no pin. Imported identity: the certificates after the leaf (its CA and
/// intermediates), no pin; or, when the import carried the leaf only, no CA and a pin of the leaf.
fn served_trust(st: &Stored) -> (Option<String>, Option<String>) {
    if !st.server_imported {
        return (Some(st.ca_pem.clone()), None);
    }
    let certs = parse_certs(st.server_pem.as_bytes()).unwrap_or_default();
    match certs.split_first() {
        Some((_, rest)) if !rest.is_empty() => (Some(certs_pem(rest)), None),
        Some((leaf, _)) => (None, Some(mqttpanel_tls::fingerprint_sha256(leaf.as_ref()))),
        None => (None, None),
    }
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

/// What a saved change touches in a running broker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Apply {
    /// Nothing live (issued certificates, the setup label of a listener).
    Save,
    /// Users, Anonymous, the client-certificate rule, revocations.
    Auth,
    /// The same, and the TLS material (server certificate, verifying CA).
    Tls,
}

/// " · 2 sessions dropped: port changed", or nothing.
fn dropped_suffix(n: usize, reason: &str) -> String {
    if n == 0 {
        String::new()
    } else {
        format!(" · {} dropped: {reason}", sessions_text(n))
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
        let mut migrated = false;
        let stored = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|_| BrokerError::new("file.unreadable", &[("file", s(CONFIG_FILE))]))?;
            let mut st = serde_json::from_str::<Stored>(&text).map_err(|e| {
                BrokerError::new("file.unreadable", &[("file", s(CONFIG_FILE))])
                    .with_detail(e.to_string())
            })?;
            if st.bind.is_none() {
                st.bind = Some(Self::legacy_bind(&st.listeners));
                migrated = true;
            }
            for l in &mut st.listeners {
                migrated |= l.legacy_bind.take().is_some();
            }
            st
        } else {
            let ca = Ca::new_root(LOCAL_CA_NAME, KeyAlg::Ecdsa, Validity::years(5))
                .map_err(|e| BrokerError::kit(&e))?;
            let server = ca
                .issue(
                    SERVER_CERT_NAME,
                    &kit::server_sans(&[]),
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
                bind: Some(Bind::Network),
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
                extra_sans: Vec::new(),
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
                ca_regenerated: false,
            }),
            log: LogBuffer::new(),
            revoked,
            apply: tokio::sync::Mutex::new(()),
            server_key: Mutex::new(None),
        };
        if !path.exists() || migrated {
            m.save(&m.lock().stored)?;
        }
        Ok(m)
    }

    /// The broker-wide reachability of a file written before change set 4: network when any listener had it (the
    /// safe reading for devices that already connect over the network), localhost when every listener had it,
    /// network (the default) without listeners or values.
    fn legacy_bind(listeners: &[StoredListener]) -> Bind {
        let binds: Vec<Bind> = listeners.iter().filter_map(|l| l.legacy_bind).collect();
        if !binds.is_empty() && binds.iter().all(|b| *b == Bind::Localhost) {
            Bind::Localhost
        } else {
            Bind::Network
        }
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

    /// Saves and, when the broker runs, applies the change at once (change set 7): the users, Anonymous and the
    /// client-certificate rule are handed over, the TLS material is swapped when `apply` is [`Apply::Tls`], and the
    /// sessions the change invalidates are dropped. Writes `line` (when given) as the one log line of the change,
    /// with " · n sessions dropped: reason" when sessions dropped. Returns the number of sessions dropped.
    fn commit(
        &self,
        inner: &mut Inner,
        apply: Apply,
        line: Option<(LogLevel, String)>,
        reason: &str,
    ) -> Result<usize, BrokerError> {
        self.save(&inner.stored)?;
        let mut dropped = 0;
        if let (Some(h), true) = (&inner.handle, apply != Apply::Save) {
            if apply == Apply::Tls {
                match self.tls_material(&inner.stored) {
                    Ok(m) => {
                        if let Err(e) = h.set_tls_material(m) {
                            self.log
                                .error(format!("new certificates could not be loaded: {e}"));
                        }
                    }
                    Err(e) => self.log.error(format!(
                        "new certificates could not be loaded: {}",
                        e.detail.clone().unwrap_or_else(|| e.key().to_string())
                    )),
                }
            }
            h.update_auth(self.auth_config(&inner.stored));
            h.set_require_client_cert(inner.stored.require_client_cert);
            dropped = h.revalidate();
        }
        let suffix = dropped_suffix(dropped, reason);
        match line {
            Some((level, text)) => {
                self.log.push(level, format!("{text}{suffix}"));
            }
            None if dropped > 0 => {
                self.log
                    .info(format!("{} dropped: {reason}", sessions_text(dropped)));
            }
            None => {}
        }
        Ok(dropped)
    }

    /// The server key: from memory after the first read of the session. `Ok(None)` when the store has none; a store
    /// error (for example the keychain refused the master key) is returned as is.
    fn server_key(&self) -> Result<Option<String>, BrokerError> {
        let mut cached = self.server_key.lock().unwrap_or_else(|p| p.into_inner());
        if cached.is_none() {
            *cached = self
                .secrets
                .get(SECRET_SERVER_KEY)
                .map_err(|e| BrokerError::keychain(&e))?;
        }
        Ok(cached.clone())
    }

    /// True when an enabled listener needs the server certificate and key (TLS or WSS).
    fn needs_tls(st: &Stored) -> bool {
        st.listeners.iter().any(|l| l.enabled && l.kind.encrypted())
    }

    /// Before a start (SPEC 6, change set 3): a server key missing from the secret store issues a new server
    /// certificate from the local CA; a CA key missing too regenerates the local CA first. Same key type as the current
    /// server certificate; the names are the automatic ones read now plus the extra names (change set 11). A store error is returned,
    /// so the start fails with its reason. Nothing is read when no TLS or WSS listener is enabled.
    fn heal_tls(&self) -> Result<(), BrokerError> {
        let mut inner = self.lock();
        if !Self::needs_tls(&inner.stored) || self.server_key()?.is_some() {
            return Ok(());
        }
        let current = if inner.stored.server_imported {
            None
        } else {
            details_of_pem(&inner.stored.server_pem).ok()
        };
        // Change set 11: the automatic names read now (interface addresses may have changed) plus the extra names.
        let sans = kit::server_sans(&inner.stored.extra_sans);
        let key_type = match &current {
            Some(d) if d.key_type.starts_with("RSA") => KeyAlg::Rsa,
            _ => KeyAlg::Ecdsa,
        };
        let (ca, regenerated) = match self.secret(SECRET_CA_KEY)? {
            Some(key) => (
                Ca::from_pem(&inner.stored.ca_pem, &key).map_err(|e| BrokerError::kit(&e))?,
                false,
            ),
            None => (
                Ca::new_root(LOCAL_CA_NAME, KeyAlg::Ecdsa, Validity::years(5))
                    .map_err(|e| BrokerError::kit(&e))?,
                true,
            ),
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
        if regenerated {
            self.secrets
                .set(SECRET_CA_KEY, &ca.key_pem)
                .map_err(|e| BrokerError::keychain(&e))?;
            inner.stored.ca_pem = ca.cert_pem.clone();
        }
        self.set_server_key(&server.key_pem)?;
        inner.stored.server_pem = format!("{}{}", server.cert_pem, inner.stored.ca_pem);
        inner.stored.server_imported = false;
        if regenerated {
            inner.ca_regenerated = true;
            self.log.warn(
                "local CA regenerated: its key was missing; issued client certificates must be issued again",
            );
        } else {
            self.log
                .warn("server certificate issued again: its key was missing");
        }
        self.save(&inner.stored)
    }

    /// True (once) when a start regenerated the local CA because its key was missing.
    pub fn take_ca_regenerated(&self) -> bool {
        std::mem::take(&mut self.lock().ca_regenerated)
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
            bind: st.bind.unwrap_or_default(),
            listeners: st
                .listeners
                .iter()
                .map(|l| Listener {
                    id: l.id.clone(),
                    kind: l.kind,
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
            extra_sans: st.extra_sans.clone(),
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

    /// Current status. While the broker runs, a listener that could not start carries its error and the state stays
    /// Running as long as one listener serves (Failed when none does).
    pub fn status(&self) -> BrokerStatus {
        let inner = self.lock();
        let st = &inner.stored;
        let (stats, listeners, state, error) = match &inner.handle {
            Some(h) => {
                let rt = h.listeners();
                let ls: Vec<ListenerStatus> = st
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
                let (state, error) = if ls.iter().any(|l| l.up) {
                    (BrokerState::Running, None)
                } else {
                    let e = ls.iter().find_map(|l| l.error.clone()).unwrap_or_else(|| {
                        crate::error::UserMessage::new("broker.noListener", &[])
                    });
                    (BrokerState::Failed, Some(e))
                };
                (Some(h.stats()), ls, state, error)
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
                inner.state,
                inner.error.clone(),
            ),
        };
        BrokerStatus {
            state,
            started_at: stats.map(|s| s.started_at),
            clients: stats.map(|s| s.clients).unwrap_or(0),
            max_clients: st.max_clients,
            msg_in_rate: stats.map(|s| s.msg_in_rate).unwrap_or(0.0),
            msg_out_rate: stats.map(|s| s.msg_out_rate).unwrap_or(0.0),
            retained: stats.map(|s| s.retained).unwrap_or(0),
            error,
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
        let tls = self.tls_material(st)?;
        let listeners = Self::specs(st, tls);
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

    /// The runtime reachability (one for the whole broker, change set 4).
    fn bind_addr(st: &Stored) -> BindAddr {
        match st.bind.unwrap_or_default() {
            Bind::Network => BindAddr::Network,
            Bind::Localhost => BindAddr::Localhost,
        }
    }

    /// The enabled listeners as runtime specifications.
    fn specs(st: &Stored, tls: Option<Arc<TlsMaterial>>) -> Vec<ListenerSpec> {
        let bind = Self::bind_addr(st);
        st.listeners
            .iter()
            .filter(|l| l.enabled)
            .map(|l| ListenerSpec {
                id: l.id.clone(),
                kind: l.kind,
                bind,
                port: l.port,
                tls: if l.kind.encrypted() {
                    tls.clone()
                } else {
                    None
                },
            })
            .collect()
    }

    /// The server certificate, key and client roots for the TLS and WSS listeners. `Ok(None)` only when no such
    /// listener is enabled: an encrypted listener never starts without its material (change set 3).
    fn tls_material(&self, st: &Stored) -> Result<Option<Arc<TlsMaterial>>, BrokerError> {
        if !Self::needs_tls(st) {
            return Ok(None);
        }
        let chain = parse_certs(st.server_pem.as_bytes())
            .map_err(|e| BrokerError::tls_file(&e, "server certificate"))?;
        let key_pem = self.server_key()?.ok_or_else(|| {
            BrokerError::new(
                "keychain.unavailable",
                &[("detail", s("the server key is missing"))],
            )
            .with_detail("the server key is missing from the secret store")
        })?;
        let key = parse_private_key(key_pem.as_bytes(), None)
            .map_err(|e| BrokerError::tls_file(&e, "server key"))?;
        let roots_pem = match st.verify_against {
            VerifyAgainst::Local => Some(st.ca_pem.clone()),
            VerifyAgainst::Imported => st.imported_ca_pem.clone(),
        };
        let client_roots = roots_pem
            .and_then(|p| parse_certs(p.as_bytes()).ok())
            .unwrap_or_default();
        Ok(Some(Arc::new(TlsMaterial {
            chain,
            key,
            client_roots,
        })))
    }

    /// Starts the broker. When it already runs, listeners that could not start are tried again.
    pub async fn start(&self) -> BrokerStatus {
        let _g = self.apply.lock().await;
        if self.is_running() {
            drop(_g);
            self.retry_failed().await;
            return self.status();
        }
        {
            let mut inner = self.lock();
            inner.state = BrokerState::Starting;
            inner.error = None;
        }
        let cfg = self.heal_tls().and_then(|_| self.runtime_config());
        let result = match cfg {
            Ok(cfg) => Broker::start(cfg).await,
            Err(e) => Err(e),
        };
        {
            let mut inner = self.lock();
            match result {
                Ok(h) => {
                    inner.handle = Some(Arc::new(h));
                    inner.state = BrokerState::Running;
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
        let _g = self.apply.lock().await;
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
            inner.error = None;
            h
        };
        if let Some(h) = handle {
            h.stop().await;
        }
        self.status()
    }

    /// While running: starts again the listeners that could not start (for example after their port was freed).
    async fn retry_failed(&self) {
        let _g = self.apply.lock().await;
        let Some(h) = self.lock().handle.clone() else {
            return;
        };
        let failed: Vec<ListenerSpec> = {
            let rt = h.listeners();
            h.specs()
                .into_iter()
                .filter(|s| rt.iter().any(|r| r.id == s.id && r.error.is_some()))
                .collect()
        };
        if failed.is_empty() {
            return;
        }
        let tls = self.heal_tls().and_then(|_| {
            let inner = self.lock();
            self.tls_material(&inner.stored)
        });
        for mut spec in failed {
            if spec.kind.encrypted() {
                spec.tls = tls.as_ref().ok().cloned().flatten();
            }
            let rt = h.start_listener(spec.clone()).await;
            self.log_listener_started(&spec, &rt, "");
        }
    }

    fn log_listener_started(
        &self,
        spec: &ListenerSpec,
        rt: &crate::runtime::ListenerRuntime,
        suffix: &str,
    ) {
        let port = rt.addrs.first().map(|a| a.port()).unwrap_or(spec.port);
        match &rt.error {
            None => self.log.info(format!(
                "listener {} {port} started{suffix}",
                spec.kind.label()
            )),
            Some(e) => self.log.error(format!(
                "listener {} {port} could not start: {}{suffix}",
                spec.kind.label(),
                e.detail.clone().unwrap_or_default()
            )),
        }
    }

    /// Brings the running listeners in line with the saved ones (change set 7): a new or switched-on listener starts,
    /// a removed or switched-off one stops and drops its sessions, a port or kind change stops and starts that one
    /// listener; the others are not touched. One log line per listener started or stopped.
    async fn apply_listeners(&self) {
        let Some(h) = self.lock().handle.clone() else {
            return;
        };
        let wants_tls = Self::needs_tls(&self.lock().stored);
        let tls = if wants_tls {
            match self.heal_tls().and_then(|_| {
                let inner = self.lock();
                self.tls_material(&inner.stored)
            }) {
                Ok(t) => t,
                Err(e) => {
                    self.log.error(format!(
                        "encrypted listeners cannot start: {}",
                        e.detail.clone().unwrap_or_else(|| e.key().to_string())
                    ));
                    None
                }
            }
        } else {
            None
        };
        let (desired, stored_ids) = {
            let inner = self.lock();
            let ids: Vec<String> = inner
                .stored
                .listeners
                .iter()
                .map(|l| l.id.clone())
                .collect();
            (Self::specs(&inner.stored, tls), ids)
        };
        let running = h.specs();
        let ports: Vec<(String, u16)> = h
            .listeners()
            .iter()
            .map(|r| (r.id.clone(), r.addrs.first().map(|a| a.port()).unwrap_or(0)))
            .collect();
        let port_of = |r: &ListenerSpec| {
            ports
                .iter()
                .find(|(id, p)| *id == r.id && *p != 0)
                .map(|(_, p)| *p)
                .unwrap_or(r.port)
        };
        for r in &running {
            if desired.iter().any(|d| d.id == r.id) {
                continue;
            }
            let n = h.stop_listener(&r.id).await.unwrap_or(0);
            let why = if stored_ids.contains(&r.id) {
                "listener switched off"
            } else {
                "listener removed"
            };
            self.log.info(format!(
                "listener {} {} stopped{}",
                r.kind.label(),
                port_of(r),
                dropped_suffix(n, why)
            ));
        }
        for d in &desired {
            match running.iter().find(|r| r.id == d.id) {
                None => {
                    let rt = h.start_listener(d.clone()).await;
                    self.log_listener_started(d, &rt, "");
                }
                Some(r) if r.kind != d.kind || r.port != d.port => {
                    let n = h.stop_listener(&r.id).await.unwrap_or(0);
                    self.log.info(format!(
                        "listener {} {} stopped",
                        r.kind.label(),
                        port_of(r)
                    ));
                    let why = if r.kind != d.kind {
                        "kind changed"
                    } else {
                        "port changed"
                    };
                    let rt = h.start_listener(d.clone()).await;
                    self.log_listener_started(d, &rt, &dropped_suffix(n, why));
                }
                Some(_) => {}
            }
        }
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
    ///
    /// While the broker runs, the new listener starts at once (change set 7).
    pub async fn add_listener(&self, req: AddListenerRequest) -> Result<BrokerConfig, BrokerError> {
        let _g = self.apply.lock().await;
        let port = self.check_port(req.port, None)?;
        let kind = match (req.preset, req.kind) {
            (Preset::Plain, ListenerKind::Tls) => ListenerKind::Tcp,
            (Preset::Plain, ListenerKind::Wss) => ListenerKind::Ws,
            (Preset::Plain, k) => k,
            (_, ListenerKind::Tcp) => ListenerKind::Tls,
            (_, ListenerKind::Ws) => ListenerKind::Wss,
            (_, k) => k,
        };
        {
            let mut inner = self.lock();
            let running = inner.handle.is_some();
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
                legacy_bind: None,
                port,
                enabled: true,
                preset: req.preset,
            });
            let mut rule_line = None;
            let before = (
                inner.stored.require_client_cert,
                inner.stored.verify_against,
            );
            if req.preset == Preset::Mtls {
                inner.stored.require_client_cert = true;
                inner.stored.verify_against = if inner.stored.imported_ca_pem.is_some() {
                    inner.stored.verify_against
                } else {
                    VerifyAgainst::Local
                };
                if !before.0 {
                    rule_line = Some((
                        LogLevel::Info,
                        "client certificate required on encrypted listeners".to_string(),
                    ));
                }
            }
            if !running {
                self.log
                    .info(format!("{} listener added on port {port}", kind.label()));
            }
            let apply = if before.1 != inner.stored.verify_against {
                Apply::Tls
            } else {
                Apply::Auth
            };
            self.commit(&mut inner, apply, rule_line, "client certificate required")?;
        }
        self.apply_listeners().await;
        Ok(self.config())
    }

    /// Updates kind, port, enabled and preset of a listener. While the broker runs, a port or kind change stops and
    /// starts that listener alone (its sessions drop), switching it off stops it, on starts it (change set 7).
    pub async fn update_listener(&self, l: Listener) -> Result<BrokerConfig, BrokerError> {
        let _g = self.apply.lock().await;
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
        {
            let mut inner = self.lock();
            if let Some(x) = inner.stored.listeners.iter_mut().find(|x| x.id == l.id) {
                x.kind = l.kind;
                x.port = l.port;
                x.enabled = l.enabled;
                x.preset = l.preset;
            }
            self.commit(&mut inner, Apply::Save, None, "")?;
        }
        self.apply_listeners().await;
        Ok(self.config())
    }

    /// Removes a listener; while the broker runs it stops at once and only its sessions drop (change set 7).
    pub async fn remove_listener(&self, listener_id: &str) -> Result<BrokerConfig, BrokerError> {
        let _g = self.apply.lock().await;
        {
            let mut inner = self.lock();
            inner.stored.listeners.retain(|l| l.id != listener_id);
            self.commit(&mut inner, Apply::Save, None, "")?;
        }
        self.apply_listeners().await;
        Ok(self.config())
    }

    /// Reachability of every listener: the whole network or this computer only (change set 4). While the broker
    /// runs, every listening socket is bound again at once and open sessions stay (change set 7).
    pub async fn set_bind(&self, bind: Bind) -> Result<BrokerConfig, BrokerError> {
        let _g = self.apply.lock().await;
        let h = {
            let mut inner = self.lock();
            let changed = inner.stored.bind != Some(bind);
            inner.stored.bind = Some(bind);
            self.commit(&mut inner, Apply::Save, None, "")?;
            if !changed {
                return Ok(self.view_with_secrets(&inner.stored));
            }
            self.log.info(match bind {
                Bind::Network => "listeners reachable from the whole network",
                Bind::Localhost => "listeners reachable from this computer only",
            });
            inner.handle.clone()
        };
        if let Some(h) = h {
            let addr = match bind {
                Bind::Network => BindAddr::Network,
                Bind::Localhost => BindAddr::Localhost,
            };
            let specs = h.specs();
            for rt in h.rebind(addr).await {
                if let (Some(e), Some(spec)) = (&rt.error, specs.iter().find(|s| s.id == rt.id)) {
                    self.log.error(format!(
                        "listener {} {} could not start: {}",
                        spec.kind.label(),
                        spec.port,
                        e.detail.clone().unwrap_or_default()
                    ));
                }
            }
        }
        Ok(self.config())
    }

    /// Turns anonymous access on or off; applies immediately when running. Off drops the sessions that logged in
    /// without a login; on drops nothing.
    pub fn set_anonymous(&self, on: bool) -> Result<BrokerConfig, BrokerError> {
        let mut inner = self.lock();
        inner.stored.anonymous = on;
        let line = if on {
            "anonymous access on: users and client certificates are not checked"
        } else {
            "anonymous access off"
        };
        self.commit(
            &mut inner,
            Apply::Auth,
            Some((LogLevel::Warn, line.into())),
            "anonymous access off",
        )?;
        Ok(self.view_with_secrets(&inner.stored))
    }

    /// Client certificate on encrypted listeners: Required (true) or Optional (false), for the whole broker. Plain
    /// listeners are never affected. Applies at once: Optional to Required drops the sessions without a client
    /// certificate on encrypted listeners, Required to Optional drops nothing.
    pub fn set_require_client_cert(&self, on: bool) -> Result<BrokerConfig, BrokerError> {
        let mut inner = self.lock();
        let changed = inner.stored.require_client_cert != on;
        inner.stored.require_client_cert = on;
        let line = changed.then(|| {
            (
                LogLevel::Info,
                if on {
                    "client certificate required on encrypted listeners"
                } else {
                    "client certificate optional on encrypted listeners"
                }
                .to_string(),
            )
        });
        self.commit(&mut inner, Apply::Auth, line, "client certificate required")?;
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
        let line = changed.then(|| {
            (
                LogLevel::Info,
                match source {
                    VerifyAgainst::Local => "client certificates verified against the local CA",
                    VerifyAgainst::Imported => {
                        "client certificates verified against the imported CA"
                    }
                }
                .to_string(),
            )
        });
        self.commit(&mut inner, Apply::Tls, line, "verifying CA changed")?;
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
        self.commit(&mut inner, Apply::Auth, None, "").map(|_| ())
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
            self.commit(
                &mut inner,
                Apply::Auth,
                Some((
                    LogLevel::Info,
                    format!("password of user {username} changed"),
                )),
                "password changed",
            )?;
        }
        Ok(self.config())
    }

    /// Removes a shared user and its password.
    pub fn remove_user(&self, username: &str) -> Result<BrokerConfig, BrokerError> {
        {
            let mut inner = self.lock();
            inner.stored.users.retain(|u| u.username != username);
            let _ = self.secrets.delete(&secret_user_password(username));
            self.commit(
                &mut inner,
                Apply::Auth,
                Some((LogLevel::Info, format!("user {username} removed"))),
                "user removed",
            )?;
        }
        Ok(self.config())
    }

    /// The plain password of a shared user, for the eye and copy icons (None when it was never kept).
    pub fn user_password(&self, username: &str) -> Result<Option<String>, BrokerError> {
        if !self
            .lock()
            .stored
            .users
            .iter()
            .any(|u| u.username == username)
        {
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
            self.log.info(format!(
                "user {APP_USER_NAME} created for the profile \"Local broker\""
            ));
            return Ok((true, pw));
        }
        {
            let mut inner = self.lock();
            if let Some(u) = inner
                .stored
                .users
                .iter_mut()
                .find(|u| u.username == APP_USER_NAME)
            {
                if !u.system {
                    u.system = true;
                    self.commit(&mut inner, Apply::Save, None, "")?;
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
        self.commit(&mut inner, Apply::Save, None, "")?;
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
        self.commit(&mut inner, Apply::Save, None, "")?;
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

    /// Revokes an issued certificate: refused from the next TLS handshake on and, while running, its open session is
    /// dropped at once.
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
            self.commit(
                &mut inner,
                Apply::Auth,
                Some((LogLevel::Warn, format!("client certificate {name} revoked"))),
                "certificate revoked",
            )?;
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
            auto_sans: kit::local_sans(),
            extra_sans: st.extra_sans.clone(),
        })
    }

    /// The served server certificate chain as PEM ("Export…" on the Server certificate line, change set 11), named
    /// "server-chain.pem".
    pub fn export_server_chain(&self) -> SaveFile {
        (
            "server-chain.pem".to_string(),
            self.lock().stored.server_pem.clone().into_bytes(),
        )
    }

    /// Regenerates the server certificate with the automatic names and the given extra names (change set 11). The extra
    /// names are checked (host names or IP literals; the first invalid one is refused with `certs.badName`) and kept
    /// in the configuration for every later generation.
    pub fn regenerate_with_extra(
        &self,
        extra: &[String],
        key_type: KeyAlg,
    ) -> Result<BrokerCerts, BrokerError> {
        let extra = kit::parse_extra_names(&extra.join(" "))
            .map_err(|bad| BrokerError::new("certs.badName", &[("name", s(bad))]))?;
        let before = std::mem::replace(&mut self.lock().stored.extra_sans, extra.clone());
        let r = self.regenerate(&kit::server_sans(&extra), key_type);
        if r.is_err() {
            self.lock().stored.extra_sans = before;
        }
        r
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

    /// Regenerates the server certificate from the local CA with exactly these names (empty `sans`: the automatic
    /// names and the stored extra names). The UI uses [`BrokerManager::regenerate_with_extra`].
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
                kit::server_sans(&inner.stored.extra_sans)
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
            self.commit(
                &mut inner,
                Apply::Tls,
                Some((LogLevel::Info, "server certificate regenerated".into())),
                "server certificate changed",
            )?;
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
            self.commit(
                &mut inner,
                Apply::Tls,
                Some((LogLevel::Info, "server certificate imported".into())),
                "server certificate changed",
            )?;
        }
        self.broker_certs()
    }

    /// Imports a CA used to verify client certificates when "Verify against" is "imported".
    pub fn import_client_ca(&self, bytes: &[u8]) -> Result<BrokerCerts, BrokerError> {
        let certs = parse_certs(bytes).map_err(|e| BrokerError::tls_file(&e, "CA"))?;
        {
            let mut inner = self.lock();
            inner.stored.imported_ca_pem = Some(certs_pem(&certs));
            let apply = if inner.stored.verify_against == VerifyAgainst::Imported {
                Apply::Tls
            } else {
                Apply::Save
            };
            self.commit(
                &mut inner,
                apply,
                Some((LogLevel::Info, "client CA imported".into())),
                "verifying CA changed",
            )?;
        }
        self.broker_certs()
    }

    /// Replaces the local CA and the server certificate. Client certificates issued before no longer verify.
    pub fn rotate_ca(&self) -> Result<BrokerCerts, BrokerError> {
        {
            let mut inner = self.lock();
            // Change set 11: the automatic names read now plus the extra names, as for every generation.
            let sans = kit::server_sans(&inner.stored.extra_sans);
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
            self.commit(
                &mut inner,
                Apply::Tls,
                Some((
                    LogLevel::Warn,
                    "CA rotated: issued client certificates must be re-issued".into(),
                )),
                "CA rotated",
            )?;
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
        self.commit(&mut inner, Apply::Save, None, "")?;
        Ok((true, issued.cert_pem, issued.key_pem))
    }

    /// Connection settings for the profile "Local broker": the given listener or the most secure enabled one.
    /// With Anonymous off it carries the user "mqtt-panel" (created when missing) and, when the listener requires
    /// one, the client certificate "mqttpanel-desktop" (issued when missing, see [`BrokerManager::ensure_desktop_cert`]).
    /// TLS and WSS listeners carry what the broker serves (see [`served_trust`]): its own CA, or after an import the
    /// imported CA, or a pin of the served certificate when the import had no CA (CONN-001).
    pub fn local_connection(
        &self,
        listener_id: Option<&str>,
    ) -> Result<LocalConnection, BrokerError> {
        let l = match listener_id {
            Some(id) => self.config().listeners.into_iter().find(|l| l.id == id),
            None => self.preferred_listener(),
        }
        .ok_or_else(|| BrokerError::new("broker.noListener", &[]))?;
        let (port, anonymous, (ca_pem, server_fingerprint)) = {
            let inner = self.lock();
            (
                inner
                    .handle
                    .as_ref()
                    .and_then(|h| h.port(&l.id))
                    .unwrap_or(l.port),
                inner.stored.anonymous,
                served_trust(&inner.stored),
            )
        };
        let encrypted = l.kind.encrypted();
        let mut lc = LocalConnection {
            listener_id: l.id.clone(),
            host: "localhost".into(),
            port,
            scheme: l.kind.scheme().into(),
            ws_path: l
                .kind
                .websocket()
                .then(|| crate::runtime::ws::WS_PATH.to_string()),
            ca_pem: ca_pem.filter(|_| encrypted),
            server_fingerprint: server_fingerprint.filter(|_| encrypted),
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

    /// True when the broker serves an imported server certificate (not one issued by its own CA).
    pub fn server_imported(&self) -> bool {
        self.lock().stored.server_imported
    }

    /// True when Anonymous is on.
    pub fn anonymous(&self) -> bool {
        self.lock().stored.anonymous
    }
}

impl RuntimeConfig {
    /// The runtime configuration of the desktop app's broker (CLI `mqttpanel broker` without `--config`).
    /// A missing server or CA key is made again first, as on a start of the app's broker.
    pub fn from_manager(manager: &BrokerManager) -> Result<RuntimeConfig, BrokerError> {
        manager.heal_tls()?;
        manager.runtime_config()
    }
}
