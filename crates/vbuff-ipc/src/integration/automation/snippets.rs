use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use vbuff_types::validation::{all_zero, is_valid_identifier, is_valid_label};

use super::IntegrationContractError;

/// Domain separator for the manifest content hash (versioned).
const SNIPPET_MANIFEST_HASH_DOMAIN: &[u8] = b"vbuff-snippet-manifest-v1\0";
/// Fail-closed bound for manifest entries, mirrored from `plan_snippet_mirror`.
const MAX_SNIPPET_MANIFEST_ENTRIES: usize = 10_000;

/// Last synchronized state of one snippet key. `Deleted` is a tombstone: it
/// proves the key was removed in a previous cycle, which is what allows the
/// planner to distinguish a safe auto-delete from a diverged edit.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnippetSyncedState {
    Present { content_hash: [u8; 32] },
    Deleted,
}

impl fmt::Debug for SnippetSyncedState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Present { .. } => {
                formatter.write_str("SnippetSyncedState::Present { content_hash: [redacted] }")
            }
            Self::Deleted => formatter.write_str("SnippetSyncedState::Deleted"),
        }
    }
}

/// Point-in-time record of what both snippet sides agreed on after a sync
/// cycle. Acts as the causal base for `plan_snippet_mirror`: `DeleteTarget`
/// and `UpsertTarget` are only planned when the target side is provably
/// unchanged relative to this manifest.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnippetSyncManifest {
    pub entries: BTreeMap<String, SnippetSyncedState>,
}

impl fmt::Debug for SnippetSyncManifest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnippetSyncManifest")
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl SnippetSyncManifest {
    /// Deterministic blake3 over the canonical serialization (sorted
    /// `BTreeMap`, length-prefixed keys) under a versioned domain separator.
    pub fn compute_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SNIPPET_MANIFEST_HASH_DOMAIN);
        hasher.update(&(self.entries.len() as u64).to_le_bytes());
        for (key, state) in &self.entries {
            hasher.update(&(key.len() as u64).to_le_bytes());
            hasher.update(key.as_bytes());
            match state {
                SnippetSyncedState::Present { content_hash } => {
                    hasher.update(&[0x00]);
                    hasher.update(content_hash);
                }
                SnippetSyncedState::Deleted => {
                    hasher.update(&[0x01]);
                }
            }
        }
        *hasher.finalize().as_bytes()
    }

    /// Bounded validation: at most 10 000 entries, keys follow the same
    /// `is_valid_label(.., 128)` rules as `snippet_map`, and a `Present` state
    /// never carries the zero hash (same rule as `snippet_map` records).
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if self.entries.len() > MAX_SNIPPET_MANIFEST_ENTRIES {
            return Err(IntegrationContractError::InvalidField);
        }
        for (key, state) in &self.entries {
            if !is_valid_label(key, 128) {
                return Err(IntegrationContractError::InvalidField);
            }
            if let SnippetSyncedState::Present { content_hash } = state
                && all_zero(content_hash)
            {
                return Err(IntegrationContractError::InvalidField);
            }
        }
        Ok(())
    }

    /// Documented update contract after a planned batch was applied:
    /// `UpsertTarget` records `Present { hash from source }`, `DeleteTarget`
    /// records a `Deleted` tombstone, `Conflict` leaves the entry unchanged.
    /// The caller then recomputes `cursor.last_manifest_hash =
    /// next.compute_hash()`; this helper never mutates the cursor itself.
    /// Entries referenced by an operation but missing from the given
    /// source/target records are rejected fail-closed.
    pub fn applied(
        &self,
        operations: &[SnippetMirrorOperation],
        source: &[SnippetMirrorRecord],
        target: &[SnippetMirrorRecord],
    ) -> Result<SnippetSyncManifest, IntegrationContractError> {
        if operations.len() > MAX_SNIPPET_MANIFEST_ENTRIES {
            return Err(IntegrationContractError::InvalidField);
        }
        let source = snippet_map(source)?;
        let target = snippet_map(target)?;
        let mut keys_by_hash = BTreeMap::new();
        for record in source.values().chain(target.values()) {
            keys_by_hash.insert(
                *blake3::hash(record.key.as_bytes()).as_bytes(),
                record.key.as_str(),
            );
        }
        let mut next = self.clone();
        for operation in operations {
            match operation.action {
                SnippetMirrorAction::UpsertTarget => {
                    let key = keys_by_hash
                        .get(&operation.key_hash)
                        .and_then(|key| source.get(*key))
                        .ok_or(IntegrationContractError::InvalidField)?;
                    next.entries.insert(
                        key.key.clone(),
                        SnippetSyncedState::Present {
                            content_hash: key.content_hash,
                        },
                    );
                }
                SnippetMirrorAction::DeleteTarget => {
                    let key = keys_by_hash
                        .get(&operation.key_hash)
                        .ok_or(IntegrationContractError::InvalidField)?;
                    next.entries
                        .insert((*key).to_owned(), SnippetSyncedState::Deleted);
                }
                SnippetMirrorAction::Conflict => {}
            }
        }
        next.validate()?;
        Ok(next)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnippetBridgeCursor {
    pub adapter: String,
    pub source_revision: u64,
    pub target_revision: u64,
    pub last_manifest_hash: [u8; 32],
    /// Causal base mirrored from the last applied cycle. Defaults to empty so
    /// legacy cursors without this field still deserialize (and are then
    /// distrusted until their hash is recomputed by the caller).
    #[serde(default)]
    pub manifest: SnippetSyncManifest,
}

impl fmt::Debug for SnippetBridgeCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnippetBridgeCursor")
            .field("adapter_bytes", &self.adapter.len())
            .field("source_revision", &self.source_revision)
            .field("target_revision", &self.target_revision)
            .field("last_manifest_hash", &"[redacted]")
            .field("manifest_entries", &self.manifest.entries.len())
            .finish()
    }
}

impl SnippetBridgeCursor {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if !is_valid_identifier(&self.adapter, 64) || all_zero(&self.last_manifest_hash) {
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

    /// Trusted causal base for the planner: `Some` only when the embedded
    /// manifest validates and its hash matches `last_manifest_hash`. Any
    /// mismatch (tampered, stale, or legacy empty manifest) is fail-closed
    /// treated as "no base", which degrades the plan to conflicts.
    pub fn trusted_manifest(&self) -> Option<&SnippetSyncManifest> {
        if self.manifest.validate().is_ok() && self.manifest.compute_hash() == self.last_manifest_hash
        {
            Some(&self.manifest)
        } else {
            None
        }
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

/// Plans a one-way snippet mirror (source → target) against a causal base.
///
/// `base` must come from `SnippetBridgeCursor::trusted_manifest`; any other
/// provenance voids the safety argument below. Invariant: `DeleteTarget` and
/// `UpsertTarget` are planned only when the target side is provably unchanged
/// relative to the base; on any doubt the plan is `Conflict`. Revisions are
/// kept on operations for observability only and never pick a winner — the
/// two sides' revision counters are independent, not causal.
///
/// Honest degradation note: without a trusted base (first run, legacy cursor,
/// tampered manifest) every diverged or target-only key plans `Conflict`.
/// The noise is intentional fail-closed behavior and disappears after the
/// first successfully applied cycle rebuilds the manifest.
pub fn plan_snippet_mirror(
    source: &[SnippetMirrorRecord],
    target: &[SnippetMirrorRecord],
    base: Option<&SnippetSyncManifest>,
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
        let synced = base.and_then(|base| base.entries.get(&key));
        let (action, source_revision, target_revision) = match (left, right) {
            (Some(left), None) => {
                let action = match synced {
                    // The key existed in the base and vanished from target:
                    // edit-vs-delete divergence, never silently resurrected.
                    Some(SnippetSyncedState::Present { .. }) => SnippetMirrorAction::Conflict,
                    // Re-add after a tombstone or a brand-new key: target is
                    // provably unchanged (it holds nothing), so nothing is
                    // overwritten by the upsert.
                    Some(SnippetSyncedState::Deleted) | None => {
                        SnippetMirrorAction::UpsertTarget
                    }
                };
                (action, left.revision, 0)
            }
            (None, Some(right)) => {
                let action = match synced {
                    // The only automatic delete: the tombstone path proves
                    // target still holds exactly what the base recorded.
                    Some(SnippetSyncedState::Present { content_hash })
                        if *content_hash == right.content_hash =>
                    {
                        SnippetMirrorAction::DeleteTarget
                    }
                    // Anything else target-only (edited target, tombstoned or
                    // unknown key, no base) is a conflict, never a delete.
                    _ => SnippetMirrorAction::Conflict,
                };
                (action, 0, right.revision)
            }
            (Some(left), Some(right)) if left.content_hash == right.content_hash => continue,
            (Some(left), Some(right)) => {
                let action = match synced {
                    // Target unchanged relative to the base: safe to push the
                    // source edit.
                    Some(SnippetSyncedState::Present { content_hash })
                        if *content_hash == right.content_hash =>
                    {
                        SnippetMirrorAction::UpsertTarget
                    }
                    // Source unchanged while target edited, both edited, or no
                    // base record: reverse sync is deliberately not planned,
                    // so this is a conflict.
                    _ => SnippetMirrorAction::Conflict,
                };
                (action, left.revision, right.revision)
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
        if !is_valid_label(&record.key, 128)
            || all_zero(&record.content_hash)
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
