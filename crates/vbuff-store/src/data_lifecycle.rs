//! Archive, annotation, export, recovery, and storage-maintenance contracts.
//!
//! Canonical clip bytes remain in `clips`/CAS. This module owns mutable
//! organization state and bounded maintenance operations around that core.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension as _, params};
use vbuff_types::{Clip, ClipId};

use crate::{Result, Store, StoreError, now_millis, raw_to_clip, row_to_clip};

mod contracts;
mod portability;

pub use contracts::{
    ArchiveVisibility, AttachmentManifest, BackupFreshness, BlobIntegrityReport, ClipAnnotations,
    CollectionRecord, CollectionRetentionPolicy, CollectionRetentionPreview, CompactionForecast,
    ExportSchemaVersion, FlavorManifest, FlavorStorage, GarbageCollectionPreview,
    ImportQuarantineEntry, PartialRestoreReport, ResidencyTransition, RestoreSelection,
    SensitiveDataResidency,
};
pub use portability::export_clips_json;

use contracts::{
    MAX_COLLECTION_ID_BYTES, MAX_COLLECTION_NAME_BYTES, MAX_EXPORT_CLIPS, require_lifecycle_update,
    to_i64, valid_identifier, valid_mime,
};

impl Store {
    pub fn set_archived(&self, id: ClipId, archived: bool) -> Result<()> {
        self.ensure_clip_exists(id)?;
        let changed = self.conn.execute(
            "UPDATE clip_annotations SET archived = ?1 WHERE clip_id = ?2",
            params![archived as i64, id.to_string_repr()],
        )?;
        require_lifecycle_update(changed, "clip annotation")
    }

    pub fn annotations(&self, id: ClipId) -> Result<ClipAnnotations> {
        self.ensure_clip_exists(id)?;
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
            .ok_or_else(|| StoreError::Corrupt("clip annotation row is missing".into()))
    }

    pub fn list_with_archive(
        &self,
        visibility: ArchiveVisibility,
        limit: usize,
    ) -> Result<Vec<Clip>> {
        let archive_clause = match visibility {
            ArchiveVisibility::Active => "AND a.archived = 0",
            ArchiveVisibility::Archived => "AND a.archived = 1",
            ArchiveVisibility::All => "",
        };
        let sql = format!(
            r#"
            SELECT c.id, c.content_hash, c.flavors, c.kind, c.created_at, c.updated_at,
                   c.byte_size, c.source_app, c.metadata_json, c.pinned, c.favorite
            FROM clips c
            JOIN clip_annotations a ON a.clip_id = c.id
            WHERE (c.expires_at IS NULL OR c.expires_at > ?1 OR EXISTS (
                SELECT 1 FROM session_protected p WHERE p.clip_id = c.id
            ))
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
        let changed = self.conn.execute(
            "UPDATE clip_annotations SET collection_id = ?1 WHERE clip_id = ?2",
            params![collection_id, id.to_string_repr()],
        )?;
        require_lifecycle_update(changed, "clip annotation")
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
        let mut applied = CollectionRetentionPreview {
            clip_ids: Vec::with_capacity(preview.clip_ids.len()),
            reclaimable_bytes: 0,
            truncated: preview.truncated,
        };
        for id in preview.clip_ids {
            let byte_size = transaction
                .query_row(
                    r#"
                    DELETE FROM clips
                    WHERE id = ?1 AND pinned = 0 AND favorite = 0
                      AND EXISTS (
                        SELECT 1 FROM clip_annotations AS annotations
                        WHERE annotations.clip_id = clips.id
                          AND annotations.collection_id = ?2
                          AND annotations.legal_hold = 0
                      )
                      AND NOT EXISTS (
                        SELECT 1 FROM session_protected AS protected
                        WHERE protected.clip_id = clips.id
                      )
                    RETURNING byte_size
                    "#,
                    params![id.to_string_repr(), collection_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if let Some(byte_size) = byte_size {
                applied.clip_ids.push(id);
                applied.reclaimable_bytes = applied
                    .reclaimable_bytes
                    .saturating_add(byte_size.max(0) as u64);
                continue;
            }
            let (clip_exists, annotation_exists): (bool, bool) = transaction.query_row(
                r#"
                SELECT EXISTS(SELECT 1 FROM clips WHERE id = ?1),
                       EXISTS(SELECT 1 FROM clip_annotations WHERE clip_id = ?1)
                "#,
                [id.to_string_repr()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if clip_exists && !annotation_exists {
                return Err(StoreError::Corrupt("clip annotation row is missing".into()));
            }
        }
        transaction.commit()?;
        if !applied.clip_ids.is_empty() {
            self.scrub_deleted_pages()?;
        }
        Ok(applied)
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
        let changed = self.conn.execute(
            &format!("UPDATE clip_residency SET {column} = 1 WHERE clip_id = ?1"),
            [id.to_string_repr()],
        )?;
        require_lifecycle_update(changed, "clip residency")
    }

    pub fn residency(&self, id: ClipId) -> Result<SensitiveDataResidency> {
        self.ensure_clip_exists(id)?;
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
            .ok_or_else(|| StoreError::Corrupt("clip residency row is missing".into()))
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
        let changed = self.conn.execute(
            "UPDATE clip_annotations SET preferred_mime = ?1 WHERE clip_id = ?2",
            params![mime, id.to_string_repr()],
        )?;
        require_lifecycle_update(changed, "clip annotation")
    }

    pub fn set_legal_hold(&self, id: ClipId, held: bool) -> Result<()> {
        self.ensure_clip_exists(id)?;
        let changed = self.conn.execute(
            "UPDATE clip_annotations SET legal_hold = ?1 WHERE clip_id = ?2",
            params![held as i64, id.to_string_repr()],
        )?;
        require_lifecycle_update(changed, "clip annotation")
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
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(StoreError::Corrupt("invalid backup checksum".into()));
        }
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

    pub(crate) fn ensure_not_legal_hold(&self, id: ClipId) -> Result<()> {
        crate::ensure_delete_eligible(&self.conn, id, false)
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

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_core::content_hash_from_flavors;
    use vbuff_types::{Body, ClipMeta, ContentKind, Flavor};

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

        let mut tampered = clip("tampered");
        tampered.content_hash = [7; 32];
        assert!(export_clips_json(&[tampered], ExportSchemaVersion::V2).is_err());

        let mut inconsistent = clip("inconsistent");
        inconsistent.meta.sensitivity_reason = Some(vbuff_types::SensitivityReason::CaptureRule);
        assert!(export_clips_json(&[inconsistent], ExportSchemaVersion::V2).is_err());
    }

    #[test]
    fn archive_annotations_and_legal_hold_are_separate_from_canonical_clip() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("immutable");
        store.insert(&clip).unwrap();
        store.insert(&clip).unwrap();
        store.set_archived(clip.id, true).unwrap();
        store.set_legal_hold(clip.id, true).unwrap();
        assert!(store.list(10).unwrap().is_empty());
        assert!(store.near_duplicate_group(clip.id, 10).unwrap().is_empty());
        assert!(store.find_near_text("immutable", 0, 10).unwrap().is_empty());
        assert!(store.suggested_pins(2, 10).unwrap().is_empty());
        assert_eq!(
            store
                .list_with_archive(ArchiveVisibility::Archived, 10)
                .unwrap()
                .len(),
            1
        );
        assert!(store.delete(clip.id).is_err());
        assert!(
            store
                .delete_with_grace_inner(
                    clip.id,
                    &[7; 32],
                    Duration::from_secs(60),
                    crate::DeletionReason::User,
                    false,
                )
                .is_err()
        );
        assert!(store.grace_bin(10).unwrap().is_empty());
        store.set_legal_hold(clip.id, false).unwrap();
        store.set_pinned(clip.id, true).unwrap();
        assert!(
            store
                .delete_with_grace_inner(
                    clip.id,
                    &[7; 32],
                    Duration::from_secs(60),
                    crate::DeletionReason::Retention,
                    false,
                )
                .is_err()
        );
        assert!(store.grace_bin(10).unwrap().is_empty());
        store.set_pinned(clip.id, false).unwrap();
        store.delete(clip.id).unwrap();
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
    }

    #[test]
    fn import_quarantine_fails_closed_on_sensitivity_reason() {
        let store = Store::open_in_memory().unwrap();
        let mut clip = clip("reason-marked import");
        clip.meta.sensitivity_reason = Some(vbuff_types::SensitivityReason::CaptureRule);
        store.stage_import(&clip, "backup.json").unwrap();
        assert!(store.import_quarantine(1).unwrap()[0].sensitive);
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

    #[test]
    fn missing_lifecycle_sidecars_and_corrupt_backup_checksum_fail_closed() {
        let store = Store::open_in_memory().unwrap();
        let clip = clip("missing sidecar");
        store.insert(&clip).unwrap();
        store
            .conn
            .execute(
                "DELETE FROM clip_residency WHERE clip_id = ?1",
                [clip.id.to_string_repr()],
            )
            .unwrap();
        assert!(
            store
                .record_residency(clip.id, ResidencyTransition::Exported)
                .is_err()
        );
        assert!(store.residency(clip.id).is_err());
        assert!(
            store
                .export_json(ExportSchemaVersion::V2, ArchiveVisibility::All, 10)
                .is_err()
        );

        store
            .conn
            .execute(
                "DELETE FROM clip_annotations WHERE clip_id = ?1",
                [clip.id.to_string_repr()],
            )
            .unwrap();
        assert!(store.set_archived(clip.id, true).is_err());
        assert!(store.annotations(clip.id).is_err());
        assert!(
            store
                .apply_batch(&[crate::StoreMutation::Delete { id: clip.id }])
                .is_err()
        );
        assert!(matches!(store.delete(clip.id), Err(StoreError::Corrupt(_))));
        store.clear_all().unwrap();
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(store.enforce_cap(0).unwrap(), 0);
        assert!(store.list(10).unwrap().is_empty());

        store
            .conn
            .execute(
                "INSERT INTO backup_state(singleton, verified_at, checksum) VALUES (1, ?1, ?2)",
                params![now_millis(), "z".repeat(64)],
            )
            .unwrap();
        assert!(
            store
                .backup_freshness(Utc::now(), Duration::from_secs(60))
                .is_err()
        );
    }
}
