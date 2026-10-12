//! Shared helpers for the integration tests: corpus loading and replay.
#![allow(dead_code)]

use base64::Engine as _;
use mqttpanel_engine::{Engine, EngineConfig, IncomingMessage, Mqtt5Props};
use serde::Deserialize;
use std::path::PathBuf;

/// Base receive time of the corpus (ms).
pub const T0: i64 = 1_760_000_000_000;

/// One recorded PUBLISH.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub t: i64,
    pub topic: String,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub payload_b64: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub qos: u8,
    #[serde(default)]
    pub retain: bool,
}

impl Event {
    pub fn bytes(&self) -> Vec<u8> {
        if let Some(b) = &self.payload_b64 {
            base64::engine::general_purpose::STANDARD
                .decode(b)
                .expect("corpus base64")
        } else {
            self.payload.clone().unwrap_or_default().into_bytes()
        }
    }

    pub fn to_message(&self) -> IncomingMessage {
        IncomingMessage {
            topic: self.topic.clone(),
            payload: self.bytes(),
            qos: self.qos,
            retain: self.retain,
            dup: false,
            received_at: self.t,
            props: self.content_type.as_ref().map(|ct| Mqtt5Props {
                content_type: Some(ct.clone()),
                ..Default::default()
            }),
        }
    }
}

pub fn corpus_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name)
}

pub fn load_text(name: &str) -> String {
    std::fs::read_to_string(corpus_path(name)).expect("corpus file")
}

pub fn load_events(name: &str) -> Vec<Event> {
    serde_json::from_str(&load_text(name)).expect("corpus json")
}

pub fn replay(engine: &mut Engine, events: &[Event]) {
    for e in events {
        engine.ingest(e.to_message());
    }
}

pub fn engine_with(name: &str) -> Engine {
    let mut e = Engine::new(EngineConfig::default());
    replay(&mut e, &load_events(name));
    e
}

pub fn msg(topic: &str, payload: &str, t: i64) -> IncomingMessage {
    IncomingMessage::new(topic, payload.as_bytes().to_vec(), t)
}

/// Deterministic 100k-topic fixture: building/f1..10/r1..100/<100 sensors>, plus Sparkplug and $SYS.
pub fn topics_100k() -> Vec<String> {
    let sensors: Vec<String> = (0..100)
        .map(|i| {
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
            format!("{}{}", KINDS[i % 10], i / 10)
        })
        .collect();
    let mut v = Vec::with_capacity(100_100);
    for f in 1..=10 {
        for r in 1..=100 {
            for s in &sensors {
                if v.len() < 99_900 {
                    v.push(format!("building/f{f}/r{r}/{s}"));
                }
            }
        }
    }
    for e in 0..10 {
        for m in 0..5 {
            v.push(format!("spBv1.0/plant/DDATA/edge{e}/dev{m}"));
        }
    }
    for i in 0..50 {
        v.push(format!("$SYS/broker/load/messages/received/{i}min"));
    }
    v
}
