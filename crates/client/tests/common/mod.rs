//! A scriptable fake broker for client tests: plain, TLS (chains, client certificates, revocation, ALPN,
//! TLS 1.2 only) and WebSocket, with configurable CONNACK codes, refusals and misbehaviour.
#![allow(dead_code, unused_imports, clippy::result_large_err)]

use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use rumqttc::mqttbytes::v4;
use rumqttc::v5::mqttbytes::v5;
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::WebPkiClientVerifier;
use rustls::{DigitallySignedStruct, DistinguishedName, RootCertStore, SignatureScheme};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub use mqttpanel_tls::kit::{TestKit, TEST_PASSPHRASE};

pub fn kit() -> &'static TestKit {
    static K: OnceLock<TestKit> = OnceLock::new();
    K.get_or_init(|| TestKit::generate().expect("test kit"))
}

#[derive(Clone)]
pub struct ServerTls {
    pub chain: Vec<Vec<u8>>,
    pub key: Vec<u8>, // PKCS#8
    pub client_ca: Option<Vec<u8>>,
    pub require_client_cert: bool,
    pub revoked_serials: Vec<String>,
    pub require_alpn: Option<String>,
    pub tls12_only: bool,
}

impl ServerTls {
    pub fn new(chain: Vec<Vec<u8>>, key: Vec<u8>) -> Self {
        ServerTls { chain, key, client_ca: None, require_client_cert: false, revoked_serials: vec![], require_alpn: None, tls12_only: false }
    }
}

#[derive(Clone, Default)]
pub struct Behavior {
    /// CONNACK return code for 3.1.1 (0 = accepted).
    pub connack_v4: u8,
    /// CONNACK reason code for 5 (0 = success).
    pub connack_v5: u8,
    /// Accept only 3.1.1: a v5 CONNECT gets a v3 CONNACK 0x01.
    pub v311_only: bool,
    /// Close right after reading CONNECT, without CONNACK.
    pub close_on_connect: bool,
    pub max_packet_size: Option<u32>,
    pub ignore_ping: bool,
    /// Send a v5 DISCONNECT with this reason this many ms after CONNACK.
    pub disconnect_after: Option<(u64, u8)>,
    /// Username/password required: (user, pass).
    pub login: Option<(String, String)>,
}

#[derive(Clone, Default)]
pub struct FakeConfig {
    pub tls: Option<ServerTls>,
    /// Serve MQTT over WebSocket on this path (others answer 404).
    pub ws_path: Option<String>,
    pub behavior: Behavior,
}

pub struct FakeBroker {
    pub port: u16,
    pub connects: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for FakeBroker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Debug)]
struct RevocationVerifier {
    inner: Arc<dyn ClientCertVerifier>,
    revoked: Vec<String>,
}

impl ClientCertVerifier for RevocationVerifier {
    fn offer_client_auth(&self) -> bool {
        self.inner.offer_client_auth()
    }
    fn client_auth_mandatory(&self) -> bool {
        self.inner.client_auth_mandatory()
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }
    fn verify_client_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], now: UnixTime) -> Result<ClientCertVerified, rustls::Error> {
        if let Ok(d) = mqttpanel_tls::cert_details(end_entity.as_ref()) {
            if self.revoked.contains(&d.serial) {
                return Err(rustls::Error::InvalidCertificate(rustls::CertificateError::Revoked));
            }
        }
        self.inner.verify_client_cert(end_entity, intermediates, now)
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(m, c, d)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(m, c, d)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn server_config(t: &ServerTls) -> Arc<rustls::ServerConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let versions: &[&rustls::SupportedProtocolVersion] = if t.tls12_only { &[&rustls::version::TLS12] } else { &[&rustls::version::TLS13, &rustls::version::TLS12] };
    let builder = rustls::ServerConfig::builder_with_provider(provider.clone()).with_protocol_versions(versions).unwrap();
    let builder = match &t.client_ca {
        Some(ca) => {
            let mut roots = RootCertStore::empty();
            for c in mqttpanel_tls::parse_certs(ca).unwrap() {
                roots.add(c).unwrap();
            }
            let b = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider);
            let inner = if t.require_client_cert { b.build().unwrap() } else { b.allow_unauthenticated().build().unwrap() };
            builder.with_client_cert_verifier(Arc::new(RevocationVerifier { inner, revoked: t.revoked_serials.clone() }))
        }
        None => builder.with_no_client_auth(),
    };
    let chain: Vec<CertificateDer<'static>> = t.chain.iter().map(|c| CertificateDer::from(c.clone())).collect();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(t.key.clone()));
    let mut cfg = builder.with_single_cert(chain, key).unwrap();
    if let Some(a) = &t.require_alpn {
        cfg.alpn_protocols = vec![a.as_bytes().to_vec()];
    }
    Arc::new(cfg)
}

/// Starts a fake broker on a random port.
pub async fn start(cfg: FakeConfig) -> FakeBroker {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connects = Arc::new(Mutex::new(Vec::new()));
    let c2 = connects.clone();
    let tls_cfg = cfg.tls.as_ref().map(server_config);
    let task = tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { return };
            let cfg = cfg.clone();
            let tls_cfg = tls_cfg.clone();
            let connects = c2.clone();
            tokio::spawn(async move {
                let _ = handle(tcp, cfg, tls_cfg, connects).await;
            });
        }
    });
    FakeBroker { port, connects, task }
}

async fn handle(tcp: TcpStream, cfg: FakeConfig, tls_cfg: Option<Arc<rustls::ServerConfig>>, connects: Arc<Mutex<Vec<String>>>) -> std::io::Result<()> {
    if let Some(tc) = tls_cfg {
        if let Some(alpn) = cfg.tls.as_ref().and_then(|t| t.require_alpn.clone()) {
            // Refuse a ClientHello without ALPN with a fatal no_application_protocol alert.
            let mut peek = vec![0u8; 4096];
            let n = tcp.peek(&mut peek).await?;
            let mut acceptor = rustls::server::Acceptor::default();
            let mut cur = std::io::Cursor::new(&peek[..n]);
            let _ = acceptor.read_tls(&mut cur);
            if let Ok(Some(accepted)) = acceptor.accept() {
                let offered = accepted.client_hello().alpn().map(|mut it| it.any(|p| p == alpn.as_bytes())).unwrap_or(false);
                if !offered {
                    let mut tcp = tcp;
                    let mut sink = vec![0u8; n];
                    tcp.read_exact(&mut sink).await?;
                    tcp.write_all(&[0x15, 0x03, 0x03, 0x00, 0x02, 0x02, 0x78]).await?;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    return Ok(());
                }
            }
        }
        let acceptor = tokio_rustls::TlsAcceptor::from(tc);
        let tls = acceptor.accept(tcp).await?;
        let cn = tls.get_ref().1.peer_certificates().and_then(|c| c.first()).and_then(|c| mqttpanel_tls::cert_details(c.as_ref()).ok()).and_then(|d| d.cn);
        if let Some(cn) = cn {
            connects.lock().unwrap().push(format!("cert:{cn}"));
        }
        return serve_maybe_ws(tls, cfg, connects).await;
    }
    serve_maybe_ws(tcp, cfg, connects).await
}

async fn serve_maybe_ws<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(s: S, cfg: FakeConfig, connects: Arc<Mutex<Vec<String>>>) -> std::io::Result<()> {
    if let Some(path) = cfg.ws_path.clone() {
        use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
        let cb = move |req: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
            if req.uri().path() != path {
                let mut e = ErrorResponse::new(Some("not found".into()));
                *e.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::NOT_FOUND;
                return Err(e);
            }
            resp.headers_mut().insert("Sec-WebSocket-Protocol", "mqtt".parse().unwrap());
            Ok(resp)
        };
        let ws = match tokio_tungstenite::accept_hdr_async(s, cb).await {
            Ok(ws) => ws,
            Err(_) => return Ok(()),
        };
        let (tx_in, rx_in) = tokio::io::duplex(1 << 20);
        let (mut rd, mut wr) = tokio::io::split(tx_in);
        let (mut sink, mut stream) = ws.split();
        tokio::spawn(async move {
            while let Some(Ok(m)) = stream.next().await {
                if let tokio_tungstenite::tungstenite::Message::Binary(b) = m {
                    if wr.write_all(&b).await.is_err() {
                        break;
                    }
                }
            }
        });
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                match rd.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sink.send(tokio_tungstenite::tungstenite::Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        return serve(rx_in, cfg.behavior, connects).await;
    }
    serve(s, cfg.behavior, connects).await
}

fn protocol_level(buf: &[u8]) -> Option<u8> {
    // fixed header byte, remaining length (1..4 bytes), then 0x00 0x04 'M' 'Q' 'T' 'T' level
    let mut i = 1;
    while i < buf.len() && buf[i] & 0x80 != 0 {
        i += 1;
    }
    let level_at = i + 1 + 6;
    buf.get(level_at).copied()
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(mut s: S, b: Behavior, connects: Arc<Mutex<Vec<String>>>) -> std::io::Result<()> {
    let mut buf = BytesMut::with_capacity(8192);
    while buf.len() < 10 {
        if s.read_buf(&mut buf).await? == 0 {
            return Ok(());
        }
    }
    let level = protocol_level(&buf).unwrap_or(4);
    if b.close_on_connect {
        return Ok(());
    }
    let v5 = level == 5;
    if v5 && b.v311_only {
        s.write_all(&[0x20, 0x02, 0x00, 0x01]).await?;
        return Ok(());
    }
    let mut w = BytesMut::new();
    // Read the CONNECT fully.
    let (client_id, login) = loop {
        if v5 {
            match v5::Packet::read(&mut buf, None) {
                Ok(v5::Packet::Connect(c, _, l)) => break (c.client_id, l.map(|l| (l.username, l.password))),
                Ok(_) => return Ok(()),
                Err(rumqttc::v5::mqttbytes::Error::InsufficientBytes(_)) => {}
                Err(_) => return Ok(()),
            }
        } else {
            match v4::Packet::read(&mut buf, 1 << 20) {
                Ok(v4::Packet::Connect(c)) => break (c.client_id, c.login.map(|l| (l.username, l.password))),
                Ok(_) => return Ok(()),
                Err(rumqttc::mqttbytes::Error::InsufficientBytes(_)) => {}
                Err(_) => return Ok(()),
            }
        }
        if s.read_buf(&mut buf).await? == 0 {
            return Ok(());
        }
    };
    connects.lock().unwrap().push(format!("{}:{}", if v5 { 5 } else { 4 }, client_id));
    let mut code4 = b.connack_v4;
    let mut code5 = b.connack_v5;
    if let Some((u, p)) = &b.login {
        if login.as_ref() != Some(&(u.clone(), p.clone())) {
            code4 = 4;
            code5 = 0x86;
        }
    }
    if v5 {
        let props = v5::ConnAckProperties {
            session_expiry_interval: None,
            receive_max: None,
            max_qos: None,
            retain_available: None,
            max_packet_size: b.max_packet_size,
            assigned_client_identifier: None,
            topic_alias_max: None,
            reason_string: None,
            user_properties: vec![],
            wildcard_subscription_available: None,
            subscription_identifiers_available: None,
            shared_subscription_available: None,
            server_keep_alive: None,
            response_information: None,
            server_reference: None,
            authentication_method: None,
            authentication_data: None,
        };
        let code = code5_of(code5);
        v5::Packet::ConnAck(v5::ConnAck { session_present: false, code, properties: Some(props) }).write(&mut w, None).unwrap();
    } else {
        w.extend_from_slice(&[0x20, 0x02, 0x00, code4]);
    }
    s.write_all(&w).await?;
    if (v5 && code5 != 0) || (!v5 && code4 != 0) {
        return Ok(());
    }
    let mut subs: Vec<String> = vec![];
    let mut retained: HashMap<String, Vec<u8>> = HashMap::new();
    let deadline = b.disconnect_after.map(|(ms, _)| tokio::time::Instant::now() + Duration::from_millis(ms));
    loop {
        // Process buffered packets
        loop {
            w.clear();
            if v5 {
                let p = match v5::Packet::read(&mut buf, None) {
                    Ok(p) => p,
                    Err(rumqttc::v5::mqttbytes::Error::InsufficientBytes(_)) => break,
                    Err(_) => return Ok(()),
                };
                match p {
                    v5::Packet::Subscribe(sub) => {
                        let mut codes = vec![];
                        for f in &sub.filters {
                            if f.path.starts_with("forbidden") {
                                codes.push(v5::SubscribeReasonCode::NotAuthorized);
                            } else {
                                subs.push(f.path.clone());
                                codes.push(v5::SubscribeReasonCode::Success(f.qos));
                            }
                        }
                        v5::Packet::SubAck(v5::SubAck { pkid: sub.pkid, return_codes: codes, properties: None }).write(&mut w, None).unwrap();
                        for f in &sub.filters {
                            for (t, pl) in &retained {
                                if mqttpanel_client::topic_matches(&f.path, t) {
                                    let mut p = v5::Publish::new(t.clone(), rumqttc::v5::mqttbytes::QoS::AtMostOnce, pl.clone(), None);
                                    p.retain = true;
                                    v5::Packet::Publish(p).write(&mut w, None).unwrap();
                                }
                            }
                        }
                    }
                    v5::Packet::Publish(p) => {
                        let topic = String::from_utf8_lossy(&p.topic).to_string();
                        let forbidden = topic.starts_with("forbidden/");
                        match p.qos as u8 {
                            1 => {
                                let mut ack = v5::PubAck::new(p.pkid, None);
                                if forbidden {
                                    ack.reason = v5::PubAckReason::NotAuthorized;
                                }
                                v5::Packet::PubAck(ack).write(&mut w, None).unwrap();
                            }
                            2 => {
                                let mut rec = v5::PubRec::new(p.pkid, None);
                                if forbidden {
                                    rec.reason = v5::PubRecReason::NotAuthorized;
                                }
                                v5::Packet::PubRec(rec).write(&mut w, None).unwrap();
                            }
                            _ => {}
                        }
                        if !forbidden {
                            if p.retain {
                                if p.payload.is_empty() {
                                    retained.remove(&topic);
                                } else {
                                    retained.insert(topic.clone(), p.payload.to_vec());
                                }
                            }
                            if subs.iter().any(|f| mqttpanel_client::topic_matches(f, &topic)) {
                                let mut out = v5::Publish::new(topic.clone(), rumqttc::v5::mqttbytes::QoS::AtMostOnce, p.payload.clone(), p.properties.clone());
                                out.retain = false;
                                v5::Packet::Publish(out).write(&mut w, None).unwrap();
                            }
                        }
                    }
                    v5::Packet::PubRel(r) => v5::Packet::PubComp(v5::PubComp::new(r.pkid, None)).write(&mut w, None).map(|_| ()).unwrap(),
                    v5::Packet::PingReq(_) => {
                        if !b.ignore_ping {
                            v5::Packet::PingResp(v5::PingResp).write(&mut w, None).unwrap();
                        }
                    }
                    v5::Packet::Disconnect(_) => return Ok(()),
                    _ => {}
                }
            } else {
                let p = match v4::Packet::read(&mut buf, 1 << 20) {
                    Ok(p) => p,
                    Err(rumqttc::mqttbytes::Error::InsufficientBytes(_)) => break,
                    Err(_) => return Ok(()),
                };
                match p {
                    v4::Packet::Subscribe(sub) => {
                        let mut codes = vec![];
                        for f in &sub.filters {
                            if f.path.starts_with("forbidden") {
                                codes.push(v4::SubscribeReasonCode::Failure);
                            } else {
                                subs.push(f.path.clone());
                                codes.push(v4::SubscribeReasonCode::Success(f.qos));
                            }
                        }
                        v4::Packet::SubAck(v4::SubAck::new(sub.pkid, codes)).write(&mut w, 1 << 20).unwrap();
                        for f in &sub.filters {
                            for (t, pl) in &retained {
                                if mqttpanel_client::topic_matches(&f.path, t) {
                                    let mut p = v4::Publish::new(t.clone(), rumqttc::QoS::AtMostOnce, pl.clone());
                                    p.retain = true;
                                    v4::Packet::Publish(p).write(&mut w, 1 << 20).unwrap();
                                }
                            }
                        }
                    }
                    v4::Packet::Publish(p) => {
                        match p.qos as u8 {
                            1 => {
                                v4::Packet::PubAck(v4::PubAck::new(p.pkid)).write(&mut w, 1 << 20).unwrap();
                            }
                            2 => {
                                v4::Packet::PubRec(v4::PubRec::new(p.pkid)).write(&mut w, 1 << 20).unwrap();
                            }
                            _ => {}
                        }
                        if p.retain {
                            if p.payload.is_empty() {
                                retained.remove(&p.topic);
                            } else {
                                retained.insert(p.topic.clone(), p.payload.to_vec());
                            }
                        }
                        if subs.iter().any(|f| mqttpanel_client::topic_matches(f, &p.topic)) {
                            let out = v4::Publish::new(p.topic.clone(), rumqttc::QoS::AtMostOnce, p.payload.to_vec());
                            v4::Packet::Publish(out).write(&mut w, 1 << 20).unwrap();
                        }
                    }
                    v4::Packet::PubRel(r) => {
                        v4::Packet::PubComp(v4::PubComp::new(r.pkid)).write(&mut w, 1 << 20).unwrap();
                    }
                    v4::Packet::PingReq => {
                        if !b.ignore_ping {
                            v4::Packet::PingResp.write(&mut w, 1 << 20).unwrap();
                        }
                    }
                    v4::Packet::Disconnect => return Ok(()),
                    _ => {}
                }
            }
            if !w.is_empty() {
                s.write_all(&w).await?;
            }
        }
        let sleep = async {
            match deadline {
                Some(d) => tokio::time::sleep_until(d).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            r = s.read_buf(&mut buf) => { if r? == 0 { return Ok(()) } }
            _ = sleep => {
                if let Some((_, reason)) = b.disconnect_after {
                    w.clear();
                    let code = disconnect_code(reason);
                    let _ = code;
                    w.extend_from_slice(&[0xE0, 0x02, reason, 0x00]);
                    s.write_all(&w).await?;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    return Ok(());
                }
            }
        }
    }
}

fn code5_of(c: u8) -> v5::ConnectReturnCode {
    use v5::ConnectReturnCode as C;
    match c {
        0 => C::Success,
        0x84 => C::UnsupportedProtocolVersion,
        0x85 => C::ClientIdentifierNotValid,
        0x86 => C::BadUserNamePassword,
        0x87 => C::NotAuthorized,
        0x88 => C::ServerUnavailable,
        0x89 => C::ServerBusy,
        0x95 => C::PacketTooLarge,
        _ => C::UnspecifiedError,
    }
}

fn disconnect_code(c: u8) -> v5::DisconnectReasonCode {
    use v5::DisconnectReasonCode as D;
    match c {
        0x8E => D::SessionTakenOver,
        0x8B => D::ServerShuttingDown,
        0x87 => D::NotAuthorized,
        _ => D::UnspecifiedError,
    }
}

/// A port with nothing listening.
pub async fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

pub fn tls_opts(port: u16) -> mqttpanel_client::ConnectOptions {
    let mut o = mqttpanel_client::ConnectOptions::new("localhost", port);
    o.scheme = mqttpanel_client::Scheme::Mqtts;
    o.connect_timeout = Duration::from_secs(5);
    o.reconnect = false;
    o
}

pub fn plain_opts(port: u16) -> mqttpanel_client::ConnectOptions {
    let mut o = mqttpanel_client::ConnectOptions::new("localhost", port);
    o.connect_timeout = Duration::from_secs(5);
    o.reconnect = false;
    o
}
