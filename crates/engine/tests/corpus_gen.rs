//! Regenerates the binary corpus files (Sparkplug, CBOR, MessagePack, Protobuf).
//! Run with `cargo test -p mqttpanel-engine --test corpus_gen -- --ignored`.
//! The normal tests check that the committed files still match this generator.

mod common;

use base64::Engine as _;
use common::T0;
use mqttpanel_engine::decode::sparkplug::{Metric, MetricValue, Payload};
use prost::Message;
use serde_json::{json, Value};

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn metric(name: Option<&str>, alias: u64, dt: u32, v: MetricValue) -> Metric {
    Metric {
        name: name.map(str::to_string),
        alias: Some(alias),
        timestamp: Some(T0 as u64),
        datatype: if name.is_some() { Some(dt) } else { None },
        value: Some(v),
        ..Default::default()
    }
}

pub fn sparkplug_corpus() -> Value {
    let nbirth = Payload {
        timestamp: Some(T0 as u64),
        seq: Some(0),
        metrics: vec![
            metric(Some("bdSeq"), 0, 8, MetricValue::LongValue(0)),
            metric(
                Some("Node Control/Rebirth"),
                1,
                11,
                MetricValue::BooleanValue(false),
            ),
            metric(Some("Temperature"), 2, 10, MetricValue::DoubleValue(21.5)),
            metric(Some("Pressure"), 3, 9, MetricValue::FloatValue(1.25)),
            metric(Some("Counter"), 4, 3, MetricValue::IntValue((-5i32) as u32)),
            metric(Some("Mode"), 5, 12, MetricValue::StringValue("auto".into())),
        ],
        ..Default::default()
    };
    let ndata = Payload {
        timestamp: Some(T0 as u64 + 1000),
        seq: Some(1),
        metrics: vec![
            metric(None, 2, 0, MetricValue::DoubleValue(22.5)),
            metric(None, 4, 0, MetricValue::IntValue((-7i32) as u32)),
        ],
        ..Default::default()
    };
    let dbirth = Payload {
        timestamp: Some(T0 as u64 + 2000),
        seq: Some(2),
        metrics: vec![metric(Some("Speed"), 10, 2, MetricValue::IntValue(1450))],
        ..Default::default()
    };
    let ddata = Payload {
        timestamp: Some(T0 as u64 + 3000),
        seq: Some(3),
        metrics: vec![metric(None, 10, 0, MetricValue::IntValue(1500))],
        ..Default::default()
    };
    let ndata2 = Payload {
        timestamp: Some(T0 as u64 + 4000),
        seq: Some(4),
        metrics: vec![metric(None, 2, 0, MetricValue::DoubleValue(23.0))],
        ..Default::default()
    };
    json!([
        {"t": T0, "topic": "spBv1.0/plant/NBIRTH/edge1", "payloadB64": b64(&nbirth.encode_to_vec())},
        {"t": T0 + 1000, "topic": "spBv1.0/plant/NDATA/edge1", "payloadB64": b64(&ndata.encode_to_vec())},
        {"t": T0 + 2000, "topic": "spBv1.0/plant/DBIRTH/edge1/pump1", "payloadB64": b64(&dbirth.encode_to_vec())},
        {"t": T0 + 3000, "topic": "spBv1.0/plant/DDATA/edge1/pump1", "payloadB64": b64(&ddata.encode_to_vec())},
        {"t": T0 + 4000, "topic": "spBv1.0/plant/NDATA/edge1", "payloadB64": b64(&ndata2.encode_to_vec())},
        {"t": T0 + 5000, "topic": "spBv1.0/STATE/scada1", "payload": "{\"online\":true,\"timestamp\":1760000005000}"}
    ])
}

pub fn cbor_corpus() -> Value {
    use ciborium::Value as C;
    let docs = [
        C::Map(vec![
            (C::Text("temp".into()), C::Float(21.5)),
            (C::Text("ok".into()), C::Bool(true)),
            (C::Text("raw".into()), C::Bytes(vec![1, 2, 3])),
            (C::Integer(7.into()), C::Text("seven".into())),
            (
                C::Text("list".into()),
                C::Array(vec![C::Integer(1.into()), C::Integer((-2).into())]),
            ),
        ]),
        C::Map(vec![
            (C::Text("temp".into()), C::Float(22.0)),
            (C::Text("ok".into()), C::Bool(false)),
            (C::Text("raw".into()), C::Bytes(vec![4])),
            (C::Integer(7.into()), C::Text("seven".into())),
            (C::Text("list".into()), C::Array(vec![C::Integer(1.into())])),
        ]),
    ];
    let mut out = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(d, &mut buf).expect("cbor");
        out.push(json!({"t": T0 + i as i64 * 1000, "topic": "lab/cbor/sensor", "payloadB64": b64(&buf), "contentType": "application/cbor"}));
    }
    out.push(json!({"t": T0 + 5000, "topic": "lab/cbor/broken", "payloadB64": b64(&[0xbf, 0x61]), "contentType": "application/cbor"}));
    Value::Array(out)
}

pub fn msgpack_corpus() -> Value {
    use rmpv::Value as M;
    let docs = [
        M::Map(vec![
            (M::from("temp"), M::F64(19.25)),
            (M::from("fan"), M::from("on")),
            (M::from(3), M::from("three")),
            (M::from("bin"), M::Binary(vec![0xde, 0xad])),
        ]),
        M::Map(vec![
            (M::from("temp"), M::F64(19.75)),
            (M::from("fan"), M::from("off")),
            (M::from(3), M::from("three")),
            (M::from("bin"), M::Binary(vec![0xbe, 0xef])),
        ]),
    ];
    let mut out = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, d).expect("msgpack");
        let ct = if i == 0 {
            "application/msgpack"
        } else {
            "application/x-msgpack"
        };
        out.push(json!({"t": T0 + i as i64 * 1000, "topic": "lab/msgpack/sensor", "payloadB64": b64(&buf), "contentType": ct}));
    }
    Value::Array(out)
}

pub const COMMON_PROTO: &str = r#"syntax = "proto3";
package plant;

message Location {
  string site = 1;
  string line = 2;
}

enum Unit {
  UNIT_UNSPECIFIED = 0;
  CELSIUS = 1;
  BAR = 2;
}
"#;

pub const TELEMETRY_PROTO: &str = r#"syntax = "proto3";
package plant;

import "plant/common.proto";

message Telemetry {
  string device = 1;
  Location location = 2;
  double temperature = 3;
  Unit unit = 4;
  int64 counter = 5;
  bool running = 6;
}

message Alarm {
  string device = 1;
  int32 code = 2;
  string text = 3;
}
"#;

pub fn proto_corpus() -> Value {
    use mqttpanel_engine::ProtoFile;
    use prost_reflect::DynamicMessage;
    let files = vec![
        ProtoFile {
            name: "plant/common.proto".into(),
            content: COMMON_PROTO.into(),
        },
        ProtoFile {
            name: "plant/telemetry.proto".into(),
            content: TELEMETRY_PROTO.into(),
        },
    ];
    mqttpanel_engine::ProtoRegistry::new(&files, vec![]).expect("schemas compile in the engine");
    let pool = {
        struct R;
        impl protox::file::FileResolver for R {
            fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
                match name {
                    "plant/common.proto" => protox::file::File::from_source(name, COMMON_PROTO),
                    "plant/telemetry.proto" => {
                        protox::file::File::from_source(name, TELEMETRY_PROTO)
                    }
                    _ => Err(protox::Error::file_not_found(name)),
                }
            }
        }
        let mut c = protox::Compiler::with_file_resolver(R);
        c.open_file("plant/telemetry.proto").expect("compile");
        c.descriptor_pool()
    };
    let enc = |ty: &str, v: Value| -> String {
        let desc = pool.get_message_by_name(ty).expect("type");
        let msg = DynamicMessage::deserialize(desc, v).expect("message from json");
        b64(&msg.encode_to_vec())
    };
    json!({
        "files": [
            {"name": "plant/common.proto", "content": COMMON_PROTO},
            {"name": "plant/telemetry.proto", "content": TELEMETRY_PROTO}
        ],
        "assignments": [
            {"fileName": "plant/telemetry.proto", "topicFilter": "plant/+/telemetry", "messageType": "plant.Telemetry"},
            {"fileName": "plant/telemetry.proto", "topicFilter": "plant/+/alarm", "messageType": "plant.Alarm"}
        ],
        "events": [
            {"t": T0, "topic": "plant/line1/telemetry", "payloadB64": enc("plant.Telemetry", json!({"device": "m1", "location": {"site": "north", "line": "1"}, "temperature": 61.5, "unit": "CELSIUS", "counter": 42, "running": true}))},
            {"t": T0 + 1000, "topic": "plant/line1/telemetry", "payloadB64": enc("plant.Telemetry", json!({"device": "m1", "location": {"site": "north", "line": "1"}, "temperature": 62.0, "unit": "CELSIUS", "counter": 43, "running": true}))},
            {"t": T0 + 2000, "topic": "plant/line1/alarm", "payloadB64": enc("plant.Alarm", json!({"device": "m1", "code": 7, "text": "overheat"}))},
            {"t": T0 + 3000, "topic": "plant/line2/telemetry", "payloadB64": enc("plant.Alarm", json!({"device": "m2", "text": "wrong type on purpose"})), "expectError": "Cannot decode as Protobuf plant.Telemetry: field 3 wire type mismatch"}
        ]
    })
}

fn write(name: &str, v: &Value) {
    let text = serde_json::to_string_pretty(v).expect("json") + "\n";
    std::fs::write(common::corpus_path(name), text).expect("write corpus");
}

#[test]
#[ignore = "regenerates the committed corpus files"]
fn regenerate_corpus() {
    write("sparkplug.json", &sparkplug_corpus());
    write("cbor.json", &cbor_corpus());
    write("msgpack.json", &msgpack_corpus());
    write("proto.json", &proto_corpus());
}

#[test]
fn committed_corpus_matches_generator() {
    for (name, v) in [
        ("sparkplug.json", sparkplug_corpus()),
        ("cbor.json", cbor_corpus()),
        ("msgpack.json", msgpack_corpus()),
        ("proto.json", proto_corpus()),
    ] {
        let committed: Value = serde_json::from_str(&common::load_text(name)).expect("parse");
        assert_eq!(
            committed, v,
            "{name} is out of date: run the ignored regenerate_corpus test"
        );
    }
}
