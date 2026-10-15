//! Topic store: a segment tree in an arena plus a path index.

use crate::buffer::StoredMessage;
use crate::series::Series;
use crate::types::PayloadFormat;
use std::collections::VecDeque;
use std::sync::Arc;

/// Index of a node in the arena.
pub type NodeId = u32;
/// Marker for "no parent".
pub const NONE: NodeId = u32::MAX;
/// The virtual root (parent of top-level segments).
pub const ROOT: NodeId = 0;

/// Seconds in the rate window.
pub const RATE_WINDOW_S: i64 = 10;

/// Message counts per second over the last 10 seconds (the rate shown is the last completed second).
#[derive(Debug, Clone, Copy, Default)]
pub struct RateWindow {
    last_sec: i64,
    counts: [u32; RATE_WINDOW_S as usize],
}

impl RateWindow {
    /// Counts one message received at `t_ms`.
    pub fn add(&mut self, t_ms: i64) {
        let s = t_ms.div_euclid(1000);
        if s > self.last_sec {
            let advance = s.saturating_sub(self.last_sec);
            if advance >= RATE_WINDOW_S {
                self.counts = [0; RATE_WINDOW_S as usize];
            } else {
                for k in 1..=advance {
                    self.counts[(self.last_sec + k).rem_euclid(RATE_WINDOW_S) as usize] = 0;
                }
            }
            self.last_sec = s;
        } else if s <= self.last_sec.saturating_sub(RATE_WINDOW_S) {
            return; // older than the window
        }
        let slot = &mut self.counts[s.rem_euclid(RATE_WINDOW_S) as usize];
        *slot = slot.saturating_add(1);
    }

    /// Messages per second shown in the UI (change set 9): the count of the last completed one-second window before
    /// `now_ms`, so the figure changes once a second and never jumps between two ticks. During the first second after
    /// the first message it reads 0.
    pub fn rate(&self, now_ms: i64) -> f64 {
        let s = now_ms.div_euclid(1000) - 1;
        if s > self.last_sec || s <= self.last_sec.saturating_sub(RATE_WINDOW_S) {
            return 0.0;
        }
        f64::from(self.counts[s.rem_euclid(RATE_WINDOW_S) as usize])
    }

    /// Resets the window.
    pub fn clear(&mut self) {
        *self = RateWindow::default();
    }
}

/// Node flags.
pub mod flags {
    pub const EXPANDED: u8 = 1;
    pub const PINNED: u8 = 2;
    pub const RETAINED: u8 = 4;
    pub const SORTED: u8 = 8;
}

/// Data of a node that received at least one message.
#[derive(Debug, Default)]
pub struct TopicData {
    pub buffer: VecDeque<StoredMessage>,
    /// Per-topic buffer size override.
    pub buffer_cap: Option<u32>,
    pub series: Vec<Series>,
    /// Index into `series` of the default series (sparklines).
    pub default_series: Option<u16>,
    pub preview: Box<str>,
    pub format: PayloadFormat,
    pub last_qos: u8,
    pub last_size: u64,
    /// Own last receive time.
    pub last_at: i64,
    /// Default series value of the last message, for sorting.
    pub value_num: Option<f64>,
}

/// A segment of the topic tree.
#[derive(Debug)]
pub struct Node {
    /// Full topic path of this node.
    pub path: Arc<str>,
    /// Byte offset of the last segment in `path`.
    pub name_start: u32,
    pub parent: NodeId,
    pub depth: u16,
    pub flags: u8,
    pub children: Vec<NodeId>,
    /// Topics in the subtree, this node included.
    pub topic_count: u32,
    /// Messages since connect, own plus subtree.
    pub messages: u64,
    /// Own messages since connect.
    pub own_messages: u64,
    /// Own or subtree, `i64::MIN` when none.
    pub last_received_at: i64,
    pub rate: RateWindow,
    pub topic: Option<Box<TopicData>>,
}

impl Node {
    /// A new node.
    pub fn new(path: Arc<str>, parent: NodeId, depth: u16) -> Self {
        let name_start = path.rfind('/').map_or(0, |i| i + 1) as u32;
        Node {
            path,
            name_start,
            parent,
            depth,
            flags: flags::SORTED,
            children: Vec::new(),
            topic_count: 0,
            messages: 0,
            own_messages: 0,
            last_received_at: i64::MIN,
            rate: RateWindow::default(),
            topic: None,
        }
    }

    /// Last segment.
    pub fn name(&self) -> &str {
        &self.path[self.name_start as usize..]
    }

    /// Flag test.
    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }

    /// Flag set or clear.
    pub fn set(&mut self, f: u8, on: bool) {
        if on {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }

    /// Last receive time as an option.
    pub fn last_at(&self) -> Option<i64> {
        (self.last_received_at != i64::MIN).then_some(self.last_received_at)
    }
}

#[cfg(test)]
mod tests {
    use super::RateWindow;

    /// X-24: 50 messages within one second read 50 msg/s during the whole next second, then 0.
    #[test]
    fn rate_is_the_last_completed_second() {
        let mut w = RateWindow::default();
        for i in 0..50 {
            w.add(10_000 + i * 19);
        }
        assert_eq!(w.rate(10_500), 0.0); // the first second is not complete yet
        assert_eq!(w.rate(11_000), 50.0);
        assert_eq!(w.rate(11_999), 50.0);
        w.add(11_200);
        assert_eq!(w.rate(11_900), 50.0); // a message of the running second does not move the figure
        assert_eq!(w.rate(12_000), 1.0);
        assert_eq!(w.rate(13_000), 0.0);
        assert_eq!(w.rate(40_000), 0.0);
    }
}
