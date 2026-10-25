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
use state::{Shared, State};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
pub use tls::{normalize_serial, PeerCert, RevocationList, TlsMaterial};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// Default maximum packet size accepted from clients (and announced in CONNACK on MQTT 5): 16 MiB.
pub const DEFAULT_MAX_PACKET: usize = 16 * 1024 * 1024;

/// Resource limits of the broker (SPEC 6, limits table). The defaults are the shipped values; tests shorten the
/// durations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Connections that have not finished CONNECT (handshake, upgrade, login) per listener.
    pub pending_per_listener: usize,
    /// Connections that have not finished CONNECT per source address.
    pub pending_per_address: usize,
    /// Largest CONNECT accepted before authentication (the configured maximum packet applies after).
    pub connect_max: usize,
    /// Wait before a refused login gets its CONNACK and the connection closes.
    pub login_failure_delay: Duration,
    /// Password checks (argon2) running at the same time; further logins wait.
    pub parallel_password_checks: usize,
    /// An unacknowledged QoS 1/2 delivery older than this is sent once more.
    pub inflight_resend: Duration,
    /// The connection closes when its in-flight window stays full this long, or a write to it stalls this long.
    pub inflight_close: Duration,
    /// How long the owner has to answer the web page prompt (no answer: refused).
    pub origin_prompt_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            pending_per_listener: 64,
            pending_per_address: 8,
            connect_max: 64 * 1024,
            login_failure_delay: Duration::from_secs(1),
            parallel_password_checks: 4,
            inflight_resend: Duration::from_secs(30),
            inflight_close: Duration::from_secs(60),
            origin_prompt_timeout: Duration::from_secs(30),
        }
    }
}

/// A web page asks to connect without a login: what the owner is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginRequest {
    pub listener_id: String,
    pub kind: ListenerKind,
    pub port: u16,
    /// The `Origin` header as sent by the browser (for example `https://example.com`).
    pub origin: String,
}

/// Asks the owner whether a web page may connect (SPEC 6, allowed web origins). The answer arrives on the returned
/// receiver: true allows (the caller also adds the origin to the listener's list), false or a dropped sender refuses.
#[derive(Clone)]
pub struct OriginPrompt(
    pub Arc<dyn Fn(OriginRequest) -> tokio::sync::oneshot::Receiver<bool> + Send + Sync>,
);

impl std::fmt::Debug for OriginPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginPrompt")
    }
}

/// Normalises an origin for comparison: trimmed, lower case, no trailing slash.
pub fn normalize_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// One listener of the runtime.
#[derive(Debug, Clone)]
pub struct ListenerSpec {
    pub id: String,
    pub kind: ListenerKind,
    pub bind: BindAddr,
    /// 0 picks a free port (see [`BrokerHandle::port`]).
    pub port: u16,
    /// Server certificate, key and client CA for TLS and WSS.
    pub tls: Option<Arc<TlsMaterial>>,
    /// Web origins allowed to connect to a WS or WSS listener while Anonymous is on (normalised).
    pub allowed_origins: Vec<String>,
}

/// Everything the runtime needs. Build it by hand, from the manager ([`RuntimeConfig::from_manager`]) or from a
/// TOML file ([`RuntimeConfig::from_toml_file`]).
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub listeners: Vec<ListenerSpec>,
    pub auth: AuthConfig,
    /// Client certificate required on every encrypted listener (TLS and WSS); plain listeners are never affected.
    pub require_client_cert: bool,
    pub max_clients: usize,
    pub max_packet_size: usize,
    /// Revoked client-certificate serials; shared, so revocations apply while running.
    pub revoked: RevocationList,
    /// Log buffer; pass the manager's to keep the log across restarts.
    pub log: LogBuffer,
    /// Resource limits.
    pub limits: Limits,
    /// Asks the owner about unknown web origins; None refuses them.
    pub origin_prompt: Option<OriginPrompt>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            listeners: Vec::new(),
            auth: AuthConfig::default(),
            require_client_cert: false,
            max_clients: 1000,
            max_packet_size: DEFAULT_MAX_PACKET,
            revoked: RevocationList::default(),
            log: LogBuffer::new(),
            limits: Limits::default(),
            origin_prompt: None,
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

/// A bound (or failed) listener of a running broker. A listener that cannot start fails alone: its error is kept here
/// and the others keep serving (change set 7).
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

/// "1 session" / "2 sessions".
pub fn sessions_text(n: usize) -> String {
    if n == 1 {
        "1 session".into()
    } else {
        format!("{n} sessions")
    }
}

/// One listener task of a running broker: its accept loops, its TLS state and the channel that ends its sessions.
struct RunningListener {
    spec: ListenerSpec,
    tls: session::TlsSlot,
    /// The live list of allowed web origins (read at each WebSocket upgrade).
    origins: Arc<RwLock<Vec<String>>>,
    addrs: Vec<SocketAddr>,
    error: Option<BrokerError>,
    /// Ends the accept loops only (a rebind keeps the sessions).
    stop_tx: watch::Sender<bool>,
    /// Ends the sessions of this listener.
    closed_tx: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl RunningListener {
    fn runtime(&self) -> ListenerRuntime {
        ListenerRuntime {
            id: self.spec.id.clone(),
            kind: self.spec.kind,
            addrs: self.addrs.clone(),
            error: self.error.clone(),
        }
    }
    fn port(&self) -> u16 {
        self.addrs
            .first()
            .map(|a| a.port())
            .unwrap_or(self.spec.port)
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
            require_client_cert: AtomicBool::new(cfg.require_client_cert),
            max_clients: cfg.max_clients.max(1),
            max_packet: cfg.max_packet_size.clamp(1024, 256 * 1024 * 1024),
            started_at_ms: log::now_ms(),
            state: Mutex::new(State::default()),
            next_conn_id: AtomicU64::new(1),
            password_checks: tokio::sync::Semaphore::new(
                cfg.limits.parallel_password_checks.max(1),
            ),
            checks_running: std::sync::atomic::AtomicUsize::new(0),
            checks_peak: std::sync::atomic::AtomicUsize::new(0),
            limits: cfg.limits.clone(),
            pending: Mutex::new(HashMap::new()),
            limit_log: Mutex::new(HashMap::new()),
            origin_prompt: RwLock::new(cfg.origin_prompt.clone()),
            origin_waits: Mutex::new(HashMap::new()),
        });
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (done_tx, done_rx) = mpsc::channel::<()>(1);
        let handle = BrokerHandle {
            shared: shared.clone(),
            rt: tokio::runtime::Handle::current(),
            shutdown_tx,
            done_tx: Mutex::new(Some(done_tx)),
            done_rx: Mutex::new(Some(done_rx)),
            listeners: Mutex::new(Vec::new()),
            tasks: Mutex::new(Vec::new()),
            revoked: cfg.revoked.clone(),
        };
        let mut started = Vec::new();
        for spec in &cfg.listeners {
            let l = handle.start_one(spec.clone(), None);
            match &l.error {
                Some(e) => shared.log.error(format!(
                    "{} listener on port {} could not start: {}",
                    spec.kind.label(),
                    spec.port,
                    e.detail.clone().unwrap_or_default()
                )),
                None => {
                    let mut where_ = match spec.bind {
                        BindAddr::Network => "all interfaces".to_string(),
                        BindAddr::Localhost => "localhost".to_string(),
                        BindAddr::Addr(ip) => ip.to_string(),
                    };
                    if spec.kind.websocket() {
                        where_.push_str(", path /mqtt");
                    }
                    let mut extra = String::new();
                    if cfg.require_client_cert && spec.kind.encrypted() {
                        extra.push_str(", client certificate required");
                    }
                    shared.log.info(format!(
                        "{} listener on port {} ({where_}{extra})",
                        spec.kind.label(),
                        l.port()
                    ));
                }
            }
            started.push(l);
        }
        if started.iter().all(|l| l.error.is_some()) {
            let _ = handle.shutdown_tx.send(true);
            let detail = started
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
        let serving = started.iter().filter(|l| l.error.is_none()).count();
        *handle.listeners_lock() = started;
        // Session expiry sweeper.
        {
            let shared = shared.clone();
            let mut sd = shutdown_rx;
            handle.tasks_lock().push(tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(1));
                loop {
                    tokio::select! {
                        _ = sd.changed() => break,
                        _ = tick.tick() => shared.expire_sessions(),
                    }
                }
            }));
        }
        shared.log.info(format!(
            "broker started · {serving} listener{}",
            if serving == 1 { "" } else { "s" }
        ));
        Ok(handle)
    }
}

/// A running broker. Dropping it stops the broker without waiting; [`BrokerHandle::stop`] waits for the tasks.
///
/// Every listener is its own task (change set 7): listeners start, stop and bind again one by one while the broker
/// runs ([`BrokerHandle::start_listener`], [`BrokerHandle::stop_listener`], [`BrokerHandle::rebind`]); the TLS
/// material ([`BrokerHandle::set_tls_material`]), the client-certificate rule and the users are read live; and the
/// session registry lets [`BrokerHandle::revalidate`] drop exactly the sessions a change invalidates.
pub struct BrokerHandle {
    shared: Arc<Shared>,
    rt: tokio::runtime::Handle,
    shutdown_tx: watch::Sender<bool>,
    done_tx: Mutex<Option<mpsc::Sender<()>>>,
    done_rx: Mutex<Option<mpsc::Receiver<()>>>,
    listeners: Mutex<Vec<RunningListener>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    revoked: RevocationList,
}

impl std::fmt::Debug for BrokerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerHandle")
            .field("listeners", &self.listeners())
            .finish()
    }
}

fn plock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl BrokerHandle {
    fn listeners_lock(&self) -> std::sync::MutexGuard<'_, Vec<RunningListener>> {
        plock(&self.listeners)
    }
    fn tasks_lock(&self) -> std::sync::MutexGuard<'_, Vec<JoinHandle<()>>> {
        plock(&self.tasks)
    }

    /// Binds one listener and spawns its accept loops. `closed` keeps the sessions channel of a listener bound again
    /// (rebind); None makes a new one. Never fails: an error is kept on the listener.
    fn start_one(
        &self,
        spec: ListenerSpec,
        closed: Option<watch::Sender<bool>>,
    ) -> RunningListener {
        let (stop_tx, stop_rx) = watch::channel(false);
        let closed_tx = closed.unwrap_or_else(|| watch::channel(false).0);
        let origins: Vec<String> = spec
            .allowed_origins
            .iter()
            .map(|o| normalize_origin(o))
            .collect();
        let mut rl = RunningListener {
            tls: Arc::new(RwLock::new(None)),
            origins: Arc::new(RwLock::new(origins)),
            addrs: Vec::new(),
            error: None,
            stop_tx,
            closed_tx,
            tasks: Vec::new(),
            spec,
        };
        let spec = &rl.spec;
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
        if spec.kind.encrypted() {
            let built = match &spec.tls {
                None => Err("no server certificate".to_string()),
                Some(m) => tls::TlsListener::new(m, self.revoked.clone()),
            };
            match built {
                Ok(t) => {
                    if let Ok(mut g) = rl.tls.write() {
                        *g = Some(Arc::new(t));
                    }
                }
                Err(e) => {
                    rl.error = Some(fail(e));
                    return rl;
                }
            }
        }
        let socks = match listener::bind(spec.bind, spec.port) {
            Ok(s) => s,
            Err(e) => {
                rl.error = Some(fail(io_detail(&e)));
                return rl;
            }
        };
        let lctx = Arc::new(ListenerCtx {
            id: spec.id.clone(),
            kind: spec.kind,
            tls: rl.tls.clone(),
            origins: rl.origins.clone(),
            pending: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            port: Arc::new(std::sync::atomic::AtomicU16::new(spec.port)),
        });
        let done = plock(&self.done_tx).clone();
        let Some(done) = done else {
            rl.error = Some(fail("broker stopping".into()));
            return rl;
        };
        let _guard = self.rt.enter();
        for sock in socks {
            let addr = match sock.local_addr() {
                Ok(a) => a,
                Err(_) => continue,
            };
            let l = match tokio::net::TcpListener::from_std(sock) {
                Ok(l) => l,
                Err(e) => {
                    self.shared
                        .log
                        .error(format!("listener {addr} could not start: {e}"));
                    continue;
                }
            };
            rl.addrs.push(addr);
            lctx.port.store(addr.port(), Ordering::Relaxed);
            let sig = listener::Signals {
                stop: stop_rx.clone(),
                shutdown: self.shutdown_tx.subscribe(),
                closed: rl.closed_tx.subscribe(),
            };
            rl.tasks.push(self.rt.spawn(listener::accept_loop(
                self.shared.clone(),
                lctx.clone(),
                l,
                sig,
                done.clone(),
            )));
        }
        if rl.addrs.is_empty() && rl.error.is_none() {
            rl.error = Some(fail("no address".into()));
        }
        rl
    }

    /// Listeners with their bound addresses or start error.
    pub fn listeners(&self) -> Vec<ListenerRuntime> {
        self.listeners_lock().iter().map(|l| l.runtime()).collect()
    }

    /// The specification of every listener of the running broker (as started, including failed ones).
    pub fn specs(&self) -> Vec<ListenerSpec> {
        self.listeners_lock()
            .iter()
            .map(|l| l.spec.clone())
            .collect()
    }

    /// The bound port of a listener.
    pub fn port(&self, listener_id: &str) -> Option<u16> {
        self.listeners_lock()
            .iter()
            .find(|l| l.spec.id == listener_id)
            .and_then(|l| l.addrs.first())
            .map(|a| a.port())
    }

    /// True when one of the running listeners is bound to this port.
    pub fn uses_port(&self, port: u16) -> bool {
        self.listeners_lock()
            .iter()
            .any(|l| l.addrs.iter().any(|a| a.port() == port))
    }

    /// True when at least one listener serves.
    pub fn serving(&self) -> bool {
        self.listeners_lock()
            .iter()
            .any(|l| l.error.is_none() && !l.addrs.is_empty())
    }

    /// Connected sessions that arrived on a listener.
    pub fn sessions_on(&self, listener_id: &str) -> usize {
        self.shared
            .lock()
            .sessions
            .values()
            .filter_map(|s| s.conn.as_ref())
            .filter(|c| c.info.listener_id == listener_id)
            .count()
    }

    /// Starts one listener while the broker runs. A listener with the same id is stopped first (its sessions drop).
    /// A listener that cannot start keeps its error (see [`ListenerRuntime::error`]); the others are not touched.
    pub async fn start_listener(&self, spec: ListenerSpec) -> ListenerRuntime {
        self.stop_listener(&spec.id).await;
        let rl = self.start_one(spec, None);
        let rt = rl.runtime();
        self.listeners_lock().push(rl);
        rt
    }

    /// Stops one listener and drops its sessions. Returns the number of sessions dropped, None when there is no
    /// such listener. Waits until its port is released.
    pub async fn stop_listener(&self, listener_id: &str) -> Option<usize> {
        let rl = {
            let mut ls = self.listeners_lock();
            let i = ls.iter().position(|l| l.spec.id == listener_id)?;
            ls.remove(i)
        };
        let dropped = self.sessions_on(listener_id);
        let _ = rl.stop_tx.send(true);
        let _ = rl.closed_tx.send(true);
        for t in rl.tasks {
            let _ = t.await;
        }
        Some(dropped)
    }

    /// Binds every listener again with a new reachability. Open sessions stay: accepted connections do not depend
    /// on the listening socket. Listeners that failed before are tried again.
    pub async fn rebind(&self, bind: BindAddr) -> Vec<ListenerRuntime> {
        let tasks: Vec<JoinHandle<()>> = {
            let mut ls = self.listeners_lock();
            let mut all = Vec::new();
            for l in ls.iter_mut() {
                let _ = l.stop_tx.send(true);
                all.append(&mut l.tasks);
            }
            all
        };
        for t in tasks {
            let _ = t.await;
        }
        let mut ls = self.listeners_lock();
        let old: Vec<RunningListener> = ls.drain(..).collect();
        for l in old {
            let mut spec = l.spec.clone();
            spec.bind = bind;
            let rl = self.start_one(spec, Some(l.closed_tx.clone()));
            ls.push(rl);
        }
        ls.iter().map(|l| l.runtime()).collect()
    }

    /// Swaps the TLS material of every encrypted listener; new handshakes use it, open sessions are kept (call
    /// [`BrokerHandle::revalidate`] to drop the ones a new CA no longer trusts). `None` refuses new handshakes.
    pub fn set_tls_material(&self, material: Option<Arc<TlsMaterial>>) -> Result<(), String> {
        let built = match &material {
            Some(m) => Some(Arc::new(tls::TlsListener::new(m, self.revoked.clone())?)),
            None => None,
        };
        for l in self.listeners_lock().iter_mut() {
            if !l.spec.kind.encrypted() {
                continue;
            }
            l.spec.tls = material.clone();
            if let Ok(mut g) = l.tls.write() {
                *g = built.clone();
            }
        }
        Ok(())
    }

    /// Client certificate required on encrypted listeners; applies to the next handshake and CONNECT.
    pub fn set_require_client_cert(&self, on: bool) {
        self.shared.require_client_cert.store(on, Ordering::Relaxed);
    }

    /// Drops the sessions the current settings no longer accept (session registry, change set 7): a password
    /// session whose user was removed or whose password changed; an anonymous session once Anonymous is off; on
    /// encrypted listeners, a session without a client certificate once one is required, and a session whose client
    /// certificate is revoked or no longer signed by the verifying CA. Returns the number of sessions dropped.
    pub fn revalidate(&self) -> usize {
        let auth = self.shared.auth();
        let require = self.shared.require_client_cert();
        let tls: HashMap<String, Option<Arc<tls::TlsListener>>> = self
            .listeners_lock()
            .iter()
            .map(|l| (l.spec.id.clone(), l.tls.read().ok().and_then(|g| g.clone())))
            .collect();
        let st = self.shared.lock();
        let mut dropped = 0;
        for c in st.sessions.values().filter_map(|s| s.conn.as_ref()) {
            let i = &c.info;
            let valid = auth.anonymous || {
                let cert_ok = if !i.kind.encrypted() {
                    true
                } else {
                    match &i.cert {
                        Some(pc) => tls
                            .get(&i.listener_id)
                            .and_then(|t| t.as_ref())
                            .is_some_and(|t| t.still_trusts(pc)),
                        None => !require,
                    }
                };
                cert_ok && auth::login_still_valid(&auth, &i.login)
            };
            if !valid && c.ctl.send(0x87).is_ok() {
                dropped += 1;
            }
        }
        dropped
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
                let ok = c.ctl.send(0x98).is_ok();
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

    /// Replaces the authentication settings; applies to the next CONNECT and TLS handshake (open sessions: see
    /// [`BrokerHandle::revalidate`]).
    pub fn update_auth(&self, auth: AuthConfig) {
        if let Ok(mut a) = self.shared.auth.write() {
            *a = Arc::new(auth);
        }
    }

    /// Replaces the allowed web origins of a listener while the broker runs (next WebSocket upgrade). False when
    /// there is no such listener.
    pub fn set_allowed_origins(&self, listener_id: &str, origins: &[String]) -> bool {
        let mut ls = self.listeners_lock();
        let Some(l) = ls.iter_mut().find(|l| l.spec.id == listener_id) else {
            return false;
        };
        let list: Vec<String> = origins.iter().map(|o| normalize_origin(o)).collect();
        l.spec.allowed_origins = list.clone();
        if let Ok(mut g) = l.origins.write() {
            *g = list;
        }
        true
    }

    /// The allowed web origins of a running listener.
    pub fn allowed_origins(&self, listener_id: &str) -> Option<Vec<String>> {
        self.listeners_lock()
            .iter()
            .find(|l| l.spec.id == listener_id)
            .map(|l| l.origins.read().map(|g| g.clone()).unwrap_or_default())
    }

    /// Replaces the callback that asks the owner about unknown web origins (None refuses them).
    pub fn set_origin_prompt(&self, prompt: Option<OriginPrompt>) {
        if let Ok(mut g) = self.shared.origin_prompt.write() {
            *g = prompt;
        }
    }

    /// The most password checks seen running at the same time (tests of the limit).
    #[doc(hidden)]
    pub fn password_checks_peak(&self) -> usize {
        self.shared.checks_peak.load(Ordering::Relaxed)
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
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
        let mut tasks: Vec<JoinHandle<()>> = self.tasks_lock().drain(..).collect();
        for l in self.listeners_lock().iter_mut() {
            tasks.append(&mut l.tasks);
        }
        for t in tasks {
            let _ = t.await;
        }
        plock(&self.done_tx).take();
        let rx = plock(&self.done_rx).take();
        if let Some(mut rx) = rx {
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
