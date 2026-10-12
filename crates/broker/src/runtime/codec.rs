//! Version-neutral view of MQTT packets on top of the rumqttc codec (`mqttbytes` v4 and v5).
//!
//! Decoding never panics on malformed input: every failure is an error that closes the connection.

use bytes::{Bytes, BytesMut};
use rumqttc::mqttbytes::v4;
use rumqttc::v5::mqttbytes::v5;
use std::time::Instant;

use super::router::SubOpts;

/// Negotiated protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Version {
    V4,
    V5,
}

impl Version {
    pub fn label(self) -> &'static str {
        match self {
            Version::V4 => "3.1.1",
            Version::V5 => "5",
        }
    }
}

/// MQTT 5 properties carried with an application message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MsgProps {
    pub payload_format_indicator: Option<u8>,
    pub message_expiry_interval: Option<u32>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Bytes>,
    pub user_properties: Vec<(String, String)>,
    pub content_type: Option<String>,
}

/// An application message inside the broker.
#[derive(Debug, Clone)]
pub(crate) struct Message {
    pub topic: String,
    pub payload: Bytes,
    pub qos: u8,
    pub retain: bool,
    pub props: Option<MsgProps>,
    pub created: Instant,
    /// Client id of the publisher (for "no local").
    pub publisher: Option<String>,
}

impl Message {
    /// Remaining message expiry in seconds; `None` when the message has no expiry; `Some(0)` when expired.
    pub fn remaining_expiry(&self) -> Option<u32> {
        let exp = self.props.as_ref()?.message_expiry_interval?;
        let elapsed = self.created.elapsed().as_secs();
        Some(u64::from(exp).saturating_sub(elapsed) as u32)
    }
    pub fn expired(&self) -> bool {
        matches!(self.remaining_expiry(), Some(0))
    }
}

/// Last will of a connection.
#[derive(Debug, Clone)]
pub(crate) struct Will {
    pub message: Message,
    pub delay: u32,
}

/// Decoded CONNECT.
#[derive(Debug, Clone)]
pub(crate) struct ConnectInfo {
    pub version: Version,
    pub client_id: String,
    pub clean_start: bool,
    pub keep_alive: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub will: Option<Will>,
    pub session_expiry: u32,
    pub receive_max: u16,
    pub max_packet_size: Option<u32>,
}

/// A packet from a client after CONNECT.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Incoming {
    Publish {
        msg: Message,
        pkid: u16,
        topic_alias: Option<u16>,
    },
    PubAck(u16),
    PubRec(u16),
    PubRel(u16),
    PubComp(u16),
    Subscribe {
        pkid: u16,
        filters: Vec<(String, SubOpts)>,
    },
    Unsubscribe {
        pkid: u16,
        filters: Vec<String>,
    },
    PingReq,
    Disconnect {
        reason: u8,
        session_expiry: Option<u32>,
    },
    /// CONNECT again, AUTH or a server-to-client packet: protocol error.
    Unexpected,
}

/// Decoding failure (closes the connection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecodeError {
    TooLarge,
    Malformed(String),
    /// First packet is CONNECT with a protocol level this broker does not speak.
    UnsupportedProtocol(u8),
    NotConnect,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::TooLarge => f.write_str("packet too large"),
            DecodeError::Malformed(s) => write!(f, "malformed packet ({s})"),
            DecodeError::UnsupportedProtocol(l) => write!(f, "unsupported protocol level {l}"),
            DecodeError::NotConnect => f.write_str("first packet is not CONNECT"),
        }
    }
}

/// Parses the fixed header: (byte1, header length, remaining length), or `None` when more bytes are needed.
fn fixed_header(buf: &[u8]) -> Result<Option<(u8, usize, usize)>, DecodeError> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let mut len = 0usize;
    let mut shift = 0;
    for i in 1..5 {
        let Some(&b) = buf.get(i) else {
            return Ok(None);
        };
        len += ((b & 0x7f) as usize) << shift;
        if b & 0x80 == 0 {
            return Ok(Some((buf[0], 1 + i, len)));
        }
        shift += 7;
    }
    Err(DecodeError::Malformed("remaining length".into()))
}

fn qos_v4(q: rumqttc::mqttbytes::QoS) -> u8 {
    q as u8
}

fn qos_v5(q: rumqttc::v5::mqttbytes::QoS) -> u8 {
    q as u8
}

fn to_qos_v4(q: u8) -> rumqttc::mqttbytes::QoS {
    match q {
        0 => rumqttc::mqttbytes::QoS::AtMostOnce,
        1 => rumqttc::mqttbytes::QoS::AtLeastOnce,
        _ => rumqttc::mqttbytes::QoS::ExactlyOnce,
    }
}

fn to_qos_v5(q: u8) -> rumqttc::v5::mqttbytes::QoS {
    match q {
        0 => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
        1 => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        _ => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
    }
}

/// Decodes the CONNECT packet at the start of `buf`. `Ok(None)`: more bytes needed.
pub(crate) fn decode_connect(
    buf: &mut BytesMut,
    max: usize,
) -> Result<Option<ConnectInfo>, DecodeError> {
    let Some((byte1, hlen, rlen)) = fixed_header(buf)? else {
        return Ok(None);
    };
    if byte1 >> 4 != 1 {
        return Err(DecodeError::NotConnect);
    }
    if rlen > max {
        return Err(DecodeError::TooLarge);
    }
    if buf.len() < hlen + rlen {
        return Ok(None);
    }
    // Variable header: protocol name (2 + n bytes), then the level.
    let vh = &buf[hlen..hlen + rlen];
    if vh.len() < 2 {
        return Err(DecodeError::Malformed("connect header".into()));
    }
    let name_len = u16::from_be_bytes([vh[0], vh[1]]) as usize;
    let level = *vh
        .get(2 + name_len)
        .ok_or_else(|| DecodeError::Malformed("connect header".into()))?;
    let name = &vh[2..2 + name_len];
    if name != b"MQTT" {
        // MQTT 3.1 ("MQIsdp", level 3) and unknown protocols.
        return Err(DecodeError::UnsupportedProtocol(level));
    }
    match level {
        4 => match v4::Packet::read(buf, max) {
            Ok(v4::Packet::Connect(c)) => {
                let will = c.last_will.map(|w| Will {
                    message: Message {
                        topic: w.topic,
                        payload: w.message,
                        qos: qos_v4(w.qos),
                        retain: w.retain,
                        props: None,
                        created: Instant::now(),
                        publisher: Some(c.client_id.clone()),
                    },
                    delay: 0,
                });
                Ok(Some(ConnectInfo {
                    version: Version::V4,
                    client_id: c.client_id,
                    clean_start: c.clean_session,
                    keep_alive: c.keep_alive,
                    username: c
                        .login
                        .as_ref()
                        .map(|l| l.username.clone())
                        .filter(|u| !u.is_empty()),
                    password: c.login.map(|l| l.password),
                    will,
                    session_expiry: 0,
                    receive_max: 65535,
                    max_packet_size: None,
                }))
            }
            Ok(_) => Err(DecodeError::NotConnect),
            Err(e) => Err(DecodeError::Malformed(e.to_string())),
        },
        5 => match v5::Packet::read(buf, Some(max as u32)) {
            Ok(v5::Packet::Connect(c, will, login)) => {
                let props = c.properties.unwrap_or_default();
                let will = match will {
                    Some(w) => {
                        let topic = String::from_utf8(w.topic.to_vec())
                            .map_err(|_| DecodeError::Malformed("will topic".into()))?;
                        let wp = w.properties.unwrap_or(v5::LastWillProperties {
                            delay_interval: None,
                            payload_format_indicator: None,
                            message_expiry_interval: None,
                            content_type: None,
                            response_topic: None,
                            correlation_data: None,
                            user_properties: Vec::new(),
                        });
                        Some(Will {
                            message: Message {
                                topic,
                                payload: w.message,
                                qos: qos_v5(w.qos),
                                retain: w.retain,
                                props: Some(MsgProps {
                                    payload_format_indicator: wp.payload_format_indicator,
                                    message_expiry_interval: wp.message_expiry_interval,
                                    response_topic: wp.response_topic,
                                    correlation_data: wp.correlation_data,
                                    user_properties: wp.user_properties,
                                    content_type: wp.content_type,
                                }),
                                created: Instant::now(),
                                publisher: Some(c.client_id.clone()),
                            },
                            delay: wp.delay_interval.unwrap_or(0),
                        })
                    }
                    None => None,
                };
                Ok(Some(ConnectInfo {
                    version: Version::V5,
                    client_id: c.client_id,
                    clean_start: c.clean_start,
                    keep_alive: c.keep_alive,
                    username: login
                        .as_ref()
                        .map(|l| l.username.clone())
                        .filter(|u| !u.is_empty()),
                    password: login.map(|l| l.password),
                    will,
                    session_expiry: props.session_expiry_interval.unwrap_or(0),
                    receive_max: props.receive_maximum.unwrap_or(65535).max(1),
                    max_packet_size: props.max_packet_size,
                }))
            }
            Ok(_) => Err(DecodeError::NotConnect),
            Err(e) => Err(DecodeError::Malformed(e.to_string())),
        },
        other => Err(DecodeError::UnsupportedProtocol(other)),
    }
}

/// Decodes the next packet after CONNECT. `Ok(None)`: more bytes needed.
pub(crate) fn decode(
    version: Version,
    buf: &mut BytesMut,
    max: usize,
) -> Result<Option<Incoming>, DecodeError> {
    let Some((byte1, hlen, rlen)) = fixed_header(buf)? else {
        return Ok(None);
    };
    if rlen > max {
        return Err(DecodeError::TooLarge);
    }
    if buf.len() < hlen + rlen {
        return Ok(None);
    }
    // MQTT 5 DISCONNECT: the codec mishandles remaining lengths 0 and 1, so parse it here.
    if version == Version::V5 && byte1 >> 4 == 14 {
        let frame = buf.split_to(hlen + rlen);
        let body = &frame[hlen..];
        let reason = body.first().copied().unwrap_or(0);
        let session_expiry = if body.len() > 1 {
            disconnect_session_expiry(&body[1..])?
        } else {
            None
        };
        return Ok(Some(Incoming::Disconnect {
            reason,
            session_expiry,
        }));
    }
    let malformed = |e: String| DecodeError::Malformed(e);
    match version {
        Version::V4 => {
            let p = v4::Packet::read(buf, max).map_err(|e| malformed(e.to_string()))?;
            Ok(Some(match p {
                v4::Packet::Publish(p) => Incoming::Publish {
                    msg: Message {
                        topic: p.topic,
                        payload: p.payload,
                        qos: qos_v4(p.qos),
                        retain: p.retain,
                        props: None,
                        created: Instant::now(),
                        publisher: None,
                    },
                    pkid: p.pkid,
                    topic_alias: None,
                },
                v4::Packet::PubAck(a) => Incoming::PubAck(a.pkid),
                v4::Packet::PubRec(a) => Incoming::PubRec(a.pkid),
                v4::Packet::PubRel(a) => Incoming::PubRel(a.pkid),
                v4::Packet::PubComp(a) => Incoming::PubComp(a.pkid),
                v4::Packet::Subscribe(s) => Incoming::Subscribe {
                    pkid: s.pkid,
                    filters: s
                        .filters
                        .into_iter()
                        .map(|f| {
                            (
                                f.path,
                                SubOpts {
                                    qos: qos_v4(f.qos),
                                    no_local: false,
                                    retain_as_published: false,
                                    retain_handling: 0,
                                    sub_id: None,
                                },
                            )
                        })
                        .collect(),
                },
                v4::Packet::Unsubscribe(u) => Incoming::Unsubscribe {
                    pkid: u.pkid,
                    filters: u.topics,
                },
                v4::Packet::PingReq => Incoming::PingReq,
                v4::Packet::Disconnect => Incoming::Disconnect {
                    reason: 0,
                    session_expiry: None,
                },
                _ => Incoming::Unexpected,
            }))
        }
        Version::V5 => {
            let p =
                v5::Packet::read(buf, Some(max as u32)).map_err(|e| malformed(e.to_string()))?;
            Ok(Some(match p {
                v5::Packet::Publish(p) => {
                    let topic = String::from_utf8(p.topic.to_vec())
                        .map_err(|_| malformed("topic not UTF-8".into()))?;
                    let (props, alias) = match p.properties {
                        Some(pp) => (
                            Some(MsgProps {
                                payload_format_indicator: pp.payload_format_indicator,
                                message_expiry_interval: pp.message_expiry_interval,
                                response_topic: pp.response_topic,
                                correlation_data: pp.correlation_data,
                                user_properties: pp.user_properties,
                                content_type: pp.content_type,
                            }),
                            pp.topic_alias,
                        ),
                        None => (None, None),
                    };
                    Incoming::Publish {
                        msg: Message {
                            topic,
                            payload: p.payload,
                            qos: qos_v5(p.qos),
                            retain: p.retain,
                            props,
                            created: Instant::now(),
                            publisher: None,
                        },
                        pkid: p.pkid,
                        topic_alias: alias,
                    }
                }
                v5::Packet::PubAck(a) => Incoming::PubAck(a.pkid),
                v5::Packet::PubRec(a) => Incoming::PubRec(a.pkid),
                v5::Packet::PubRel(a) => Incoming::PubRel(a.pkid),
                v5::Packet::PubComp(a) => Incoming::PubComp(a.pkid),
                v5::Packet::Subscribe(s) => {
                    let sub_id = s.properties.as_ref().and_then(|p| p.id);
                    Incoming::Subscribe {
                        pkid: s.pkid,
                        filters: s
                            .filters
                            .into_iter()
                            .map(|f| {
                                (
                                    f.path,
                                    SubOpts {
                                        qos: qos_v5(f.qos),
                                        no_local: f.nolocal,
                                        retain_as_published: f.preserve_retain,
                                        retain_handling: match f.retain_forward_rule {
                                            v5::RetainForwardRule::OnEverySubscribe => 0,
                                            v5::RetainForwardRule::OnNewSubscribe => 1,
                                            v5::RetainForwardRule::Never => 2,
                                        },
                                        sub_id,
                                    },
                                )
                            })
                            .collect(),
                    }
                }
                v5::Packet::Unsubscribe(u) => Incoming::Unsubscribe {
                    pkid: u.pkid,
                    filters: u.filters,
                },
                v5::Packet::PingReq(_) => Incoming::PingReq,
                v5::Packet::Disconnect(d) => Incoming::Disconnect {
                    reason: d.reason_code as u8,
                    session_expiry: d.properties.and_then(|p| p.session_expiry_interval),
                },
                _ => Incoming::Unexpected,
            }))
        }
    }
}

/// CONNACK outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnAckCode {
    Success,
    BadLogin,
    QuotaExceeded,
    BadClientId,
    UnsupportedProtocol,
}

/// Extra CONNACK data for MQTT 5.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConnAckExtra {
    pub assigned_client_id: Option<String>,
    pub max_packet_size: Option<u32>,
    pub receive_max: Option<u16>,
}

/// Encodes a CONNACK. MQTT 3.1.1 bad login is 0x05 (not authorized, as the spec and test plan require).
pub(crate) fn encode_connack(
    version: Version,
    out: &mut BytesMut,
    session_present: bool,
    code: ConnAckCode,
    extra: ConnAckExtra,
) {
    match version {
        Version::V4 => {
            let c = match code {
                ConnAckCode::Success => v4::ConnectReturnCode::Success,
                ConnAckCode::BadLogin => v4::ConnectReturnCode::NotAuthorized,
                ConnAckCode::QuotaExceeded => v4::ConnectReturnCode::ServiceUnavailable,
                ConnAckCode::BadClientId => v4::ConnectReturnCode::BadClientId,
                ConnAckCode::UnsupportedProtocol => v4::ConnectReturnCode::RefusedProtocolVersion,
            };
            let _ =
                v4::Packet::ConnAck(v4::ConnAck::new(c, session_present)).write(out, usize::MAX);
        }
        Version::V5 => {
            let c = match code {
                ConnAckCode::Success => v5::ConnectReturnCode::Success,
                ConnAckCode::BadLogin => v5::ConnectReturnCode::BadUserNamePassword,
                ConnAckCode::QuotaExceeded => v5::ConnectReturnCode::QuotaExceeded,
                ConnAckCode::BadClientId => v5::ConnectReturnCode::ClientIdentifierNotValid,
                ConnAckCode::UnsupportedProtocol => {
                    v5::ConnectReturnCode::UnsupportedProtocolVersion
                }
            };
            let props = if code == ConnAckCode::Success {
                Some(v5::ConnAckProperties {
                    session_expiry_interval: None,
                    receive_max: extra.receive_max,
                    max_qos: None,
                    retain_available: Some(1),
                    max_packet_size: extra.max_packet_size,
                    assigned_client_identifier: extra.assigned_client_id,
                    topic_alias_max: None,
                    reason_string: None,
                    user_properties: Vec::new(),
                    wildcard_subscription_available: Some(1),
                    subscription_identifiers_available: Some(1),
                    shared_subscription_available: Some(1),
                    server_keep_alive: None,
                    response_information: None,
                    server_reference: None,
                    authentication_method: None,
                    authentication_data: None,
                })
            } else {
                None
            };
            let _ = v5::Packet::ConnAck(v5::ConnAck {
                session_present,
                code: c,
                properties: props,
            })
            .write(out, None);
        }
    }
}

/// Encodes a PUBLISH for a subscriber. Returns false when it exceeds `max_size` (the message is skipped).
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_publish(
    version: Version,
    out: &mut BytesMut,
    msg: &Message,
    qos: u8,
    retain: bool,
    pkid: u16,
    dup: bool,
    sub_ids: &[usize],
    max_size: Option<u32>,
) -> bool {
    match version {
        Version::V4 => {
            let p = v4::Publish {
                dup,
                qos: to_qos_v4(qos),
                retain,
                topic: msg.topic.clone(),
                pkid: if qos == 0 { 0 } else { pkid },
                payload: msg.payload.clone(),
            };
            v4::Packet::Publish(p)
                .write(out, max_size.map(|m| m as usize).unwrap_or(usize::MAX))
                .is_ok()
        }
        Version::V5 => {
            let props = if msg.props.is_some() || !sub_ids.is_empty() {
                let mp = msg.props.clone().unwrap_or_default();
                Some(v5::PublishProperties {
                    payload_format_indicator: mp.payload_format_indicator,
                    message_expiry_interval: msg.remaining_expiry(),
                    topic_alias: None,
                    response_topic: mp.response_topic,
                    correlation_data: mp.correlation_data,
                    user_properties: mp.user_properties,
                    subscription_identifiers: sub_ids.to_vec(),
                    content_type: mp.content_type,
                })
            } else {
                None
            };
            let p = v5::Publish {
                dup,
                qos: to_qos_v5(qos),
                retain,
                topic: Bytes::copy_from_slice(msg.topic.as_bytes()),
                pkid: if qos == 0 { 0 } else { pkid },
                payload: msg.payload.clone(),
                properties: props,
            };
            v5::Packet::Publish(p).write(out, max_size).is_ok()
        }
    }
}

/// Acknowledgement kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Ack {
    PubAck,
    PubRec,
    PubRel,
    PubComp,
}

pub(crate) fn encode_ack(version: Version, out: &mut BytesMut, ack: Ack, pkid: u16) {
    match version {
        Version::V4 => {
            let p = match ack {
                Ack::PubAck => v4::Packet::PubAck(v4::PubAck::new(pkid)),
                Ack::PubRec => v4::Packet::PubRec(v4::PubRec::new(pkid)),
                Ack::PubRel => v4::Packet::PubRel(v4::PubRel::new(pkid)),
                Ack::PubComp => v4::Packet::PubComp(v4::PubComp::new(pkid)),
            };
            let _ = p.write(out, usize::MAX);
        }
        Version::V5 => {
            let p = match ack {
                Ack::PubAck => v5::Packet::PubAck(v5::PubAck::new(pkid, None)),
                Ack::PubRec => v5::Packet::PubRec(v5::PubRec::new(pkid, None)),
                Ack::PubRel => v5::Packet::PubRel(v5::PubRel::new(pkid, None)),
                Ack::PubComp => v5::Packet::PubComp(v5::PubComp::new(pkid, None)),
            };
            let _ = p.write(out, None);
        }
    }
}

/// SUBACK with one entry per filter: `Some(granted qos)` or `None` for a refused filter.
pub(crate) fn encode_suback(version: Version, out: &mut BytesMut, pkid: u16, codes: &[Option<u8>]) {
    match version {
        Version::V4 => {
            let rc = codes
                .iter()
                .map(|c| match c {
                    Some(q) => v4::SubscribeReasonCode::Success(to_qos_v4(*q)),
                    None => v4::SubscribeReasonCode::Failure,
                })
                .collect();
            let _ = v4::Packet::SubAck(v4::SubAck::new(pkid, rc)).write(out, usize::MAX);
        }
        Version::V5 => {
            let rc = codes
                .iter()
                .map(|c| match c {
                    Some(q) => v5::SubscribeReasonCode::Success(to_qos_v5(*q)),
                    None => v5::SubscribeReasonCode::TopicFilterInvalid,
                })
                .collect();
            let _ = v5::Packet::SubAck(v5::SubAck {
                pkid,
                return_codes: rc,
                properties: None,
            })
            .write(out, None);
        }
    }
}

/// UNSUBACK; `existed[i]` tells whether the filter was subscribed (MQTT 5 reason codes).
pub(crate) fn encode_unsuback(version: Version, out: &mut BytesMut, pkid: u16, existed: &[bool]) {
    match version {
        Version::V4 => {
            let _ = v4::Packet::UnsubAck(v4::UnsubAck::new(pkid)).write(out, usize::MAX);
        }
        Version::V5 => {
            let reasons = existed
                .iter()
                .map(|e| {
                    if *e {
                        v5::UnsubAckReason::Success
                    } else {
                        v5::UnsubAckReason::NoSubscriptionExisted
                    }
                })
                .collect();
            let _ = v5::Packet::UnsubAck(v5::UnsubAck {
                pkid,
                reasons,
                properties: None,
            })
            .write(out, None);
        }
    }
}

pub(crate) fn encode_pingresp(version: Version, out: &mut BytesMut) {
    match version {
        Version::V4 => {
            let _ = v4::Packet::PingResp.write(out, usize::MAX);
        }
        Version::V5 => {
            let _ = v5::Packet::PingResp(v5::PingResp).write(out, None);
        }
    }
}

/// Server DISCONNECT (MQTT 5 only; MQTT 3.1.1 just closes the connection). Written by hand: the codec's
/// DISCONNECT encoder declares a wrong remaining length.
pub(crate) fn encode_disconnect(version: Version, out: &mut BytesMut, reason: u8) {
    if version != Version::V5 {
        return;
    }
    out.extend_from_slice(&[0xE0, 0x02, reason, 0x00]);
}

/// Session expiry interval (property 0x11) from the properties of a DISCONNECT.
fn disconnect_session_expiry(props: &[u8]) -> Result<Option<u32>, DecodeError> {
    let bad = || DecodeError::Malformed("disconnect properties".into());
    // Property length (variable byte integer).
    let mut len = 0usize;
    let mut i = 0usize;
    let mut shift = 0;
    loop {
        let b = *props.get(i).ok_or_else(bad)?;
        len += ((b & 0x7f) as usize) << shift;
        i += 1;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 21 {
            return Err(bad());
        }
    }
    let end = i
        .checked_add(len)
        .filter(|e| *e <= props.len())
        .ok_or_else(bad)?;
    let mut p = &props[i..end];
    while let Some((&id, rest)) = p.split_first() {
        match id {
            0x11 => {
                let v = rest.get(..4).ok_or_else(bad)?;
                return Ok(Some(u32::from_be_bytes([v[0], v[1], v[2], v[3]])));
            }
            // Reason string, server reference: UTF-8 string (2-byte length).
            0x1F | 0x1C => {
                let l = rest
                    .get(..2)
                    .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
                    .ok_or_else(bad)?;
                p = rest.get(2 + l..).ok_or_else(bad)?;
            }
            // User property: two strings.
            0x26 => {
                let l1 = rest
                    .get(..2)
                    .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
                    .ok_or_else(bad)?;
                let r2 = rest.get(2 + l1..).ok_or_else(bad)?;
                let l2 = r2
                    .get(..2)
                    .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
                    .ok_or_else(bad)?;
                p = r2.get(2 + l2..).ok_or_else(bad)?;
            }
            _ => return Err(bad()),
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_never_panics() {
        let mut seed: u64 = 0x1234_5678_9abc_def0;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..20_000 {
            let len = (next() % 64) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            if round % 3 == 0 && !bytes.is_empty() {
                bytes[0] = [0x10, 0x30, 0x32, 0x82, 0xA2, 0xE0, 0x62, 0x40][(next() % 8) as usize];
            }
            let mut b = BytesMut::from(&bytes[..]);
            let _ = decode_connect(&mut b, 1 << 16);
            for v in [Version::V4, Version::V5] {
                let mut b = BytesMut::from(&bytes[..]);
                for _ in 0..4 {
                    match decode(v, &mut b, 1 << 16) {
                        Ok(Some(_)) => continue,
                        _ => break,
                    }
                }
            }
        }
    }
}
