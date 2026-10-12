//! Message counters with a sliding 10 second window for rates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const WINDOW: usize = 10;

#[derive(Debug)]
pub(crate) struct Stats {
    start: Instant,
    pub total_in: AtomicU64,
    pub total_out: AtomicU64,
    pub dropped: AtomicU64,
    buckets: Mutex<[(u64, u64, u64); WINDOW]>, // (second, in, out)
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            start: Instant::now(),
            total_in: AtomicU64::new(0),
            total_out: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            buckets: Mutex::new([(u64::MAX, 0, 0); WINDOW]),
        }
    }

    fn second(&self) -> u64 {
        self.start.elapsed().as_secs()
    }

    fn add(&self, inc_in: u64, inc_out: u64) {
        let sec = self.second();
        if let Ok(mut b) = self.buckets.lock() {
            let slot = &mut b[(sec as usize) % WINDOW];
            if slot.0 != sec {
                *slot = (sec, 0, 0);
            }
            slot.1 += inc_in;
            slot.2 += inc_out;
        }
    }

    pub fn msg_in(&self) {
        self.total_in.fetch_add(1, Ordering::Relaxed);
        self.add(1, 0);
    }

    pub fn msg_out(&self, n: u64) {
        if n == 0 {
            return;
        }
        self.total_out.fetch_add(n, Ordering::Relaxed);
        self.add(0, n);
    }

    /// (in, out) messages per second over the last 10 seconds.
    pub fn rates(&self) -> (f64, f64) {
        let sec = self.second();
        let Ok(b) = self.buckets.lock() else {
            return (0.0, 0.0);
        };
        let (mut i, mut o) = (0u64, 0u64);
        for (s, bi, bo) in b.iter() {
            if *s != u64::MAX && *s <= sec && sec - *s < WINDOW as u64 {
                i += bi;
                o += bo;
            }
        }
        // While the broker is younger than the window, divide by its age.
        let span = (sec + 1).min(WINDOW as u64) as f64;
        (i as f64 / span, o as f64 / span)
    }
}
