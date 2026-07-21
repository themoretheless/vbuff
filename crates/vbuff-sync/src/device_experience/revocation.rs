use std::fmt;

use serde::{Deserialize, Serialize};

use super::{MAX_DEVICE_ID_BYTES, MAX_PLAN_ITEMS, valid_identifier};
use crate::crypto::{SealedEnvelope, seal_to};
use crate::{Result, SyncError};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationTombstone {
    pub item_hash: [u8; 32],
    pub target_device_hash: [u8; 32],
    pub revoked_epoch: u64,
    pub issued_at_ms: u64,
}

impl fmt::Debug for RevocationTombstone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RevocationTombstone")
            .field("item_hash", &"[redacted]")
            .field("target_device_hash", &"[redacted]")
            .field("revoked_epoch", &self.revoked_epoch)
            .field("issued_at_ms", &self.issued_at_ms)
            .finish()
    }
}

pub fn sealed_revocation_tombstones(
    target_device_id: &str,
    item_ids: &[String],
    revoked_epoch: u64,
    issued_at_ms: u64,
    recipient_public_key: &[u8; 32],
) -> Result<SealedEnvelope> {
    if !valid_identifier(target_device_id, MAX_DEVICE_ID_BYTES)
        || item_ids.is_empty()
        || item_ids.len() > MAX_PLAN_ITEMS
        || item_ids.iter().any(|item| !valid_identifier(item, 128))
    {
        return Err(SyncError::Invalid("invalid revocation plan".into()));
    }
    let target_device_hash = *blake3::hash(target_device_id.as_bytes()).as_bytes();
    let tombstones = item_ids
        .iter()
        .map(|item| RevocationTombstone {
            item_hash: *blake3::hash(item.as_bytes()).as_bytes(),
            target_device_hash,
            revoked_epoch,
            issued_at_ms,
        })
        .collect::<Vec<_>>();
    let payload = serde_json::to_vec(&tombstones)?;
    let aad = format!("vbuff-revoke-v1:{revoked_epoch}:{issued_at_ms}");
    seal_to(recipient_public_key, &payload, aad.as_bytes())
}
