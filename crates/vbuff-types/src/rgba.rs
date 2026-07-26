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
///
/// Returns the raw dimensions without validating them: zero and overflow-prone
/// values come back as-is. Use [`parse_rgba_dims_checked`] when the dimensions
/// will gate a buffer allocation or decode.
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

/// Byte length a raw RGBA8 payload with the given dimensions must have.
///
/// Returns `None` for zero dimensions or when `width * height * 4` overflows
/// `usize`, so fail-closed callers can treat `None` as "reject this payload".
pub fn rgba_required_len(width: usize, height: usize) -> Option<usize> {
    if width == 0 || height == 0 {
        return None;
    }
    width.checked_mul(height)?.checked_mul(4)
}

/// Parse an RGBA MIME string into `(width, height, required_len)`.
///
/// Unlike [`parse_rgba_dims`], returns `None` when the dimensions are missing,
/// zero, or imply a payload length that overflows `usize`.
pub fn parse_rgba_dims_checked(mime: &str) -> Option<(usize, usize, usize)> {
    let (width, height) = parse_rgba_dims(mime)?;
    let required = rgba_required_len(width, height)?;
    Some((width, height, required))
}

/// True when the MIME essence (everything before the first `;` parameter) is
/// the vbuff RGBA prefix, compared case-insensitively.
pub fn is_rgba_mime(mime: &str) -> bool {
    mime.split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case(RGBA_MIME_PREFIX))
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

    #[test]
    fn checked_parse_rejects_overflow_dims() {
        let mime = "image/x-vbuff-rgba;width=18446744073709551615;height=2";
        assert_eq!(parse_rgba_dims_checked(mime), None);
    }

    #[test]
    fn checked_parse_rejects_zero_dims() {
        assert_eq!(
            parse_rgba_dims_checked("image/x-vbuff-rgba;width=0;height=2"),
            None
        );
        assert_eq!(
            parse_rgba_dims_checked("image/x-vbuff-rgba;width=2;height=0"),
            None
        );
    }

    #[test]
    fn checked_parse_returns_dims_with_required_len() {
        assert_eq!(
            parse_rgba_dims_checked("image/x-vbuff-rgba;width=4;height=2"),
            Some((4, 2, 4 * 2 * 4))
        );
    }

    #[test]
    fn is_rgba_mime_ignores_case_and_parameters() {
        assert!(is_rgba_mime("IMAGE/X-VBUFF-RGBA;width=1;height=1"));
        assert!(is_rgba_mime(" image/x-vbuff-rgba ;width=1;height=1"));
        assert!(!is_rgba_mime("image/png"));
        assert!(!is_rgba_mime("text/plain"));
    }

    #[test]
    fn required_len_rejects_overflow_and_zero() {
        assert_eq!(rgba_required_len(usize::MAX, 2), None);
        assert_eq!(rgba_required_len(0, 2), None);
        assert_eq!(rgba_required_len(2, 0), None);
        assert_eq!(rgba_required_len(4, 2), Some(32));
    }
}
