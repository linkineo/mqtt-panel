//! Export of buffered messages (CSV, JSON, NDJSON) and chart CSV.

use crate::buffer::StoredMessage;
use crate::engine::Engine;
use crate::path;
use crate::search::path_cmp;
use crate::types::{ChartSeriesRef, ExportEstimate, ExportFormat, ExportOptions, ExportScope};
use base64::Engine as _;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::{self, Write};

/// ISO 8601 UTC with milliseconds, e.g. `2026-10-12T08:30:00.125Z`.
pub fn iso_ms(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let milli = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    // Civil from days (proleptic Gregorian).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{milli:03}Z",
        sod / 3600,
        (sod / 60) % 60,
        sod % 60
    )
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn num_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A writer that only counts bytes.
struct Counter(u64);
impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const PROP_COLUMNS: [&str; 6] = [
    "contentType",
    "payloadFormat",
    "responseTopic",
    "correlationData",
    "messageExpiry",
    "userProperties",
];

impl Engine {
    /// Messages selected by the options, in receive order, with their topics.
    fn export_selection(&self, o: &ExportOptions) -> Vec<(&str, &StoredMessage)> {
        let in_scope = |p: &str| match &o.scope {
            ExportScope::Topic { topic } => p == topic,
            ExportScope::Subtree { prefix } => {
                prefix.is_empty()
                    || p == prefix
                    || (p.starts_with(prefix.as_str())
                        && p.as_bytes().get(prefix.len()) == Some(&b'/'))
            }
            ExportScope::Topics { topics } => topics.iter().any(|t| t == p),
        };
        let mut out = Vec::new();
        for n in self.nodes.iter().skip(1) {
            let Some(td) = n.topic.as_ref() else { continue };
            if !in_scope(&n.path) {
                continue;
            }
            for m in &td.buffer {
                if o.from_ms.is_some_and(|f| m.received_at < f)
                    || o.to_ms.is_some_and(|t| m.received_at > t)
                {
                    continue;
                }
                out.push((&*n.path, m));
            }
        }
        out.sort_by_key(|(_, m)| m.seq);
        out
    }

    fn payload_cell(m: &StoredMessage, raw_base64: bool) -> (String, &'static str) {
        if !raw_base64 {
            if let Ok(s) = std::str::from_utf8(&m.payload) {
                return (s.to_string(), "text");
            }
        }
        (
            base64::engine::general_purpose::STANDARD.encode(&m.payload),
            "base64",
        )
    }

    fn numeric_fields(&self, topic: &str, m: &StoredMessage) -> Vec<(String, f64)> {
        let (format, doc) = self.doc_of(topic, m);
        let mut out = Vec::new();
        if let Some(d) = doc {
            if format.is_structured() || d.is_number() {
                path::walk_leaves(&d, 8, 256, &mut |p, v| {
                    if let Some(f) = v.as_f64() {
                        out.push((path::label(p), f));
                    }
                });
            }
        }
        out
    }

    fn props_json(m: &StoredMessage) -> Value {
        match &m.props {
            Some(p) => serde_json::to_value(p.as_ref()).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    fn props_cells(m: &StoredMessage) -> Vec<String> {
        let Some(p) = m.props.as_deref() else {
            return vec![String::new(); PROP_COLUMNS.len()];
        };
        vec![
            p.content_type.clone().unwrap_or_default(),
            p.payload_format.map(|v| v.to_string()).unwrap_or_default(),
            p.response_topic.clone().unwrap_or_default(),
            p.correlation_data.clone().unwrap_or_default(),
            p.message_expiry.map(|v| v.to_string()).unwrap_or_default(),
            if p.user_properties.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&p.user_properties).unwrap_or_default()
            },
        ]
    }

    /// Writes the export.
    pub fn export(&self, o: &ExportOptions, out: &mut dyn Write) -> io::Result<()> {
        let sel = self.export_selection(o);
        match o.format {
            ExportFormat::Csv => {
                let fields: Vec<Vec<(String, f64)>> =
                    sel.iter().map(|(t, m)| self.numeric_fields(t, m)).collect();
                let mut columns: Vec<String> = Vec::new();
                let mut col_index: BTreeMap<String, usize> = BTreeMap::new();
                for f in &fields {
                    for (k, _) in f {
                        if !col_index.contains_key(k) {
                            col_index.insert(k.clone(), columns.len());
                            columns.push(k.clone());
                        }
                    }
                }
                let mut header = vec![
                    "time".to_string(),
                    "topic".into(),
                    "payload".into(),
                    "qos".into(),
                    "retained".into(),
                ];
                if o.include_properties {
                    header.extend(PROP_COLUMNS.iter().map(|s| s.to_string()));
                }
                header.extend(columns.iter().cloned());
                writeln!(
                    out,
                    "{}",
                    header
                        .iter()
                        .map(|h| csv_cell(h))
                        .collect::<Vec<_>>()
                        .join(",")
                )?;
                for ((topic, m), f) in sel.iter().zip(fields.iter()) {
                    let (payload, _) = Self::payload_cell(m, o.raw_base64);
                    let mut row = vec![
                        iso_ms(m.received_at),
                        csv_cell(topic),
                        csv_cell(&payload),
                        m.qos.to_string(),
                        m.retain.to_string(),
                    ];
                    if o.include_properties {
                        row.extend(Self::props_cells(m).iter().map(|c| csv_cell(c)));
                    }
                    let mut vals = vec![String::new(); columns.len()];
                    for (k, v) in f {
                        if let Some(&i) = col_index.get(k) {
                            vals[i] = num_text(*v);
                        }
                    }
                    row.extend(vals);
                    writeln!(out, "{}", row.join(","))?;
                }
            }
            ExportFormat::Json | ExportFormat::Ndjson => {
                let ndjson = o.format == ExportFormat::Ndjson;
                if !ndjson {
                    write!(out, "[")?;
                }
                for (i, (topic, m)) in sel.iter().enumerate() {
                    let (payload, enc) = Self::payload_cell(m, o.raw_base64);
                    let mut obj = Map::new();
                    obj.insert("time".into(), Value::String(iso_ms(m.received_at)));
                    obj.insert("receivedAt".into(), Value::from(m.received_at));
                    obj.insert("topic".into(), Value::String(topic.to_string()));
                    obj.insert("payload".into(), Value::String(payload));
                    obj.insert("encoding".into(), Value::String(enc.into()));
                    obj.insert("qos".into(), Value::from(m.qos));
                    obj.insert("retained".into(), Value::Bool(m.retain));
                    if m.truncated() {
                        obj.insert("size".into(), Value::from(m.size as u64));
                        obj.insert("truncated".into(), Value::Bool(true));
                    }
                    if o.include_properties {
                        obj.insert("properties".into(), Self::props_json(m));
                    }
                    let text =
                        serde_json::to_string(&Value::Object(obj)).map_err(io::Error::other)?;
                    if ndjson {
                        writeln!(out, "{text}")?;
                    } else {
                        if i > 0 {
                            write!(out, ",")?;
                        }
                        write!(out, "\n{text}")?;
                    }
                }
                if !ndjson {
                    writeln!(out, "\n]")?;
                }
            }
        }
        Ok(())
    }

    /// Exact size preview of an export: messages, topics, bytes.
    pub fn export_estimate(&self, o: &ExportOptions) -> ExportEstimate {
        let sel = self.export_selection(o);
        let mut topics: Vec<&str> = sel.iter().map(|(t, _)| *t).collect();
        topics.sort_unstable();
        topics.dedup();
        let mut c = Counter(0);
        let _ = self.export(o, &mut c);
        ExportEstimate {
            messages: sel.len() as u64,
            topics: topics.len() as u64,
            bytes: c.0,
        }
    }

    /// Chart CSV: `time` plus one column per series, rows on the union of timestamps, empty
    /// cells where a series has no point at that time.
    pub fn export_chart_csv(&self, series: &[ChartSeriesRef], since_ms: Option<i64>) -> String {
        let data: Vec<BTreeMap<i64, f64>> = series
            .iter()
            .map(|s| {
                self.series(&s.topic, &s.path, since_ms)
                    .points
                    .into_iter()
                    .collect()
            })
            .collect();
        let mut times: Vec<i64> = data.iter().flat_map(|d| d.keys().copied()).collect();
        times.sort_unstable();
        times.dedup();
        let mut out = String::from("time");
        for s in series {
            out.push(',');
            out.push_str(&csv_cell(&s.name));
        }
        out.push('\n');
        for t in times {
            out.push_str(&iso_ms(t));
            for d in &data {
                out.push(',');
                if let Some(v) = d.get(&t) {
                    out.push_str(&num_text(*v));
                }
            }
            out.push('\n');
        }
        out
    }

    /// Topics in an export scope, in tree order (for the UI preview).
    pub fn export_topics(&self, o: &ExportOptions) -> Vec<String> {
        let sel = self.export_selection(o);
        let mut topics: Vec<&str> = sel.iter().map(|(t, _)| *t).collect();
        topics.sort_by(|a, b| path_cmp(a, b));
        topics.dedup();
        topics.into_iter().map(str::to_string).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso() {
        assert_eq!(iso_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_ms(1_760_000_000_123), "2025-10-09T08:53:20.123Z");
        assert_eq!(iso_ms(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
