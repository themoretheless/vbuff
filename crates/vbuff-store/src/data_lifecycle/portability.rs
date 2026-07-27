use chrono::DateTime;
use rusqlite::{OptionalExtension as _, params};
use serde::Serialize;
use vbuff_core::content_hash_from_flavors;
use vbuff_types::{Body, Clip, ClipId};

use super::contracts::{
    ArchiveVisibility, ExportSchemaVersion, ImportQuarantineEntry, MAX_EXPORT_BYTES,
    MAX_EXPORT_CLIPS, MAX_IMPORT_BYTES, MAX_IMPORT_SOURCE_BYTES, MAX_RESTORE_SELECTION,
    PartialRestoreReport, RestoreSelection, require_lifecycle_update, to_i64, valid_identifier,
};
use crate::{Result, Store, StoreError, now_millis};

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
        if clip.meta.sensitivity_reason.is_some() && !clip.meta.sensitive {
            return Err(StoreError::Maintenance(
                "portable export contains inconsistent sensitivity metadata".into(),
            ));
        }
        let body_bytes = portable_inline_size(clip).ok_or_else(|| {
            StoreError::Maintenance("portable export requires resolved inline bodies".into())
        })?;
        if body_bytes != clip.meta.byte_size {
            return Err(StoreError::Maintenance(
                "portable export body size does not match clip metadata".into(),
            ));
        }
        if content_hash_from_flavors(&clip.flavors) != clip.content_hash {
            return Err(StoreError::Maintenance(
                "portable export content hash does not match clip bodies".into(),
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
        let payload = serde_json::to_string(clip)?;
        if payload.len() > MAX_IMPORT_BYTES {
            return Err(StoreError::Maintenance(
                "import payload exceeds bound".into(),
            ));
        }
        let import_id = ClipId::new().to_string_repr();
        let source_fingerprint = source_fingerprint(source);
        let sensitive = clip.meta.sensitive || clip.meta.sensitivity_reason.is_some();
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
                sensitive as i64,
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
            let duplicate: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM clips WHERE content_hash = ?1)",
                [clip.content_hash.as_slice()],
                |row| row.get(0),
            )?;
            if duplicate {
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
        Ok(self.conn.execute(
            "DELETE FROM import_quarantine WHERE import_id = ?1",
            [import_id],
        )? == 1)
    }

    pub fn export_json(
        &self,
        version: ExportSchemaVersion,
        visibility: ArchiveVisibility,
        limit: usize,
    ) -> Result<String> {
        let clips = self.list_with_archive(visibility, limit.min(MAX_EXPORT_CLIPS))?;
        let output = export_clips_json(&clips, version)?;
        let transaction = self.conn.unchecked_transaction()?;
        for clip in &clips {
            let changed = transaction.execute(
                "UPDATE clip_residency SET ever_exported = 1 WHERE clip_id = ?1",
                [clip.id.to_string_repr()],
            )?;
            require_lifecycle_update(changed, "clip residency")?;
        }
        transaction.commit()?;
        Ok(output)
    }
}

fn source_fingerprint(source: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-import-source-v1\0");
    hasher.update(source.as_bytes());
    hasher.finalize().to_hex()[..16].to_owned()
}

fn portable_inline_size(clip: &Clip) -> Option<u64> {
    clip.flavors
        .iter()
        .try_fold(0_u64, |total, flavor| match &flavor.body {
            Body::Inline(bytes) => total.checked_add(bytes.len() as u64),
            Body::Spilled { .. } => None,
        })
}
