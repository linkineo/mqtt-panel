//! Casting rules: how a JSON value becomes a plottable number (TESTPLAN D-01..D-10).

use crate::types::FieldCast;
use serde_json::Value;

/// Rule a value was cast with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A JSON number, used as is.
    Number,
    /// A string holding a decimal or scientific number.
    ParsedNumber,
    /// A JSON boolean or boolean-like text, as 0 or 1.
    Boolean,
}

/// Result of casting one value.
#[derive(Debug, Clone, PartialEq)]
pub enum Cast {
    /// A plottable value.
    Value {
        value: f64,
        rule: Rule,
        /// True for integers beyond 2^53 that cannot be represented exactly.
        precision_loss: bool,
    },
    /// Not plottable; the reason is shown in the series stats.
    Skip(String),
}

impl Cast {
    /// The value when plottable.
    pub fn value(&self) -> Option<f64> {
        match self {
            Cast::Value { value, .. } => Some(*value),
            Cast::Skip(_) => None,
        }
    }
}

const MAX_EXACT: f64 = 9_007_199_254_740_992.0; // 2^53

/// Casts a JSON value with the series rules.
pub fn cast_value(v: &Value) -> Cast {
    match v {
        Value::Number(n) => {
            let (value, precision_loss) = if let Some(i) = n.as_i64() {
                (i as f64, i.unsigned_abs() > MAX_EXACT as u64)
            } else if let Some(u) = n.as_u64() {
                (u as f64, u > MAX_EXACT as u64)
            } else {
                (n.as_f64().unwrap_or(f64::NAN), false)
            };
            if !value.is_finite() {
                return Cast::Skip("number out of range".into());
            }
            Cast::Value {
                value: if value == 0.0 { 0.0 } else { value },
                rule: Rule::Number,
                precision_loss,
            }
        }
        Value::Bool(b) => Cast::Value {
            value: if *b { 1.0 } else { 0.0 },
            rule: Rule::Boolean,
            precision_loss: false,
        },
        Value::String(s) => cast_str(s),
        Value::Null => Cast::Skip("null".into()),
        Value::Object(_) => Cast::Skip("object".into()),
        Value::Array(_) => Cast::Skip("array".into()),
    }
}

/// Casts a text value: boolean-like words first, then decimal or scientific numbers.
pub fn cast_str(s: &str) -> Cast {
    let t = s.trim();
    if let Some(b) = boolean_like(t) {
        return Cast::Value {
            value: if b { 1.0 } else { 0.0 },
            rule: Rule::Boolean,
            precision_loss: false,
        };
    }
    if is_decimal(t) {
        if let Ok(v) = t.parse::<f64>() {
            if v.is_finite() {
                return Cast::Value {
                    value: if v == 0.0 { 0.0 } else { v },
                    rule: Rule::ParsedNumber,
                    precision_loss: false,
                };
            }
        }
    }
    let shown: String = s.chars().take(24).collect();
    Cast::Skip(format!("text \"{shown}\" is not a number"))
}

/// Boolean-like text, case-insensitive: true/false, t/f, on/off, yes/no, 1/0.
pub fn boolean_like(t: &str) -> Option<bool> {
    if t.len() > 5 {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    match lower.as_str() {
        "true" | "t" | "on" | "yes" | "1" => Some(true),
        "false" | "f" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// `[+-]? (digits [. digits?] | . digits) ([eE] [+-]? digits)?`
fn is_decimal(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - f;
    }
    if int_digits == 0 && frac_digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return false;
        }
    }
    i == b.len()
}

/// Classifies a value for the field picker.
pub fn field_cast(v: &Value) -> FieldCast {
    match v {
        Value::Number(_) => FieldCast::Number,
        Value::Bool(_) => FieldCast::Boolean,
        Value::Null => FieldCast::Null,
        Value::Object(_) => FieldCast::Object,
        Value::Array(_) => FieldCast::Array,
        Value::String(s) => match cast_str(s) {
            Cast::Value {
                rule: Rule::Boolean,
                ..
            } => FieldCast::Boolean,
            Cast::Value { .. } => FieldCast::ParsedNumber,
            Cast::Skip(_) => FieldCast::Text,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(x: Value) -> Option<f64> {
        cast_value(&x).value()
    }

    #[test]
    fn numbers_and_strings() {
        assert_eq!(v(json!(21.5)), Some(21.5));
        assert_eq!(v(json!(" 21.5 ")), Some(21.5));
        assert_eq!(v(json!("1e3")), Some(1000.0));
        assert_eq!(v(json!(".5")), Some(0.5));
        assert_eq!(v(json!("-3.")), Some(-3.0));
        for bad in [
            "21,5", "0x1A", "21.5°C", "NaN", "Infinity", "-inf", "", "e3", "1e", "1e999",
        ] {
            assert_eq!(v(json!(bad)), None, "{bad}");
        }
        assert!(v(json!(-0.0)).unwrap().is_sign_positive());
    }

    #[test]
    fn booleans() {
        assert_eq!(v(json!(true)), Some(1.0));
        assert_eq!(v(json!("Off")), Some(0.0));
        assert!(matches!(
            cast_value(&json!("1")),
            Cast::Value {
                rule: Rule::Boolean,
                ..
            }
        ));
        assert_eq!(field_cast(&json!("yes")), FieldCast::Boolean);
        assert_eq!(field_cast(&json!("auto")), FieldCast::Text);
        assert_eq!(field_cast(&json!("2.5")), FieldCast::ParsedNumber);
    }

    #[test]
    fn precision() {
        let big: Value = serde_json::from_str("9007199254740993").unwrap();
        assert!(matches!(
            cast_value(&big),
            Cast::Value {
                precision_loss: true,
                ..
            }
        ));
        let ok: Value = serde_json::from_str("4294967296").unwrap();
        assert!(matches!(
            cast_value(&ok),
            Cast::Value {
                precision_loss: false,
                ..
            }
        ));
    }
}
