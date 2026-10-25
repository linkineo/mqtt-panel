//! Binding listeners and accepting connections (TCP, TLS, WS, WSS).

use super::session::{serve, ListenerCtx};
use super::state::Shared;
use super::tls::{describe_cert_error, ClientAuthMode};
use super::ws;
use crate::config::ListenerKind;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// A connection that has not finished CONNECT, counted against the pending limits until dropped.
pub(crate) struct PendingSlot {
    shared: Arc<Shared>,
    listener: Arc<AtomicUsize>,
    ip: IpAddr,
}

impl PendingSlot {
    /// Takes a slot, or says which limit is reached.
    fn take(
        shared: &Arc<Shared>,
        listener: &Arc<AtomicUsize>,
        ip: IpAddr,
    ) -> Result<PendingSlot, &'static str> {
        let lim = &shared.limits;
        let mut per_ip = shared.pending.lock().unwrap_or_else(|p| p.into_inner());
        let n = per_ip.get(&ip).copied().unwrap_or(0);
        if n >= lim.pending_per_address {
            return Err("address");
        }
        if listener.fetch_add(1, Ordering::SeqCst) >= lim.pending_per_listener {
            listener.fetch_sub(1, Ordering::SeqCst);
            return Err("listener");
        }
        per_ip.insert(ip, n + 1);
        Ok(PendingSlot {
            shared: shared.clone(),
            listener: listener.clone(),
            ip,
        })
    }
}

impl Drop for PendingSlot {
    fn drop(&mut self) {
        self.listener.fetch_sub(1, Ordering::SeqCst);
        let mut per_ip = self
            .shared
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(n) = per_ip.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                per_ip.remove(&self.ip);
            }
        }
    }
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
                    let ip = peer.ip().to_canonical();
                    let slot = match PendingSlot::take(&shared, &lctx.pending, ip) {
                        Ok(s) => s,
                        Err(which) => {
                            drop(sock);
                            let lim = &shared.limits;
                            shared.warn_limit(ip, || match which {
                                "address" => format!(
                                    "connections from {} refused: more than {} waiting to log in from this address",
                                    peer_text(&peer), lim.pending_per_address
                                ),
                                _ => format!(
                                    "connections from {} refused: more than {} waiting to log in on the {} listener",
                                    peer_text(&peer), lim.pending_per_listener, lctx.kind.label()
                                ),
                            });
                            continue;
                        }
                    };
                    let _ = sock.set_nodelay(true);
                    let (shared, lctx, sd, cl, done) = (shared.clone(), lctx.clone(), sig.shutdown.clone(), sig.closed.clone(), done.clone());
                    tokio::spawn(async move {
                        handle(shared, lctx, sock, peer, sd, cl, slot).await;
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
    slot: PendingSlot,
) {
    match lctx.kind {
        ListenerKind::Tcp => serve(shared, lctx, sock, peer, None, shutdown, closed, slot).await,
        ListenerKind::Ws => upgrade(shared, lctx, sock, peer, None, shutdown, closed, slot).await,
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
            let (cfg, hs) = match tls.config_for(mode) {
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
                    let from_slot = hs.lock().ok().and_then(|s| s.failure.clone());
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
                hs.lock().ok().and_then(|s| s.verified.clone())
            };
            if lctx.kind == ListenerKind::Tls {
                serve(shared, lctx, stream, peer, cert, shutdown, closed, slot).await;
            } else {
                upgrade(shared, lctx, stream, peer, cert, shutdown, closed, slot).await;
            }
        }
    }
}

/// WebSocket upgrade with the web origin rule (SPEC 6): a browser (an `Origin` header) may connect when the broker
/// requires a login, or when its origin is on the listener's list; an unknown origin while Anonymous is on asks the
/// owner. Clients without an `Origin` header are not affected.
#[allow(clippy::too_many_arguments)]
async fn upgrade<S>(
    shared: Arc<Shared>,
    lctx: Arc<ListenerCtx>,
    mut stream: S,
    peer: SocketAddr,
    cert: Option<super::tls::PeerCert>,
    shutdown: watch::Receiver<bool>,
    closed: watch::Receiver<bool>,
    slot: PendingSlot,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let head = match tokio::time::timeout(HANDSHAKE_TIMEOUT, ws::read_head(&mut stream)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => {
            shared.log.debug(format!(
                "WebSocket upgrade from {} failed: {e}",
                peer_text(&peer)
            ));
            return;
        }
        Err(_) => {
            shared.log.debug(format!(
                "WebSocket upgrade from {} timed out",
                peer_text(&peer)
            ));
            return;
        }
    };
    if let Some(origin) = ws::origin_of(&head) {
        if !origin_allowed(&shared, &lctx, &origin, &peer).await {
            ws::refuse_forbidden(&mut stream).await;
            return;
        }
    }
    let stream = ws::Rewind::new(head, stream);
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, ws::accept(stream, shared.max_packet)).await {
        Ok(Ok(ws)) => serve(shared, lctx, ws, peer, cert, shutdown, closed, slot).await,
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

/// Decides whether a browser page with this origin may open the WebSocket (see [`upgrade`]).
async fn origin_allowed(
    shared: &Arc<Shared>,
    lctx: &Arc<ListenerCtx>,
    origin: &str,
    peer: &SocketAddr,
) -> bool {
    if !shared.auth().anonymous {
        // A login is required: the page still has to log in like any client.
        return true;
    }
    let norm = super::normalize_origin(origin);
    let listed = |l: &ListenerCtx| l.origins.read().map(|g| g.contains(&norm)).unwrap_or(false);
    if listed(lctx) {
        shared.log.debug(format!(
            "web page at {origin} connects to the {} listener (allowed web origin)",
            lctx.kind.label()
        ));
        return true;
    }
    if !crate::manager::valid_origin(&norm) {
        // `null` (sandboxed pages, local files) or a malformed origin: any page can send it, never asked.
        shared.warn_limit(peer.ip().to_canonical(), || {
            format!(
                "web page at {origin} refused on the {} listener: not an allowed web origin",
                lctx.kind.label()
            )
        });
        return false;
    }
    let key = (lctx.id.clone(), norm.clone());
    // One prompt per origin: later upgrades from the same origin wait on the same answer.
    let mut rx = {
        let mut waits = shared
            .origin_waits
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        match waits.get(&key) {
            Some(rx) => rx.clone(),
            None => {
                let prompt = shared.origin_prompt.read().ok().and_then(|g| g.clone());
                let Some(prompt) = prompt else {
                    drop(waits);
                    shared.warn_limit(peer.ip().to_canonical(), || format!(
                        "web page at {origin} refused on the {} listener: not an allowed web origin",
                        lctx.kind.label()
                    ));
                    return false;
                };
                let (tx, rx) = tokio::sync::watch::channel(None);
                waits.insert(key.clone(), rx.clone());
                let answer = (prompt.0)(super::OriginRequest {
                    listener_id: lctx.id.clone(),
                    kind: lctx.kind,
                    port: lctx.port.load(Ordering::Relaxed),
                    origin: origin.to_string(),
                });
                let (shared2, lctx2, key2, origin2) = (
                    shared.clone(),
                    lctx.clone(),
                    key.clone(),
                    origin.to_string(),
                );
                let wait = shared.limits.origin_prompt_timeout;
                tokio::spawn(async move {
                    let allowed = matches!(tokio::time::timeout(wait, answer).await, Ok(Ok(true)));
                    if allowed {
                        if let Ok(mut g) = lctx2.origins.write() {
                            if !g.contains(&key2.1) {
                                g.push(key2.1.clone());
                            }
                        }
                        shared2.log.info(format!(
                            "web page at {origin2} allowed on the {} listener",
                            lctx2.kind.label()
                        ));
                    } else {
                        shared2.log.warn(format!(
                            "web page at {origin2} refused on the {} listener",
                            lctx2.kind.label()
                        ));
                    }
                    shared2
                        .origin_waits
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&key2);
                    let _ = tx.send(Some(allowed));
                });
                rx
            }
        }
    };
    loop {
        if let Some(a) = *rx.borrow_and_update() {
            return a;
        }
        if rx.changed().await.is_err() {
            return rx.borrow().unwrap_or(false);
        }
    }
}
