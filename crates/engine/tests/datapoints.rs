//! TESTPLAN section 3: D-01..D-15, replayed from `corpus/datapoints.json` (D-12 and D-15 are
//! generated in the test).

mod common;

use common::{engine_with, msg, T0};
use mqttpanel_engine::{CastRule, Engine, EngineConfig, PayloadFormat, SeriesData};

fn series(topic: &str, path: &str) -> SeriesData {
    let e = engine_with("datapoints.json");
    e.series(topic, path, None)
}

fn values(s: &SeriesData) -> Vec<f64> {
    s.points.iter().map(|p| p.1).collect()
}

#[test]
fn d01_integers() {
    let s = series("d01/ints", "$.v");
    assert_eq!(
        values(&s),
        vec![0.0, 1.0, -5.0, 4294967296.0, 9007199254740992.0]
    );
    assert!(s.precision_loss, "the last integer loses precision");
    assert_eq!(s.cast_rule, CastRule::Number);
    assert_eq!(s.skipped, 0);
}

#[test]
fn d02_floats() {
    let s = series("d02/floats", "$.v");
    assert_eq!(values(&s), vec![21.5, 0.0, 1e-7, 1.5e300]);
    assert!(s.points[1].1.is_sign_positive(), "-0.0 plots as 0");
    assert!(!s.precision_loss);
}

#[test]
fn d03_numeric_strings() {
    let s = series("d03/strings", "$.v");
    assert_eq!(values(&s), vec![21.5, 21.5, 1000.0]);
    assert_eq!(s.skipped, 3);
    assert_eq!(s.cast_rule, CastRule::ParsedNumber);
    assert!(s
        .first_skip_reason
        .as_deref()
        .unwrap_or("")
        .contains("21,5"));
}

#[test]
fn d04_booleans() {
    let s = series("d04/bools", "$.v");
    assert_eq!(values(&s), vec![1.0, 0.0, 1.0]);
    assert!(s.step);
    assert_eq!(s.cast_rule, CastRule::Boolean);
}

#[test]
fn d05_boolean_like_text() {
    let s = series("d05/boollike", "$.v");
    assert_eq!(
        values(&s),
        vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]
    );
    assert_eq!(s.cast_rule, CastRule::Boolean);
    assert!(s.step);
}

#[test]
fn d06_mixed_over_time() {
    let s = series("d06/mixed", "$.v");
    assert_eq!(values(&s), vec![21.5, 22.0, 1.0]);
    assert_eq!(s.skipped, 4);
    assert!(s.first_skip_reason.as_deref().unwrap_or("").contains("n/a"));
    assert_eq!(s.cast_rule, CastRule::Mixed);
}

#[test]
fn d07_nested_path_and_array_index() {
    let e = engine_with("datapoints.json");
    let a = e.series("d07/nested", "$.supply.temp", None);
    assert_eq!(values(&a), vec![20.5, 21.0, 21.5]);
    let b = e.series("d07/nested", "$.readings[2].v", None);
    assert_eq!(values(&b), vec![3.0, 8.0]);
    assert_eq!(b.skipped, 1, "missing index is skipped, not an error");
    // Same answer for a path given without "$." and through the buffer fallback.
    let c = e.series("d07/nested", "readings[1].v", None);
    assert_eq!(values(&c), vec![2.0, 5.0, 7.0]);
}

#[test]
fn d08_gaps_for_absent_field() {
    let s = series("d08/absent", "$.v");
    assert_eq!(values(&s), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    assert_eq!(s.gaps, vec![4], "gap before the point after the 7 s hole");
    assert_eq!(s.skipped, 6);
    // Configurable factor.
    let mut e = Engine::new(EngineConfig {
        gap_factor: 10.0,
        ..EngineConfig::default()
    });
    common::replay(&mut e, &common::load_events("datapoints.json"));
    assert!(e.series("d08/absent", "$.v", None).gaps.is_empty());
}

#[test]
fn d09_whole_payload_raw_number() {
    let e = engine_with("datapoints.json");
    let s = e.series("d09/raw", "$", None);
    assert_eq!(values(&s), vec![23.0, 24.5, 25.0]);
    let d = e.topic_detail("d09/raw");
    assert_eq!(d.default_field.as_deref(), Some("$"));
    assert_eq!(d.fields.len(), 1);
    assert_eq!(d.fields[0].label, "value");
    // "23" is valid JSON (a number); " 25 " too. Rule: number.
    assert!(matches!(
        s.cast_rule,
        CastRule::Number | CastRule::ParsedNumber
    ));
    // A raw text number that is not JSON is a parsed number.
    let mut e2 = Engine::default();
    e2.ingest(msg("raw/text", "23 ", T0));
    e2.ingest(msg("raw/text", "+24", T0 + 1000));
    let s2 = e2.series("raw/text", "$", None);
    assert_eq!(values(&s2), vec![23.0, 24.0]);
    // "23 " is JSON (a number with trailing whitespace); "+24" is not JSON and parses as text.
    assert_eq!(s2.cast_rule, CastRule::Mixed);
}

#[test]
fn d10_nan_and_infinity() {
    let e = engine_with("datapoints.json");
    let s = e.series("d10/nan", "$.v", None);
    assert_eq!(values(&s), vec![1.0]);
    assert_eq!(s.skipped, 5);
    let whole = e.series("d10/nan", "$", None);
    assert!(whole.points.is_empty());
    // Invalid JSON shows as text.
    let d = e.topic_detail("d10/nan");
    let last = d.last.expect("last message");
    assert_eq!(last.decoded.detected, PayloadFormat::Text);
    assert_eq!(last.decoded.text.as_deref(), Some("Infinity"));
}

#[test]
fn d11_receive_time_not_payload_time() {
    let s = series("d11/ts", "$.v");
    assert_eq!(s.points, vec![(T0, 1.0), (T0 + 1000, 2.0)]);
}

#[test]
fn d12_point_cap_drops_oldest() {
    let mut e = Engine::default();
    for i in 0..6000 {
        e.ingest(msg("cap/t", &format!("{{\"v\":{i}}}"), T0 + i));
    }
    let s = e.series("cap/t", "$.v", None);
    assert_eq!(s.points.len(), 5000);
    assert_eq!(s.points[0].1, 1000.0);
    assert_eq!(s.count, 6000);
    let mem = e.memory();
    assert!(mem.points >= 5000);
    // Points alone take at least 5000 * 16 bytes.
    assert!(mem.bytes > 80_000);
}

#[test]
fn d13_two_series_on_one_topic() {
    let e = engine_with("datapoints.json");
    let a = e.series("d13/two", "$.a", None);
    let b = e.series("d13/two", "$.b", None);
    assert_eq!(values(&a), vec![1.0, 2.0, 3.0]);
    assert_eq!(a.cast_rule, CastRule::Mixed);
    assert_eq!(values(&b), vec![1.0, 0.0]);
    // An automatic series starts with the first plottable value: the earlier "x" is not counted.
    assert_eq!(b.skipped, 0);
    assert_eq!(b.cast_rule, CastRule::Boolean);
}

#[test]
fn d14_out_of_order_kept_in_receive_order() {
    let s = series("d14/order", "$.v");
    assert_eq!(
        s.points,
        vec![(T0 + 5000, 1.0), (T0 + 3000, 2.0), (T0 + 4000, 3.0)]
    );
}

#[test]
fn d15_large_payload_field_at_end() {
    let mut e = Engine::default();
    let filler = "x".repeat(2 * 1024 * 1024);
    let payload = format!("{{\"blob\":\"{filler}\",\"v\":42.5}}");
    assert!(payload.len() > 2 * 1024 * 1024);
    e.ingest(msg("big/json", &payload, T0));
    let s = e.series("big/json", "$.v", None);
    assert_eq!(values(&s), vec![42.5]);
    let d = e.topic_detail("big/json");
    let last = d.last.expect("last");
    assert!(last.truncated, "kept truncated above 1 MiB");
    assert!(last.decoded.paged, "render limit respected");
    assert!(last.decoded.pretty.is_none());
    assert!(last
        .decoded
        .hex
        .as_ref()
        .is_some_and(|h| h.len() < 64 * 1024 * 6));
    let full = e
        .message("big/json", last.seq, PayloadFormat::Auto, true)
        .expect("message");
    assert!(!full.decoded.paged);
}
