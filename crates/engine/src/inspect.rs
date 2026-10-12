//! Detail pane queries: topic detail, message, history, diff, fields, series.

use crate::buffer::{self, StoredMessage};
use crate::cast::{cast_value, field_cast};
use crate::decode::{self};
use crate::diff;
use crate::engine::Engine;
use crate::path;
use crate::series::Series;
use crate::store::flags;
use crate::types::{
    CastRule, DiffMode, DiffResult, FieldCast, FieldInfo, HistoryEntry, MessageView, PayloadFormat,
    SeriesData, TopicDetail,
};
use serde_json::Value;

/// Most fields listed by the picker.
pub const MAX_FIELDS: usize = 500;

impl Engine {
    pub(crate) fn view_of(
        &self,
        topic: &str,
        m: &StoredMessage,
        format: PayloadFormat,
        format_anyway: bool,
    ) -> MessageView {
        let opts = self.decode_opts(topic, format_anyway);
        MessageView {
            seq: m.seq,
            topic: topic.to_string(),
            received_at: m.received_at,
            qos: m.qos,
            retain: m.retain,
            dup: m.dup,
            size: m.size as u64,
            truncated: m.truncated(),
            decoded: decode::decode_payload(&m.payload, format, m.content_type(), topic, &opts),
            properties: m.props.as_deref().cloned(),
        }
    }

    fn buffer_of(&self, topic: &str) -> Option<&std::collections::VecDeque<StoredMessage>> {
        let id = self.lookup(topic)?;
        self.nodes[id as usize].topic.as_ref().map(|t| &t.buffer)
    }

    /// One buffered message, decoded with `format` (`auto` detects).
    pub fn message(
        &self,
        topic: &str,
        seq: u64,
        format: PayloadFormat,
        format_anyway: bool,
    ) -> Option<MessageView> {
        let buf = self.buffer_of(topic)?;
        let i = buffer::find(buf, seq)?;
        Some(self.view_of(topic, &buf[i], format, format_anyway))
    }

    /// Everything the detail pane shows for a topic.
    pub fn topic_detail(&self, topic: &str) -> TopicDetail {
        let id = self.lookup(topic);
        let node = id.map(|i| &self.nodes[i as usize]);
        let td = node.and_then(|n| n.topic.as_deref());
        let last = td.and_then(|t| t.buffer.back());
        TopicDetail {
            topic: topic.to_string(),
            exists: td.is_some(),
            last: last.map(|m| self.view_of(topic, m, PayloadFormat::Auto, false)),
            history_count: td.map_or(0, |t| t.buffer.len() as u64),
            buffer_size: td.map_or(self.cfg.buffer_per_topic, |t| self.buffer_cap(t)) as u64,
            messages: node.map_or(0, |n| n.own_messages),
            pinned: node.is_some_and(|n| n.has(flags::PINNED)) || self.pinned_paths.contains(topic),
            fields: self.fields(topic),
            default_field: td
                .and_then(|t| t.default_series.and_then(|i| t.series.get(i as usize)))
                .map(|s| s.path.to_string()),
            seq: self.change_seq,
        }
    }

    /// Leaf fields of the last message, for the field picker.
    pub fn fields(&self, topic: &str) -> Vec<FieldInfo> {
        let Some(m) = self.buffer_of(topic).and_then(|b| b.back()) else {
            return Vec::new();
        };
        let (format, doc) = self.doc_of(topic, m);
        match doc {
            Some(doc) if format.is_structured() => {
                let mut out = Vec::new();
                path::walk_leaves(&doc, 32, MAX_FIELDS, &mut |p, v| out.push(field_info(p, v)));
                out
            }
            Some(doc) => vec![field_info("$", &doc)],
            None => vec![FieldInfo {
                path: "$".into(),
                label: "value".into(),
                last_value: format!("<binary {}>", decode::human_size(m.size)),
                cast: FieldCast::Text,
                plottable: false,
            }],
        }
    }

    /// The buffered messages of a topic with the keys that changed against the previous one.
    pub fn history(&self, topic: &str) -> Vec<HistoryEntry> {
        let Some(buf) = self.buffer_of(topic) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(buf.len());
        let mut prev: Option<(&StoredMessage, Option<Value>, bool)> = None;
        for m in buf {
            let opts = self.decode_opts(topic, true);
            let a = decode::analyse(
                &m.payload,
                m.content_type(),
                topic,
                opts.protobuf,
                opts.sparkplug_aliases,
            );
            let structured = a.format.is_structured();
            let changed_keys = match &prev {
                None => Vec::new(),
                Some((pm, pdoc, pstruct)) => match (pdoc, &a.doc) {
                    (Some(x), Some(y)) if *pstruct && structured => diff::changed_keys(x, y),
                    _ => {
                        if pm.payload == m.payload {
                            Vec::new()
                        } else {
                            vec!["payload".to_string()]
                        }
                    }
                },
            };
            out.push(HistoryEntry {
                seq: m.seq,
                received_at: m.received_at,
                size: m.size as u64,
                qos: m.qos,
                retain: m.retain,
                changed_keys,
                preview: a.preview,
            });
            prev = Some((m, a.doc, structured));
        }
        out
    }

    /// Compares two buffered messages. A missing `baseline_seq` falls back to the message
    /// before `seq`; a missing `seq` to the latest message.
    pub fn diff(&self, topic: &str, baseline_seq: u64, seq: u64) -> DiffResult {
        let empty = |a: u64, b: u64| DiffResult {
            mode: DiffMode::Text,
            lines: Vec::new(),
            text_lines: Vec::new(),
            changes: 0,
            a,
            b,
        };
        let Some(buf) = self.buffer_of(topic) else {
            return empty(baseline_seq, seq);
        };
        let Some(bi) = buffer::find(buf, seq).or_else(|| buf.len().checked_sub(1)) else {
            return empty(baseline_seq, seq);
        };
        let ai = buffer::find(buf, baseline_seq).or_else(|| bi.checked_sub(1));
        let new = &buf[bi];
        let Some(ai) = ai else {
            // Nothing to compare with: everything is new.
            let mut r = self.diff_pair(topic, None, new);
            r.a = 0;
            return r;
        };
        self.diff_pair(topic, Some(&buf[ai]), new)
    }

    fn diff_pair(
        &self,
        topic: &str,
        old: Option<&StoredMessage>,
        new: &StoredMessage,
    ) -> DiffResult {
        let nv = self.structured_of(topic, new);
        let ov = old.and_then(|o| self.structured_of(topic, o));
        let a = old.map_or(0, |o| o.seq);
        match (old, &ov, &nv) {
            (Some(_), Some(x), Some(y)) => {
                let (lines, changes) = diff::json_diff(x, y);
                DiffResult {
                    mode: DiffMode::Json,
                    lines,
                    text_lines: Vec::new(),
                    changes,
                    a,
                    b: new.seq,
                }
            }
            (None, None, Some(y)) => {
                let (lines, changes) = diff::json_diff(&Value::Null, y);
                DiffResult {
                    mode: DiffMode::Json,
                    lines,
                    text_lines: Vec::new(),
                    changes,
                    a,
                    b: new.seq,
                }
            }
            _ => {
                let text = |m: &StoredMessage| match decode::json::as_text(&m.payload) {
                    Some(s) => s.to_string(),
                    None => {
                        decode::hex_dump(&m.payload[..m.payload.len().min(decode::PAGED_HEX_BYTES)])
                    }
                };
                let ot = old.map(text).unwrap_or_default();
                let (text_lines, changes) = diff::text_diff(&ot, &text(new));
                DiffResult {
                    mode: DiffMode::Text,
                    lines: Vec::new(),
                    text_lines,
                    changes,
                    a,
                    b: new.seq,
                }
            }
        }
    }

    /// Points of a series. Auto-recorded series answer from memory; other paths are resolved on
    /// the buffered messages.
    pub fn series(&self, topic: &str, path_text: &str, since_ms: Option<i64>) -> SeriesData {
        let norm = path::normalize(path_text).unwrap_or_else(|| path_text.to_string());
        let empty = || SeriesData {
            topic: topic.to_string(),
            path: norm.clone(),
            points: Vec::new(),
            gaps: Vec::new(),
            skipped: 0,
            first_skip_reason: None,
            cast_rule: CastRule::None,
            step: false,
            precision_loss: false,
            count: 0,
        };
        let Some(td) = self
            .lookup(topic)
            .and_then(|i| self.nodes[i as usize].topic.as_deref())
        else {
            return empty();
        };
        if let Some(s) = td.series.iter().find(|s| *s.path == *norm) {
            return s.to_data(topic, since_ms, self.cfg.gap_factor);
        }
        let Some(steps) = path::parse(&norm) else {
            return empty();
        };
        let mut s = Series::new(&norm);
        for m in &td.buffer {
            let (_, doc) = self.doc_of(topic, m);
            match doc {
                None => s.skip("binary payload"),
                Some(d) => match path::resolve(&d, &steps) {
                    Some(v) => s.record(m.received_at, &cast_value(v), self.cfg.points_per_series),
                    None => s.skip("field absent"),
                },
            }
        }
        s.to_data(topic, since_ms, self.cfg.gap_factor)
    }
}

fn field_info(p: &str, v: &Value) -> FieldInfo {
    let cast = field_cast(v);
    let mut last_value = v.to_string();
    if last_value.len() > 200 {
        let cut = (0..=200)
            .rev()
            .find(|&i| last_value.is_char_boundary(i))
            .unwrap_or(0);
        last_value.truncate(cut);
        last_value.push('…');
    }
    FieldInfo {
        path: p.to_string(),
        label: path::label(p),
        last_value,
        cast,
        plottable: matches!(
            cast,
            FieldCast::Number | FieldCast::ParsedNumber | FieldCast::Boolean
        ),
    }
}
