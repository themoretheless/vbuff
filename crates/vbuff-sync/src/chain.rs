//! Domain-separated preimages and the generic signed hash chain.
//!
//! Every append-only log in this crate (device membership, the sync audit
//! ledger, clip chain-of-custody) is a [`SignedChain`]: a vector of
//! [`ChainLink`]s where each link commits to its predecessor's hash and
//! carries an Ed25519 signature over the same bytes that produced that hash.
//!
//! The point of the generic is that `append` and `verify` cannot drift.
//! Both call the single [`ChainEntry::expected_signing_key`] hook, so the key
//! a writer must hold is by construction the key a reader requires; both
//! enforce the same [`ChainEntry::MAX_ENTRIES`] bound (fail-closed on write
//! *and* on read); both build the link preimage with [`link_preimage`].
//!
//! # Preimage framing
//!
//! Framing follows `docs/domain-separation-convention.md` §5.1:
//!
//! ```text
//! preimage := DOMAIN || 0x00 || field*
//! field    := fixed-width bytes            // hashes, integers, discriminants
//!           | u32_be(len) || bytes         // variable-length
//! ```
//!
//! The domain constant is a bare ASCII label; the terminator belongs to the
//! framing and is emitted exactly once by [`Preimage::new`]. Every
//! variable-length field carries a length prefix, including the last one:
//! "the final field cannot be ambiguous" is true but fragile, and it is the
//! reasoning that made `merkle.rs::leaf_hash` collidable once a field was
//! appended after it. A repeated group is preceded by its `u32_be` count.
//!
//! Enums are encoded as explicit `u8` discriminants, never as `Serialize`
//! output of a renameable variant, so a `#[serde(rename)]` cannot silently
//! move signed bytes.
//!
//! # Signatures
//!
//! Signatures are made over the preimage itself, never over the bare 32-byte
//! digest. That removes the `sign(&hash)` pattern flagged in §2.7 of the
//! convention document: a digest signed on its own carries no domain, so it
//! is only as safe as the accident that its preimage happened to have one.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Result, SyncError};

/// Builder for a domain-separated, unambiguous preimage.
///
/// See the module documentation for the framing. The builder is infallible
/// until [`Preimage::finish`], which fails closed if any variable-length
/// field exceeded the `u32` length prefix instead of silently truncating it.
pub struct Preimage {
    buf: Vec<u8>,
    overflow: bool,
}

impl Preimage {
    /// Start a preimage with `domain` followed by the single `0x00`
    /// terminator. `domain` must be a bare label with no NUL of its own.
    #[must_use]
    pub fn new(domain: &[u8]) -> Self {
        debug_assert!(!domain.contains(&0), "domain must not carry its own NUL");
        let mut buf = Vec::with_capacity(domain.len() + 1);
        buf.extend_from_slice(domain);
        buf.push(0);
        Self {
            buf,
            overflow: false,
        }
    }

    /// Append a fixed-width field verbatim (hash, key, digest).
    pub fn fixed(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    /// Append a variable-length field behind a `u32_be` length prefix.
    pub fn var(&mut self, bytes: &[u8]) -> &mut Self {
        match u32::try_from(bytes.len()) {
            Ok(len) => {
                self.buf.extend_from_slice(&len.to_be_bytes());
                self.buf.extend_from_slice(bytes);
            }
            // Poison instead of truncating: a wrapped length would make two
            // different field tuples share a preimage.
            Err(_) => self.overflow = true,
        }
        self
    }

    /// Append an optional variable-length field as `0x00`, or `0x01`
    /// followed by the length-prefixed bytes.
    pub fn optional(&mut self, value: Option<&[u8]>) -> &mut Self {
        match value {
            None => self.byte(0),
            Some(bytes) => self.byte(1).var(bytes),
        }
    }

    /// Append a single byte: an enum discriminant, a boolean, or a count
    /// that is known to fit.
    pub fn byte(&mut self, value: u8) -> &mut Self {
        self.buf.push(value);
        self
    }

    /// Append a big-endian `u32`. Also the encoding for a repeated group's
    /// element count.
    pub fn u32_be(&mut self, value: u32) -> &mut Self {
        self.fixed(&value.to_be_bytes())
    }

    /// Append a big-endian `u64`.
    pub fn u64_be(&mut self, value: u64) -> &mut Self {
        self.fixed(&value.to_be_bytes())
    }

    /// Finish the preimage, or fail closed if a field overflowed its
    /// length prefix.
    pub fn finish(self) -> Result<Vec<u8>> {
        if self.overflow {
            return Err(SyncError::Invalid(
                "preimage field exceeds the u32 length prefix".into(),
            ));
        }
        Ok(self.buf)
    }
}

/// Serde support for the fixed-width signature (serde derives arrays only up
/// to 32 elements); deserialization fails closed on any other length, so a
/// signature of the wrong size cannot be represented at all.
mod serde_signature {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        value.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        <[u8; 64]>::try_from(bytes.as_slice())
            .map_err(|_| serde::de::Error::custom("chain signature must be 64 bytes"))
    }
}

/// The payload of one link: everything that is hashed and signed.
///
/// Implementors describe *what* goes into a link and *who* may sign it; the
/// chain mechanics (linking, bound, hashing, signing, verification) live in
/// [`SignedChain`] and are not reimplementable per chain.
pub trait ChainEntry: Clone + Serialize + DeserializeOwned + PartialEq {
    /// Bare ASCII domain label, `vbuff-<purpose>-v<n>`, without a NUL.
    ///
    /// Bump the version whenever the preimage layout, the field order, or
    /// the field encoding changes. Persisted chains additionally need their
    /// serialized shape to change so stale data fails to deserialize rather
    /// than verifying under the new rules.
    const DOMAIN: &'static [u8];

    /// Fail-closed upper bound on the number of links, enforced by both
    /// [`SignedChain::append`] and [`SignedChain::verify`].
    const MAX_ENTRIES: usize;

    /// Human-readable chain name used in error messages.
    const LABEL: &'static str;

    /// External trust input threaded through append and verify, identical on
    /// both sides. `()` for chains that authorize themselves from replayed
    /// state.
    type Authority;

    /// State replayed from the links preceding the one being authorized.
    type State: Default;

    /// Contribute this payload's fields to the link preimage, after the
    /// domain and the previous hash.
    ///
    /// The field order is part of the signed format: changing it requires a
    /// [`ChainEntry::DOMAIN`] bump.
    fn extend_preimage(&self, preimage: &mut Preimage);

    /// The one Ed25519 key permitted to sign this link, or an error if the
    /// link is not admissible at all.
    ///
    /// This is the single authorization decision for the chain. It runs
    /// unchanged in [`SignedChain::append`] (deciding which key the writer
    /// must hold) and in [`SignedChain::verify`] (deciding which key the
    /// reader checks the signature against), so the two cannot drift apart
    /// and leave a gap where a link is accepted under a key nobody
    /// re-derives.
    fn expected_signing_key(
        &self,
        index: usize,
        state: &Self::State,
        authority: &Self::Authority,
    ) -> Result<[u8; 32]>;

    /// Fold this payload into the replayed state. The default is a no-op,
    /// for chains whose authorization does not depend on history.
    fn apply(&self, state: &mut Self::State) {
        let _ = state;
    }
}

/// One link: its payload, its position in the chain, and its signature.
///
/// The fields are public so callers can inspect a chain, but nothing here is
/// trustworthy until [`SignedChain::verify`] has replayed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainLink<E> {
    pub payload: E,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
    /// Ed25519 signature over the link preimage (not over `hash`).
    #[serde(with = "serde_signature")]
    pub signature: [u8; 64],
}

/// Append-only hash chain of signed links.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(serialize = "E: ChainEntry", deserialize = "E: ChainEntry"))]
pub struct SignedChain<E: ChainEntry> {
    pub entries: Vec<ChainLink<E>>,
}

impl<E: ChainEntry> Default for SignedChain<E> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

/// The exact bytes hashed and signed for one link.
pub fn link_preimage<E: ChainEntry>(payload: &E, previous_hash: &[u8; 32]) -> Result<Vec<u8>> {
    let mut preimage = Preimage::new(E::DOMAIN);
    preimage.fixed(previous_hash);
    payload.extend_preimage(&mut preimage);
    preimage.finish()
}

impl<E: ChainEntry> SignedChain<E> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Hash of the last link, or all-zero for an empty chain.
    #[must_use]
    pub fn head(&self) -> [u8; 32] {
        self.entries.last().map_or([0; 32], |link| link.hash)
    }

    /// Fold every link into the chain's replayed state.
    ///
    /// This does not verify anything; it is the projection callers use to
    /// read the current state of a chain they have already verified.
    #[must_use]
    pub fn replay(&self) -> E::State {
        self.replay_prefix(self.entries.len())
    }

    fn replay_prefix(&self, links: usize) -> E::State {
        let mut state = E::State::default();
        for link in self.entries.iter().take(links) {
            link.payload.apply(&mut state);
        }
        state
    }

    /// Append a link signed with `signing_key`.
    ///
    /// Fails closed on a full chain, on a payload the chain's authorization
    /// hook rejects, and on any key other than the one that hook names. The
    /// chain is left untouched on every failure.
    ///
    /// The authorization state is replayed from the existing links without
    /// re-checking their signatures, so a chain loaded from an untrusted
    /// source must be passed through [`SignedChain::verify`] before it is
    /// extended.
    pub fn append(
        &mut self,
        payload: E,
        authority: &E::Authority,
        signing_key: &SigningKey,
    ) -> Result<[u8; 32]> {
        if self.entries.len() >= E::MAX_ENTRIES {
            return Err(SyncError::Invalid(format!("{} is full", E::LABEL)));
        }
        let index = self.entries.len();
        let state = self.replay_prefix(index);
        let expected = payload.expected_signing_key(index, &state, authority)?;
        if signing_key.verifying_key().to_bytes() != expected {
            return Err(SyncError::Invalid(format!(
                "{} author key does not match the registered signing key",
                E::LABEL
            )));
        }
        let previous_hash = self.head();
        let preimage = link_preimage(&payload, &previous_hash)?;
        let hash = *blake3::hash(&preimage).as_bytes();
        let signature = signing_key.sign(&preimage).to_bytes();
        self.entries.push(ChainLink {
            payload,
            previous_hash,
            hash,
            signature,
        });
        Ok(hash)
    }

    /// Re-derive and check the whole chain: bound, links, authorization, and
    /// every signature against the key the authorization hook names.
    pub fn verify(&self, authority: &E::Authority) -> Result<()> {
        if self.entries.len() > E::MAX_ENTRIES {
            return Err(SyncError::Invalid(format!(
                "{} exceeds the entry limit",
                E::LABEL
            )));
        }
        let mut state = E::State::default();
        let mut previous_hash = [0_u8; 32];
        for (index, link) in self.entries.iter().enumerate() {
            let preimage = link_preimage(&link.payload, &link.previous_hash)?;
            if link.previous_hash != previous_hash
                || *blake3::hash(&preimage).as_bytes() != link.hash
            {
                return Err(SyncError::Invalid(format!(
                    "{} hash chain is broken",
                    E::LABEL
                )));
            }
            let expected = link
                .payload
                .expected_signing_key(index, &state, authority)?;
            let key = VerifyingKey::from_bytes(&expected).map_err(|_| SyncError::Crypto)?;
            key.verify(&preimage, &Signature::from_bytes(&link.signature))
                .map_err(|_| SyncError::Crypto)?;
            link.payload.apply(&mut state);
            previous_hash = link.hash;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preimage_framing_is_pinned() {
        let mut preimage = Preimage::new(b"dom");
        preimage.var(b"a").var(b"bc");
        assert_eq!(
            preimage.finish().unwrap(),
            b"dom\x00\x00\x00\x00\x01a\x00\x00\x00\x02bc"
        );

        let empty = Preimage::new(b"dom");
        assert_eq!(empty.finish().unwrap(), b"dom\x00");

        let mut scalars = Preimage::new(b"dom");
        scalars.byte(7).u32_be(1).u64_be(2);
        assert_eq!(
            scalars.finish().unwrap(),
            b"dom\x00\x07\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x02"
        );

        let mut absent = Preimage::new(b"dom");
        absent.optional(None);
        let mut present = Preimage::new(b"dom");
        present.optional(Some(b""));
        assert_eq!(absent.finish().unwrap(), b"dom\x00\x00");
        assert_eq!(present.finish().unwrap(), b"dom\x00\x01\x00\x00\x00\x00");
    }

    #[test]
    fn length_prefixes_remove_field_boundary_ambiguity() {
        let mut split = Preimage::new(b"dom");
        split.var(b"a").var(b"bc");
        let mut shifted = Preimage::new(b"dom");
        shifted.var(b"ab").var(b"c");
        assert_ne!(split.finish().unwrap(), shifted.finish().unwrap());

        // A NUL inside a field cannot borrow the domain terminator: the
        // terminator is emitted once, before any field, and every field is
        // length-prefixed.
        let mut nul_inside = Preimage::new(b"dom");
        nul_inside.var(b"\x00x");
        let mut nul_split = Preimage::new(b"dom");
        nul_split.var(b"\x00").var(b"x");
        assert_ne!(nul_inside.finish().unwrap(), nul_split.finish().unwrap());

        // Two domains never share a preimage for the same fields.
        let mut first = Preimage::new(b"vbuff-a-v1");
        first.var(b"x");
        let mut second = Preimage::new(b"vbuff-b-v1");
        second.var(b"x");
        assert_ne!(first.finish().unwrap(), second.finish().unwrap());
    }
}
