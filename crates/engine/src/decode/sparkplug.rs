//! Sparkplug B payloads.
//!
//! The message structs below are written by hand from the public Sparkplug B payload structure
//! (Payload, Metric, DataSet, Template, PropertySet, MetaData).

use base64::Engine as _;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// A Sparkplug B payload.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Payload {
    #[prost(uint64, optional, tag = "1")]
    pub timestamp: Option<u64>,
    #[prost(message, repeated, tag = "2")]
    pub metrics: Vec<Metric>,
    #[prost(uint64, optional, tag = "3")]
    pub seq: Option<u64>,
    #[prost(string, optional, tag = "4")]
    pub uuid: Option<String>,
    #[prost(bytes = "vec", optional, tag = "5")]
    pub body: Option<Vec<u8>>,
}

/// A metric.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Metric {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(uint64, optional, tag = "2")]
    pub alias: Option<u64>,
    #[prost(uint64, optional, tag = "3")]
    pub timestamp: Option<u64>,
    #[prost(uint32, optional, tag = "4")]
    pub datatype: Option<u32>,
    #[prost(bool, optional, tag = "5")]
    pub is_historical: Option<bool>,
    #[prost(bool, optional, tag = "6")]
    pub is_transient: Option<bool>,
    #[prost(bool, optional, tag = "7")]
    pub is_null: Option<bool>,
    #[prost(message, optional, tag = "8")]
    pub metadata: Option<MetaData>,
    #[prost(message, optional, tag = "9")]
    pub properties: Option<PropertySet>,
    #[prost(oneof = "MetricValue", tags = "10, 11, 12, 13, 14, 15, 16, 17, 18")]
    pub value: Option<MetricValue>,
}

/// The value of a metric.
#[derive(Clone, PartialEq, prost::Oneof)]
pub enum MetricValue {
    #[prost(uint32, tag = "10")]
    IntValue(u32),
    #[prost(uint64, tag = "11")]
    LongValue(u64),
    #[prost(float, tag = "12")]
    FloatValue(f32),
    #[prost(double, tag = "13")]
    DoubleValue(f64),
    #[prost(bool, tag = "14")]
    BooleanValue(bool),
    #[prost(string, tag = "15")]
    StringValue(String),
    #[prost(bytes = "vec", tag = "16")]
    BytesValue(Vec<u8>),
    #[prost(message, tag = "17")]
    DatasetValue(DataSet),
    #[prost(message, tag = "18")]
    TemplateValue(Template),
}

/// Metric metadata.
#[derive(Clone, PartialEq, prost::Message)]
pub struct MetaData {
    #[prost(bool, optional, tag = "1")]
    pub is_multi_part: Option<bool>,
    #[prost(string, optional, tag = "2")]
    pub content_type: Option<String>,
    #[prost(uint64, optional, tag = "3")]
    pub size: Option<u64>,
    #[prost(uint64, optional, tag = "4")]
    pub seq: Option<u64>,
    #[prost(string, optional, tag = "5")]
    pub file_name: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub file_type: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub md5: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub description: Option<String>,
}

/// A property set.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PropertySet {
    #[prost(string, repeated, tag = "1")]
    pub keys: Vec<String>,
    #[prost(message, repeated, tag = "2")]
    pub values: Vec<PropertyValue>,
}

/// A list of property sets.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PropertySetList {
    #[prost(message, repeated, tag = "1")]
    pub propertyset: Vec<PropertySet>,
}

/// A property value.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PropertyValue {
    #[prost(uint32, optional, tag = "1")]
    pub r#type: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub is_null: Option<bool>,
    #[prost(oneof = "PropertyValueKind", tags = "3, 4, 5, 6, 7, 8, 9, 10")]
    pub value: Option<PropertyValueKind>,
}

/// The value of a property.
#[derive(Clone, PartialEq, prost::Oneof)]
pub enum PropertyValueKind {
    #[prost(uint32, tag = "3")]
    IntValue(u32),
    #[prost(uint64, tag = "4")]
    LongValue(u64),
    #[prost(float, tag = "5")]
    FloatValue(f32),
    #[prost(double, tag = "6")]
    DoubleValue(f64),
    #[prost(bool, tag = "7")]
    BooleanValue(bool),
    #[prost(string, tag = "8")]
    StringValue(String),
    #[prost(message, tag = "9")]
    PropertysetValue(PropertySet),
    #[prost(message, tag = "10")]
    PropertysetsValue(PropertySetList),
}

/// A data set (table).
#[derive(Clone, PartialEq, prost::Message)]
pub struct DataSet {
    #[prost(uint64, optional, tag = "1")]
    pub num_of_columns: Option<u64>,
    #[prost(string, repeated, tag = "2")]
    pub columns: Vec<String>,
    #[prost(uint32, repeated, packed = "false", tag = "3")]
    pub types: Vec<u32>,
    #[prost(message, repeated, tag = "4")]
    pub rows: Vec<DataSetRow>,
}

/// A data set row.
#[derive(Clone, PartialEq, prost::Message)]
pub struct DataSetRow {
    #[prost(message, repeated, tag = "1")]
    pub elements: Vec<DataSetValue>,
}

/// A data set cell.
#[derive(Clone, PartialEq, prost::Message)]
pub struct DataSetValue {
    #[prost(oneof = "DataSetValueKind", tags = "1, 2, 3, 4, 5, 6")]
    pub value: Option<DataSetValueKind>,
}

/// The value of a data set cell.
#[derive(Clone, PartialEq, prost::Oneof)]
pub enum DataSetValueKind {
    #[prost(uint32, tag = "1")]
    IntValue(u32),
    #[prost(uint64, tag = "2")]
    LongValue(u64),
    #[prost(float, tag = "3")]
    FloatValue(f32),
    #[prost(double, tag = "4")]
    DoubleValue(f64),
    #[prost(bool, tag = "5")]
    BooleanValue(bool),
    #[prost(string, tag = "6")]
    StringValue(String),
}

/// A template (user-defined type) definition or instance.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Template {
    #[prost(string, optional, tag = "1")]
    pub version: Option<String>,
    #[prost(message, repeated, tag = "2")]
    pub metrics: Vec<Metric>,
    #[prost(message, repeated, tag = "3")]
    pub parameters: Vec<TemplateParameter>,
    #[prost(string, optional, tag = "4")]
    pub template_ref: Option<String>,
    #[prost(bool, optional, tag = "5")]
    pub is_definition: Option<bool>,
}

/// A template parameter.
#[derive(Clone, PartialEq, prost::Message)]
pub struct TemplateParameter {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(uint32, optional, tag = "2")]
    pub r#type: Option<u32>,
    #[prost(oneof = "ParameterValue", tags = "3, 4, 5, 6, 7, 8")]
    pub value: Option<ParameterValue>,
}

/// The value of a template parameter.
#[derive(Clone, PartialEq, prost::Oneof)]
pub enum ParameterValue {
    #[prost(uint32, tag = "3")]
    IntValue(u32),
    #[prost(uint64, tag = "4")]
    LongValue(u64),
    #[prost(float, tag = "5")]
    FloatValue(f32),
    #[prost(double, tag = "6")]
    DoubleValue(f64),
    #[prost(bool, tag = "7")]
    BooleanValue(bool),
    #[prost(string, tag = "8")]
    StringValue(String),
}

/// Sparkplug data type names, indexed by code.
pub fn datatype_name(code: u32) -> &'static str {
    match code {
        1 => "Int8",
        2 => "Int16",
        3 => "Int32",
        4 => "Int64",
        5 => "UInt8",
        6 => "UInt16",
        7 => "UInt32",
        8 => "UInt64",
        9 => "Float",
        10 => "Double",
        11 => "Boolean",
        12 => "String",
        13 => "DateTime",
        14 => "Text",
        15 => "UUID",
        16 => "DataSet",
        17 => "Bytes",
        18 => "File",
        19 => "Template",
        20 => "PropertySet",
        21 => "PropertySetList",
        22 => "Int8Array",
        23 => "Int16Array",
        24 => "Int32Array",
        25 => "Int64Array",
        26 => "UInt8Array",
        27 => "UInt16Array",
        28 => "UInt32Array",
        29 => "UInt64Array",
        30 => "FloatArray",
        31 => "DoubleArray",
        32 => "BooleanArray",
        33 => "StringArray",
        34 => "DateTimeArray",
        _ => "Unknown",
    }
}

/// Parsed parts of a Sparkplug B topic `spBv1.0/group/TYPE/edge[/device]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpTopic<'a> {
    pub group: &'a str,
    pub message_type: &'a str,
    pub edge: &'a str,
    pub device: Option<&'a str>,
}

impl SpTopic<'_> {
    /// Key of the edge node, scope of the alias map.
    pub fn edge_key(&self) -> String {
        format!("{}/{}", self.group, self.edge)
    }
    /// True for NBIRTH and DBIRTH.
    pub fn is_birth(&self) -> bool {
        self.message_type == "NBIRTH" || self.message_type == "DBIRTH"
    }
}

/// Parses a Sparkplug B topic carrying a protobuf payload (not STATE).
pub fn parse_topic(topic: &str) -> Option<SpTopic<'_>> {
    let rest = topic.strip_prefix("spBv1.0/")?;
    let mut it = rest.split('/');
    let group = it.next()?;
    let message_type = it.next()?;
    let edge = it.next()?;
    let device = it.next();
    if it.next().is_some() {
        return None;
    }
    matches!(
        message_type,
        "NBIRTH" | "NDEATH" | "DBIRTH" | "DDEATH" | "NDATA" | "DDATA" | "NCMD" | "DCMD"
    )
    .then_some(SpTopic {
        group,
        message_type,
        edge,
        device,
    })
}

/// Alias map of one edge node, established by NBIRTH and DBIRTH.
#[derive(Debug, Clone, Default)]
pub struct AliasMap {
    /// alias -> (name, datatype)
    pub by_alias: HashMap<u64, (String, u32)>,
    /// name -> datatype
    pub datatype_by_name: HashMap<String, u32>,
}

impl AliasMap {
    /// Records the names, aliases and data types of a birth payload.
    pub fn learn(&mut self, p: &Payload) {
        for m in &p.metrics {
            if let (Some(name), Some(dt)) = (&m.name, m.datatype) {
                self.datatype_by_name.insert(name.clone(), dt);
            }
            if let (Some(name), Some(alias)) = (&m.name, m.alias) {
                self.by_alias
                    .insert(alias, (name.clone(), m.datatype.unwrap_or(0)));
            }
        }
    }
}

/// Decodes the protobuf bytes.
pub fn decode_raw(bytes: &[u8]) -> Result<Payload, String> {
    <Payload as prost::Message>::decode(bytes).map_err(|e| {
        let s = e.to_string();
        s.strip_prefix("failed to decode Protobuf message: ")
            .unwrap_or(&s)
            .to_string()
    })
}

/// Converts a payload to JSON, resolving aliases with `aliases`.
pub fn to_json(p: &Payload, aliases: Option<&AliasMap>) -> Value {
    let mut out = Map::new();
    out.insert("timestamp".into(), opt_u64(p.timestamp));
    out.insert("seq".into(), opt_u64(p.seq));
    if let Some(u) = &p.uuid {
        out.insert("uuid".into(), Value::String(u.clone()));
    }
    if let Some(b) = &p.body {
        out.insert(
            "body".into(),
            Value::String(base64::engine::general_purpose::STANDARD.encode(b)),
        );
    }
    out.insert(
        "metrics".into(),
        Value::Array(p.metrics.iter().map(|m| metric_json(m, aliases)).collect()),
    );
    Value::Object(out)
}

/// Name, data type and value of each metric, aliases resolved.
pub fn named_values(p: &Payload, aliases: Option<&AliasMap>) -> Vec<(String, Value)> {
    p.metrics
        .iter()
        .map(|m| {
            let (name, dt) = resolve(m, aliases);
            let name = name.unwrap_or_else(|| format!("#{}", m.alias.unwrap_or(0)));
            (name, metric_value(m, dt))
        })
        .collect()
}

fn opt_u64(v: Option<u64>) -> Value {
    v.map(Value::from).unwrap_or(Value::Null)
}

fn resolve(m: &Metric, aliases: Option<&AliasMap>) -> (Option<String>, u32) {
    let mut name = m.name.clone();
    let mut dt = m.datatype.unwrap_or(0);
    if let Some(map) = aliases {
        if name.is_none() {
            if let Some((n, d)) = m.alias.and_then(|a| map.by_alias.get(&a)) {
                name = Some(n.clone());
                if dt == 0 {
                    dt = *d;
                }
            }
        }
        if dt == 0 {
            if let Some(d) = name.as_ref().and_then(|n| map.datatype_by_name.get(n)) {
                dt = *d;
            }
        }
    }
    (name, dt)
}

fn metric_json(m: &Metric, aliases: Option<&AliasMap>) -> Value {
    let (name, dt) = resolve(m, aliases);
    let mut o = Map::new();
    o.insert(
        "name".into(),
        name.map(Value::String).unwrap_or(Value::Null),
    );
    o.insert("alias".into(), opt_u64(m.alias));
    o.insert("timestamp".into(), opt_u64(m.timestamp));
    o.insert(
        "datatype".into(),
        if dt == 0 {
            Value::Null
        } else {
            Value::String(datatype_name(dt).to_string())
        },
    );
    o.insert("value".into(), metric_value(m, dt));
    if m.is_historical == Some(true) {
        o.insert("isHistorical".into(), Value::Bool(true));
    }
    if m.is_transient == Some(true) {
        o.insert("isTransient".into(), Value::Bool(true));
    }
    if let Some(ps) = &m.properties {
        o.insert("properties".into(), propertyset_json(ps));
    }
    Value::Object(o)
}

fn float(f: f64) -> Value {
    serde_json::Number::from_f64(f)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn int_by_type(v: u32, dt: u32) -> Value {
    match dt {
        1 => Value::from(v as i8),
        2 => Value::from(v as i16),
        3 => Value::from(v as i32),
        _ => Value::from(v),
    }
}

fn long_by_type(v: u64, dt: u32) -> Value {
    match dt {
        1..=4 => Value::from(v as i64),
        _ => Value::from(v),
    }
}

fn metric_value(m: &Metric, dt: u32) -> Value {
    if m.is_null == Some(true) {
        return Value::Null;
    }
    match &m.value {
        None => Value::Null,
        Some(MetricValue::IntValue(v)) => int_by_type(*v, dt),
        Some(MetricValue::LongValue(v)) => long_by_type(*v, dt),
        Some(MetricValue::FloatValue(v)) => float(f64::from(*v)),
        Some(MetricValue::DoubleValue(v)) => float(*v),
        Some(MetricValue::BooleanValue(v)) => Value::Bool(*v),
        Some(MetricValue::StringValue(v)) => Value::String(v.clone()),
        Some(MetricValue::BytesValue(v)) => {
            Value::String(base64::engine::general_purpose::STANDARD.encode(v))
        }
        Some(MetricValue::DatasetValue(ds)) => dataset_json(ds),
        Some(MetricValue::TemplateValue(t)) => template_json(t),
    }
}

fn dataset_json(ds: &DataSet) -> Value {
    let rows: Vec<Value> = ds
        .rows
        .iter()
        .map(|r| {
            Value::Array(
                r.elements
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let dt = ds.types.get(i).copied().unwrap_or(0);
                        match &e.value {
                            None => Value::Null,
                            Some(DataSetValueKind::IntValue(v)) => int_by_type(*v, dt),
                            Some(DataSetValueKind::LongValue(v)) => long_by_type(*v, dt),
                            Some(DataSetValueKind::FloatValue(v)) => float(f64::from(*v)),
                            Some(DataSetValueKind::DoubleValue(v)) => float(*v),
                            Some(DataSetValueKind::BooleanValue(v)) => Value::Bool(*v),
                            Some(DataSetValueKind::StringValue(v)) => Value::String(v.clone()),
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    json!({
        "columns": ds.columns,
        "types": ds.types.iter().map(|t| datatype_name(*t)).collect::<Vec<_>>(),
        "rows": rows,
    })
}

fn template_json(t: &Template) -> Value {
    let params: Vec<Value> = t
        .parameters
        .iter()
        .map(|p| {
            let dt = p.r#type.unwrap_or(0);
            let v = match &p.value {
                None => Value::Null,
                Some(ParameterValue::IntValue(v)) => int_by_type(*v, dt),
                Some(ParameterValue::LongValue(v)) => long_by_type(*v, dt),
                Some(ParameterValue::FloatValue(v)) => float(f64::from(*v)),
                Some(ParameterValue::DoubleValue(v)) => float(*v),
                Some(ParameterValue::BooleanValue(v)) => Value::Bool(*v),
                Some(ParameterValue::StringValue(v)) => Value::String(v.clone()),
            };
            json!({ "name": p.name, "type": datatype_name(dt), "value": v })
        })
        .collect();
    json!({
        "version": t.version,
        "templateRef": t.template_ref,
        "isDefinition": t.is_definition.unwrap_or(false),
        "parameters": params,
        "metrics": t.metrics.iter().map(|m| metric_json(m, None)).collect::<Vec<_>>(),
    })
}

fn propertyset_json(ps: &PropertySet) -> Value {
    let mut o = Map::new();
    for (k, v) in ps.keys.iter().zip(ps.values.iter()) {
        let dt = v.r#type.unwrap_or(0);
        let val = if v.is_null == Some(true) {
            Value::Null
        } else {
            match &v.value {
                None => Value::Null,
                Some(PropertyValueKind::IntValue(x)) => int_by_type(*x, dt),
                Some(PropertyValueKind::LongValue(x)) => long_by_type(*x, dt),
                Some(PropertyValueKind::FloatValue(x)) => float(f64::from(*x)),
                Some(PropertyValueKind::DoubleValue(x)) => float(*x),
                Some(PropertyValueKind::BooleanValue(x)) => Value::Bool(*x),
                Some(PropertyValueKind::StringValue(x)) => Value::String(x.clone()),
                Some(PropertyValueKind::PropertysetValue(p)) => propertyset_json(p),
                Some(PropertyValueKind::PropertysetsValue(l)) => {
                    Value::Array(l.propertyset.iter().map(propertyset_json).collect())
                }
            }
        };
        o.insert(k.clone(), val);
    }
    Value::Object(o)
}
