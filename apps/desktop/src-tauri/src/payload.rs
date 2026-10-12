//! Payload encoding for publish (JSON, text, hex, base64), with plain-word validation errors (P-08).

use base64::Engine as _;
use mqttpanel_client::{msg, UserMessage};

pub fn encode(format: &str, text: &str) -> Result<Vec<u8>, UserMessage> {
    match format {
        "json" => {
            if text.trim().is_empty() {
                return Ok(Vec::new());
            }
            serde_json::from_str::<serde_json::Value>(text)
                .map(|_| text.as_bytes().to_vec())
                .map_err(|e| msg!("payload.invalidJson", line = e.line(), column = e.column(), detail = json_detail(&e)))
        }
        "hex" => decode_hex(text),
        "base64" => {
            let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            let engines = [
                base64::engine::general_purpose::STANDARD,
                base64::engine::general_purpose::STANDARD_NO_PAD,
                base64::engine::general_purpose::URL_SAFE,
                base64::engine::general_purpose::URL_SAFE_NO_PAD,
            ];
            for e in engines {
                if let Ok(b) = e.decode(clean.as_bytes()) {
                    return Ok(b);
                }
            }
            Err(msg!("payload.invalidBase64", detail = "use A-Z, a-z, 0-9, + and /, optionally = at the end"))
        }
        _ => Ok(text.as_bytes().to_vec()),
    }
}

fn json_detail(e: &serde_json::Error) -> String {
    let s = e.to_string();
    // "expected value at line 1 column 5" -> "expected value"
    match s.find(" at line ") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

fn decode_hex(text: &str) -> Result<Vec<u8>, UserMessage> {
    let mut digits = String::new();
    let mut positions = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' || c == ':' {
            i += 1;
            continue;
        }
        if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
            i += 2;
            continue;
        }
        if !c.is_ascii_hexdigit() {
            return Err(msg!("payload.invalidHex", position = i + 1));
        }
        digits.push(c);
        positions.push(i + 1);
        i += 1;
    }
    if digits.len() % 2 != 0 {
        return Err(msg!("payload.invalidHex", position = positions.last().copied().unwrap_or(1)));
    }
    hex_pairs(&digits).ok_or_else(|| msg!("payload.invalidHex", position = 1))
}

fn hex_pairs(d: &str) -> Option<Vec<u8>> {
    (0..d.len()).step_by(2).map(|i| u8::from_str_radix(&d[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::encode;
    #[test]
    fn p08_formats() {
        assert_eq!(encode("hex", "0x01 0x02 ff").unwrap(), vec![1, 2, 255]);
        assert_eq!(encode("hex", "01 02 0").unwrap_err().key, "payload.invalidHex");
        assert_eq!(encode("hex", "zz").unwrap_err().text, "Invalid hex at position 1. Use pairs of 0-9 and A-F; spaces and 0x are allowed.");
        assert_eq!(encode("base64", "aGVsbG8=").unwrap(), b"hello");
        assert_eq!(encode("base64", "aGVsbG8").unwrap(), b"hello");
        let e = encode("json", "{\n  \"a\": ,\n}").unwrap_err();
        assert_eq!(e.key, "payload.invalidJson");
        assert!(e.text.starts_with("Invalid JSON at line 2, column"));
        assert_eq!(encode("json", "{\"cmd\":\"restart\",\"delay\":5}").unwrap().len(), 27);
    }
}
