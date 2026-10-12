//! View and query types shared with the UI.
//!
//! Every type here serializes exactly like the TypeScript type of the same name in
//! `packages/engine-client/src/types.ts` (camelCase field names, `null` for absent values).

use serde::{Deserialize, Serialize};

pub use crate::message::UserMessage;

/// Search filters shared by the tree and the table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filters {
    /// Only topics whose retained flag is set.
    pub retained_only: bool,
    /// Only topics that received a message within this many minutes of `now`.
    pub changed_within_min: Option<f64>,
    /// Case-insensitive substring of the last payload (UTF-8, lossy).
    pub payload_contains: String,
    /// QoS of the last message.
    pub qos: Option<u8>,
    /// Only topics that have a last message.
    pub has_value: bool,
}

impl Filters {
    /// True when at least one filter restricts the result.
    pub fn is_active(&self) -> bool {
        self.retained_only
            || self.changed_within_min.is_some()
            || !self.payload_contains.is_empty()
            || self.qos.is_some()
            || self.has_value
    }
}

/// Tree query: search text plus filters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TreeQuery {
    /// Plain text (substring), MQTT wildcard filter (+, #), or regex when it starts with "/".
    pub search: String,
    /// Filters.
    pub filters: Filters,
}

impl TreeQuery {
    /// True when the query restricts the visible rows (search or filters).
    pub fn is_active(&self) -> bool {
        !self.search.is_empty() || self.filters.is_active()
    }
}

/// One visible row of the topic tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRow {
    pub path: String,
    pub name: String,
    pub depth: u32,
    pub has_children: bool,
    pub expanded: bool,
    pub topic_count: u64,
    pub messages: u64,
    pub is_topic: bool,
    pub value: Option<String>,
    pub retained: bool,
    pub last_received_at: Option<i64>,
    pub active: bool,
    pub rate: f64,
    pub spark: Vec<f64>,
    pub spark_times: Vec<i64>,
    pub format_hint: Option<String>,
    pub pinned: bool,
}

/// A window of visible tree rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeSlice {
    pub total: u64,
    pub start: u64,
    pub rows: Vec<TreeRow>,
    pub seq: u64,
}

/// Table columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TableColumn {
    Pin,
    #[default]
    Topic,
    Value,
    Spark,
    Updated,
    Rate,
    Messages,
    Qos,
    Retained,
    Size,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

/// Table sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TableSort {
    pub column: TableColumn,
    pub dir: SortDir,
}

/// Table query: the tree query plus a sort.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TableQuery {
    pub search: String,
    pub filters: Filters,
    pub sort: TableSort,
}

impl TableQuery {
    /// The search and filter part of the query.
    pub fn tree_query(&self) -> TreeQuery {
        TreeQuery {
            search: self.search.clone(),
            filters: self.filters.clone(),
        }
    }
}

/// One table row (a topic that received a message).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    pub path: String,
    pub value: Option<String>,
    pub spark: Vec<f64>,
    pub spark_times: Vec<i64>,
    pub last_received_at: Option<i64>,
    pub rate: f64,
    pub messages: u64,
    pub qos: Option<u8>,
    pub retained: bool,
    pub size: u64,
    pub pinned: bool,
}

/// A window of table rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSlice {
    pub total: u64,
    pub topics_in_view: u64,
    pub start: u64,
    pub rows: Vec<TableRow>,
    pub seq: u64,
}

/// Payload format, forced or detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PayloadFormat {
    #[default]
    Auto,
    Json,
    Text,
    Hex,
    Base64,
    Image,
    Cbor,
    Msgpack,
    Sparkplug,
    Protobuf,
}

impl PayloadFormat {
    /// True for formats decoded to a structured JSON value.
    pub fn is_structured(self) -> bool {
        matches!(
            self,
            PayloadFormat::Json
                | PayloadFormat::Cbor
                | PayloadFormat::Msgpack
                | PayloadFormat::Sparkplug
                | PayloadFormat::Protobuf
        )
    }
}

/// MQTT 5 properties of a message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Mqtt5Props {
    pub content_type: Option<String>,
    pub payload_format: Option<u8>,
    pub response_topic: Option<String>,
    /// UTF-8 when valid, else hex.
    pub correlation_data: Option<String>,
    /// Seconds.
    pub message_expiry: Option<u32>,
    pub user_properties: Vec<(String, String)>,
    pub subscription_ids: Vec<u32>,
    pub topic_alias: Option<u16>,
}

/// A decoded payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedPayload {
    /// Format used.
    pub format: PayloadFormat,
    /// Format auto-detection chose.
    pub detected: PayloadFormat,
    pub json: Option<serde_json::Value>,
    pub pretty: Option<String>,
    pub text: Option<String>,
    pub hex: Option<String>,
    pub base64: Option<String>,
    pub image_data_url: Option<String>,
    pub error: Option<UserMessage>,
    pub paged: bool,
}

/// One buffered message, decoded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub seq: u64,
    pub topic: String,
    pub received_at: i64,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    pub size: u64,
    pub truncated: bool,
    pub decoded: DecodedPayload,
    pub properties: Option<Mqtt5Props>,
}

/// One entry of the per-topic history list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub seq: u64,
    pub received_at: i64,
    pub size: u64,
    pub qos: u8,
    pub retain: bool,
    pub changed_keys: Vec<String>,
    pub preview: String,
}

/// How a field value casts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldCast {
    Number,
    ParsedNumber,
    Boolean,
    Text,
    Null,
    Object,
    Array,
}

/// A field of the last message, for the field picker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub path: String,
    pub label: String,
    pub last_value: String,
    pub cast: FieldCast,
    pub plottable: bool,
}

/// Everything the detail pane needs for one topic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicDetail {
    pub topic: String,
    pub exists: bool,
    pub last: Option<MessageView>,
    pub history_count: u64,
    pub buffer_size: u64,
    pub messages: u64,
    pub pinned: bool,
    pub fields: Vec<FieldInfo>,
    pub default_field: Option<String>,
    pub seq: u64,
}

/// Kind of a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffKind {
    Same,
    Changed,
    Added,
    Removed,
}

/// One line of a pretty-printed JSON diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub indent: u32,
    pub key: Option<String>,
    pub path: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub open: Option<String>,
    pub close: Option<String>,
    pub kind: DiffKind,
    pub plottable: bool,
}

/// One line of a text diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLine {
    pub kind: DiffKind,
    pub text: String,
}

/// Diff mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffMode {
    Json,
    Text,
}

/// Result of comparing two buffered messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub mode: DiffMode,
    pub lines: Vec<DiffLine>,
    pub text_lines: Vec<TextLine>,
    pub changes: u64,
    pub a: u64,
    pub b: u64,
}

/// Cast rule of a series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CastRule {
    Number,
    ParsedNumber,
    Boolean,
    Mixed,
    None,
}

/// Points of one series (topic plus field path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesData {
    pub topic: String,
    pub path: String,
    pub points: Vec<(i64, f64)>,
    pub gaps: Vec<u64>,
    pub skipped: u64,
    pub first_skip_reason: Option<String>,
    pub cast_rule: CastRule,
    pub step: bool,
    pub precision_loss: bool,
    pub count: u64,
}

/// Live memory estimate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEstimate {
    pub bytes: u64,
    pub budget_bytes: u64,
    pub points: u64,
    pub messages: u64,
}

/// What to export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExportScope {
    Topic { topic: String },
    Subtree { prefix: String },
    Topics { topics: Vec<String> },
}

/// Export file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Json,
    Ndjson,
}

/// Export options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub scope: ExportScope,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub format: ExportFormat,
    pub include_properties: bool,
    pub raw_base64: bool,
}

/// Size preview of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportEstimate {
    pub messages: u64,
    pub topics: u64,
    pub bytes: u64,
}

/// A series of a chart, for the chart CSV export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeriesRef {
    pub name: String,
    pub topic: String,
    pub path: String,
}
