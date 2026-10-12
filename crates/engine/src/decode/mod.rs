//! Payload decoders and format detection.

pub mod cbor;
pub mod image;
pub mod json;
pub mod msgpack;
pub mod protobuf;
pub mod sparkplug;

use crate::message::UserMessage;
use crate::types::{DecodedPayload, PayloadFormat};
use base64::Engine as _;
use serde_json::Value;

pub use protobuf::{ProtoAssignment, ProtoFile, ProtoRegistry};
pub use sparkplug::AliasMap;

/// Bytes shown in the hex view of a paged payload.
pub const PAGED_HEX_BYTES: usize = 64 * 1024;

/// Context for decoding.
#[derive(Clone, Copy)]
pub struct DecodeOptions<'a> {
    /// Payloads above this size are shown paged (raw hex of the first 64 KiB only).
    pub render_limit: usize,
    /// Decode even above the render limit.
    pub format_anyway: bool,
    /// Protobuf schemas and assignments.
    pub protobuf: Option<&'a ProtoRegistry>,
    /// Sparkplug alias map of the edge node the topic belongs to.
    pub sparkplug_aliases: Option<&'a AliasMap>,
}

impl Default for DecodeOptions<'_> {
    fn default() -> Self {
        DecodeOptions {
            render_limit: 256 * 1024,
            format_anyway: false,
            protobuf: None,
            sparkplug_aliases: None,
        }
    }
}

fn content_type_base(ct: Option<&str>) -> Option<String> {
    ct.and_then(|c| c.split(';').next())
        .map(|c| c.trim().to_ascii_lowercase())
}

/// Detection that does not need to parse the payload (everything except JSON vs text).
fn detect_cheap(
    bytes: &[u8],
    content_type: Option<&str>,
    topic: &str,
    protobuf: Option<&ProtoRegistry>,
) -> Option<PayloadFormat> {
    if image::sniff(bytes).is_some() || image::image_content_type(content_type).is_some() {
        return Some(PayloadFormat::Image);
    }
    if sparkplug::parse_topic(topic).is_some() {
        return Some(PayloadFormat::Sparkplug);
    }
    if protobuf.and_then(|p| p.type_for(topic)).is_some() {
        return Some(PayloadFormat::Protobuf);
    }
    match content_type_base(content_type).as_deref() {
        Some("application/cbor") => Some(PayloadFormat::Cbor),
        Some("application/msgpack") | Some("application/x-msgpack") => Some(PayloadFormat::Msgpack),
        _ => None,
    }
}

/// Auto-detects the payload format. Binary payloads are reported as `hex`.
pub fn detect(
    bytes: &[u8],
    content_type: Option<&str>,
    topic: &str,
    protobuf: Option<&ProtoRegistry>,
) -> PayloadFormat {
    if let Some(f) = detect_cheap(bytes, content_type, topic, protobuf) {
        return f;
    }
    if json::looks_like_json(bytes) && json::parse(bytes).is_ok() {
        return PayloadFormat::Json;
    }
    if json::as_text(bytes).is_some() {
        PayloadFormat::Text
    } else {
        PayloadFormat::Hex
    }
}

/// Decodes a structured format to JSON.
pub fn decode_structured(
    bytes: &[u8],
    format: PayloadFormat,
    topic: &str,
    opts: &DecodeOptions<'_>,
) -> Result<Value, UserMessage> {
    match format {
        PayloadFormat::Json => json::parse(bytes)
            .map_err(|d| UserMessage::new("decode.json", vec![("detail", d.into())])),
        PayloadFormat::Cbor => cbor::decode(bytes)
            .map_err(|d| UserMessage::new("decode.cbor", vec![("detail", d.into())])),
        PayloadFormat::Msgpack => msgpack::decode(bytes)
            .map_err(|d| UserMessage::new("decode.msgpack", vec![("detail", d.into())])),
        PayloadFormat::Sparkplug => sparkplug::decode_raw(bytes)
            .map(|p| sparkplug::to_json(&p, opts.sparkplug_aliases))
            .map_err(|d| UserMessage::new("decode.sparkplug", vec![("detail", d.into())])),
        PayloadFormat::Protobuf => match opts
            .protobuf
            .and_then(|p| p.type_for(topic).map(|t| (p, t)))
        {
            Some((reg, ty)) => reg.decode(ty, bytes),
            None => Err(UserMessage::new(
                "decode.protobuf",
                vec![
                    ("type", "?".into()),
                    ("detail", "no message type is assigned to this topic".into()),
                ],
            )),
        },
        _ => Err(UserMessage::new(
            "decode.json",
            vec![("detail", "not a structured format".into())],
        )),
    }
}

/// Classic hex dump: offset, 16 bytes, ASCII column.
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 + 16);
    for (i, chunk) in bytes.chunks(16).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{:08x} ", i * 16));
        for j in 0..16 {
            if j == 8 {
                out.push(' ');
            }
            match chunk.get(j) {
                Some(b) => out.push_str(&format!(" {b:02x}")),
                None => out.push_str("   "),
            }
        }
        out.push_str("  |");
        for &b in chunk {
            out.push(if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        out.push('|');
    }
    out
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decodes a payload for display. `format = auto` uses the detected format. A forced format
/// that fails sets `error` and still fills `hex`.
pub fn decode_payload(
    bytes: &[u8],
    format: PayloadFormat,
    content_type: Option<&str>,
    topic: &str,
    opts: &DecodeOptions<'_>,
) -> DecodedPayload {
    let mut out = DecodedPayload {
        format,
        detected: PayloadFormat::Hex,
        json: None,
        pretty: None,
        text: None,
        hex: None,
        base64: None,
        image_data_url: None,
        error: None,
        paged: false,
    };
    if bytes.len() > opts.render_limit && !opts.format_anyway {
        let detected =
            detect_cheap(bytes, content_type, topic, opts.protobuf).unwrap_or_else(|| {
                let head = &bytes[..bytes.len().min(4096)];
                let first = head.iter().find(|c| !c.is_ascii_whitespace());
                let utf8 = match std::str::from_utf8(head) {
                    Ok(_) => true,
                    Err(e) => e.error_len().is_none(),
                };
                if utf8 && matches!(first, Some(b'{' | b'[')) {
                    PayloadFormat::Json
                } else if utf8 {
                    PayloadFormat::Text
                } else {
                    PayloadFormat::Hex
                }
            });
        out.detected = detected;
        out.format = if format == PayloadFormat::Auto {
            detected
        } else {
            format
        };
        out.paged = true;
        out.hex = Some(hex_dump(&bytes[..bytes.len().min(PAGED_HEX_BYTES)]));
        return out;
    }
    let detected = detect(bytes, content_type, topic, opts.protobuf);
    out.detected = detected;
    let used = if format == PayloadFormat::Auto {
        detected
    } else {
        format
    };
    out.format = used;
    out.base64 = Some(b64(bytes));
    let text = json::as_text(bytes).map(str::to_string);
    match used {
        PayloadFormat::Auto => {}
        f if f.is_structured() => match decode_structured(bytes, f, topic, opts) {
            Ok(v) => {
                out.pretty = serde_json::to_string_pretty(&v).ok();
                out.json = Some(v);
                out.text = text;
            }
            Err(e) => {
                out.error = Some(e);
                out.hex = Some(hex_dump(bytes));
                out.text = text;
            }
        },
        PayloadFormat::Text => match std::str::from_utf8(json::strip_bom(bytes)) {
            Ok(s) => out.text = Some(s.to_string()),
            Err(_) => {
                out.text = Some(String::from_utf8_lossy(bytes).into_owned());
                out.hex = Some(hex_dump(bytes));
            }
        },
        PayloadFormat::Hex => {
            out.hex = Some(hex_dump(bytes));
            out.text = text;
        }
        PayloadFormat::Base64 => {
            // Forced base64: the payload is base64 text; show what it decodes to.
            let compact: Vec<u8> = bytes
                .iter()
                .copied()
                .filter(|c| !c.is_ascii_whitespace())
                .collect();
            match base64::engine::general_purpose::STANDARD
                .decode(&compact)
                .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&compact))
                .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&compact))
            {
                Ok(dec) => {
                    out.text = json::as_text(&dec).map(str::to_string);
                    out.hex = Some(hex_dump(&dec));
                }
                Err(e) => {
                    out.error = Some(UserMessage::new(
                        "decode.base64",
                        vec![("detail", e.to_string().into())],
                    ));
                    out.hex = Some(hex_dump(bytes));
                    out.text = text;
                }
            }
        }
        PayloadFormat::Image => match image::data_url(bytes, content_type) {
            Some(url) => out.image_data_url = Some(url),
            None => {
                out.error = Some(UserMessage::new("decode.notImage", vec![]));
                out.hex = Some(hex_dump(bytes));
            }
        },
        _ => {}
    }
    out
}

/// Result of analysing a payload at ingest.
pub(crate) struct Analysis {
    pub format: PayloadFormat,
    /// Value used for series and fields (Sparkplug: metric name -> value).
    pub doc: Option<Value>,
    pub preview: String,
}

/// Preview text truncated to `max` characters, control characters replaced by spaces.
pub fn preview_text(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max * 4));
    let mut prev_space = false;
    for (n, c) in s.chars().enumerate() {
        if n >= max {
            out.push('…');
            break;
        }
        let c = if c.is_control() { ' ' } else { c };
        if c == ' ' && prev_space {
            continue;
        }
        prev_space = c == ' ';
        out.push(c);
    }
    out
}

/// Human size, "23 B", "1.2 KB".
pub fn human_size(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// Max payload size parsed at ingest for series extraction.
pub(crate) const MAX_ANALYSE: usize = 16 * 1024 * 1024;

/// Analyses a payload at ingest: format, series document and preview.
pub(crate) fn analyse(
    bytes: &[u8],
    content_type: Option<&str>,
    topic: &str,
    protobuf: Option<&ProtoRegistry>,
    aliases: Option<&AliasMap>,
) -> Analysis {
    if bytes.len() > MAX_ANALYSE {
        return Analysis {
            format: PayloadFormat::Hex,
            doc: None,
            preview: format!("<{}>", human_size(bytes.len())),
        };
    }
    if let Some(f) = detect_cheap(bytes, content_type, topic, protobuf) {
        let opts = DecodeOptions {
            render_limit: usize::MAX,
            format_anyway: true,
            protobuf,
            sparkplug_aliases: aliases,
        };
        return match f {
            PayloadFormat::Image => Analysis {
                format: f,
                doc: None,
                preview: format!("<image {}>", human_size(bytes.len())),
            },
            PayloadFormat::Sparkplug => match sparkplug::decode_raw(bytes) {
                Ok(p) => {
                    let named = sparkplug::named_values(&p, aliases);
                    let mut map = serde_json::Map::new();
                    for (k, v) in named {
                        map.insert(k, v);
                    }
                    let doc = Value::Object(map);
                    let preview = preview_text(&doc.to_string(), 80);
                    Analysis {
                        format: f,
                        doc: Some(doc),
                        preview,
                    }
                }
                Err(_) => binary_analysis(bytes),
            },
            _ => match decode_structured(bytes, f, topic, &opts) {
                Ok(v) => {
                    let preview = preview_text(&v.to_string(), 80);
                    Analysis {
                        format: f,
                        doc: Some(v),
                        preview,
                    }
                }
                Err(_) => binary_analysis(bytes),
            },
        };
    }
    if json::looks_like_json(bytes) {
        if let Ok(v) = json::parse(bytes) {
            let s = String::from_utf8_lossy(json::strip_bom(bytes));
            return Analysis {
                format: PayloadFormat::Json,
                doc: Some(v),
                preview: preview_text(&s, 80),
            };
        }
    }
    match json::as_text(bytes) {
        Some(s) => Analysis {
            format: PayloadFormat::Text,
            doc: Some(Value::String(s.to_string())),
            preview: preview_text(s, 80),
        },
        None => binary_analysis(bytes),
    }
}

fn binary_analysis(bytes: &[u8]) -> Analysis {
    Analysis {
        format: PayloadFormat::Hex,
        doc: None,
        preview: format!("<binary {}>", human_size(bytes.len())),
    }
}

/// Series document of a stored payload (same as at ingest).
pub(crate) fn series_doc(
    bytes: &[u8],
    content_type: Option<&str>,
    topic: &str,
    protobuf: Option<&ProtoRegistry>,
    aliases: Option<&AliasMap>,
) -> (PayloadFormat, Option<Value>) {
    let a = analyse(bytes, content_type, topic, protobuf, aliases);
    (a.format, a.doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_dump_layout() {
        let d = hex_dump(b"{\"a\":1}\x00\x01");
        assert!(d.starts_with("00000000  7b 22 61 22 3a 31 7d 00  01"));
        assert!(d.ends_with("|{\"a\":1}..|"));
    }

    #[test]
    fn preview_truncates() {
        let s = "x".repeat(100);
        let p = preview_text(&s, 80);
        assert_eq!(p.chars().count(), 81);
        assert_eq!(preview_text("a\nb", 80), "a b");
    }
}
