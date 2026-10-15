//! TESTPLAN section 4: X-01, X-02, X-03, X-11, plus tree, table, search, export behaviour.

mod common;

use common::{msg, topics_100k, T0};
use mqttpanel_engine::{
    ChartSeriesRef, DiffKind, DiffMode, Engine, EngineConfig, ExportFormat, ExportOptions,
    ExportScope, Filters, IncomingMessage, SortDir, TableColumn, TableQuery, TableSort, TreeQuery,
};
use std::time::Instant;

fn q(search: &str) -> TreeQuery {
    TreeQuery {
        search: search.to_string(),
        filters: Filters::default(),
    }
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[test]
fn x01_100k_topics_slice_and_search() {
    let topics = topics_100k();
    assert_eq!(topics.len(), 100_000);
    let mut e = Engine::default();
    let t = Instant::now();
    for (i, topic) in topics.iter().enumerate() {
        e.ingest(msg(
            topic,
            &format!("{{\"value\":{}}}", i % 50),
            T0 + (i as i64 % 10_000),
        ));
    }
    let insert = t.elapsed();
    assert_eq!(e.topic_count(), 100_000);

    // Lookup
    let t = Instant::now();
    let d = e.topic_detail("building/f7/r42/co23");
    let lookup = t.elapsed();
    assert!(d.exists);

    // Slice 60 rows with the tree expanded to depth 3.
    e.expand_to_depth(3);
    let now = T0 + 10_000;
    let first = Instant::now();
    let s = e.tree_slice(&q(""), 0, 60, now);
    let first_slice = first.elapsed();
    assert_eq!(s.rows.len(), 60);
    let t = Instant::now();
    let s2 = e.tree_slice(&q(""), 50_000, 60, now);
    let slice = t.elapsed();
    assert_eq!(s2.rows.len(), 60);
    assert!(s.total > 100_000);
    // A message arrives: cached rows survive (no structural change).
    e.ingest(msg("building/f1/r1/temperature0", "{\"value\":1}", now));
    let t = Instant::now();
    e.tree_slice(&q(""), 70_000, 60, now);
    let slice_after_ingest = t.elapsed();

    let t = Instant::now();
    let sub = e.tree_slice(&q("humidity3"), 0, 60, now);
    let search_sub = t.elapsed();
    // building, floors, rooms, sensors (f10/r100 is not in the fixture: 99 900 building topics)
    assert_eq!(sub.total, 1 + 10 + 999 + 999);
    let t = Instant::now();
    let wild = e.tree_slice(&q("building/+/r7/#"), 0, 60, now);
    let search_wild = t.elapsed();
    assert_eq!(wild.total, 1 + 10 + 10 + 1000);
    let t = Instant::now();
    let re = e.tree_slice(&q("/^building/f10/r99/(co2|voc)\\d$"), 0, 60, now);
    let search_re = t.elapsed();
    assert_eq!(re.total, 1 + 1 + 1 + 20);

    println!(
        "X-01: insert {:.1} ms, lookup {:.3} ms, first slice {:.2} ms, slice {:.3} ms, slice after ingest {:.3} ms, \
         search substring {:.2} ms, wildcard {:.2} ms, regex {:.2} ms",
        ms(insert),
        ms(lookup),
        ms(first_slice),
        ms(slice),
        ms(slice_after_ingest),
        ms(search_sub),
        ms(search_wild),
        ms(search_re)
    );
    #[cfg(not(debug_assertions))]
    {
        assert!(ms(slice) < 2.0, "slice of 60 rows under 2 ms");
        assert!(ms(slice_after_ingest) < 2.0);
        assert!(ms(search_sub) < 50.0, "search under 50 ms");
        assert!(ms(search_wild) < 50.0);
        assert!(ms(search_re) < 50.0);
    }
}

#[test]
fn x02_ring_buffer_sizes() {
    for size in [10usize, 200, 10_000] {
        let mut e = Engine::new(EngineConfig {
            buffer_per_topic: size,
            ..EngineConfig::default()
        });
        let n = size + 25;
        for i in 0..n {
            e.ingest(msg("buf/t", &i.to_string(), T0 + i as i64));
        }
        let h = e.history("buf/t");
        assert_eq!(h.len(), size);
        assert_eq!(h[0].preview, (n - size).to_string(), "oldest dropped");
        let d = e.topic_detail("buf/t");
        assert_eq!(d.messages, n as u64, "count since connect keeps growing");
        assert_eq!(d.buffer_size, size as u64);
        assert_eq!(d.history_count, size as u64);
    }
    // Per-topic override and clamping.
    let mut e = Engine::default();
    for i in 0..300 {
        e.ingest(msg("a", &i.to_string(), T0 + i));
    }
    e.set_topic_buffer("a", 50);
    assert_eq!(e.history("a").len(), 50);
    e.set_topic_buffer("a", 1);
    assert_eq!(e.topic_detail("a").buffer_size, 10);
    // Sequence numbers are unique per connection.
    let h = e.history("a");
    assert!(h.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[test]
fn x03_counts_since_connect() {
    let mut e = Engine::default();
    let replay = [("a/b/c", 3), ("a/b/d", 2), ("a/e", 4), ("a", 1), ("x", 5)];
    let mut t = T0;
    for (topic, n) in replay {
        for _ in 0..n {
            e.ingest(msg(topic, "1", t));
            t += 10;
        }
    }
    e.expand_to_depth(5);
    let rows = e.tree_slice(&q(""), 0, 100, t).rows;
    let get = |p: &str| rows.iter().find(|r| r.path == p).cloned().expect(p);
    assert_eq!(get("a").messages, 10);
    assert_eq!(get("a/b").messages, 5);
    assert_eq!(get("a/b/c").messages, 3);
    assert_eq!(get("a/e").messages, 4);
    assert_eq!(get("x").messages, 5);
    assert_eq!(get("a").topic_count, 4);
    assert_eq!(get("a/b").topic_count, 2);
    assert!(get("a").is_topic && !get("a/b").is_topic);

    // Reconnect resets.
    e.reset_counts();
    e.ingest(msg("a/b/c", "2", t));
    let rows = e.tree_slice(&q(""), 0, 100, t).rows;
    let get = |p: &str| rows.iter().find(|r| r.path == p).cloned().expect(p);
    assert_eq!(get("a").messages, 1);
    assert_eq!(get("a/b/c").messages, 1);
    assert_eq!(get("x").messages, 0);

    // Clear history on a subtree.
    e.ingest(msg("x", "2", t));
    e.clear_history(Some("a/b"));
    let rows = e.tree_slice(&q(""), 0, 100, t).rows;
    let get = |p: &str| rows.iter().find(|r| r.path == p).cloned().expect(p);
    assert_eq!(get("a").messages, 0);
    assert_eq!(get("x").messages, 1);
    assert!(e.history("a/b/c").is_empty());
    assert_eq!(e.history("x").len(), 6);
    e.clear_history(None);
    assert!(e.history("x").is_empty());
    assert_eq!(e.memory().messages, 0);
}

#[test]
fn x11_diff_json_and_text() {
    let mut e = Engine::default();
    e.ingest(msg(
        "ahu/state",
        r#"{"supply":{"temp":18.5,"fan":true},"mode":"auto","list":[1,2,3],"gone":1}"#,
        T0,
    ));
    e.ingest(msg(
        "ahu/state",
        r#"{"supply":{"temp":19.0,"fan":true},"mode":"auto","list":[1,5],"new":{"a":1}}"#,
        T0 + 1000,
    ));
    e.ingest(msg(
        "ahu/state",
        r#"{"supply":{"temp":19.5,"fan":false},"mode":"eco","list":[1,5],"new":{"a":1}}"#,
        T0 + 2000,
    ));
    let h = e.history("ahu/state");
    let d = e.diff("ahu/state", h[0].seq, h[1].seq);
    assert_eq!(d.mode, DiffMode::Json);
    let find = |path: &str| {
        d.lines
            .iter()
            .find(|l| l.path.as_deref() == Some(path))
            .cloned()
            .expect(path)
    };
    let temp = find("$.supply.temp");
    assert_eq!(temp.kind, DiffKind::Changed);
    assert_eq!(temp.old_value.as_deref(), Some("18.5"));
    assert_eq!(temp.new_value.as_deref(), Some("19.0"));
    assert!(temp.plottable);
    assert_eq!(temp.indent, 2);
    assert_eq!(find("$.supply.fan").kind, DiffKind::Same);
    assert_eq!(find("$.list[1]").kind, DiffKind::Changed);
    assert_eq!(find("$.list[2]").kind, DiffKind::Removed);
    assert_eq!(find("$.gone").kind, DiffKind::Removed);
    assert_eq!(find("$.new").kind, DiffKind::Added);
    assert_eq!(find("$.new.a").kind, DiffKind::Added);
    assert!(!find("$.mode").plottable);
    // changed: temp, list[1]; removed: list[2], gone; added: new.a
    assert_eq!(d.changes, 5);
    assert_eq!(
        d.lines.first().and_then(|l| l.open.clone()).as_deref(),
        Some("{")
    );
    assert_eq!(
        d.lines.last().and_then(|l| l.close.clone()).as_deref(),
        Some("}")
    );

    // Baseline from history: first against third.
    let d13 = e.diff("ahu/state", h[0].seq, h[2].seq);
    assert!(d13.changes > d.changes);
    // Unknown baseline falls back to the previous message.
    let prev = e.diff("ahu/state", 0, h[2].seq);
    assert_eq!(prev.a, h[1].seq);
    assert_eq!(prev.changes, 3, "{:#?}", prev.lines); // temp, fan, mode

    // History changed keys.
    assert!(h[0].changed_keys.is_empty());
    assert_eq!(h[2].changed_keys, vec!["supply.temp", "supply.fan", "mode"]);

    // Text line diff.
    e.ingest(msg("log/text", "line one\nline two\nline three", T0));
    e.ingest(msg(
        "log/text",
        "line one\nline 2\nline three\nline four",
        T0 + 1,
    ));
    let h = e.history("log/text");
    assert_eq!(h[1].changed_keys, vec!["payload"]);
    let d = e.diff("log/text", h[0].seq, h[1].seq);
    assert_eq!(d.mode, DiffMode::Text);
    assert_eq!(d.changes, 3);
    assert!(d
        .text_lines
        .iter()
        .any(|l| l.kind == DiffKind::Removed && l.text == "line two"));
    assert!(d
        .text_lines
        .iter()
        .any(|l| l.kind == DiffKind::Added && l.text == "line four"));
}

#[test]
fn tree_collapsed_by_default_natural_order_and_expansion() {
    let mut e = Engine::default();
    for t in ["b/r10", "b/r2", "b/r1", "$SYS/uptime", "a//x", "/lead", "c"] {
        e.ingest(msg(t, "1", T0));
    }
    let rows = e.tree_slice(&q(""), 0, 100, T0).rows;
    let paths: Vec<&str> = rows.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, vec!["", "$SYS", "a", "b", "c"]);
    e.set_expanded("b", true);
    let rows = e.tree_slice(&q(""), 0, 100, T0).rows;
    let paths: Vec<&str> = rows.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["", "$SYS", "a", "b", "b/r1", "b/r2", "b/r10", "c"]
    );
    assert!(rows[3].expanded && rows[3].has_children);
    assert_eq!(rows[4].depth, 1);
    // Empty segments are segments.
    e.set_expanded("a", true);
    e.set_expanded("a/", true);
    assert!(e
        .tree_slice(&q(""), 0, 100, T0)
        .rows
        .iter()
        .any(|r| r.path == "a//x" && r.depth == 2));
    // Saved expansion round trip, including paths not seen yet.
    let mut saved = e.expanded();
    saved.push("later/x".into());
    e.collapse_all();
    assert_eq!(e.tree_slice(&q(""), 0, 100, T0).total, 5);
    e.set_expanded_set(saved.clone());
    e.ingest(msg("later/x/y", "1", T0));
    assert!(e.expanded().contains(&"later/x".to_string()));
    // Reveal expands ancestors.
    e.collapse_all();
    let idx = e.reveal_topic("b/r10", &q(""));
    assert_eq!(idx, 6);
    assert_eq!(e.reveal_topic("nope", &q("")), -1);
}

#[test]
fn tree_search_filters_and_values() {
    let mut e = Engine::default();
    e.ingest(IncomingMessage {
        retain: true,
        qos: 1,
        ..msg("home/kitchen/temp", "{\"value\":21.5}", T0)
    });
    e.ingest(msg("home/kitchen/light", "ON", T0 + 1000));
    e.ingest(msg("home/garage/door", "closed", T0 + 2000));
    let mut cam = msg("home/garage/cam", "", T0 + 2000);
    cam.payload = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0];
    e.ingest(cam);
    let mut bin = msg("home/garage/raw", "", T0);
    bin.payload = vec![0, 1, 2, 3, 0xff];
    e.ingest(bin);
    let s = e.tree_slice(&q("kitchen"), 0, 100, T0 + 3000);
    let paths: Vec<&str> = s.rows.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "home",
            "home/kitchen",
            "home/kitchen/light",
            "home/kitchen/temp"
        ]
    );
    assert!(
        s.rows[0].expanded,
        "ancestors auto-expanded during a search"
    );
    assert!(e.expanded().is_empty(), "stored expansion unchanged");
    let temp = &s.rows[3];
    assert_eq!(temp.value.as_deref(), Some("{\"value\":21.5}"));
    // Change set 8: a JSON payload draws no sparkline until a value is picked.
    assert!(temp.spark.is_empty());
    assert!(temp.retained);
    // Filters.
    let retained = TreeQuery {
        search: String::new(),
        filters: Filters {
            retained_only: true,
            ..Filters::default()
        },
    };
    assert_eq!(e.tree_slice(&retained, 0, 100, T0).total, 3);
    let contains = TreeQuery {
        search: String::new(),
        filters: Filters {
            payload_contains: "CLOS".into(),
            ..Filters::default()
        },
    };
    let r = e.tree_slice(&contains, 0, 100, T0).rows;
    assert_eq!(r.last().map(|r| r.path.as_str()), Some("home/garage/door"));
    let changed = TreeQuery {
        search: String::new(),
        filters: Filters {
            changed_within_min: Some(0.04),
            ..Filters::default()
        },
    };
    // within 2.4 s of T0+3000: light (T0+1000) and garage (T0+2000) topics
    let r = e.tree_slice(&changed, 0, 100, T0 + 3000).rows;
    assert!(r.iter().any(|r| r.path == "home/garage/door"));
    assert!(!r.iter().any(|r| r.path == "home/kitchen/temp"));
    let qos1 = TreeQuery {
        search: String::new(),
        filters: Filters {
            qos: Some(1),
            ..Filters::default()
        },
    };
    assert_eq!(
        e.tree_slice(&qos1, 0, 100, T0)
            .rows
            .last()
            .map(|r| r.path.clone())
            .as_deref(),
        Some("home/kitchen/temp")
    );
    // Binary preview.
    e.expand_to_depth(3);
    let rows = e.tree_slice(&q(""), 0, 100, T0 + 3000).rows;
    let raw = rows
        .iter()
        .find(|r| r.path == "home/garage/raw")
        .expect("raw");
    assert_eq!(raw.value.as_deref(), Some("<binary 5 B>"));
    // Retained deletion clears the flag.
    e.ingest(IncomingMessage {
        retain: true,
        ..msg("home/kitchen/temp", "", T0 + 4000)
    });
    assert_eq!(e.tree_slice(&retained, 0, 100, T0).total, 0);
    assert!(e.retained_under("home").is_empty());
}

#[test]
fn table_sorting_and_pins() {
    let mut e = Engine::default();
    let data = [
        ("s/a", "5", 3),
        ("s/b", "40", 1),
        ("s/c", "text", 2),
        ("s/d", "-1", 5),
    ];
    let mut t = T0;
    for (topic, payload, n) in data {
        for _ in 0..n {
            e.ingest(msg(topic, payload, t));
            t += 100;
        }
    }
    let tq = |col, dir| TableQuery {
        sort: TableSort { column: col, dir },
        ..TableQuery::default()
    };
    let order = |e: &mut Engine, col, dir| -> Vec<String> {
        e.table_slice(&tq(col, dir), 0, 10, t)
            .rows
            .into_iter()
            .map(|r| r.path)
            .collect()
    };
    assert_eq!(
        order(&mut e, TableColumn::Topic, SortDir::Asc),
        vec!["s/a", "s/b", "s/c", "s/d"]
    );
    assert_eq!(
        order(&mut e, TableColumn::Messages, SortDir::Desc),
        vec!["s/d", "s/a", "s/c", "s/b"]
    );
    assert_eq!(
        order(&mut e, TableColumn::Value, SortDir::Asc),
        vec!["s/d", "s/a", "s/b", "s/c"]
    );
    assert_eq!(order(&mut e, TableColumn::Rate, SortDir::Desc)[0], "s/d");
    assert_eq!(order(&mut e, TableColumn::Updated, SortDir::Desc)[0], "s/d");
    assert_eq!(order(&mut e, TableColumn::Size, SortDir::Desc)[0], "s/c");
    for col in [
        TableColumn::Pin,
        TableColumn::Spark,
        TableColumn::Qos,
        TableColumn::Retained,
    ] {
        assert_eq!(order(&mut e, col, SortDir::Asc).len(), 4);
    }
    e.set_pinned(vec!["s/c".into()]);
    assert_eq!(
        order(&mut e, TableColumn::Topic, SortDir::Asc),
        vec!["s/c", "s/a", "s/b", "s/d"]
    );
    assert_eq!(
        order(&mut e, TableColumn::Topic, SortDir::Desc),
        vec!["s/c", "s/d", "s/b", "s/a"]
    );
    let s = e.table_slice(
        &TableQuery {
            search: "s/a".into(),
            ..tq(TableColumn::Topic, SortDir::Asc)
        },
        0,
        10,
        t,
    );
    assert_eq!(s.total, 1);
    assert_eq!(s.topics_in_view, 4);
    assert!(e.topic_detail("s/c").pinned);
    assert_eq!(e.known_topics("s/", 2), vec!["s/a", "s/b"]);
    assert_eq!(
        e.next_changed(Some("s/b"), 10_000, t).as_deref(),
        Some("s/c")
    );
    assert_eq!(
        e.next_changed(Some("s/d"), 10_000, t).as_deref(),
        Some("s/a")
    );
    assert!(e.msg_rate(t) > 0.0);
    assert!(e.seq() > 0);
}

#[test]
fn json_shapes_match_the_contract() {
    let mut e = Engine::default();
    e.ingest(msg("j/t", "{\"value\":1}", T0));
    let v = serde_json::to_value(e.tree_slice(&q(""), 0, 10, T0)).unwrap();
    let row = &v["rows"][0];
    for k in [
        "path",
        "name",
        "depth",
        "hasChildren",
        "expanded",
        "topicCount",
        "messages",
        "isTopic",
        "value",
        "retained",
        "lastReceivedAt",
        "active",
        "rate",
        "spark",
        "sparkTimes",
        "formatHint",
        "pinned",
    ] {
        assert!(row.get(k).is_some(), "TreeRow.{k}");
    }
    let tq: TableQuery = serde_json::from_str(
        r#"{"search":"","filters":{"retainedOnly":false,"changedWithinMin":null,"payloadContains":"","qos":null,"hasValue":false},"sort":{"column":"messages","dir":"desc"}}"#,
    )
    .unwrap();
    assert_eq!(tq.sort.column, TableColumn::Messages);
    let d = serde_json::to_value(e.topic_detail("j/t")).unwrap();
    assert_eq!(d["last"]["decoded"]["format"], "json");
    assert_eq!(d["fields"][0]["cast"], "number");
    assert!(
        d["defaultField"].is_null(),
        "no guess on a JSON payload (change set 8)"
    );
    let s = serde_json::to_value(e.series("j/t", "$.value", None)).unwrap();
    assert_eq!(s["castRule"], "number");
    assert_eq!(s["points"][0][1], 1.0);
    let o: ExportOptions = serde_json::from_str(
        r#"{"scope":{"kind":"subtree","prefix":"j"},"fromMs":null,"toMs":null,"format":"ndjson","includeProperties":false,"rawBase64":false}"#,
    )
    .unwrap();
    assert_eq!(o.scope, ExportScope::Subtree { prefix: "j".into() });
    assert_eq!(
        serde_json::to_value(e.memory())
            .unwrap()
            .get("budgetBytes")
            .and_then(|b| b.as_u64()),
        Some(256 * 1024 * 1024)
    );
}

#[test]
fn export_csv_json_ndjson_and_estimate() {
    let mut e = Engine::default();
    e.ingest(msg("plant/a", r#"{"t":20.5,"s":{"p":1}}"#, T0));
    e.ingest(msg("plant/b", "hello, \"world\"", T0 + 1000));
    let mut bin = msg("plant/c", "", T0 + 2000);
    bin.payload = vec![0, 159, 146, 150];
    e.ingest(bin);
    e.ingest(msg("other/x", "1", T0 + 3000));
    let opts = |format, scope| ExportOptions {
        scope,
        from_ms: None,
        to_ms: None,
        format,
        include_properties: false,
        raw_base64: false,
    };
    let sub = ExportScope::Subtree {
        prefix: "plant".into(),
    };
    let mut out = Vec::new();
    e.export(&opts(ExportFormat::Csv, sub.clone()), &mut out)
        .unwrap();
    let csv = String::from_utf8(out.clone()).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "time,topic,payload,qos,retained,t,s.p");
    assert!(lines[1].starts_with("2025-10-09T08:53:20.000Z,plant/a,"));
    assert!(lines[1].ends_with(",0,false,20.5,1"));
    assert!(lines[2].contains("\"hello, \"\"world\"\"\""));
    assert!(lines[3].contains("AJ+Slg=="), "binary as base64");
    assert_eq!(lines.len(), 4);
    let est = e.export_estimate(&opts(ExportFormat::Csv, sub.clone()));
    assert_eq!(est.messages, 3);
    assert_eq!(est.topics, 3);
    assert_eq!(est.bytes, out.len() as u64);

    let mut out = Vec::new();
    e.export(&opts(ExportFormat::Json, sub.clone()), &mut out)
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v.as_array().map(Vec::len), Some(3));
    assert_eq!(
        e.export_estimate(&opts(ExportFormat::Json, sub.clone()))
            .bytes,
        out.len() as u64
    );

    let mut out = Vec::new();
    let o = ExportOptions {
        from_ms: Some(T0 + 500),
        to_ms: Some(T0 + 2500),
        ..opts(
            ExportFormat::Ndjson,
            ExportScope::Topics {
                topics: vec!["plant/b".into(), "plant/c".into(), "other/x".into()],
            },
        )
    };
    e.export(&o, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 2);
    for l in text.lines() {
        serde_json::from_str::<serde_json::Value>(l).unwrap();
    }
    assert_eq!(e.export_estimate(&o).messages, 2);
}

#[test]
fn chart_csv_union_of_timestamps() {
    let mut e = Engine::default();
    e.ingest(msg("a", "{\"v\":1}", T0));
    e.ingest(msg("b", "{\"w\":10}", T0 + 500));
    e.ingest(msg("a", "{\"v\":2}", T0 + 1000));
    let csv = e.export_chart_csv(
        &[
            ChartSeriesRef {
                name: "A, v".into(),
                topic: "a".into(),
                path: "$.v".into(),
            },
            ChartSeriesRef {
                name: "B".into(),
                topic: "b".into(),
                path: "$.w".into(),
            },
        ],
        None,
    );
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "time,\"A, v\",B");
    assert_eq!(lines[1], "2025-10-09T08:53:20.000Z,1,");
    assert_eq!(lines[2], "2025-10-09T08:53:20.500Z,,10");
    assert_eq!(lines[3], "2025-10-09T08:53:21.000Z,2,");
}

#[test]
fn fields_for_the_picker() {
    let mut e = Engine::default();
    e.ingest(msg(
        "ahu01/state",
        r#"{"supply":{"temp":18.5,"fan":"on"},"setpoint":"21","alarm":{"active":false},"mode":"auto","ts":"2026-10-12T08:00:00Z"}"#,
        T0,
    ));
    let f = e.fields("ahu01/state");
    let plottable: Vec<&str> = f
        .iter()
        .filter(|x| x.plottable)
        .map(|x| x.label.as_str())
        .collect();
    assert_eq!(
        plottable,
        vec!["supply.temp", "supply.fan", "setpoint", "alarm.active"]
    );
    let text: Vec<&str> = f
        .iter()
        .filter(|x| !x.plottable)
        .map(|x| x.label.as_str())
        .collect();
    assert_eq!(text, vec!["mode", "ts"]);
    // A path that is absent from the sample can still be asked for.
    assert!(e
        .series("ahu01/state", "$.supply.pressure", None)
        .points
        .is_empty());
    // Non-JSON payload: a single "$" entry labelled value.
    e.ingest(msg("plain", "hello", T0));
    let f = e.fields("plain");
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].label, "value");
    assert!(!f[0].plottable);
}

#[test]
fn odd_input_never_panics() {
    let mut e = Engine::default();
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let topics = [
        "/",
        "//",
        "a/+/#",
        "$SYS",
        "spBv1.0/g/NDATA/e",
        "spBv1.0/g/DBIRTH/e/d",
        "x/\u{e9}/\u{1f600}",
        "",
    ];
    for i in 0..2000 {
        let len = (next() % 64) as usize;
        let payload: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        let topic = topics[i % topics.len()];
        let t = match i % 5 {
            0 => i64::MIN / 2,
            1 => i64::MAX / 2,
            _ => T0 + i as i64,
        };
        let mut m = msg(topic, "", t);
        m.payload = payload;
        m.retain = i % 3 == 0;
        e.ingest(m);
    }
    for search in ["", "/(", "/.*", "+", "#", "\u{e9}", "a/+/#"] {
        let q = TreeQuery {
            search: search.into(),
            filters: Filters {
                changed_within_min: Some(f64::MAX),
                ..Filters::default()
            },
        };
        e.tree_slice(&q, usize::MAX, usize::MAX, i64::MAX);
        e.tree_slice(&q, 0, 100, i64::MIN);
        e.table_slice(
            &TableQuery {
                search: search.into(),
                ..TableQuery::default()
            },
            0,
            100,
            0,
        );
    }
    for t in topics {
        let d = e.topic_detail(t);
        e.history(t);
        e.fields(t);
        e.diff(t, 0, u64::MAX);
        for p in ["$", "$.", "$[", "$[\"", "$.a[99999999999999999999]", "a..b"] {
            e.series(t, p, Some(i64::MIN));
        }
        if let Some(last) = d.last {
            for f in [
                mqttpanel_engine::PayloadFormat::Json,
                mqttpanel_engine::PayloadFormat::Cbor,
                mqttpanel_engine::PayloadFormat::Msgpack,
                mqttpanel_engine::PayloadFormat::Sparkplug,
                mqttpanel_engine::PayloadFormat::Protobuf,
                mqttpanel_engine::PayloadFormat::Base64,
                mqttpanel_engine::PayloadFormat::Image,
                mqttpanel_engine::PayloadFormat::Text,
            ] {
                e.message(t, last.seq, f, true);
            }
        }
    }
    e.next_changed(None, i64::MAX, i64::MIN);
    e.expand_changed(i64::MAX, i64::MIN);
    e.known_topics("", 0);
    e.memory();
}

/// X-16 (change set 8): a bare number drives the sparkline by itself; a JSON payload only after "Use as sparkline";
/// a second pick replaces the first; the pick applies to topics already known and to new messages.
#[test]
fn x16_sparkline_rule() {
    let mut e = Engine::default();
    e.ingest(msg("r/humidity", "41.8", T0));
    e.ingest(msg("r/humidity", "42.0", T0 + 1000));
    e.ingest(msg(
        "r/state",
        r#"{"mode":"cooling","supply":{"temp":18.2,"fan":72}}"#,
        T0,
    ));
    e.ingest(msg(
        "r/state",
        r#"{"mode":"cooling","supply":{"temp":18.4,"fan":70}}"#,
        T0 + 1000,
    ));
    let rows = |e: &mut Engine| {
        e.expand_to_depth(9);
        e.tree_slice(&q(""), 0, 50, T0 + 2000).rows
    };
    let r = rows(&mut e);
    let hum = r.iter().find(|x| x.path == "r/humidity").unwrap();
    assert_eq!(hum.spark, vec![41.8, 42.0]);
    let st = r.iter().find(|x| x.path == "r/state").unwrap();
    assert!(st.spark.is_empty(), "JSON: nothing until picked");
    assert_eq!(e.topic_detail("r/state").default_field, None);

    let mut picks = std::collections::HashMap::new();
    picks.insert("r/state".to_string(), "$.supply.temp".to_string());
    e.set_sparkline_paths(picks.clone());
    let r = rows(&mut e);
    let st = r.iter().find(|x| x.path == "r/state").unwrap();
    assert_eq!(
        st.spark,
        vec![18.2, 18.4],
        "history recorded before the pick is used"
    );
    assert_eq!(
        e.topic_detail("r/state").default_field.as_deref(),
        Some("$.supply.temp")
    );

    picks.insert("r/state".to_string(), "$.supply.fan".to_string());
    e.set_sparkline_paths(picks);
    e.ingest(msg(
        "r/state",
        r#"{"mode":"cooling","supply":{"temp":18.5,"fan":68}}"#,
        T0 + 1500,
    ));
    let r = rows(&mut e);
    let st = r.iter().find(|x| x.path == "r/state").unwrap();
    assert_eq!(
        st.spark,
        vec![72.0, 70.0, 68.0],
        "one sparkline per topic: the second pick replaces the first"
    );

    // A text value never drives a sparkline.
    let mut picks = std::collections::HashMap::new();
    picks.insert("r/state".to_string(), "$.mode".to_string());
    e.set_sparkline_paths(picks);
    let r = rows(&mut e);
    assert!(r
        .iter()
        .find(|x| x.path == "r/state")
        .unwrap()
        .spark
        .is_empty());
}
