//! B-14 load test (ignored by default; run in release, with enough file descriptors):
//! `ulimit -n 8192; cargo test -p mqttpanel-broker --release --test load -- --ignored --nocapture`
//!
//! 1 000 connected clients publish at QoS 1, together 5 000 messages per second (fan-in) for 10 s, to topics
//! under `load/`; 4 subscribers on `load/#` at QoS 1 receive 20 000 messages per second (fan-out).

mod common;

use common::*;
use mqttpanel_broker::config::ListenerKind;
use mqttpanel_broker::runtime::{AuthConfig, Broker, RuntimeConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn b14_thousand_clients_5000_msgs_fan_in() {
    const CLIENTS: usize = 1000;
    const SUBSCRIBERS: usize = 4;
    const SECONDS: u64 = 10;
    const PER_CLIENT_RATE: u64 = 5; // 1000 x 5 = 5 000 msg/s
    let cfg = RuntimeConfig {
        listeners: vec![spec("tcp", ListenerKind::Tcp, false)],
        auth: AuthConfig {
            anonymous: true,
            ..Default::default()
        },
        max_clients: 2000,
        ..Default::default()
    };
    let b = Broker::start(cfg).await.unwrap();
    let port = b.port("tcp").unwrap();

    let received = Arc::new(AtomicU64::new(0));
    let mut subs = Vec::new();
    for i in 0..SUBSCRIBERS {
        let mut c = connect(opts4(&format!("sub-{i}"), port, None, None))
            .await
            .expect("subscriber");
        c.subscribe("load/#", 1).await;
        let r = received.clone();
        let mut rx = std::mem::replace(&mut c.rx, tokio::sync::mpsc::unbounded_channel().1);
        subs.push((
            c,
            tokio::spawn(async move {
                while rx.recv().await.is_some() {
                    r.fetch_add(1, Ordering::Relaxed);
                }
            }),
        ));
    }
    let t0 = Instant::now();
    let mut pubs = Vec::with_capacity(CLIENTS);
    for i in 0..CLIENTS {
        pubs.push(Arc::new(
            connect(opts4(&format!("pub-{i}"), port, None, None))
                .await
                .expect("publisher"),
        ));
    }
    println!(
        "B-14: {CLIENTS} publishers + {SUBSCRIBERS} subscribers connected in {:?}",
        t0.elapsed()
    );
    assert_eq!(b.stats().clients as usize, CLIENTS + SUBSCRIBERS);

    let start_at = Instant::now();
    let mut tasks = Vec::new();
    for (i, c) in pubs.iter().enumerate() {
        let c = c.clone();
        tasks.push(tokio::spawn(async move {
            // Spread the publishers over the second.
            tokio::time::sleep(Duration::from_micros(
                (i as u64 * 1_000_000 / CLIENTS as u64) % 1_000_000,
            ))
            .await;
            let mut tick = tokio::time::interval(Duration::from_millis(1000 / PER_CLIENT_RATE));
            for n in 0..SECONDS * PER_CLIENT_RATE {
                tick.tick().await;
                c.publish(
                    &format!("load/{i}"),
                    1,
                    false,
                    format!("{{\"n\":{n}}}"),
                    None,
                )
                .await;
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let sent = CLIENTS as u64 * SECONDS * PER_CLIENT_RATE;
    let expected = sent * SUBSCRIBERS as u64;
    let deadline = Instant::now() + Duration::from_secs(30);
    while received.load(Ordering::Relaxed) < expected && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let elapsed = start_at.elapsed().as_secs_f64();
    let got = received.load(Ordering::Relaxed);
    let st = b.stats();
    println!(
        "B-14: sent {sent} (fan-in {:.0} msg/s), delivered {got}/{expected} (fan-out {:.0} msg/s) in {elapsed:.1} s; broker in {} out {} dropped {}",
        sent as f64 / elapsed,
        got as f64 / elapsed,
        st.msg_in_total,
        st.msg_out_total,
        st.dropped
    );
    assert_eq!(got, expected, "no drops at QoS 1");
    assert_eq!(st.dropped, 0);
    drop(subs);
    b.stop().await;
}
