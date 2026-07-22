//! Signed evidence that an on-device transform requested no network capability.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::manifest::{PluginCapability, PluginManifest};
use crate::{PluginError, Result};

const SIGNATURE_DOMAIN: &[u8] = b"vbuff-offline-run-v1\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflineRunEvidence {
    pub schema: u16,
    pub manifest_hash: [u8; 32],
    pub model_hash: [u8; 32],
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
    pub attempted_network_calls: u64,
}

impl OfflineRunEvidence {
    pub fn from_manifest(
        manifest: &PluginManifest,
        model_hash: [u8; 32],
        started_at_ms: u64,
        finished_at_ms: u64,
        attempted_network_calls: u64,
    ) -> Result<Self> {
        manifest.validate()?;
        if manifest
            .requested_capabilities
            .contains(&PluginCapability::Network)
            || !manifest.network_hosts.is_empty()
            || attempted_network_calls != 0
            || finished_at_ms < started_at_ms
        {
            return Err(PluginError::CapabilityDenied(
                "offline execution cannot request or attempt network access".into(),
            ));
        }
        let evidence = Self {
            schema: 1,
            manifest_hash: manifest.hash()?,
            model_hash,
            started_at_ms,
            finished_at_ms,
            attempted_network_calls,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| PluginError::Serialization(error.to_string()))
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.attempted_network_calls != 0
            || self.finished_at_ms < self.started_at_ms
        {
            return Err(PluginError::CapabilityDenied(
                "offline execution evidence is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedOfflineAttestation {
    pub evidence: OfflineRunEvidence,
    pub host_public_key: [u8; 32],
    pub signature: Vec<u8>,
}

impl SignedOfflineAttestation {
    pub fn sign(evidence: OfflineRunEvidence, host_key: &SigningKey) -> Result<Self> {
        let signature = host_key
            .sign(&signing_bytes(&evidence)?)
            .to_bytes()
            .to_vec();
        Ok(Self {
            evidence,
            host_public_key: host_key.verifying_key().to_bytes(),
            signature,
        })
    }

    pub fn verify(&self) -> Result<()> {
        self.evidence.validate()?;
        let key = VerifyingKey::from_bytes(&self.host_public_key)
            .map_err(|_| PluginError::InvalidSignature)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| PluginError::InvalidSignature)?;
        key.verify(&signing_bytes(&self.evidence)?, &signature)
            .map_err(|_| PluginError::InvalidSignature)
    }
}

fn signing_bytes(evidence: &OfflineRunEvidence) -> Result<Vec<u8>> {
    let canonical = evidence.canonical_bytes()?;
    let mut bytes = Vec::with_capacity(SIGNATURE_DOMAIN.len() + canonical.len());
    bytes.extend_from_slice(SIGNATURE_DOMAIN);
    bytes.extend_from_slice(&canonical);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::protocol::PROTOCOL_VERSION;

    use super::*;

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "dev.vbuff.local-model".into(),
            name: "Local model".into(),
            version: "1.0.0".into(),
            protocol_version: PROTOCOL_VERSION,
            executable_path: "bin/model".into(),
            requested_capabilities: BTreeSet::from([PluginCapability::ReadClipContent]),
            network_hosts: BTreeSet::new(),
            file_paths: BTreeSet::new(),
            process_commands: BTreeSet::new(),
        }
    }

    #[test]
    fn offline_badge_requires_zero_network_capability_and_calls() {
        let evidence = OfflineRunEvidence::from_manifest(&manifest(), [3; 32], 10, 20, 0).unwrap();
        let signed =
            SignedOfflineAttestation::sign(evidence, &SigningKey::from_bytes(&[8; 32])).unwrap();
        signed.verify().unwrap();
        assert!(OfflineRunEvidence::from_manifest(&manifest(), [3; 32], 10, 20, 1).is_err());
        assert!(OfflineRunEvidence::from_manifest(&manifest(), [3; 32], 20, 10, 0).is_err());
    }
}
