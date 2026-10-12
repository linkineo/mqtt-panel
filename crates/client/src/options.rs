//! Connection options.

use mqttpanel_tls::Identity;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Mqtt,
    Mqtts,
    Ws,
    Wss,
}

impl Scheme {
    pub fn is_tls(self) -> bool {
        matches!(self, Scheme::Mqtts | Scheme::Wss)
    }
    pub fn is_ws(self) -> bool {
        matches!(self, Scheme::Ws | Scheme::Wss)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Mqtt => "mqtt",
            Scheme::Mqtts => "mqtts",
            Scheme::Ws => "ws",
            Scheme::Wss => "wss",
        }
    }
    pub fn parse(s: &str) -> Option<Scheme> {
        match s {
            "mqtt" | "tcp" => Some(Scheme::Mqtt),
            "mqtts" | "ssl" | "tls" => Some(Scheme::Mqtts),
            "ws" => Some(Scheme::Ws),
            "wss" => Some(Scheme::Wss),
            _ => None,
        }
    }
}

/// Requested protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionPref {
    #[serde(rename = "3.1.1")]
    V311,
    #[serde(rename = "5")]
    V5,
    #[serde(rename = "auto")]
    Auto,
}

/// Negotiated protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Version {
    #[serde(rename = "3.1.1")]
    V311,
    #[serde(rename = "5")]
    V5,
}

impl Version {
    pub fn label(self) -> &'static str {
        match self {
            Version::V311 => "3.1.1",
            Version::V5 => "5",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TlsOptions {
    /// Verify the server certificate against the system roots plus `ca_pem`.
    pub verify_server: bool,
    /// Extra trust anchors (PEM or DER, one or several certificates).
    pub ca: Option<Vec<u8>>,
    pub identity: Option<Identity>,
    /// SHA-256 fingerprints trusted for this profile ("Trust this certificate").
    pub trusted_fingerprints: Vec<String>,
    /// Server name (SNI) override; also used for the name check.
    pub sni: Option<String>,
    /// Protocol tag (ALPN).
    pub alpn: Option<String>,
    /// Lab only: accept any certificate.
    pub skip_verify: bool,
}

#[derive(Debug, Clone)]
pub struct Will {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub scheme: Scheme,
    /// WebSocket path, default "/mqtt".
    pub ws_path: String,
    pub version: VersionPref,
    pub client_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: TlsOptions,
    pub keep_alive: u16,
    pub clean_start: bool,
    pub session_expiry: u32,
    pub last_will: Option<Will>,
    pub subscriptions: Vec<(String, u8)>,
    pub connect_timeout: Duration,
    /// Reconnect after an unexpected loss (managed connections).
    pub reconnect: bool,
    pub reconnect_min: Duration,
    pub reconnect_max: Duration,
}

impl ConnectOptions {
    /// Defaults for a host and port: plain MQTT, auto version, random client id, keep-alive 60 s.
    pub fn new(host: &str, port: u16) -> Self {
        ConnectOptions {
            host: host.to_string(),
            port,
            scheme: Scheme::Mqtt,
            ws_path: "/mqtt".into(),
            version: VersionPref::Auto,
            client_id: random_client_id(),
            username: None,
            password: None,
            tls: TlsOptions { verify_server: true, ..Default::default() },
            keep_alive: 60,
            clean_start: true,
            session_expiry: 0,
            last_will: None,
            subscriptions: vec![],
            connect_timeout: Duration::from_secs(10),
            reconnect: true,
            reconnect_min: Duration::from_secs(1),
            reconnect_max: Duration::from_secs(30),
        }
    }
    /// "host:port" for messages (IPv6 in brackets).
    pub fn endpoint(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// "mqttpanel-" plus four random hex digits.
pub fn random_client_id() -> String {
    let n: u16 = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0) as u16)
        ^ (std::process::id() as u16).rotate_left(5);
    format!("mqttpanel-{n:04x}")
}
