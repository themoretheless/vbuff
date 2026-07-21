use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{MAX_PLAN_ITEMS, all_zero};
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    WaitingRetry,
    Delivered,
    Failed,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub event_id: [u8; 16],
    pub item_hash: [u8; 32],
    pub target_device_hash: [u8; 32],
    pub attempts: u8,
    pub status: OutboxStatus,
    pub next_retry_ms: Option<u64>,
    pub last_error_code: Option<String>,
}

impl fmt::Debug for OutboxEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OutboxEntry")
            .field("event_id", &"[redacted]")
            .field("attempts", &self.attempts)
            .field("status", &self.status)
            .field("next_retry_ms", &self.next_retry_ms)
            .field("last_error_code", &self.last_error_code)
            .finish()
    }
}

#[derive(Clone, Default)]
pub struct SyncOutbox {
    entries: BTreeMap<[u8; 16], OutboxEntry>,
}

impl fmt::Debug for SyncOutbox {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyncOutbox")
            .field("entry_count", &self.entries.len())
            .finish()
    }
}

impl SyncOutbox {
    pub fn enqueue(&mut self, entry: OutboxEntry) -> Result<()> {
        if entry.event_id.iter().all(|byte| *byte == 0)
            || all_zero(&entry.item_hash)
            || all_zero(&entry.target_device_hash)
            || entry.attempts != 0
            || entry.status != OutboxStatus::Pending
            || entry.next_retry_ms.is_some()
            || entry.last_error_code.is_some()
            || self.entries.len() >= MAX_PLAN_ITEMS
            || self.entries.contains_key(&entry.event_id)
        {
            return Err(SyncError::Invalid("invalid outbox entry".into()));
        }
        self.entries.insert(entry.event_id, entry);
        Ok(())
    }

    pub fn record_retry(&mut self, event_id: [u8; 16], at_ms: u64, code: &str) -> Result<()> {
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(SyncError::Invalid("invalid outbox error code".into()));
        }
        let entry = self
            .entries
            .get_mut(&event_id)
            .ok_or_else(|| SyncError::Invalid("outbox event not found".into()))?;
        if !matches!(
            entry.status,
            OutboxStatus::Pending | OutboxStatus::WaitingRetry
        ) {
            return Err(SyncError::Invalid("outbox event cannot be retried".into()));
        }
        entry.attempts = entry.attempts.saturating_add(1);
        entry.status = if entry.attempts >= 8 {
            OutboxStatus::Failed
        } else {
            OutboxStatus::WaitingRetry
        };
        entry.next_retry_ms = (entry.status == OutboxStatus::WaitingRetry).then_some(at_ms);
        entry.last_error_code = Some(code.into());
        Ok(())
    }

    pub fn mark_delivered(&mut self, event_id: [u8; 16]) -> Result<()> {
        let entry = self
            .entries
            .get_mut(&event_id)
            .ok_or_else(|| SyncError::Invalid("outbox event not found".into()))?;
        if !matches!(
            entry.status,
            OutboxStatus::Pending | OutboxStatus::WaitingRetry
        ) {
            return Err(SyncError::Invalid(
                "outbox event cannot be delivered".into(),
            ));
        }
        entry.status = OutboxStatus::Delivered;
        entry.next_retry_ms = None;
        entry.last_error_code = None;
        Ok(())
    }

    pub fn entries(&self) -> impl Iterator<Item = &OutboxEntry> {
        self.entries.values()
    }
}
