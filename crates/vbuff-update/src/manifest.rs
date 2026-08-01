use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use url::Url;
use vbuff_types::validation::{self, is_valid_identifier};

use crate::{Result, UpdateError};

const MAX_ARTIFACTS: usize = 32;
const MAX_TARGET_LEN: usize = 96;
const MAX_ARTIFACT_URL_LEN: usize = 2 * 1024;
const UPDATE_SIGNATURE_DOMAIN: &str = "vbuff-update-manifest-v1";
const ROTATION_CONFIRMATION_DOMAIN: &str = "vbuff-update-key-rotation-v1";

/// Builds the byte string that every signature in this crate is computed over.
///
/// Layout, and the written domain convention it encodes:
///
/// ```text
/// domain || 0x00 || parts[0] || 0x00 || parts[1] || … || parts[n-1]
/// ```
///
/// * **The domain is a bare ASCII label, never a NUL-terminated constant.**
///   The terminator belongs to the framing, so it is appended here exactly
///   once. Constants that carried their own `\0` were the reason the
///   convention drifted: a copied constant can silently lose the terminator,
///   and nothing in the type system notices.
/// * **The domain terminator is mandatory**, even when `parts` is empty.
///   Without it `"vbuff-update-manifest-v1"` + `"x"` and
///   `"vbuff-update-manifest-v1x"` + `""` would hash identically, so a
///   signature made under one domain could be replayed under another whose
///   name is a prefix of the first.
/// * **Exactly one `0x00` separates adjacent parts, and there is no trailing
///   terminator** after the final part. This is what the three historical
///   copies did, so the bytes are unchanged; see the pinned tests.
/// * **No length prefixes.** The framing is therefore only unambiguous while
///   every part except the last is NUL-free — the caller's obligation. All
///   current call sites pass a key id validated by
///   `vbuff_types::validation::valid_key_id` (`[A-Za-z0-9._-]`, so NUL-free)
///   followed by a single trailing payload, which may contain anything.
///   `serde_json` output cannot contain a raw NUL in any case, since control
///   characters are escaped. A caller that needs two variable-length,
///   NUL-permitting fields must add explicit length framing rather than lean
///   on this function.
///
/// The debug assertion below is the executable form of that obligation.
///
/// Shared with [`crate::attestation`]; each module keeps its own domain
/// constant next to the payload it covers, and the framing lives only here.
pub(crate) fn signing_preimage(domain: &str, parts: &[&[u8]]) -> Vec<u8> {
    debug_assert!(
        parts
            .split_last()
            .is_none_or(|(_, leading)| leading.iter().all(|part| !part.contains(&0))),
        "only the final signing-preimage part may contain NUL bytes"
    );
    let capacity =
        domain.len() + parts.len().max(1) + parts.iter().map(|part| part.len()).sum::<usize>();
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(domain.as_bytes());
    bytes.push(0);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            bytes.push(0);
        }
        bytes.extend_from_slice(part);
    }
    bytes
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub target: String,
    pub url: String,
    pub sha256: [u8; 32],
    pub byte_size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRotation {
    pub key_id: String,
    pub public_key: [u8; 32],
    pub activates_at_sequence: u64,
    /// Proof-of-possession: Ed25519 signature by the new key over the
    /// domain-separated rotation blob (see `confirmation_bytes`). Without it a
    /// lost or mistyped new key would brick the update channel irrevocably.
    pub confirmation: Vec<u8>,
}

impl KeyRotation {
    /// Build a rotation entry together with a fresh proof-of-possession
    /// signed by `new_key`. `manifest_sequence` is the sequence of the
    /// manifest that will carry this rotation.
    pub fn confirmed(
        key_id: impl Into<String>,
        new_key: &SigningKey,
        activates_at_sequence: u64,
        manifest_sequence: u64,
    ) -> Result<Self> {
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        let public_key = new_key.verifying_key().to_bytes();
        let blob = Self::confirmation_bytes(
            &key_id,
            &public_key,
            activates_at_sequence,
            manifest_sequence,
        )?;
        let confirmation = new_key.sign(&blob).to_bytes().to_vec();
        Ok(Self {
            key_id,
            public_key,
            activates_at_sequence,
            confirmation,
        })
    }

    fn confirmation_bytes(
        key_id: &str,
        public_key: &[u8; 32],
        activates_at_sequence: u64,
        manifest_sequence: u64,
    ) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct RotationPayload<'a> {
            key_id: &'a str,
            public_key: &'a [u8; 32],
            activates_at_sequence: u64,
            manifest_sequence: u64,
        }
        let payload = serde_json::to_vec(&RotationPayload {
            key_id,
            public_key,
            activates_at_sequence,
            manifest_sequence,
        })
        .map_err(|error| UpdateError::Serialization(error.to_string()))?;
        Ok(signing_preimage(
            ROTATION_CONFIRMATION_DOMAIN,
            &[key_id.as_bytes(), &payload],
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub schema: u16,
    pub sequence: u64,
    pub version: Version,
    pub minimum_client: Version,
    pub published_at_ms: u64,
    pub rollout_percent: u8,
    pub artifacts: Vec<Artifact>,
    pub next_key: Option<KeyRotation>,
}

impl UpdateManifest {
    pub fn validate(&self) -> Result<()> {
        // Schema 2 adds proof-of-possession to key rotations. Older clients
        // reject schema 2 outright (fail-closed), so rotation manifests are
        // never applied by clients that cannot confirm the new key.
        if !matches!(self.schema, 1 | 2) {
            return Err(UpdateError::InvalidManifest(
                "unsupported manifest schema".into(),
            ));
        }
        if self.schema == 1 && self.next_key.is_some() {
            return Err(UpdateError::InvalidManifest(
                "key rotation requires manifest schema 2".into(),
            ));
        }
        if self.sequence == 0 {
            return Err(UpdateError::InvalidManifest(
                "release sequence must be non-zero".into(),
            ));
        }
        if self.rollout_percent > 100 {
            return Err(UpdateError::InvalidManifest(
                "rollout percent exceeds 100".into(),
            ));
        }
        if self.artifacts.is_empty() || self.artifacts.len() > MAX_ARTIFACTS {
            return Err(UpdateError::InvalidManifest(
                "artifact count is outside the supported range".into(),
            ));
        }
        let mut targets = BTreeSet::new();
        for artifact in &self.artifacts {
            if !is_valid_identifier(&artifact.target, MAX_TARGET_LEN) {
                return Err(UpdateError::InvalidManifest(
                    "artifact target is invalid".into(),
                ));
            }
            if !targets.insert(&artifact.target) {
                return Err(UpdateError::InvalidManifest(
                    "artifact targets must be unique".into(),
                ));
            }
            let url = Url::parse(&artifact.url)
                .map_err(|_| UpdateError::InvalidManifest("artifact URL is invalid".into()))?;
            if artifact.url.len() > MAX_ARTIFACT_URL_LEN
                || url.scheme() != "https"
                || url.host_str().is_none()
                || artifact.byte_size == 0
            {
                return Err(UpdateError::InvalidManifest(
                    "artifact URL or size is unsafe".into(),
                ));
            }
        }
        if let Some(rotation) = &self.next_key {
            validate_key_id(&rotation.key_id)?;
            if rotation.activates_at_sequence <= self.sequence {
                return Err(UpdateError::InvalidManifest(
                    "rotated key must activate after the signed manifest".into(),
                ));
            }
            VerifyingKey::from_bytes(&rotation.public_key)
                .map_err(|_| UpdateError::InvalidManifest("rotated key is invalid".into()))?;
            if rotation.confirmation.len() != Signature::BYTE_SIZE {
                return Err(UpdateError::InvalidManifest(
                    "rotation confirmation is invalid".into(),
                ));
            }
        }
        Ok(())
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|error| UpdateError::Serialization(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedUpdateManifest {
    pub key_id: String,
    pub manifest: UpdateManifest,
    pub signature: Vec<u8>,
}

impl SignedUpdateManifest {
    pub fn sign(
        key_id: impl Into<String>,
        manifest: UpdateManifest,
        key: &SigningKey,
    ) -> Result<Self> {
        manifest.validate()?;
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        let signature = key
            .sign(&manifest_signing_bytes(&key_id, &manifest)?)
            .to_bytes()
            .to_vec();
        Ok(Self {
            key_id,
            manifest,
            signature,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedKey {
    pub public_key: [u8; 32],
    pub activates_at_sequence: u64,
    pub revoked_at_sequence: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateKeyring {
    keys: BTreeMap<String, TrustedKey>,
}

impl UpdateKeyring {
    /// Trust a new key id. Refuses to overwrite an existing id: key ids are
    /// claimed exactly once, so a compromised active key cannot squat or
    /// replace another trusted key (which would also erase its revocation).
    pub fn trust(&mut self, key_id: impl Into<String>, key: TrustedKey) -> Result<()> {
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        VerifyingKey::from_bytes(&key.public_key)
            .map_err(|_| UpdateError::InvalidManifest("trusted key is invalid".into()))?;
        if self.keys.contains_key(&key_id) {
            return Err(UpdateError::DuplicateKeyId);
        }
        self.keys.insert(key_id, key);
        Ok(())
    }

    /// Revoke a key starting at `at_sequence`. Repeated revocation keeps the
    /// earliest sequence: a later manifest must never resurrect a key by
    /// pushing its revocation into the future.
    pub fn revoke(&mut self, key_id: &str, at_sequence: u64) -> Result<()> {
        let key = self.keys.get_mut(key_id).ok_or(UpdateError::UntrustedKey)?;
        key.revoked_at_sequence = Some(
            key.revoked_at_sequence
                .map_or(at_sequence, |previous| previous.min(at_sequence)),
        );
        Ok(())
    }

    fn active_key(&self, key_id: &str, sequence: u64) -> Result<VerifyingKey> {
        let key = self.keys.get(key_id).ok_or(UpdateError::UntrustedKey)?;
        let active = sequence >= key.activates_at_sequence
            && key
                .revoked_at_sequence
                .is_none_or(|revoked| sequence < revoked);
        if !active {
            return Err(UpdateError::UntrustedKey);
        }
        VerifyingKey::from_bytes(&key.public_key).map_err(|_| UpdateError::UntrustedKey)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedUpdate {
    pub version: Version,
    pub sequence: u64,
    pub eligible_for_rollout: bool,
    pub artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug)]
pub struct UpdateVerifier {
    keyring: UpdateKeyring,
    highest_accepted_sequence: u64,
}

impl UpdateVerifier {
    pub fn new(keyring: UpdateKeyring, highest_accepted_sequence: u64) -> Self {
        Self {
            keyring,
            highest_accepted_sequence,
        }
    }

    pub fn verify(
        &mut self,
        signed: &SignedUpdateManifest,
        current_version: &Version,
        installation_id: &[u8],
    ) -> Result<VerifiedUpdate> {
        signed.manifest.validate()?;
        validate_key_id(&signed.key_id)?;
        let key = self
            .keyring
            .active_key(&signed.key_id, signed.manifest.sequence)?;
        let signature =
            Signature::from_slice(&signed.signature).map_err(|_| UpdateError::InvalidSignature)?;
        key.verify(
            &manifest_signing_bytes(&signed.key_id, &signed.manifest)?,
            &signature,
        )
        .map_err(|_| UpdateError::InvalidSignature)?;

        if signed.manifest.sequence <= self.highest_accepted_sequence
            || signed.manifest.version <= *current_version
        {
            return Err(UpdateError::DowngradeOrReplay);
        }
        if current_version < &signed.manifest.minimum_client {
            return Err(UpdateError::IncompatibleClient);
        }

        if let Some(rotation) = &signed.manifest.next_key {
            // Proof-of-possession: the new key must have signed this exact
            // rotation (key id, public key, activation, manifest sequence).
            // Checked after the manifest signature so an unsigned manifest
            // cannot probe confirmation failures, and before the keyring is
            // mutated so a failed check leaves no trace.
            let confirmation = Signature::from_slice(&rotation.confirmation)
                .map_err(|_| UpdateError::RotationNotConfirmed)?;
            let rotated_key = VerifyingKey::from_bytes(&rotation.public_key)
                .map_err(|_| UpdateError::RotationNotConfirmed)?;
            rotated_key
                .verify(
                    &KeyRotation::confirmation_bytes(
                        &rotation.key_id,
                        &rotation.public_key,
                        rotation.activates_at_sequence,
                        signed.manifest.sequence,
                    )?,
                    &confirmation,
                )
                .map_err(|_| UpdateError::RotationNotConfirmed)?;
            self.keyring.trust(
                rotation.key_id.clone(),
                TrustedKey {
                    public_key: rotation.public_key,
                    activates_at_sequence: rotation.activates_at_sequence,
                    revoked_at_sequence: None,
                },
            )?;
            // Rotation retires the signing key once the new key activates;
            // keeping it valid past that point would defeat the rotation.
            self.keyring
                .revoke(&signed.key_id, rotation.activates_at_sequence)?;
        }
        self.highest_accepted_sequence = signed.manifest.sequence;

        Ok(VerifiedUpdate {
            version: signed.manifest.version.clone(),
            sequence: signed.manifest.sequence,
            eligible_for_rollout: rollout_bucket(installation_id, signed.manifest.sequence)
                < signed.manifest.rollout_percent,
            artifacts: signed.manifest.artifacts.clone(),
        })
    }

    pub fn keyring(&self) -> &UpdateKeyring {
        &self.keyring
    }

    pub fn highest_accepted_sequence(&self) -> u64 {
        self.highest_accepted_sequence
    }
}

fn validate_key_id(key_id: &str) -> Result<()> {
    if !validation::valid_key_id(key_id) {
        return Err(UpdateError::InvalidManifest("key id is invalid".into()));
    }
    Ok(())
}

fn manifest_signing_bytes(key_id: &str, manifest: &UpdateManifest) -> Result<Vec<u8>> {
    validate_key_id(key_id)?;
    let canonical = manifest.canonical_bytes()?;
    Ok(signing_preimage(
        UPDATE_SIGNATURE_DOMAIN,
        &[key_id.as_bytes(), &canonical],
    ))
}

/// Not a signing preimage: this is a stable bucket assignment, and its
/// framing is deliberately its own. The domain carries no terminator and the
/// fixed-width sequence precedes the trailing installation id, which is
/// unambiguous without one. Re-framing it through [`signing_preimage`] would
/// reshuffle every installation's rollout bucket, so it stays as published.
fn rollout_bucket(installation_id: &[u8], sequence: u64) -> u8 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-staged-rollout-v1");
    hasher.update(&sequence.to_be_bytes());
    hasher.update(installation_id);
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    (u64::from_be_bytes(prefix) % 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lower-case hex, so a broken pin prints a diff that can be read and
    /// copied instead of a thousand-element byte-vector dump.
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Fixture shared by the two manifest preimage pins. Deliberately built
    /// here rather than via `manifest()`: the pinned bytes must not move when
    /// an unrelated test tweaks that helper.
    fn pinned_manifest() -> UpdateManifest {
        UpdateManifest {
            schema: 2,
            sequence: 7,
            version: Version::parse("1.2.3").unwrap(),
            minimum_client: Version::parse("1.0.0").unwrap(),
            published_at_ms: 1_700_000_000_000,
            rollout_percent: 50,
            artifacts: vec![Artifact {
                target: "aarch64-apple-darwin".into(),
                url: "https://releases.vbuff.dev/vbuff".into(),
                sha256: [0xab; 32],
                byte_size: 1024,
            }],
            next_key: None,
        }
    }

    fn manifest(sequence: u64, version: &str) -> UpdateManifest {
        UpdateManifest {
            schema: 1,
            sequence,
            version: Version::parse(version).unwrap(),
            minimum_client: Version::parse("0.1.0").unwrap(),
            published_at_ms: 100,
            rollout_percent: 25,
            artifacts: vec![Artifact {
                target: "aarch64-apple-darwin".into(),
                url: "https://releases.vbuff.dev/vbuff".into(),
                sha256: [3; 32],
                byte_size: 42,
            }],
            next_key: None,
        }
    }

    fn verifier(key: &SigningKey) -> UpdateVerifier {
        let mut keyring = UpdateKeyring::default();
        keyring
            .trust(
                "release-1",
                TrustedKey {
                    public_key: key.verifying_key().to_bytes(),
                    activates_at_sequence: 1,
                    revoked_at_sequence: None,
                },
            )
            .unwrap();
        UpdateVerifier::new(keyring, 0)
    }

    #[test]
    fn signed_manifest_rejects_tampering_wrong_key_and_replay() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let signed = SignedUpdateManifest::sign("release-1", manifest(10, "0.2.0"), &key).unwrap();
        let current = Version::parse("0.1.0").unwrap();

        let mut valid = verifier(&key);
        valid.verify(&signed, &current, b"install-a").unwrap();
        assert_eq!(
            valid.verify(&signed, &current, b"install-a"),
            Err(UpdateError::DowngradeOrReplay)
        );

        let mut tampered = signed.clone();
        tampered.manifest.version = Version::parse("9.0.0").unwrap();
        assert_eq!(
            verifier(&key).verify(&tampered, &current, b"install-a"),
            Err(UpdateError::InvalidSignature)
        );

        let wrong = SigningKey::from_bytes(&[8; 32]);
        assert_eq!(
            verifier(&wrong).verify(&signed, &current, b"install-a"),
            Err(UpdateError::InvalidSignature)
        );

        let mut rebound = signed;
        rebound.key_id = "release-alias".into();
        let mut alias_verifier = verifier(&key);
        alias_verifier
            .keyring
            .trust(
                "release-alias",
                TrustedKey {
                    public_key: key.verifying_key().to_bytes(),
                    activates_at_sequence: 1,
                    revoked_at_sequence: None,
                },
            )
            .unwrap();
        assert_eq!(
            alias_verifier.verify(&rebound, &current, b"install-a"),
            Err(UpdateError::InvalidSignature)
        );
    }

    #[test]
    fn signed_rotation_only_activates_for_future_sequences() {
        let first = SigningKey::from_bytes(&[1; 32]);
        let second = SigningKey::from_bytes(&[2; 32]);
        let mut rotating = manifest(10, "0.2.0");
        rotating.schema = 2;
        rotating.next_key = Some(KeyRotation::confirmed("release-2", &second, 11, 10).unwrap());
        let signed = SignedUpdateManifest::sign("release-1", rotating, &first).unwrap();
        let mut verifier = verifier(&first);
        verifier
            .verify(&signed, &Version::parse("0.1.0").unwrap(), b"install")
            .unwrap();

        let next = SignedUpdateManifest::sign("release-2", manifest(11, "0.3.0"), &second).unwrap();
        assert!(
            verifier
                .verify(&next, &Version::parse("0.2.0").unwrap(), b"install")
                .is_ok()
        );

        // The rotation retired the previous key at the activation sequence.
        let stale = SignedUpdateManifest::sign("release-1", manifest(12, "0.4.0"), &first).unwrap();
        assert_eq!(
            verifier.verify(&stale, &Version::parse("0.3.0").unwrap(), b"install"),
            Err(UpdateError::UntrustedKey)
        );
    }

    #[test]
    fn rotated_key_is_rejected_before_activation() {
        let first = SigningKey::from_bytes(&[1; 32]);
        let second = SigningKey::from_bytes(&[2; 32]);
        let mut rotating = manifest(10, "0.2.0");
        rotating.schema = 2;
        rotating.next_key = Some(KeyRotation::confirmed("release-2", &second, 12, 10).unwrap());
        let signed = SignedUpdateManifest::sign("release-1", rotating, &first).unwrap();
        let mut verifier = verifier(&first);
        verifier
            .verify(&signed, &Version::parse("0.1.0").unwrap(), b"install")
            .unwrap();

        // Past the watermark but before activation: the new key is not live.
        let early =
            SignedUpdateManifest::sign("release-2", manifest(11, "0.3.0"), &second).unwrap();
        assert_eq!(
            verifier.verify(&early, &Version::parse("0.2.0").unwrap(), b"install"),
            Err(UpdateError::UntrustedKey)
        );

        let activated =
            SignedUpdateManifest::sign("release-2", manifest(12, "0.3.0"), &second).unwrap();
        assert!(
            verifier
                .verify(&activated, &Version::parse("0.2.0").unwrap(), b"install")
                .is_ok()
        );
    }

    #[test]
    fn trust_rejects_duplicate_key_id_without_overwriting() {
        let first = SigningKey::from_bytes(&[1; 32]);
        let second = SigningKey::from_bytes(&[2; 32]);
        let mut keyring = UpdateKeyring::default();
        let original = TrustedKey {
            public_key: first.verifying_key().to_bytes(),
            activates_at_sequence: 1,
            revoked_at_sequence: Some(9),
        };
        keyring.trust("release-1", original.clone()).unwrap();
        let overwrite = TrustedKey {
            public_key: second.verifying_key().to_bytes(),
            activates_at_sequence: 1,
            revoked_at_sequence: None,
        };
        assert_eq!(
            keyring.trust("release-1", overwrite),
            Err(UpdateError::DuplicateKeyId)
        );
        assert_eq!(keyring.keys.get("release-1"), Some(&original));
    }

    #[test]
    fn revoke_keeps_earliest_sequence() {
        let key = SigningKey::from_bytes(&[1; 32]);
        let mut keyring = UpdateKeyring::default();
        keyring
            .trust(
                "release-1",
                TrustedKey {
                    public_key: key.verifying_key().to_bytes(),
                    activates_at_sequence: 1,
                    revoked_at_sequence: None,
                },
            )
            .unwrap();
        keyring.revoke("release-1", 20).unwrap();
        keyring.revoke("release-1", 12).unwrap();
        assert_eq!(
            keyring.keys.get("release-1").unwrap().revoked_at_sequence,
            Some(12)
        );
        keyring.revoke("release-1", 15).unwrap();
        assert_eq!(
            keyring.keys.get("release-1").unwrap().revoked_at_sequence,
            Some(12)
        );
    }

    #[test]
    fn compromised_key_cannot_squat_another_trusted_key_id() {
        let compromised = SigningKey::from_bytes(&[9; 32]);
        let victim = SigningKey::from_bytes(&[5; 32]);
        let replacement = SigningKey::from_bytes(&[6; 32]);
        let mut keyring = UpdateKeyring::default();
        keyring
            .trust(
                "compromised",
                TrustedKey {
                    public_key: compromised.verifying_key().to_bytes(),
                    activates_at_sequence: 1,
                    revoked_at_sequence: None,
                },
            )
            .unwrap();
        keyring
            .trust(
                "root",
                TrustedKey {
                    public_key: victim.verifying_key().to_bytes(),
                    activates_at_sequence: 1,
                    revoked_at_sequence: None,
                },
            )
            .unwrap();
        let mut verifier = UpdateVerifier::new(keyring, 0);

        // The compromised key publishes a rotation that claims the victim's
        // key id. The confirmation is valid (the attacker owns the
        // replacement key); only the duplicate-id refusal stops it.
        let mut attack = manifest(10, "0.2.0");
        attack.schema = 2;
        attack.next_key = Some(KeyRotation::confirmed("root", &replacement, 11, 10).unwrap());
        let attack = SignedUpdateManifest::sign("compromised", attack, &compromised).unwrap();
        assert_eq!(
            verifier.verify(&attack, &Version::parse("0.1.0").unwrap(), b"install"),
            Err(UpdateError::DuplicateKeyId)
        );

        // The failed manifest left no trace: the victim can still ship.
        let legit = SignedUpdateManifest::sign("root", manifest(10, "0.2.0"), &victim).unwrap();
        assert!(
            verifier
                .verify(&legit, &Version::parse("0.1.0").unwrap(), b"install")
                .is_ok()
        );
    }

    #[test]
    fn schema_one_rejects_key_rotation() {
        let second = SigningKey::from_bytes(&[2; 32]);
        let mut rotating = manifest(10, "0.2.0");
        rotating.next_key = Some(KeyRotation::confirmed("release-2", &second, 11, 10).unwrap());
        assert!(matches!(
            rotating.validate(),
            Err(UpdateError::InvalidManifest(_))
        ));
    }

    #[test]
    fn schema_two_requires_confirmation() {
        let second = SigningKey::from_bytes(&[2; 32]);
        let mut rotating = manifest(10, "0.2.0");
        rotating.schema = 2;
        rotating.next_key = Some(KeyRotation {
            key_id: "release-2".into(),
            public_key: second.verifying_key().to_bytes(),
            activates_at_sequence: 11,
            confirmation: Vec::new(),
        });
        assert!(matches!(
            rotating.validate(),
            Err(UpdateError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rotation_confirmation_by_an_unrelated_key_is_rejected() {
        let first = SigningKey::from_bytes(&[1; 32]);
        let second = SigningKey::from_bytes(&[2; 32]);
        let third = SigningKey::from_bytes(&[3; 32]);
        let mut rotating = manifest(10, "0.2.0");
        rotating.schema = 2;
        let mut rotation = KeyRotation::confirmed("release-2", &second, 11, 10).unwrap();
        // Re-sign the exact rotation blob with a key that is not the new key.
        let blob = KeyRotation::confirmation_bytes(
            "release-2",
            &second.verifying_key().to_bytes(),
            11,
            10,
        )
        .unwrap();
        rotation.confirmation = third.sign(&blob).to_bytes().to_vec();
        rotating.next_key = Some(rotation);
        let signed = SignedUpdateManifest::sign("release-1", rotating, &first).unwrap();
        assert_eq!(
            verifier(&first).verify(&signed, &Version::parse("0.1.0").unwrap(), b"install"),
            Err(UpdateError::RotationNotConfirmed)
        );
    }

    #[test]
    fn tampered_confirmation_breaks_the_manifest_signature() {
        let first = SigningKey::from_bytes(&[1; 32]);
        let second = SigningKey::from_bytes(&[2; 32]);
        let mut rotating = manifest(10, "0.2.0");
        rotating.schema = 2;
        rotating.next_key = Some(KeyRotation::confirmed("release-2", &second, 11, 10).unwrap());
        let mut signed = SignedUpdateManifest::sign("release-1", rotating, &first).unwrap();
        // The confirmation is part of the signed canonical manifest, so
        // flipping it after signing invalidates the manifest signature.
        signed.manifest.next_key.as_mut().unwrap().confirmation[0] ^= 1;
        assert_eq!(
            verifier(&first).verify(&signed, &Version::parse("0.1.0").unwrap(), b"install"),
            Err(UpdateError::InvalidSignature)
        );
    }

    /// Pins the framing itself. Every byte below is load-bearing: the domain
    /// terminator, the single separator between parts, and the *absence* of a
    /// trailing terminator. Changing any of them silently invalidates every
    /// signature ever issued by this crate.
    #[test]
    fn signing_preimage_framing_is_pinned() {
        // "dom" 0x00 "a" 0x00 "bc"
        assert_eq!(
            hex(&signing_preimage("dom", &[b"a", b"bc"])),
            "646f6d0061006263"
        );
        // A lone part is terminated by the domain separator only.
        assert_eq!(hex(&signing_preimage("dom", &[b"a"])), "646f6d0061");
        // The domain terminator is emitted even with nothing to separate.
        assert_eq!(hex(&signing_preimage("dom", &[])), "646f6d00");
        // Prefix-domain confusion is what the terminator buys: without it
        // both of these would be the same byte string.
        assert_ne!(
            signing_preimage("dom", &[b"ain"]),
            signing_preimage("domain", &[b""])
        );
        // Empty parts still consume a separator, so the split stays unique.
        assert_eq!(hex(&signing_preimage("d", &[b"", b"x"])), "64000078");
    }

    /// Byte-for-byte pin of the manifest signing preimage. These are the
    /// bytes released manifests were signed over; they must never move.
    #[test]
    fn manifest_signing_preimage_is_pinned() {
        let bytes = manifest_signing_bytes("release-1", &pinned_manifest()).unwrap();
        assert!(bytes.starts_with(b"vbuff-update-manifest-v1\0release-1\0"));
        assert_eq!(
            hex(&bytes),
            "76627566662d7570646174652d6d616e69666573742d76310072656c656173652d31\
             007b22736368656d61223a322c2273657175656e6365223a372c2276657273696f6e\
             223a22312e322e33222c226d696e696d756d5f636c69656e74223a22312e302e3022\
             2c227075626c69736865645f61745f6d73223a313730303030303030303030302c22\
             726f6c6c6f75745f70657263656e74223a35302c22617274696661637473223a5b7b\
             22746172676574223a22616172636836342d6170706c652d64617277696e222c2275\
             726c223a2268747470733a2f2f72656c65617365732e76627566662e6465762f7662\
             756666222c22736861323536223a5b3137312c3137312c3137312c3137312c313731\
             2c3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c31\
             37312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c313731\
             2c3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c31\
             37312c3137315d2c22627974655f73697a65223a313032347d5d2c226e6578745f6b\
             6579223a6e756c6c7d"
        );
    }

    /// Byte-for-byte pin of the key-rotation proof-of-possession preimage.
    #[test]
    fn rotation_confirmation_preimage_is_pinned() {
        let bytes = KeyRotation::confirmation_bytes("release-2", &[0xcd; 32], 11, 7).unwrap();
        assert!(bytes.starts_with(b"vbuff-update-key-rotation-v1\0release-2\0"));
        assert_eq!(
            hex(&bytes),
            "76627566662d7570646174652d6b65792d726f746174696f6e2d76310072656c6561\
             73652d32007b226b65795f6964223a2272656c656173652d32222c227075626c6963\
             5f6b6579223a5b3230352c3230352c3230352c3230352c3230352c3230352c323035\
             2c3230352c3230352c3230352c3230352c3230352c3230352c3230352c3230352c32\
             30352c3230352c3230352c3230352c3230352c3230352c3230352c3230352c323035\
             2c3230352c3230352c3230352c3230352c3230352c3230352c3230352c3230355d2c\
             226163746976617465735f61745f73657175656e6365223a31312c226d616e696665\
             73745f73657175656e6365223a377d"
        );
    }

    #[test]
    fn rollout_is_stable_per_installation_and_sequence() {
        let first = rollout_bucket(b"install-a", 42);
        assert_eq!(first, rollout_bucket(b"install-a", 42));
        assert!(first < 100);
    }
}
