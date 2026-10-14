//! Binding listeners and accepting connections (TCP, TLS, WS, WSS).

use super::session::{serve, ListenerCtx};
use super::state::Shared;
use super::tls::{describe_cert_error, ClientAuthMode};
use super::ws;
use crate::config::ListenerKind;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a listener binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindAddr {
    /// All interfaces, IPv4 and IPv6.
    Network,
    /// Loopback only (127.0.0.1, plus ::1 when available).
    Localhost,
    /// One explicit address (CLI `--bind`).
    Addr(IpAddr),
}

fn bind_one(addr: SocketAddr, dual_stack: bool) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    if addr.is_ipv6() {
        sock.set_only_v6(!dual_stack)?;
    }
    #[cfg(unix)]
    sock.set_reuse_address(true)?;
    sock.bind(&addr.into())?;
    sock.listen(1024)?;
    sock.set_nonblocking(true)?;
    Ok(sock.into())
}

/// Binds the sockets of one listener. Port 0 picks a free port (tests).
pub(crate) fn bind(bind: BindAddr, port: u16) -> std::io::Result<Vec<std::net::TcpListener>> {
    match bind {
        BindAddr::Network => match bind_one(
            SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port),
            true,
        ) {
            Ok(l) => Ok(vec![l]),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Err(e),
            Err(_) => Ok(vec![bind_one(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
                false,
            )?]),
        },
        BindAddr::Localhost => {
            let v4 = bind_one(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
                false,
            )?;
            let actual = v4.local_addr()?.port();
            let mut out = vec![v4];
            if let Ok(v6) = bind_one(
                SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), actual),
                false,
            ) {
                out.push(v6);
            }
            Ok(out)
        }
        BindAddr::Addr(ip) => Ok(vec![bind_one(
            SocketAddr::new(ip, port),
            ip.is_ipv6() && ip.is_unspecified(),
        )?]),
    }
}

/// The watch channels of one accept loop: `stop` ends the loop only (open sessions stay), `shutdown` stops the whole
/// broker, `closed` ends the sessions of this listener (removed, switched off, port or kind changed).
pub(crate) struct Signals {
    pub stop: watch::Receiver<bool>,
    pub shutdown: watch::Receiver<bool>,
    pub closed: watch::Receiver<bool>,
}

/// Accepts connections until the listener stops or the broker shuts down.
pub(crate) async fn accept_loop(
    shared: Arc<Shared>,
    lctx: Arc<ListenerCtx>,
    listener: TcpListener,
    mut sig: Signals,
    done: mpsc::Sender<()>,
) {
    loop {
        tokio::select! {
            _ = sig.stop.changed() => break,
            _ = sig.shutdown.changed() => break,
            r = listener.accept() => match r {
                Ok((sock, peer)) => {
                    let _ = sock.set_nodelay(true);
                    let (shared, lctx, sd, cl, done) = (shared.clone(), lctx.clone(), sig.shutdown.clone(), sig.closed.clone(), done.clone());
                    tokio::spawn(async move {
                        handle(shared, lctx, sock, peer, sd, cl).await;
                        drop(done);
                    });
                }
                Err(e) => {
                    shared.log.debug(format!("accept failed: {e}"));
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }
}

fn peer_text(peer: &SocketAddr) -> String {
    match peer.ip() {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => format!("{v4}:{}", peer.port()),
            None => peer.to_string(),
        },
        _ => peer.to_string(),
    }
}

async fn handle(
    shared: Arc<Shared>,
    lctx: Arc<ListenerCtx>,
    sock: TcpStream,
    peer: SocketAddr,
    shutdown: watch::Receiver<bool>,
    closed: watch::Receiver<bool>,
) {
    let max = shared.max_packet;
    match lctx.kind {
        ListenerKind::Tcp => serve(shared, lctx, sock, peer, None, shutdown, closed).await,
        ListenerKind::Ws => {
            match tokio::time::timeout(HANDSHAKE_TIMEOUT, ws::accept(sock, max)).await {
                Ok(Ok(ws)) => serve(shared, lctx, ws, peer, None, shutdown, closed).await,
                Ok(Err(e)) => shared.log.debug(format!(
                    "WebSocket upgrade from {} failed: {e}",
                    peer_text(&peer)
                )),
                Err(_) => shared.log.debug(format!(
                    "WebSocket upgrade from {} timed out",
                    peer_text(&peer)
                )),
            }
        }
        ListenerKind::Tls | ListenerKind::Wss => {
            // Read at each handshake: the material and the rule can change while the broker runs (change set 7).
            let Some(tls) = lctx.tls() else { return };
            let mode = if shared.auth().anonymous {
                ClientAuthMode::Off
            } else if shared.require_client_cert() {
                ClientAuthMode::Required
            } else {
                ClientAuthMode::Optional
            };
            let (cfg, slot) = match tls.config_for(mode) {
                Ok(x) => x,
                Err(e) => {
                    shared.log.warn(format!(
                        "TLS handshake failed from {}: {e}",
                        peer_text(&peer)
                    ));
                    return;
                }
            };
            let acceptor = tokio_rustls::TlsAcceptor::from(cfg);
            let stream = match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(sock)).await
            {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    let from_slot = slot.lock().ok().and_then(|s| s.failure.clone());
                    let reason = from_slot.unwrap_or_else(|| {
                        e.get_ref()
                            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
                            .map(describe_cert_error)
                            .unwrap_or_else(|| crate::error::short_detail(&e.to_string()))
                    });
                    shared.log.warn(format!(
                        "TLS handshake failed from {}: {reason}",
                        peer_text(&peer)
                    ));
                    return;
                }
                Err(_) => {
                    shared.log.warn(format!(
                        "TLS handshake failed from {}: timeout",
                        peer_text(&peer)
                    ));
                    return;
                }
            };
            let cert = if mode == ClientAuthMode::Off {
                None
            } else {
                slot.lock().ok().and_then(|s| s.verified.clone())
            };
            if lctx.kind == ListenerKind::Tls {
                serve(shared, lctx, stream, peer, cert, shutdown, closed).await;
            } else {
                match tokio::time::timeout(HANDSHAKE_TIMEOUT, ws::accept(stream, max)).await {
                    Ok(Ok(ws)) => serve(shared, lctx, ws, peer, cert, shutdown, closed).await,
                    Ok(Err(e)) => shared.log.debug(format!(
                        "WebSocket upgrade from {} failed: {e}",
                        peer_text(&peer)
                    )),
                    Err(_) => shared.log.debug(format!(
                        "WebSocket upgrade from {} timed out",
                        peer_text(&peer)
                    )),
                }
            }
        }
    }
}
