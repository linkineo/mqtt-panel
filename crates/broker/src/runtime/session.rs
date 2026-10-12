//! One client connection: CONNECT and authentication, then the MQTT session loop.

use super::auth::{authenticate, AuthOutcome};
use super::codec::{
    self, Ack, ConnAckCode, ConnAckExtra, ConnectInfo, DecodeError, Incoming, Version, Will,
};
use super::log::now_ms;
use super::router::{parse_shared, topic_matches, valid_filter, valid_topic};
use super::state::{ClientInfo, ConnRef, Delivery, Outbound, Shared};
use super::tls::PeerCert;
use crate::config::ListenerKind;
use bytes::BytesMut;
use std::collections::{BTreeMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

/// Time a client has to send CONNECT after the transport is up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Outgoing QoS 1/2 messages in flight per client, at most.
const MAX_INFLIGHT: usize = 1024;

/// The listener a connection arrived on.
#[derive(Debug, Clone)]
pub(crate) struct ListenerCtx {
    pub id: String,
    pub kind: ListenerKind,
    pub require_client_cert: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum Stage {
    AwaitAck,
    AwaitRec,
    AwaitComp,
}

struct OutEntry {
    delivery: Delivery,
    stage: Stage,
}

/// Why the session loop ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Close {
    /// DISCONNECT from the client; `publish_will` for MQTT 5 reason 0x04.
    Client {
        publish_will: bool,
    },
    Eof,
    Error(String),
    KeepAlive,
    Protocol(String),
    /// Closed by the broker with this reason code (takeover, administrative action).
    Kicked(u8),
    Shutdown,
}

impl Close {
    fn text(&self) -> String {
        match self {
            Close::Client { .. } => "disconnected".into(),
            Close::Eof => "connection closed by the client".into(),
            Close::Error(e) => format!("connection lost ({e})"),
            Close::KeepAlive => "keep-alive timeout".into(),
            Close::Protocol(e) => format!("protocol error ({e})"),
            Close::Kicked(0x8E) => "session taken over by another connection".into(),
            Close::Kicked(_) => "disconnected by the broker".into(),
            Close::Shutdown => "broker stopping".into(),
        }
    }
    fn publish_will(&self) -> bool {
        match self {
            Close::Client { publish_will } => *publish_will,
            Close::Kicked(0x8E) | Close::Shutdown => false,
            _ => true,
        }
    }
}

fn display_addr(peer: &SocketAddr) -> String {
    if peer.ip().is_loopback() {
        "localhost".into()
    } else {
        match peer.ip() {
            std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => format!("{v4}:{}", peer.port()),
                None => peer.to_string(),
            },
            _ => peer.to_string(),
        }
    }
}

async fn write_out<W: AsyncWrite + Unpin>(w: &mut W, out: &mut BytesMut) -> std::io::Result<()> {
    if !out.is_empty() {
        w.write_all(out).await?;
        out.clear();
    }
    w.flush().await
}

/// Serves one connection until it closes. Never panics on network input.
pub(crate) async fn serve<S>(
    shared: Arc<Shared>,
    lctx: Arc<ListenerCtx>,
    stream: S,
    peer: SocketAddr,
    cert: Option<PeerCert>,
    mut shutdown: watch::Receiver<bool>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let (mut rd, mut wr) = tokio::io::split(stream);
    let mut inbuf = BytesMut::with_capacity(4096);
    let mut out = BytesMut::with_capacity(4096);
    let max = shared.max_packet;

    // 1. CONNECT
    let connect = tokio::time::timeout(CONNECT_TIMEOUT, async {
        loop {
            match codec::decode_connect(&mut inbuf, max) {
                Ok(Some(c)) => return Ok(c),
                Ok(None) => {}
                Err(e) => return Err(e),
            }
            match rd.read_buf(&mut inbuf).await {
                Ok(0) | Err(_) => {
                    return Err(DecodeError::Malformed("closed before CONNECT".into()))
                }
                Ok(_) => {}
            }
        }
    })
    .await;
    let mut c: ConnectInfo = match connect {
        Ok(Ok(c)) => c,
        Ok(Err(DecodeError::UnsupportedProtocol(level))) => {
            shared.log.info(format!(
                "client from {} rejected: unsupported MQTT protocol level {level}",
                display_addr(&peer)
            ));
            codec::encode_connack(
                Version::V4,
                &mut out,
                false,
                ConnAckCode::UnsupportedProtocol,
                ConnAckExtra::default(),
            );
            let _ = write_out(&mut wr, &mut out).await;
            return;
        }
        Ok(Err(e)) => {
            shared.log.debug(format!(
                "connection from {} closed: {e}",
                display_addr(&peer)
            ));
            return;
        }
        Err(_) => {
            shared.log.debug(format!(
                "connection from {} closed: no CONNECT within 10 s",
                display_addr(&peer)
            ));
            return;
        }
    };
    let v = c.version;

    // 2. Client id
    let mut assigned = None;
    if c.client_id.is_empty() {
        if v == Version::V4 && !c.clean_start {
            codec::encode_connack(
                v,
                &mut out,
                false,
                ConnAckCode::BadClientId,
                ConnAckExtra::default(),
            );
            let _ = write_out(&mut wr, &mut out).await;
            shared.log.info(format!(
                "client from {} rejected: empty client ID with a persistent session",
                display_addr(&peer)
            ));
            return;
        }
        let id = format!("auto-{:016x}", rand::random::<u64>());
        c.client_id = id.clone();
        if v == Version::V5 {
            assigned = Some(id);
        }
    }
    let client_id = c.client_id.clone();

    // 3. Capacity
    let connected_now = shared.lock().connected;
    if connected_now >= shared.max_clients {
        codec::encode_connack(
            v,
            &mut out,
            false,
            ConnAckCode::QuotaExceeded,
            ConnAckExtra::default(),
        );
        let _ = write_out(&mut wr, &mut out).await;
        shared.log.warn(format!(
            "client {client_id} rejected: maximum of {} clients reached",
            shared.max_clients
        ));
        return;
    }

    // 4. Authentication
    let auth = shared.auth();
    let (lid, req, verified) = (lctx.id.clone(), lctx.require_client_cert, cert.is_some());
    let (user, pass) = (c.username.clone(), c.password.clone());
    let outcome = tokio::task::spawn_blocking(move || {
        authenticate(&auth, &lid, req, verified, user.as_deref(), pass.as_deref())
    })
    .await
    .unwrap_or(AuthOutcome::BadLogin);
    if outcome == AuthOutcome::BadLogin {
        codec::encode_connack(
            v,
            &mut out,
            false,
            ConnAckCode::BadLogin,
            ConnAckExtra::default(),
        );
        let _ = write_out(&mut wr, &mut out).await;
        shared.log.warn(format!(
            "client {client_id} rejected: bad username or password"
        ));
        return;
    }

    // 5. Register the session
    let conn_id = shared.next_conn_id.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::unbounded_channel::<Outbound>();
    let queued = Arc::new(AtomicUsize::new(0));
    let info = Arc::new(ClientInfo {
        client_id: client_id.clone(),
        address: display_addr(&peer),
        version: v,
        transport: lctx.kind.label(),
        connected_at: now_ms(),
        cert: cert.clone(),
        username: c.username.clone(),
        msg_in: AtomicU64::new(0),
        msg_out: AtomicU64::new(0),
        subscriptions: AtomicU32::new(0),
        dropped: AtomicU64::new(0),
        last_drop_log: Mutex::new(None),
    });
    let persistent = match v {
        Version::V4 => !c.clean_start,
        Version::V5 => c.session_expiry > 0,
    };
    let session_expiry = match v {
        Version::V4 => u32::MAX,
        Version::V5 => c.session_expiry,
    };
    let session_present;
    {
        let mut st = shared.lock();
        if let Some(old) = st.sessions.get(&client_id).and_then(|s| s.conn.clone()) {
            let _ = old.tx.send(Outbound::Disconnect(0x8E));
            shared
                .log
                .info(format!("client {client_id} taken over by a new connection"));
        }
        if c.clean_start {
            Shared::drop_session(&mut st, &client_id);
        }
        session_present = st.sessions.contains_key(&client_id);
        let sess = st.sessions.entry(client_id.clone()).or_default();
        sess.persistent = persistent;
        sess.expiry = session_expiry;
        sess.expires_at = None;
        let cref = ConnRef {
            conn_id,
            tx: tx.clone(),
            queued: queued.clone(),
            info: info.clone(),
        };
        sess.conn = Some(cref);
        info.subscriptions
            .store(sess.subs.len() as u32, Ordering::Relaxed);
        let offline: Vec<Delivery> = sess.offline.drain(..).collect();
        for d in offline {
            queued.fetch_add(1, Ordering::Relaxed);
            let _ = tx.send(Outbound::Publish(d));
        }
        st.connected += 1;
        if let Some(pc) = &cert {
            st.cert_seen.entry(pc.serial.clone()).or_default().connected += 1;
        }
    }
    drop(tx);

    let extra = ConnAckExtra {
        assigned_client_id: assigned,
        max_packet_size: Some(max as u32),
        receive_max: Some(65535),
    };
    codec::encode_connack(v, &mut out, session_present, ConnAckCode::Success, extra);
    let mut desc = format!(
        "client {client_id} connected from {} ({}, MQTT {}",
        info.address,
        lctx.kind.label(),
        v.label()
    );
    if let Some(u) = &c.username {
        desc.push_str(&format!(", user {u}"));
    }
    if let Some(cn) = cert.as_ref().and_then(|p| p.cn.as_ref()) {
        desc.push_str(&format!(", certificate {cn}"));
    }
    desc.push(')');
    shared.log.info(desc);

    // 6. Session loop
    let keep_alive =
        (c.keep_alive > 0).then(|| Duration::from_millis(u64::from(c.keep_alive) * 1500));
    let max_out_size = c.max_packet_size;
    let max_inflight = (c.receive_max as usize).clamp(1, MAX_INFLIGHT);
    let mut session_expiry_now = session_expiry;
    let mut deadline =
        tokio::time::Instant::now() + keep_alive.unwrap_or(Duration::from_secs(86_400));
    let mut inflight: BTreeMap<u16, OutEntry> = BTreeMap::new();
    let mut next_pkid: u16 = 0;
    let mut qos2_in: HashSet<u16> = HashSet::new();
    let mut will: Option<Will> = c.will.take();

    let close = 'session: loop {
        if let Err(e) = write_out(&mut wr, &mut out).await {
            break 'session Close::Error(crate::error::short_detail(&e.to_string()));
        }
        tokio::select! {
            biased;
            _ = shutdown.changed() => {
                codec::encode_disconnect(v, &mut out, 0x8B);
                break 'session Close::Shutdown;
            }
            r = rd.read_buf(&mut inbuf) => {
                match r {
                    Ok(0) => break 'session Close::Eof,
                    Err(e) => break 'session Close::Error(crate::error::short_detail(&e.to_string())),
                    Ok(_) => {}
                }
                if let Some(k) = keep_alive {
                    deadline = tokio::time::Instant::now() + k;
                }
                loop {
                    let p = match codec::decode(v, &mut inbuf, max) {
                        Ok(Some(p)) => p,
                        Ok(None) => break,
                        Err(DecodeError::TooLarge) => {
                            codec::encode_disconnect(v, &mut out, 0x95);
                            break 'session Close::Protocol("packet too large".into());
                        }
                        Err(e) => {
                            codec::encode_disconnect(v, &mut out, 0x81);
                            break 'session Close::Protocol(e.to_string());
                        }
                    };
                    match p {
                        Incoming::Publish { mut msg, pkid, topic_alias } => {
                            if topic_alias.is_some() || !valid_topic(&msg.topic) {
                                codec::encode_disconnect(v, &mut out, if topic_alias.is_some() { 0x94 } else { 0x90 });
                                break 'session Close::Protocol(format!("invalid topic {:?}", msg.topic));
                            }
                            info.msg_in.fetch_add(1, Ordering::Relaxed);
                            shared.stats.msg_in();
                            msg.publisher = Some(client_id.clone());
                            match msg.qos {
                                0 => shared.route(msg),
                                1 => {
                                    shared.route(msg);
                                    codec::encode_ack(v, &mut out, Ack::PubAck, pkid);
                                }
                                _ => {
                                    if qos2_in.insert(pkid) {
                                        shared.route(msg);
                                    }
                                    codec::encode_ack(v, &mut out, Ack::PubRec, pkid);
                                }
                            }
                        }
                        Incoming::PubRel(pkid) => {
                            qos2_in.remove(&pkid);
                            codec::encode_ack(v, &mut out, Ack::PubComp, pkid);
                        }
                        Incoming::PubAck(pkid) => {
                            if inflight.get(&pkid).is_some_and(|e| e.stage == Stage::AwaitAck) {
                                inflight.remove(&pkid);
                            }
                        }
                        Incoming::PubRec(pkid) => {
                            if let Some(e) = inflight.get_mut(&pkid) {
                                e.stage = Stage::AwaitComp;
                            }
                            codec::encode_ack(v, &mut out, Ack::PubRel, pkid);
                        }
                        Incoming::PubComp(pkid) => {
                            inflight.remove(&pkid);
                        }
                        Incoming::Subscribe { pkid, filters } => {
                            let mut codes = Vec::with_capacity(filters.len());
                            let mut retained_out: Vec<Delivery> = Vec::new();
                            let mut names = Vec::new();
                            {
                                let mut st = shared.lock();
                                for (filter, mut opts) in filters {
                                    let shared_sub = parse_shared(&filter);
                                    let plain = shared_sub.map(|(_, f)| f).unwrap_or(filter.as_str());
                                    let bad_share = filter.starts_with("$share/") && shared_sub.is_none();
                                    if bad_share || !valid_filter(plain) {
                                        codes.push(None);
                                        continue;
                                    }
                                    opts.qos = opts.qos.min(2);
                                    let existed = st
                                        .sessions
                                        .get_mut(&client_id)
                                        .map(|s| s.subs.insert(filter.clone(), opts).is_some())
                                        .unwrap_or(false);
                                    st.router.subscribe(&client_id, &filter, opts);
                                    codes.push(Some(opts.qos));
                                    names.push(filter.clone());
                                    let send_retained = shared_sub.is_none()
                                        && (opts.retain_handling == 0 || (opts.retain_handling == 1 && !existed));
                                    if send_retained {
                                        for m in st.retained.values() {
                                            if topic_matches(&filter, &m.topic) && !m.expired() {
                                                retained_out.push(Delivery {
                                                    msg: m.clone(),
                                                    qos: opts.qos.min(m.qos),
                                                    retain: true,
                                                    sub_ids: opts.sub_id.into_iter().collect(),
                                                    dup: false,
                                                });
                                            }
                                        }
                                    }
                                }
                                if let Some(s) = st.sessions.get(&client_id) {
                                    info.subscriptions.store(s.subs.len() as u32, Ordering::Relaxed);
                                }
                                if let Some(s) = st.sessions.get_mut(&client_id) {
                                    for d in retained_out {
                                        shared.deliver(s, d);
                                    }
                                }
                            }
                            codec::encode_suback(v, &mut out, pkid, &codes);
                            if !names.is_empty() {
                                shared.log.info(format!("client {client_id} subscribed to {}", names.join(", ")));
                            }
                        }
                        Incoming::Unsubscribe { pkid, filters } => {
                            let mut existed = Vec::with_capacity(filters.len());
                            {
                                let mut st = shared.lock();
                                for f in &filters {
                                    let had = st.sessions.get_mut(&client_id).map(|s| s.subs.remove(f).is_some()).unwrap_or(false);
                                    st.router.unsubscribe(&client_id, f);
                                    existed.push(had);
                                }
                                if let Some(s) = st.sessions.get(&client_id) {
                                    info.subscriptions.store(s.subs.len() as u32, Ordering::Relaxed);
                                }
                            }
                            codec::encode_unsuback(v, &mut out, pkid, &existed);
                            shared.log.debug(format!("client {client_id} unsubscribed from {}", filters.join(", ")));
                        }
                        Incoming::PingReq => codec::encode_pingresp(v, &mut out),
                        Incoming::Disconnect { reason, session_expiry } => {
                            if let Some(se) = session_expiry {
                                session_expiry_now = se;
                            }
                            break 'session Close::Client { publish_will: reason == 0x04 };
                        }
                        Incoming::Unexpected => {
                            codec::encode_disconnect(v, &mut out, 0x82);
                            break 'session Close::Protocol("unexpected packet".into());
                        }
                    }
                }
            }
            cmd = rx.recv(), if inflight.len() < max_inflight => {
                let Some(mut cmd) = cmd else { break 'session Close::Kicked(0x8E) };
                let mut budget = 256;
                loop {
                    match cmd {
                        Outbound::Disconnect(reason) => {
                            codec::encode_disconnect(v, &mut out, reason);
                            break 'session Close::Kicked(reason);
                        }
                        Outbound::Publish(d) => {
                            queued.fetch_sub(1, Ordering::Relaxed);
                            if !d.msg.expired() {
                                send_publish(&shared, &info, v, &mut out, d, max_out_size, &mut inflight, &mut next_pkid);
                            }
                        }
                    }
                    budget -= 1;
                    if budget == 0 || inflight.len() >= max_inflight || out.len() > 256 * 1024 {
                        break;
                    }
                    match rx.try_recv() {
                        Ok(next) => cmd = next,
                        Err(_) => break,
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline), if keep_alive.is_some() => {
                codec::encode_disconnect(v, &mut out, 0x8D);
                break 'session Close::KeepAlive;
            }
        }
    };
    let _ = write_out(&mut wr, &mut out).await;
    let _ = wr.shutdown().await;

    // 7. Clean up
    let publish_will = close.publish_will();
    {
        let mut st = shared.lock();
        st.connected = st.connected.saturating_sub(1);
        if let Some(pc) = &cert {
            let e = st.cert_seen.entry(pc.serial.clone()).or_default();
            e.connected = e.connected.saturating_sub(1);
            e.last_seen = Some(now_ms());
        }
        let mine = st
            .sessions
            .get(&client_id)
            .and_then(|s| s.conn.as_ref())
            .is_some_and(|c| c.conn_id == conn_id);
        if mine {
            let keep = persistent && session_expiry_now > 0;
            if keep {
                let mut pending: Vec<Delivery> = inflight
                    .into_values()
                    .filter(|e| e.stage != Stage::AwaitComp)
                    .map(|e| Delivery {
                        dup: true,
                        ..e.delivery
                    })
                    .collect();
                while let Ok(cmd) = rx.try_recv() {
                    if let Outbound::Publish(d) = cmd {
                        if d.qos > 0 {
                            pending.push(d);
                        }
                    }
                }
                if let Some(s) = st.sessions.get_mut(&client_id) {
                    s.conn = None;
                    s.expiry = session_expiry_now;
                    s.expires_at = if session_expiry_now == u32::MAX {
                        None
                    } else {
                        Some(Instant::now() + Duration::from_secs(u64::from(session_expiry_now)))
                    };
                    for d in pending.into_iter().rev() {
                        s.offline.push_front(d);
                    }
                    while s.offline.len() > super::state::OFFLINE_LIMIT {
                        s.offline.pop_back();
                    }
                }
            } else {
                Shared::drop_session(&mut st, &client_id);
            }
        }
    }
    let reason = close.text();
    match &close {
        Close::Client { .. } | Close::Shutdown => {
            shared.log.info(format!("client {client_id} disconnected"))
        }
        _ => shared
            .log
            .info(format!("client {client_id} disconnected: {reason}")),
    }
    if publish_will {
        if let Some(w) = will.take() {
            publish_will_message(shared, client_id, w);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn send_publish(
    shared: &Shared,
    info: &ClientInfo,
    v: Version,
    out: &mut BytesMut,
    d: Delivery,
    max_out_size: Option<u32>,
    inflight: &mut BTreeMap<u16, OutEntry>,
    next_pkid: &mut u16,
) {
    let pkid = if d.qos > 0 {
        let mut tries = 0u32;
        loop {
            *next_pkid = next_pkid.wrapping_add(1);
            if *next_pkid == 0 {
                *next_pkid = 1;
            }
            if !inflight.contains_key(next_pkid) {
                break *next_pkid;
            }
            tries += 1;
            if tries > 65_535 {
                return;
            }
        }
    } else {
        0
    };
    if !codec::encode_publish(
        v,
        out,
        &d.msg,
        d.qos,
        d.retain,
        pkid,
        d.dup,
        &d.sub_ids,
        max_out_size,
    ) {
        shared.log.debug(format!(
            "message on {} skipped for {}: larger than the client accepts",
            d.msg.topic, info.client_id
        ));
        return;
    }
    info.msg_out.fetch_add(1, Ordering::Relaxed);
    shared.stats.msg_out(1);
    if d.qos > 0 {
        let stage = if d.qos == 1 {
            Stage::AwaitAck
        } else {
            Stage::AwaitRec
        };
        inflight.insert(pkid, OutEntry { delivery: d, stage });
    }
}

fn publish_will_message(shared: Arc<Shared>, client_id: String, w: Will) {
    let mut msg = w.message;
    msg.created = Instant::now();
    if w.delay == 0 {
        shared.log.info(format!(
            "last will of {client_id} published on {}",
            msg.topic
        ));
        shared.route(msg);
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(u64::from(w.delay))).await;
        let reconnected = shared
            .lock()
            .sessions
            .get(&client_id)
            .is_some_and(|s| s.conn.is_some());
        if !reconnected {
            shared.log.info(format!(
                "last will of {client_id} published on {}",
                msg.topic
            ));
            shared.route(msg);
        }
    });
}
