//! Engine benchmark (TESTPLAN X-01, Q-02 at engine level). Run with:
//! `cargo run -p mqttpanel-engine --release --example bench`
//!
//! Builds 100 000 topics, then replays 5 000 msg/s worth of messages over 2 000 topics for
//! 20 simulated seconds as fast as possible, and prints ingest rate, slice, sort, search and
//! memory figures.

use mqttpanel_engine::{
    Engine, Filters, IncomingMessage, SortDir, TableColumn, TableQuery, TableSort, TreeQuery,
};
use std::time::Instant;

const T0: i64 = 1_760_000_000_000;

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn topics() -> Vec<String> {
    const KINDS: [&str; 10] = [
        "temperature",
        "humidity",
        "co2",
        "occupancy",
        "setpoint",
        "valve",
        "fan",
        "lux",
        "voc",
        "pressure",
    ];
    let mut v = Vec::with_capacity(100_000);
    'outer: for f in 1..=10 {
        for r in 1..=100 {
            for s in 0..100 {
                v.push(format!("building/f{f}/r{r}/{}{}", KINDS[s % 10], s / 10));
                if v.len() == 99_900 {
                    break 'outer;
                }
            }
        }
    }
    for e in 0..10 {
        for d in 0..5 {
            v.push(format!("spBv1.0/plant/DDATA/edge{e}/dev{d}"));
        }
    }
    for i in 0..50 {
        v.push(format!("$SYS/broker/load/messages/received/{i}min"));
    }
    v
}

fn q(search: &str) -> TreeQuery {
    TreeQuery {
        search: search.into(),
        filters: Filters::default(),
    }
}

fn main() {
    let topics = topics();
    let mut e = Engine::default();
    let t = Instant::now();
    for (i, topic) in topics.iter().enumerate() {
        e.ingest(IncomingMessage::new(
            topic,
            format!("{{\"value\":{}}}", i % 100),
            T0 + (i as i64 % 1000),
        ));
    }
    let build = t.elapsed();
    println!(
        "build 100k topics: {:.1} ms ({} topics)",
        ms(build),
        e.topic_count()
    );

    // Replay: 5 000 msg/s over 2 000 topics for 20 simulated seconds.
    let hot: Vec<&String> = topics.iter().step_by(50).take(2000).collect();
    let seconds = 20;
    let per_second = 5000;
    let payloads: Vec<String> = (0..per_second)
        .map(|i| {
            format!(
                "{{\"value\":{}.{},\"status\":\"ok\",\"supply\":{{\"temp\":{},\"fan\":true}}}}",
                20 + i % 7,
                i % 10,
                18 + i % 5
            )
        })
        .collect();
    let mut msgs = Vec::with_capacity(seconds * per_second);
    for s in 0..seconds {
        for i in 0..per_second {
            let topic = hot[(s * per_second + i) % hot.len()];
            let at = T0 + 1000 + (s as i64) * 1000 + (i as i64 * 1000 / per_second as i64);
            msgs.push(IncomingMessage::new(
                topic,
                payloads[i].clone().into_bytes(),
                at,
            ));
        }
    }
    let n = msgs.len();
    let t = Instant::now();
    for m in msgs {
        e.ingest(m);
    }
    let replay = t.elapsed();
    let now = T0 + 1000 + seconds as i64 * 1000;
    println!(
        "ingest: {n} messages in {:.1} ms = {:.0} msg/s (target 5 000 msg/s)",
        ms(replay),
        n as f64 / replay.as_secs_f64()
    );

    e.expand_to_depth(3);
    let t = Instant::now();
    let s = e.tree_slice(&q(""), 0, 60, now);
    let first = t.elapsed();
    let t = Instant::now();
    let s2 = e.tree_slice(&q(""), 40_000, 60, now);
    let slice = t.elapsed();
    println!(
        "tree slice 60 rows (expanded at depth 3, {} visible): first {:.2} ms (builds the visible list), cached {:.3} ms",
        s.total,
        ms(first),
        ms(slice)
    );
    assert_eq!(s2.rows.len(), 60);

    let tq = TableQuery {
        sort: TableSort {
            column: TableColumn::Rate,
            dir: SortDir::Desc,
        },
        ..TableQuery::default()
    };
    let t = Instant::now();
    let ts = e.table_slice(&tq, 0, 60, now);
    let table = t.elapsed();
    println!(
        "table slice 60 rows sorted by rate over {} topics: {:.2} ms",
        ts.total,
        ms(table)
    );
    let tq2 = TableQuery {
        sort: TableSort {
            column: TableColumn::Topic,
            dir: SortDir::Asc,
        },
        ..TableQuery::default()
    };
    let t = Instant::now();
    e.table_slice(&tq2, 0, 60, now);
    println!(
        "table slice 60 rows sorted by topic: {:.2} ms",
        ms(t.elapsed())
    );

    for (label, search) in [
        ("substring", "humidity3"),
        ("wildcard", "building/+/r7/#"),
        ("regex", "/^building/f(1|2)/r1\\d/co2\\d$"),
    ] {
        let t = Instant::now();
        let r = e.tree_slice(&q(search), 0, 60, now);
        println!(
            "search {label} {search:?}: {:.2} ms ({} rows)",
            ms(t.elapsed()),
            r.total
        );
    }

    let t = Instant::now();
    let mem = e.memory();
    println!(
        "memory estimate: {:.1} MiB ({} messages buffered, {} series points), computed in {:.2} ms",
        mem.bytes as f64 / 1048576.0,
        mem.messages,
        mem.points,
        ms(t.elapsed())
    );
}
