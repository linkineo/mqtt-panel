//! Per-topic ring buffer of messages.

use crate::types::Mqtt5Props;
use std::collections::VecDeque;

/// Smallest and largest per-topic buffer size.
pub const MIN_BUFFER: usize = 10;
/// Largest per-topic buffer size.
pub const MAX_BUFFER: usize = 10_000;

/// Clamps a buffer size to the allowed range.
pub fn clamp_buffer(n: usize) -> usize {
    n.clamp(MIN_BUFFER, MAX_BUFFER)
}

/// A buffered message.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    /// Connection-unique sequence number.
    pub seq: u64,
    pub received_at: i64,
    /// Kept bytes (truncated above the keep limit).
    pub payload: Box<[u8]>,
    /// Original size on the wire.
    pub size: usize,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    pub props: Option<Box<Mqtt5Props>>,
}

impl StoredMessage {
    /// True when the payload was kept truncated.
    pub fn truncated(&self) -> bool {
        self.payload.len() < self.size
    }

    /// Content type from the MQTT 5 properties.
    pub fn content_type(&self) -> Option<&str> {
        self.props.as_ref().and_then(|p| p.content_type.as_deref())
    }

    /// Heap bytes owned by this message (payload and properties).
    pub fn heap_bytes(&self) -> usize {
        let mut n = self.payload.len();
        if let Some(p) = &self.props {
            n += std::mem::size_of::<Mqtt5Props>() + props_heap(p);
        }
        n
    }
}

fn props_heap(p: &Mqtt5Props) -> usize {
    let s = |o: &Option<String>| o.as_ref().map_or(0, |x| x.capacity());
    s(&p.content_type)
        + s(&p.response_topic)
        + s(&p.correlation_data)
        + p.user_properties.capacity() * std::mem::size_of::<(String, String)>()
        + p.user_properties
            .iter()
            .map(|(a, b)| a.capacity() + b.capacity())
            .sum::<usize>()
        + p.subscription_ids.capacity() * 4
}

/// Pushes a message, dropping the oldest above `cap`.
pub fn push(buf: &mut VecDeque<StoredMessage>, msg: StoredMessage, cap: usize) {
    while buf.len() >= cap {
        buf.pop_front();
    }
    buf.push_back(msg);
}

/// Drops the oldest messages above `cap` and releases spare capacity.
pub fn truncate(buf: &mut VecDeque<StoredMessage>, cap: usize) {
    if buf.len() > cap {
        let drop = buf.len() - cap;
        buf.drain(..drop);
        buf.shrink_to(cap);
    }
}

/// Finds a message by sequence number (sequence numbers increase within a buffer).
pub fn find(buf: &VecDeque<StoredMessage>, seq: u64) -> Option<usize> {
    buf.binary_search_by(|m| m.seq.cmp(&seq)).ok()
}
