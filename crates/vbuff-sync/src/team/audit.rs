use serde::{Deserialize, Serialize};

use super::{invalid, validate_hash};
use crate::Result;

const MAX_CHANGE_ENTRIES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionChangeKind {
    Role,
    Policy,
    Metadata,
    SnippetRevision,
    PluginApproval,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionChange {
    pub sequence: u64,
    pub actor_hash: [u8; 32],
    pub kind: CollectionChangeKind,
    pub subject_hash: [u8; 32],
    pub before_hash: Option<[u8; 32]>,
    pub after_hash: [u8; 32],
    pub changed_at_ms: i64,
}

impl std::fmt::Debug for CollectionChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollectionChange")
            .field("sequence", &self.sequence)
            .field("actor_hash", &"[redacted]")
            .field("kind", &self.kind)
            .field("subject_hash", &"[redacted]")
            .field("before_hash", &self.before_hash.map(|_| "[redacted]"))
            .field("after_hash", &"[redacted]")
            .field("changed_at_ms", &self.changed_at_ms)
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionChangelog {
    entries: Vec<CollectionChange>,
}

impl CollectionChangelog {
    pub fn append(&mut self, change: CollectionChange) -> Result<()> {
        if self.entries.len() >= MAX_CHANGE_ENTRIES {
            return invalid("collection changelog is full");
        }
        if change.changed_at_ms < 0 {
            return invalid("collection change timestamp is invalid");
        }
        validate_hash(
            &change.actor_hash,
            "collection change actor hash is invalid",
        )?;
        validate_hash(
            &change.subject_hash,
            "collection change subject hash is invalid",
        )?;
        if let Some(before_hash) = &change.before_hash {
            validate_hash(before_hash, "collection change before hash is invalid")?;
        }
        validate_hash(
            &change.after_hash,
            "collection change after hash is invalid",
        )?;
        let expected = match self.entries.last() {
            Some(entry) => entry.sequence.checked_add(1).ok_or_else(|| {
                crate::SyncError::Invalid("collection change sequence overflow".into())
            })?,
            None => 1,
        };
        if change.sequence != expected {
            return invalid("collection change sequence is not contiguous");
        }
        self.entries.push(change);
        Ok(())
    }

    pub fn entries(&self) -> &[CollectionChange] {
        &self.entries
    }
}

impl std::fmt::Debug for CollectionChangelog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollectionChangelog")
            .field("entry_count", &self.entries.len())
            .finish()
    }
}
