//! Protobuf with user `.proto` files compiled at runtime (protox + prost-reflect).

use crate::message::UserMessage;
use crate::search::topic_matches;
use prost_reflect::{DescriptorPool, DynamicMessage, Kind, MessageDescriptor, SerializeOptions};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use serde_json::Value;
use std::collections::HashMap;

/// A `.proto` source file. Imports are resolved by name among the files given together.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoFile {
    /// Import name, e.g. `plant/telemetry.proto`.
    pub name: String,
    /// Source text.
    pub content: String,
}

/// Assignment of a message type to a topic filter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoAssignment {
    pub file_name: String,
    pub topic_filter: String,
    /// Fully qualified, e.g. `plant.Telemetry`.
    pub message_type: String,
}

struct MemoryResolver {
    files: HashMap<String, String>,
}

impl FileResolver for MemoryResolver {
    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        match self.files.get(name) {
            Some(src) => File::from_source(name, src),
            None => {
                // Allow imports written relative to a sub folder ("common.proto" next to the file).
                let found = self
                    .files
                    .iter()
                    .find(|(k, _)| k.rsplit('/').next() == Some(name));
                match found {
                    Some((_, src)) => File::from_source(name, src),
                    None => Err(protox::Error::file_not_found(name)),
                }
            }
        }
    }
}

fn not_proto(detail: impl std::fmt::Display) -> UserMessage {
    UserMessage::new("file.notProto", vec![("detail", detail.to_string().into())])
}

fn compile(files: &[ProtoFile]) -> Result<DescriptorPool, UserMessage> {
    let resolver = MemoryResolver {
        files: files
            .iter()
            .map(|f| (f.name.clone(), f.content.clone()))
            .collect(),
    };
    let mut chain = ChainFileResolver::new();
    chain.add(resolver);
    chain.add(GoogleFileResolver::new());
    let mut compiler = protox::Compiler::with_file_resolver(chain);
    compiler.include_imports(true);
    for f in files {
        compiler.open_file(&f.name).map_err(not_proto)?;
    }
    Ok(compiler.descriptor_pool())
}

/// Lists the message types (fully qualified) declared in the files, for the type picker.
pub fn proto_message_types(files: &[ProtoFile]) -> Result<Vec<String>, UserMessage> {
    let pool = compile(files)?;
    let mut names: Vec<String> = pool
        .all_messages()
        .map(|m| m.full_name().to_string())
        .filter(|n| !n.starts_with("google.protobuf."))
        .collect();
    names.sort();
    Ok(names)
}

/// Compiled schemas plus their topic assignments.
#[derive(Debug, Clone)]
pub struct ProtoRegistry {
    pool: DescriptorPool,
    assignments: Vec<ProtoAssignment>,
}

impl ProtoRegistry {
    /// Compiles the files and checks that every assigned message type exists.
    pub fn new(
        files: &[ProtoFile],
        assignments: Vec<ProtoAssignment>,
    ) -> Result<Self, UserMessage> {
        let pool = compile(files)?;
        for a in &assignments {
            if pool.get_message_by_name(&a.message_type).is_none() {
                return Err(not_proto(format!(
                    "message type {} not found in {}",
                    a.message_type, a.file_name
                )));
            }
        }
        Ok(ProtoRegistry { pool, assignments })
    }

    /// Message type assigned to a topic (first matching filter).
    pub fn type_for(&self, topic: &str) -> Option<&str> {
        self.assignments
            .iter()
            .find(|a| topic_matches(&a.topic_filter, topic))
            .map(|a| a.message_type.as_str())
    }

    /// Approximate heap size, for the memory estimate.
    pub fn approx_bytes(&self) -> usize {
        16 * 1024
    }

    /// Decodes bytes as `message_type` to JSON.
    pub fn decode(&self, message_type: &str, bytes: &[u8]) -> Result<Value, UserMessage> {
        let err = |detail: String| {
            UserMessage::new(
                "decode.protobuf",
                vec![("type", message_type.into()), ("detail", detail.into())],
            )
        };
        let desc = self
            .pool
            .get_message_by_name(message_type)
            .ok_or_else(|| err("unknown message type".to_string()))?;
        if let Some(detail) = check_wire(&desc, bytes) {
            return Err(err(detail));
        }
        let msg = DynamicMessage::decode(desc, bytes).map_err(|e| {
            let s = e.to_string();
            err(s
                .strip_prefix("failed to decode Protobuf message: ")
                .unwrap_or(&s)
                .to_string())
        })?;
        let opts = SerializeOptions::new()
            .stringify_64_bit_integers(false)
            .use_proto_field_name(true)
            .skip_default_fields(false);
        msg.serialize_with_options(serde_json::value::Serializer, &opts)
            .map_err(|e| err(e.to_string()))
    }
}

/// Decodes with an explicit registry, for callers outside the engine.
pub fn decode_with(
    registry: &ProtoRegistry,
    message_type: &str,
    bytes: &[u8],
) -> Result<Value, UserMessage> {
    registry.decode(message_type, bytes)
}

fn read_varint(b: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let c = *b.get(*pos)?;
        *pos += 1;
        v |= u64::from(c & 0x7f) << shift;
        if c & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn wire_name(wt: u64) -> &'static str {
    match wt {
        0 => "varint",
        1 => "64-bit",
        2 => "length-delimited",
        3 => "start group",
        4 => "end group",
        5 => "32-bit",
        _ => "invalid",
    }
}

/// Top-level wire check that names the field whose wire type does not match the schema.
fn check_wire(desc: &MessageDescriptor, b: &[u8]) -> Option<String> {
    let mut pos = 0usize;
    while pos < b.len() {
        let start = pos;
        let tag = read_varint(b, &mut pos)?;
        let field = tag >> 3;
        let wt = tag & 7;
        if field == 0 {
            return Some(format!("invalid field number 0 at byte {start}"));
        }
        if let Some(fd) = u32::try_from(field).ok().and_then(|f| desc.get_field(f)) {
            let expected = match fd.kind() {
                Kind::Double | Kind::Fixed64 | Kind::Sfixed64 => 1,
                Kind::Float | Kind::Fixed32 | Kind::Sfixed32 => 5,
                Kind::String | Kind::Bytes | Kind::Message(_) => 2,
                _ => 0,
            };
            let packed_ok = fd.is_list() && wt == 2 && expected != 2;
            let group_ok = matches!(fd.kind(), Kind::Message(_)) && wt == 3;
            if wt != expected && !packed_ok && !group_ok {
                return Some(format!("field {field} wire type mismatch"));
            }
        }
        match wt {
            0 => {
                read_varint(b, &mut pos)?;
            }
            1 => pos += 8,
            5 => pos += 4,
            2 => {
                let len = read_varint(b, &mut pos)?;
                pos = pos.saturating_add(usize::try_from(len).ok()?);
            }
            3 | 4 => return None,
            other => {
                return Some(format!(
                    "invalid wire type {other} ({}) at byte {start}",
                    wire_name(other)
                ))
            }
        }
        if pos > b.len() {
            return Some(format!("truncated payload (field {field})"));
        }
    }
    None
}
