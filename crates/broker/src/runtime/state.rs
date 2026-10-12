//! Shared broker state: sessions, subscription index, retained messages and message routing.

use super::auth::AuthConfig;
use super::codec::{Message, Version};
use super::log::LogBuffer;
use super::router::{Router, SubOpts};
use super::stats::Stats;
use super::tls::PeerCert;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// QoS 0 messages queued for one client beyond which new QoS 0 messages are dropped (slow client).
pub(crate) const QOS0_QUEUE_LIMIT: usize = 1_000;
/// Hard limit of queued messages per client (QoS 1 and 2 included).
pub(crate) const QUEUE_HARD_LIMIT: usize = 100_000;
/// Messages kept for a disconnected persistent session.
pub(crate) const OFFLINE_LIMIT: usize = 1_000;

/// A message on its way to one subscriber.
#[derive(Debug, Clone)]
pub(crate) struct Delivery {
    pub msg: Arc<Message>,
    pub qos: u8,
    pub retain: bool,
    pub sub_ids: Vec<usize>,
    pub dup: bool,
}

/// Commands to a connection task.
#[derive(Debug)]
pub(crate) enum Outbound {
    Publish(Delivery),
    /// Close with this MQTT 5 reason code (0x8E takeover, 0x98 administrative action).
    Disconnect(u8),
}

/// What the clients table shows about a connection.
#[derive(Debug)]
pub(crate) struct ClientInfo {
    pub client_id: String,
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
    pub max_clients: usize,
    pub max_packet: usize,
    pub started_at_ms: i64,
    pub state: Mutex<State>,
    pub next_conn_id: AtomicU64,
}

impl Shared {
    pub fn auth(&self) -> Arc<AuthConfig> {
        self.auth.read().map(|a| a.clone()).unwrap_or_default()
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
