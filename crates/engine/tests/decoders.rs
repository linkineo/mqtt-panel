//! TESTPLAN X-12: decoders, replayed from the corpus files.

mod common;

use common::{engine_with, load_text, msg, T0};
use mqttpanel_engine::{
    decode_payload, detect, proto_message_types, DecodeOptions, Engine, IncomingMessage,
    Mqtt5Props, PayloadFormat, ProtoAssignment, ProtoFile, TreeQuery,
};
use serde_json::{json, Value};

fn dec(bytes: &[u8], f: PayloadFormat) -> mqttpanel_engine::DecodedPayload {
    decode_payload(bytes, f, None, "t", &DecodeOptions::default())
}

#[test]
fn x12_json_bom_and_comments() {
    let d = dec(b"\xEF\xBB\xBF{\"a\":1}", PayloadFormat::Auto);
    assert_eq!(d.detected, PayloadFormat::Json);
    assert_eq!(d.json, Some(json!({"a": 1})));
    assert_eq!(d.pretty.as_deref(), Some("{\n  \"a\": 1\n}"));
    let c = dec(b"{\"a\":1 // note\n}", PayloadFormat::Auto);
    assert_eq!(c.detected, PayloadFormat::Text, "comments are not JSON");
    let forced = dec(b"{\"a\":1 // note\n}", PayloadFormat::Json);
    let err = forced.error.expect("error");
    assert_eq!(err.key, "decode.json");
    assert!(err.text.starts_with("Not valid JSON: "));
    assert!(forced.hex.is_some(), "raw bytes still shown");
    // Field order preserved.
    let o = dec(b"{\"z\":1,\"a\":2}", PayloadFormat::Auto);
    assert_eq!(o.pretty.as_deref(), Some("{\n  \"z\": 1,\n  \"a\": 2\n}"));
}

#[test]
fn x12_text_binary_base64_image() {
    let t = dec("grüezi\tworld\r\n".as_bytes(), PayloadFormat::Auto);
    assert_eq!(t.detected, PayloadFormat::Text);
    assert_eq!(t.text.as_deref(), Some("grüezi\tworld\r\n"));
    let b = dec(&[0x00, 0x01, 0x41, 0xff], PayloadFormat::Auto);
    assert_eq!(b.detected, PayloadFormat::Hex);
    assert!(b.hex.as_deref().unwrap_or("").contains("00 01 41 ff"));
    assert_eq!(b.base64.as_deref(), Some("AAFB/w=="));
    let b64 = dec(b"aGVsbG8=", PayloadFormat::Base64);
    assert_eq!(b64.text.as_deref(), Some("hello"));
    let bad = dec(b"not base64!!", PayloadFormat::Base64);
    assert_eq!(bad.error.map(|e| e.key).as_deref(), Some("decode.base64"));
    let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    let i = dec(&png, PayloadFormat::Auto);
    assert_eq!(i.detected, PayloadFormat::Image);
    assert!(i
        .image_data_url
        .as_deref()
        .unwrap_or("")
        .starts_with("data:image/png;base64,"));
    let ct = decode_payload(
        b"\x00\x01",
        PayloadFormat::Auto,
        Some("image/x-icon"),
        "t",
        &DecodeOptions::default(),
    );
    assert_eq!(ct.detected, PayloadFormat::Image);
    let not = dec(b"plain", PayloadFormat::Image);
    assert_eq!(not.error.map(|e| e.key).as_deref(), Some("decode.notImage"));
    // Forced hex on text.
    let h = dec(b"AB", PayloadFormat::Hex);
    assert_eq!(h.format, PayloadFormat::Hex);
    assert!(h.hex.unwrap().starts_with("00000000  41 42"));
}

#[test]
fn x12_paged_above_render_limit() {
    let big = vec![b'a'; 300 * 1024];
    let d = dec(&big, PayloadFormat::Auto);
    assert!(d.paged);
    assert!(d.text.is_none() && d.base64.is_none());
    assert_eq!(
        d.hex.as_deref().map(|h| h.lines().count()),
        Some(64 * 1024 / 16)
    );
    let opts = DecodeOptions {
        format_anyway: true,
        ..DecodeOptions::default()
    };
    let full = decode_payload(&big, PayloadFormat::Auto, None, "t", &opts);
    assert!(!full.paged);
    assert_eq!(full.text.map(|t| t.len()), Some(300 * 1024));
}

#[test]
fn x12_cbor() {
    let e = engine_with("cbor.json");
    let d = e.topic_detail("lab/cbor/sensor");
    let last = d.last.expect("last");
    assert_eq!(last.decoded.detected, PayloadFormat::Cbor);
    assert_eq!(
        last.decoded.json,
        Some(json!({"temp": 22.0, "ok": false, "raw": "BA==", "7": "seven", "list": [1]}))
    );
    let s = e.series("lab/cbor/sensor", "$.temp", None);
    assert_eq!(
        s.points.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![21.5, 22.0]
    );
    let ok = e.series("lab/cbor/sensor", "$.ok", None);
    assert!(ok.step);
    let broken = e.topic_detail("lab/cbor/broken").last.expect("last");
    assert_eq!(
        broken.decoded.error.map(|m| m.key).as_deref(),
        Some("decode.cbor")
    );
    assert!(broken.decoded.hex.is_some());
}

#[test]
fn x12_msgpack() {
    let e = engine_with("msgpack.json");
    let h = e.history("lab/msgpack/sensor");
    assert_eq!(h.len(), 2);
    assert_eq!(h[1].changed_keys, vec!["temp", "fan", "bin"]);
    let m = e
        .message("lab/msgpack/sensor", h[0].seq, PayloadFormat::Auto, false)
        .expect("msg");
    assert_eq!(m.decoded.detected, PayloadFormat::Msgpack);
    assert_eq!(
        m.decoded.json,
        Some(json!({"temp": 19.25, "fan": "on", "3": "three", "bin": "3q0="}))
    );
    let fan = e.series("lab/msgpack/sensor", "$.fan", None);
    assert_eq!(
        fan.points.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![1.0, 0.0]
    );
    // Forced msgpack on JSON text fails with the catalog key.
    let forced = dec(b"{\"a\":1}", PayloadFormat::Msgpack);
    assert_eq!(
        forced.error.map(|m| m.key).as_deref(),
        Some("decode.msgpack")
    );
}

#[test]
fn x12_sparkplug_birth_data_and_aliases() {
    let mut e = engine_with("sparkplug.json");
    let nd = e.topic_detail("spBv1.0/plant/NDATA/edge1");
    let h = e.history("spBv1.0/plant/NDATA/edge1");
    let first = e
        .message(
            "spBv1.0/plant/NDATA/edge1",
            h[0].seq,
            PayloadFormat::Auto,
            false,
        )
        .expect("ndata");
    assert_eq!(first.decoded.detected, PayloadFormat::Sparkplug);
    let metrics = first
        .decoded
        .json
        .as_ref()
        .and_then(|j| j["metrics"].as_array().cloned())
        .expect("metrics");
    assert_eq!(
        metrics[0]["name"], "Temperature",
        "alias resolved from NBIRTH"
    );
    assert_eq!(metrics[0]["datatype"], "Double");
    assert_eq!(metrics[0]["value"], 22.5);
    assert_eq!(metrics[1]["name"], "Counter");
    assert_eq!(metrics[1]["value"], -7, "Int32 from the birth data type");
    assert_eq!(
        first.decoded.json.as_ref().map(|j| j["seq"].clone()),
        Some(json!(1))
    );
    // Series by metric name.
    let temp = e.series("spBv1.0/plant/NDATA/edge1", "$.Temperature", None);
    assert_eq!(
        temp.points.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![22.5, 23.0]
    );
    // Change set 8: a structured payload has no sparkline until a value is picked.
    assert_eq!(nd.default_field, None);
    e.set_sparkline_paths(
        [(
            "spBv1.0/plant/NDATA/edge1".to_string(),
            "$.Temperature".to_string(),
        )]
        .into(),
    );
    assert_eq!(
        e.topic_detail("spBv1.0/plant/NDATA/edge1")
            .default_field
            .as_deref(),
        Some("$.Temperature")
    );
    let birth = e.fields("spBv1.0/plant/NBIRTH/edge1");
    let labels: Vec<&str> = birth.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "bdSeq",
            "Node Control/Rebirth",
            "Temperature",
            "Pressure",
            "Counter",
            "Mode"
        ]
    );
    assert!(!birth[5].plottable);
    // Device aliases from DBIRTH.
    let speed = e.series("spBv1.0/plant/DDATA/edge1/pump1", "$.Speed", None);
    assert_eq!(
        speed.points.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![1500.0]
    );
    // STATE is JSON, not protobuf.
    let st = e.topic_detail("spBv1.0/STATE/scada1").last.expect("state");
    assert_eq!(st.decoded.detected, PayloadFormat::Json);
    // Format hint on the spBv1.0 root.
    let rows = e.tree_slice(&TreeQuery::default(), 0, 10, T0).rows;
    assert_eq!(rows[0].path, "spBv1.0");
    assert_eq!(rows[0].format_hint.as_deref(), Some("Sparkplug B"));
    // Broken payload on a Sparkplug topic.
    e.ingest(msg("spBv1.0/plant/NDATA/edge9", "\u{1}garbage", T0));
    let bad = e
        .topic_detail("spBv1.0/plant/NDATA/edge9")
        .last
        .expect("bad");
    assert_eq!(
        bad.decoded.error.map(|m| m.key).as_deref(),
        Some("decode.sparkplug")
    );
}

fn proto_setup() -> (Engine, Value) {
    let v: Value = serde_json::from_str(&load_text("proto.json")).expect("proto corpus");
    let files: Vec<ProtoFile> = serde_json::from_value(v["files"].clone()).expect("files");
    let assignments: Vec<ProtoAssignment> =
        serde_json::from_value(v["assignments"].clone()).expect("assignments");
    let mut e = Engine::default();
    e.set_proto_schemas(files, assignments)
        .expect("schemas compile");
    let events: Vec<common::Event> = serde_json::from_value(v["events"].clone()).expect("events");
    common::replay(&mut e, &events);
    (e, v)
}

#[test]
fn x12_protobuf_with_imports() {
    let (e, v) = proto_setup();
    let files: Vec<ProtoFile> = serde_json::from_value(v["files"].clone()).unwrap();
    assert_eq!(
        proto_message_types(&files).unwrap(),
        vec!["plant.Alarm", "plant.Location", "plant.Telemetry"]
    );
    let d = e.topic_detail("plant/line1/telemetry");
    let last = d.last.expect("last");
    assert_eq!(last.decoded.detected, PayloadFormat::Protobuf);
    let j = last.decoded.json.expect("json");
    assert_eq!(j["temperature"], 62.0);
    assert_eq!(j["location"]["site"], "north");
    assert_eq!(j["unit"], "CELSIUS");
    assert_eq!(j["counter"], 43);
    let s = e.series("plant/line1/telemetry", "$.temperature", None);
    assert_eq!(
        s.points.iter().map(|p| p.1).collect::<Vec<_>>(),
        vec![61.5, 62.0]
    );
    let alarm = e.topic_detail("plant/line1/alarm").last.expect("alarm");
    assert_eq!(
        alarm.decoded.json.as_ref().map(|j| j["text"].clone()),
        Some(json!("overheat"))
    );
}

#[test]
fn x12_protobuf_wrong_message_type() {
    let (e, v) = proto_setup();
    let expected = v["events"][3]["expectError"]
        .as_str()
        .expect("expected text");
    let wrong = e.topic_detail("plant/line2/telemetry").last.expect("wrong");
    let err = wrong.decoded.error.expect("decode error");
    assert_eq!(err.key, "decode.protobuf");
    assert_eq!(err.text, expected);
    assert_eq!(err.params["type"], "plant.Telemetry");
    assert!(wrong.decoded.hex.is_some(), "raw bytes shown");
}

#[test]
fn x12_protobuf_schema_errors() {
    let bad = vec![ProtoFile {
        name: "x.proto".into(),
        content: "syntax = \"proto3\"; message {".into(),
    }];
    let err = proto_message_types(&bad).unwrap_err();
    assert_eq!(err.key, "file.notProto");
    let missing_import = vec![ProtoFile {
        name: "y.proto".into(),
        content: "syntax = \"proto3\"; import \"nope.proto\"; message Y { int32 a = 1; }".into(),
    }];
    assert!(proto_message_types(&missing_import).is_err());
    let mut e = Engine::default();
    let unknown = e.set_proto_schemas(
        vec![ProtoFile {
            name: "z.proto".into(),
            content: "syntax = \"proto3\"; package z; message Z { int32 a = 1; }".into(),
        }],
        vec![ProtoAssignment {
            file_name: "z.proto".into(),
            topic_filter: "#".into(),
            message_type: "z.Missing".into(),
        }],
    );
    assert!(unknown.is_err());
}

#[test]
fn x12_content_type_detection_order() {
    let mut e = Engine::default();
    let props = |ct: &str| {
        Some(Mqtt5Props {
            content_type: Some(ct.into()),
            ..Default::default()
        })
    };
    e.ingest(IncomingMessage {
        props: props("application/cbor; charset=x"),
        ..msg("ct/cbor", "", T0)
    });
    assert_eq!(
        detect(
            &[0xa1, 0x61, 0x61, 0x01],
            Some("application/cbor"),
            "t",
            None
        ),
        PayloadFormat::Cbor
    );
    assert_eq!(
        detect(b"{}", None, "spBv1.0/g/NDATA/e", None),
        PayloadFormat::Sparkplug
    );
    assert_eq!(detect(b"{}", None, "x", None), PayloadFormat::Json);
    assert_eq!(detect(b"23", None, "x", None), PayloadFormat::Json);
    assert_eq!(detect(b"hello", None, "x", None), PayloadFormat::Text);
    assert_eq!(detect(b"a\x01b", None, "x", None), PayloadFormat::Hex);
}
