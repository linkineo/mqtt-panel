//! Image detection by magic bytes or content type.

use base64::Engine as _;

/// MIME type from magic bytes (PNG, JPEG, GIF, WebP).
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// An image content type (`image/*`), lower-cased, without parameters.
pub fn image_content_type(content_type: Option<&str>) -> Option<String> {
    let ct = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    ct.starts_with("image/").then_some(ct)
}

/// A `data:` URL for the image, or `None` when the bytes are not an image.
pub fn data_url(bytes: &[u8], content_type: Option<&str>) -> Option<String> {
    let mime = match sniff(bytes) {
        Some(m) => m.to_string(),
        None => image_content_type(content_type)?,
    };
    if bytes.is_empty() {
        return None;
    }
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
