use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Result, UpdateError};

const MAX_ARTIFACTS: usize = 32;
const MAX_TARGET_LEN: usize = 96;
const MAX_KEY_ID_LEN: usize = 96;
const MAX_ARTIFACT_URL_LEN: usize = 2 * 1024;
const UPDATE_SIGNATURE_DOMAIN: &[u8] = b"vbuff-update-manifest-v1\0";

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
        if self.schema != 1 {
            return Err(UpdateError::InvalidManifest(
                "unsupported manifest schema".into(),
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
            if artifact.target.is_empty()
                || artifact.target.len() > MAX_TARGET_LEN
                || !artifact
                    .target
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
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
    pub fn trust(&mut self, key_id: impl Into<String>, key: TrustedKey) -> Result<()> {
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        VerifyingKey::from_bytes(&key.public_key)
            .map_err(|_| UpdateError::InvalidManifest("trusted key is invalid".into()))?;
        self.keys.insert(key_id, key);
        Ok(())
    }

    pub fn revoke(&mut self, key_id: &str, at_sequence: u64) -> Result<()> {
        let key = self.keys.get_mut(key_id).ok_or(UpdateError::UntrustedKey)?;
        key.revoked_at_sequence = Some(at_sequence);
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
            self.keyring.trust(
                rotation.key_id.clone(),
                TrustedKey {
                    public_key: rotation.public_key,
                    activates_at_sequence: rotation.activates_at_sequence,
                    revoked_at_sequence: None,
                },
            )?;
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
}

fn validate_key_id(key_id: &str) -> Result<()> {
    if key_id.is_empty()
        || key_id.len() > MAX_KEY_ID_LEN
        || !key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(UpdateError::InvalidManifest("key id is invalid".into()));
    }
    Ok(())
}

fn manifest_signing_bytes(key_id: &str, manifest: &UpdateManifest) -> Result<Vec<u8>> {
    validate_key_id(key_id)?;
    let canonical = manifest.canonical_bytes()?;
    let mut bytes =
        Vec::with_capacity(UPDATE_SIGNATURE_DOMAIN.len() + key_id.len() + 1 + canonical.len());
    bytes.extend_from_slice(UPDATE_SIGNATURE_DOMAIN);
    bytes.extend_from_slice(key_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&canonical);
    Ok(bytes)
}

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
        rotating.next_key = Some(KeyRotation {
            key_id: "release-2".into(),
            public_key: second.verifying_key().to_bytes(),
            activates_at_sequence: 11,
        });
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
    }

    #[test]
    fn rollout_is_stable_per_installation_and_sequence() {
        let first = rollout_bucket(b"install-a", 42);
        assert_eq!(first, rollout_bucket(b"install-a", 42));
        assert!(first < 100);
    }
}
