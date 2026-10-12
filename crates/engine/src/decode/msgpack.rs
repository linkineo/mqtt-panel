//! MessagePack to JSON.

use base64::Engine as _;
use rmpv::Value as Mp;
use serde_json::Value;

/// Decodes MessagePack bytes to a JSON value (binary as base64, non-string keys stringified).
pub fn decode(bytes: &[u8]) -> Result<Value, String> {
    let mut rd = bytes;
    let v = rmpv::decode::read_value_with_max_depth(&mut rd, 256).map_err(|e| e.to_string())?;
    if !rd.is_empty() {
        return Err(format!("{} unexpected bytes after the value", rd.len()));
    }
    Ok(to_json(v))
}

fn to_json(v: Mp) -> Value {
    match v {
        Mp::Nil => Value::Null,
        Mp::Boolean(b) => Value::Bool(b),
        Mp::Integer(i) => {
            if let Some(x) = i.as_i64() {
                Value::from(x)
            } else if let Some(x) = i.as_u64() {
                Value::from(x)
            } else {
                Value::from(i.as_f64().unwrap_or(0.0))
            }
        }
        Mp::F32(f) => serde_json::Number::from_f64(f64::from(f))
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Mp::F64(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Mp::String(s) => match s.into_str() {
            Some(s) => Value::String(s),
            None => Value::Null,
        },
        Mp::Binary(b) => Value::String(base64::engine::general_purpose::STANDARD.encode(b)),
        Mp::Array(a) => Value::Array(a.into_iter().map(to_json).collect()),
        Mp::Map(m) => {
            let mut out = serde_json::Map::with_capacity(m.len());
            for (k, val) in m {
                let key = match to_json(k) {
                    Value::String(s) => s,
                    o => o.to_string(),
                };
                out.insert(key, to_json(val));
            }
            Value::Object(out)
        }
        Mp::Ext(t, data) => serde_json::json!({
            "ext": t,
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
    }
}
