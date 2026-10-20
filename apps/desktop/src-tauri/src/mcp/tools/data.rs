//! Explorer and data group (docs/MCP.md section 4): topics, values, history, search, publish, retained messages and
//! exports. Reads are bounded (owner): 200 topics, 500 history rows per call with a `before` cursor, 100 search hits;
//! bigger extracts go to a file with `export_data`. Same engine functions as the Explorer (`tree`, `table_slice`,
//! `topic_detail`, `history`, `message`, `export`).

use super::{ask_prop, conn_prop, file_prop, schema, shown, BoxFuture, Call, Fail, Outcome, ToolDef, Wait};
use crate::commands as cmd;
use crate::mcp::text;
use crate::state::{lock, Conn};
use crate::types::PublishRequest;
use mqttpanel_client::catalog::Lang;
use mqttpanel_engine::{DecodedPayload, ExportFormat, ExportOptions, ExportScope, Filters, PayloadFormat, TableQuery};
use serde_json::{json, Value};

macro_rules! h {
    ($f:path) => {
        |c: Call| -> BoxFuture<Result<Outcome, Fail>> { Box::pin($f(c)) }
    };
}

pub const TOPICS_MAX: usize = 200;
pub const HISTORY_MAX: usize = 500;
pub const HISTORY_DEFAULT: usize = 200;
pub const SEARCH_MAX: usize = 100;
/// Longest payload text returned for one message (the full payload goes to a file with export_data).
const PAYLOAD_MAX: usize = 16 * 1024;
const ROW_PAYLOAD_MAX: usize = 2000;

pub fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "list_topics",
            description: "The topics a connection has received, in tree order, with the last value (short preview), message rate, message count, QoS, retained flag and the number of subtopics. filter: plain text (part of the topic), an MQTT filter with + and #, or a regular expression starting with /. At most 200 topics per call. Read-only.",
            schema: || schema(json!({ "connection": conn_prop(), "filter": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 200 } }), &["connection"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(list_topics),
        },
        ToolDef {
            name: "get_topic",
            description: "One topic: its last payload decoded (JSON, text, or hex for binary; long payloads are cut), the MQTT 5 properties, QoS, retained, the message count, the history size and the fields that can be charted. Read-only.",
            schema: || schema(json!({ "connection": conn_prop(), "topic": { "type": "string" } }), &["connection", "topic"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(get_topic),
        },
        ToolDef {
            name: "get_history",
            description: "The buffered messages of one topic, newest first: time, payload (cut at 2000 characters), QoS, retained, size and the keys that changed. At most 500 rows per call (default 200); for older rows call again with before set to the nextBefore of the result. Bigger extracts: export_data. Read-only.",
            schema: || {
                schema(
                    json!({
                        "connection": conn_prop(),
                        "topic": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 },
                        "before": { "type": "integer", "description": "Only messages older than this sequence number (nextBefore of the previous call)." }
                    }),
                    &["connection", "topic"],
                )
            },
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(get_history),
        },
        ToolDef {
            name: "search_values",
            description: "The topics whose last value contains a text (not case-sensitive), with that value. At most 100 per call. Read-only.",
            schema: || schema(json!({ "connection": conn_prop(), "text": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 100 } }), &["connection", "text"]),
            writes: false,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(search_values),
        },
        ToolDef {
            name: "publish",
            description: "Publish one message on a connected connection (it appears in the connection's sent log). format: text (default), json (checked), hex or base64. The message goes to real devices: check the topic with the user first.",
            schema: || {
                schema(
                    json!({
                        "connection": conn_prop(),
                        "topic": { "type": "string", "description": "A topic without + or #." },
                        "payload": { "type": "string" },
                        "qos": { "type": "integer", "enum": [0, 1, 2], "default": 0 },
                        "retain": { "type": "boolean", "default": false },
                        "format": { "type": "string", "enum": ["text", "json", "hex", "base64"], "default": "text" }
                    }),
                    &["connection", "topic"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(publish),
        },
        ToolDef {
            name: "delete_retained",
            description: "Delete the retained message of a topic on the broker (publishes an empty retained message), or with subtree true every retained message at and under it. Cannot be undone; confirm with the user first.",
            schema: || schema(json!({ "connection": conn_prop(), "topic": { "type": "string" }, "subtree": { "type": "boolean", "default": false } }), &["connection", "topic"]),
            writes: true,
            destructive: true,
            secret_fields: &[],
            wait: Wait::Call,
            handler: h!(delete_retained),
        },
        ToolDef {
            name: "export_data",
            description: "Export the buffered messages of a topic or a subtree to a file: CSV, JSON or NDJSON, optionally between from and to (milliseconds since 1970, UTC). Recommended: leave file out, the user picks the place in MQTT Panel's save dialog; or give an absolute file path. Returns the path and a summary (messages, topics, size).",
            schema: || {
                schema(
                    json!({
                        "connection": conn_prop(),
                        "scope": { "type": "string", "enum": ["topic", "subtree"], "default": "topic" },
                        "topic": { "type": "string", "description": "The topic, or the top of the subtree." },
                        "format": { "type": "string", "enum": ["csv", "json", "ndjson"], "default": "csv" },
                        "from": { "type": "integer" },
                        "to": { "type": "integer" },
                        "include_properties": { "type": "boolean", "default": false },
                        "ask_in_app": ask_prop(),
                        "file": file_prop()
                    }),
                    &["connection", "topic"],
                )
            },
            writes: true,
            destructive: false,
            secret_fields: &[],
            wait: Wait::MaybeUser,
            handler: h!(export_data),
        },
    ]
}

// ---------------------------------------------------------------- helpers

fn cut(s: &str, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        (s.to_string(), false)
    } else {
        (s.chars().take(max).collect::<String>() + "…", true)
    }
}

/// A decoded payload for the chat: JSON when structured, else text, else hex; images are not returned.
fn payload_value(d: &DecodedPayload, max: usize) -> (Value, &'static str, bool) {
    if let Some(j) = &d.json {
        let flat = serde_json::to_string(j).unwrap_or_default();
        if flat.len() <= max {
            return (j.clone(), "json", false);
        }
        let (t, _) = cut(&flat, max);
        return (json!(t), "json", true);
    }
    if d.image_data_url.is_some() {
        return (Value::Null, "image", false);
    }
    if let Some(t) = &d.text {
        let (t, cutoff) = cut(t, max);
        return (json!(t), "text", cutoff);
    }
    if let Some(h) = &d.hex {
        let (t, cutoff) = cut(h, max);
        return (json!(t), "hex", cutoff);
    }
    (Value::Null, "empty", false)
}

fn iso(ms: i64) -> String {
    mqttpanel_engine::export::iso_ms(ms)
}

/// list_topics and `mqttpanel://topics/{connection}`.
pub fn topics_json(conn: &Conn, filter: &str, limit: usize) -> Value {
    let now = crate::types::now_ms();
    let mut e = lock(&conn.engine);
    let q = TableQuery { search: filter.to_string(), ..Default::default() };
    let slice = e.table_slice(&q, 0, limit.min(TOPICS_MAX), now);
    let rows: Vec<Value> = slice
        .rows
        .iter()
        .map(|r| {
            json!({
                "topic": r.path,
                "value": r.value,
                "ratePerSecond": r.rate,
                "messages": r.messages,
                "qos": r.qos,
                "retained": r.retained,
                "size": r.size,
                "lastReceived": r.last_received_at.map(iso),
                "subtopics": e.subtopic_count(&r.path),
            })
        })
        .collect();
    json!({ "total": slice.total, "shown": rows.len(), "truncated": (rows.len() as u64) < slice.total, "topics": rows })
}

fn size_text(lang: Lang, bytes: usize) -> String {
    let _ = lang;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// ---------------------------------------------------------------- reads

async fn list_topics(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let filter = c.opt_str("filter").unwrap_or_default();
    let limit = c.opt_u64("limit")?.unwrap_or(TOPICS_MAX as u64).clamp(1, TOPICS_MAX as u64) as usize;
    let mut data = topics_json(&conn, &filter, limit);
    data["connection"] = json!(p.id);
    let total = data["total"].as_u64().unwrap_or(0);
    let shown_n = data["shown"].as_u64().unwrap_or(0);
    let text = if total == 0 {
        c.t("data.noTopics", &[("name", json!(p.name))])
    } else {
        let list: Vec<String> = data["topics"].as_array().map(|a| a.iter().take(10).filter_map(|t| t["topic"].as_str().map(str::to_string)).collect()).unwrap_or_default();
        c.t("data.topics", &[("name", json!(p.name)), ("shown", json!(shown_n)), ("total", json!(total)), ("list", json!(list.join(", ")))])
    };
    Ok(Outcome::new(text, data))
}

async fn get_topic(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let topic = c.str("topic")?;
    let d = lock(&conn.engine).topic_detail(&topic);
    let Some(last) = d.last.as_ref().filter(|_| d.exists) else {
        return Err(Fail::from(text::um("data.noTopic", &[("name", json!(p.name)), ("topic", json!(topic))])));
    };
    let (payload, format, truncated) = payload_value(&last.decoded, PAYLOAD_MAX);
    let subtopics = lock(&conn.engine).subtopic_count(&topic);
    let fields: Vec<Value> = d.fields.iter().map(|f| json!({ "path": f.path, "label": f.label, "lastValue": f.last_value, "plottable": f.plottable })).collect();
    let preview = match &payload {
        Value::String(s) => cut(s, 120).0,
        Value::Null => String::new(),
        v => cut(&v.to_string(), 120).0,
    };
    let text = c.t("data.topic", &[("topic", json!(topic)), ("value", json!(preview)), ("messages", json!(d.messages)), ("history", json!(d.history_count))]);
    Ok(Outcome::new(
        text,
        json!({
            "connection": p.id,
            "topic": topic,
            "receivedAt": iso(last.received_at),
            "payload": payload,
            "format": format,
            "detectedFormat": last.decoded.detected,
            "truncated": truncated || last.truncated,
            "size": last.size,
            "qos": last.qos,
            "retained": last.retain,
            "properties": last.properties,
            "messages": d.messages,
            "inHistory": d.history_count,
            "bufferSize": d.buffer_size,
            "subtopics": subtopics,
            "fields": fields,
        }),
    ))
}

async fn get_history(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let topic = c.str("topic")?;
    let limit = c.opt_u64("limit")?.unwrap_or(HISTORY_DEFAULT as u64).clamp(1, HISTORY_MAX as u64) as usize;
    let before = c.opt_u64("before")?;
    let e = lock(&conn.engine);
    let all = e.history(&topic);
    let older: Vec<_> = all.iter().filter(|h| before.is_none_or(|b| h.seq < b)).collect();
    let start = older.len().saturating_sub(limit);
    let page = &older[start..];
    let rows: Vec<Value> = page
        .iter()
        .rev()
        .map(|h| {
            let (payload, format, cutoff) = match e.message(&topic, h.seq, PayloadFormat::Auto, false) {
                Some(m) => payload_value(&m.decoded, ROW_PAYLOAD_MAX),
                None => (json!(h.preview), "text", false),
            };
            json!({ "seq": h.seq, "time": iso(h.received_at), "payload": payload, "format": format, "truncated": cutoff, "qos": h.qos, "retained": h.retain, "size": h.size, "changedKeys": h.changed_keys })
        })
        .collect();
    drop(e);
    let more = start > 0;
    let next_before = if more { page.first().map(|h| h.seq) } else { None };
    let text = if rows.is_empty() {
        c.t("data.historyEmpty", &[("topic", json!(topic))])
    } else {
        let mut t = c.t("data.history", &[("count", json!(rows.len())), ("topic", json!(topic))]);
        if let Some(nb) = next_before {
            t = format!("{t} {}", c.t("data.historyMore", &[("before", json!(nb))]));
        }
        t
    };
    Ok(Outcome::new(text, json!({ "connection": p.id, "topic": topic, "rows": rows, "more": more, "nextBefore": next_before, "inHistory": all.len() })))
}

async fn search_values(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let needle = c.str("text")?;
    let limit = c.opt_u64("limit")?.unwrap_or(SEARCH_MAX as u64).clamp(1, SEARCH_MAX as u64) as usize;
    let now = crate::types::now_ms();
    let q = TableQuery { filters: Filters { payload_contains: needle.clone(), ..Default::default() }, ..Default::default() };
    let slice = lock(&conn.engine).table_slice(&q, 0, limit, now);
    let hits: Vec<Value> = slice.rows.iter().map(|r| json!({ "topic": r.path, "value": r.value, "lastReceived": r.last_received_at.map(iso) })).collect();
    let text = if hits.is_empty() {
        c.t("data.searchNone", &[("name", json!(p.name)), ("text", json!(needle))])
    } else {
        c.t("data.search", &[("name", json!(p.name)), ("text", json!(needle)), ("count", json!(slice.total))])
    };
    Ok(Outcome::new(text, json!({ "connection": p.id, "total": slice.total, "truncated": (hits.len() as u64) < slice.total, "hits": hits })))
}

// ---------------------------------------------------------------- writes

async fn publish(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let topic = c.str("topic")?;
    let qos = c.opt_u64("qos")?.unwrap_or(0);
    if qos > 2 {
        return Err(Fail::args("qos must be 0, 1 or 2"));
    }
    let format = c.opt_enum("format", &["text", "json", "hex", "base64"])?.unwrap_or_else(|| "text".into());
    let retain = c.opt_bool("retain").unwrap_or(false);
    let req = PublishRequest { topic: topic.clone(), payload: c.opt_str("payload").unwrap_or_default(), format, qos: qos as u8, retain, props: None, request: None };
    let entry = cmd::do_publish(&conn, &req, "pending", true)?;
    let key = if retain { "data.publishedRetained" } else { "data.published" };
    let text = c.t(key, &[("topic", json!(topic)), ("name", json!(p.name)), ("qos", json!(qos)), ("size", json!(entry.size))]);
    Ok(Outcome::new(text, json!({ "connection": p.id, "topic": topic, "size": entry.size, "qos": qos, "retain": retain, "sentLogId": entry.id })))
}

async fn delete_retained(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let topic = c.str("topic")?;
    let subtree = c.opt_bool("subtree").unwrap_or(false);
    let topics: Vec<String> = if subtree { cmd::retained_under_on(c.state(), &p.id, &topic)?.into_iter().map(|r| r.topic).collect() } else { vec![topic.clone()] };
    if topics.is_empty() {
        return Err(Fail::from(text::um("data.noRetained", &[("topic", json!(topic)), ("name", json!(p.name))])));
    }
    for t in &topics {
        cmd::do_publish(&conn, &cmd::delete_request(t), "deletedRetained", true)?;
    }
    let text = if subtree {
        c.t("data.retainedDeletedMany", &[("count", json!(topics.len())), ("topic", json!(topic)), ("name", json!(p.name))])
    } else {
        c.t("data.retainedDeleted", &[("topic", json!(topic)), ("name", json!(p.name))])
    };
    Ok(Outcome::new(text, json!({ "connection": p.id, "deleted": topics })))
}

async fn export_data(c: Call) -> Result<Outcome, Fail> {
    let (p, conn) = c.connection("connection")?;
    let topic = c.str("topic")?;
    let scope = match c.opt_enum("scope", &["topic", "subtree"])?.as_deref() {
        Some("subtree") => ExportScope::Subtree { prefix: topic.clone() },
        _ => ExportScope::Topic { topic: topic.clone() },
    };
    let format = match c.opt_enum("format", &["csv", "json", "ndjson"])?.as_deref() {
        Some("json") => ExportFormat::Json,
        Some("ndjson") => ExportFormat::Ndjson,
        _ => ExportFormat::Csv,
    };
    let options = ExportOptions { scope, from_ms: c.opt_i64("from")?, to_ms: c.opt_i64("to")?, format, include_properties: c.opt_bool("include_properties").unwrap_or(false), raw_base64: false };
    let est = lock(&conn.engine).export_estimate(&options);
    if est.messages == 0 {
        return Err(Fail::from(text::um("data.nothingToExport", &[("topic", json!(topic)), ("name", json!(p.name))])));
    }
    let (default_name, data) = cmd::export_data_on(c.state(), &p.id, &options)?;
    let path = c.save_target(&default_name, &default_name).await?;
    crate::files::write_file(&path, &data)?;
    let text = c.t(
        "data.exported",
        &[("rows", json!(est.messages)), ("topics", json!(est.topics)), ("size", json!(size_text(c.lang(), data.len()))), ("file", json!(shown(&path)))],
    );
    let toast = c.t("data.exportedToast", &[("rows", json!(est.messages)), ("file", json!(crate::files::file_name(&path)))]);
    let mut out = Outcome::new(text, json!({ "connection": p.id, "file": shown(&path), "messages": est.messages, "topics": est.topics, "bytes": data.len() }));
    out.toast = Some(toast);
    Ok(out)
}
