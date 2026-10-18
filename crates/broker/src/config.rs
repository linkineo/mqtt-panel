//! Types shared with the UI (`packages/engine-client/src/types.ts`, broker section), serde camelCase.

use crate::error::UserMessage;
use mqttpanel_tls::kit::KeyAlg;
pub use mqttpanel_tls::CertDetails;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Listener transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListenerKind {
    Tcp,
    Tls,
    Ws,
    Wss,
}

impl ListenerKind {
    /// True for TLS and WSS.
    pub fn encrypted(self) -> bool {
        matches!(self, ListenerKind::Tls | ListenerKind::Wss)
    }
    /// True for WS and WSS.
    pub fn websocket(self) -> bool {
        matches!(self, ListenerKind::Ws | ListenerKind::Wss)
    }
    /// URL scheme of a client profile for this listener.
    pub fn scheme(self) -> &'static str {
        match self {
            ListenerKind::Tcp => "mqtt",
            ListenerKind::Tls => "mqtts",
            ListenerKind::Ws => "ws",
            ListenerKind::Wss => "wss",
        }
    }
    /// Label used in log lines and the clients table ("TCP", "TLS", "WS", "WSS").
    pub fn label(self) -> &'static str {
        match self {
            ListenerKind::Tcp => "TCP",
            ListenerKind::Tls => "TLS",
            ListenerKind::Ws => "WS",
            ListenerKind::Wss => "WSS",
        }
    }
    pub fn id_prefix(self) -> &'static str {
        match self {
            ListenerKind::Tcp => "tcp",
            ListenerKind::Tls => "tls",
            ListenerKind::Ws => "ws",
            ListenerKind::Wss => "wss",
        }
    }
}

/// Reachability of the broker: one choice for every listener (change set 4, 13 Oct 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Bind {
    /// All interfaces (0.0.0.0 and ::).
    #[default]
    Network,
    /// Loopback only (127.0.0.1 and ::1).
    Localhost,
}

/// Setup chosen in the Add listener flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Plain,
    Encrypted,
    Passwords,
    Mtls,
}

/// A listener (TS `Listener`). There is no per-listener login (users are shared by every listener) and no
/// per-listener reachability (`BrokerConfig::bind`, change set 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub id: String,
    pub kind: ListenerKind,
    pub port: u16,
    pub enabled: bool,
    pub preset: Preset,
}

/// Which CA verifies client certificates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VerifyAgainst {
    #[default]
    Local,
    Imported,
}

/// A shared user as shown to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    pub username: String,
    /// Created by the app for the profile "Local broker" (user "mqtt-panel").
    pub system: bool,
    /// The plain password is in the keychain and can be revealed and copied.
    pub has_password: bool,
}

/// The broker configuration (TS `BrokerConfig`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerConfig {
    /// Reachability of every listener: the whole network (default) or this computer only (change set 4).
    pub bind: Bind,
    pub listeners: Vec<Listener>,
    pub anonymous: bool,
    pub users: Vec<UserView>,
    /// Client certificate on encrypted listeners (broker-wide, owner 13 Oct 2026): required on every TLS and WSS
    /// listener when true, optional otherwise. Plain listeners are never affected.
    pub require_client_cert: bool,
    pub verify_against: VerifyAgainst,
    pub max_clients: u32,
}

/// Runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrokerState {
    Stopped,
    Starting,
    Running,
    Failed,
}

/// Status of one listener.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStatus {
    pub id: String,
    pub up: bool,
    pub error: Option<UserMessage>,
}

/// The broker status (TS `BrokerStatus`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerStatus {
    pub state: BrokerState,
    pub started_at: Option<i64>,
    pub clients: u32,
    pub max_clients: u32,
    pub msg_in_rate: f64,
    pub msg_out_rate: f64,
    pub retained: u32,
    pub error: Option<UserMessage>,
    pub listeners: Vec<ListenerStatus>,
}

/// Last time a client certificate was seen: `"connected"`, `"never"` or a time in ms since the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastSeen {
    Connected,
    Never,
    At(i64),
}

impl Serialize for LastSeen {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            LastSeen::Connected => s.serialize_str("connected"),
            LastSeen::Never => s.serialize_str("never"),
            LastSeen::At(t) => s.serialize_i64(*t),
        }
    }
}

impl<'de> Deserialize<'de> for LastSeen {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        Ok(match v {
            serde_json::Value::String(s) if s == "connected" => LastSeen::Connected,
            serde_json::Value::Number(n) => LastSeen::At(n.as_i64().unwrap_or(0)),
            _ => LastSeen::Never,
        })
    }
}

/// An issued client certificate (TS `IssuedCert`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedCert {
    pub name: String,
    pub serial: String,
    pub issued_at: String,
    pub expires_at: String,
    pub key_type: String,
    pub last_seen: LastSeen,
    pub revoked: bool,
    pub system: bool,
    /// Additive field (not yet in types.ts): issued by a CA that was rotated away; must be re-issued.
    pub superseded: bool,
}

/// Broker certificates (TS `BrokerCerts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerCerts {
    pub ca: CertDetails,
    pub server: CertDetails,
    pub imported: bool,
    pub imported_ca: Option<CertDetails>,
}

/// A connected client (TS `BrokerClient`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerClient {
    pub client_id: String,
    pub address: String,
    /// "3.1.1" or "5".
    pub protocol: String,
    /// "TCP", "TLS", "WS" or "WSS".
    pub transport: String,
    pub connected_at: i64,
    pub subscriptions: u32,
    pub msg_in: u64,
    pub msg_out: u64,
    pub cert_cn: Option<String>,
    pub username: Option<String>,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// A broker log line (TS `LogLine`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub seq: u64,
    pub at: i64,
    pub level: LogLevel,
    pub text: String,
}

/// TS `AddListenerRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddListenerRequest {
    pub preset: Preset,
    pub kind: ListenerKind,
    pub port: u32,
}

/// Export format of a client certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CertFormat {
    Pem,
    Pkcs12,
}

/// TS `IssueCertRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCertRequest {
    pub name: String,
    pub valid_years: u8,
    pub key_type: KeyAlg,
    pub format: CertFormat,
    pub passphrase: String,
    pub save_ca: bool,
}

/// A file produced for the user to save: (file name, bytes).
pub type SaveFile = (String, Vec<u8>);

/// What the "Connect to this app's broker" flow needs to build the "Local broker" profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalConnection {
    pub listener_id: String,
    pub host: String,
    pub port: u16,
    /// mqtt, mqtts, ws or wss.
    pub scheme: String,
    /// "/mqtt" for WS and WSS listeners.
    pub ws_path: Option<String>,
    /// CA certificate (PEM) for TLS and WSS listeners: the broker's own CA, or, after a server certificate from
    /// another issuer was imported, the CA and intermediates that import carried (None when it had the leaf only).
    pub ca_pem: Option<String>,
    /// SHA-256 fingerprint ("3F:9A:…") of the served certificate, set only for an imported leaf-only identity on a
    /// TLS or WSS listener: no issuer is known, so the profile pins the certificate instead.
    #[serde(default)]
    pub server_fingerprint: Option<String>,
    /// Client certificate (PEM) for mutual TLS ("mqttpanel-desktop").
    pub client_cert_pem: Option<String>,
    /// Client key (PKCS#8 PEM, unencrypted) for mutual TLS. Secret: goes to the keychain, never to a file.
    pub client_key_pem: Option<String>,
    /// The app's own user "mqtt-panel" (None with Anonymous on).
    pub username: Option<String>,
    pub password: Option<String>,
}
