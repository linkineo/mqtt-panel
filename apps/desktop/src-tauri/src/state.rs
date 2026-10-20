//! Application state: the profile store, settings, open connections (each with its engine), pending trust prompts.

use crate::types::*;
use mqttpanel_client::{msg, ClientEvent, ClientHandle, ConnInfo, Incoming, TrustInfo, UserMessage};
use mqttpanel_engine::{Engine, EngineConfig, IncomingMessage, Mqtt5Props, ProtoAssignment, ProtoFile};
use mqttpanel_profiles::{Settings, Store, StoredProfile};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc;

pub const SENT_LOG_MAX: usize = 500;

/// Locks a mutex, recovering from poisoning (a panicked command must not take the app down).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct AppState {
    pub store: Store,
    pub settings: Mutex<Settings>,
    pub conns: Mutex<HashMap<String, Arc<Conn>>>,
    pub pending_trust: Mutex<HashMap<String, TrustInfo>>,
    pub broker: crate::broker::BrokerState,
    /// The assistant interface (docs/MCP.md): session, acknowledgement waits, Assistant log, requests for the user.
    pub assistant: crate::mcp::Hub,
}

#[derive(Debug, Clone)]
pub struct LinkState {
    pub state: String,
    pub negotiated: Option<String>,
    pub error: Option<UserMessage>,
    pub attempt: u32,
    pub max_packet: Option<u32>,
    pub sub_errors: Vec<SubscriptionError>,
}

pub struct PendingRequest {
    pub sent_id: String,
    pub at: i64,
    pub response_topic: String,
    pub deadline: i64,
}

pub struct RepeatState {
    pub status: RepeatStatus,
    pub cancel: tokio::sync::watch::Sender<bool>,
    pub sent_id: String,
}

/// One open (or recently open) connection and its engine.
pub struct Conn {
    pub id: String,
    pub name: Mutex<String>,
    pub is_local: bool,
    pub requested_version: String,
    pub engine: Mutex<Engine>,
    pub handle: Mutex<Option<ClientHandle>>,
    pub link: Mutex<LinkState>,
    pub sent: Mutex<VecDeque<SentEntry>>,
    pub tokens: Mutex<HashMap<u64, String>>,
    pub requests: Mutex<HashMap<String, PendingRequest>>,
    pub response_subs: Mutex<HashSet<String>>,
    pub updates: Mutex<Vec<SentEntry>>,
    pub repeat: Mutex<Option<RepeatState>>,
    pub correlation: AtomicU64,
    pub ever_connected: Mutex<bool>,
    /// The open connection accepted the broker certificate without verification (profile switch or "only this time").
    pub skip_verify: std::sync::atomic::AtomicBool,
}

pub fn engine_config(s: &Settings) -> EngineConfig {
    EngineConfig {
        buffer_per_topic: s.buffer_per_topic as usize,
        points_per_series: s.points_per_series as usize,
        payload_render_limit: s.payload_render_limit_kb as usize * 1024,
        memory_budget: s.memory_budget_mb as usize * 1024 * 1024,
        ..Default::default()
    }
}

impl Conn {
    pub fn new(p: &StoredProfile, settings: &Settings) -> Conn {
        let mut engine = Engine::new(engine_config(settings));
        apply_proto(&mut engine, p);
        engine.set_sparkline_paths(p.profile.sparkline_paths());
        Conn {
            id: p.profile.id.clone(),
            name: Mutex::new(p.profile.name.clone()),
            is_local: p.profile.is_local_broker,
            requested_version: p.profile.mqtt_version.clone(),
            engine: Mutex::new(engine),
            handle: Mutex::new(None),
            link: Mutex::new(LinkState { state: "connecting".into(), negotiated: None, error: None, attempt: 0, max_packet: None, sub_errors: vec![] }),
            sent: Mutex::new(VecDeque::new()),
            tokens: Mutex::new(HashMap::new()),
            requests: Mutex::new(HashMap::new()),
            response_subs: Mutex::new(HashSet::new()),
            updates: Mutex::new(Vec::new()),
            repeat: Mutex::new(None),
            correlation: AtomicU64::new(1041),
            ever_connected: Mutex::new(false),
            skip_verify: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn status(&self, now: i64) -> ConnectionStatus {
        let link = lock(&self.link).clone();
        let (topics, rate) = {
            let e = lock(&self.engine);
            (e.topic_count(), e.msg_rate(now))
        };
        ConnectionStatus {
            id: self.id.clone(),
            name: lock(&self.name).clone(),
            state: link.state,
            negotiated_version: link.negotiated,
            requested_version: self.requested_version.clone(),
            topics,
            msg_rate: (rate * 10.0).round() / 10.0,
            skip_verify: self.skip_verify.load(std::sync::atomic::Ordering::Relaxed),
            error: link.error,
            reconnect_attempt: link.attempt,
            max_packet_size: link.max_packet,
            is_local_broker: self.is_local,
            subscription_errors: link.sub_errors,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(lock(&self.link).state.as_str(), "connected" | "connecting" | "reconnecting")
    }

    pub fn handle(&self) -> Option<ClientHandle> {
        lock(&self.handle).clone()
    }

    pub fn negotiated_v5(&self) -> bool {
        lock(&self.link).negotiated.as_deref() == Some("5")
    }

    /// Adds or replaces a sent-log entry and queues it for the next tick.
    pub fn upsert_sent(&self, entry: SentEntry) {
        {
            let mut log = lock(&self.sent);
            if let Some(e) = log.iter_mut().find(|e| e.id == entry.id) {
                *e = entry.clone();
            } else {
                log.push_front(entry.clone());
                while log.len() > SENT_LOG_MAX {
                    log.pop_back();
                }
            }
        }
        lock(&self.updates).push(entry);
    }

    pub fn update_sent<F: FnOnce(&mut SentEntry)>(&self, id: &str, f: F) {
        let updated = {
            let mut log = lock(&self.sent);
            log.iter_mut().find(|e| e.id == id).map(|e| {
                f(e);
                e.clone()
            })
        };
        if let Some(e) = updated {
            lock(&self.updates).push(e);
        }
    }

    pub fn next_correlation(&self) -> String {
        format!("c-{}", self.correlation.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

pub fn apply_proto(engine: &mut Engine, p: &StoredProfile) {
    if p.profile.proto_schemas.is_empty() {
        return;
    }
    let files = p.blobs.proto_files.iter().map(|(name, content)| ProtoFile { name: name.clone(), content: content.clone() }).collect();
    let assignments = p
        .profile
        .proto_schemas
        .iter()
        .map(|s| ProtoAssignment { file_name: s.file_name.clone(), topic_filter: s.topic_filter.clone(), message_type: s.message_type.clone() })
        .collect();
    let _ = engine.set_proto_schemas(files, assignments);
}

fn props_to_engine(p: &mqttpanel_client::Props5) -> Mqtt5Props {
    Mqtt5Props {
        content_type: p.content_type.clone(),
        payload_format: p.payload_format,
        response_topic: p.response_topic.clone(),
        correlation_data: p.correlation_data.as_ref().map(|d| match std::str::from_utf8(d) {
            Ok(s) => s.to_string(),
            Err(_) => d.iter().map(|b| format!("{b:02x}")).collect(),
        }),
        message_expiry: p.message_expiry,
        user_properties: p.user_properties.clone(),
        subscription_ids: p.subscription_ids.iter().map(|i| *i as u32).collect(),
        topic_alias: p.topic_alias,
    }
}

fn correlation_text(d: &[u8]) -> String {
    match std::str::from_utf8(d) {
        Ok(s) => s.to_string(),
        Err(_) => d.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

/// Applies the first CONNACK.
pub fn on_connected(conn: &Conn, info: &ConnInfo) {
    let mut link = lock(&conn.link);
    link.state = "connected".into();
    link.negotiated = Some(info.version.label().to_string());
    link.error = None;
    link.attempt = 0;
    link.max_packet = info.max_packet_size;
}

/// Consumes client events: messages go into the engine in batches; acks, replies and state changes update the connection.
pub async fn pump(conn: Arc<Conn>, mut rx: mpsc::Receiver<ClientEvent>) {
    let mut batch = Vec::with_capacity(1024);
    while let Some(first) = rx.recv().await {
        batch.clear();
        batch.push(first);
        while batch.len() < 4096 {
            match rx.try_recv() {
                Ok(e) => batch.push(e),
                Err(_) => break,
            }
        }
        let now = now_ms();
        let mut msgs: Vec<Incoming> = vec![];
        for ev in batch.drain(..) {
            match ev {
                ClientEvent::Message(m) => msgs.push(m),
                other => {
                    if !msgs.is_empty() {
                        ingest(&conn, std::mem::take(&mut msgs), now);
                    }
                    handle_event(&conn, other, now);
                }
            }
        }
        if !msgs.is_empty() {
            ingest(&conn, msgs, now);
        }
    }
}

fn ingest(conn: &Conn, msgs: Vec<Incoming>, now: i64) {
    let mut replies: Vec<(String, Reply)> = vec![];
    {
        let subs = lock(&conn.response_subs).clone();
        let mut reqs = lock(&conn.requests);
        let mut e = lock(&conn.engine);
        for m in msgs {
            if !subs.is_empty() && subs.contains(&m.topic) {
                if let Some(cd) = m.props.as_ref().and_then(|p| p.correlation_data.as_ref()) {
                    let key = correlation_text(cd);
                    if reqs.get(&key).is_some_and(|r| r.response_topic == m.topic) {
                        let req = reqs.remove(&key).expect("checked");
                        replies.push((
                            req.sent_id,
                            Reply { at: now, rtt_ms: now - req.at, payload_text: String::from_utf8_lossy(&m.payload).to_string(), correlation_data: key },
                        ));
                    }
                }
            }
            e.ingest(IncomingMessage {
                topic: m.topic,
                payload: m.payload.to_vec(),
                qos: m.qos,
                retain: m.retain,
                dup: m.dup,
                received_at: now,
                props: m.props.as_ref().map(props_to_engine),
            });
        }
    }
    for (id, reply) in replies {
        conn.update_sent(&id, |e| e.reply = Some(reply));
    }
}

fn handle_event(conn: &Conn, ev: ClientEvent, now: i64) {
    match ev {
        ClientEvent::Message(_) => {}
        ClientEvent::Connected(info) => {
            let reconnect = std::mem::replace(&mut *lock(&conn.ever_connected), true);
            if reconnect {
                lock(&conn.engine).reset_counts();
            }
            on_connected(conn, &info);
        }
        ClientEvent::Published { token, result } => {
            let id = lock(&conn.tokens).remove(&token);
            if let Some(id) = id {
                conn.update_sent(&id, |e| match result {
                    Ok(s) => {
                        if e.status == "pending" {
                            e.status = match s {
                                mqttpanel_client::PubStatus::Sent => "sent".into(),
                                mqttpanel_client::PubStatus::Acked => "acked".into(),
                            };
                        }
                    }
                    Err(m) => {
                        e.status = "failed".into();
                        e.reason = Some(m);
                    }
                });
            }
        }
        ClientEvent::Subscribed { filter, result } => {
            let mut link = lock(&conn.link);
            link.sub_errors.retain(|s| s.filter != filter);
            if let Err(error) = result {
                link.sub_errors.push(SubscriptionError { filter, error });
            }
        }
        ClientEvent::Lost(m) => {
            {
                let mut link = lock(&conn.link);
                link.state = "reconnecting".into();
                link.error = Some(m);
            }
            halt_repeat(conn, Some(msg!("repeat.stoppedDisconnect")));
        }
        ClientEvent::Reconnecting { attempt } => {
            let mut link = lock(&conn.link);
            link.state = "reconnecting".into();
            link.attempt = attempt;
            if link.error.is_none() || attempt > 1 {
                link.error = Some(msg!("error.reconnecting", attempt = attempt));
            }
        }
        ClientEvent::Failed(f) => {
            let mut link = lock(&conn.link);
            link.state = "failed".into();
            link.error = Some(f.message());
            drop(link);
            *lock(&conn.handle) = None;
            halt_repeat(conn, Some(msg!("repeat.stoppedDisconnect")));
        }
        ClientEvent::Closed => {
            let mut link = lock(&conn.link);
            link.state = "disconnected".into();
            drop(link);
            *lock(&conn.handle) = None;
            halt_repeat(conn, Some(msg!("repeat.stoppedDisconnect")));
        }
    }
    // Requests that timed out.
    let expired: Vec<String> = {
        let mut reqs = lock(&conn.requests);
        let ids: Vec<String> = reqs.iter().filter(|(_, r)| r.deadline < now).map(|(k, _)| k.clone()).collect();
        ids.into_iter().filter_map(|k| reqs.remove(&k).map(|r| r.sent_id)).collect()
    };
    for id in expired {
        conn.update_sent(&id, |e| e.reply_timed_out = true);
    }
}

pub fn halt_repeat(conn: &Conn, reason: Option<UserMessage>) {
    let mut rep = lock(&conn.repeat);
    if let Some(r) = rep.as_mut() {
        if r.status.running {
            let _ = r.cancel.send(true);
            r.status.running = false;
            r.status.next_at = None;
            r.status.stopped_reason = reason;
        }
    }
}

/// Sweeps request timeouts (called from the ticker).
pub fn sweep_requests(conn: &Conn, now: i64) {
    let expired: Vec<String> = {
        let mut reqs = lock(&conn.requests);
        let ids: Vec<String> = reqs.iter().filter(|(_, r)| r.deadline < now).map(|(k, _)| k.clone()).collect();
        ids.into_iter().filter_map(|k| reqs.remove(&k).map(|r| r.sent_id)).collect()
    };
    for id in expired {
        conn.update_sent(&id, |e| e.reply_timed_out = true);
    }
}
