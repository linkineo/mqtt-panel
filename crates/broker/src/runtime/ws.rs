//! MQTT over WebSocket: the HTTP upgrade on path `/mqtt` with sub-protocol `mqtt`, and a byte-stream adapter.

use bytes::Bytes;
use futures_util::{Sink, Stream};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// The WebSocket path served by WS and WSS listeners.
pub const WS_PATH: &str = "/mqtt";

/// Accepts the WebSocket upgrade; refuses other paths with HTTP 404.
pub(crate) async fn accept<S>(stream: S, max_message: usize) -> Result<WsBytes<S>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let callback = |req: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
        if req.uri().path() != WS_PATH {
            let mut r = ErrorResponse::new(Some(format!("Not found: use {WS_PATH}")));
            *r.status_mut() = StatusCode::NOT_FOUND;
            return Err(r);
        }
        let wants_mqtt = req
            .headers()
            .get_all("Sec-WebSocket-Protocol")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .any(|p| p.trim().eq_ignore_ascii_case("mqtt"));
        if wants_mqtt {
            resp.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static("mqtt"));
        }
        Ok(resp)
    };
    let mut cfg = WebSocketConfig::default();
    cfg.max_message_size = Some(max_message.saturating_add(16));
    cfg.max_frame_size = Some(max_message.saturating_add(16));
    let ws = tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(cfg)).await.map_err(|e| e.to_string())?;
    Ok(WsBytes::new(ws))
}

/// A WebSocket seen as a byte stream: binary frames in, one binary frame per write out.
pub struct WsBytes<S> {
    inner: WebSocketStream<S>,
    pending: Bytes,
    eof: bool,
}

impl<S> WsBytes<S> {
    pub fn new(inner: WebSocketStream<S>) -> Self {
        WsBytes { inner, pending: Bytes::new(), eof: false }
    }
}

fn io_err(e: tokio_tungstenite::tungstenite::Error) -> io::Error {
    io::Error::other(e.to_string())
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for WsBytes<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        loop {
            if !self.pending.is_empty() {
                let n = self.pending.len().min(buf.remaining());
                let chunk = self.pending.split_to(n);
                buf.put_slice(&chunk);
                return Poll::Ready(Ok(()));
            }
            if self.eof {
                return Poll::Ready(Ok(()));
            }
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    self.eof = true;
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(io_err(e))),
                Poll::Ready(Some(Ok(msg))) => match msg {
                    Message::Binary(b) => self.pending = b,
                    // MQTT over WebSocket uses binary frames; tolerate text frames carrying bytes.
                    Message::Text(t) => self.pending = Bytes::copy_from_slice(t.as_bytes()),
                    Message::Close(_) => self.eof = true,
                    _ => {}
                },
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for WsBytes<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_ready(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(io_err(e))),
            Poll::Ready(Ok(())) => {}
        }
        Pin::new(&mut self.inner).start_send(Message::Binary(Bytes::copy_from_slice(buf))).map_err(io_err)?;
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx).map_err(io_err)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx).map_err(io_err)
    }
}
