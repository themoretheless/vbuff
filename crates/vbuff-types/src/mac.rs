//! One domain-separated HMAC-SHA256 primitive, shared by every MAC mechanism
//! in the workspace.
//!
//! # Why this exists
//!
//! Every MAC mechanism used to spell its preimage out twice: once on the
//! signing path and once on the verifying path. Two copies agree only until
//! somebody edits one of them, and the failure is silent in the dangerous
//! direction: a field appended to the signing copy but not the verifying copy
//! does not break any test, it simply stops being covered, and verification
//! keeps accepting messages whose new field was tampered with. A mechanism
//! built on this module has exactly one statement of what its MAC covers, and
//! both directions read that statement.
//!
//! # Framing
//!
//! ```text
//! preimage := label ‖ separator ‖ parts[0] ‖ parts[1] ‖ … ‖ parts[n-1]
//! proof    := HMAC-SHA256(key, preimage)
//! ```
//!
//! [`MacDomain::new`] is the convention for every new mechanism: a single
//! `0x00` between the label and the first part. It matches
//! `vbuff-update::manifest::signing_preimage` and §5.1 of
//! `docs/domain-separation-convention.md`. The terminator is not decoration:
//! without it a proof issued under `vbuff-foo-v1` is a valid proof under
//! `vbuff-foo-v11` for a message that starts with `1`, because the two
//! preimages are the same bytes.
//!
//! The `legacy_*` constructors exist only because five mechanisms were already
//! issuing proofs under a different framing when this module landed. They are
//! byte-compatible escape hatches for frozen formats, never a choice for new
//! code; each one documents what it is frozen against.
//!
//! # Caller obligation: parts must be unambiguous by layout
//!
//! Parts are concatenated raw, with no length prefixes. `parts` must therefore
//! be unambiguous on its own: either every part is fixed-width, or at most one
//! part is variable-length and it comes last. Two adjacent variable-length
//! parts let an attacker move the boundary between them and obtain the same
//! preimage from different data. A mechanism that needs two variable-length
//! fields must length-prefix them itself (`u32_be`, per the convention
//! document) before handing them over.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Byte length of every proof this module produces.
pub const MAC_LEN: usize = 32;

/// What separates a domain label from the first part of the preimage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DomainSeparator {
    /// A single `0x00`. The convention.
    Nul,
    /// Nothing at all. Frozen legacy framing.
    None,
    /// A single ASCII byte outside the alphabet of the following part. Frozen
    /// legacy framing.
    Ascii(u8),
}

/// A domain label plus the framing that separates it from the message.
///
/// Declare one `const` per mechanism next to the data it covers, so the
/// framing decision is a single reviewable line rather than something implied
/// by two `mac.update(..)` sequences that have to be diffed against each
/// other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacDomain {
    label: &'static str,
    separator: DomainSeparator,
}

impl MacDomain {
    /// The convention: `label ‖ 0x00 ‖ parts`.
    ///
    /// Use this for every new mechanism. `label` must be ASCII of the form
    /// `vbuff-<purpose>-v<n>` and must not contain a `0x00` of its own.
    #[must_use]
    pub const fn new(label: &'static str) -> Self {
        Self {
            label,
            separator: DomainSeparator::Nul,
        }
    }

    /// Frozen legacy framing: the label is written with no terminator at all.
    ///
    /// Only unambiguous while no other domain in the workspace has this label
    /// as a prefix *and* the first part can begin with the remaining bytes.
    /// Reserved for mechanisms whose proofs were already issued under this
    /// framing; adding the terminator would change every one of those bytes.
    /// New mechanisms must use [`MacDomain::new`].
    #[must_use]
    pub const fn legacy_unterminated(label: &'static str) -> Self {
        Self {
            label,
            separator: DomainSeparator::None,
        }
    }

    /// Frozen legacy framing: `label ‖ separator ‖ parts`, where `separator`
    /// is a single ASCII byte.
    ///
    /// Unambiguous exactly when `separator` cannot occur at the start of the
    /// first part, for example `b'.'` against a base64url payload. Reserved
    /// for already-issued formats; new mechanisms must use
    /// [`MacDomain::new`].
    #[must_use]
    pub const fn legacy_ascii_separated(label: &'static str, separator: u8) -> Self {
        assert!(separator != 0, "use MacDomain::new for NUL framing");
        assert!(
            separator.is_ascii_graphic(),
            "a legacy separator must be a printable ASCII byte"
        );
        Self {
            label,
            separator: DomainSeparator::Ascii(separator),
        }
    }

    /// The domain label, without its framing.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.label
    }
}

/// A computed HMAC that has not been observed yet.
///
/// The only two things that can be done with it are producing the tag and
/// checking a tag against it, and the check is constant-time. Returning this
/// instead of `[u8; 32]` is what keeps a verifier from writing
/// `computed == received`, which leaks the length of the matching prefix
/// through timing and is how a MAC gets forged one byte at a time.
#[must_use = "a MacProof does nothing until it is finished or verified"]
pub struct MacProof(HmacSha256);

impl MacProof {
    /// The tag, for a signer to emit.
    #[must_use]
    pub fn finish(self) -> [u8; MAC_LEN] {
        self.0.finalize().into_bytes().into()
    }

    /// Constant-time check of `tag` against this proof.
    ///
    /// Returns `false` for any tag whose length is not [`MAC_LEN`], and
    /// otherwise compares every byte regardless of where the first difference
    /// is.
    #[must_use]
    pub fn verify(self, tag: &[u8]) -> bool {
        // `Mac::verify_slice` rejects a length mismatch first and then
        // compares with `subtle::ConstantTimeEq`; it is the constant-time
        // comparison, not a convenience wrapper around `==`.
        self.0.verify_slice(tag).is_ok()
    }
}

impl core::fmt::Debug for MacProof {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("MacProof([redacted])")
    }
}

/// Builds `HMAC-SHA256(key, domain ‖ separator ‖ parts…)`.
///
/// This is the only place in the workspace that lays out a MAC preimage. Both
/// directions of a mechanism must reach their tag through one call site of
/// this function; see the module docs for why.
///
/// `key` may be any length: HMAC derives a block-sized key from it, so there
/// is no failure mode to handle. Mechanisms that want to reject specific keys
/// (an all-zero key, say) do so in their own wrapper, on both paths at once.
pub fn hmac_proof(domain: MacDomain, key: &[u8], parts: &[&[u8]]) -> MacProof {
    debug_assert!(
        !domain.label.as_bytes().contains(&0),
        "a domain label must not contain a NUL of its own"
    );
    debug_assert!(
        domain.label.is_ascii(),
        "a domain label must be ASCII so its byte length is its character length"
    );
    // Infallible: `hmac` accepts a key of any length and its `new_from_slice`
    // returns `Ok` unconditionally (hmac 0.13, `HmacCore::new_from_slice`).
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(domain.label.as_bytes());
    match domain.separator {
        DomainSeparator::Nul => mac.update(&[0]),
        DomainSeparator::Ascii(byte) => mac.update(&[byte]),
        DomainSeparator::None => {}
    }
    for part in parts {
        mac.update(part);
    }
    MacProof(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    const NUL: MacDomain = MacDomain::new("vbuff-mac-test-v1");

    #[test]
    fn framing_bytes_are_pinned_per_separator() {
        let key = [7_u8; 32];
        // Pinned so that a change to the framing shows up here rather than as
        // a silent format break in a downstream mechanism.
        assert_eq!(
            hex(&hmac_proof(NUL, &key, &[b"payload"]).finish()),
            "7c7b29a64ed397ad1eed15572013231057e502d9b94eb2dcf73e62f367795bc7"
        );
        assert_eq!(
            hex(&hmac_proof(
                MacDomain::legacy_unterminated("vbuff-mac-test-v1"),
                &key,
                &[b"payload"]
            )
            .finish()),
            "407efcbbeb7944725aa3f6841b71e9cd4d7e01de8d55f6bf3dc6c4a18218004e"
        );
        assert_eq!(
            hex(&hmac_proof(
                MacDomain::legacy_ascii_separated("vbuff-mac-test-v1", b'.'),
                &key,
                &[b"payload"]
            )
            .finish()),
            "62149bc09e87ae1fe2451c353440fd833098854d4c12d5136f32309768201f04"
        );
    }

    #[test]
    fn the_terminator_is_what_stops_a_prefix_domain_collision() {
        let key = [1_u8; 32];
        // Without a terminator, "vbuff-a-v1" over "1x" and "vbuff-a-v11" over
        // "x" are the same preimage. With one, they are not.
        let short = MacDomain::legacy_unterminated("vbuff-a-v1");
        let long = MacDomain::legacy_unterminated("vbuff-a-v11");
        assert_eq!(
            hmac_proof(short, &key, &[b"1x"]).finish(),
            hmac_proof(long, &key, &[b"x"]).finish()
        );
        assert_ne!(
            hmac_proof(MacDomain::new("vbuff-a-v1"), &key, &[b"1x"]).finish(),
            hmac_proof(MacDomain::new("vbuff-a-v11"), &key, &[b"x"]).finish()
        );
    }

    #[test]
    fn a_proof_from_another_domain_never_verifies() {
        let key = [4_u8; 32];
        let parts: [&[u8]; 2] = [b"fixed-width-a", b"fixed-width-b"];
        let tag = hmac_proof(MacDomain::new("vbuff-other-v1"), &key, &parts).finish();
        assert!(!hmac_proof(NUL, &key, &parts).verify(&tag));
        assert!(hmac_proof(MacDomain::new("vbuff-other-v1"), &key, &parts).verify(&tag));
    }

    #[test]
    fn verification_rejects_wrong_keys_lengths_and_single_bit_flips() {
        let key = [2_u8; 32];
        let tag = hmac_proof(NUL, &key, &[b"body"]).finish();
        assert!(hmac_proof(NUL, &key, &[b"body"]).verify(&tag));
        assert!(!hmac_proof(NUL, &[3_u8; 32], &[b"body"]).verify(&tag));
        assert!(!hmac_proof(NUL, &key, &[b"bodz"]).verify(&tag));
        assert!(!hmac_proof(NUL, &key, &[b"body"]).verify(&tag[..MAC_LEN - 1]));
        assert!(!hmac_proof(NUL, &key, &[b"body"]).verify(&[]));
        let mut flipped = tag;
        flipped[MAC_LEN - 1] ^= 1;
        assert!(!hmac_proof(NUL, &key, &[b"body"]).verify(&flipped));
    }

    #[test]
    fn part_boundaries_are_invisible_to_the_mac() {
        // Documents the caller obligation rather than hiding it: raw
        // concatenation means the split into parts carries no information.
        let key = [5_u8; 32];
        assert_eq!(
            hmac_proof(NUL, &key, &[b"ab", b"cd"]).finish(),
            hmac_proof(NUL, &key, &[b"abcd"]).finish()
        );
    }

    #[test]
    fn any_key_length_is_accepted() {
        let parts: [&[u8]; 1] = [b"x"];
        assert_ne!(
            hmac_proof(NUL, &[], &parts).finish(),
            hmac_proof(NUL, &[9_u8; 512], &parts).finish()
        );
    }

    #[test]
    fn a_proof_never_prints_its_state() {
        assert_eq!(
            format!("{:?}", hmac_proof(NUL, &[1_u8; 32], &[b"secret"])),
            "MacProof([redacted])"
        );
    }
}
