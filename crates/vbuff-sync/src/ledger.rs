//! Signed wipe receipts and a tamper-evident local sync audit ledger.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::clock::HybridLogicalClock;
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    Sent,
    Received,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncLedgerDecision {
    Allowed,
    DeniedByPolicy,
    Applied,
    RejectedEpoch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub item_hash: [u8; 32],
    pub peer_device: String,
    pub direction: SyncDirection,
    pub epoch: u64,
    pub decision: SyncLedgerDecision,
    pub clock: HybridLogicalClock,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedLedgerEntry {
    pub signer_device: String,
    pub event: SyncEvent,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncLedger {
    pub entries: Vec<SignedLedgerEntry>,
}

impl SyncLedger {
    pub fn append(
        &mut self,
        signer_device: impl Into<String>,
        event: SyncEvent,
        signing_key: &SigningKey,
    ) -> Result<[u8; 32]> {
        let signer_device = signer_device.into();
        let previous_hash = self.entries.last().map_or([0; 32], |entry| entry.hash);
        let hash = ledger_hash(&signer_device, &event, &previous_hash)?;
        let signature = signing_key.sign(&hash).to_bytes().to_vec();
        self.entries.push(SignedLedgerEntry {
            signer_device,
            event,
            previous_hash,
            hash,
            signature,
        });
        Ok(hash)
    }

    pub fn verify(&self, keys: &BTreeMap<String, [u8; 32]>) -> Result<()> {
        let mut previous_hash = [0_u8; 32];
        for entry in &self.entries {
            if entry.previous_hash != previous_hash
                || entry.hash
                    != ledger_hash(&entry.signer_device, &entry.event, &entry.previous_hash)?
            {
                return Err(SyncError::Invalid(
                    "sync ledger hash chain is broken".into(),
                ));
            }
            let key_bytes = keys
                .get(&entry.signer_device)
                .ok_or_else(|| SyncError::Invalid("unknown ledger signer".into()))?;
            let key = VerifyingKey::from_bytes(key_bytes).map_err(|_| SyncError::Crypto)?;
            let signature =
                Signature::from_slice(&entry.signature).map_err(|_| SyncError::Crypto)?;
            key.verify(&entry.hash, &signature)
                .map_err(|_| SyncError::Crypto)?;
            previous_hash = entry.hash;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WipeReceipt {
    pub device_id: String,
    pub item_hash: [u8; 32],
    pub epoch: u64,
    pub applied_at_ms: u64,
    pub signature: Vec<u8>,
}

pub fn issue_wipe_receipt(
    device_id: impl Into<String>,
    item_hash: [u8; 32],
    epoch: u64,
    applied_at_ms: u64,
    signing_key: &SigningKey,
) -> Result<WipeReceipt> {
    let mut receipt = WipeReceipt {
        device_id: device_id.into(),
        item_hash,
        epoch,
        applied_at_ms,
        signature: Vec::new(),
    };
    receipt.signature = signing_key
        .sign(&receipt_payload(&receipt)?)
        .to_bytes()
        .to_vec();
    Ok(receipt)
}

pub fn verify_wipe_receipt(receipt: &WipeReceipt, key: &VerifyingKey) -> Result<()> {
    let signature = Signature::from_slice(&receipt.signature).map_err(|_| SyncError::Crypto)?;
    key.verify(&receipt_payload(receipt)?, &signature)
        .map_err(|_| SyncError::Crypto)
}

fn ledger_hash(
    signer_device: &str,
    event: &SyncEvent,
    previous_hash: &[u8; 32],
) -> Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-sync-ledger-v1");
    hasher.update(previous_hash);
    hasher.update(signer_device.as_bytes());
    hasher.update(&serde_json::to_vec(event)?);
    Ok(*hasher.finalize().as_bytes())
}

fn receipt_payload(receipt: &WipeReceipt) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        &receipt.device_id,
        receipt.item_hash,
        receipt.epoch,
        receipt.applied_at_ms,
    ))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> SyncEvent {
        SyncEvent {
            item_hash: [4; 32],
            peer_device: "phone".into(),
            direction: SyncDirection::Sent,
            epoch: 2,
            decision: SyncLedgerDecision::Allowed,
            clock: HybridLogicalClock::new("laptop", 10),
        }
    }

    #[test]
    fn signed_ledger_detects_tampering() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut ledger = SyncLedger::default();
        ledger.append("laptop", event(), &key).unwrap();
        let keys = BTreeMap::from([("laptop".into(), key.verifying_key().to_bytes())]);
        ledger.verify(&keys).unwrap();
        ledger.entries[0].event.epoch = 99;
        assert!(ledger.verify(&keys).is_err());
    }

    #[test]
    fn wipe_receipt_is_bound_to_item_device_and_epoch() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let mut receipt = issue_wipe_receipt("phone", [3; 32], 4, 100, &key).unwrap();
        verify_wipe_receipt(&receipt, &key.verifying_key()).unwrap();
        receipt.epoch = 5;
        assert!(verify_wipe_receipt(&receipt, &key.verifying_key()).is_err());
    }
}
