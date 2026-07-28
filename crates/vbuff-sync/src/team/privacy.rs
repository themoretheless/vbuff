use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{invalid, validate_hash, validate_id};
use crate::Result;

const MAX_VARIABLES: usize = 128;
const MAX_VARIABLE_VALUE_BYTES: usize = 4 * 1024;
const MAX_RECEIPTS: usize = 4_096;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadReceiptLedger {
    enabled: bool,
    receipts: BTreeMap<String, i64>,
}

impl ReadReceiptLedger {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            receipts: BTreeMap::new(),
        }
    }

    pub fn record(
        &mut self,
        collection_id: &str,
        item_id: &str,
        member_hash: &[u8; 32],
        read_at_ms: i64,
    ) -> Result<bool> {
        validate_id(collection_id)?;
        validate_id(item_id)?;
        validate_hash(member_hash, "read receipt member hash is invalid")?;
        if read_at_ms < 0 {
            return invalid("read receipt timestamp is invalid");
        }
        if !self.enabled {
            return Ok(false);
        }
        let key = receipt_key(collection_id, item_id, member_hash);
        if !self.receipts.contains_key(&key) && self.receipts.len() >= MAX_RECEIPTS {
            return invalid("read receipt ledger is full");
        }
        self.receipts.insert(key, read_at_ms);
        Ok(true)
    }

    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }
}

impl std::fmt::Debug for ReadReceiptLedger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReadReceiptLedger")
            .field("enabled", &self.enabled)
            .field("receipt_count", &self.receipts.len())
            .finish()
    }
}

fn receipt_key(collection_id: &str, item_id: &str, member_hash: &[u8; 32]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-team-read-receipt-v1\0");
    hasher.update(collection_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(item_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(member_hash);
    hasher.finalize().to_hex().to_string()
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamDefaultDenylist {
    pub source_app_ids: BTreeSet<String>,
    /// Versioned detector identifiers, never captured clip values.
    pub detector_ids: BTreeSet<String>,
}

impl TeamDefaultDenylist {
    pub fn validate(&self) -> Result<()> {
        if self.source_app_ids.len() > 1_024 || self.detector_ids.len() > 1_024 {
            return invalid("team denylist is too large");
        }
        for value in self.source_app_ids.iter().chain(&self.detector_ids) {
            validate_id(value)?;
        }
        Ok(())
    }

    pub fn denies_source(&self, source_app_id: &str) -> Result<bool> {
        self.validate()?;
        validate_id(source_app_id)?;
        Ok(self.source_app_ids.contains(source_app_id))
    }
}

impl std::fmt::Debug for TeamDefaultDenylist {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TeamDefaultDenylist")
            .field("source_app_count", &self.source_app_ids.len())
            .field("detector_count", &self.detector_ids.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedVariableCatalog {
    values: BTreeMap<String, String>,
}

impl SharedVariableCatalog {
    pub fn new(values: BTreeMap<String, String>) -> Result<Self> {
        if values.len() > MAX_VARIABLES {
            return invalid("too many shared variables");
        }
        for (name, value) in &values {
            validate_variable_name(name)?;
            if value.is_empty()
                || value.len() > MAX_VARIABLE_VALUE_BYTES
                || value.chars().any(char::is_control)
            {
                return invalid("invalid shared variable value");
            }
        }
        Ok(Self { values })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn resolve(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }
}

impl std::fmt::Debug for SharedVariableCatalog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedVariableCatalog")
            .field("names", &self.values.keys().collect::<Vec<_>>())
            .field("values", &"[redacted]")
            .finish()
    }
}

fn validate_variable_name(value: &str) -> Result<()> {
    validate_id(value)?;
    if value.starts_with('.') || value.ends_with('.') {
        return invalid("invalid shared variable name");
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamConfigAuditSnapshot {
    pub member_hash: [u8; 32],
    pub policy_hash: [u8; 32],
    pub policy_version: u64,
    pub capture_healthy: bool,
    pub denied_source_count: u32,
    pub unavailable_capability_count: u16,
}

impl TeamConfigAuditSnapshot {
    pub fn validate(&self) -> Result<()> {
        validate_hash(&self.member_hash, "audit member hash is invalid")?;
        validate_hash(&self.policy_hash, "audit policy hash is invalid")?;
        if self.policy_version == 0 {
            return invalid("audit policy version is invalid");
        }
        Ok(())
    }
}

impl std::fmt::Debug for TeamConfigAuditSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TeamConfigAuditSnapshot")
            .field("member_hash", &"[redacted]")
            .field("policy_hash", &"[redacted]")
            .field("policy_version", &self.policy_version)
            .field("capture_healthy", &self.capture_healthy)
            .field("denied_source_count", &self.denied_source_count)
            .field(
                "unavailable_capability_count",
                &self.unavailable_capability_count,
            )
            .finish()
    }
}
