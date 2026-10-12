//! Output formats of `sub`: text, json, ndjson and raw.

use crate::args::Format;
use base64::Engine as _;
use chrono::{DateTime, Local, SecondsFormat, Utc};
use mqttpanel_client::{Incoming, Props5};
use serde::Serialize;

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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropsOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_format: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correlation_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correlation_data_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_expiry: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    user_properties: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    subscription_ids: Vec<usize>,
}

impl PropsOut {
    pub fn new(p: &Props5) -> PropsOut {
        let (cd, cd64) = match &p.correlation_data {
            Some(v) => match std::str::from_utf8(v) {
                Ok(s) => (Some(s.to_string()), None),
                Err(_) => (None, Some(b64(v))),
            },
            None => (None, None),
        };
        PropsOut {
            content_type: p.content_type.clone(),
            payload_format: p.payload_format,
            response_topic: p.response_topic.clone(),
            correlation_data: cd,
            correlation_data_base64: cd64,
            message_expiry: p.message_expiry,
            user_properties: p.user_properties.clone(),
            subscription_ids: p.subscription_ids.clone(),
        }
    }
}

/// One message as JSON, fields in this order: time (UTC, ms), topic, payload (null when not UTF-8, then
/// payloadBase64), qos, retain, and properties on MQTT 5.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageOut<'a> {
    time: String,
    topic: &'a str,
    payload: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_base64: Option<String>,
    qos: u8,
    retain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<PropsOut>,
}

pub fn message_json(m: &Incoming, at: DateTime<Utc>) -> MessageOut<'_> {
    let text = std::str::from_utf8(&m.payload).ok();
    MessageOut {
        time: at.to_rfc3339_opts(SecondsFormat::Millis, true),
        topic: &m.topic,
        payload: text,
        payload_base64: if text.is_none() {
            Some(b64(&m.payload))
        } else {
            None
        },
        qos: m.qos,
        retain: m.retain,
        properties: m.props.as_ref().map(PropsOut::new),
    }
}

/// Renders one message in the chosen format (with its trailing newline, except raw).
pub fn render(format: Format, m: &Incoming, at: DateTime<Utc>) -> Vec<u8> {
    match format {
        Format::Raw => m.payload.to_vec(),
        Format::Text => {
            let local: DateTime<Local> = at.into();
            format!(
                "{} {} {}\n",
                local.format("%H:%M:%S%.3f"),
                m.topic,
                payload_text(&m.payload)
            )
            .into_bytes()
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

    fn incoming(topic: &str, payload: &[u8], props: Option<Props5>) -> Incoming {
        Incoming {
            topic: topic.into(),
            payload: payload.to_vec().into(),
            qos: 1,
            retain: true,
            dup: false,
            props,
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
        let bin = incoming(
            "x",
            &[0, 255],
            Some(Props5 {
                content_type: Some("b".into()),
                ..Default::default()
            }),
        );
        let v = serde_json::to_value(message_json(&bin, at)).unwrap();
        assert!(v["payload"].is_null());
        assert_eq!(v["payloadBase64"], "AP8=");
        assert_eq!(v["properties"], serde_json::json!({"contentType": "b"}));
        assert!(String::from_utf8(render(Format::Text, &bin, at))
            .unwrap()
            .ends_with(" x 0x00ff\n"));
    }
}
