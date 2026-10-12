//! JSON and text detection.

use serde_json::Value;

/// Strips a UTF-8 byte order mark.
pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// Parses JSON (BOM stripped; comments are invalid JSON).
pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(strip_bom(bytes)).map_err(|e| e.to_string())
}

/// Cheap guess whether bytes may be JSON, used before parsing.
pub fn looks_like_json(bytes: &[u8]) -> bool {
    let b = strip_bom(bytes);
    match b.iter().find(|c| !c.is_ascii_whitespace()) {
        Some(c) => matches!(
            c,
            b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
        ),
        None => false,
    }
}

/// Valid UTF-8 without control characters other than tab, CR and LF.
pub fn as_text(bytes: &[u8]) -> Option<&str> {
    let s = std::str::from_utf8(strip_bom(bytes)).ok()?;
    if s.bytes()
        .any(|c| (c < 0x20 && c != b'\t' && c != b'\r' && c != b'\n') || c == 0x7f)
    {
        return None;
    }
    Some(s)
}
