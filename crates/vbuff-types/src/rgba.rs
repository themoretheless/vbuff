//! The `image/x-vbuff-rgba` MIME convention for raw RGBA image payloads.
//!
//! `arboard` hands callers raw RGBA8 pixels plus width/height, not an encoded
//! PNG, so vbuff tags that raw payload with a vbuff-specific MIME string that
//! records the dimensions inline: `image/x-vbuff-rgba;width=W;height=H`. Both
//! the platform clipboard backend (writer) and the GUI (thumbnail reader)
//! need to build/parse this same string, so the format and its helpers live
//! here once instead of being duplicated in each crate.

/// MIME prefix used for raw RGBA image payloads.
pub const RGBA_MIME_PREFIX: &str = "image/x-vbuff-rgba";

/// Build the full `image/x-vbuff-rgba;width=W;height=H` MIME string.
pub fn rgba_mime(width: usize, height: usize) -> String {
    format!("{RGBA_MIME_PREFIX};width={width};height={height}")
}

/// Parse `width=W;height=H` out of an RGBA MIME string.
pub fn parse_rgba_dims(mime: &str) -> Option<(usize, usize)> {
    let mut width = None;
    let mut height = None;
    for part in mime.split(';') {
        if let Some(v) = part.trim().strip_prefix("width=") {
            width = v.parse().ok();
        } else if let Some(v) = part.trim().strip_prefix("height=") {
            height = v.parse().ok();
        }
    }
    Some((width?, height?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgba_dims() {
        let mime = "image/x-vbuff-rgba;width=4;height=2";
        assert_eq!(parse_rgba_dims(mime), Some((4, 2)));
    }

    #[test]
    fn missing_dims_is_none() {
        assert_eq!(parse_rgba_dims("image/x-vbuff-rgba"), None);
    }

    #[test]
    fn builds_expected_mime_string() {
        assert_eq!(rgba_mime(4, 2), "image/x-vbuff-rgba;width=4;height=2");
    }
}
