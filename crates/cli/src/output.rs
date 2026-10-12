//! Output formats of `sub`: text, json, ndjson and raw.

use crate::args::Format;
use base64::Engine as _;
use chrono::{DateTime, Local, SecondsFormat, Utc};
use mqttpanel_client::{Incoming, Props5};
use serde_json::{json, Map, Value};

/// Payload as text: UTF-8 as is, anything else as "0x" plus lowercase hex.
pub fn payload_text(p: &[u8]) -> String {
    match std::str::from_utf8(p) {
        Ok(s) => s.to_string(),
        Err(_) => format!("0x{}", hex::encode(p)),
    }
}

fn b64(p: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(p)
}

/// MQTT 5 properties as JSON; absent and empty fields are left out.
pub fn properties_json(p: &Props5) -> Value {
    let mut m = Map::new();
    if let Some(v) = &p.content_type {
        m.insert("contentType".into(), json!(v));
    }
    if let Some(v) = p.payload_format {
        m.insert("payloadFormat".into(), json!(v));
    }
    if let Some(v) = &p.response_topic {
        m.insert("responseTopic".into(), json!(v));
    }
    if let Some(v) = &p.correlation_data {
        match std::str::from_utf8(v) {
            Ok(s) => m.insert("correlationData".into(), json!(s)),
            Err(_) => m.insert("correlationDataBase64".into(), json!(b64(v))),
        };
    }
    if let Some(v) = p.message_expiry {
        m.insert("messageExpiry".into(), json!(v));
    }
    if !p.user_properties.is_empty() {
        let list: Vec<Value> = p.user_properties.iter().map(|(k, v)| json!([k, v])).collect();
        m.insert("userProperties".into(), Value::Array(list));
    }
    if !p.subscription_ids.is_empty() {
        m.insert("subscriptionIds".into(), json!(p.subscription_ids));
    }
    Value::Object(m)
}

/// One message as a JSON object: time (UTC, ms), topic, payload (null when not UTF-8, then payloadBase64), qos,
/// retain, and properties on MQTT 5.
pub fn message_json(m: &Incoming, at: DateTime<Utc>) -> Value {
    let mut o = Map::new();
    o.insert("time".into(), json!(at.to_rfc3339_opts(SecondsFormat::Millis, true)));
    o.insert("topic".into(), json!(m.topic));
    match std::str::from_utf8(&m.payload) {
        Ok(s) => {
            o.insert("payload".into(), json!(s));
        }
        Err(_) => {
            o.insert("payload".into(), Value::Null);
            o.insert("payloadBase64".into(), json!(b64(&m.payload)));
        }
    }
    o.insert("qos".into(), json!(m.qos));
    o.insert("retain".into(), json!(m.retain));
    if let Some(p) = &m.props {
        o.insert("properties".into(), properties_json(p));
    }
    Value::Object(o)
}

/// Renders one message in the chosen format (with its trailing newline, except raw).
pub fn render(format: Format, m: &Incoming, at: DateTime<Utc>) -> Vec<u8> {
    match format {
        Format::Raw => m.payload.to_vec(),
        Format::Text => {
            let local: DateTime<Local> = at.into();
            format!("{} {} {}\n", local.format("%H:%M:%S%.3f"), m.topic, payload_text(&m.payload)).into_bytes()
        }
        Format::Json => {
            let mut s = serde_json::to_string_pretty(&message_json(m, at)).unwrap_or_default();
            s.push('\n');
            s.into_bytes()
        }
        Format::Ndjson => {
            let mut s = serde_json::to_string(&message_json(m, at)).unwrap_or_default();
            s.push('\n');
            s.into_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes_shim::*;

    mod bytes_shim {
        pub fn incoming(topic: &str, payload: &[u8], props: Option<mqttpanel_client::Props5>) -> mqttpanel_client::Incoming {
            mqttpanel_client::Incoming { topic: topic.into(), payload: payload.to_vec().into(), qos: 1, retain: true, dup: false, props }
        }
    }

    #[test]
    fn formats() {
        let at: DateTime<Utc> = "2026-10-13T08:09:10.123Z".parse().unwrap();
        let m = incoming("a/b", b"hello", None);
        let ndj = String::from_utf8(render(Format::Ndjson, &m, at)).unwrap();
        assert_eq!(ndj, "{\"time\":\"2026-10-13T08:09:10.123Z\",\"topic\":\"a/b\",\"payload\":\"hello\",\"qos\":1,\"retain\":true}\n");
        assert_eq!(render(Format::Raw, &m, at), b"hello");
        let text = String::from_utf8(render(Format::Text, &m, at)).unwrap();
        assert!(text.ends_with(".123 a/b hello\n"), "{text}");
        let bin = incoming("x", &[0, 255], Some(Props5 { content_type: Some("b".into()), ..Default::default() }));
        let v = message_json(&bin, at);
        assert_eq!(v["payload"], Value::Null);
        assert_eq!(v["payloadBase64"], "AP8=");
        assert_eq!(v["properties"], json!({"contentType": "b"}));
        assert!(String::from_utf8(render(Format::Text, &bin, at)).unwrap().ends_with(" x 0x00ff\n"));
    }
}
