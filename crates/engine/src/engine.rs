//! The engine: one instance per MQTT connection. Pure state machine, no I/O, no clock.

use crate::buffer::{self, StoredMessage};
use crate::cast::{cast_value, Cast};
use crate::decode::{
    self, sparkplug, AliasMap, DecodeOptions, ProtoAssignment, ProtoFile, ProtoRegistry,
};
use crate::message::UserMessage;
use crate::path;
use crate::search::natural_cmp;
use crate::series::Series;
use crate::store::{flags, Node, NodeId, PathRef, RateWindow, TopicData, NONE, ROOT};
use crate::types::{MemoryEstimate, Mqtt5Props, PayloadFormat};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Most series recorded automatically per topic.
pub const MAX_AUTO_SERIES: usize = 64;
/// Deepest field level recorded automatically.
pub const MAX_AUTO_DEPTH: usize = 8;
/// Leaves visited per message for automatic series.
pub const MAX_LEAVES: usize = 256;
/// Values in a sparkline.
pub const SPARK_POINTS: usize = 10;
/// Default longest topic kept in the tree, in bytes.
pub const DEFAULT_MAX_TOPIC_LEN: usize = 1024;
/// Marker appended to a topic cut at [`EngineConfig::max_topic_len`].
pub const TRUNCATED_MARK: &str = "…";

/// Engine settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineConfig {
    /// Ring buffer size per topic (10..10 000).
    pub buffer_per_topic: usize,
    /// Points kept per series; the oldest are dropped.
    pub points_per_series: usize,
    /// Payloads above this size are shown paged.
    pub payload_render_limit: usize,
    /// Payloads above this size are kept truncated.
    pub max_kept_payload: usize,
    /// Memory budget reported with the estimate.
    pub memory_budget: usize,
    /// A gap starts when an interval exceeds this factor times the median interval.
    pub gap_factor: f64,
    /// Longest topic kept in the tree, in bytes (per profile, default 1024). A longer topic is counted under its
    /// first bytes followed by "…" ([`Engine::long_topics`] counts such messages).
    pub max_topic_len: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            buffer_per_topic: 200,
            points_per_series: 5000,
            payload_render_limit: 256 * 1024,
            max_kept_payload: 1024 * 1024,
            memory_budget: 256 * 1024 * 1024,
            gap_factor: 5.0,
            max_topic_len: DEFAULT_MAX_TOPIC_LEN,
        }
    }
}

/// A PUBLISH received from the broker.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    /// Receive time, milliseconds since the Unix epoch (supplied by the caller).
    pub received_at: i64,
    /// MQTT 5 properties; `None` on 3.1.1.
    pub props: Option<Mqtt5Props>,
}

impl IncomingMessage {
    /// A message without properties.
    pub fn new(topic: &str, payload: impl Into<Vec<u8>>, received_at: i64) -> Self {
        IncomingMessage {
            topic: topic.to_string(),
            payload: payload.into(),
            qos: 0,
            retain: false,
            dup: false,
            received_at,
            props: None,
        }
    }
}

/// Cached visible rows of the tree.
#[derive(Debug, Clone)]
pub(crate) struct TreeCache {
    pub key: (crate::types::TreeQuery, u64, u64, u64, i64),
    /// (node, shown expanded)
    pub rows: Vec<(NodeId, bool)>,
}

/// Cached sorted rows of the table.
#[derive(Debug, Clone)]
pub(crate) struct TableCache {
    pub key: (crate::types::TableQuery, u64, u64, i64),
    pub rows: Vec<NodeId>,
}

/// The engine state of one connection.
#[derive(Debug)]
pub struct Engine {
    pub(crate) cfg: EngineConfig,
    pub(crate) nodes: Vec<Node>,
    pub(crate) index: HashMap<PathRef, NodeId>,
    /// Bytes of the shared topic strings behind the node paths (memory estimate).
    pub(crate) path_bytes: usize,
    /// Messages whose topic was longer than the limit and was cut.
    pub(crate) long_topics: u64,
    pub(crate) msg_seq: u64,
    pub(crate) change_seq: u64,
    pub(crate) structure_ver: u64,
    pub(crate) expand_ver: u64,
    pub(crate) global_rate: RateWindow,
    pub(crate) topics: usize,
    pub(crate) pending_expanded: HashSet<String>,
    pub(crate) pinned_paths: HashSet<String>,
    pub(crate) unsorted: Vec<NodeId>,
    pub(crate) tree_cache: Option<TreeCache>,
    pub(crate) table_cache: Option<TableCache>,
    pub(crate) rank: Option<(u64, Vec<u32>)>,
    pub(crate) aliases: HashMap<String, AliasMap>,
    pub(crate) proto: Option<ProtoRegistry>,
    /// Per topic, the value picked with "Use as sparkline" (change set 8).
    pub(crate) sparkline_paths: HashMap<String, String>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(EngineConfig::default())
    }
}

impl Engine {
    /// A new, empty engine.
    pub fn new(cfg: EngineConfig) -> Self {
        let mut cfg = cfg;
        cfg.buffer_per_topic = buffer::clamp_buffer(cfg.buffer_per_topic);
        cfg.points_per_series = cfg.points_per_series.max(1);
        Engine {
            cfg,
            nodes: vec![Node::new(PathRef::new(Arc::from(""), 0), NONE, 0)],
            index: HashMap::new(),
            path_bytes: 0,
            long_topics: 0,
            msg_seq: 0,
            change_seq: 0,
            structure_ver: 0,
            expand_ver: 0,
            global_rate: RateWindow::default(),
            topics: 0,
            pending_expanded: HashSet::new(),
            pinned_paths: HashSet::new(),
            unsorted: Vec::new(),
            tree_cache: None,
            table_cache: None,
            rank: None,
            aliases: HashMap::new(),
            proto: None,
            sparkline_paths: HashMap::new(),
        }
    }

    /// Current settings.
    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// Applies new settings; buffers and series shrink at once when their caps go down.
    pub fn set_config(&mut self, cfg: EngineConfig) {
        let mut cfg = cfg;
        cfg.buffer_per_topic = buffer::clamp_buffer(cfg.buffer_per_topic);
        cfg.points_per_series = cfg.points_per_series.max(1);
        let shrink_buf = cfg.buffer_per_topic < self.cfg.buffer_per_topic;
        let shrink_pts = cfg.points_per_series < self.cfg.points_per_series;
        self.cfg = cfg;
        if shrink_buf || shrink_pts {
            for n in &mut self.nodes {
                if let Some(td) = n.topic.as_mut() {
                    if shrink_buf && td.buffer_cap.is_none() {
                        buffer::truncate(&mut td.buffer, self.cfg.buffer_per_topic);
                    }
                    if shrink_pts {
                        for s in &mut td.series {
                            s.truncate(self.cfg.points_per_series);
                        }
                    }
                }
            }
        }
        self.bump();
    }

    pub(crate) fn bump(&mut self) {
        self.change_seq += 1;
    }

    /// Sequence number that changes on every change (re-request slices when it moves).
    pub fn seq(&self) -> u64 {
        self.change_seq
    }

    /// Number of topics (paths that received a message).
    pub fn topic_count(&self) -> usize {
        self.topics
    }

    /// Messages per second over the 10 s window ending at `now_ms`.
    pub fn msg_rate(&self, now_ms: i64) -> f64 {
        self.global_rate.rate(now_ms)
    }

    pub(crate) fn lookup(&self, topic: &str) -> Option<NodeId> {
        self.index.get(topic).copied()
    }

    /// Messages received on a topic longer than [`EngineConfig::max_topic_len`] (shown cut).
    pub fn long_topics(&self) -> u64 {
        self.long_topics
    }

    fn get_or_create(&mut self, topic: &str) -> NodeId {
        if let Some(&id) = self.index.get(topic) {
            return id;
        }
        // One allocation for the whole topic; every new level points into it.
        let full: Arc<str> = Arc::from(topic);
        self.path_bytes += 16 + ((topic.len() + 7) & !7);
        let mut parent = ROOT;
        let ends = topic
            .match_indices('/')
            .map(|(i, _)| i)
            .chain(std::iter::once(topic.len()));
        for end in ends {
            let prefix = &topic[..end];
            if let Some(&id) = self.index.get(prefix) {
                parent = id;
                continue;
            }
            let id = self.nodes.len() as NodeId;
            let depth = if parent == ROOT {
                0
            } else {
                self.nodes[parent as usize].depth.saturating_add(1)
            };
            let path = PathRef::new(full.clone(), end);
            let mut node = Node::new(path.clone(), parent, depth);
            if !self.pending_expanded.is_empty() && self.pending_expanded.remove(prefix) {
                node.set(flags::EXPANDED, true);
            }
            if !self.pinned_paths.is_empty() && self.pinned_paths.contains(prefix) {
                node.set(flags::PINNED, true);
            }
            let name = node.name().to_string();
            self.nodes.push(node);
            self.index.insert(path, id);
            let p = &self.nodes[parent as usize];
            let needs_sort = p.has(flags::SORTED)
                && p.children.last().is_some_and(|&last| {
                    let last_name = self.nodes[last as usize].name();
                    natural_cmp(&name, last_name) == std::cmp::Ordering::Less
                });
            let p = &mut self.nodes[parent as usize];
            p.children.push(id);
            if needs_sort {
                p.set(flags::SORTED, false);
                self.unsorted.push(parent);
            }
            parent = id;
            self.structure_ver += 1;
        }
        parent
    }

    /// Sorts the children lists that received out-of-order segments.
    pub(crate) fn ensure_sorted(&mut self) {
        while let Some(id) = self.unsorted.pop() {
            let mut children = std::mem::take(&mut self.nodes[id as usize].children);
            {
                let nodes = &self.nodes;
                children.sort_by(|&a, &b| {
                    natural_cmp(nodes[a as usize].name(), nodes[b as usize].name())
                });
            }
            let n = &mut self.nodes[id as usize];
            n.children = children;
            n.set(flags::SORTED, true);
        }
    }

    /// Ingests one received message. Cost is O(depth) plus decoding the payload.
    pub fn ingest(&mut self, msg: IncomingMessage) {
        if msg.topic.is_empty() {
            return;
        }
        self.msg_seq += 1;
        self.change_seq += 1;
        let seq = self.msg_seq;
        let t = msg.received_at;
        let max = self.cfg.max_topic_len.max(1);
        let id = if msg.topic.len() > max {
            // Kept under its first bytes: memory per topic stays bounded whatever the broker sends.
            self.long_topics += 1;
            let mut cut = max;
            while !msg.topic.is_char_boundary(cut) {
                cut -= 1;
            }
            let shown = format!("{}{TRUNCATED_MARK}", &msg.topic[..cut]);
            self.get_or_create(&shown)
        } else {
            self.get_or_create(&msg.topic)
        };

        // Sparkplug: births establish the alias map of the edge node.
        let edge_key = sparkplug::parse_topic(&msg.topic).map(|sp| {
            let key = sp.edge_key();
            if sp.is_birth() {
                if let Ok(p) = sparkplug::decode_raw(&msg.payload) {
                    let map = self.aliases.entry(key.clone()).or_default();
                    if sp.message_type == "NBIRTH" {
                        *map = AliasMap::default();
                    }
                    map.learn(&p);
                }
            }
            key
        });
        let content_type = msg.props.as_ref().and_then(|p| p.content_type.as_deref());
        let aliases = edge_key.as_ref().and_then(|k| self.aliases.get(k));
        let analysis = decode::analyse(
            &msg.payload,
            content_type,
            &msg.topic,
            self.proto.as_ref(),
            aliases,
        );

        if self.nodes[id as usize].topic.is_none() {
            self.nodes[id as usize].topic = Some(Box::default());
            self.topics += 1;
            self.structure_ver += 1;
            let mut cur = id;
            while cur != NONE {
                let n = &mut self.nodes[cur as usize];
                n.topic_count += 1;
                cur = n.parent;
            }
        }
        let mut cur = id;
        while cur != NONE {
            let n = &mut self.nodes[cur as usize];
            n.messages += 1;
            if t > n.last_received_at {
                n.last_received_at = t;
            }
            n.rate.add(t);
            cur = n.parent;
        }
        self.global_rate.add(t);

        let cap_points = self.cfg.points_per_series;
        let cap_buf = self.cfg.buffer_per_topic;
        let picked = self.sparkline_paths.get(&*msg.topic).cloned();
        let max_kept = self.cfg.max_kept_payload;
        let node = &mut self.nodes[id as usize];
        node.own_messages += 1;
        if msg.retain {
            node.set(flags::RETAINED, !msg.payload.is_empty());
        }
        let Some(td) = node.topic.as_mut() else {
            return;
        };
        td.preview = analysis.preview.into_boxed_str();
        td.format = analysis.format;
        td.last_qos = msg.qos;
        td.last_size = msg.payload.len() as u64;
        td.last_at = t;
        update_series(td, analysis.doc.as_ref(), t, cap_points, picked.as_deref());

        let size = msg.payload.len();
        let mut payload = msg.payload;
        if payload.len() > max_kept {
            payload.truncate(max_kept);
        }
        let stored = StoredMessage {
            seq,
            received_at: t,
            payload: payload.into_boxed_slice(),
            size,
            qos: msg.qos,
            retain: msg.retain,
            dup: msg.dup,
            props: msg.props.map(Box::new),
        };
        let cap = td.buffer_cap.map_or(cap_buf, |c| c as usize);
        buffer::push(&mut td.buffer, stored, cap);
    }

    /// Reconnect: messages-since-connect back to 0 everywhere.
    pub fn reset_counts(&mut self) {
        for n in &mut self.nodes {
            n.messages = 0;
            n.own_messages = 0;
        }
        self.bump();
    }

    /// Drops buffers, series and counts at or under `prefix` (`None` = everything).
    /// Topics stay known; their last values are gone.
    pub fn clear_history(&mut self, prefix: Option<&str>) {
        let under = |p: &str| match prefix {
            None => true,
            Some(pre) => {
                p == pre
                    || (p.len() > pre.len()
                        && p.starts_with(pre)
                        && p.as_bytes()[pre.len()] == b'/')
            }
        };
        for i in 1..self.nodes.len() {
            if !under(&self.nodes[i].path) {
                continue;
            }
            let n = &mut self.nodes[i];
            n.own_messages = 0;
            n.rate.clear();
            if let Some(td) = n.topic.as_mut() {
                td.buffer = Default::default();
                td.series = Vec::new();
                td.default_series = None;
                td.value_num = None;
                td.preview = "".into();
            }
        }
        if prefix.is_none() {
            self.global_rate.clear();
            self.aliases.clear();
        }
        self.recount();
        self.bump();
    }

    /// Recomputes subtree message counts from the own counts.
    fn recount(&mut self) {
        for n in &mut self.nodes {
            n.messages = n.own_messages;
        }
        // Children always have a larger index than their parent.
        for i in (1..self.nodes.len()).rev() {
            let m = self.nodes[i].messages;
            let p = self.nodes[i].parent;
            if p != NONE {
                self.nodes[p as usize].messages += m;
            }
        }
    }

    /// Sets the ring buffer size of one topic (10..10 000).
    pub fn set_topic_buffer(&mut self, topic: &str, size: usize) {
        let size = buffer::clamp_buffer(size);
        let id = self.get_or_create(topic);
        let n = &mut self.nodes[id as usize];
        if n.topic.is_none() {
            n.topic = Some(Box::default());
            self.topics += 1;
            self.structure_ver += 1;
            let mut cur = id;
            while cur != NONE {
                let n = &mut self.nodes[cur as usize];
                n.topic_count += 1;
                cur = n.parent;
            }
        }
        if let Some(td) = self.nodes[id as usize].topic.as_mut() {
            td.buffer_cap = Some(size as u32);
            buffer::truncate(&mut td.buffer, size);
        }
        self.bump();
    }

    /// Ring buffer capacity of a topic.
    /// The values picked with "Use as sparkline", per topic (change set 8); replaces the previous set and applies at
    /// once to the topics already known.
    pub fn set_sparkline_paths(&mut self, paths: HashMap<String, String>) {
        self.sparkline_paths = paths;
        for n in &mut self.nodes {
            let Some(td) = n.topic.as_mut() else { continue };
            let picked = self.sparkline_paths.get(&*n.path).map(String::as_str);
            if let Some(p) = picked {
                if !td.series.iter().any(|s| &*s.path == p) {
                    td.series.push(Series::new(p));
                }
            }
            set_default_series(td, picked);
            td.value_num = td
                .default_series
                .and_then(|i| td.series[i as usize].points.back().map(|p| p.1));
        }
        self.table_cache = None;
        self.bump();
    }

    pub(crate) fn buffer_cap(&self, td: &TopicData) -> usize {
        td.buffer_cap
            .map_or(self.cfg.buffer_per_topic, |c| c as usize)
    }

    /// Replaces the Protobuf schemas and their topic assignments.
    pub fn set_proto_schemas(
        &mut self,
        files: Vec<ProtoFile>,
        assignments: Vec<ProtoAssignment>,
    ) -> Result<(), UserMessage> {
        if files.is_empty() && assignments.is_empty() {
            self.proto = None;
        } else {
            self.proto = Some(ProtoRegistry::new(&files, assignments)?);
        }
        self.bump();
        Ok(())
    }

    /// Decode options for a topic.
    pub(crate) fn decode_opts<'a>(&'a self, topic: &str, format_anyway: bool) -> DecodeOptions<'a> {
        let aliases = sparkplug::parse_topic(topic).and_then(|sp| self.aliases.get(&sp.edge_key()));
        DecodeOptions {
            render_limit: self.cfg.payload_render_limit,
            format_anyway,
            protobuf: self.proto.as_ref(),
            sparkplug_aliases: aliases,
        }
    }

    /// Series document of a stored message (what series and fields read).
    pub(crate) fn doc_of(&self, topic: &str, m: &StoredMessage) -> (PayloadFormat, Option<Value>) {
        let opts = self.decode_opts(topic, true);
        decode::series_doc(
            &m.payload,
            m.content_type(),
            topic,
            opts.protobuf,
            opts.sparkplug_aliases,
        )
    }

    /// Structured JSON of a stored message (for diffs), or `None` for text and binary.
    pub(crate) fn structured_of(&self, topic: &str, m: &StoredMessage) -> Option<Value> {
        let opts = self.decode_opts(topic, true);
        let f = decode::detect(&m.payload, m.content_type(), topic, opts.protobuf);
        if f.is_structured() {
            decode::decode_structured(&m.payload, f, topic, &opts).ok()
        } else {
            None
        }
    }

    /// Live memory estimate: payloads, buffers, series points and per-node overhead.
    pub fn memory(&self) -> MemoryEstimate {
        use std::mem::size_of;
        let mut bytes = size_of::<Engine>();
        let mut points = 0u64;
        let mut messages = 0u64;
        bytes += self.nodes.capacity() * size_of::<Node>();
        // Path index: hashbrown stores (key, value) plus one control byte per bucket.
        let buckets = hash_buckets(self.index.capacity());
        bytes += buckets * (size_of::<(PathRef, NodeId)>() + 1) + 16;
        // Shared topic strings (Arc<str>: two counters plus the bytes), once per topic.
        bytes += self.path_bytes;
        for n in &self.nodes {
            bytes += n.children.capacity() * size_of::<NodeId>();
            if let Some(td) = &n.topic {
                bytes += size_of::<TopicData>();
                bytes += td.preview.len();
                bytes += td.buffer.capacity() * size_of::<StoredMessage>();
                messages += td.buffer.len() as u64;
                for m in &td.buffer {
                    bytes += m.heap_bytes();
                }
                bytes += td.series.capacity() * size_of::<Series>();
                for s in &td.series {
                    bytes += s.heap_bytes();
                    points += s.points.len() as u64;
                }
            }
        }
        bytes += self.unsorted.capacity() * size_of::<NodeId>();
        if let Some(c) = &self.tree_cache {
            bytes += c.rows.capacity() * size_of::<(NodeId, bool)>();
        }
        if let Some(c) = &self.table_cache {
            bytes += c.rows.capacity() * size_of::<NodeId>();
        }
        if let Some((_, r)) = &self.rank {
            bytes += r.capacity() * size_of::<u32>();
        }
        for s in self.pending_expanded.iter().chain(self.pinned_paths.iter()) {
            bytes += s.capacity() + size_of::<String>() + 1;
        }
        for (k, m) in &self.aliases {
            bytes += k.capacity() + size_of::<(String, AliasMap)>();
            bytes += hash_buckets(m.by_alias.capacity()) * (size_of::<(u64, (String, u32))>() + 1);
            bytes += hash_buckets(m.datatype_by_name.capacity()) * (size_of::<(String, u32)>() + 1);
            for (n, _) in m.by_alias.values() {
                bytes += n.capacity();
            }
            for n in m.datatype_by_name.keys() {
                bytes += n.capacity();
            }
        }
        if let Some(p) = &self.proto {
            bytes += p.approx_bytes();
        }
        MemoryEstimate {
            bytes: bytes as u64,
            budget_bytes: self.cfg.memory_budget as u64,
            points,
            messages,
        }
    }
}

/// Buckets hashbrown allocates for a given capacity.
fn hash_buckets(cap: usize) -> usize {
    if cap == 0 {
        0
    } else if cap < 4 {
        4
    } else if cap < 8 {
        8
    } else {
        (cap * 8 / 7).next_power_of_two()
    }
}

/// Records the series of one message: every castable leaf (bounded), plus the existing series.
fn update_series(
    td: &mut TopicData,
    doc: Option<&Value>,
    t: i64,
    cap: usize,
    picked: Option<&str>,
) {
    let mut touched: u64 = 0;
    let series = &mut td.series;
    if let Some(doc) = doc {
        path::walk_leaves(doc, MAX_AUTO_DEPTH, MAX_LEAVES, &mut |p, v| {
            let idx = series.iter().position(|s| &*s.path == p);
            match idx {
                Some(i) => {
                    series[i].record(t, &cast_value(v), cap);
                    if i < 64 {
                        touched |= 1 << i;
                    }
                }
                None => {
                    if series.len() < MAX_AUTO_SERIES {
                        let c = cast_value(v);
                        if matches!(c, Cast::Value { .. }) {
                            let mut s = Series::new(p);
                            s.record(t, &c, cap);
                            series.push(s);
                            touched |= 1 << (series.len() - 1);
                        }
                    }
                }
            }
        });
    }
    for (i, s) in series.iter_mut().enumerate() {
        if i < 64 && touched & (1 << i) != 0 {
            continue;
        }
        match doc {
            None => s.skip("binary payload"),
            Some(d) => {
                match path::parse(&s.path).and_then(|st| path::resolve(d, &st).map(cast_value)) {
                    Some(c) => s.record(t, &c, cap),
                    None => s.skip("field absent"),
                }
            }
        }
    }
    // The picked value is followed even beyond the automatic series cap.
    if let (Some(p), Some(d)) = (picked, doc) {
        if !td.series.iter().any(|s| &*s.path == p) {
            if let Some(c) = path::parse(p).and_then(|st| path::resolve(d, &st).map(cast_value)) {
                if matches!(c, Cast::Value { .. }) {
                    let mut s = Series::new(p);
                    s.record(t, &c, cap);
                    td.series.push(s);
                }
            }
        }
    }
    set_default_series(td, picked);
    td.value_num = td.default_series.and_then(|i| {
        let s = &td.series[i as usize];
        s.points.back().filter(|p| p.0 == t).map(|p| p.1)
    });
}

/// Sparkline series (change set 8): the whole payload when it casts to a number or boolean (a "$" series exists only
/// then), else the value picked with "Use as sparkline"; nothing is guessed from a structured payload.
fn set_default_series(td: &mut TopicData, picked: Option<&str>) {
    let root = td.series.iter().position(|s| &*s.path == "$");
    let pick = picked.and_then(|p| td.series.iter().position(|s| &*s.path == p));
    td.default_series = root.or(pick).map(|i| i as u16);
}
