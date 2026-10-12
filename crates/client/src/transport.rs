//! Transport: TCP, TLS, WebSocket and secure WebSocket, with every failure turned into a plain sentence or a trust prompt.

use crate::catalog::UserMessage;
use crate::msg;
use crate::options::ConnectOptions;
use crate::tlsconf::{self, Captured};
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use mqttpanel_tls::{cert_details, CertDetails};
use rustls::{AlertDescription, CertificateError};
use rustls_pki_types::ServerName;
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest, Message};
use tokio_tungstenite::WebSocketStream;

/// Why a certificate needs the user's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrustReason {
    SelfSigned,
    UnknownCa,
    Expired,
    NotYetValid,
    FingerprintChanged,
}

/// Everything the trust prompt shows (see the TS type TrustPrompt; profileId is added by the app).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustInfo {
    pub reason: TrustReason,
    pub reason_text: UserMessage,
    pub leaf: CertDetails,
    pub chain: Vec<CertDetails>,
    pub ca_offer: bool,
    pub previous_fingerprint: Option<String>,
    #[serde(rename = "unsafe")]
    pub unsafe_: bool,
    /// DER of the CA offered by "Trust anything this CA signs".
    #[serde(skip)]
    pub ca_der: Option<Vec<u8>>,
}

/// A failed connection attempt.
#[derive(Debug, Clone)]
pub enum ConnectFailure {
    /// A plain sentence for the user.
    Message(UserMessage),
    /// The certificate needs the user's decision.
    Trust(Box<TrustInfo>),
}

impl ConnectFailure {
    pub fn message(&self) -> UserMessage {
        match self {
            ConnectFailure::Message(m) => m.clone(),
            ConnectFailure::Trust(t) => t.reason_text.clone(),
        }
    }
    /// Failures that will not go away by retrying (wrong certificate, refused login, bad configuration).
    pub fn is_permanent(&self) -> bool {
        match self {
            ConnectFailure::Trust(_) => true,
            ConnectFailure::Message(m) => m.key.starts_with("tls.")
                || matches!(
                    m.key.as_str(),
                    "error.badLogin" | "error.badLoginAnonymous" | "error.notAuthorized" | "error.clientIdRejected" | "error.protocolVersion" | "error.dns" | "error.wsHandshake" | "error.wsProtocol" | "error.tlsOnPlain" | "error.plainOnTls"
                ),
        }
    }
}

impl From<UserMessage> for ConnectFailure {
    fn from(m: UserMessage) -> Self {
        ConnectFailure::Message(m)
    }
}

/// An open byte transport.
pub enum Conn {
    Tcp(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Ws(Box<WebSocketStream<TcpStream>>),
    Wss(Box<WebSocketStream<TlsStream<TcpStream>>>),
}

/// What the transport learned while connecting (shown by Try connect and Properties).
#[derive(Debug, Clone, Default)]
pub struct TransportInfo {
    pub server_cert: Option<CertDetails>,
    pub chain: Vec<CertDetails>,
    pub tls_version: Option<String>,
    pub verified: Option<bool>,
}

impl Conn {
    /// Reads more bytes into `buf`; 0 means the peer closed. Cancel-safe.
    pub async fn read_into(&mut self, buf: &mut BytesMut) -> io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.read_buf(buf).await,
            Conn::Tls(s) => s.read_buf(buf).await,
            Conn::Ws(s) => ws_read(s.as_mut(), buf).await,
            Conn::Wss(s) => ws_read(s.as_mut(), buf).await,
        }
    }

    pub async fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        match self {
            Conn::Tcp(s) => {
                s.write_all(data).await?;
                s.flush().await
            }
            Conn::Tls(s) => {
                s.write_all(data).await?;
                s.flush().await
            }
            Conn::Ws(s) => s.send(Message::Binary(data.to_vec().into())).await.map_err(ws_io),
            Conn::Wss(s) => s.send(Message::Binary(data.to_vec().into())).await.map_err(ws_io),
        }
    }

    pub async fn shutdown(&mut self) {
        let _ = match self {
            Conn::Tcp(s) => s.shutdown().await,
            Conn::Tls(s) => s.shutdown().await,
            Conn::Ws(s) => SinkExt::close(s.as_mut()).await.map_err(ws_io),
            Conn::Wss(s) => SinkExt::close(s.as_mut()).await.map_err(ws_io),
        };
    }

    pub fn is_tls(&self) -> bool {
        matches!(self, Conn::Tls(_) | Conn::Wss(_))
    }
}

async fn ws_read<S>(s: &mut WebSocketStream<S>, buf: &mut BytesMut) -> io::Result<usize>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        match s.next().await {
            None => return Ok(0),
            Some(Ok(Message::Binary(b))) => {
                buf.extend_from_slice(&b);
                if !b.is_empty() {
                    return Ok(b.len());
                }
            }
            Some(Ok(Message::Close(_))) => return Ok(0),
            Some(Ok(_)) => continue,
            Some(Err(e)) => return Err(ws_io(e)),
        }
    }
}

fn ws_io(e: tungstenite::Error) -> io::Error {
    match e {
        tungstenite::Error::Io(e) => e,
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => io::Error::new(io::ErrorKind::UnexpectedEof, "closed"),
        other => io::Error::other(other.to_string()),
    }
}

/// Opens TCP with DNS, refused and timeout classification.
pub async fn tcp_connect(opts: &ConnectOptions) -> Result<TcpStream, UserMessage> {
    let host = opts.host.trim().trim_start_matches('[').trim_end_matches(']').to_string();
    let deadline = Instant::now() + opts.connect_timeout;
    let addrs: Vec<std::net::SocketAddr> = match tokio::time::timeout(opts.connect_timeout, tokio::net::lookup_host((host.as_str(), opts.port))).await {
        Ok(Ok(a)) => a.collect(),
        Ok(Err(_)) => return Err(msg!("error.dns", host = host)),
        Err(_) => return Err(msg!("error.dns", host = host)),
    };
    if addrs.is_empty() {
        return Err(msg!("error.dns", host = host));
    }
    let mut last_refused = false;
    for addr in addrs {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, TcpStream::connect(addr)).await {
            Ok(Ok(s)) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Ok(Err(e)) if e.kind() == io::ErrorKind::ConnectionRefused => last_refused = true,
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }
    if last_refused {
        Err(msg!("error.refused", host = host, port = opts.port))
    } else if Instant::now() >= deadline {
        Err(msg!("error.timeout", host = host, port = opts.port, seconds = opts.connect_timeout.as_secs()))
    } else {
        Err(msg!("error.network", host = host, port = opts.port))
    }
}

fn server_name(opts: &ConnectOptions) -> ServerName<'static> {
    let sni = opts.tls.sni.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let host = sni.unwrap_or(opts.host.trim().trim_start_matches('[').trim_end_matches(']'));
    ServerName::try_from(host.to_string()).unwrap_or_else(|_| ServerName::try_from("localhost".to_string()).expect("static name"))
}

/// Connects the transport for the options. The returned info carries the server certificate for TLS.
pub async fn connect(opts: &ConnectOptions) -> Result<(Conn, TransportInfo), ConnectFailure> {
    let tcp = tcp_connect(opts).await?;
    let mut info = TransportInfo::default();
    if !opts.scheme.is_tls() {
        if opts.scheme.is_ws() {
            let ws = ws_handshake(tcp, opts, "ws").await?;
            return Ok((Conn::Ws(Box::new(ws)), info));
        }
        return Ok((Conn::Tcp(tcp), info));
    }
    let (config, captured) = tlsconf::client_config(&opts.tls).map_err(|e| match e {
        tlsconf::ConfigError::BadCa => msg!("file.notCertificate"),
        tlsconf::ConfigError::BadIdentity => msg!("tls.keyMismatch"),
    })?;
    let connector = tokio_rustls::TlsConnector::from(config);
    let left = opts.connect_timeout;
    let tls = match tokio::time::timeout(left, connector.connect(server_name(opts), tcp)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(classify_tls_io(&e, &captured, opts)),
        Err(_) => return Err(msg!("error.timeout", host = opts.host, port = opts.port, seconds = left.as_secs()).into()),
    };
    {
        let cap = captured.lock().unwrap_or_else(|e| e.into_inner());
        info.chain = cap.chain.iter().filter_map(|c| cert_details(c.as_ref()).ok()).collect();
        info.server_cert = info.chain.first().cloned();
        info.verified = Some(!cap.accepted_by_trust);
    }
    info.tls_version = tls.get_ref().1.protocol_version().map(|v| match v {
        rustls::ProtocolVersion::TLSv1_3 => "TLS 1.3".to_string(),
        rustls::ProtocolVersion::TLSv1_2 => "TLS 1.2".to_string(),
        other => format!("{other:?}"),
    });
    if opts.scheme.is_ws() {
        let ws = ws_handshake(tls, opts, "wss").await?;
        return Ok((Conn::Wss(Box::new(ws)), info));
    }
    Ok((Conn::Tls(Box::new(tls)), info))
}

async fn ws_handshake<S>(stream: S, opts: &ConnectOptions, scheme: &str) -> Result<WebSocketStream<S>, ConnectFailure>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let path = if opts.ws_path.starts_with('/') { opts.ws_path.clone() } else { format!("/{}", opts.ws_path) };
    let host = if opts.host.contains(':') && !opts.host.starts_with('[') { format!("[{}]", opts.host) } else { opts.host.clone() };
    let url = format!("{scheme}://{host}:{}{path}", opts.port);
    let mut req = url.as_str().into_client_request().map_err(|_| msg!("error.wsProtocol", status = 0))?;
    req.headers_mut().insert("Sec-WebSocket-Protocol", tungstenite::http::HeaderValue::from_static("mqtt"));
    match tokio::time::timeout(opts.connect_timeout, tokio_tungstenite::client_async(req, stream)).await {
        Ok(Ok((ws, _resp))) => Ok(ws),
        Ok(Err(tungstenite::Error::Http(resp))) => {
            let status = resp.status().as_u16();
            if status == 404 || status == 400 || status == 403 {
                Err(msg!("error.wsHandshake", path = path, status = status).into())
            } else {
                Err(msg!("error.wsProtocol", status = status).into())
            }
        }
        Ok(Err(tungstenite::Error::Io(e))) => Err(classify_plain_io(&e, opts).into()),
        Ok(Err(tungstenite::Error::Tls(_))) => Err(msg!("tls.handshake", host = opts.host, port = opts.port, detail = "TLS").into()),
        Ok(Err(tungstenite::Error::Protocol(_))) | Ok(Err(tungstenite::Error::ConnectionClosed)) => {
            Err(msg!("error.wsProtocol", status = 0).into())
        }
        Ok(Err(_)) => Err(msg!("error.wsProtocol", status = 0).into()),
        Err(_) => Err(msg!("error.timeout", host = opts.host, port = opts.port, seconds = opts.connect_timeout.as_secs()).into()),
    }
}

/// An I/O error on a plain connection.
pub fn classify_plain_io(e: &io::Error, opts: &ConnectOptions) -> UserMessage {
    match e.kind() {
        io::ErrorKind::ConnectionRefused => msg!("error.refused", host = opts.host, port = opts.port),
        io::ErrorKind::TimedOut => msg!("error.timeout", host = opts.host, port = opts.port, seconds = opts.connect_timeout.as_secs()),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionAborted => {
            msg!("error.closed", host = opts.host, port = opts.port)
        }
        _ => msg!("error.network", host = opts.host, port = opts.port),
    }
}

/// Finds a rustls error inside an I/O error.
pub fn rustls_error(e: &io::Error) -> Option<rustls::Error> {
    let inner = e.get_ref()?;
    inner.downcast_ref::<rustls::Error>().cloned()
}

/// An I/O error during or after the TLS handshake.
pub fn classify_tls_io(e: &io::Error, captured: &Arc<Mutex<Captured>>, opts: &ConnectOptions) -> ConnectFailure {
    match rustls_error(e) {
        Some(err) => classify_tls_error(&err, captured, opts),
        None => match e.kind() {
            // The peer closed during the handshake: most often a plain MQTT port.
            io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe => {
                msg!("error.tlsOnPlain", host = opts.host, port = opts.port).into()
            }
            _ => classify_plain_io(e, opts).into(),
        },
    }
}

fn date_of(unix: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(unix)
        .map(|t| format!("{:04}-{:02}-{:02}", t.year(), u8::from(t.month()), t.day()))
        .unwrap_or_default()
}

/// Maps a rustls error to a sentence or a trust prompt, using the chain the server presented.
pub fn classify_tls_error(err: &rustls::Error, captured: &Arc<Mutex<Captured>>, opts: &ConnectOptions) -> ConnectFailure {
    let cap = captured.lock().unwrap_or_else(|e| e.into_inner());
    let chain: Vec<CertDetails> = cap.chain.iter().filter_map(|c| cert_details(c.as_ref()).ok()).collect();
    let host = opts.host.clone();
    let port = opts.port;
    let has_identity = opts.tls.identity.is_some();
    let our_cn = opts
        .tls
        .identity
        .as_ref()
        .and_then(|i| i.certs.first())
        .and_then(|c| cert_details(c.as_ref()).ok())
        .and_then(|d| d.cn)
        .unwrap_or_default();
    match err {
        rustls::Error::InvalidCertificate(ce) => {
            let Some(leaf) = chain.first().cloned() else {
                return msg!("tls.handshake", host = host, port = port, detail = "no certificate").into();
            };
            let trust = |reason: TrustReason, text: UserMessage, unsafe_: bool| -> ConnectFailure {
                let last = chain.last().cloned();
                let ca_offer = chain.len() > 1 && last.as_ref().is_some_and(|c| c.is_ca);
                let ca_der = if ca_offer { cap.chain.last().map(|c| c.as_ref().to_vec()) } else { None };
                let (reason, text, previous) = if !opts.tls.trusted_fingerprints.is_empty() && !matches!(reason, TrustReason::Expired | TrustReason::NotYetValid) {
                    let old = opts.tls.trusted_fingerprints.last().cloned().unwrap_or_default();
                    (
                        TrustReason::FingerprintChanged,
                        msg!("tls.reason.fingerprintChanged", old = old.clone(), new = leaf.fingerprint.clone()),
                        Some(old),
                    )
                } else {
                    (reason, text, None)
                };
                ConnectFailure::Trust(Box::new(TrustInfo {
                    reason,
                    reason_text: text,
                    leaf: leaf.clone(),
                    chain: chain.clone(),
                    ca_offer,
                    previous_fingerprint: previous,
                    unsafe_,
                    ca_der,
                }))
            };
            match ce {
                CertificateError::UnknownIssuer => {
                    if leaf.self_signed {
                        trust(TrustReason::SelfSigned, msg!("tls.reason.selfSigned"), false)
                    } else if chain.len() == 1 && opts.tls.ca.is_some() {
                        let issuer = leaf.issuer_cn.clone().map(|cn| format!("CN={cn}")).unwrap_or_else(|| leaf.issuer.clone());
                        msg!("tls.missingIntermediate", issuer = issuer).into()
                    } else {
                        let issuer = chain.last().map(|c| c.issuer.clone()).unwrap_or_default();
                        trust(TrustReason::UnknownCa, msg!("tls.reason.unknownCa", issuer = issuer), false)
                    }
                }
                CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                    trust(TrustReason::Expired, msg!("tls.expired", date = date_of(leaf.not_after_unix)), true)
                }
                CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                    trust(TrustReason::NotYetValid, msg!("tls.notYetValid", date = date_of(leaf.not_before_unix)), true)
                }
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                    let names = if leaf.san.is_empty() { leaf.cn.clone().unwrap_or_default() } else { leaf.san.join(", ") };
                    let shown_host = opts.tls.sni.clone().filter(|s| !s.trim().is_empty()).unwrap_or(host);
                    msg!("tls.wrongName", names = names, host = shown_host).into()
                }
                CertificateError::Revoked => msg!("tls.handshake", host = host, port = port, detail = "server certificate revoked").into(),
                _ => msg!("tls.handshake", host = host, port = port, detail = "certificate not accepted").into(),
            }
        }
        rustls::Error::AlertReceived(alert) => match alert {
            AlertDescription::CertificateRequired => msg!("tls.clientCertRequired").into(),
            AlertDescription::HandshakeFailure if !has_identity => msg!("tls.clientCertRequired").into(),
            AlertDescription::CertificateRevoked => msg!("tls.clientCertRevoked", name = our_cn).into(),
            AlertDescription::UnknownCA | AlertDescription::BadCertificate | AlertDescription::CertificateUnknown | AlertDescription::AccessDenied
                if has_identity =>
            {
                msg!("tls.clientCertRejected").into()
            }
            AlertDescription::DecryptError if has_identity => msg!("tls.clientCertRejected").into(),
            AlertDescription::NoApplicationProtocol => msg!("tls.handshakeAlpn").into(),
            AlertDescription::ProtocolVersion => msg!("tls.handshake", host = host, port = port, detail = "no common TLS version").into(),
            other => msg!("tls.handshake", host = host, port = port, detail = format!("{other:?}")).into(),
        },
        rustls::Error::InvalidMessage(_) | rustls::Error::InappropriateMessage { .. } | rustls::Error::InappropriateHandshakeMessage { .. } => {
            msg!("error.tlsOnPlain", host = host, port = port).into()
        }
        rustls::Error::NoApplicationProtocol => msg!("tls.handshakeAlpn").into(),
        rustls::Error::InvalidCertRevocationList(_) => msg!("tls.handshake", host = host, port = port, detail = "revocation list").into(),
        other => msg!("tls.handshake", host = host, port = port, detail = short_tls_detail(other)).into(),
    }
}

fn short_tls_detail(e: &rustls::Error) -> String {
    match e {
        rustls::Error::PeerIncompatible(_) => "incompatible TLS settings".into(),
        rustls::Error::PeerMisbehaved(_) => "unexpected TLS behaviour".into(),
        rustls::Error::DecryptError => "decryption failed".into(),
        _ => "TLS error".into(),
    }
}

/// Probes whether a port speaks TLS (used when a plain connection fails before CONNACK: E-05).
pub async fn port_speaks_tls(opts: &ConnectOptions) -> bool {
    let Ok(Ok(tcp)) = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((opts.host.as_str(), opts.port))).await else {
        return false;
    };
    let connector = tokio_rustls::TlsConnector::from(tlsconf::probe_config());
    match tokio::time::timeout(Duration::from_secs(3), connector.connect(server_name(opts), tcp)).await {
        Ok(Ok(_)) => true,
        // A TLS server that refuses us still answers with TLS alerts.
        Ok(Err(e)) => matches!(rustls_error(&e), Some(rustls::Error::AlertReceived(_)) | Some(rustls::Error::InvalidCertificate(_))),
        Err(_) => false,
    }
}
