//! TESTPLAN Q-05 (and D-12): the memory estimate is within 10 % of the real heap usage after a
//! known replay, measured with a counting global allocator. This file holds a single test so
//! no other test allocates concurrently.

mod common;

use common::{msg, T0};
use mqttpanel_engine::{Engine, EngineConfig, TableQuery, TreeQuery};
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

fn check(label: &str, build: impl FnOnce() -> Box<Engine>) {
    let before = LIVE.load(Ordering::SeqCst);
    let engine = build();
    let real = LIVE.load(Ordering::SeqCst) - before;
    let est = engine.memory();
    let ratio = est.bytes as f64 / real as f64;
    println!(
        "Q-05 {label}: real {real} B, estimate {} B, ratio {ratio:.3}, points {}, messages {}",
        est.bytes, est.points, est.messages
    );
    assert!(
        (0.9..=1.1).contains(&ratio),
        "{label}: estimate off by more than 10 %"
    );
    drop(engine);
}

#[test]
fn q05_memory_estimate_within_10_percent() {
    // Many small JSON topics, the tree and table caches built.
    check("5k topics x 40 msgs", || {
        let mut e = Box::new(Engine::default());
        for round in 0..40 {
            for i in 0..5000 {
                let topic = format!("site/b{}/f{}/r{}/sensor{}", i % 5, i % 7, i % 50, i);
                let payload = format!(
                    "{{\"value\":{},\"status\":\"ok\",\"n\":{round}}}",
                    (i * round) % 97
                );
                e.ingest(msg(&topic, &payload, T0 + round * 1000 + (i % 1000)));
            }
        }
        e.expand_to_depth(3);
        e.tree_slice(&TreeQuery::default(), 0, 60, T0 + 40_000);
        e.table_slice(&TableQuery::default(), 0, 60, T0 + 40_000);
        e
    });
    // D-12: one topic filling the 5 000-point cap.
    check("5k-point series", || {
        let mut e = Box::new(Engine::new(EngineConfig::default()));
        for i in 0..8000 {
            e.ingest(msg(
                "cap/t",
                &format!("{{\"v\":{i},\"w\":{}}}", i * 2),
                T0 + i,
            ));
        }
        e
    });
    // Larger payloads and text.
    check("large payloads", || {
        let mut e = Box::new(Engine::default());
        let body = "y".repeat(10_000);
        for i in 0..2000 {
            e.ingest(msg(&format!("blob/{}", i % 100), &body, T0 + i));
        }
        e
    });
}
