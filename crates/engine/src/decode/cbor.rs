//! CBOR to JSON.

use base64::Engine as _;
use ciborium::Value as Cbor;
use serde_json::Value;

/// Decodes CBOR bytes to a JSON value (bytes as base64, non-string keys stringified).
pub fn decode(bytes: &[u8]) -> Result<Value, String> {
    let v: Cbor = ciborium::de::from_reader(bytes).map_err(|e| e.to_string())?;
    Ok(to_json(v, 0))
}

fn to_json(v: Cbor, depth: usize) -> Value {
    if depth > 128 {
        return Value::Null;
    }
    match v {
        Cbor::Integer(i) => {
            let n = i128::from(i);
            if let Ok(x) = i64::try_from(n) {
                Value::from(x)
            } else if let Ok(x) = u64::try_from(n) {
                Value::from(x)
            } else {
                Value::from(n as f64)
            }
        }
        Cbor::Bytes(b) => Value::String(base64::engine::general_purpose::STANDARD.encode(b)),
        Cbor::Float(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Cbor::Text(s) => Value::String(s),
        Cbor::Bool(b) => Value::Bool(b),
        Cbor::Null => Value::Null,
        Cbor::Tag(_, inner) => to_json(*inner, depth + 1),
        Cbor::Array(a) => Value::Array(a.into_iter().map(|x| to_json(x, depth + 1)).collect()),
        Cbor::Map(m) => {
            let mut out = serde_json::Map::with_capacity(m.len());
            for (k, val) in m {
                let key = match k {
                    Cbor::Text(s) => s,
                    other => {
                        let j = to_json(other, depth + 1);
                        match j {
                            Value::String(s) => s,
                            o => o.to_string(),
                        }
                    }
                };
                out.insert(key, to_json(val, depth + 1));
            }
            Value::Object(out)
        }
        _ => Value::Null,
    }
}
