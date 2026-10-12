//! The in-process MQTT broker: listeners, sessions, routing, retained store, statistics and log.

pub mod auth;
pub(crate) mod codec;
pub mod listener;
pub mod log;
pub(crate) mod router;
pub(crate) mod session;
pub(crate) mod state;
pub(crate) mod stats;
pub mod tls;
pub mod ws;

use crate::config::{BrokerClient, ListenerKind, LogLevel, LogLine};
use crate::error::{n, s, BrokerError};
pub use auth::{hash_password, verify_password, AuthConfig, Credential};
pub use listener::BindAddr;
pub use log::LogBuffer;
use session::ListenerCtx;
pub use state::CertSeen;
use state::{Outbound, Shared, State};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
pub use tls::{normalize_serial, PeerCert, RevocationList, TlsMaterial};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// Default maximum packet size accepted from clients (and announced in CONNACK on MQTT 5): 16 MiB.
pub const DEFAULT_MAX_PACKET: usize = 16 * 1024 * 1024;

/// One listener of the runtime.
#[derive(Debug, Clone)]
pub struct ListenerSpec {
    pub id: String,
    pub kind: ListenerKind,
    pub bind: BindAddr,
    /// 0 picks a free port (see [`BrokerHandle::port`]).
    pub port: u16,
    /// Mutual TLS on this listener (TLS and WSS only).
    pub require_client_cert: bool,
    /// Server certificate, key and client CA for TLS and WSS.
    pub tls: Option<Arc<TlsMaterial>>,
}

/// Everything the runtime needs. Build it by hand, from the manager ([`RuntimeConfig::from_manager`]) or from a
/// TOML file ([`RuntimeConfig::from_toml_file`]).
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub listeners: Vec<ListenerSpec>,
    pub auth: AuthConfig,
    pub max_clients: usize,
    pub max_packet_size: usize,
    /// Revoked client-certificate serials; shared, so revocations apply while running.
    pub revoked: RevocationList,
    /// Log buffer; pass the manager's to keep the log across restarts.
    pub log: LogBuffer,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            listeners: Vec::new(),
            auth: AuthConfig::default(),
            max_clients: 1000,
            max_packet_size: DEFAULT_MAX_PACKET,
            revoked: RevocationList::default(),
            log: LogBuffer::new(),
        }
    }
}

impl RuntimeConfig {
    /// Applies a bind override (CLI `--bind`) to every listener.
    pub fn with_bind(mut self, addr: std::net::IpAddr) -> Self {
        for l in &mut self.listeners {
            l.bind = BindAddr::Addr(addr);
        }
        self
    }
}

/// A bound (or failed) listener of a running broker.
#[derive(Debug, Clone)]
pub struct ListenerRuntime {
    pub id: String,
    pub kind: ListenerKind,
    pub addrs: Vec<SocketAddr>,
    pub error: Option<BrokerError>,
}

/// Broker statistics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuntimeStats {
    pub clients: u32,
    pub msg_in_rate: f64,
    pub msg_out_rate: f64,
    pub msg_in_total: u64,
    pub msg_out_total: u64,
    pub dropped: u64,
    pub retained: u32,
    /// Start time, ms since the epoch.
    pub started_at: i64,
    pub uptime_s: u64,
}

/// Entry point of the runtime.
pub struct Broker;

fn io_detail(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::AddrInUse => "port already in use".into(),
        std::io::ErrorKind::PermissionDenied => "permission denied".into(),
        std::io::ErrorKind::AddrNotAvailable => "address not available on this computer".into(),
        _ => crate::error::short_detail(&e.to_string()),
    }
}

impl Broker {
    /// Starts the broker: binds every listener and spawns the tasks on the current tokio runtime.
    /// Listeners that cannot start are reported in [`BrokerHandle::listeners`]; the start fails only when none is up.
    pub async fn start(cfg: RuntimeConfig) -> Result<BrokerHandle, BrokerError> {
        if cfg.listeners.is_empty() {
            return Err(BrokerError::new("broker.noListener", &[]));
        }
        let shared = Arc::new(Shared {
            log: cfg.log.clone(),
            stats: stats::Stats::new(),
            auth: RwLock::new(Arc::new(cfg.auth.clone())),
            max_clients: cfg.max_clients.max(1),
            max_packet: cfg.max_packet_size.clamp(1024, 256 * 1024 * 1024),
            started_at_ms: log::now_ms(),
            state: Mutex::new(State::default()),
            next_conn_id: AtomicU64::new(1),
        });
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (done_tx, done_rx) = mpsc::channel::<()>(1);
        let mut tasks = Vec::new();
        let mut listeners = Vec::new();
        for spec in &cfg.listeners {
            let fail = |detail: String| {
                BrokerError::new(
                    "broker.listenerFailed",
                    &[
                        ("kind", s(spec.kind.label())),
                        ("port", n(spec.port)),
                        ("detail", s(detail.clone())),
                    ],
                )
                .with_detail(detail)
            };
            let tls = if spec.kind.encrypted() {
                let built = match &spec.tls {
                    None => Err("no server certificate".to_string()),
                    Some(m) => tls::TlsListener::new(m, cfg.revoked.clone()),
                };
                match built {
                    Ok(t) => Some(Arc::new(t)),
                    Err(e) => {
                        shared.log.error(format!(
                            "{} listener on port {} could not start: {e}",
                            spec.kind.label(),
                            spec.port
                        ));
                        listeners.push(ListenerRuntime {
                            id: spec.id.clone(),
                            kind: spec.kind,
                            addrs: vec![],
                            error: Some(fail(e)),
                        });
                        continue;
                    }
                }
            } else {
                None
            };
            let socks = match listener::bind(spec.bind, spec.port) {
                Ok(s) => s,
                Err(e) => {
                    let d = io_detail(&e);
                    shared.log.error(format!(
                        "{} listener on port {} could not start: {d}",
                        spec.kind.label(),
                        spec.port
                    ));
                    listeners.push(ListenerRuntime {
                        id: spec.id.clone(),
                        kind: spec.kind,
                        addrs: vec![],
                        error: Some(fail(d)),
                    });
                    continue;
                }
            };
            let lctx = Arc::new(ListenerCtx {
                id: spec.id.clone(),
                kind: spec.kind,
                require_client_cert: spec.require_client_cert && spec.kind.encrypted(),
            });
            let mut addrs = Vec::new();
            for sock in socks {
                let addr = match sock.local_addr() {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let l = match tokio::net::TcpListener::from_std(sock) {
                    Ok(l) => l,
                    Err(e) => {
                        shared
                            .log
                            .error(format!("listener {addr} could not start: {e}"));
                        continue;
                    }
                };
                addrs.push(addr);
                tasks.push(tokio::spawn(listener::accept_loop(
                    shared.clone(),
                    lctx.clone(),
                    tls.clone(),
                    l,
                    shutdown_rx.clone(),
                    done_tx.clone(),
                )));
            }
            let port = addrs.first().map(|a| a.port()).unwrap_or(spec.port);
            let mut where_ = match spec.bind {
                BindAddr::Network => "all interfaces".to_string(),
                BindAddr::Localhost => "localhost".to_string(),
                BindAddr::Addr(ip) => ip.to_string(),
            };
            if spec.kind.websocket() {
                where_.push_str(", path /mqtt");
            }
            let mut extra = String::new();
            if lctx.require_client_cert {
                extra.push_str(", client certificate required");
            }
            shared.log.info(format!(
                "{} listener on port {port} ({where_}{extra})",
                spec.kind.label()
            ));
            listeners.push(ListenerRuntime {
                id: spec.id.clone(),
                kind: spec.kind,
                addrs,
                error: None,
            });
        }
        if listeners.iter().all(|l| l.error.is_some()) {
            let _ = shutdown_tx.send(true);
            let detail = listeners
                .iter()
                .find_map(|l| {
                    l.error
                        .as_ref()
                        .and_then(|e| e.message.params.get("detail"))
                        .and_then(|d| d.as_str())
                        .map(String::from)
                })
                .unwrap_or_default();
            return Err(
                BrokerError::new("broker.startFailed", &[("detail", s(detail.clone()))])
                    .with_detail(detail),
            );
        }
        // Session expiry sweeper.
        {
            let shared = shared.clone();
            let mut sd = shutdown_rx.clone();
            tasks.push(tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(1));
                loop {
                    tokio::select! {
                        _ = sd.changed() => break,
                        _ = tick.tick() => shared.expire_sessions(),
                    }
                }
            }));
        }
        shared.log.info("broker started");
        Ok(BrokerHandle {
            shared,
            shutdown_tx,
            done_rx: Some(done_rx),
            done_tx: Some(done_tx),
            tasks,
            listeners,
            revoked: cfg.revoked,
        })
    }
}

/// A running broker. Dropping it stops the broker without waiting; [`BrokerHandle::stop`] waits for the tasks.
pub struct BrokerHandle {
    shared: Arc<Shared>,
    shutdown_tx: watch::Sender<bool>,
    done_rx: Option<mpsc::Receiver<()>>,
    done_tx: Option<mpsc::Sender<()>>,
    tasks: Vec<JoinHandle<()>>,
    listeners: Vec<ListenerRuntime>,
    revoked: RevocationList,
}

impl std::fmt::Debug for BrokerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerHandle")
            .field("listeners", &self.listeners)
            .finish()
    }
}

impl BrokerHandle {
    /// Listeners with their bound addresses or start error.
    pub fn listeners(&self) -> &[ListenerRuntime] {
        &self.listeners
    }

    /// The bound port of a listener.
    pub fn port(&self, listener_id: &str) -> Option<u16> {
        self.listeners
            .iter()
            .find(|l| l.id == listener_id)
            .and_then(|l| l.addrs.first())
            .map(|a| a.port())
    }

    /// True when one of the running listeners is bound to this port.
    pub fn uses_port(&self, port: u16) -> bool {
        self.listeners
            .iter()
            .any(|l| l.addrs.iter().any(|a| a.port() == port))
    }

    pub fn stats(&self) -> RuntimeStats {
        let (i, o) = self.shared.stats.rates();
        let st = self.shared.lock();
        RuntimeStats {
            clients: st.connected as u32,
            msg_in_rate: i,
            msg_out_rate: o,
            msg_in_total: self.shared.stats.total_in.load(Ordering::Relaxed),
            msg_out_total: self.shared.stats.total_out.load(Ordering::Relaxed),
            dropped: self.shared.stats.dropped.load(Ordering::Relaxed),
            retained: st.retained.len() as u32,
            started_at: self.shared.started_at_ms,
            uptime_s: ((log::now_ms() - self.shared.started_at_ms).max(0) / 1000) as u64,
        }
    }

    /// Connected clients, oldest first.
    pub fn clients(&self) -> Vec<BrokerClient> {
        let st = self.shared.lock();
        let mut v: Vec<BrokerClient> = st
            .sessions
            .values()
            .filter_map(|s| s.conn.as_ref())
            .map(|c| {
                let i = &c.info;
                BrokerClient {
                    client_id: i.client_id.clone(),
                    address: i.address.clone(),
                    protocol: i.version.label().to_string(),
                    transport: i.transport.to_string(),
                    connected_at: i.connected_at,
                    subscriptions: i.subscriptions.load(Ordering::Relaxed),
                    msg_in: i.msg_in.load(Ordering::Relaxed),
                    msg_out: i.msg_out.load(Ordering::Relaxed),
                    cert_cn: i.cert.as_ref().and_then(|c| c.cn.clone()),
                    username: i.username.clone(),
                }
            })
            .collect();
        v.sort_by(|a, b| {
            a.connected_at
                .cmp(&b.connected_at)
                .then(a.client_id.cmp(&b.client_id))
        });
        v
    }

    /// Disconnects a client (MQTT 5 reason 0x98, administrative action). Its last will is published.
    pub fn disconnect_client(&self, client_id: &str) -> bool {
        let st = self.shared.lock();
        match st.sessions.get(client_id).and_then(|s| s.conn.as_ref()) {
            Some(c) => {
                let ok = c.tx.send(Outbound::Disconnect(0x98)).is_ok();
                drop(st);
                if ok {
                    self.shared.log.info(format!(
                        "client {client_id} disconnected from the Broker screen"
                    ));
                }
                ok
            }
            None => false,
        }
    }

    /// Log lines after `after_seq` at `level` or above.
    pub fn log(&self, level: LogLevel, after_seq: u64) -> Vec<LogLine> {
        self.shared.log.lines(level, after_seq)
    }

    /// The log buffer.
    pub fn log_buffer(&self) -> &LogBuffer {
        &self.shared.log
    }

    /// Connection state per client-certificate serial (normalised), for "last seen".
    pub fn cert_seen(&self) -> HashMap<String, CertSeen> {
        self.shared.lock().cert_seen.clone()
    }

    /// Replaces the authentication settings; applies to the next CONNECT and TLS handshake.
    pub fn update_auth(&self, auth: AuthConfig) {
        if let Ok(mut a) = self.shared.auth.write() {
            *a = Arc::new(auth);
        }
    }

    /// The live revocation list.
    pub fn revocations(&self) -> &RevocationList {
        &self.revoked
    }

    /// Number of retained messages.
    pub fn retained_count(&self) -> usize {
        self.shared.lock().retained.len()
    }

    /// Stops listeners and closes every client (MQTT 5 clients get DISCONNECT 0x8B), waiting up to 5 s.
    pub async fn stop(mut self) {
        let _ = self.shutdown_tx.send(true);
        for t in self.tasks.drain(..) {
            let _ = t.await;
        }
        self.done_tx.take();
        if let Some(mut rx) = self.done_rx.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                while rx.recv().await.is_some() {}
            })
            .await;
        }
        self.shared.log.info("broker stopped");
    }
}

impl Drop for BrokerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
    }
}
