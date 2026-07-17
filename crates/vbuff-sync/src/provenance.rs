//! Signed clip chain-of-custody records.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustodyAction {
    Captured,
    Sent,
    Received,
    Pasted,
    Burned,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyEvent {
    pub item_hash: [u8; 32],
    pub action: CustodyAction,
    pub device_id: String,
    pub peer_device: Option<String>,
    pub source_app: Option<String>,
    pub timestamp_ms: u64,
    pub sensitive: bool,
}

impl std::fmt::Debug for CustodyEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CustodyEvent")
            .field("action", &self.action)
            .field("device_id", &self.device_id)
            .field("peer_device", &self.peer_device)
            .field(
                "source_app",
                &self.source_app.as_ref().map(|_| "[redacted]"),
            )
            .field("timestamp_ms", &self.timestamp_ms)
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCustodyEntry {
    pub event: CustodyEvent,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
    pub signer_device: String,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceChain {
    pub entries: Vec<SignedCustodyEntry>,
}

impl ProvenanceChain {
    pub fn append(
        &mut self,
        event: CustodyEvent,
        signer_device: impl Into<String>,
        key: &SigningKey,
    ) -> Result<[u8; 32]> {
        validate_event(&event)?;
        let signer_device = signer_device.into();
        if signer_device != event.device_id {
            return Err(SyncError::Invalid(
                "custody event must be signed by the acting device".into(),
            ));
        }
        let previous_hash = self.entries.last().map_or([0; 32], |entry| entry.hash);
        let hash = event_hash(&event, &previous_hash)?;
        let signature = key.sign(&hash).to_bytes().to_vec();
        self.entries.push(SignedCustodyEntry {
            event,
            previous_hash,
            hash,
            signer_device,
            signature,
        });
        Ok(hash)
    }

    pub fn verify(&self, keys: &BTreeMap<String, [u8; 32]>) -> Result<()> {
        let mut previous_hash = [0_u8; 32];
        for entry in &self.entries {
            validate_event(&entry.event)?;
            if entry.signer_device != entry.event.device_id
                || entry.previous_hash != previous_hash
                || event_hash(&entry.event, &entry.previous_hash)? != entry.hash
            {
                return Err(SyncError::Invalid("custody chain is broken".into()));
            }
            let key = keys
                .get(&entry.signer_device)
                .ok_or_else(|| SyncError::Invalid("unknown custody signer".into()))?;
            let key = VerifyingKey::from_bytes(key).map_err(|_| SyncError::Crypto)?;
            let signature =
                Signature::from_slice(&entry.signature).map_err(|_| SyncError::Crypto)?;
            key.verify(&entry.hash, &signature)
                .map_err(|_| SyncError::Crypto)?;
            previous_hash = entry.hash;
        }
        Ok(())
    }

    pub fn sensitive_left_origin(&self) -> bool {
        self.entries.iter().any(|entry| {
            entry.event.sensitive
                && matches!(
                    entry.event.action,
                    CustodyAction::Sent | CustodyAction::Received
                )
        })
    }
}

fn validate_event(event: &CustodyEvent) -> Result<()> {
    if event.device_id.is_empty()
        || event.device_id.len() > 128
        || event
            .peer_device
            .as_ref()
            .is_some_and(|peer| peer.len() > 128)
        || event.source_app.as_ref().is_some_and(|app| app.len() > 512)
    {
        return Err(SyncError::Invalid("invalid custody event".into()));
    }
    Ok(())
}

fn event_hash(event: &CustodyEvent, previous_hash: &[u8; 32]) -> Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-custody-v1");
    hasher.update(previous_hash);
    hasher.update(&serde_json::to_vec(event)?);
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custody_chain_is_signed_redacted_and_flags_sensitive_travel() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let mut chain = ProvenanceChain::default();
        chain
            .append(
                CustodyEvent {
                    item_hash: [2; 32],
                    action: CustodyAction::Sent,
                    device_id: "laptop".into(),
                    peer_device: Some("phone".into()),
                    source_app: Some("secret.app".into()),
                    timestamp_ms: 10,
                    sensitive: true,
                },
                "laptop",
                &key,
            )
            .unwrap();
        let keys = BTreeMap::from([("laptop".into(), key.verifying_key().to_bytes())]);
        chain.verify(&keys).unwrap();
        assert!(chain.sensitive_left_origin());
        assert!(!format!("{:?}", chain.entries[0].event).contains("secret.app"));
        chain.entries[0].event.timestamp_ms = 11;
        assert!(chain.verify(&keys).is_err());
    }
}
