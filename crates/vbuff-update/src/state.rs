//! Durable persistence for the update verifier's security state.
//!
//! Without persistence the anti-replay watermark (`highest_accepted_sequence`)
//! and the keyring (including revocations) lived only in memory: every process
//! restart re-opened a downgrade/replay window and forgot rotated or revoked
//! keys. `UpdateVerifier::load`/`store` close that window. Loading fails
//! closed: a corrupt, unversioned, or unreadable state file is an error, never
//! a silent reset to sequence 0.
//!
//! # On-disk format
//!
//! The file is a JSON envelope with three fields, in this order:
//!
//! ```json
//! {"schema":1,"state":{"keyring":{"keys":{}},"highest_accepted_sequence":10},"checksum":"<64 hex chars>"}
//! ```
//!
//! * `schema`: format version, currently [`VERIFIER_STATE_SCHEMA`]. It is a
//!   required field with no serde default, so the pre-schema format (a bare
//!   `VerifierState` object) is rejected rather than guessed at.
//! * `state`: the [`VerifierState`] body.
//! * `checksum`: lowercase hex BLAKE3 over the domain-separated preimage
//!   described in [`state_checksum`].
//!
//! # Schema policy (fail-closed, no downgrade)
//!
//! * `schema` missing (a file written before this field existed): error.
//!   Accepting it would mean trusting a body whose integrity cannot be
//!   checked, and the alternative (starting over from a bootstrap keyring)
//!   is exactly the silent watermark reset this module exists to prevent.
//!   Recovery is an explicit operator decision: delete the file to
//!   re-provision from scratch, accepting that the replay window reopens.
//! * `schema` greater than [`VERIFIER_STATE_SCHEMA`]: error. A newer build
//!   wrote state this build cannot interpret; downgrading to a zeroed
//!   watermark would let an attacker replay every release the newer build had
//!   already accepted, so an older binary must refuse to run rather than
//!   quietly rewind. No state is rewritten on this path.
//! * `schema` less than [`VERIFIER_STATE_SCHEMA`]: no such version exists yet
//!   (1 is the first). The check is `!=`, so any unknown value errors; when a
//!   schema 2 is introduced, migration must be added deliberately here and
//!   must carry the watermark forward, never reset it.
//!
//! # Integrity
//!
//! The checksum detects truncation, partial writes, bit rot, and hand-editing
//! of the file. It is a plain (unkeyed) hash, not a MAC: an attacker who can
//! write the file can also recompute the checksum. Defending against that
//! requires filesystem permissions plus a key this crate does not own; the
//! checksum's job is to make damaged state fail loudly instead of loading as
//! a plausible-looking older watermark.

use std::io::Write;
use std::path::Path;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::manifest::{SignedUpdateManifest, UpdateKeyring, UpdateVerifier, VerifiedUpdate};
use crate::{Result, UpdateError};

/// Domain separator for the state checksum, matching the crate's convention
/// of a NUL-terminated `vbuff-<purpose>-v<n>` prefix on every hashed or
/// signed preimage. It keeps this digest from ever colliding with a manifest
/// signing preimage or a rollout bucket hash.
const VERIFIER_STATE_CHECKSUM_DOMAIN: &[u8] = b"vbuff-update-verifier-state-v1\0";

/// Version of the persisted envelope this build reads and writes.
const VERIFIER_STATE_SCHEMA: u32 = 1;

/// Serializable snapshot of everything the verifier must survive a restart.
///
/// `deny_unknown_fields` is deliberate: an unexpected field means the file was
/// written by something this build does not understand, and guessing is worse
/// than failing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierState {
    pub keyring: UpdateKeyring,
    pub highest_accepted_sequence: u64,
}

/// The on-disk envelope: schema version, body, and integrity tag.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedVerifierState {
    schema: u32,
    state: VerifierState,
    checksum: String,
}

/// Reads only the schema version, ignoring every other field, so that a file
/// written by a future build fails with "unsupported schema" instead of a
/// confusing missing-field error from the full parse.
#[derive(Deserialize)]
struct SchemaProbe {
    schema: u32,
}

/// BLAKE3 over `DOMAIN || schema (4 bytes, big endian) || canonical body`.
///
/// The body is `serde_json::to_vec(state)`: serde's derived `Serialize` emits
/// struct fields in declaration order and the keyring is a `BTreeMap`, so the
/// bytes are deterministic for a given state, on any platform and in any
/// process. The preimage is therefore recomputed from the *parsed* state on
/// load rather than taken from the raw file bytes: a file that has been
/// re-indented or whose keys were reordered still verifies, while any change
/// to the actual keyring or watermark does not. Binding `schema` into the
/// preimage stops a future envelope from being replayed as this one by
/// editing the version number alone.
fn state_checksum(schema: u32, state: &VerifierState) -> Result<blake3::Hash> {
    let body =
        serde_json::to_vec(state).map_err(|error| UpdateError::Serialization(error.to_string()))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(VERIFIER_STATE_CHECKSUM_DOMAIN);
    hasher.update(&schema.to_be_bytes());
    hasher.update(&body);
    Ok(hasher.finalize())
}

/// Parse and authenticate a state file. Every failure is an error; none of
/// them fall back to a fresh state.
fn decode_state(bytes: &[u8]) -> Result<VerifierState> {
    let probe: SchemaProbe = serde_json::from_slice(bytes).map_err(|error| {
        UpdateError::Serialization(format!(
            "verifier state file has no readable schema version: {error}"
        ))
    })?;
    if probe.schema != VERIFIER_STATE_SCHEMA {
        return Err(UpdateError::Serialization(format!(
            "verifier state schema {} is unsupported; this build reads schema {VERIFIER_STATE_SCHEMA}",
            probe.schema
        )));
    }

    let persisted: PersistedVerifierState = serde_json::from_slice(bytes)
        .map_err(|error| UpdateError::Serialization(error.to_string()))?;
    let stored = blake3::Hash::from_hex(&persisted.checksum).map_err(|_| {
        UpdateError::Serialization("verifier state checksum is not a valid digest".into())
    })?;
    let expected = state_checksum(persisted.schema, &persisted.state)?;
    if stored != expected {
        return Err(UpdateError::Serialization(
            "verifier state checksum mismatch: the file is damaged or was edited".into(),
        ));
    }
    Ok(persisted.state)
}

impl UpdateVerifier {
    pub fn from_state(state: VerifierState) -> Self {
        Self::new(state.keyring, state.highest_accepted_sequence)
    }

    pub fn state(&self) -> VerifierState {
        VerifierState {
            keyring: self.keyring().clone(),
            highest_accepted_sequence: self.highest_accepted_sequence(),
        }
    }

    /// Load the verifier state from `path`.
    ///
    /// * Missing file: `bootstrap()` builds the initial keyring and the
    ///   watermark starts at 0 (first-run provisioning).
    /// * Corrupt, unreadable, unversioned, future-schema, or
    ///   checksum-mismatched file: returns an error. There is deliberately no
    ///   silent fallback to a fresh state, because resetting the watermark to
    ///   0 would reopen the downgrade/replay window and dropping the keyring
    ///   would forget revocations.
    pub fn load(path: impl AsRef<Path>, bootstrap: impl FnOnce() -> UpdateKeyring) -> Result<Self> {
        let path = path.as_ref();
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::new(bootstrap(), 0));
            }
            Err(error) => return Err(UpdateError::Io(error.to_string())),
        };
        Ok(Self::from_state(decode_state(&bytes)?))
    }

    /// Persist the full verifier state to `path` as one atomic write
    /// (write-temp-then-rename plus fsync, the same pattern the store uses).
    ///
    /// Callers must invoke this after every successful `verify()`: the
    /// watermark and any keyring changes (rotations, revocations) are only
    /// durable once stored. Skipping it degrades security to the old
    /// in-memory behavior - the next restart reopens the replay window.
    /// [`UpdateVerifier::verify_and_store`] makes that pairing the default.
    pub fn store(&self, path: impl AsRef<Path>) -> Result<()> {
        let state = self.state();
        let persisted = PersistedVerifierState {
            schema: VERIFIER_STATE_SCHEMA,
            checksum: state_checksum(VERIFIER_STATE_SCHEMA, &state)?
                .to_hex()
                .to_string(),
            state,
        };
        let bytes = serde_json::to_vec(&persisted)
            .map_err(|error| UpdateError::Serialization(error.to_string()))?;
        let mut file = atomic_write_file::AtomicWriteFile::open(path)
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        file.write_all(&bytes)
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        file.as_file()
            .sync_all()
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        file.commit()
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        Ok(())
    }

    /// Verify `signed` and persist the resulting state in one step, so that
    /// durability is not opt-in.
    ///
    /// If the write fails the in-memory verifier is rolled back to its
    /// pre-verify state and the I/O error is returned: an update accepted in
    /// memory but not on disk would advance the watermark for this process
    /// only, and the next restart would happily replay the same manifest (or,
    /// after a rotation, disagree with disk about which keys are trusted).
    /// Callers must treat the error as "update not accepted".
    pub fn verify_and_store(
        &mut self,
        signed: &SignedUpdateManifest,
        current_version: &Version,
        installation_id: &[u8],
        path: impl AsRef<Path>,
    ) -> Result<VerifiedUpdate> {
        let previous = self.state();
        let verified = self.verify(signed, current_version, installation_id)?;
        if let Err(error) = self.store(path) {
            *self = Self::from_state(previous);
            return Err(error);
        }
        Ok(verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ed25519_dalek::SigningKey;

    use crate::manifest::{Artifact, TrustedKey, UpdateManifest};

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn keyring(revoked_at_sequence: Option<u64>) -> UpdateKeyring {
        let mut keyring = UpdateKeyring::default();
        keyring
            .trust(
                "release-1",
                TrustedKey {
                    public_key: signing_key().verifying_key().to_bytes(),
                    activates_at_sequence: 1,
                    revoked_at_sequence,
                },
            )
            .unwrap();
        keyring
    }

    fn state() -> VerifierState {
        VerifierState {
            keyring: keyring(Some(9)),
            highest_accepted_sequence: 10,
        }
    }

    fn signed_release() -> SignedUpdateManifest {
        let manifest = UpdateManifest {
            schema: 1,
            sequence: 10,
            version: Version::parse("0.2.0").unwrap(),
            minimum_client: Version::parse("0.1.0").unwrap(),
            published_at_ms: 100,
            rollout_percent: 100,
            artifacts: vec![Artifact {
                target: "aarch64-apple-darwin".into(),
                url: "https://releases.vbuff.dev/vbuff".into(),
                sha256: [3; 32],
                byte_size: 42,
            }],
            next_key: None,
        };
        SignedUpdateManifest::sign("release-1", manifest, &signing_key()).unwrap()
    }

    fn client() -> Version {
        Version::parse("0.1.0").unwrap()
    }

    fn stored_file() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("verifier-state.json");
        UpdateVerifier::from_state(state()).store(&path).unwrap();
        (dir, path)
    }

    fn read(path: &Path) -> String {
        String::from_utf8(std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn roundtrip_preserves_watermark_and_keyring() {
        let (_dir, path) = stored_file();
        let loaded = UpdateVerifier::load(&path, UpdateKeyring::default).unwrap();
        assert_eq!(loaded.state(), state());
    }

    #[test]
    fn stored_file_carries_current_schema_and_checksum() {
        let (_dir, path) = stored_file();
        let text = read(&path);
        assert!(text.starts_with(r#"{"schema":1,"state":"#), "{text}");
        let persisted: PersistedVerifierState = serde_json::from_str(&text).unwrap();
        assert_eq!(persisted.schema, VERIFIER_STATE_SCHEMA);
        assert_eq!(
            persisted.checksum,
            state_checksum(VERIFIER_STATE_SCHEMA, &state())
                .unwrap()
                .to_hex()
                .to_string()
        );
    }

    #[test]
    fn serialized_body_is_byte_stable() {
        // The checksum is only meaningful if the canonical body does not
        // wobble between runs or between store/load cycles.
        let first = serde_json::to_vec(&state()).unwrap();
        let reparsed: VerifierState = serde_json::from_slice(&first).unwrap();
        assert_eq!(serde_json::to_vec(&reparsed).unwrap(), first);
    }

    #[test]
    fn tampered_watermark_fails_the_checksum() {
        let (_dir, path) = stored_file();
        let text = read(&path).replace(
            r#""highest_accepted_sequence":10"#,
            r#""highest_accepted_sequence":11"#,
        );
        assert!(text.contains(r#""highest_accepted_sequence":11"#));
        std::fs::write(&path, text).unwrap();

        let error = UpdateVerifier::load(&path, UpdateKeyring::default).unwrap_err();
        assert!(
            matches!(&error, UpdateError::Serialization(message) if message.contains("checksum mismatch")),
            "{error:?}"
        );
    }

    #[test]
    fn flipped_byte_in_the_keyring_fails_the_checksum() {
        let (_dir, path) = stored_file();
        let mut bytes = std::fs::read(&path).unwrap();
        // Flip the first digit of the trusted key's first public-key byte.
        let offset = bytes
            .windows(14)
            .position(|window| window == br#""public_key":["#)
            .unwrap()
            + 14;
        bytes[offset] = if bytes[offset] == b'1' { b'2' } else { b'1' };
        std::fs::write(&path, &bytes).unwrap();

        let error = UpdateVerifier::load(&path, UpdateKeyring::default).unwrap_err();
        assert!(matches!(error, UpdateError::Serialization(_)), "{error:?}");
    }

    #[test]
    fn revoked_checksum_cannot_be_stripped_by_dropping_the_field() {
        // Removing a revocation is the interesting forgery: it re-arms a
        // burned key. Without the field the body no longer matches the tag.
        let (_dir, path) = stored_file();
        let text = read(&path).replace(
            r#""revoked_at_sequence":9"#,
            r#""revoked_at_sequence":null"#,
        );
        std::fs::write(&path, text).unwrap();

        let error = UpdateVerifier::load(&path, UpdateKeyring::default).unwrap_err();
        assert!(
            matches!(&error, UpdateError::Serialization(message) if message.contains("checksum mismatch")),
            "{error:?}"
        );
    }

    #[test]
    fn future_schema_is_rejected_without_resetting_state() {
        let (_dir, path) = stored_file();
        let text = read(&path).replace(r#"{"schema":1,"#, r#"{"schema":2,"#);
        std::fs::write(&path, &text).unwrap();

        let error = UpdateVerifier::load(&path, UpdateKeyring::default).unwrap_err();
        assert!(
            matches!(&error, UpdateError::Serialization(message) if message.contains("schema 2 is unsupported")),
            "{error:?}"
        );
        // The rejected file is left untouched: a newer build must still be
        // able to read its own state after the older binary refused it.
        assert_eq!(read(&path), text);
    }

    #[test]
    fn pre_schema_file_is_rejected_rather_than_migrated() {
        // The format that shipped before this field: a bare VerifierState.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("verifier-state.json");
        std::fs::write(&path, serde_json::to_vec(&state()).unwrap()).unwrap();

        let error = UpdateVerifier::load(&path, UpdateKeyring::default).unwrap_err();
        assert!(
            matches!(&error, UpdateError::Serialization(message) if message.contains("no readable schema version")),
            "{error:?}"
        );
    }

    #[test]
    fn envelope_without_checksum_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("verifier-state.json");
        let body = serde_json::to_string(&state()).unwrap();
        std::fs::write(&path, format!(r#"{{"schema":1,"state":{body}}}"#)).unwrap();

        assert!(UpdateVerifier::load(&path, UpdateKeyring::default).is_err());
    }

    #[test]
    fn unknown_field_in_the_body_is_rejected() {
        let (_dir, path) = stored_file();
        let text = read(&path).replace(r#""keyring":"#, r#""surprise":true,"keyring":"#);
        std::fs::write(&path, text).unwrap();

        assert!(UpdateVerifier::load(&path, UpdateKeyring::default).is_err());
    }

    #[test]
    fn verify_and_store_rolls_back_when_the_write_fails() {
        let mut verifier = UpdateVerifier::new(keyring(None), 0);
        let before = verifier.state();

        // A directory that does not exist makes the atomic write fail.
        let dir = tempfile::tempdir().unwrap();
        let unwritable = dir.path().join("missing").join("verifier-state.json");
        let error = verifier
            .verify_and_store(&signed_release(), &client(), b"install", &unwritable)
            .unwrap_err();

        assert!(matches!(error, UpdateError::Io(_)), "{error:?}");
        // The watermark did not advance, so the update can be retried once
        // the storage problem is fixed.
        assert_eq!(verifier.state(), before);
        assert!(!unwritable.exists());
    }

    #[test]
    fn verify_and_store_persists_the_watermark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("verifier-state.json");
        let bootstrap = || keyring(None);

        let mut verifier = UpdateVerifier::load(&path, bootstrap).unwrap();
        verifier
            .verify_and_store(&signed_release(), &client(), b"install", &path)
            .unwrap();

        // A restart sees the watermark, so the same release cannot replay.
        let mut reloaded = UpdateVerifier::load(&path, bootstrap).unwrap();
        assert_eq!(reloaded.highest_accepted_sequence(), 10);
        assert_eq!(
            reloaded.verify(&signed_release(), &client(), b"install"),
            Err(UpdateError::DowngradeOrReplay)
        );
    }
}
