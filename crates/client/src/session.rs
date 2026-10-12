//! MQTT session over a [`Conn`]: CONNECT/CONNACK with 3.1.1/5 negotiation, QoS 0/1/2 in both directions,
//! subscriptions, keep-alive, and a managed connection that reconnects and restores subscriptions.

use crate::catalog::{human_size, UserMessage};
use crate::msg;
use crate::options::{ConnectOptions, Version, VersionPref};
use crate::transport::{self, Conn, ConnectFailure, TransportInfo};
use bytes::{Bytes, BytesMut};
use rumqttc::mqttbytes::v4;
use rumqttc::mqttbytes::QoS;
use rumqttc::v5::mqttbytes::v5;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

const MAX_INCOMING: usize = 268_435_455;

/// MQTT 5 message properties (also used for outgoing messages).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Props5 {
    pub content_type: Option<String>,
    pub payload_format: Option<u8>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub message_expiry: Option<u32>,
    pub user_properties: Vec<(String, String)>,
    pub subscription_ids: Vec<usize>,
    pub topic_alias: Option<u16>,
}

/// A received message.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub topic: String,
    pub payload: Bytes,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    pub props: Option<Props5>,
}

/// A message to publish.
#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub props: Option<Props5>,
}

/// What the broker said in CONNACK and what the transport learned.
#[derive(Debug, Clone)]
pub struct ConnInfo {
    pub version: Version,
    pub max_packet_size: Option<u32>,
    pub session_present: bool,
    pub assigned_client_id: Option<String>,
    pub server_keep_alive: Option<u16>,
    pub latency_ms: u64,
    pub transport: TransportInfo,
}

/// Outcome of a publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PubStatus {
    /// QoS 0: written to the socket.
    Sent,
    /// QoS 1 PUBACK or QoS 2 PUBCOMP received.
    Acked,
}

/// Events from a managed connection.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    Connected(Box<ConnInfo>),
    Message(Incoming),
    Published { token: u64, result: Result<PubStatus, UserMessage> },
    Subscribed { filter: String, result: Result<u8, UserMessage> },
    /// The connection was lost; a reconnect follows when enabled.
    Lost(UserMessage),
    Reconnecting { attempt: u32 },
    /// Reconnecting stopped (permanent failure).
    Failed(ConnectFailure),
    /// Closed by the user.
    Closed,
}

enum Command {
    Publish { token: u64, msg: Outgoing },
    Subscribe(Vec<(String, u8)>),
    Unsubscribe(Vec<String>),
    Disconnect(oneshot::Sender<()>),
}

/// Handle to a managed connection.
#[derive(Clone)]
pub struct ClientHandle {
    tx: mpsc::UnboundedSender<Command>,
    tokens: Arc<AtomicU64>,
}

impl ClientHandle {
    /// Queues a publish; the result arrives as [`ClientEvent::Published`] with the returned token.
    pub fn publish(&self, msg: Outgoing) -> u64 {
        let token = self.tokens.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(Command::Publish { token, msg });
        token
    }
    pub fn subscribe(&self, filters: Vec<(String, u8)>) {
        let _ = self.tx.send(Command::Subscribe(filters));
    }
    pub fn unsubscribe(&self, filters: Vec<String>) {
        let _ = self.tx.send(Command::Unsubscribe(filters));
    }
    /// Sends DISCONNECT and closes; waits up to two seconds.
    pub async fn disconnect(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Command::Disconnect(tx)).is_ok() {
            let _ = tokio::time::timeout(Duration::from_secs(2), rx).await;
        }
    }
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

fn qos_of(q: u8) -> QoS {
    match q {
        0 => QoS::AtMostOnce,
        1 => QoS::AtLeastOnce,
        _ => QoS::ExactlyOnce,
    }
}

fn qos_u8(q: QoS) -> u8 {
    q as u8
}

fn qos5_of(q: u8) -> rumqttc::v5::mqttbytes::QoS {
    use rumqttc::v5::mqttbytes::QoS as Q;
    match q {
        0 => Q::AtMostOnce,
        1 => Q::AtLeastOnce,
        _ => Q::ExactlyOnce,
    }
}

fn qos5_u8(q: rumqttc::v5::mqttbytes::QoS) -> u8 {
    q as u8
}

#[allow(clippy::large_enum_variant)]
enum Pkt {
    V4(v4::Packet),
    V5(v5::Packet),
}

/// Length of the complete frame at the start of `buf`, or None when more bytes are needed.
fn frame_len(buf: &[u8]) -> Result<Option<usize>, String> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let mut len: usize = 0;
    let mut mult: usize = 1;
    for i in 1..5 {
        let Some(&b) = buf.get(i) else { return Ok(None) };
        len += (b & 0x7f) as usize * mult;
        if b & 0x80 == 0 {
            let total = 1 + i + len;
            return Ok(if buf.len() >= total { Some(total) } else { None });
        }
        mult *= 128;
    }
    Err("malformed remaining length".into())
}

/// Parses a v5 DISCONNECT frame by hand (the codec mishandles short ones).
fn read_disconnect_v5(frame: &[u8]) -> v5::DisconnectReasonCode {
    let hdr = frame.iter().skip(1).position(|b| b & 0x80 == 0).map(|p| p + 2).unwrap_or(2);
    let code = frame.get(hdr).copied().unwrap_or(0);
    disconnect_code(code)
}

fn disconnect_code(c: u8) -> v5::DisconnectReasonCode {
    use v5::DisconnectReasonCode as D;
    match c {
        0x00 => D::NormalDisconnection,
        0x04 => D::DisconnectWithWillMessage,
        0x81 => D::MalformedPacket,
        0x82 => D::ProtocolError,
        0x83 => D::ImplementationSpecificError,
        0x87 => D::NotAuthorized,
        0x89 => D::ServerBusy,
        0x8B => D::ServerShuttingDown,
        0x8D => D::KeepAliveTimeout,
        0x8E => D::SessionTakenOver,
        0x8F => D::TopicFilterInvalid,
        0x90 => D::TopicNameInvalid,
        0x93 => D::ReceiveMaximumExceeded,
        0x94 => D::TopicAliasInvalid,
        0x95 => D::PacketTooLarge,
        0x96 => D::MessageRateTooHigh,
        0x97 => D::QuotaExceeded,
        0x98 => D::AdministrativeAction,
        0x99 => D::PayloadFormatInvalid,
        _ => D::UnspecifiedError,
    }
}

fn read_packet(version: Version, buf: &mut BytesMut) -> Result<Option<Pkt>, String> {
    let Some(total) = frame_len(buf)? else { return Ok(None) };
    if version == Version::V5 && buf[0] & 0xF0 == 0xE0 {
        let frame = buf.split_to(total);
        return Ok(Some(Pkt::V5(v5::Packet::Disconnect(v5::Disconnect::new(read_disconnect_v5(&frame))))));
    }
    match version {
        Version::V311 => v4::Packet::read(buf, MAX_INCOMING).map(|p| Some(Pkt::V4(p))).map_err(|e| e.to_string()),
        Version::V5 => v5::Packet::read(buf, Some(MAX_INCOMING as u32)).map(|p| Some(Pkt::V5(p))).map_err(|e| e.to_string()),
    }
}

/// A well-formed v5 DISCONNECT (the codec's writer emits a wrong length).
fn disconnect_bytes(version: Version) -> Vec<u8> {
    match version {
        Version::V311 => vec![0xE0, 0x00],
        Version::V5 => vec![0xE0, 0x00],
    }
}

fn reason_param_v5(code: v5::DisconnectReasonCode) -> String {
    use v5::DisconnectReasonCode as D;
    match code {
        D::SessionTakenOver => "@reason.sessionTakenOver".into(),
        D::ServerShuttingDown => "@reason.serverShuttingDown".into(),
        D::KeepAliveTimeout => "@reason.keepAliveTimeout".into(),
        D::NotAuthorized => "@reason.notAuthorized".into(),
        D::QuotaExceeded => "@reason.quotaExceeded".into(),
        D::TopicNameInvalid => "@reason.topicInvalid".into(),
        D::PacketTooLarge => "@reason.packetTooLarge".into(),
        D::AdministrativeAction => "@reason.administrativeAction".into(),
        D::ImplementationSpecificError => "@reason.implementationSpecific".into(),
        D::UnspecifiedError => "@reason.unspecified".into(),
        other => format!("0x{:02X}", other as u8),
    }
}

fn connack_v4(code: v4::ConnectReturnCode, opts: &ConnectOptions) -> Option<UserMessage> {
    use v4::ConnectReturnCode as C;
    let user = opts.username.clone().unwrap_or_default();
    match code {
        C::Success => None,
        C::RefusedProtocolVersion => Some(msg!("error.protocolVersion")),
        C::BadClientId => Some(msg!("error.clientIdRejected", clientId = opts.client_id, code = "0x02")),
        C::ServiceUnavailable => Some(msg!("error.serverUnavailable", code = "0x03")),
        C::BadUserNamePassword if user.is_empty() => Some(msg!("error.badLoginAnonymous", code = "0x04")),
        C::BadUserNamePassword => Some(msg!("error.badLogin", user = user, code = "0x04")),
        C::NotAuthorized if user.is_empty() && opts.password.is_none() => Some(msg!("error.notAuthorized", code = "0x05")),
        C::NotAuthorized => Some(msg!("error.badLogin", user = user, code = "0x05")),
    }
}

fn connack_v5_code(code: v5::ConnectReturnCode) -> u8 {
    use v5::ConnectReturnCode as C;
    match code {
        C::Success => 0x00,
        C::RefusedProtocolVersion => 0x01,
        C::BadClientId => 0x02,
        C::ServiceUnavailable => 0x03,
        C::UnspecifiedError => 0x80,
        C::MalformedPacket => 0x81,
        C::ProtocolError => 0x82,
        C::ImplementationSpecificError => 0x83,
        C::UnsupportedProtocolVersion => 0x84,
        C::ClientIdentifierNotValid => 0x85,
        C::BadUserNamePassword => 0x86,
        C::NotAuthorized => 0x87,
        C::ServerUnavailable => 0x88,
        C::ServerBusy => 0x89,
        C::Banned => 0x8A,
        C::BadAuthenticationMethod => 0x8C,
        C::TopicNameInvalid => 0x90,
        C::PacketTooLarge => 0x95,
        C::QuotaExceeded => 0x97,
        C::PayloadFormatInvalid => 0x99,
        C::RetainNotSupported => 0x9A,
        C::QoSNotSupported => 0x9B,
        C::UseAnotherServer => 0x9C,
        C::ServerMoved => 0x9D,
        C::ConnectionRateExceeded => 0x9F,
    }
}

fn connack_v5(code: v5::ConnectReturnCode, reason: Option<String>, opts: &ConnectOptions) -> Option<UserMessage> {
    let c = connack_v5_code(code);
    let hex = format!("0x{c:02X}");
    let user = opts.username.clone().unwrap_or_default();
    match c {
        0x00 => None,
        0x01 | 0x84 => Some(msg!("error.protocolVersion")),
        0x02 | 0x85 => Some(msg!("error.clientIdRejected", clientId = opts.client_id, code = hex)),
        0x03 | 0x88 | 0x89 => Some(msg!("error.serverUnavailable", code = hex)),
        0x86 if user.is_empty() => Some(msg!("error.badLoginAnonymous", code = hex)),
        0x86 => Some(msg!("error.badLogin", user = user, code = hex)),
        0x87 | 0x8A => Some(msg!("error.notAuthorized", code = hex)),
        0x95 => Some(msg!("error.connackOther", reason = "@reason.packetTooLarge", code = hex)),
        0x97 => Some(msg!("error.connackOther", reason = "@reason.quotaExceeded", code = hex)),
        _ => Some(msg!("error.connackOther", reason = reason.unwrap_or_else(|| "@reason.unspecified".into()), code = hex)),
    }
}

fn props_out(p: &Props5) -> v5::PublishProperties {
    v5::PublishProperties {
        payload_format_indicator: p.payload_format,
        message_expiry_interval: p.message_expiry,
        topic_alias: None,
        response_topic: p.response_topic.clone().filter(|s| !s.is_empty()),
        correlation_data: p.correlation_data.clone().filter(|d| !d.is_empty()).map(Bytes::from),
        user_properties: p.user_properties.clone(),
        subscription_identifiers: vec![],
        content_type: p.content_type.clone().filter(|s| !s.is_empty()),
    }
}

fn props_in(p: &v5::PublishProperties) -> Props5 {
    Props5 {
        content_type: p.content_type.clone(),
        payload_format: p.payload_format_indicator,
        response_topic: p.response_topic.clone(),
        correlation_data: p.correlation_data.as_ref().map(|b| b.to_vec()),
        message_expiry: p.message_expiry_interval,
        user_properties: p.user_properties.clone(),
        subscription_ids: p.subscription_identifiers.clone(),
        topic_alias: p.topic_alias,
    }
}

/// Writes one packet.
async fn send(conn: &mut Conn, version: Version, pkt: Pkt, wbuf: &mut BytesMut) -> std::io::Result<()> {
    wbuf.clear();
    let r = match (version, pkt) {
        (_, Pkt::V4(p)) => p.write(wbuf, MAX_INCOMING).map(|_| ()).map_err(|e| e.to_string()),
        (_, Pkt::V5(p)) => p.write(wbuf, None).map(|_| ()).map_err(|e| e.to_string()),
    };
    r.map_err(std::io::Error::other)?;
    conn.write_all(wbuf).await
}

/// Opens the transport and performs CONNECT/CONNACK with one protocol version.
async fn connect_once(opts: &ConnectOptions, version: Version) -> Result<(Conn, BytesMut, ConnInfo), ConnectFailure> {
    let started = Instant::now();
    let (mut conn, tinfo) = transport::connect(opts).await?;
    let mut wbuf = BytesMut::with_capacity(512);
    let keep_alive = opts.keep_alive;
    let pkt = match version {
        Version::V311 => {
            let mut c = v4::Connect::new(opts.client_id.clone());
            c.keep_alive = keep_alive;
            c.clean_session = opts.clean_start;
            if let Some(u) = &opts.username {
                c.login = Some(v4::Login::new(u.clone(), opts.password.clone().unwrap_or_default()));
            }
            c.last_will = opts.last_will.as_ref().map(|w| v4::LastWill::new(w.topic.clone(), w.payload.clone(), qos_of(w.qos), w.retain));
            Pkt::V4(v4::Packet::Connect(c))
        }
        Version::V5 => {
            let mut props = v5::ConnectProperties::new();
            if opts.session_expiry > 0 {
                props.session_expiry_interval = Some(opts.session_expiry);
            }
            let c = v5::Connect { keep_alive, client_id: opts.client_id.clone(), clean_start: opts.clean_start, properties: Some(props) };
            let will = opts.last_will.as_ref().map(|w| v5::LastWill::new(w.topic.clone(), w.payload.clone(), qos5_of(w.qos), w.retain, None));
            let login = opts.username.as_ref().map(|u| v5::Login::new(u.clone(), opts.password.clone().unwrap_or_default()));
            Pkt::V5(v5::Packet::Connect(c, will, login))
        }
    };
    if let Err(e) = send(&mut conn, version, pkt, &mut wbuf).await {
        return Err(write_error(&e, &conn, opts));
    }
    let mut buf = BytesMut::with_capacity(4096);
    let deadline = Instant::now() + opts.connect_timeout;
    loop {
        // A 3.1.1-only broker answers a v5 CONNECT with a v3 CONNACK (remaining length 2).
        if version == Version::V5 && buf.len() >= 4 && buf[0] == 0x20 && buf[1] == 0x02 {
            let code = buf[3];
            return Err(if code == 0x01 {
                msg!("error.protocolVersion").into()
            } else {
                let c = match code {
                    2 => v4::ConnectReturnCode::BadClientId,
                    3 => v4::ConnectReturnCode::ServiceUnavailable,
                    4 => v4::ConnectReturnCode::BadUserNamePassword,
                    _ => v4::ConnectReturnCode::NotAuthorized,
                };
                connack_v4(c, opts).unwrap_or_else(|| msg!("error.protocolVersion")).into()
            });
        }
        match read_packet(version, &mut buf) {
            Ok(Some(Pkt::V4(v4::Packet::ConnAck(ack)))) => {
                if let Some(m) = connack_v4(ack.code, opts) {
                    return Err(m.into());
                }
                let info = ConnInfo {
                    version,
                    max_packet_size: None,
                    session_present: ack.session_present,
                    assigned_client_id: None,
                    server_keep_alive: None,
                    latency_ms: started.elapsed().as_millis() as u64,
                    transport: tinfo,
                };
                return Ok((conn, buf, info));
            }
            Ok(Some(Pkt::V5(v5::Packet::ConnAck(ack)))) => {
                let reason = ack.properties.as_ref().and_then(|p| p.reason_string.clone());
                if let Some(m) = connack_v5(ack.code, reason, opts) {
                    return Err(m.into());
                }
                let p = ack.properties;
                let info = ConnInfo {
                    version,
                    max_packet_size: p.as_ref().and_then(|p| p.max_packet_size),
                    session_present: ack.session_present,
                    assigned_client_id: p.as_ref().and_then(|p| p.assigned_client_identifier.clone()),
                    server_keep_alive: p.as_ref().and_then(|p| p.server_keep_alive),
                    latency_ms: started.elapsed().as_millis() as u64,
                    transport: tinfo,
                };
                return Ok((conn, buf, info));
            }
            Ok(Some(Pkt::V5(v5::Packet::Disconnect(d)))) => {
                return Err(msg!("error.disconnectedBy", reason = reason_param_v5(d.reason_code)).into());
            }
            Ok(Some(_)) => return Err(msg!("error.protocol").into()),
            Ok(None) => {}
            Err(_) => return Err(closed_before_connack(&conn, opts, true).await),
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, conn.read_into(&mut buf)).await {
            Ok(Ok(0)) => return Err(closed_before_connack(&conn, opts, false).await),
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(read_error(&e, &conn, opts).await),
            Err(_) => return Err(msg!("error.timeout", host = opts.host, port = opts.port, seconds = opts.connect_timeout.as_secs()).into()),
        }
    }
}

fn write_error(e: &std::io::Error, conn: &Conn, opts: &ConnectOptions) -> ConnectFailure {
    if conn.is_tls() {
        if let Some(err) = transport::rustls_error(e) {
            let cap = Arc::new(std::sync::Mutex::new(crate::tlsconf::Captured::default()));
            return transport::classify_tls_error(&err, &cap, opts);
        }
    }
    transport::classify_plain_io(e, opts).into()
}

async fn read_error(e: &std::io::Error, conn: &Conn, opts: &ConnectOptions) -> ConnectFailure {
    if conn.is_tls() {
        if let Some(err) = transport::rustls_error(e) {
            // Client-certificate refusals arrive after the TLS 1.3 handshake, on the first read.
            let cap = Arc::new(std::sync::Mutex::new(crate::tlsconf::Captured::default()));
            return transport::classify_tls_error(&err, &cap, opts);
        }
    }
    closed_before_connack(conn, opts, false).await
}

/// Marker key for "closed before CONNACK" so auto-negotiation can retry with 3.1.1.
const CLOSED_KEY: &str = "error.closed";

async fn closed_before_connack(conn: &Conn, opts: &ConnectOptions, garbage: bool) -> ConnectFailure {
    if !conn.is_tls() && !opts.scheme.is_ws() && transport::port_speaks_tls(opts).await {
        return msg!("error.plainOnTls", host = opts.host, port = opts.port).into();
    }
    if garbage {
        return msg!("error.protocol").into();
    }
    msg!(CLOSED_KEY, host = opts.host, port = opts.port).into()
}

/// Connects with version negotiation: "auto" tries MQTT 5, then 3.1.1.
pub async fn establish(opts: &ConnectOptions) -> Result<(Conn, BytesMut, ConnInfo), ConnectFailure> {
    match opts.version {
        VersionPref::V311 => connect_once(opts, Version::V311).await,
        VersionPref::V5 => connect_once(opts, Version::V5).await,
        VersionPref::Auto => match connect_once(opts, Version::V5).await {
            Err(ConnectFailure::Message(m)) if m.key == "error.protocolVersion" || m.key == CLOSED_KEY || m.key == "error.protocol" => {
                connect_once(opts, Version::V311).await
            }
            other => other,
        },
    }
}

/// Result of Try connect.
#[derive(Debug, Clone)]
pub struct TestReport {
    pub info: ConnInfo,
}

/// CONNECT then DISCONNECT, reporting what the broker said.
pub async fn test_connection(opts: &ConnectOptions) -> Result<TestReport, ConnectFailure> {
    let (mut conn, _buf, info) = establish(opts).await?;
    let _ = conn.write_all(&disconnect_bytes(info.version)).await;
    conn.shutdown().await;
    Ok(TestReport { info })
}

/// Connects (first attempt reported to the caller) and then keeps the connection alive in the background,
/// reconnecting after a loss when `opts.reconnect` is set.
pub async fn connect(opts: ConnectOptions) -> Result<(ClientHandle, ConnInfo, mpsc::Receiver<ClientEvent>), ConnectFailure> {
    let (conn, buf, info) = establish(&opts).await?;
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (evt_tx, evt_rx) = mpsc::channel(65_536);
    let handle = ClientHandle { tx: cmd_tx, tokens: Arc::new(AtomicU64::new(1)) };
    let first = info.clone();
    tokio::spawn(managed(opts, conn, buf, info, cmd_rx, evt_tx));
    Ok((handle, first, evt_rx))
}

async fn managed(
    opts: ConnectOptions,
    conn: Conn,
    buf: BytesMut,
    info: ConnInfo,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    evt_tx: mpsc::Sender<ClientEvent>,
) {
    let mut subs: Vec<(String, u8)> = opts.subscriptions.clone();
    let mut current = Some((conn, buf, info));
    let mut attempt: u32 = 0;
    loop {
        let (conn, buf, info) = match current.take() {
            Some(c) => c,
            None => {
                attempt += 1;
                let _ = evt_tx.send(ClientEvent::Reconnecting { attempt }).await;
                let delay = backoff(&opts, attempt);
                // Commands queued while disconnected: honour Disconnect, fail publishes.
                let deadline = tokio::time::sleep(delay);
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        cmd = cmd_rx.recv() => match cmd {
                            None => return,
                            Some(Command::Disconnect(done)) => { let _ = evt_tx.send(ClientEvent::Closed).await; let _ = done.send(()); return; }
                            Some(Command::Publish { token, .. }) => { let _ = evt_tx.send(ClientEvent::Published { token, result: Err(msg!("error.notConnected")) }).await; }
                            Some(Command::Subscribe(f)) => { for x in f { if !subs.iter().any(|s| s.0 == x.0) { subs.push(x) } } }
                            Some(Command::Unsubscribe(f)) => subs.retain(|s| !f.contains(&s.0)),
                        }
                    }
                }
                match establish(&opts).await {
                    Ok(c) => c,
                    Err(f) if f.is_permanent() => {
                        let _ = evt_tx.send(ClientEvent::Failed(f)).await;
                        return;
                    }
                    Err(f) => {
                        let _ = evt_tx.send(ClientEvent::Lost(f.message())).await;
                        continue;
                    }
                }
            }
        };
        attempt = 0;
        let version = info.version;
        let _ = evt_tx.send(ClientEvent::Connected(Box::new(info.clone()))).await;
        let exit = run(conn, buf, version, &info, &opts, &mut subs, &mut cmd_rx, &evt_tx).await;
        match exit {
            Exit::User => {
                let _ = evt_tx.send(ClientEvent::Closed).await;
                return;
            }
            Exit::Lost(m) => {
                let _ = evt_tx.send(ClientEvent::Lost(m)).await;
                if !opts.reconnect {
                    let _ = evt_tx.send(ClientEvent::Closed).await;
                    return;
                }
            }
        }
    }
}

fn backoff(opts: &ConnectOptions, attempt: u32) -> Duration {
    let base = opts.reconnect_min.as_millis() as u64;
    let max = opts.reconnect_max.as_millis() as u64;
    let ms = base.saturating_mul(1u64 << attempt.saturating_sub(1).min(10)).min(max.max(base));
    Duration::from_millis(ms)
}

enum Exit {
    User,
    Lost(UserMessage),
}

struct Pending {
    token: u64,
}

#[allow(clippy::too_many_arguments)]
async fn run(
    mut conn: Conn,
    mut buf: BytesMut,
    version: Version,
    info: &ConnInfo,
    opts: &ConnectOptions,
    subs: &mut Vec<(String, u8)>,
    cmd_rx: &mut mpsc::UnboundedReceiver<Command>,
    evt_tx: &mpsc::Sender<ClientEvent>,
) -> Exit {
    let mut wbuf = BytesMut::with_capacity(4096);
    let mut next_pkid: u16 = 0;
    let mut outgoing: HashMap<u16, Pending> = HashMap::new();
    let mut sub_pending: HashMap<u16, Vec<String>> = HashMap::new();
    let mut alias_in: HashMap<u16, String> = HashMap::new();
    let keep_alive = Duration::from_secs(info.server_keep_alive.unwrap_or(opts.keep_alive) as u64);
    let mut last_sent = Instant::now();
    let mut last_recv = Instant::now();
    let mut ping_sent: Option<Instant> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut alloc = |outgoing: &HashMap<u16, Pending>, sub_pending: &HashMap<u16, Vec<String>>| -> u16 {
        loop {
            next_pkid = next_pkid.wrapping_add(1);
            if next_pkid != 0 && !outgoing.contains_key(&next_pkid) && !sub_pending.contains_key(&next_pkid) {
                return next_pkid;
            }
        }
    };

    macro_rules! lost {
        ($m:expr) => {{
            for (_, p) in outgoing.drain() {
                let _ = evt_tx.send(ClientEvent::Published { token: p.token, result: Err(msg!("error.notConnected")) }).await;
            }
            return Exit::Lost($m);
        }};
    }
    macro_rules! write {
        ($pkt:expr) => {{
            if let Err(e) = send(&mut conn, version, $pkt, &mut wbuf).await {
                lost!(io_lost(&e, opts));
            }
            last_sent = Instant::now();
        }};
    }

    // Subscribe to everything the profile asks for.
    if !subs.is_empty() {
        let pkid = alloc(&outgoing, &sub_pending);
        let filters: Vec<String> = subs.iter().map(|s| s.0.clone()).collect();
        sub_pending.insert(pkid, filters);
        write!(subscribe_packet(version, pkid, subs));
    }

    loop {
        // Drain complete packets already buffered.
        loop {
            let pkt = match read_packet(version, &mut buf) {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(_) => lost!(msg!("error.protocol")),
            };
            last_recv = Instant::now();
            ping_sent = None;
            match pkt {
                Pkt::V4(p) => match p {
                    v4::Packet::Publish(p) => {
                        let qos = qos_u8(p.qos);
                        if qos == 1 {
                            write!(Pkt::V4(v4::Packet::PubAck(v4::PubAck::new(p.pkid))));
                        } else if qos == 2 {
                            write!(Pkt::V4(v4::Packet::PubRec(v4::PubRec::new(p.pkid))));
                        }
                        let m = Incoming { topic: p.topic, payload: p.payload, qos, retain: p.retain, dup: p.dup, props: None };
                        if evt_tx.send(ClientEvent::Message(m)).await.is_err() {
                            return Exit::User;
                        }
                    }
                    v4::Packet::PubRel(r) => write!(Pkt::V4(v4::Packet::PubComp(v4::PubComp::new(r.pkid)))),
                    v4::Packet::PubAck(a) => {
                        if let Some(p) = outgoing.remove(&a.pkid) {
                            let _ = evt_tx.send(ClientEvent::Published { token: p.token, result: Ok(PubStatus::Acked) }).await;
                        }
                    }
                    v4::Packet::PubRec(r) => write!(Pkt::V4(v4::Packet::PubRel(v4::PubRel::new(r.pkid)))),
                    v4::Packet::PubComp(c) => {
                        if let Some(p) = outgoing.remove(&c.pkid) {
                            let _ = evt_tx.send(ClientEvent::Published { token: p.token, result: Ok(PubStatus::Acked) }).await;
                        }
                    }
                    v4::Packet::SubAck(s) => {
                        let filters = sub_pending.remove(&s.pkid).unwrap_or_default();
                        for (f, rc) in filters.into_iter().zip(s.return_codes) {
                            let result = match rc {
                                v4::SubscribeReasonCode::Success(q) => Ok(qos_u8(q)),
                                v4::SubscribeReasonCode::Failure => Err(msg!("error.subscribeRefused", filter = f.clone(), reason = "@reason.code", code = "0x80")),
                            };
                            let _ = evt_tx.send(ClientEvent::Subscribed { filter: f, result }).await;
                        }
                    }
                    v4::Packet::UnsubAck(_) | v4::Packet::PingResp => {}
                    _ => {}
                },
                Pkt::V5(p) => match p {
                    v5::Packet::Publish(p) => {
                        let qos = qos5_u8(p.qos);
                        if qos == 1 {
                            write!(Pkt::V5(v5::Packet::PubAck(v5::PubAck::new(p.pkid, None))));
                        } else if qos == 2 {
                            write!(Pkt::V5(v5::Packet::PubRec(v5::PubRec::new(p.pkid, None))));
                        }
                        let props = p.properties.as_ref().map(props_in).unwrap_or_default();
                        let mut topic = String::from_utf8_lossy(&p.topic).to_string();
                        if let Some(alias) = props.topic_alias {
                            if topic.is_empty() {
                                topic = alias_in.get(&alias).cloned().unwrap_or_default();
                            } else {
                                alias_in.insert(alias, topic.clone());
                            }
                        }
                        let m = Incoming { topic, payload: p.payload, qos, retain: p.retain, dup: p.dup, props: Some(props) };
                        if evt_tx.send(ClientEvent::Message(m)).await.is_err() {
                            return Exit::User;
                        }
                    }
                    v5::Packet::PubRel(r) => write!(Pkt::V5(v5::Packet::PubComp(v5::PubComp::new(r.pkid, None)))),
                    v5::Packet::PubAck(a) => {
                        if let Some(p) = outgoing.remove(&a.pkid) {
                            let reason = a.properties.as_ref().and_then(|x| x.reason_string.clone());
                            let result = puback_result(puback_code(a.reason), a.reason, reason);
                            let _ = evt_tx.send(ClientEvent::Published { token: p.token, result }).await;
                        }
                    }
                    v5::Packet::PubRec(r) => {
                        let code = pubrec_code(r.reason);
                        if pubrec_failed(&r.reason) {
                            if let Some(p) = outgoing.remove(&r.pkid) {
                                let reason = r.properties.as_ref().and_then(|x| x.reason_string.clone());
                                let _ = evt_tx.send(ClientEvent::Published { token: p.token, result: Err(pub_refused(code, pubrec_reason_key(&r.reason), reason)) }).await;
                            }
                        } else {
                            write!(Pkt::V5(v5::Packet::PubRel(v5::PubRel::new(r.pkid, None))));
                        }
                    }
                    v5::Packet::PubComp(c) => {
                        if let Some(p) = outgoing.remove(&c.pkid) {
                            let _ = evt_tx.send(ClientEvent::Published { token: p.token, result: Ok(PubStatus::Acked) }).await;
                        }
                    }
                    v5::Packet::SubAck(s) => {
                        let filters = sub_pending.remove(&s.pkid).unwrap_or_default();
                        for (f, rc) in filters.into_iter().zip(s.return_codes) {
                            let result = suback_v5(&f, rc);
                            let _ = evt_tx.send(ClientEvent::Subscribed { filter: f, result }).await;
                        }
                    }
                    v5::Packet::Disconnect(d) => {
                        lost!(msg!("error.disconnectedBy", reason = reason_param_v5(d.reason_code)));
                    }
                    _ => {}
                },
            }
        }

        tokio::select! {
            r = conn.read_into(&mut buf) => match r {
                Ok(0) => lost!(msg!("error.closed", host = opts.host, port = opts.port)),
                Ok(_) => {}
                Err(e) => lost!(io_lost(&e, opts)),
            },
            cmd = cmd_rx.recv() => match cmd {
                None => return Exit::User,
                Some(Command::Disconnect(done)) => {
                    let _ = conn.write_all(&disconnect_bytes(version)).await;
                    conn.shutdown().await;
                    for (_, p) in outgoing.drain() {
                        let _ = evt_tx.send(ClientEvent::Published { token: p.token, result: Err(msg!("error.notConnected")) }).await;
                    }
                    let _ = done.send(());
                    return Exit::User;
                }
                Some(Command::Publish { token, msg: m }) => {
                    let pkid = if m.qos > 0 { alloc(&outgoing, &sub_pending) } else { 0 };
                    let pkt = match version {
                        Version::V311 => {
                            let mut p = v4::Publish::new(m.topic.clone(), qos_of(m.qos), m.payload.clone());
                            p.retain = m.retain;
                            p.pkid = pkid;
                            Pkt::V4(v4::Packet::Publish(p))
                        }
                        Version::V5 => {
                            let mut p = v5::Publish::new(m.topic.clone(), qos5_of(m.qos), Bytes::from(m.payload.clone()), m.props.as_ref().map(props_out));
                            p.retain = m.retain;
                            p.pkid = pkid;
                            Pkt::V5(v5::Packet::Publish(p))
                        }
                    };
                    let size = match &pkt { Pkt::V4(v4::Packet::Publish(p)) => p.size(), Pkt::V5(v5::Packet::Publish(p)) => p.size(), _ => 0 };
                    if let Some(max) = info.max_packet_size {
                        if size as u64 > max as u64 {
                            let _ = evt_tx.send(ClientEvent::Published { token, result: Err(msg!("error.packetTooLarge", size = human_size(max as u64))) }).await;
                            continue;
                        }
                    }
                    if m.qos > 0 {
                        outgoing.insert(pkid, Pending { token });
                    }
                    write!(pkt);
                    if m.qos == 0 {
                        let _ = evt_tx.send(ClientEvent::Published { token, result: Ok(PubStatus::Sent) }).await;
                    }
                }
                Some(Command::Subscribe(filters)) => {
                    let new: Vec<(String, u8)> = filters.into_iter().filter(|f| !subs.iter().any(|s| s.0 == f.0)).collect();
                    if !new.is_empty() {
                        subs.extend(new.iter().cloned());
                        let pkid = alloc(&outgoing, &sub_pending);
                        sub_pending.insert(pkid, new.iter().map(|s| s.0.clone()).collect());
                        write!(subscribe_packet(version, pkid, &new));
                    }
                }
                Some(Command::Unsubscribe(filters)) => {
                    subs.retain(|s| !filters.contains(&s.0));
                    for f in filters {
                        let pkid = alloc(&outgoing, &sub_pending);
                        let pkt = match version {
                            Version::V311 => { let mut u = v4::Unsubscribe::new(f); u.pkid = pkid; Pkt::V4(v4::Packet::Unsubscribe(u)) }
                            Version::V5 => { let mut u = v5::Unsubscribe::new(f, None); u.pkid = pkid; Pkt::V5(v5::Packet::Unsubscribe(u)) }
                        };
                        write!(pkt);
                    }
                }
            },
            _ = tick.tick() => {
                if !keep_alive.is_zero() {
                    if let Some(sent) = ping_sent {
                        if sent.elapsed() > keep_alive / 2 + Duration::from_secs(1) && last_recv < sent {
                            lost!(msg!("error.keepAlive"));
                        }
                    } else if last_sent.elapsed() >= keep_alive || last_recv.elapsed() >= keep_alive {
                        let pkt = match version { Version::V311 => Pkt::V4(v4::Packet::PingReq), Version::V5 => Pkt::V5(v5::Packet::PingReq(v5::PingReq)) };
                        write!(pkt);
                        ping_sent = Some(Instant::now());
                    }
                }
            }
        }
    }
}

fn io_lost(e: &std::io::Error, opts: &ConnectOptions) -> UserMessage {
    if let Some(rustls::Error::AlertReceived(a)) = transport::rustls_error(e) {
        if matches!(a, rustls::AlertDescription::CertificateRevoked) {
            return msg!("tls.clientCertRevoked", name = "");
        }
    }
    transport::classify_plain_io(e, opts)
}

fn subscribe_packet(version: Version, pkid: u16, filters: &[(String, u8)]) -> Pkt {
    match version {
        Version::V311 => {
            let mut s = v4::Subscribe::new_many(filters.iter().map(|(f, q)| v4::SubscribeFilter::new(f.clone(), qos_of(*q))));
            s.pkid = pkid;
            Pkt::V4(v4::Packet::Subscribe(s))
        }
        Version::V5 => {
            let mut s = v5::Subscribe::new_many(filters.iter().map(|(f, q)| v5::Filter::new(f.clone(), qos5_of(*q))), None);
            s.pkid = pkid;
            Pkt::V5(v5::Packet::Subscribe(s))
        }
    }
}

fn suback_v5(filter: &str, rc: v5::SubscribeReasonCode) -> Result<u8, UserMessage> {
    use v5::SubscribeReasonCode as S;
    let (reason, code) = match rc {
        S::Success(q) => return Ok(qos5_u8(q)),
        S::NotAuthorized => ("@reason.notAuthorized", 0x87),
        S::QuotaExceeded => ("@reason.quotaExceeded", 0x97),
        S::TopicFilterInvalid => ("@reason.topicInvalid", 0x8F),
        S::ImplementationSpecific => ("@reason.implementationSpecific", 0x83),
        S::Failure | S::Unspecified => ("@reason.unspecified", 0x80),
        S::PkidInUse => ("@reason.unspecified", 0x91),
        S::SharedSubscriptionsNotSupported => ("@reason.unspecified", 0x9E),
        S::SubscriptionIdNotSupported => ("@reason.unspecified", 0xA1),
        S::WildcardSubscriptionsNotSupported => ("@reason.unspecified", 0xA2),
    };
    let _ = code;
    Err(msg!("error.subscribeRefused", filter = filter, reason = reason))
}

fn pub_refused(_code: u8, reason_key: &str, reason_string: Option<String>) -> UserMessage {
    match reason_string {
        Some(s) if !s.is_empty() => msg!("error.publishRefused", reason = s, code = format!("0x{_code:02X}")),
        _ => msg!("error.publishRefused", reason = reason_key, code = format!("0x{_code:02X}")),
    }
}

fn puback_result(code: u8, reason: v5::PubAckReason, reason_string: Option<String>) -> Result<PubStatus, UserMessage> {
    use v5::PubAckReason as R;
    match reason {
        R::Success | R::NoMatchingSubscribers => Ok(PubStatus::Acked),
        R::NotAuthorized => Err(pub_refused(code, "@reason.notAuthorized", reason_string)),
        R::TopicNameInvalid => Err(pub_refused(code, "@reason.topicInvalid", reason_string)),
        R::QuotaExceeded => Err(pub_refused(code, "@reason.quotaExceeded", reason_string)),
        R::ImplementationSpecificError => Err(pub_refused(code, "@reason.implementationSpecific", reason_string)),
        _ => Err(pub_refused(code, "@reason.unspecified", reason_string)),
    }
}

fn pubrec_failed(r: &v5::PubRecReason) -> bool {
    !matches!(r, v5::PubRecReason::Success | v5::PubRecReason::NoMatchingSubscribers)
}

fn pubrec_reason_key(r: &v5::PubRecReason) -> &'static str {
    match r {
        v5::PubRecReason::NotAuthorized => "@reason.notAuthorized",
        v5::PubRecReason::TopicNameInvalid => "@reason.topicInvalid",
        v5::PubRecReason::QuotaExceeded => "@reason.quotaExceeded",
        _ => "@reason.unspecified",
    }
}

fn puback_code(r: v5::PubAckReason) -> u8 {
    use v5::PubAckReason as R;
    match r {
        R::Success => 0x00,
        R::NoMatchingSubscribers => 0x10,
        R::UnspecifiedError => 0x80,
        R::ImplementationSpecificError => 0x83,
        R::NotAuthorized => 0x87,
        R::TopicNameInvalid => 0x90,
        R::PacketIdentifierInUse => 0x91,
        R::QuotaExceeded => 0x97,
        R::PayloadFormatInvalid => 0x99,
    }
}

fn pubrec_code(r: v5::PubRecReason) -> u8 {
    use v5::PubRecReason as R;
    match r {
        R::Success => 0x00,
        R::NoMatchingSubscribers => 0x10,
        R::UnspecifiedError => 0x80,
        R::ImplementationSpecificError => 0x83,
        R::NotAuthorized => 0x87,
        R::TopicNameInvalid => 0x90,
        R::PacketIdentifierInUse => 0x91,
        R::QuotaExceeded => 0x97,
        R::PayloadFormatInvalid => 0x99,
    }
}
