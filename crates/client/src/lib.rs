//! MQTT 3.1.1 and 5 client for MQTT Panel over TCP, TLS (rustls) and WebSocket.
//!
//! Every failure is a [`catalog::UserMessage`] (a catalog key with parameters) or a trust prompt
//! ([`transport::TrustInfo`]); raw library errors never reach the user.

pub mod catalog;
pub mod options;
pub mod session;
pub mod tlsconf;
pub mod transport;

pub use catalog::{Lang, UserMessage};
pub use options::{random_client_id, ConnectOptions, Scheme, TlsOptions, Version, VersionPref, Will};
pub use session::{connect, establish, test_connection, ClientEvent, ClientHandle, ConnInfo, Incoming, Outgoing, Props5, PubStatus, TestReport};
pub use transport::{ConnectFailure, TransportInfo, TrustInfo, TrustReason};

/// MQTT topic filter matching (+ and #; "$" topics are not matched by a leading wildcard).
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    let mut f = filter.split('/');
    let mut t = topic.split('/');
    let mut first = true;
    loop {
        match (f.next(), t.next()) {
            (Some("#"), Some(seg)) => return !(first && seg.starts_with('$')),
            (Some("#"), None) => return true,
            (Some("+"), Some(seg)) => {
                if first && seg.starts_with('$') {
                    return false;
                }
            }
            (Some(a), Some(b)) if a == b => {}
            (None, None) => return true,
            _ => return false,
        }
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::topic_matches;
    #[test]
    fn wildcards() {
        assert!(topic_matches("#", "a/b"));
        assert!(!topic_matches("#", "$SYS/x"));
        assert!(topic_matches("$SYS/#", "$SYS/x"));
        assert!(topic_matches("a/+/c", "a/b/c"));
        assert!(!topic_matches("a/+/c", "a/b/d"));
        assert!(topic_matches("a/#", "a"));
        assert!(topic_matches("a/b", "a/b"));
        assert!(!topic_matches("a/b", "a/b/c"));
    }
}
