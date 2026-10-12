//! The broker's live log: a bounded ring buffer of [`LogLine`]s, mirrored to `tracing`.

use crate::config::{LogLevel, LogLine};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Lines kept in memory.
pub const LOG_CAPACITY: usize = 2000;

/// A shared log buffer. Cloning shares the buffer (the manager keeps one across restarts).
#[derive(Debug, Clone, Default)]
pub struct LogBuffer {
    inner: Arc<Mutex<LogInner>>,
}

#[derive(Debug, Default)]
struct LogInner {
    seq: u64,
    lines: VecDeque<LogLine>,
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl LogBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a line and returns its sequence number.
    pub fn push(&self, level: LogLevel, text: impl Into<String>) -> u64 {
        let text = text.into();
        match level {
            LogLevel::Debug => tracing::debug!(target: "mqttpanel_broker", "{text}"),
            LogLevel::Info => tracing::info!(target: "mqttpanel_broker", "{text}"),
            LogLevel::Warn => tracing::warn!(target: "mqttpanel_broker", "{text}"),
            LogLevel::Error => tracing::error!(target: "mqttpanel_broker", "{text}"),
        }
        let Ok(mut g) = self.inner.lock() else {
            return 0;
        };
        g.seq += 1;
        let seq = g.seq;
        g.lines.push_back(LogLine {
            seq,
            at: now_ms(),
            level,
            text,
        });
        while g.lines.len() > LOG_CAPACITY {
            g.lines.pop_front();
        }
        seq
    }

    pub fn debug(&self, text: impl Into<String>) {
        self.push(LogLevel::Debug, text);
    }
    pub fn info(&self, text: impl Into<String>) {
        self.push(LogLevel::Info, text);
    }
    pub fn warn(&self, text: impl Into<String>) {
        self.push(LogLevel::Warn, text);
    }
    pub fn error(&self, text: impl Into<String>) {
        self.push(LogLevel::Error, text);
    }

    /// Lines with `seq > after_seq` at `level` or above, oldest first.
    pub fn lines(&self, level: LogLevel, after_seq: u64) -> Vec<LogLine> {
        let Ok(g) = self.inner.lock() else {
            return Vec::new();
        };
        g.lines
            .iter()
            .filter(|l| l.seq > after_seq && l.level >= level)
            .cloned()
            .collect()
    }

    /// Sequence number of the newest line (0 when empty).
    pub fn last_seq(&self) -> u64 {
        self.inner.lock().map(|g| g.seq).unwrap_or(0)
    }

    /// True when a line containing `needle` exists (tests and diagnostics).
    pub fn contains(&self, needle: &str) -> bool {
        self.inner
            .lock()
            .map(|g| g.lines.iter().any(|l| l.text.contains(needle)))
            .unwrap_or(false)
    }
}
