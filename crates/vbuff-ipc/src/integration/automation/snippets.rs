use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{IntegrationContractError, valid_identifier, valid_label};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnippetBridgeCursor {
    pub adapter: String,
    pub source_revision: u64,
    pub target_revision: u64,
    pub last_manifest_hash: [u8; 32],
}

impl fmt::Debug for SnippetBridgeCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnippetBridgeCursor")
            .field("adapter_bytes", &self.adapter.len())
            .field("source_revision", &self.source_revision)
            .field("target_revision", &self.target_revision)
            .field("last_manifest_hash", &"[redacted]")
            .finish()
    }
}

impl SnippetBridgeCursor {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if !valid_identifier(&self.adapter, 64)
            || self.last_manifest_hash.iter().all(|byte| *byte == 0)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }

    pub fn accepts(&self, source_revision: u64, target_revision: u64) -> bool {
        self.validate().is_ok()
            && source_revision >= self.source_revision
            && target_revision >= self.target_revision
            && (source_revision > self.source_revision || target_revision > self.target_revision)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetMirrorRecord {
    pub key: String,
    pub content_hash: [u8; 32],
    pub revision: u64,
}

impl fmt::Debug for SnippetMirrorRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnippetMirrorRecord")
            .field("key", &"[redacted]")
            .field("content_hash", &"[redacted]")
            .field("revision", &self.revision)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnippetMirrorAction {
    UpsertTarget,
    DeleteTarget,
    Conflict,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetMirrorOperation {
    pub key_hash: [u8; 32],
    pub action: SnippetMirrorAction,
    pub source_revision: u64,
    pub target_revision: u64,
}

impl fmt::Debug for SnippetMirrorOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnippetMirrorOperation")
            .field("key_hash", &"[redacted]")
            .field("action", &self.action)
            .field("source_revision", &self.source_revision)
            .field("target_revision", &self.target_revision)
            .finish()
    }
}

pub fn plan_snippet_mirror(
    source: &[SnippetMirrorRecord],
    target: &[SnippetMirrorRecord],
) -> Result<Vec<SnippetMirrorOperation>, IntegrationContractError> {
    if source.len() > 10_000 || target.len() > 10_000 {
        return Err(IntegrationContractError::InvalidField);
    }
    let source = snippet_map(source)?;
    let target = snippet_map(target)?;
    let keys = source
        .keys()
        .chain(target.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut operations = Vec::new();
    for key in keys {
        let left = source.get(&key);
        let right = target.get(&key);
        let (action, source_revision, target_revision) = match (left, right) {
            (Some(left), None) => (SnippetMirrorAction::UpsertTarget, left.revision, 0),
            (None, Some(right)) => (SnippetMirrorAction::DeleteTarget, 0, right.revision),
            (Some(left), Some(right)) if left.content_hash == right.content_hash => continue,
            (Some(left), Some(right)) if left.revision > right.revision => (
                SnippetMirrorAction::UpsertTarget,
                left.revision,
                right.revision,
            ),
            (Some(left), Some(right)) => {
                (SnippetMirrorAction::Conflict, left.revision, right.revision)
            }
            (None, None) => continue,
        };
        operations.push(SnippetMirrorOperation {
            key_hash: *blake3::hash(key.as_bytes()).as_bytes(),
            action,
            source_revision,
            target_revision,
        });
    }
    Ok(operations)
}

fn snippet_map(
    records: &[SnippetMirrorRecord],
) -> Result<BTreeMap<String, SnippetMirrorRecord>, IntegrationContractError> {
    let mut map = BTreeMap::new();
    for record in records {
        if !valid_label(&record.key, 128)
            || record.content_hash.iter().all(|byte| *byte == 0)
            || map.insert(record.key.clone(), record.clone()).is_some()
        {
            return Err(IntegrationContractError::InvalidField);
        }
    }
    Ok(map)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VimRegisterAction {
    ReadHistory,
    AddYank,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VimRegisterRequest {
    pub namespace: String,
    pub slot: u16,
    pub action: VimRegisterAction,
}

impl VimRegisterRequest {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if self.namespace != "vbuff"
            || self.slot > 999
            || (self.action == VimRegisterAction::AddYank && self.slot != 0)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}
