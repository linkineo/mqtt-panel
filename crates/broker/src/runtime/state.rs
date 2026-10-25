//! Shared broker state: sessions, subscription index, retained messages and message routing.

use super::auth::{AuthConfig, Login};
use super::codec::{Message, Version};
use super::log::LogBuffer;
use super::router::{Router, SubOpts};
use super::stats::Stats;
use super::tls::PeerCert;
use crate::config::ListenerKind;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// QoS 0 messages queued for one client beyond which new QoS 0 messages are dropped (slow client).
pub(crate) const QOS0_QUEUE_LIMIT: usize = 1_000;
/// Hard limit of queued messages per client (QoS 1 and 2 included).
pub(crate) const QUEUE_HARD_LIMIT: usize = 100_000;
/// Messages kept for a disconnected persistent session.
pub(crate) const OFFLINE_LIMIT: usize = 1_000;

/// The answer to a pending web page prompt (None while the owner has not answered).
pub(crate) type OriginWait = tokio::sync::watch::Receiver<Option<bool>>;

/// A message on its way to one subscriber.
#[derive(Debug, Clone)]
pub(crate) struct Delivery {
    pub msg: Arc<Message>,
    pub qos: u8,
    pub retain: bool,
    pub sub_ids: Vec<usize>,
    pub dup: bool,
}

/// Messages to a connection task. Control orders (takeover, kick, live-apply drops) travel on their own channel
/// ([`ConnRef::ctl`]), which the session loop polls even while the in-flight window is full.
#[derive(Debug)]
pub(crate) enum Outbound {
    Publish(Delivery),
}

/// What the clients table shows about a connection, and the session registry entry (change set 7): the listener it
/// arrived on and how it logged in, so a change drops exactly the sessions it invalidates.
#[derive(Debug)]
pub(crate) struct ClientInfo {
    pub client_id: String,
    pub listener_id: String,
    pub kind: ListenerKind,
    pub login: Login,
    pub address: String,
    pub version: Version,
    pub transport: &'static str,
    pub connected_at: i64,
    pub cert: Option<PeerCert>,
    pub username: Option<String>,
    pub msg_in: AtomicU64,
    pub msg_out: AtomicU64,
    pub subscriptions: AtomicU32,
    pub dropped: AtomicU64,
    pub last_drop_log: Mutex<Option<Instant>>,
}

/// A live connection attached to a session.
#[derive(Debug, Clone)]
pub(crate) struct ConnRef {
    pub conn_id: u64,
    pub tx: mpsc::UnboundedSender<Outbound>,
    /// Close with this MQTT 5 reason code (0x8E takeover, 0x87 no longer authorised, 0x98 administrative action).
    pub ctl: mpsc::UnboundedSender<u8>,
    pub queued: Arc<AtomicUsize>,
    pub info: Arc<ClientInfo>,
}

#[derive(Debug, Default)]
pub(crate) struct Session {
    pub conn: Option<ConnRef>,
    pub subs: HashMap<String, SubOpts>,
    pub offline: VecDeque<Delivery>,
    /// Kept after the connection closes (clean session off, or MQTT 5 session expiry > 0).
    pub persistent: bool,
    /// Session expiry in seconds (u32::MAX: never).
    pub expiry: u32,
    /// When a disconnected persistent session expires.
    pub expires_at: Option<Instant>,
}

/// Connection count and last disconnect time per client-certificate serial.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CertSeen {
    pub connected: u32,
    /// Last disconnect time (ms since the epoch), when the certificate disconnected at least once.
    pub last_seen: Option<i64>,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    pub sessions: HashMap<String, Session>,
    pub router: Router,
    pub retained: HashMap<String, Arc<Message>>,
    pub cert_seen: HashMap<String, CertSeen>,
    pub connected: usize,
}

pub(crate) struct Shared {
    pub log: LogBuffer,
    pub stats: Stats,
    pub auth: RwLock<Arc<AuthConfig>>,
    /// Client certificate required on encrypted listeners; read at each handshake and CONNECT (change set 7).
    pub require_client_cert: AtomicBool,
    pub max_clients: usize,
    pub max_packet: usize,
    pub started_at_ms: i64,
    pub state: Mutex<State>,
    pub next_conn_id: AtomicU64,
    pub limits: super::Limits,
    /// Bounds the argon2 verifications running at the same time.
    pub password_checks: tokio::sync::Semaphore,
    /// Password checks running now, and the most seen at once (tests).
    pub checks_running: AtomicUsize,
    pub checks_peak: AtomicUsize,
    /// Connections that have not finished CONNECT, per source address.
    pub pending: Mutex<HashMap<IpAddr, usize>>,
    /// Last "limit reached" warning per source address (one per minute).
    pub limit_log: Mutex<HashMap<IpAddr, Instant>>,
    pub origin_prompt: RwLock<Option<super::OriginPrompt>>,
    /// Origin prompts waiting for the owner, per (listener id, origin): later upgrades wait on the same answer.
    pub origin_waits: Mutex<HashMap<(String, String), OriginWait>>,
}

impl Shared {
    pub fn auth(&self) -> Arc<AuthConfig> {
        self.auth.read().map(|a| a.clone()).unwrap_or_default()
    }

    pub fn require_client_cert(&self) -> bool {
        self.require_client_cert.load(Ordering::Relaxed)
    }

    /// Logs a warning about a limit at most once per minute per source address.
    pub fn warn_limit(&self, ip: IpAddr, text: impl FnOnce() -> String) {
        let mut m = self.limit_log.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        if m.get(&ip)
            .is_some_and(|t| now.duration_since(*t) < Duration::from_secs(60))
        {
            return;
        }
        if m.len() > 4096 {
            m.retain(|_, t| now.duration_since(*t) < Duration::from_secs(60));
        }
        m.insert(ip, now);
        drop(m);
        self.log.warn(text());
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A panic while holding the lock would poison it; keep serving with the inner state.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Routes a published message to subscribers and updates the retained store.
    pub fn route(&self, msg: Message) {
        let msg = Arc::new(msg);
        let mut st = self.lock();
        if msg.retain {
            if msg.payload.is_empty() {
                st.retained.remove(&msg.topic);
            } else {
                st.retained.insert(msg.topic.clone(), msg.clone());
            }
        }
        let matches = st.router.matches(&msg.topic);
        // One delivery per client: highest QoS of its matching subscriptions, all subscription ids.
        let mut targets: HashMap<String, (u8, bool, Vec<usize>)> = HashMap::new();
        let publisher = msg.publisher.as_deref();
        let mut add = |client: &str, o: &SubOpts| {
            if o.no_local && Some(client) == publisher {
                return;
            }
            let e = targets
                .entry(client.to_string())
                .or_insert((0, false, Vec::new()));
            e.0 = e.0.max(o.qos);
            e.1 |= o.retain_as_published;
            if let Some(id) = o.sub_id {
                e.2.push(id);
            }
        };
        for m in &matches.direct {
            add(&m.client_id, &m.opts);
        }
        for group in &matches.shared {
            let pick = group
                .iter()
                .find(|m| {
                    st.sessions
                        .get(&m.client_id)
                        .is_some_and(|s| s.conn.is_some())
                })
                .or_else(|| group.first());
            if let Some(m) = pick {
                add(&m.client_id, &m.opts);
            }
        }
        for (client, (qos, rap, sub_ids)) in targets {
            if let Some(sess) = st.sessions.get_mut(&client) {
                let d = Delivery {
                    msg: msg.clone(),
                    qos: qos.min(msg.qos),
                    retain: rap && msg.retain,
                    sub_ids,
                    dup: false,
                };
                self.deliver(sess, d);
            }
        }
    }

    /// Hands a delivery to a session: its connection queue, or the offline queue of a persistent session.
    pub fn deliver(&self, sess: &mut Session, d: Delivery) {
        if let Some(c) = &sess.conn {
            let q = c.queued.load(Ordering::Relaxed);
            if (d.qos == 0 && q >= QOS0_QUEUE_LIMIT) || q >= QUEUE_HARD_LIMIT {
                self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                let n = c.info.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                let mut last = c
                    .info
                    .last_drop_log
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                if last.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
                    *last = Some(Instant::now());
                    self.log.warn(format!(
                        "client {} is too slow: dropped QoS {} messages ({} so far)",
                        c.info.client_id, d.qos, n
                    ));
                }
                return;
            }
            c.queued.fetch_add(1, Ordering::Relaxed);
            if c.tx.send(Outbound::Publish(d)).is_err() {
                c.queued.fetch_sub(1, Ordering::Relaxed);
            }
            return;
        }
        if sess.persistent && d.qos > 0 {
            if sess.offline.len() >= OFFLINE_LIMIT {
                sess.offline.pop_front();
                self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            }
            sess.offline.push_back(d);
        }
    }

    /// Removes a session and its subscriptions.
    pub fn drop_session(st: &mut State, client_id: &str) {
        if let Some(sess) = st.sessions.remove(client_id) {
            for f in sess.subs.keys() {
                st.router.unsubscribe(client_id, f);
            }
        }
    }

    /// Removes disconnected sessions whose expiry passed.
    pub fn expire_sessions(&self) {
        let now = Instant::now();
        let mut st = self.lock();
        let expired: Vec<String> = st
            .sessions
            .iter()
            .filter(|(_, s)| s.conn.is_none() && s.expires_at.is_some_and(|t| t <= now))
            .map(|(k, _)| k.clone())
            .collect();
        for id in expired {
            Self::drop_session(&mut st, &id);
        }
    }
}
