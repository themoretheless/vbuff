//! Content hashing for deduplication.
//!
//! Two copies are considered "the same" only if every paste-able byte is
//! identical. The hash is therefore computed over the *whole* flavor set, with
//! flavors sorted by MIME so ordering differences do not change the digest, and
//! each entry length-prefixed so concatenation is unambiguous.
//!
//! The bytes are hashed raw, never normalized (no newline/whitespace/encoding
//! changes), so editor selections round-trip exactly.

use vbuff_types::{Body, Flavor};

/// A flavor as seen at capture time, before it becomes a stored [`Flavor`].
///
/// Only inline bytes participate in the canonical hash. (Spilled bodies are
/// hashed at spill time from the same bytes, so the digest is stable.)
pub struct CanonicalFlavor<'a> {
    pub mime: &'a str,
    pub bytes: &'a [u8],
}

/// Compute the BLAKE3 content hash over a set of canonical flavors.
///
/// The input is sorted by MIME internally, so callers need not pre-sort.
pub fn content_hash(flavors: &[CanonicalFlavor<'_>]) -> [u8; 32] {
    let mut order: Vec<usize> = (0..flavors.len()).collect();
    order.sort_by(|&a, &b| flavors[a].mime.cmp(flavors[b].mime));

    let mut hasher = blake3::Hasher::new();
    for &i in &order {
        let f = &flavors[i];
        hasher.update(f.mime.as_bytes());
        hasher.update(&(f.bytes.len() as u64).to_le_bytes());
        hasher.update(f.bytes);
    }
    *hasher.finalize().as_bytes()
}

/// Convenience: hash a slice of stored [`Flavor`]s.
///
/// Only inline bodies contribute bytes; a spilled body contributes its
/// `blob_ref` digest bytes instead (the blob ref is itself a content hash, so
/// this remains stable for identical content).
pub fn content_hash_from_flavors(flavors: &[Flavor]) -> [u8; 32] {
    let mut order = flavors.iter().collect::<Vec<_>>();
    order.sort_by(|left, right| left.mime.cmp(&right.mime));
    let mut hasher = blake3::Hasher::new();
    for flavor in order {
        let bytes = match &flavor.body {
            Body::Inline(bytes) => bytes.as_slice(),
            Body::Spilled { blob_ref, .. } => blob_ref.as_bytes(),
        };
        hasher.update(flavor.mime.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_hashes_equal() {
        let a = [
            CanonicalFlavor {
                mime: "text/plain",
                bytes: b"hello",
            },
            CanonicalFlavor {
                mime: "text/html",
                bytes: b"<b>hi</b>",
            },
        ];
        let b = [
            CanonicalFlavor {
                mime: "text/plain",
                bytes: b"hello",
            },
            CanonicalFlavor {
                mime: "text/html",
                bytes: b"<b>hi</b>",
            },
        ];
        assert_eq!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn flavor_order_does_not_matter() {
        let a = [
            CanonicalFlavor {
                mime: "text/plain",
                bytes: b"hello",
            },
            CanonicalFlavor {
                mime: "text/html",
                bytes: b"<b>hi</b>",
            },
        ];
        // Same flavors, reversed input order -> same hash.
        let b = [
            CanonicalFlavor {
                mime: "text/html",
                bytes: b"<b>hi</b>",
            },
            CanonicalFlavor {
                mime: "text/plain",
                bytes: b"hello",
            },
        ];
        assert_eq!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn different_content_hashes_differ() {
        let a = [CanonicalFlavor {
            mime: "text/plain",
            bytes: b"hello",
        }];
        let b = [CanonicalFlavor {
            mime: "text/plain",
            bytes: b"world",
        }];
        assert_ne!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn length_prefix_prevents_collision() {
        // Without a length prefix, ("ab","") and ("a","b") could collide.
        let a = [
            CanonicalFlavor {
                mime: "x",
                bytes: b"ab",
            },
            CanonicalFlavor {
                mime: "y",
                bytes: b"",
            },
        ];
        let b = [
            CanonicalFlavor {
                mime: "x",
                bytes: b"a",
            },
            CanonicalFlavor {
                mime: "y",
                bytes: b"b",
            },
        ];
        assert_ne!(content_hash(&a), content_hash(&b));
    }
}
