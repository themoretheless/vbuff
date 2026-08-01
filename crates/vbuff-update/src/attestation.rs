use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use vbuff_types::validation;

use crate::manifest::signing_preimage;
use crate::{Result, UpdateError};

/// Bare label; the NUL terminator is framing and belongs to
/// [`signing_preimage`], which documents the convention.
const ATTESTATION_SIGNATURE_DOMAIN: &str = "vbuff-build-attestation-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildAttestation {
    pub schema: u16,
    pub source_commit: String,
    pub builder_id: String,
    pub source_date_epoch: u64,
    pub artifact_sha256: [u8; 32],
}

impl BuildAttestation {
    fn validate(&self) -> Result<()> {
        let commit_is_hex = (7..=64).contains(&self.source_commit.len())
            && self
                .source_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit());
        if self.schema != 1
            || !commit_is_hex
            || self.builder_id.is_empty()
            || self.builder_id.len() > 256
        {
            return Err(UpdateError::InvalidManifest(
                "build attestation fields are invalid".into(),
            ));
        }
        Ok(())
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| UpdateError::Serialization(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedBuildAttestation {
    pub key_id: String,
    pub attestation: BuildAttestation,
    pub signature: Vec<u8>,
}

impl SignedBuildAttestation {
    pub fn sign(
        key_id: impl Into<String>,
        attestation: BuildAttestation,
        key: &SigningKey,
    ) -> Result<Self> {
        let key_id = key_id.into();
        let signature = key
            .sign(&signing_bytes(&key_id, &attestation)?)
            .to_bytes()
            .to_vec();
        Ok(Self {
            key_id,
            attestation,
            signature,
        })
    }

    pub fn verify(&self, key: &VerifyingKey, artifact: &[u8]) -> Result<()> {
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| UpdateError::InvalidSignature)?;
        key.verify(&signing_bytes(&self.key_id, &self.attestation)?, &signature)
            .map_err(|_| UpdateError::InvalidSignature)?;
        verify_artifact_checksum(artifact, &self.attestation.artifact_sha256)
    }
}

fn signing_bytes(key_id: &str, attestation: &BuildAttestation) -> Result<Vec<u8>> {
    validate_key_id(key_id)?;
    let canonical = attestation.canonical_bytes()?;
    Ok(signing_preimage(
        ATTESTATION_SIGNATURE_DOMAIN,
        &[key_id.as_bytes(), &canonical],
    ))
}

fn validate_key_id(key_id: &str) -> Result<()> {
    if validation::valid_key_id(key_id) {
        Ok(())
    } else {
        Err(UpdateError::InvalidManifest(
            "attestation key id is invalid".into(),
        ))
    }
}

pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn verify_artifact_checksum(artifact: &[u8], expected: &[u8; 32]) -> Result<()> {
    if sha256_bytes(artifact) == *expected {
        Ok(())
    } else {
        Err(UpdateError::ChecksumMismatch)
    }
}

pub fn parse_sha256_hex(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UpdateError::InvalidManifest(
            "SHA-256 must contain exactly 64 hexadecimal characters".into(),
        ));
    }
    let mut hash = [0_u8; 32];
    for (index, slot) in hash.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            UpdateError::InvalidManifest("SHA-256 contains invalid hexadecimal".into())
        })?;
    }
    Ok(hash)
}

pub fn verify_reader_checksum(mut reader: impl Read, expected: &[u8; 32]) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| UpdateError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if actual != *expected {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Byte-for-byte pin of the attestation signing preimage: domain,
    /// terminator, key id, separator, canonical JSON, no trailing terminator.
    /// Any drift here invalidates every attestation already published.
    #[test]
    fn attestation_signing_preimage_is_pinned() {
        let bytes = signing_bytes(
            "release-1",
            &BuildAttestation {
                schema: 1,
                source_commit: "0123456789abcdef".into(),
                builder_id: "https://github.com/vbuff/vbuff/actions".into(),
                source_date_epoch: 1_700_000_000,
                artifact_sha256: [0xab; 32],
            },
        )
        .unwrap();
        assert!(bytes.starts_with(b"vbuff-build-attestation-v1\0release-1\0"));
        assert_eq!(
            hex(&bytes),
            "76627566662d6275696c642d6174746573746174696f6e2d76310072656c65617365\
             2d31007b22736368656d61223a312c22736f757263655f636f6d6d6974223a223031\
             3233343536373839616263646566222c226275696c6465725f6964223a2268747470\
             733a2f2f6769746875622e636f6d2f76627566662f76627566662f616374696f6e73\
             222c22736f757263655f646174655f65706f6368223a313730303030303030302c22\
             61727469666163745f736861323536223a5b3137312c3137312c3137312c3137312c\
             3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137\
             312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c\
             3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137312c3137\
             312c3137312c3137315d7d"
        );
    }

    #[test]
    fn attestation_binds_source_builder_and_artifact() {
        let key = SigningKey::from_bytes(&[5; 32]);
        let artifact = b"release binary";
        let signed = SignedBuildAttestation::sign(
            "release-1",
            BuildAttestation {
                schema: 1,
                source_commit: "0123456789abcdef".into(),
                builder_id: "https://github.com/vbuff/vbuff/actions".into(),
                source_date_epoch: 123,
                artifact_sha256: sha256_bytes(artifact),
            },
            &key,
        )
        .unwrap();

        signed.verify(&key.verifying_key(), artifact).unwrap();
        assert_eq!(
            signed.verify(&key.verifying_key(), b"tampered"),
            Err(UpdateError::ChecksumMismatch)
        );

        let mut rebound = signed;
        rebound.key_id = "release-2".into();
        assert_eq!(
            rebound.verify(&key.verifying_key(), artifact),
            Err(UpdateError::InvalidSignature)
        );
        assert!(SignedBuildAttestation::sign("unsafe/key", rebound.attestation, &key).is_err());
    }

    #[test]
    fn streaming_verifier_accepts_only_exact_hex_and_bytes() {
        let expected = sha256_bytes(b"artifact");
        assert_eq!(parse_sha256_hex(&hex(&expected)).unwrap(), expected);
        assert_eq!(
            verify_reader_checksum(&b"artifact"[..], &expected).unwrap(),
            expected
        );
        assert_eq!(
            verify_reader_checksum(&b"changed"[..], &expected),
            Err(UpdateError::ChecksumMismatch)
        );
        assert!(parse_sha256_hex("abc").is_err());
    }
}
