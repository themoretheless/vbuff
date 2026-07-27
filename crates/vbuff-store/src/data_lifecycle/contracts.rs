use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vbuff_types::{Body, Clip, ClipId};

use crate::{Result, StoreError};

pub(super) const MAX_COLLECTION_ID_BYTES: usize = 96;
pub(super) const MAX_COLLECTION_NAME_BYTES: usize = 160;
pub(super) const MAX_MIME_BYTES: usize = 255;
pub(super) const MAX_IMPORT_SOURCE_BYTES: usize = 1_024;
pub(super) const MAX_IMPORT_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_RESTORE_SELECTION: usize = 1_000;
pub(super) const MAX_EXPORT_CLIPS: usize = 10_000;
pub(super) const MAX_EXPORT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArchiveVisibility {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionRetentionPolicy {
    pub max_age_days: Option<u32>,
    pub max_items: Option<u32>,
    pub max_bytes: Option<u64>,
}

impl CollectionRetentionPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.max_age_days.is_none() && self.max_items.is_none() && self.max_bytes.is_none() {
            return Err(StoreError::Maintenance(
                "collection retention must bound age, count, or bytes".into(),
            ));
        }
        if self
            .max_age_days
            .is_some_and(|days| days == 0 || days > 3_650)
            || self.max_items.is_some_and(|items| items > 1_000_000)
            || self
                .max_bytes
                .is_some_and(|bytes| bytes == 0 || bytes > 16 * 1_024 * 1_024 * 1_024 * 1_024)
        {
            return Err(StoreError::Maintenance(
                "collection retention exceeds lifecycle bounds".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CollectionRecord {
    pub id: String,
    pub name: String,
    pub retention: CollectionRetentionPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlavorStorage {
    Inline,
    ContentAddressed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlavorManifest {
    pub mime: String,
    pub byte_size: u64,
    pub storage: FlavorStorage,
    pub blob_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AttachmentManifest {
    pub schema_version: u16,
    pub clip_id: ClipId,
    pub flavors: Vec<FlavorManifest>,
    pub thumbnail_present: bool,
    pub ocr_text_present: bool,
    pub derived_index_present: bool,
}

impl AttachmentManifest {
    pub fn from_stored_clip(clip: &Clip) -> Self {
        let flavors = clip
            .flavors
            .iter()
            .map(|flavor| match &flavor.body {
                Body::Inline(bytes) => FlavorManifest {
                    mime: flavor.mime.clone(),
                    byte_size: bytes.len() as u64,
                    storage: FlavorStorage::Inline,
                    blob_ref: None,
                },
                Body::Spilled {
                    blob_ref,
                    byte_size,
                } => FlavorManifest {
                    mime: flavor.mime.clone(),
                    byte_size: *byte_size,
                    storage: FlavorStorage::ContentAddressed,
                    blob_ref: Some(blob_ref.clone()),
                },
            })
            .collect();
        Self {
            schema_version: 1,
            clip_id: clip.id,
            flavors,
            thumbnail_present: clip.flavors.iter().any(|flavor| {
                flavor.mime.starts_with("image/") && flavor.mime.contains("thumbnail")
            }),
            ocr_text_present: clip
                .flavors
                .iter()
                .any(|flavor| flavor.mime.eq_ignore_ascii_case("text/x-vbuff-ocr")),
            derived_index_present: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SensitiveDataResidency {
    pub ever_on_disk: bool,
    pub ever_synced: bool,
    pub ever_exported: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyTransition {
    Persisted,
    Synced,
    Exported,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ClipAnnotations {
    pub archived: bool,
    pub collection_id: Option<String>,
    pub preferred_mime: Option<String>,
    pub legal_hold: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CollectionRetentionPreview {
    pub clip_ids: Vec<ClipId>,
    pub reclaimable_bytes: u64,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BlobIntegrityReport {
    pub checked: usize,
    pub healthy: usize,
    pub quarantined: usize,
    pub remaining: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GarbageCollectionPreview {
    pub blob_count: usize,
    pub reclaimable_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CompactionForecast {
    pub sqlite_free_bytes: u64,
    pub orphan_blob_bytes: u64,
    pub orphan_blob_count: usize,
    pub estimated_reclaimable_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BackupFreshness {
    pub verified_at: DateTime<Utc>,
    pub age_seconds: u64,
    pub stale: bool,
    pub checksum_prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportQuarantineEntry {
    pub import_id: String,
    pub source_fingerprint: String,
    pub clip_id: ClipId,
    pub byte_size: u64,
    pub sensitive: bool,
    pub staged_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreSelection {
    pub import_ids: Vec<String>,
}

impl RestoreSelection {
    pub fn validate(&self) -> Result<()> {
        if self.import_ids.is_empty() || self.import_ids.len() > MAX_RESTORE_SELECTION {
            return Err(StoreError::Maintenance(
                "restore selection is empty or exceeds its bound".into(),
            ));
        }
        let mut unique = HashSet::with_capacity(self.import_ids.len());
        if self
            .import_ids
            .iter()
            .any(|id| !valid_identifier(id) || !unique.insert(id))
        {
            return Err(StoreError::Maintenance(
                "restore selection contains an invalid or duplicate id".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PartialRestoreReport {
    pub requested: usize,
    pub restored: usize,
    pub unavailable: usize,
    pub deduplicated: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum ExportSchemaVersion {
    V1,
    V2,
}

impl ExportSchemaVersion {
    pub const LATEST: Self = Self::V2;

    pub const fn compatibility_note(self) -> &'static str {
        match self {
            Self::V1 => {
                "portable core clip fields; newer provenance and policy metadata omitted; policy-bearing clips cannot be downgraded"
            }
            Self::V2 => {
                "current Clip and ClipMeta fields; lifecycle sidecars such as archive and collections are excluded"
            }
        }
    }
}

impl TryFrom<u16> for ExportSchemaVersion {
    type Error = &'static str;

    fn try_from(value: u16) -> std::result::Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            _ => Err("unsupported export schema"),
        }
    }
}

impl From<ExportSchemaVersion> for u16 {
    fn from(value: ExportSchemaVersion) -> Self {
        match value {
            ExportSchemaVersion::V1 => 1,
            ExportSchemaVersion::V2 => 2,
        }
    }
}

pub(super) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_COLLECTION_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(super) fn valid_mime(value: &str) -> bool {
    value.len() <= MAX_MIME_BYTES
        && value.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty()
                && !subtype.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\'))
        })
}

pub(super) fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| StoreError::Maintenance("value exceeds SQLite range".into()))
}

pub(super) fn require_lifecycle_update(changed: usize, row_name: &str) -> Result<()> {
    if changed == 1 {
        Ok(())
    } else {
        Err(StoreError::Corrupt(format!("{row_name} row is missing")))
    }
}
