//! TOML configuration file for `mqttpanel broker --config <file>`.
//!
//! ```toml
//! # All keys are optional except listener kind and port.
//! max_clients = 1000             # default 1000
//! max_packet_size = 16777216     # bytes, default 16 MiB
//! bind = "network"               # every listener: network (all interfaces, default) | localhost | an IP address
//!
//! [auth]
//! anonymous = false              # true: anyone may connect; users and client certificates are ignored
//! [[auth.users]]                 # shared users, accepted on every listener (there is no per-listener login)
//! username = "alice"
//! password = "plain text"        # either a plain password ...
//! [[auth.users]]
//! username = "bob"
//! password_hash = "$argon2id$v=19$m=19456,t=2,p=1$..."   # ... or an argon2 PHC hash
//!
//! [tls]                          # default certificate for every tls/wss listener
//! cert = "server.pem"            # PEM chain (leaf first), or a PKCS#12 bundle (then omit key)
//! key = "server.key"             # PEM key (PKCS#8, SEC1 or PKCS#1; encrypted PKCS#8 with key_passphrase)
//! key_passphrase = ""            # optional
//! client_ca = "ca.pem"           # CA that verifies client certificates (mutual TLS)
//! require_client_cert = true     # client certificate required on every tls/wss listener (default false)
//! revoked = ["4a:0b:..."]        # revoked client-certificate serials
//!
//! [[listener]]
//! id = "tcp"                     # optional, defaults to the kind (tcp, tls-2, ...)
//! kind = "tcp"                   # tcp | tls | ws | wss  (ws and wss serve the path /mqtt)
//! port = 1883                    # reachability is broker-wide (top-level `bind`); a per-listener bind is refused
//!
//! [[listener]]
//! kind = "tls"
//! port = 8883
//! # cert, key, key_passphrase and client_ca may also be set per listener.
//! ```
//!
//! Relative paths are resolved against the directory of the configuration file.

use crate::config::ListenerKind;
use crate::error::{s, BrokerError};
use crate::runtime::{
    AuthConfig, BindAddr, Credential, ListenerSpec, RevocationList, RuntimeConfig, TlsMaterial,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    max_clients: Option<usize>,
    max_packet_size: Option<usize>,
    /// Reachability of every listener (change set 4: one choice for the whole broker).
    bind: Option<String>,
    #[serde(default)]
    auth: FileAuth,
    tls: Option<FileTls>,
    #[serde(default)]
    listener: Vec<FileListener>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileAuth {
    #[serde(default)]
    anonymous: bool,
    #[serde(default)]
    users: Vec<FileUser>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileUser {
    username: String,
    password: Option<String>,
    password_hash: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
struct FileTls {
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
    key_passphrase: Option<String>,
    client_ca: Option<PathBuf>,
    #[serde(default)]
    require_client_cert: bool,
    #[serde(default)]
    revoked: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileListener {
    id: Option<String>,
    kind: ListenerKind,
    port: u16,
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
    key_passphrase: Option<String>,
    client_ca: Option<PathBuf>,
}

fn unreadable(file: &Path) -> BrokerError {
    BrokerError::new(
        "file.unreadable",
        &[(
            "file",
            s(file
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()),
        )],
    )
}

fn read(base: &Path, p: &Path) -> Result<Vec<u8>, BrokerError> {
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    std::fs::read(&full).map_err(|_| unreadable(&full))
}

fn material(
    base: &Path,
    cert: &Path,
    key: Option<&Path>,
    pass: Option<&str>,
    ca: Option<&Path>,
) -> Result<TlsMaterial, BrokerError> {
    let cert_bytes = read(base, cert)?;
    let key_bytes = key.map(|k| read(base, k)).transpose()?;
    let name = cert
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let id = mqttpanel_tls::load_identity(&cert_bytes, key_bytes.as_deref(), pass)
        .map_err(|e| BrokerError::tls_file(&e, &name))?;
    let client_roots = match ca {
        Some(ca) => mqttpanel_tls::parse_certs(&read(base, ca)?)
            .map_err(|e| BrokerError::tls_file(&e, &ca.to_string_lossy()))?,
        None => Vec::new(),
    };
    Ok(TlsMaterial {
        chain: id.certs,
        key: id.key,
        client_roots,
    })
}

impl RuntimeConfig {
    /// Reads a TOML configuration file (format in the module documentation).
    pub fn from_toml_file(path: &Path) -> Result<RuntimeConfig, BrokerError> {
        let text = std::fs::read_to_string(path).map_err(|_| unreadable(path))?;
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Self::from_toml_str(&text, &base).map_err(|e| {
            if e.key() == "error.internal" {
                unreadable(path).with_detail(e.detail.unwrap_or_default())
            } else {
                e
            }
        })
    }

    /// Parses a TOML configuration; relative paths resolve against `base`.
    pub fn from_toml_str(text: &str, base: &Path) -> Result<RuntimeConfig, BrokerError> {
        let fc: FileConfig = toml::from_str(text).map_err(|e| {
            BrokerError::internal("invalid configuration file").with_detail(e.to_string())
        })?;
        let mut auth = AuthConfig {
            anonymous: fc.auth.anonymous,
            ..Default::default()
        };
        for u in fc.auth.users {
            let cred = match (u.password, u.password_hash) {
                (_, Some(h)) => Credential::Argon2(h),
                (Some(p), None) => Credential::Plain(p),
                (None, None) => continue,
            };
            auth.users.insert(u.username, cred);
        }
        let default_tls = fc.tls.clone().unwrap_or_default();
        let shared_material = match &default_tls.cert {
            Some(c) => Some(Arc::new(material(
                base,
                c,
                default_tls.key.as_deref(),
                default_tls.key_passphrase.as_deref(),
                default_tls.client_ca.as_deref(),
            )?)),
            None => None,
        };
        let bind = match fc.bind.as_deref() {
            None | Some("network") => BindAddr::Network,
            Some("localhost") => BindAddr::Localhost,
            Some(ip) => BindAddr::Addr(
                ip.parse()
                    .map_err(|_| BrokerError::internal(format!("invalid bind address {ip}")))?,
            ),
        };
        let mut listeners = Vec::new();
        for (i, l) in fc.listener.into_iter().enumerate() {
            let id = l.id.unwrap_or_else(|| {
                if i == 0 {
                    l.kind.id_prefix().to_string()
                } else {
                    format!("{}-{}", l.kind.id_prefix(), i + 1)
                }
            });
            let tls = if l.kind.encrypted() {
                match &l.cert {
                    Some(c) => Some(Arc::new(material(
                        base,
                        c,
                        l.key.as_deref().or(default_tls.key.as_deref()),
                        l.key_passphrase
                            .as_deref()
                            .or(default_tls.key_passphrase.as_deref()),
                        l.client_ca.as_deref().or(default_tls.client_ca.as_deref()),
                    )?)),
                    None => match (&shared_material, &l.client_ca) {
                        (Some(m), Some(ca)) => {
                            let mut m2 = (**m).clone();
                            m2.client_roots = mqttpanel_tls::parse_certs(&read(base, ca)?)
                                .map_err(|e| BrokerError::tls_file(&e, &ca.to_string_lossy()))?;
                            Some(Arc::new(m2))
                        }
                        (m, _) => m.clone(),
                    },
                }
            } else {
                None
            };
            listeners.push(ListenerSpec {
                id,
                kind: l.kind,
                bind,
                port: l.port,
                tls,
            });
        }
        Ok(RuntimeConfig {
            listeners,
            auth,
            require_client_cert: default_tls.require_client_cert,
            max_clients: fc.max_clients.unwrap_or(1000),
            max_packet_size: fc
                .max_packet_size
                .unwrap_or(crate::runtime::DEFAULT_MAX_PACKET),
            revoked: RevocationList::new(default_tls.revoked.iter()),
            log: Default::default(),
        })
    }
}
