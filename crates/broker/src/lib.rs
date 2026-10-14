//! Embedded MQTT broker of MQTT Panel (desktop app and `mqttpanel broker`).
//!
//! - [`runtime`]: the in-process broker. [`Broker::start`] binds TCP, TLS, WS and WSS listeners (WS path `/mqtt`)
//!   and serves MQTT 3.1.1 and 5 (QoS 0-2, retained messages, last will, persistent sessions, shared
//!   subscriptions `$share/group/filter`, session takeover). Built on tokio, tokio-rustls (ring) and
//!   tokio-tungstenite, with the packet codec of rumqttc.
//! - [`manager`]: [`BrokerManager`] keeps `broker.json` and the secrets, issues certificates, applies presets
//!   and owns the running broker. Every change applies while it runs (change set 7): one task per listener, TLS
//!   material swapped for new handshakes, and a session registry that drops only the sessions a change invalidates.
//! - [`toml_config`]: the TOML file format of `mqttpanel broker --config`.
//!
//! Authentication order for each CONNECT: anonymous on accepts everyone (client certificate not requested, users
//! ignored); otherwise a listener that requires a client certificate accepts a verified one (a login is then
//! optional and never replaces the certificate); otherwise the username and password must match a shared user
//! (one set of users for every listener, no per-listener login); else CONNACK 0x05 (3.1.1) or 0x86 (5) and the
//! log line "client <id> rejected: bad username or password". With anonymous on, a valid login is still recorded as
//! such, so turning anonymous off later keeps that session.

pub mod config;
pub mod error;
pub mod manager;
pub mod runtime;
pub mod toml_config;

pub use config::*;
pub use error::{BrokerError, UserMessage};
pub use manager::BrokerManager;
pub use runtime::{Broker, BrokerHandle, ListenerSpec, RuntimeConfig};
