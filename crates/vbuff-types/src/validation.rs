//! Shared validation predicates for identifiers, labels, key ids, versions,
//! and byte buffers.
//!
//! This module is the single canonical home for the small validation
//! predicates that were historically copy-pasted across vbuff crates
//! (`vbuff-core`, `vbuff-ipc`, `vbuff-sync`, `vbuff-store`, `vbuff-plugin`,
//! `vbuff-update`). Every function here is pure, allocation-free, and has no
//! dependencies beyond `core`/`std`.
//!
//! # Canonical identifier grammar
//!
//! An *identifier* is a non-empty ASCII string of at most `max_len` **bytes**
//! whose bytes are all in `[A-Za-z0-9._-]`. Because the allowed set is pure
//! ASCII, any string containing a non-ASCII character is rejected by
//! construction. Length is measured with [`str::len`] (bytes, not chars),
//! matching every historical copy.
//!
//! # Canonical label grammar
//!
//! A *label* is a non-empty UTF-8 string of at most `max_len` **bytes** that
//! contains no control characters (per [`char::is_control`]). Unlike
//! identifiers, labels may contain arbitrary non-control Unicode, including
//! spaces. Some call sites additionally require the label to be non-blank
//! after trimming; use [`is_valid_trimmed_label`] for those.

/// Default byte budget for identifiers.
///
/// 128 is by far the most common cap across the historical copies
/// (vbuff-ipc request/clip/device ids, vbuff-sync device/vault/backend ids,
/// vbuff-store embedding backend ids, vbuff-plugin command/pipeline ids,
/// vbuff-core detector ids).
pub const DEFAULT_MAX_IDENTIFIER_BYTES: usize = 128;

/// Byte budget for update-manifest / attestation signing key ids.
///
/// Both historical copies in `vbuff-update` (`MAX_KEY_ID_BYTES` in
/// `attestation.rs` and `MAX_KEY_ID_LEN` in `manifest.rs`) used 96.
pub const MAX_KEY_ID_BYTES: usize = 96;

/// Returns `true` when `value` is a canonical identifier.
///
/// Contract:
/// - non-empty;
/// - `value.len() <= max_len` (bytes, not chars);
/// - every byte is ASCII alphanumeric or one of `.`, `_`, `-`.
///
/// Consequently the empty string, whitespace, control characters, `/`, `:`
/// and any non-ASCII text are all rejected.
///
/// ```
/// use vbuff_types::validation::is_valid_identifier;
///
/// assert!(is_valid_identifier("device-01.a_b", 128));
/// assert!(!is_valid_identifier("", 128));
/// assert!(!is_valid_identifier("has space", 128));
/// assert!(!is_valid_identifier("host:port", 128));
/// assert!(!is_valid_identifier("caf\u{e9}", 128));
/// ```
pub fn is_valid_identifier(value: &str, max_len: usize) -> bool {
    is_valid_identifier_with_extra(value, max_len, &[])
}

/// [`is_valid_identifier`] with additional allowed ASCII punctuation bytes.
///
/// Same contract as [`is_valid_identifier`], except bytes listed in `extra`
/// are also accepted. This covers historical variants that widen the
/// canonical `[A-Za-z0-9._-]` set, e.g. terminal host/session identifiers
/// (extra `b":"`) or editor language ids (extra `b"+#"`).
///
/// `extra` widens the set only; it cannot remove bytes from the canonical
/// set. Call sites that are *stricter* than the canon (lowercase-only ids,
/// underscore-only error codes) must keep their own predicate.
///
/// ```
/// use vbuff_types::validation::is_valid_identifier_with_extra;
///
/// assert!(is_valid_identifier_with_extra("host:22", 256, b":"));
/// assert!(is_valid_identifier_with_extra("c++", 64, b"+#"));
/// assert!(!is_valid_identifier_with_extra("host:22", 256, b""));
/// ```
pub fn is_valid_identifier_with_extra(value: &str, max_len: usize, extra: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.')
                || extra.contains(&byte)
        })
}

/// Returns `true` when `value` is a canonical human-readable label.
///
/// Contract:
/// - non-empty (byte-level: a string of only spaces is still *valid* here;
///   see [`is_valid_trimmed_label`] for the stricter variant);
/// - `value.len() <= max_len` (bytes, not chars);
/// - contains no control characters (per [`char::is_control`], so tabs,
///   newlines and `NUL` are rejected).
///
/// Arbitrary non-control Unicode is allowed.
///
/// ```
/// use vbuff_types::validation::is_valid_label;
///
/// assert!(is_valid_label("My Collection", 128));
/// assert!(is_valid_label("метка", 128));
/// assert!(is_valid_label(" ", 128)); // blank but byte-non-empty
/// assert!(!is_valid_label("", 128));
/// assert!(!is_valid_label("a\nb", 128));
/// ```
pub fn is_valid_label(value: &str, max_len: usize) -> bool {
    !value.is_empty() && value.len() <= max_len && !value.chars().any(char::is_control)
}

/// [`is_valid_label`] that additionally rejects whitespace-only strings.
///
/// Matches the historical `vbuff-core` variants that used
/// `!value.trim().is_empty()` as the emptiness check
/// (`recall/memory.rs::valid_label`, `workflow/selection.rs` tag check).
///
/// ```
/// use vbuff_types::validation::is_valid_trimmed_label;
///
/// assert!(is_valid_trimmed_label("My Collection", 128));
/// assert!(!is_valid_trimmed_label("   ", 128));
/// ```
pub fn is_valid_trimmed_label(value: &str, max_len: usize) -> bool {
    is_valid_label(value, max_len) && !value.trim().is_empty()
}

/// Returns `true` when `key_id` is a valid signing-key identifier.
///
/// Exactly [`is_valid_identifier`] with the fixed [`MAX_KEY_ID_BYTES`] (96)
/// budget, matching both historical `validate_key_id` copies in
/// `vbuff-update` (which differ only in how they wrap the failure into an
/// error; that wrapping stays at the call site).
///
/// ```
/// use vbuff_types::validation::valid_key_id;
///
/// assert!(valid_key_id("release-2026.ed25519"));
/// assert!(!valid_key_id(""));
/// assert!(!valid_key_id(&"k".repeat(97)));
/// ```
pub fn valid_key_id(key_id: &str) -> bool {
    is_valid_identifier(key_id, MAX_KEY_ID_BYTES)
}

/// Returns `true` when `value` is a strict `MAJOR.MINOR.PATCH` version.
///
/// Contract (exact port of the two verbatim `vbuff-plugin` copies):
/// - exactly three dot-separated segments;
/// - each segment is non-empty and consists solely of ASCII digits.
///
/// Notes preserved from the originals: leading zeros are accepted
/// (`"01.2.3"` is valid), there is no length cap on segments, and no
/// pre-release/build suffixes are allowed (`"1.2.3-beta"` is invalid).
///
/// ```
/// use vbuff_types::validation::valid_version;
///
/// assert!(valid_version("1.2.3"));
/// assert!(!valid_version("1.2"));
/// assert!(!valid_version("1.2.3.4"));
/// assert!(!valid_version("1.2.3-beta"));
/// ```
pub fn valid_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let valid = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    });
    valid && parts.next().is_none()
}

/// Returns `true` when every byte in `bytes` is zero.
///
/// Used to reject all-zero hashes, nonces, and session keys as
/// "obviously never initialized". Notes:
/// - the empty slice returns `true` (vacuous truth), same as the historical
///   const-generic helper would for `[u8; 0]`; existing call sites always
///   pass fixed-size non-empty arrays, so this edge never changes behavior;
/// - this is a plain short-circuiting scan, **not** constant-time; do not
///   use it to compare secret values (none of the historical call sites
///   did: they only gate on the all-zero sentinel).
///
/// Fixed-size arrays coerce implicitly: `all_zero(&[0u8; 32])` works as-is.
///
/// ```
/// use vbuff_types::validation::all_zero;
///
/// assert!(all_zero(&[0u8; 32]));
/// assert!(all_zero(&[]));
/// assert!(!all_zero(&[0, 0, 1]));
/// ```
pub fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_accepts_canonical_charset() {
        assert!(is_valid_identifier("a", 128));
        assert!(is_valid_identifier("Device-01_a.b", 128));
        assert!(is_valid_identifier("...", 128));
        assert!(is_valid_identifier("ABCxyz0189._-", 128));
    }

    #[test]
    fn identifier_rejects_empty_string() {
        assert!(!is_valid_identifier("", 128));
        assert!(!is_valid_identifier("", 0));
    }

    #[test]
    fn identifier_length_boundary_is_inclusive() {
        let at_max = "x".repeat(128);
        let over_max = "x".repeat(129);
        assert!(is_valid_identifier(&at_max, 128));
        assert!(!is_valid_identifier(&over_max, 128));
        assert!(is_valid_identifier("x", 1));
        assert!(!is_valid_identifier("xy", 1));
    }

    #[test]
    fn identifier_length_is_measured_in_bytes() {
        // U+00E9 is two bytes in UTF-8; it must fail on charset anyway,
        // but a two-byte budget with two ASCII chars passes.
        assert!(is_valid_identifier("ab", 2));
        assert!(!is_valid_identifier("abc", 2));
    }

    #[test]
    fn identifier_rejects_non_ascii() {
        assert!(!is_valid_identifier("caf\u{e9}", 128));
        assert!(!is_valid_identifier("тест", 128));
        assert!(!is_valid_identifier("emoji\u{1F600}", 128));
    }

    #[test]
    fn identifier_rejects_spaces_and_forbidden_punctuation() {
        assert!(!is_valid_identifier("has space", 128));
        assert!(!is_valid_identifier(" leading", 128));
        assert!(!is_valid_identifier("a/b", 128));
        assert!(!is_valid_identifier("a:b", 128));
        assert!(!is_valid_identifier("a+b", 128));
        assert!(!is_valid_identifier("a\tb", 128));
        assert!(!is_valid_identifier("a\0b", 128));
    }

    #[test]
    fn identifier_extra_widens_but_never_narrows() {
        assert!(is_valid_identifier_with_extra("host:2222", 256, b":"));
        assert!(!is_valid_identifier_with_extra("host:2222", 256, b""));
        assert!(is_valid_identifier_with_extra("c#", 64, b"+#"));
        // Canonical punctuation stays allowed regardless of `extra`.
        assert!(is_valid_identifier_with_extra("a.b-c_d", 64, b":"));
        // Non-listed bytes are still rejected.
        assert!(!is_valid_identifier_with_extra("a b", 64, b":"));
    }

    #[test]
    fn label_accepts_unicode_and_spaces() {
        assert!(is_valid_label("My Collection", 128));
        assert!(is_valid_label("метка с пробелами", 128));
        assert!(is_valid_label(" ", 128));
    }

    #[test]
    fn label_rejects_empty_and_control_characters() {
        assert!(!is_valid_label("", 128));
        assert!(!is_valid_label("a\nb", 128));
        assert!(!is_valid_label("a\tb", 128));
        assert!(!is_valid_label("a\0b", 128));
        assert!(!is_valid_label("\u{7f}", 128));
    }

    #[test]
    fn label_length_boundary_is_inclusive_in_bytes() {
        let at_max = "y".repeat(64);
        let over_max = "y".repeat(65);
        assert!(is_valid_label(&at_max, 64));
        assert!(!is_valid_label(&over_max, 64));
        // 33 two-byte chars = 66 bytes > 64, even though only 33 chars.
        let wide = "я".repeat(33);
        assert!(!is_valid_label(&wide, 64));
    }

    #[test]
    fn trimmed_label_rejects_blank_strings() {
        assert!(is_valid_trimmed_label("ok", 64));
        assert!(is_valid_trimmed_label(" ok ", 64));
        assert!(!is_valid_trimmed_label("   ", 64));
        assert!(!is_valid_trimmed_label("", 64));
    }

    #[test]
    fn key_id_uses_96_byte_budget() {
        assert!(valid_key_id("release-2026.ed25519"));
        assert!(valid_key_id(&"k".repeat(96)));
        assert!(!valid_key_id(&"k".repeat(97)));
        assert!(!valid_key_id(""));
        assert!(!valid_key_id("key id"));
        assert!(!valid_key_id("key:id"));
    }

    #[test]
    fn version_accepts_three_numeric_segments() {
        assert!(valid_version("1.2.3"));
        assert!(valid_version("0.0.0"));
        assert!(valid_version("01.2.3")); // leading zeros preserved from originals
        assert!(valid_version("10.200.3000"));
    }

    #[test]
    fn version_rejects_wrong_shape() {
        assert!(!valid_version(""));
        assert!(!valid_version("1"));
        assert!(!valid_version("1.2"));
        assert!(!valid_version("1.2.3.4"));
        assert!(!valid_version("1..3"));
        assert!(!valid_version(".1.2"));
        assert!(!valid_version("1.2."));
        assert!(!valid_version("1.2.x"));
        assert!(!valid_version("1.2.3-beta"));
        assert!(!valid_version("v1.2.3"));
        assert!(!valid_version("1. 2.3"));
        assert!(!valid_version("１.2.3")); // fullwidth digit is not ASCII
    }

    #[test]
    fn all_zero_boundaries() {
        assert!(all_zero(&[]));
        assert!(all_zero(&[0]));
        assert!(all_zero(&[0u8; 32]));
        assert!(!all_zero(&[1]));
        assert!(!all_zero(&[0, 0, 0, 1]));
        assert!(!all_zero(&[1, 0, 0, 0]));
        let array: [u8; 16] = [0; 16];
        assert!(all_zero(&array)); // fixed-size arrays coerce
    }
}
