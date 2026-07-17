//! Read-only migration preflight, live checkpoints, and reversible import journals.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use vbuff_types::{ClipId, ContentKind};

use crate::adapter::ImportRecord;
use crate::{PluginError, Result};

const MAX_MIGRATION_RECORDS: usize = 1_000_000;
const MAX_SOURCE_ID_LEN: usize = 512;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub source_id: String,
    pub revision: u64,
    pub record: ImportRecord,
    pub pinned: bool,
    pub snippet: bool,
}

impl std::fmt::Debug for MigrationRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MigrationRecord")
            .field("source_id", &"[redacted]")
            .field("revision", &self.revision)
            .field("record", &self.record)
            .field("pinned", &self.pinned)
            .field("snippet", &self.snippet)
            .finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationPreflight {
    pub records: usize,
    pub pins: usize,
    pub snippets: usize,
    pub images: usize,
    pub likely_secrets: usize,
    pub total_bytes: u64,
    pub unsupported_records: usize,
    pub kinds: BTreeMap<ContentKind, usize>,
}

impl MigrationPreflight {
    pub fn scan(records: &[MigrationRecord]) -> Result<Self> {
        if records.len() > MAX_MIGRATION_RECORDS {
            return Err(PluginError::InvalidInput(
                "migration record count exceeds the preflight limit".into(),
            ));
        }
        let mut summary = Self::default();
        let mut source_ids = BTreeSet::new();
        for migration in records {
            validate_source_id(&migration.source_id)?;
            if !source_ids.insert(&migration.source_id) {
                return Err(PluginError::InvalidInput(
                    "migration source ids must be unique per scan".into(),
                ));
            }
            summary.records = summary.records.saturating_add(1);
            summary.pins = summary.pins.saturating_add(usize::from(migration.pinned));
            summary.snippets = summary
                .snippets
                .saturating_add(usize::from(migration.snippet));
            let kind = migration.record.kind_hint.unwrap_or(ContentKind::Other);
            *summary.kinds.entry(kind).or_default() += 1;
            summary.images = summary
                .images
                .saturating_add(usize::from(kind == ContentKind::Image));
            let mut saw_secret = false;
            for flavor in &migration.record.flavors {
                summary.total_bytes = summary.total_bytes.saturating_add(flavor.body.byte_size());
                if !saw_secret
                    && flavor
                        .as_text()
                        .is_some_and(|text| !vbuff_core::secret::detect_secrets(text).is_empty())
                {
                    saw_secret = true;
                }
            }
            summary.likely_secrets = summary
                .likely_secrets
                .saturating_add(usize::from(saw_secret));
            summary.unsupported_records = summary
                .unsupported_records
                .saturating_add(usize::from(migration.record.flavors.is_empty()));
        }
        Ok(summary)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveMigrationTracker {
    revisions: BTreeMap<String, u64>,
}

impl LiveMigrationTracker {
    pub fn changed<'a>(&self, records: &'a [MigrationRecord]) -> Result<Vec<&'a MigrationRecord>> {
        if records.len() > MAX_MIGRATION_RECORDS {
            return Err(PluginError::InvalidInput(
                "migration poll exceeds the record limit".into(),
            ));
        }
        let mut seen = BTreeSet::new();
        let mut changed = Vec::new();
        for record in records {
            validate_source_id(&record.source_id)?;
            if !seen.insert(&record.source_id) {
                return Err(PluginError::InvalidInput(
                    "duplicate source id in migration poll".into(),
                ));
            }
            if self
                .revisions
                .get(&record.source_id)
                .is_none_or(|revision| record.revision > *revision)
            {
                changed.push(record);
            }
        }
        changed.sort_by(|left, right| left.source_id.cmp(&right.source_id));
        Ok(changed)
    }

    pub fn checkpoint(&mut self, imported: &[MigrationRecord]) -> Result<()> {
        if imported.len() > MAX_MIGRATION_RECORDS {
            return Err(PluginError::InvalidInput(
                "migration checkpoint exceeds the record limit".into(),
            ));
        }
        let mut seen = BTreeSet::new();
        for record in imported {
            validate_source_id(&record.source_id)?;
            if !seen.insert(&record.source_id) {
                return Err(PluginError::InvalidInput(
                    "duplicate source id in migration checkpoint".into(),
                ));
            }
            self.revisions
                .entry(record.source_id.clone())
                .and_modify(|revision| *revision = (*revision).max(record.revision))
                .or_insert(record.revision);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBatchJournal {
    pub batch_id: String,
    inserted: Vec<ClipId>,
    committed: bool,
}

impl ImportBatchJournal {
    pub fn new(batch_id: impl Into<String>) -> Result<Self> {
        let batch_id = batch_id.into();
        validate_source_id(&batch_id)?;
        Ok(Self {
            batch_id,
            inserted: Vec::new(),
            committed: false,
        })
    }

    pub fn record_insert(&mut self, id: ClipId) -> Result<()> {
        if self.committed {
            return Err(PluginError::InvalidInput(
                "committed import journal is immutable".into(),
            ));
        }
        if !self.inserted.contains(&id) {
            self.inserted.push(id);
        }
        Ok(())
    }

    pub fn commit(&mut self) {
        self.committed = true;
    }

    pub fn rollback_plan(&self) -> Result<RollbackPlan> {
        if !self.committed {
            return Err(PluginError::InvalidInput(
                "uncommitted import does not need a rollback plan".into(),
            ));
        }
        Ok(RollbackPlan {
            batch_id: self.batch_id.clone(),
            delete_ids: self.inserted.iter().rev().copied().collect(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackPlan {
    pub batch_id: String,
    pub delete_ids: Vec<ClipId>,
}

fn validate_source_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_SOURCE_ID_LEN
        || value.contains('\0')
        || value.chars().any(|character| character.is_control())
    {
        return Err(PluginError::InvalidInput(
            "migration source id is invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use vbuff_types::Flavor;

    use super::*;

    fn record(id: &str, revision: u64, text: &str) -> MigrationRecord {
        MigrationRecord {
            source_id: id.into(),
            revision,
            record: ImportRecord {
                flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
                kind_hint: Some(ContentKind::Text),
                created_at_ms: Some(1),
                source_label: Some("competitor".into()),
            },
            pinned: false,
            snippet: false,
        }
    }

    #[test]
    fn preflight_counts_without_exposing_content_and_live_tracker_is_incremental() {
        let records = vec![
            record("one", 1, "ordinary"),
            record("two", 1, "ghp_abcdefghijklmnopqrstuvwxyz123456"),
        ];
        let preflight = MigrationPreflight::scan(&records).unwrap();
        assert_eq!(preflight.records, 2);
        assert_eq!(preflight.likely_secrets, 1);
        assert!(!format!("{:?}", records[1]).contains("ghp_"));

        let mut tracker = LiveMigrationTracker::default();
        assert_eq!(tracker.changed(&records).unwrap().len(), 2);
        tracker.checkpoint(&records).unwrap();
        assert!(tracker.changed(&records).unwrap().is_empty());
        assert!(
            tracker
                .checkpoint(&[records[0].clone(), records[0].clone()])
                .is_err()
        );
        let changed = vec![record("two", 2, "updated")];
        assert_eq!(tracker.changed(&changed).unwrap().len(), 1);
    }

    #[test]
    fn committed_import_rolls_back_in_reverse_order() {
        let first = ClipId::new();
        let second = ClipId::new();
        let mut journal = ImportBatchJournal::new("ditto-2026-07").unwrap();
        journal.record_insert(first).unwrap();
        journal.record_insert(second).unwrap();
        journal.commit();
        assert_eq!(journal.rollback_plan().unwrap().delete_ids, [second, first]);
        assert!(journal.record_insert(ClipId::new()).is_err());
    }
}
