//! P-12: a deep topic costs memory linear in its length, and topics longer than the per-profile limit (default
//! 1024 bytes) are kept cut. Measured with a counting global allocator; this file holds a single test so no other
//! test allocates concurrently.

mod common;

use common::{msg, T0};
use mqttpanel_engine::{Engine, EngineConfig};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

// SAFETY: delegates to the system allocator and only counts sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::SeqCst);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size() as isize, Ordering::SeqCst);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            LIVE.fetch_add(new_size as isize - layout.size() as isize, Ordering::SeqCst);
        }
        p
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Heap kept by an engine after one message on `topic`, and the engine.
fn cost(cfg: EngineConfig, topic: &str) -> (isize, Box<Engine>) {
    let before = LIVE.load(Ordering::SeqCst);
    let mut e = Box::new(Engine::new(cfg));
    e.ingest(msg(topic, "1", T0));
    (LIVE.load(Ordering::SeqCst) - before, e)
}

const MB: isize = 1024 * 1024;

#[test]
fn p12_deep_topics_cost_linear_memory_and_long_topics_are_cut() {
    // The review's proof of concept: one 32 000-byte topic of '/' (32 001 levels) took about 570 MB.
    let deep = "/".repeat(32_000);

    // Default limit (1024 bytes): the topic is kept cut, the message counted.
    let (bytes, e) = cost(EngineConfig::default(), &deep);
    println!("P-12 default limit: {bytes} B");
    assert!(bytes < 2 * MB, "{bytes} B");
    assert_eq!(e.long_topics(), 1);
    assert_eq!(e.topic_count(), 1);
    let shown = e.known_topics("", 10);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].len(), 1024 + "…".len());
    assert!(shown[0].ends_with('…'));
    drop(e);

    // No practical limit: linear, a few MB for 32 001 levels (twice the levels, about twice the memory).
    let unlimited = EngineConfig {
        max_topic_len: 65_535,
        ..Default::default()
    };
    let (half, e1) = cost(unlimited.clone(), &"/".repeat(16_000));
    drop(e1);
    let (full, e2) = cost(unlimited, &deep);
    println!("P-12 no limit: 16 000 B topic {half} B, 32 000 B topic {full} B");
    assert!(full < 16 * MB, "{full} B");
    let ratio = full as f64 / half as f64;
    assert!(ratio < 2.6, "not linear: ratio {ratio:.2}");
    assert_eq!(e2.long_topics(), 0);
    // The estimate still tracks the real heap.
    let est = e2.memory().bytes as f64;
    assert!((0.8..=1.2).contains(&(est / full as f64)), "estimate {est} vs {full}");
    drop(e2);

    // Multibyte characters are never cut in half.
    let cfg = EngineConfig {
        max_topic_len: 5,
        ..Default::default()
    };
    let (_, e) = cost(cfg, "aaaaé/b");
    assert_eq!(e.known_topics("", 10), vec!["aaaa…".to_string()]);
}
