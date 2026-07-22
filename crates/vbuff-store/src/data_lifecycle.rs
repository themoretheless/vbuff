//! Archive, annotation, export, recovery, and storage-maintenance contracts.
//!
//! Canonical clip bytes remain in `clips`/CAS. This module owns mutable
//! organization state and bounded maintenance operations around that core.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension as _, params};
use serde::{Deserialize, Serialize};
use vbuff_core::capture::{CaptureDecision, CaptureInput, CapturePolicy, SelectionSource};
use vbuff_core::content_hash_from_flavors;
use vbuff_types::{Body, Clip, ClipId};

use crate::{Result, Store, StoreError, now_millis, raw_to_clip, row_to_clip};

const MAX_COLLECTION_ID_BYTES: usize = 96;
const MAX_COLLECTION_NAME_BYTES: usize = 160;
const MAX_MIME_BYTES: usize = 255;
const MAX_IMPORT_SOURCE_BYTES: usize = 1_024;
const MAX_IMPORT_BYTES: usize = 512 * 1024 * 1024;
const MAX_RESTORE_SELECTION: usize = 1_000;
const MAX_EXPORT_CLIPS: usize = 10_000;
const MAX_EXPORT_BYTES: usize = 512 * 1024 * 1024;

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

#[derive(Serialize)]
struct ExportEnvelope {
    schema_version: u16,
    compatibility_note: &'static str,
    clips: Vec<serde_json::Value>,
}

pub fn export_clips_json(clips: &[Clip], version: ExportSchemaVersion) -> Result<String> {
    if clips.len() > MAX_EXPORT_CLIPS {
        return Err(StoreError::Maintenance(
            "export clip count exceeds bound".into(),
        ));
    }
    let mut body_total = 0usize;
    let mut encoded_total = 0usize;
    let mut values = Vec::with_capacity(clips.len());
    for clip in clips {
        let body_bytes = portable_inline_size(clip).ok_or_else(|| {
            StoreError::Maintenance("portable export requires resolved inline bodies".into())
        })?;
        if body_bytes != clip.meta.byte_size {
            return Err(StoreError::Maintenance(
                "portable export body size does not match clip metadata".into(),
            ));
        }
        body_total = body_total
            .checked_add(usize::try_from(body_bytes).unwrap_or(usize::MAX))
            .ok_or_else(|| StoreError::Maintenance("export size overflow".into()))?;
        if body_total > MAX_EXPORT_BYTES {
            return Err(StoreError::Maintenance(
                "export byte size exceeds bound".into(),
            ));
        }
        if version == ExportSchemaVersion::V1
            && (clip.meta.sensitive
                || clip.meta.sensitivity_reason.is_some()
                || clip.meta.expires_at.is_some()
                || !clip.meta.sync_eligible)
        {
            return Err(StoreError::Maintenance(
                "v1 export cannot preserve this clip's privacy policy".into(),
            ));
        }
        let mut value = serde_json::to_value(clip)?;
        if version == ExportSchemaVersion::V1
            && let Some(meta) = value
                .get_mut("meta")
                .and_then(serde_json::Value::as_object_mut)
        {
            for key in [
                "provenance",
                "generation",
                "lineage",
                "expires_at",
                "sensitive",
                "sensitivity_reason",
                "sync_eligible",
                "ai_allowed",
            ] {
                meta.remove(key);
            }
        }
        encoded_total = encoded_total
            .checked_add(serde_json::to_vec(&value)?.len())
            .ok_or_else(|| StoreError::Maintenance("export size overflow".into()))?;
        if encoded_total > MAX_EXPORT_BYTES {
            return Err(StoreError::Maintenance(
                "encoded export size exceeds bound".into(),
            ));
        }
        values.push(value);
    }
    let output = serde_json::to_string_pretty(&ExportEnvelope {
        schema_version: version.into(),
        compatibility_note: version.compatibility_note(),
        clips: values,
    })?;
    if output.len() > MAX_EXPORT_BYTES {
        return Err(StoreError::Maintenance(
            "encoded export size exceeds bound".into(),
        ));
    }
    Ok(output)
}

impl Store {
    pub fn set_archived(&self, id: ClipId, archived: bool) -> Result<()> {
        self.ensure_clip_exists(id)?;
        self.conn.execute(
            "UPDATE clip_annotations SET archived = ?1 WHERE clip_id = ?2",
            params![archived as i64, id.to_string_repr()],
        )?;
        Ok(())
    }

    pub fn annotations(&self, id: ClipId) -> Result<ClipAnnotations> {
        self.conn
            .query_row(
                r#"
                SELECT archived, collection_id, preferred_mime, legal_hold
                FROM clip_annotations WHERE clip_id = ?1
                "#,
                [id.to_string_repr()],
                |row| {
                    Ok(ClipAnnotations {
                        archived: row.get::<_, i64>(0)? != 0,
                        collection_id: row.get(1)?,
                        preferred_mime: row.get(2)?,
                        legal_hold: row.get::<_, i64>(3)? != 0,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::ClipNotFound(id.to_string_repr()))
    }

    pub fn list_with_archive(
        &self,
        visibility: ArchiveVisibility,
        limit: usize,
    ) -> Result<Vec<Clip>> {
        let archive_clause = match visibility {
            ArchiveVisibility::Active => "AND COALESCE(a.archived, 0) = 0",
            ArchiveVisibility::Archived => "AND COALESCE(a.archived, 0) = 1",
            ArchiveVisibility::All => "",
        };
        let sql = format!(
            r#"
            SELECT c.id, c.content_hash, c.flavors, c.kind, c.created_at, c.updated_at,
                   c.byte_size, c.source_app, c.metadata_json, c.pinned, c.favorite
            FROM clips c
            LEFT JOIN clip_annotations a ON a.clip_id = c.id
            WHERE (c.expires_at IS NULL OR c.expires_at > ?1)
            {archive_clause}
            ORDER BY c.pinned DESC, c.updated_at DESC, c.seq DESC
            LIMIT ?2
            "#,
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            params![now_millis(), limit.min(MAX_EXPORT_CLIPS) as i64],
            row_to_clip,
        )?;
        let mut clips = Vec::new();
        for row in rows {
            clips.push(raw_to_clip(row?)?);
        }
        self.hydrate_clips(&mut clips)?;
        Ok(clips)
    }

    /// Return the newest active clip by actual recency, independent of the
    /// pinned-first ordering used by the GUI projection.
    pub fn latest_by_recency(&self) -> Result<Option<Clip>> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT c.id, c.content_hash, c.flavors, c.kind, c.created_at, c.updated_at,
                       c.byte_size, c.source_app, c.metadata_json, c.pinned, c.favorite
                FROM clips c
                LEFT JOIN clip_annotations a ON a.clip_id = c.id
                WHERE (c.expires_at IS NULL OR c.expires_at > ?1)
                  AND COALESCE(a.archived, 0) = 0
                ORDER BY c.updated_at DESC, c.seq DESC
                LIMIT 1
                "#,
                [now_millis()],
                row_to_clip,
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let mut clip = raw_to_clip(row)?;
        self.hydrate_clip(&mut clip)?;
        Ok(Some(clip))
    }

    /// Fetch one non-expired clip by id from the authoritative repository.
    pub fn get_clip(&self, id: ClipId) -> Result<Option<Clip>> {
        let row = self
            .conn
            .query_row(
                r#"
                SELECT id, content_hash, flavors, kind, created_at, updated_at,
                       byte_size, source_app, metadata_json, pinned, favorite
                FROM clips
                WHERE id = ?1 AND (expires_at IS NULL OR expires_at > ?2)
                "#,
                params![id.to_string_repr(), now_millis()],
                row_to_clip,
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let mut clip = raw_to_clip(row)?;
        self.hydrate_clip(&mut clip)?;
        Ok(Some(clip))
    }

    pub fn upsert_collection(&self, record: &CollectionRecord) -> Result<()> {
        if !valid_identifier(&record.id)
            || record.id.len() > MAX_COLLECTION_ID_BYTES
            || record.name.trim().is_empty()
            || record.name.len() > MAX_COLLECTION_NAME_BYTES
            || record.name.chars().any(char::is_control)
        {
            return Err(StoreError::Maintenance(
                "invalid collection identity".into(),
            ));
        }
        record.retention.validate()?;
        self.conn.execute(
            r#"
            INSERT INTO collection_policies(
                id, name, max_age_days, max_items, max_bytes
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                max_age_days = excluded.max_age_days,
                max_items = excluded.max_items,
                max_bytes = excluded.max_bytes
            "#,
            params![
                record.id,
                record.name.trim(),
                record.retention.max_age_days.map(i64::from),
                record.retention.max_items.map(i64::from),
                record.retention.max_bytes.map(to_i64).transpose()?,
            ],
        )?;
        Ok(())
    }

    pub fn set_collection(&self, id: ClipId, collection_id: Option<&str>) -> Result<()> {
        self.ensure_clip_exists(id)?;
        if let Some(collection_id) = collection_id {
            if !valid_identifier(collection_id) || collection_id.len() > MAX_COLLECTION_ID_BYTES {
                return Err(StoreError::Maintenance("invalid collection id".into()));
            }
            let exists: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM collection_policies WHERE id = ?1)",
                [collection_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(StoreError::Maintenance("collection does not exist".into()));
            }
        }
        self.conn.execute(
            "UPDATE clip_annotations SET collection_id = ?1 WHERE clip_id = ?2",
            params![collection_id, id.to_string_repr()],
        )?;
        Ok(())
    }

    pub fn collection_retention_preview(
        &self,
        collection_id: &str,
        limit: usize,
    ) -> Result<CollectionRetentionPreview> {
        let policy = self.collection_policy(collection_id)?;
        let bounded_limit = limit.min(10_000);
        let mut statement = self.conn.prepare(
            r#"
            SELECT c.id, c.updated_at, c.byte_size
            FROM clips c
            JOIN clip_annotations a ON a.clip_id = c.id
            WHERE a.collection_id = ?1 AND a.legal_hold = 0
              AND c.pinned = 0 AND c.favorite = 0
              AND NOT EXISTS (
                SELECT 1 FROM session_protected p WHERE p.clip_id = c.id
              )
            ORDER BY c.updated_at DESC, c.seq DESC
            "#,
        )?;
        let rows = statement.query_map([collection_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?.max(0) as u64,
            ))
        })?;
        let now = now_millis();
        let cutoff = policy
            .max_age_days
            .map(|days| now.saturating_sub(i64::from(days) * 24 * 60 * 60 * 1_000));
        let mut retained_bytes = 0u64;
        let mut ids = Vec::new();
        let mut bytes = 0u64;
        let mut truncated = false;
        for (index, row) in rows.enumerate() {
            let (id, updated_at, byte_size) = row?;
            retained_bytes = retained_bytes.saturating_add(byte_size);
            let over_age = cutoff.is_some_and(|cutoff| updated_at < cutoff);
            let over_count = policy.max_items.is_some_and(|max| index >= max as usize);
            let over_bytes = policy.max_bytes.is_some_and(|max| retained_bytes > max);
            if over_age || over_count || over_bytes {
                if ids.len() == bounded_limit {
                    truncated = true;
                    continue;
                }
                ids.push(
                    ClipId::parse(&id)
                        .map_err(|_| StoreError::Corrupt("bad collection clip id".into()))?,
                );
                bytes = bytes.saturating_add(byte_size);
            }
        }
        Ok(CollectionRetentionPreview {
            clip_ids: ids,
            reclaimable_bytes: bytes,
            truncated,
        })
    }

    pub fn enforce_collection_retention(
        &self,
        collection_id: &str,
        limit: usize,
    ) -> Result<CollectionRetentionPreview> {
        let preview = self.collection_retention_preview(collection_id, limit)?;
        let transaction = self.conn.unchecked_transaction()?;
        for id in &preview.clip_ids {
            transaction.execute("DELETE FROM clips WHERE id = ?1", [id.to_string_repr()])?;
        }
        transaction.commit()?;
        if !preview.clip_ids.is_empty() {
            self.scrub_deleted_pages()?;
        }
        Ok(preview)
    }

    pub fn attachment_manifest(&self, id: ClipId) -> Result<AttachmentManifest> {
        let (raw, derived_index_present) = self
            .conn
            .query_row(
                r#"
                SELECT id, content_hash, flavors, kind, created_at, updated_at,
                       byte_size, source_app, metadata_json, pinned, favorite,
                       item_text != ''
                FROM clips WHERE id = ?1
                "#,
                [id.to_string_repr()],
                |row| Ok((row_to_clip(row)?, row.get::<_, i64>(11)? != 0)),
            )
            .optional()?
            .ok_or_else(|| StoreError::ClipNotFound(id.to_string_repr()))?;
        let mut manifest = AttachmentManifest::from_stored_clip(&raw_to_clip(raw)?);
        manifest.derived_index_present = derived_index_present;
        Ok(manifest)
    }

    pub fn scrub_blobs(&self, limit: usize) -> Result<BlobIntegrityReport> {
        let Some(cas) = &self.cas else {
            return Ok(BlobIntegrityReport::default());
        };
        let bounded = limit.min(1_024);
        if bounded == 0 {
            let remaining = self.conn.query_row(
                r#"
                SELECT COUNT(*) FROM blob_refs r
                WHERE r.refcount > 0 AND NOT EXISTS (
                    SELECT 1 FROM blob_quarantine q
                    WHERE q.hash = r.hash AND q.kind = r.kind
                )
                "#,
                [],
                |row| row.get::<_, i64>(0),
            )? as usize;
            return Ok(BlobIntegrityReport {
                remaining,
                ..BlobIntegrityReport::default()
            });
        }
        let cursor = self.blob_scrub_cursor.borrow().clone();
        let mut statement = self.conn.prepare(
            r#"
            SELECT r.hash, r.kind, r.byte_size FROM blob_refs r
            WHERE r.refcount > 0 AND NOT EXISTS (
                SELECT 1 FROM blob_quarantine q
                WHERE q.hash = r.hash AND q.kind = r.kind
            ) AND (
                ?1 IS NULL OR r.hash > ?1 OR (r.hash = ?1 AND r.kind > ?2)
            )
            ORDER BY r.hash, r.kind LIMIT ?3
            "#,
        )?;
        let rows = statement.query_map(
            params![
                cursor.as_ref().map(|(hash, _)| hash.as_str()),
                cursor.as_ref().map_or(i64::MIN, |(_, kind)| *kind),
                bounded as i64,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?.max(0) as u64,
                ))
            },
        )?;
        let references = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let Some((last_hash, last_kind)) = references
            .last()
            .map(|(hash, kind, _)| (hash.clone(), *kind))
        else {
            *self.blob_scrub_cursor.borrow_mut() = None;
            return Ok(BlobIntegrityReport::default());
        };
        let remaining = self.conn.query_row(
            r#"
            SELECT COUNT(*) FROM blob_refs r
            WHERE r.refcount > 0 AND NOT EXISTS (
                SELECT 1 FROM blob_quarantine q
                WHERE q.hash = r.hash AND q.kind = r.kind
            ) AND (r.hash > ?1 OR (r.hash = ?1 AND r.kind > ?2))
            "#,
            params![last_hash, last_kind],
            |row| row.get::<_, i64>(0),
        )? as usize;
        let mut report = BlobIntegrityReport {
            remaining,
            ..BlobIntegrityReport::default()
        };
        for (blob_ref, kind, byte_size) in references {
            report.checked += 1;
            let kind = super::kind_from_int(kind);
            if cas.verify(kind, &blob_ref, byte_size).is_ok() {
                report.healthy += 1;
                continue;
            }
            cas.quarantine(kind, &blob_ref)?;
            self.conn.execute(
                r#"
                INSERT OR REPLACE INTO blob_quarantine(hash, kind, quarantined_at, reason)
                VALUES (?1, ?2, ?3, 'integrity verification failed')
                "#,
                params![blob_ref, super::kind_to_int(kind), now_millis()],
            )?;
            report.quarantined += 1;
        }
        *self.blob_scrub_cursor.borrow_mut() = if remaining == 0 {
            None
        } else {
            Some((last_hash, last_kind))
        };
        Ok(report)
    }

    pub fn gc_dry_run(&self) -> Result<GarbageCollectionPreview> {
        let Some(cas) = &self.cas else {
            return Ok(GarbageCollectionPreview::default());
        };
        let live = self.live_blob_refs()?;
        let (blob_count, reclaimable_bytes) = cas.orphan_inventory(&live)?;
        Ok(GarbageCollectionPreview {
            blob_count,
            reclaimable_bytes,
        })
    }

    pub fn compaction_forecast(&self) -> Result<CompactionForecast> {
        let page_size: u64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))?
            .max(0) as u64;
        let free_pages: u64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get::<_, i64>(0))?
            .max(0) as u64;
        let gc = self.gc_dry_run()?;
        let sqlite_free_bytes = page_size.saturating_mul(free_pages);
        Ok(CompactionForecast {
            sqlite_free_bytes,
            orphan_blob_bytes: gc.reclaimable_bytes,
            orphan_blob_count: gc.blob_count,
            estimated_reclaimable_bytes: sqlite_free_bytes.saturating_add(gc.reclaimable_bytes),
        })
    }

    pub fn record_residency(&self, id: ClipId, transition: ResidencyTransition) -> Result<()> {
        self.ensure_clip_exists(id)?;
        let column = match transition {
            ResidencyTransition::Persisted => "ever_on_disk",
            ResidencyTransition::Synced => "ever_synced",
            ResidencyTransition::Exported => "ever_exported",
        };
        self.conn.execute(
            &format!("UPDATE clip_residency SET {column} = 1 WHERE clip_id = ?1"),
            [id.to_string_repr()],
        )?;
        Ok(())
    }

    pub fn residency(&self, id: ClipId) -> Result<SensitiveDataResidency> {
        self.conn
            .query_row(
                "SELECT ever_on_disk, ever_synced, ever_exported FROM clip_residency WHERE clip_id = ?1",
                [id.to_string_repr()],
                |row| {
                    Ok(SensitiveDataResidency {
                        ever_on_disk: row.get::<_, i64>(0)? != 0,
                        ever_synced: row.get::<_, i64>(1)? != 0,
                        ever_exported: row.get::<_, i64>(2)? != 0,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::ClipNotFound(id.to_string_repr()))
    }

    pub fn set_preferred_flavor(&self, id: ClipId, mime: Option<&str>) -> Result<()> {
        let clip = self
            .load_clip_by_id(id)?
            .ok_or_else(|| StoreError::ClipNotFound(id.to_string_repr()))?;
        if let Some(mime) = mime
            && (!valid_mime(mime)
                || !clip
                    .flavors
                    .iter()
                    .any(|flavor| flavor.mime.eq_ignore_ascii_case(mime)))
        {
            return Err(StoreError::Maintenance(
                "preferred flavor is invalid or unavailable".into(),
            ));
        }
        self.conn.execute(
            "UPDATE clip_annotations SET preferred_mime = ?1 WHERE clip_id = ?2",
            params![mime, id.to_string_repr()],
        )?;
        Ok(())
    }

    pub fn set_legal_hold(&self, id: ClipId, held: bool) -> Result<()> {
        self.ensure_clip_exists(id)?;
        self.conn.execute(
            "UPDATE clip_annotations SET legal_hold = ?1 WHERE clip_id = ?2",
            params![held as i64, id.to_string_repr()],
        )?;
        Ok(())
    }

    pub fn record_verified_backup(&self, verified_at: DateTime<Utc>, checksum: &str) -> Result<()> {
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(StoreError::Maintenance("invalid backup checksum".into()));
        }
        self.conn.execute(
            r#"
            INSERT INTO backup_state(singleton, verified_at, checksum)
            VALUES (1, ?1, ?2)
            ON CONFLICT(singleton) DO UPDATE SET
                verified_at = excluded.verified_at,
                checksum = excluded.checksum
            "#,
            params![
                verified_at.timestamp_millis(),
                checksum.to_ascii_lowercase()
            ],
        )?;
        Ok(())
    }

    pub fn backup_freshness(
        &self,
        now: DateTime<Utc>,
        stale_after: Duration,
    ) -> Result<Option<BackupFreshness>> {
        let record = self
            .conn
            .query_row(
                "SELECT verified_at, checksum FROM backup_state WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((verified_at, checksum)) = record else {
            return Ok(None);
        };
        let verified_at = DateTime::from_timestamp_millis(verified_at)
            .ok_or_else(|| StoreError::Corrupt("invalid backup verification time".into()))?;
        if verified_at > now {
            return Err(StoreError::Corrupt(
                "backup verification time is in the future".into(),
            ));
        }
        let age_seconds = now.signed_duration_since(verified_at).num_seconds() as u64;
        Ok(Some(BackupFreshness {
            verified_at,
            age_seconds,
            stale: age_seconds > stale_after.as_secs(),
            checksum_prefix: checksum.chars().take(12).collect(),
        }))
    }

    pub fn stage_import(&self, clip: &Clip, source: &str) -> Result<String> {
        let body_bytes = portable_inline_size(clip);
        if source.trim().is_empty()
            || source.len() > MAX_IMPORT_SOURCE_BYTES
            || source.chars().any(char::is_control)
            || body_bytes != Some(clip.meta.byte_size)
            || content_hash_from_flavors(&clip.flavors) != clip.content_hash
            || body_bytes.is_none_or(|bytes| bytes > MAX_IMPORT_BYTES as u64)
        {
            return Err(StoreError::Maintenance("invalid import candidate".into()));
        }
        let clip = sanitize_import_privacy(clip.clone())?;
        let payload = serde_json::to_string(&clip)?;
        if payload.len() > MAX_IMPORT_BYTES {
            return Err(StoreError::Maintenance(
                "import payload exceeds bound".into(),
            ));
        }
        let import_id = ClipId::new().to_string_repr();
        let source_fingerprint = source_fingerprint(source);
        self.conn.execute(
            r#"
            INSERT INTO import_quarantine(
                import_id, source_fingerprint, clip_id, staged_at,
                byte_size, sensitive, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                import_id,
                source_fingerprint,
                clip.id.to_string_repr(),
                now_millis(),
                to_i64(clip.meta.byte_size)?,
                clip.meta.sensitive as i64,
                payload,
            ],
        )?;
        Ok(import_id)
    }

    pub fn import_quarantine(&self, limit: usize) -> Result<Vec<ImportQuarantineEntry>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT import_id, source_fingerprint, clip_id, staged_at, byte_size, sensitive
            FROM import_quarantine ORDER BY staged_at, import_id LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map([limit.min(MAX_RESTORE_SELECTION) as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)? != 0,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (import_id, source_fingerprint, clip_id, staged_at, byte_size, sensitive) = row?;
            entries.push(ImportQuarantineEntry {
                import_id,
                source_fingerprint,
                clip_id: ClipId::parse(&clip_id)
                    .map_err(|_| StoreError::Corrupt("bad import clip id".into()))?,
                byte_size: byte_size.max(0) as u64,
                sensitive,
                staged_at: DateTime::from_timestamp_millis(staged_at)
                    .ok_or_else(|| StoreError::Corrupt("bad import timestamp".into()))?,
            });
        }
        Ok(entries)
    }

    pub fn restore_imports(&self, selection: &RestoreSelection) -> Result<PartialRestoreReport> {
        selection.validate()?;
        let mut report = PartialRestoreReport {
            requested: selection.import_ids.len(),
            ..PartialRestoreReport::default()
        };
        for import_id in &selection.import_ids {
            let payload = self
                .conn
                .query_row(
                    "SELECT payload_json FROM import_quarantine WHERE import_id = ?1",
                    [import_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(payload) = payload else {
                report.unavailable += 1;
                continue;
            };
            if payload.len() > MAX_IMPORT_BYTES {
                return Err(StoreError::Corrupt(
                    "quarantined import exceeds bound".into(),
                ));
            }
            let clip: Clip = serde_json::from_str(&payload)?;
            if portable_inline_size(&clip) != Some(clip.meta.byte_size)
                || content_hash_from_flavors(&clip.flavors) != clip.content_hash
            {
                return Err(StoreError::Corrupt(
                    "quarantined import failed body or content hash verification".into(),
                ));
            }
            let clip = sanitize_import_privacy(clip)?;
            self.purge_expired()?;
            let duplicate = self
                .conn
                .query_row(
                    r#"
                    SELECT c.id, COALESCE(a.archived, 0)
                    FROM clips c
                    LEFT JOIN clip_annotations a ON a.clip_id = c.id
                    WHERE c.content_hash = ?1
                      AND (c.expires_at IS NULL OR c.expires_at > ?2)
                    "#,
                    params![clip.content_hash.as_slice(), now_millis()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)),
                )
                .optional()?;
            if let Some((existing_id, archived)) = duplicate {
                if archived {
                    let existing_id = ClipId::parse(&existing_id)
                        .map_err(|_| StoreError::Corrupt("bad duplicate clip id".into()))?;
                    self.set_archived(existing_id, false)?;
                }
                self.conn.execute(
                    "DELETE FROM import_quarantine WHERE import_id = ?1",
                    [import_id],
                )?;
                report.restored += 1;
                report.deduplicated += 1;
                continue;
            }
            self.insert(&clip)?;
            self.conn.execute(
                "DELETE FROM import_quarantine WHERE import_id = ?1",
                [import_id],
            )?;
            report.restored += 1;
        }
        Ok(report)
    }

    pub fn reject_import(&self, import_id: &str) -> Result<bool> {
        if !valid_identifier(import_id) {
            return Err(StoreError::Maintenance("invalid import id".into()));
        }
        let deleted = self.conn.execute(
            "DELETE FROM import_quarantine WHERE import_id = ?1",
            [import_id],
        )?;
        if deleted > 0 {
            self.scrub_deleted_pages()?;
        }
        Ok(deleted == 1)
    }

    pub fn export_json(
        &self,
        version: ExportSchemaVersion,
        visibility: ArchiveVisibility,
        limit: usize,
    ) -> Result<String> {
        if limit > MAX_EXPORT_CLIPS {
            return Err(StoreError::Maintenance(
                "export clip count exceeds bound".into(),
            ));
        }
        let clips = self.list_with_archive(visibility, limit)?;
        let output = export_clips_json(&clips, version)?;
        let transaction = self.conn.unchecked_transaction()?;
        for clip in &clips {
            transaction.execute(
                "UPDATE clip_residency SET ever_exported = 1 WHERE clip_id = ?1",
                [clip.id.to_string_repr()],
            )?;
        }
        transaction.commit()?;
        Ok(output)
    }

    pub(crate) fn ensure_not_legal_hold(&self, id: ClipId) -> Result<()> {
        let held = self
            .conn
            .query_row(
                "SELECT legal_hold FROM clip_annotations WHERE clip_id = ?1",
                [id.to_string_repr()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        match held {
            None => Err(StoreError::ClipNotFound(id.to_string_repr())),
            Some(0) => Ok(()),
            Some(_) => Err(StoreError::Maintenance(
                "clip is under legal hold; release it before deletion".into(),
            )),
        }
    }

    fn ensure_clip_exists(&self, id: ClipId) -> Result<()> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM clips WHERE id = ?1)",
            [id.to_string_repr()],
            |row| row.get(0),
        )?;
        if exists {
            Ok(())
        } else {
            Err(StoreError::ClipNotFound(id.to_string_repr()))
        }
    }

    fn collection_policy(&self, collection_id: &str) -> Result<CollectionRetentionPolicy> {
        let policy = self
            .conn
            .query_row(
                r#"
                SELECT max_age_days, max_items, max_bytes
                FROM collection_policies WHERE id = ?1
                "#,
                [collection_id],
                |row| {
                    Ok(CollectionRetentionPolicy {
                        max_age_days: row.get::<_, Option<i64>>(0)?.map(|value| value as u32),
                        max_items: row.get::<_, Option<i64>>(1)?.map(|value| value as u32),
                        max_bytes: row.get::<_, Option<i64>>(2)?.map(|value| value as u64),
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::Maintenance("collection does not exist".into()))?;
        policy.validate()?;
        Ok(policy)
    }

    fn live_blob_refs(&self) -> Result<HashSet<(vbuff_types::ContentKind, String)>> {
        let mut statement = self
            .conn
            .prepare("SELECT hash, kind FROM blob_refs WHERE refcount > 0")?;
        let rows = statement.query_map([], |row| {
            Ok((
                super::kind_from_int(row.get::<_, i64>(1)?),
                row.get::<_, String>(0)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<HashSet<_>>>()?)
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_COLLECTION_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_mime(value: &str) -> bool {
    value.len() <= MAX_MIME_BYTES
        && value.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty()
                && !subtype.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\'))
        })
}

fn source_fingerprint(source: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-import-source-v1\0");
    hasher.update(source.as_bytes());
    hasher.finalize().to_hex()[..16].to_owned()
}

fn sanitize_import_privacy(mut clip: Clip) -> Result<Clip> {
    let decision = CapturePolicy::default().decide(CaptureInput {
        flavors: &clip.flavors,
        provenance: &clip.meta.provenance,
        source: SelectionSource::Clipboard,
        primary_intended: true,
        coherent_generation: true,
        concealed: false,
        self_write: false,
    });
    let CaptureDecision::Capture {
        sensitive,
        memory_only,
        expires_after,
        sensitivity_reason,
        ..
    } = decision
    else {
        return Err(StoreError::Maintenance(
            "import candidate is rejected by capture policy".into(),
        ));
    };
    if memory_only {
        return Err(StoreError::Maintenance(
            "memory-only content cannot enter import quarantine".into(),
        ));
    }

    // Imported metadata is untrusted. Detection may only tighten the record,
    // and restored imports stay local/AI-disabled until a future explicit
    // review workflow can grant broader use.
    clip.meta.sync_eligible = false;
    clip.meta.ai_allowed = false;
    if sensitive {
        clip.meta.sensitive = true;
        clip.meta.sensitivity_reason = sensitivity_reason.or(clip.meta.sensitivity_reason);
        let detected_expiry = expires_after
            .and_then(|ttl| chrono::Duration::from_std(ttl).ok())
            .map(|ttl| Utc::now() + ttl);
        clip.meta.expires_at = match (clip.meta.expires_at, detected_expiry) {
            (Some(imported), Some(detected)) => Some(imported.min(detected)),
            (imported, detected) => imported.or(detected),
        };
    }
    Ok(clip)
}

fn portable_inline_size(clip: &Clip) -> Option<u64> {
    clip.flavors
        .iter()
        .try_fold(0_u64, |total, flavor| match &flavor.body {
            Body::Inline(bytes) => total.checked_add(bytes.len() as u64),
            Body::Spilled { .. } => None,
        })
}

fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| StoreError::Maintenance("value exceeds SQLite range".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{ClipMeta, ContentKind, Flavor};

    fn clip(text: &str) -> Clip {
        let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
        Clip {
            id: ClipId::new(),
            content_hash: content_hash_from_flavors(&flavors),
            meta: ClipMeta::now(ContentKind::Text, text.len() as u64, None),
            flavors,
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn export_downgrade_is_explicit_and_omits_new_policy_fields() {
        let output = export_clips_json(&[clip("portable")], ExportSchemaVersion::V1).unwrap();
        assert!(output.contains("\"schema_version\": 1"));
        assert!(!output.contains("sync_eligible"));
        assert!(!output.contains("sensitivity_reason"));
        assert!(output.contains("portable"));
    }

    #[test]
    fn export_rejects_privacy_downgrades_and_unresolved_payloads() {
        let mut sensitive = clip("sensitive");
        sensitive.meta.sensitive = true;
        assert!(export_clips_json(&[sensitive], ExportSchemaVersion::V1).is_err());

        let mut expiring = clip("expiring");
        expiring.meta.expires_at = Some(Utc::now() + chrono::Duration::minutes(1));
        assert!(export_clips_json(&[expiring], ExportSchemaVersion::V1).is_err());

        let mut local_only = clip("local only");
        local_only.meta.sync_eligible = false;
        assert!(export_clips_json(&[local_only], ExportSchemaVersion::V1).is_err());

        let mut unresolved = clip("unresolved");
        unresolved.flavors[0].body = Body::Spilled {
            blob_ref: "a".repeat(64),
            byte_size: unresolved.meta.byte_size,
        };
        unresolved.content_hash = content_hash_from_flavors(&unresolved.flavors);
        assert!(export_clips_json(&[unresolved], ExportSchemaVersion::V2).is_err());

        let mut mismatched = clip("mismatched");
        mismatched.meta.byte_size += 1;
        assert!(export_clips_json(&[mismatched], ExportSchemaVersion::V2).is_err());
    }

    #[test]
    fn archive_annotations_and_legal_hold_are_separate_from_canonical_clip() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("immutable");
        store.insert(&clip).unwrap();
        store.set_archived(clip.id, true).unwrap();
        store.set_legal_hold(clip.id, true).unwrap();
        assert!(store.list(10).unwrap().is_empty());
        assert_eq!(
            store
                .list_with_archive(ArchiveVisibility::Archived, 10)
                .unwrap()
                .len(),
            1
        );
        assert!(store.delete(clip.id).is_err());
        store.set_legal_hold(clip.id, false).unwrap();
        store.delete(clip.id).unwrap();
    }

    #[test]
    fn authoritative_latest_ignores_pinned_first_projection_order() {
        let store = Store::open_in_memory().unwrap();
        let mut older = clip("older pinned");
        older.pinned = true;
        older.meta.created_at = Utc::now() - chrono::Duration::minutes(2);
        let newer = clip("newer unpinned");
        store.insert(&older).unwrap();
        store.insert(&newer).unwrap();

        assert_eq!(store.list(2).unwrap()[0].id, older.id);
        assert_eq!(store.latest_by_recency().unwrap().unwrap().id, newer.id);
        assert_eq!(store.get_clip(older.id).unwrap().unwrap().id, older.id);
    }

    #[test]
    fn collection_retention_previews_before_deleting() {
        let store = Store::open_in_memory().unwrap();
        let record = CollectionRecord {
            id: "work".into(),
            name: "Work".into(),
            retention: CollectionRetentionPolicy {
                max_age_days: None,
                max_items: Some(1),
                max_bytes: None,
            },
        };
        store.upsert_collection(&record).unwrap();
        let first = clip("first");
        let second = clip("second");
        store.insert(&first).unwrap();
        store.insert(&second).unwrap();
        store.set_collection(first.id, Some("work")).unwrap();
        store.set_collection(second.id, Some("work")).unwrap();
        let preview = store.collection_retention_preview("work", 10).unwrap();
        assert_eq!(preview.clip_ids.len(), 1);
        assert_eq!(store.count().unwrap(), 2);
        store.enforce_collection_retention("work", 10).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn import_stays_quarantined_until_selected_restore() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("imported");
        let import_id = store.stage_import(&clip, "/private/backup.json").unwrap();
        assert_eq!(store.count().unwrap(), 0);
        let entries = store.import_quarantine(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!format!("{entries:?}").contains("private/backup"));
        let report = store
            .restore_imports(&RestoreSelection {
                import_ids: vec![import_id],
            })
            .unwrap();
        assert_eq!(report.restored, 1);
        assert_eq!(store.count().unwrap(), 1);
        let restored = store.list(1).unwrap().pop().unwrap();
        assert!(!restored.meta.sync_eligible);
        assert!(!restored.meta.ai_allowed);
    }

    #[test]
    fn import_reclassifies_secret_content_instead_of_trusting_metadata() {
        let store = Store::open_in_memory().unwrap();
        let mut untrusted = clip("ghp_abcdefghijklmnopqrstuvwxyz123456");
        untrusted.meta.sensitive = false;
        untrusted.meta.sensitivity_reason = None;
        untrusted.meta.expires_at = None;
        untrusted.meta.sync_eligible = true;
        untrusted.meta.ai_allowed = true;

        let import_id = store.stage_import(&untrusted, "backup.json").unwrap();
        store
            .restore_imports(&RestoreSelection {
                import_ids: vec![import_id],
            })
            .unwrap();
        let restored = store.list(1).unwrap().pop().unwrap();
        assert!(restored.meta.sensitive);
        assert!(restored.meta.sensitivity_reason.is_some());
        assert!(restored.meta.expires_at.is_some());
        assert!(!restored.meta.sync_eligible);
        assert!(!restored.meta.ai_allowed);
    }

    #[test]
    fn import_rejects_memory_only_secret_classes_before_quarantine() {
        let store = Store::open_in_memory().unwrap();
        let mut untrusted = clip("verification code 123456");
        untrusted.meta.sensitive = false;
        untrusted.meta.sensitivity_reason = None;

        assert!(store.stage_import(&untrusted, "backup.json").is_err());
        assert!(store.import_quarantine(10).unwrap().is_empty());
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn import_requires_inline_size_consistency_and_dedup_does_not_mutate_live_clip() {
        let store = Store::open_in_memory().unwrap();
        let original = clip("same bytes");
        store.insert(&original).unwrap();

        let mut duplicate = clip("same bytes");
        duplicate.meta.source_app = Some("untrusted.import".into());
        let import_id = store.stage_import(&duplicate, "backup.json").unwrap();
        let report = store
            .restore_imports(&RestoreSelection {
                import_ids: vec![import_id],
            })
            .unwrap();
        assert_eq!(report.restored, 1);
        assert_eq!(report.deduplicated, 1);
        let live = store.list(1).unwrap().pop().unwrap();
        assert_eq!(live.id, original.id);
        assert_eq!(live.meta.source_app, None);

        let mut mismatched = clip("wrong size");
        mismatched.meta.byte_size += 1;
        assert!(store.stage_import(&mismatched, "backup.json").is_err());

        let mut unresolved = clip("spilled");
        unresolved.flavors[0].body = Body::Spilled {
            blob_ref: "b".repeat(64),
            byte_size: unresolved.meta.byte_size,
        };
        unresolved.content_hash = content_hash_from_flavors(&unresolved.flavors);
        assert!(store.stage_import(&unresolved, "backup.json").is_err());

        let staged = clip("tampered after staging");
        let import_id = store.stage_import(&staged, "backup.json").unwrap();
        let mut tampered = staged;
        tampered.meta.byte_size += 1;
        store
            .conn
            .execute(
                "UPDATE import_quarantine SET payload_json = ?1 WHERE import_id = ?2",
                params![serde_json::to_string(&tampered).unwrap(), import_id],
            )
            .unwrap();
        assert!(
            store
                .restore_imports(&RestoreSelection {
                    import_ids: vec![import_id],
                })
                .is_err()
        );
    }

    #[test]
    fn restore_import_revives_archived_duplicate_before_removing_quarantine() {
        let store = Store::open_in_memory().unwrap();
        let original = clip("archived duplicate");
        store.insert(&original).unwrap();
        store.set_archived(original.id, true).unwrap();
        assert!(store.list(10).unwrap().is_empty());

        let import_id = store
            .stage_import(&clip("archived duplicate"), "backup.json")
            .unwrap();
        let report = store
            .restore_imports(&RestoreSelection {
                import_ids: vec![import_id],
            })
            .unwrap();

        assert_eq!(report.restored, 1);
        assert_eq!(report.deduplicated, 1);
        assert_eq!(store.list(10).unwrap()[0].id, original.id);
        assert!(store.import_quarantine(10).unwrap().is_empty());
    }

    #[test]
    fn restore_import_replaces_expired_duplicate_with_active_copy() {
        let store = Store::open_in_memory().unwrap();
        let mut expired = clip("expired duplicate");
        expired.meta.expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        store.insert(&expired).unwrap();
        assert!(store.list(10).unwrap().is_empty());

        let import_id = store
            .stage_import(&clip("expired duplicate"), "backup.json")
            .unwrap();
        let report = store
            .restore_imports(&RestoreSelection {
                import_ids: vec![import_id],
            })
            .unwrap();

        assert_eq!(report.restored, 1);
        assert_eq!(report.deduplicated, 0);
        assert_eq!(store.list(10).unwrap().len(), 1);
        assert!(store.import_quarantine(10).unwrap().is_empty());
    }

    #[test]
    fn export_residency_updates_roll_back_as_one_transaction() {
        let store = Store::open_in_memory().unwrap();
        let first = clip("first export");
        let second = clip("second export");
        store.insert(&first).unwrap();
        store.insert(&second).unwrap();
        store
            .conn
            .execute_batch(&format!(
                r#"
                CREATE TRIGGER reject_one_export
                BEFORE UPDATE OF ever_exported ON clip_residency
                WHEN NEW.clip_id = '{}' AND NEW.ever_exported = 1
                BEGIN
                    SELECT RAISE(ABORT, 'test export failure');
                END;
                "#,
                first.id.to_string_repr()
            ))
            .unwrap();

        assert!(
            store
                .export_json(ExportSchemaVersion::V2, ArchiveVisibility::All, 10)
                .is_err()
        );
        assert!(!store.residency(first.id).unwrap().ever_exported);
        assert!(!store.residency(second.id).unwrap().ever_exported);
    }

    #[test]
    fn store_export_rejects_an_oversized_limit_instead_of_truncating() {
        let store = Store::open_in_memory().unwrap();
        assert!(
            store
                .export_json(
                    ExportSchemaVersion::V2,
                    ArchiveVisibility::Active,
                    MAX_EXPORT_CLIPS + 1,
                )
                .is_err()
        );
    }

    #[test]
    fn preference_residency_backup_and_manifest_are_bounded_sidecars() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("sidecars");
        store.insert(&clip).unwrap();
        store
            .set_preferred_flavor(clip.id, Some("text/plain"))
            .unwrap();
        store
            .record_residency(clip.id, ResidencyTransition::Exported)
            .unwrap();
        assert_eq!(
            store
                .annotations(clip.id)
                .unwrap()
                .preferred_mime
                .as_deref(),
            Some("text/plain")
        );
        assert!(store.residency(clip.id).unwrap().ever_exported);
        assert!(!AttachmentManifest::from_stored_clip(&clip).derived_index_present);
        let manifest = store.attachment_manifest(clip.id).unwrap();
        assert_eq!(manifest.flavors.len(), 1);
        assert!(manifest.derived_index_present);

        let now = Utc::now();
        store.record_verified_backup(now, &"a".repeat(64)).unwrap();
        let freshness = store
            .backup_freshness(now, Duration::from_secs(60))
            .unwrap()
            .unwrap();
        assert!(!freshness.stale);
        assert_eq!(freshness.checksum_prefix, "a".repeat(12));
    }

    #[test]
    fn future_backup_timestamp_is_corrupt_not_fresh() {
        let store = Store::open_in_memory().unwrap();
        let now = Utc::now();
        store
            .record_verified_backup(now + chrono::Duration::seconds(1), &"b".repeat(64))
            .unwrap();
        assert!(
            store
                .backup_freshness(now, Duration::from_secs(60))
                .is_err()
        );
    }
}
