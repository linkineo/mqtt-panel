//! mqttpanel-engine: the pure engine of MQTT Panel.
//!
//! One [`Engine`] per MQTT connection. It holds the topic tree, per-topic ring buffers, series
//! with casting rules, decoders, diff, search and export. It performs no I/O, starts no
//! threads and reads no clock: callers pass receive times and `now_ms` in milliseconds since
//! the Unix epoch. View types serialize exactly like `packages/engine-client/src/types.ts`.

pub mod buffer;
pub mod cast;
pub mod decode;
pub mod diff;
mod engine;
pub mod export;
pub mod ffi;
mod inspect;
pub mod message;
pub mod path;
pub mod search;
pub mod series;
pub mod store;
mod table;
mod tree;
pub mod types;

pub use cast::{cast_value, Cast};
pub use decode::protobuf::proto_message_types;
pub use decode::{
    decode_payload, detect, DecodeOptions, ProtoAssignment, ProtoFile, ProtoRegistry,
};
pub use engine::{Engine, EngineConfig, IncomingMessage};
pub use message::UserMessage;
pub use search::topic_matches;
pub use types::*;
