//! F-02 harness: starts the broker in-process on random free ports with the F-01 certificate kit, plus test clients
//! (rumqttc with a ring rustls config, and a raw packet client for exact CONNACK codes, WebSocket and garbage).
#![allow(dead_code)]

use bytes::BytesMut;
use mqttpanel_broker::config::ListenerKind;
use mqttpanel_broker::runtime::{
    AuthConfig, BindAddr, Broker, BrokerHandle, ListenerSpec, RuntimeConfig, TlsMaterial,
};
use mqttpanel_tls::kit::TestKit;
use rumqttc::mqttbytes::v4;
use rumqttc::v5::mqttbytes::v5;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

pub const T: Duration = Duration::from_secs(5);

/// The F-01 kit, generated once per test binary.
pub fn kit() -> &'static TestKit {
    static KIT: OnceLock<TestKit> = OnceLock::new();
    KIT.get_or_init(|| TestKit::generate().expect("test kit"))
}

pub fn der(pem: &str) -> Vec<CertificateDer<'static>> {
    mqttpanel_tls::parse_certs(pem.as_bytes()).expect("pem")
}

/// Server material from the kit: server cert for localhost from the root, client certificates verified by the root.
pub fn kit_material() -> Arc<TlsMaterial> {
    let k = kit();
    Arc::new(TlsMaterial {
        chain: vec![
            CertificateDer::from(k.server.cert_der.clone()),
            CertificateDer::from(k.root.cert_der.clone()),
        ],
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(k.server.key_der.clone())),
        client_roots: vec![CertificateDer::from(k.root.cert_der.clone())],
    })
}

/// A listener on a free port, localhost.
pub fn spec(id: &str, kind: ListenerKind, require_client_cert: bool) -> ListenerSpec {
    ListenerSpec {
        id: id.into(),
        kind,
        bind: BindAddr::Localhost,
        port: 0,
        require_client_cert,
        tls: kind.encrypted().then(kit_material),
    }
}

/// Starts a broker with the given listeners and auth.
pub async fn start(listeners: Vec<ListenerSpec>, auth: AuthConfig) -> BrokerHandle {
    let cfg = RuntimeConfig {
        listeners,
        auth,
        ..Default::default()
    };
    Broker::start(cfg).await.expect("broker starts")
}

/// Anonymous broker with one listener of each kind (ids tcp, tls, ws, wss).
pub async fn start_all_anonymous() -> BrokerHandle {
    start(
        vec![
            spec("tcp", ListenerKind::Tcp, false),
            spec("tls", ListenerKind::Tls, false),
            spec("ws", ListenerKind::Ws, false),
            spec("wss", ListenerKind::Wss, false),
        ],
        AuthConfig {
            anonymous: true,
            ..Default::default()
        },
    )
    .await
}

/// A free TCP port (bound and released).
/// Picked outside the ephemeral ranges (client sockets of parallel tests use those) and never twice per process.
pub fn free_port() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let _ = NEXT.compare_exchange(
        0,
        20_000 + (std::process::id() % 400) as u16 * 20,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
    loop {
        let p = NEXT.fetch_add(1, Ordering::SeqCst);
        let free = ["0.0.0.0", "127.0.0.1"]
            .iter()
            .all(|a| std::net::TcpListener::bind((*a, p)).is_ok());
        if free {
            return p;
        }
    }
}

/// rustls client config (ring) trusting `roots`, with an optional client identity.
pub fn tls_cfg(
    roots: &[CertificateDer<'static>],
    identity: Option<mqttpanel_tls::Identity>,
) -> Arc<rustls::ClientConfig> {
    let mut store = rustls::RootCertStore::empty();
    for r in roots {
        store.add(r.clone()).expect("root");
    }
    let b = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("versions")
    .with_root_certificates(store);
    let cfg = match identity {
        Some(id) => b
            .with_client_auth_cert(id.certs, id.key)
            .expect("client cert"),
        None => b.with_no_client_auth(),
    };
    Arc::new(cfg)
}

/// Client config trusting the kit root, with an optional kit identity.
pub fn kit_tls(identity: Option<&mqttpanel_tls::kit::Issued>) -> Arc<rustls::ClientConfig> {
    let id = identity.map(|i| mqttpanel_tls::Identity {
        certs: vec![CertificateDer::from(i.cert_der.clone())],
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(i.key_der.clone())),
    });
    tls_cfg(&[CertificateDer::from(kit().root.cert_der.clone())], id)
}

// ------------------------------------------------------------------ test client

/// Connection options of the test client.
#[derive(Clone)]
pub struct Opts {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub tls: Option<Arc<rustls::ClientConfig>>,
    pub login: Option<(String, String)>,
    pub v5: bool,
    /// Clean session (3.1.1) / clean start with session expiry 0 (5). `false`: persistent session.
    pub clean: bool,
    pub keep_alive: u16,
}

fn opts(
    id: &str,
    port: u16,
    tls: Option<Arc<rustls::ClientConfig>>,
    login: Option<(&str, &str)>,
    v5: bool,
) -> Opts {
    Opts {
        id: id.into(),
        host: "127.0.0.1".into(),
        port,
        tls,
        login: login.map(|(u, p)| (u.to_string(), p.to_string())),
        v5,
        clean: true,
        keep_alive: 30,
    }
}

/// MQTT 3.1.1 options.
pub fn opts4(
    id: &str,
    port: u16,
    tls: Option<Arc<rustls::ClientConfig>>,
    login: Option<(&str, &str)>,
) -> Opts {
    opts(id, port, tls, login, false)
}

/// MQTT 5 options.
pub fn opts5(
    id: &str,
    port: u16,
    tls: Option<Arc<rustls::ClientConfig>>,
    login: Option<(&str, &str)>,
) -> Opts {
    opts(id, port, tls, login, true)
}

/// A received application message.
#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub props: Option<v5::PublishProperties>,
}

type Writer = Arc<tokio::sync::Mutex<tokio::io::WriteHalf<Box<dyn Io>>>>;

/// A connected test client (built on the raw codec over TCP or TLS); incoming publishes arrive on `rx`.
pub struct Client {
    w: Writer,
    v5: bool,
    pub rx: mpsc::UnboundedReceiver<Msg>,
    pub assigned_id: Option<String>,
    pub max_packet: Option<u32>,
    subacks: Arc<std::sync::Mutex<Vec<oneshot::Sender<Vec<u8>>>>>,
    next_pkid: std::sync::atomic::AtomicU16,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn write_packet(w: &Writer, b: &[u8]) {
    let mut g = w.lock().await;
    let _ = g.write_all(b).await;
    let _ = g.flush().await;
}

/// Connects; `Err` describes why (refused CONNACK code, TLS failure, closed).
pub async fn connect(o: Opts) -> Result<Client, String> {
    let fut = async {
        let tcp = tokio::net::TcpStream::connect((o.host.as_str(), o.port))
            .await
            .map_err(|e| e.to_string())?;
        let io: Box<dyn Io> = match &o.tls {
            Some(cfg) => {
                let name = ServerName::try_from("localhost").expect("name").to_owned();
                Box::new(
                    tokio_rustls::TlsConnector::from(cfg.clone())
                        .connect(name, tcp)
                        .await
                        .map_err(|e| e.to_string())?,
                )
            }
            None => Box::new(tcp),
        };
        let (mut r, w) = tokio::io::split(io);
        let w: Writer = Arc::new(tokio::sync::Mutex::new(w));
        let mut b = BytesMut::new();
        if o.v5 {
            let props = (!o.clean).then(|| {
                let mut p = v5::ConnectProperties::new();
                p.session_expiry_interval = Some(3600);
                p
            });
            let c = v5::Connect {
                keep_alive: o.keep_alive,
                client_id: o.id.clone(),
                clean_start: o.clean,
                properties: props,
            };
            let l = o
                .login
                .as_ref()
                .map(|(u, p)| v5::Login::new(u.clone(), p.clone()));
            v5::Packet::Connect(c, None, l)
                .write(&mut b, None)
                .expect("encode");
        } else {
            let mut c = v4::Connect::new(o.id.clone());
            c.keep_alive = o.keep_alive;
            c.clean_session = o.clean;
            c.login = o
                .login
                .as_ref()
                .map(|(u, p)| v4::Login::new(u.clone(), p.clone()));
            v4::Packet::Connect(c)
                .write(&mut b, usize::MAX)
                .expect("encode");
        }
        write_packet(&w, &b).await;
        let mut buf = BytesMut::new();
        let (assigned_id, max_packet) = loop {
            if let Some(f) = frame(&mut buf) {
                if f[0] >> 4 != 2 {
                    return Err(format!("expected CONNACK, got {:#x}", f[0]));
                }
                let code = f.get(header_len(&f) + 1).copied().unwrap_or(0xff);
                if code != 0 {
                    return Err(format!("refused {code:#x}"));
                }
                if o.v5 {
                    let mut fb = BytesMut::from(&f[..]);
                    if let Ok(v5::Packet::ConnAck(a)) = v5::Packet::read(&mut fb, None) {
                        let p = a.properties;
                        break (
                            p.as_ref()
                                .and_then(|p| p.assigned_client_identifier.clone()),
                            p.as_ref().and_then(|p| p.max_packet_size),
                        );
                    }
                }
                break (None, None);
            }
            match r.read_buf(&mut buf).await {
                Ok(0) => return Err("closed".into()),
                Err(e) => return Err(e.to_string()),
                Ok(_) => {}
            }
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let subacks: Arc<std::sync::Mutex<Vec<oneshot::Sender<Vec<u8>>>>> = Default::default();
        let (w2, subs2, v5) = (w.clone(), subacks.clone(), o.v5);
        let task = tokio::spawn(async move {
            loop {
                while let Some(p) = parse(&mut buf, v5) {
                    match p {
                        Pkt::Publish {
                            topic,
                            payload,
                            qos,
                            retain,
                            pkid,
                            props,
                        } => {
                            let mut ack = BytesMut::new();
                            if qos == 1 {
                                ack.extend_from_slice(&[0x40, 0x02]);
                                ack.extend_from_slice(&pkid.to_be_bytes());
                            } else if qos == 2 {
                                ack.extend_from_slice(&[0x50, 0x02]);
                                ack.extend_from_slice(&pkid.to_be_bytes());
                            }
                            if !ack.is_empty() {
                                write_packet(&w2, &ack).await;
                            }
                            let _ = tx.send(Msg {
                                topic,
                                payload,
                                qos,
                                retain,
                                props,
                            });
                        }
                        Pkt::PubRel(id) => {
                            write_packet(&w2, &[0x70, 0x02, (id >> 8) as u8, id as u8]).await
                        }
                        Pkt::PubRec(id) => {
                            write_packet(&w2, &[0x62, 0x02, (id >> 8) as u8, id as u8]).await
                        }
                        Pkt::SubAck(codes) => {
                            let waiter = subs2.lock().ok().and_then(|mut v| {
                                if v.is_empty() {
                                    None
                                } else {
                                    Some(v.remove(0))
                                }
                            });
                            if let Some(wt) = waiter {
                                let _ = wt.send(codes);
                            }
                        }
                        _ => {}
                    }
                }
                match r.read_buf(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
            }
        });
        Ok(Client {
            w,
            v5: o.v5,
            rx,
            assigned_id,
            max_packet,
            subacks,
            next_pkid: std::sync::atomic::AtomicU16::new(1),
            task,
        })
    };
    tokio::time::timeout(T, fut)
        .await
        .unwrap_or_else(|_| Err("timeout".into()))
}

/// Connects with MQTT 3.1.1 options (alias of [`connect`]).
pub async fn c4(o: Opts) -> Result<Client, String> {
    connect(o).await
}

/// Connects with MQTT 5 options (alias of [`connect`]).
pub async fn c5(o: Opts) -> Result<Client, String> {
    connect(o).await
}

fn q4(q: u8) -> rumqttc::mqttbytes::QoS {
    match q {
        0 => rumqttc::mqttbytes::QoS::AtMostOnce,
        1 => rumqttc::mqttbytes::QoS::AtLeastOnce,
        _ => rumqttc::mqttbytes::QoS::ExactlyOnce,
    }
}

fn q5(q: u8) -> rumqttc::v5::mqttbytes::QoS {
    match q {
        0 => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
        1 => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        _ => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
    }
}

impl Client {
    fn pkid(&self) -> u16 {
        let p = self
            .next_pkid
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if p == 0 {
            self.next_pkid
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        } else {
            p
        }
    }

    /// Subscribes and waits for SUBACK; returns the granted codes.
    pub async fn subscribe(&self, filter: &str, qos: u8) -> Vec<u8> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut v) = self.subacks.lock() {
            v.push(tx);
        }
        let mut b = BytesMut::new();
        if self.v5 {
            let mut s = v5::Subscribe::new(v5::Filter::new(filter, q5(qos)), None);
            s.pkid = self.pkid();
            v5::Packet::Subscribe(s)
                .write(&mut b, None)
                .expect("encode");
        } else {
            let mut s = v4::Subscribe::new(filter, q4(qos));
            s.pkid = self.pkid();
            v4::Packet::Subscribe(s)
                .write(&mut b, usize::MAX)
                .expect("encode");
        }
        write_packet(&self.w, &b).await;
        tokio::time::timeout(T, rx)
            .await
            .expect("SUBACK")
            .expect("SUBACK")
    }

    /// Publishes (QoS 1 and 2 handshakes are completed by the reader task).
    pub async fn publish(
        &self,
        topic: &str,
        qos: u8,
        retain: bool,
        payload: impl AsRef<[u8]>,
        props: Option<v5::PublishProperties>,
    ) {
        let mut b = BytesMut::new();
        let pkid = if qos > 0 { self.pkid() } else { 0 };
        if self.v5 {
            let mut p = v5::Publish::new(topic, q5(qos), payload.as_ref().to_vec(), props);
            p.retain = retain;
            p.pkid = pkid;
            v5::Packet::Publish(p).write(&mut b, None).expect("encode");
        } else {
            let mut p = v4::Publish::new(topic, q4(qos), payload.as_ref().to_vec());
            p.retain = retain;
            p.pkid = pkid;
            v4::Packet::Publish(p)
                .write(&mut b, usize::MAX)
                .expect("encode");
        }
        write_packet(&self.w, &b).await;
    }

    /// Clean DISCONNECT.
    pub async fn disconnect(&self) {
        write_packet(&self.w, &[0xE0, 0x00]).await;
        let mut g = self.w.lock().await;
        let _ = g.shutdown().await;
    }
}

/// Waits for the next message on a receiver.
pub async fn next<P>(rx: &mut mpsc::UnboundedReceiver<P>) -> Option<P> {
    tokio::time::timeout(T, rx.recv()).await.ok().flatten()
}

/// Splits one complete frame off `buf`.
fn frame(buf: &mut BytesMut) -> Option<BytesMut> {
    if buf.len() < 2 {
        return None;
    }
    let mut len = 0usize;
    let mut shift = 0;
    let mut i = 1;
    loop {
        let b = *buf.get(i)?;
        len += ((b & 0x7f) as usize) << shift;
        i += 1;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    if buf.len() < i + len {
        return None;
    }
    Some(buf.split_to(i + len))
}

fn header_len(f: &[u8]) -> usize {
    let mut i = 1;
    while f.get(i).is_some_and(|b| b & 0x80 != 0) {
        i += 1;
    }
    i + 1
}

/// Parses the next complete packet from `buf`.
fn parse(buf: &mut BytesMut, v5: bool) -> Option<Pkt> {
    let mut f = frame(buf)?;
    let i = header_len(&f);
    let byte1 = f[0];
    Some(match byte1 >> 4 {
        2 => Pkt::ConnAck {
            code: f.get(i + 1).copied().unwrap_or(0xff),
            session_present: f.get(i).is_some_and(|b| b & 1 == 1),
        },
        14 => Pkt::Disconnect(f.get(i).copied().unwrap_or(0)),
        3 => {
            if v5 {
                match v5::Packet::read(&mut f, None) {
                    Ok(v5::Packet::Publish(p)) => Pkt::Publish {
                        topic: String::from_utf8_lossy(&p.topic).into_owned(),
                        payload: p.payload.to_vec(),
                        qos: p.qos as u8,
                        retain: p.retain,
                        pkid: p.pkid,
                        props: p.properties,
                    },
                    _ => Pkt::Other(byte1),
                }
            } else {
                match v4::Packet::read(&mut f, usize::MAX) {
                    Ok(v4::Packet::Publish(p)) => Pkt::Publish {
                        topic: p.topic,
                        payload: p.payload.to_vec(),
                        qos: p.qos as u8,
                        retain: p.retain,
                        pkid: p.pkid,
                        props: None,
                    },
                    _ => Pkt::Other(byte1),
                }
            }
        }
        5 => Pkt::PubRec(u16::from_be_bytes([f[i], f[i + 1]])),
        6 => Pkt::PubRel(u16::from_be_bytes([f[i], f[i + 1]])),
        9 => {
            let body = &f[i + 2..];
            let codes = if v5 {
                body.get(1 + body[0] as usize..)
                    .unwrap_or_default()
                    .to_vec()
            } else {
                body.to_vec()
            };
            Pkt::SubAck(codes)
        }
        _ => Pkt::Other(byte1),
    })
}

// ------------------------------------------------------------------ raw client

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<X: AsyncRead + AsyncWrite + Unpin + Send> Io for X {}

/// A minimal MQTT client on raw packets.
pub struct Raw {
    pub io: Box<dyn Io>,
    buf: BytesMut,
    pub v5: bool,
}

/// A packet from the broker, as seen by the raw client.
#[derive(Debug, Clone, PartialEq)]
pub enum Pkt {
    ConnAck {
        code: u8,
        session_present: bool,
    },
    Publish {
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
        pkid: u16,
        props: Option<v5::PublishProperties>,
    },
    SubAck(Vec<u8>),
    Disconnect(u8),
    PubRel(u16),
    PubRec(u16),
    Other(u8),
    Closed,
}

pub async fn tcp(port: u16) -> tokio::net::TcpStream {
    tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("tcp connect")
}

pub async fn tls_stream(
    port: u16,
    cfg: Arc<rustls::ClientConfig>,
) -> std::io::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    let s = tcp(port).await;
    let name = ServerName::try_from("localhost").expect("name").to_owned();
    tokio_rustls::TlsConnector::from(cfg).connect(name, s).await
}

pub async fn ws_upgrade<S: AsyncRead + AsyncWrite + Unpin + Send>(
    s: S,
    scheme: &str,
    port: u16,
    path: &str,
) -> Result<mqttpanel_broker::runtime::ws::WsBytes<S>, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = format!("{scheme}://localhost:{port}{path}")
        .into_client_request()
        .map_err(|e| e.to_string())?;
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", "mqtt".parse().expect("header"));
    let (ws, resp) = tokio_tungstenite::client_async(req, s)
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(
        resp.headers()
            .get("Sec-WebSocket-Protocol")
            .and_then(|v| v.to_str().ok()),
        Some("mqtt")
    );
    Ok(mqttpanel_broker::runtime::ws::WsBytes::new(ws))
}

impl Raw {
    pub fn new(io: Box<dyn Io>, v5: bool) -> Raw {
        Raw {
            io,
            buf: BytesMut::new(),
            v5,
        }
    }

    pub async fn tcp(port: u16, v5: bool) -> Raw {
        Raw::new(Box::new(tcp(port).await), v5)
    }

    pub async fn send_raw(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.io.write_all(bytes).await?;
        self.io.flush().await
    }

    pub async fn send4(&mut self, p: v4::Packet) {
        let mut b = BytesMut::new();
        p.write(&mut b, usize::MAX).expect("encode");
        let _ = self.send_raw(&b).await;
    }

    pub async fn send5(&mut self, p: v5::Packet) {
        let mut b = BytesMut::new();
        p.write(&mut b, None).expect("encode");
        let _ = self.send_raw(&b).await;
    }

    /// CONNECT and wait for CONNACK; returns the CONNACK reason code.
    pub async fn connect(
        &mut self,
        id: &str,
        login: Option<(&str, &str)>,
        will: Option<(&str, &str)>,
        keep_alive: u16,
    ) -> u8 {
        if self.v5 {
            let c = v5::Connect {
                keep_alive,
                client_id: id.into(),
                clean_start: true,
                properties: None,
            };
            let w = will.map(|(t, p)| {
                v5::LastWill::new(
                    t,
                    p.as_bytes().to_vec(),
                    rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
                    false,
                    None,
                )
            });
            let l = login.map(|(u, p)| v5::Login::new(u, p));
            self.send5(v5::Packet::Connect(c, w, l)).await;
        } else {
            let mut c = v4::Connect::new(id);
            c.keep_alive = keep_alive;
            c.last_will = will.map(|(t, p)| {
                v4::LastWill::new(t, p.as_bytes().to_vec(), rumqttc::QoS::AtLeastOnce, false)
            });
            c.login = login.map(|(u, p)| v4::Login::new(u, p));
            self.send4(v4::Packet::Connect(c)).await;
        }
        match self.read().await {
            Pkt::ConnAck { code, .. } => code,
            other => panic!("expected CONNACK, got {other:?}"),
        }
    }

    pub async fn subscribe(&mut self, filter: &str, qos: u8) -> Vec<u8> {
        if self.v5 {
            let q = match qos {
                0 => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
                1 => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
                _ => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
            };
            let f = v5::Filter::new(filter, q);
            self.send5(v5::Packet::Subscribe(v5::Subscribe::new(f, None)))
                .await;
        } else {
            let q = match qos {
                0 => rumqttc::QoS::AtMostOnce,
                1 => rumqttc::QoS::AtLeastOnce,
                _ => rumqttc::QoS::ExactlyOnce,
            };
            let mut s = v4::Subscribe::new(filter, q);
            s.pkid = 1;
            self.send4(v4::Packet::Subscribe(s)).await;
        }
        loop {
            match self.read().await {
                Pkt::SubAck(c) => return c,
                Pkt::Closed => panic!("closed while subscribing"),
                _ => {}
            }
        }
    }

    pub async fn publish(&mut self, topic: &str, payload: &[u8], retain: bool) {
        if self.v5 {
            let mut p = v5::Publish::new(
                topic,
                rumqttc::v5::mqttbytes::QoS::AtMostOnce,
                payload.to_vec(),
                None,
            );
            p.retain = retain;
            self.send5(v5::Packet::Publish(p)).await;
        } else {
            let mut p = v4::Publish::new(topic, rumqttc::QoS::AtMostOnce, payload.to_vec());
            p.retain = retain;
            self.send4(v4::Packet::Publish(p)).await;
        }
    }

    /// Next PUBLISH (QoS 1/2 acknowledged), or None on timeout/close.
    pub async fn next_publish(&mut self, timeout: Duration) -> Option<Pkt> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let p = tokio::time::timeout_at(deadline, self.read()).await.ok()?;
            match p {
                Pkt::Publish { .. } => return Some(p),
                Pkt::Closed => return None,
                _ => {}
            }
        }
    }

    /// Reads one packet (acks incoming QoS 1/2 publishes).
    pub async fn read(&mut self) -> Pkt {
        loop {
            if let Some(p) = self.try_frame() {
                if let Pkt::Publish { qos, pkid, .. } = &p {
                    let (q, id) = (*qos, *pkid);
                    if q == 1 {
                        if self.v5 {
                            self.send5(v5::Packet::PubAck(v5::PubAck::new(id, None)))
                                .await;
                        } else {
                            self.send4(v4::Packet::PubAck(v4::PubAck::new(id))).await;
                        }
                    } else if q == 2 {
                        if self.v5 {
                            self.send5(v5::Packet::PubRec(v5::PubRec::new(id, None)))
                                .await;
                        } else {
                            self.send4(v4::Packet::PubRec(v4::PubRec::new(id))).await;
                        }
                    }
                }
                if let Pkt::PubRel(id) = p {
                    if self.v5 {
                        self.send5(v5::Packet::PubComp(v5::PubComp::new(id, None)))
                            .await;
                    } else {
                        self.send4(v4::Packet::PubComp(v4::PubComp::new(id))).await;
                    }
                    continue;
                }
                return p;
            }
            match self.io.read_buf(&mut self.buf).await {
                Ok(0) | Err(_) => return Pkt::Closed,
                Ok(_) => {}
            }
        }
    }

    fn try_frame(&mut self) -> Option<Pkt> {
        parse(&mut self.buf, self.v5)
    }
}

/// A non-loopback local IPv4 address, if the machine has one (no packet is sent).
pub fn lan_ipv4() -> Option<std::net::Ipv4Addr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// Polls `cond` until true or 5 s pass.
pub async fn eventually(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + T;
    while tokio::time::Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    cond()
}
