//! One-time, read-only SQLite import. Publication happens only after validation.
use crate::{Result, Store, StoreError};
use duckdb::{Connection, params_from_iter, types::Value};
use std::path::Path;

pub(crate) fn migrate_if_needed(destination: &Path) -> Result<()> {
    if destination.exists()
        || destination.file_name().and_then(|v| v.to_str()) != Some("history.duckdb")
    {
        return Ok(());
    }
    let source = destination.with_file_name("history.db");
    if !source.exists() || source == destination {
        return Ok(());
    }
    let sqlite =
        rusqlite::Connection::open_with_flags(&source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    sqlite.execute_batch("BEGIN")?;
    let check: String = sqlite.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(StoreError::Migration(
            "SQLite integrity check failed; original preserved".into(),
        ));
    }
    let version: i64 = sqlite.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if !(1..=7).contains(&version) {
        return Err(StoreError::Migration(format!(
            "unsupported SQLite schema {version}"
        )));
    }
    let staging = destination.with_extension(format!("{}.migrating", vbuff_types::ClipId::new()));
    let result = (|| {
        let mut target = Connection::open_with_flags(&staging, crate::database_config()?)?;
        crate::harden_file_permissions(&staging)?;
        let tx = target.transaction()?;
        Store::apply_migrations(&tx)?;
        tx.execute_batch(crate::tags::TAG_SCHEMA)?;
        let mut tables = tx.prepare("SELECT table_name FROM information_schema.tables WHERE table_schema = 'main' AND table_type = 'BASE TABLE' AND table_name NOT IN ('store_metadata','blob_refs') ORDER BY table_name")?;
        let names = tables
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<duckdb::Result<Vec<_>>>()?;
        drop(tables);
        for table in names {
            let exists: bool = sqlite.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [&table],
                |r| r.get(0),
            )?;
            if !exists {
                continue;
            }
            let mut column_query = sqlite.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
            let column_info = column_query
                .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let columns = column_info
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
            let columns_sql = columns
                .iter()
                .map(|c| quote(c))
                .collect::<Vec<_>>()
                .join(",");
            let mut keys = column_info
                .iter()
                .filter(|(_, rank)| *rank > 0)
                .collect::<Vec<_>>();
            keys.sort_by_key(|(_, rank)| *rank);
            if keys.is_empty() {
                return Err(StoreError::Migration(format!(
                    "missing primary key in {table}"
                )));
            }
            let order = keys
                .iter()
                .map(|(name, _)| quote(name))
                .collect::<Vec<_>>()
                .join(",");
            let select = format!("SELECT {columns_sql} FROM \"{table}\" ORDER BY {order}");
            let mut read = sqlite.prepare(&select)?;
            let mut rows = read.query([])?;
            tx.execute(&format!("DELETE FROM \"{table}\""), [])?;
            let placeholders = (1..=columns.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>()
                .join(",");
            let mut write = tx.prepare(&format!(
                "INSERT INTO \"{table}\" ({columns_sql}) VALUES ({placeholders})"
            ))?;
            let mut expected = 0_i64;
            let mut source_hash = blake3::Hasher::new();
            while let Some(row) = rows.next()? {
                let values = (0..columns.len())
                    .map(|i| {
                        Ok(match row.get_ref(i)? {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(v) => Value::BigInt(v),
                            rusqlite::types::ValueRef::Real(v) => Value::Double(v),
                            rusqlite::types::ValueRef::Text(v) => Value::Text(
                                std::str::from_utf8(v)
                                    .map_err(|_| {
                                        StoreError::Migration("invalid UTF-8 in source".into())
                                    })?
                                    .into(),
                            ),
                            rusqlite::types::ValueRef::Blob(v) => Value::Blob(v.to_vec()),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                hash_values(&mut source_hash, &values)?;
                write.execute(params_from_iter(values))?;
                expected += 1;
            }
            let actual: i64 =
                tx.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
                    r.get(0)
                })?;
            if expected != actual {
                return Err(StoreError::Migration(format!(
                    "row count mismatch in {table}"
                )));
            }
            let mut verify = tx.prepare(&select)?;
            let mut copied = verify.query([])?;
            let mut destination_hash = blake3::Hasher::new();
            while let Some(row) = copied.next()? {
                let values = (0..columns.len())
                    .map(|i| row.get::<_, Value>(i))
                    .collect::<duckdb::Result<Vec<_>>>()?;
                hash_values(&mut destination_hash, &values)?;
            }
            if source_hash.finalize() != destination_hash.finalize() {
                return Err(StoreError::Migration(format!(
                    "content verification failed in {table}"
                )));
            }
        }
        if version == 7 {
            for table in ["clip_annotations", "clip_residency"] {
                let missing: i64 = tx.query_row(
                    &format!(
                        "SELECT COUNT(*) FROM clips WHERE id NOT IN (SELECT clip_id FROM {table})"
                    ),
                    [],
                    |r| r.get(0),
                )?;
                if missing != 0 {
                    return Err(StoreError::Migration(format!(
                        "missing lifecycle records in {table}"
                    )));
                }
            }
        }
        tx.execute_batch("INSERT OR IGNORE INTO clip_annotations(clip_id) SELECT id FROM clips; INSERT OR IGNORE INTO clip_residency(clip_id) SELECT id FROM clips;")?;
        // Restart native sequences above the imported recency/event identifiers.
        for (sequence, table, column) in [
            ("clip_sequence", "clips", "seq"),
            ("merge_sequence", "dedup_merge_ledger", "event_seq"),
        ] {
            let maximum: i64 = tx.query_row(
                &format!("SELECT COALESCE(MAX({column}),0) FROM {table}"),
                [],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE store_metadata SET value = $1 WHERE key = $2",
                duckdb::params![maximum, sequence],
            )?;
        }
        tx.commit()?;
        let cas = crate::cas::CasStore::new(
            destination.parent().unwrap_or(Path::new(".")).join("blobs"),
        )?;
        let new_cas = crate::cas::CasStore::new(
            destination
                .parent()
                .unwrap_or(Path::new("."))
                .join("duckdb-blobs"),
        )?;
        let mut records = target.prepare(&format!(
            "SELECT {} FROM clips",
            crate::clip_projection("clips")
        ))?;
        let rows = records.query_map([], crate::row_to_clip)?;
        let references = target.unchecked_transaction()?;
        references.execute("DELETE FROM blob_refs", [])?;
        for raw in rows {
            let mut clip = crate::raw_to_clip(raw?)?;
            Store::update_blob_references(&references, &clip.flavors, clip.meta.kind, 1)?;
            for flavor in &clip.flavors {
                if let vbuff_types::Body::Spilled {
                    blob_ref,
                    byte_size,
                } = &flavor.body
                {
                    new_cas.copy_from(&cas, clip.meta.kind, blob_ref, *byte_size)?;
                }
            }
            cas.hydrate_flavors(&mut clip.flavors, clip.meta.kind)?;
            if vbuff_core::content_hash_from_flavors(&clip.flavors) != clip.content_hash {
                return Err(StoreError::Migration(
                    "clip content hash mismatch; source preserved".into(),
                ));
            }
            let projection = if clip.meta.sensitive {
                String::new()
            } else {
                crate::searchable_projection(&clip, 1_048_576)
            };
            references.execute(
                "UPDATE clips SET item_text = $1 WHERE id = $2",
                duckdb::params![projection, clip.id.to_string_repr()],
            )?;
            references.execute(
                "DELETE FROM clip_facets WHERE clip_id = $1",
                [clip.id.to_string_repr()],
            )?;
            if !clip.meta.sensitive
                && let Some(text) = clip.primary_text()
            {
                for facet in vbuff_core::facets::extract_facets(text, clip.meta.kind, false) {
                    references.execute(
                        "INSERT OR IGNORE INTO clip_facets VALUES ($1,$2,$3)",
                        duckdb::params![clip.id.to_string_repr(), facet.key, facet.value],
                    )?;
                }
            }
        }
        references.commit()?;
        drop(records);
        for table in [
            "clip_annotations",
            "clip_residency",
            "clip_facets",
            "content_audit",
            "dedup_merge_ledger",
        ] {
            let orphaned: i64 = target.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {table} WHERE clip_id NOT IN (SELECT id FROM clips)"
                ),
                [],
                |r| r.get(0),
            )?;
            if orphaned != 0 {
                return Err(StoreError::Migration(format!(
                    "orphaned lifecycle records in {table}"
                )));
            }
        }
        target.execute_batch("CHECKPOINT")?;
        drop(target);
        // No replacement of an existing destination, including one created concurrently.
        std::fs::hard_link(&staging, destination).map_err(StoreError::Io)?;
        std::fs::remove_file(&staging).map_err(StoreError::Io)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

fn hash_values(hash: &mut blake3::Hasher, values: &[Value]) -> Result<()> {
    hash.update(&(values.len() as u64).to_le_bytes());
    for value in values {
        match value {
            Value::Null => {
                hash.update(&[0]);
            }
            Value::BigInt(v) => {
                hash.update(&[1]);
                hash.update(&v.to_le_bytes());
            }
            Value::Double(v) => {
                hash.update(&[2]);
                hash.update(&v.to_bits().to_le_bytes());
            }
            Value::Text(v) => {
                hash.update(&[3]);
                hash.update(&(v.len() as u64).to_le_bytes());
                hash.update(v.as_bytes());
            }
            Value::Blob(v) => {
                hash.update(&[4]);
                hash.update(&(v.len() as u64).to_le_bytes());
                hash.update(v);
            }
            _ => return Err(StoreError::Migration("unexpected copied value type".into())),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn source(path: &Path, flavors: &str) {
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute_batch("CREATE TABLE clips (seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE, content_hash BLOB NOT NULL,flavors TEXT NOT NULL,kind INTEGER NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,byte_size INTEGER NOT NULL,source_app TEXT,preview TEXT NOT NULL DEFAULT '',pinned INTEGER NOT NULL DEFAULT 0,favorite INTEGER NOT NULL DEFAULT 0); PRAGMA user_version=1;").unwrap();
        db.execute("INSERT INTO clips VALUES(41,?1,?2,?3,0,1700000000000,1700000000000,5,'fixture','hello',1,1)", rusqlite::params![vbuff_types::ClipId::new().to_string_repr(), vbuff_core::content_hash_from_flavors(&serde_json::from_str::<Vec<vbuff_types::Flavor>>(flavors).unwrap_or_default()).as_slice(), flavors]).unwrap();
    }
    pub(crate) fn migrates_v1() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("history.db");
        source(
            &old,
            &serde_json::to_string(&vec![vbuff_types::Flavor::inline(
                "text/plain",
                b"hello".to_vec(),
            )])
            .unwrap(),
        );
        let original = std::fs::read(&old).unwrap();
        let destination = dir.path().join("history.duckdb");
        migrate_if_needed(&destination).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), original);
        let store = Store::open(&destination).unwrap();
        let clips = store.list(10).unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].primary_text(), Some("hello"));
        assert_eq!(store.search("hello", 10).unwrap().len(), 1);
        assert!(clips[0].pinned && clips[0].favorite);
        assert_eq!(
            store
                .conn
                .query_row("SELECT seq FROM clips", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            41
        );
        assert!(store.doctor().unwrap().is_healthy());
        store.insert(&clips[0]).unwrap();
        assert!(
            store
                .conn
                .query_row("SELECT seq FROM clips", [], |r| r.get::<_, i64>(0))
                .unwrap()
                > 41
        );
    }
    pub(crate) fn failed_import_preserves_source() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("history.db");
        source(&old, "not JSON");
        let original = std::fs::read(&old).unwrap();
        let destination = dir.path().join("history.duckdb");
        assert!(migrate_if_needed(&destination).is_err());
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&old).unwrap(), original);
    }
    pub(crate) fn missing_blob_blocks_publication() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("history.db");
        let flavor = vbuff_types::Flavor {
            body: vbuff_types::Body::Spilled {
                blob_ref: "0".repeat(64),
                byte_size: 5,
            },
            ..vbuff_types::Flavor::inline("text/plain", vec![])
        };
        source(&old, &serde_json::to_string(&vec![flavor]).unwrap());
        let original = std::fs::read(&old).unwrap();
        let destination = dir.path().join("history.duckdb");
        assert!(migrate_if_needed(&destination).is_err());
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&old).unwrap(), original);
    }
}
