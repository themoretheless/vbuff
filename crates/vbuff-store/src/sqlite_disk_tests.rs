//! Integration tests for `vbuff-store` against a real on-disk SQLite database.

use crate::{DeletionReason, Store};
use vbuff_core::content_hash_from_flavors;
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

fn make_clip(text: &str) -> Clip {
    let flavors = vec![Flavor::inline(
        "text/plain;charset=utf-8",
        text.as_bytes().to_vec(),
    )];
    let content_hash = content_hash_from_flavors(&flavors);
    Clip {
        id: ClipId::new(),
        flavors,
        content_hash,
        meta: ClipMeta::now(
            ContentKind::Text,
            text.len() as u64,
            Some("integration.test".into()),
        ),
        pinned: false,
        favorite: false,
    }
}

#[test]
fn persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");

    {
        let store = Store::open(&db).unwrap();
        store.insert(&make_clip("persisted clip")).unwrap();
        store.insert(&make_clip("another clip")).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    // Reopen the same file: data should still be there.
    let store = Store::open(&db).unwrap();
    assert_eq!(store.count().unwrap(), 2);
    let listed = store.list(10).unwrap();
    assert_eq!(listed[0].primary_text(), Some("another clip"));
}

#[test]
fn migrates_schema_five_to_lifecycle_schema_without_losing_clips() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let clip = make_clip("schema five lifecycle migration");
    {
        let store = Store::open(&db).unwrap();
        store.insert(&clip).unwrap();
    }
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute_batch(
                r#"
                DROP INDEX idx_clips_normalized_hash;
                DROP TABLE dedup_merge_ledger;
                DROP TABLE grace_bin;
                DROP TABLE retention_rules;
                ALTER TABLE clips DROP COLUMN normalized_hash;
                PRAGMA user_version = 5;
                "#,
            )
            .unwrap();
    }

    let store = Store::open(&db).unwrap();
    assert_eq!(
        store.doctor().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    assert_eq!(
        store.list(1).unwrap()[0].primary_text(),
        clip.primary_text()
    );
    assert_eq!(store.retention_rules().unwrap().len(), 10);
    assert_eq!(store.backfill_normalized_fingerprints(10).unwrap(), 0);
    assert_eq!(store.near_duplicate_group(clip.id, 10).unwrap().len(), 1);
}

#[test]
fn migrates_schema_six_to_seven_and_backfills_lifecycle_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let mut clip = make_clip("schema six lifecycle migration");
    clip.pinned = true;
    clip.favorite = true;
    {
        let store = Store::open(&db).unwrap();
        store.insert(&clip).unwrap();
    }
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = OFF;
                DROP TRIGGER clips_lifecycle_ai;
                DROP TABLE clip_annotations;
                DROP TABLE clip_residency;
                DROP TABLE collection_policies;
                DROP TABLE blob_quarantine;
                DROP TABLE backup_state;
                DROP TABLE import_quarantine;
                PRAGMA user_version = 6;
                "#,
            )
            .unwrap();
    }

    let store = Store::open(&db).unwrap();
    assert_eq!(
        store.doctor().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    let restored = store.list(1).unwrap().pop().unwrap();
    assert_eq!(restored.id, clip.id);
    assert_eq!(restored.content_hash, clip.content_hash);
    assert_eq!(restored.flavors, clip.flavors);
    assert_eq!(
        restored.meta.created_at.timestamp_millis(),
        clip.meta.created_at.timestamp_millis()
    );
    assert_eq!(restored.meta.source_app, clip.meta.source_app);
    assert_eq!(restored.pinned, clip.pinned);
    assert_eq!(restored.favorite, clip.favorite);
    assert_eq!(store.annotations(clip.id).unwrap(), Default::default());
    assert_eq!(
        store.residency(clip.id).unwrap(),
        crate::SensitiveDataResidency {
            ever_on_disk: true,
            ever_synced: false,
            ever_exported: false,
        }
    );

    let connection = rusqlite::Connection::open(&db).unwrap();
    let lifecycle_tables: i64 = connection
        .query_row(
            r#"
            SELECT COUNT(*) FROM sqlite_master
            WHERE type = 'table' AND name IN (
                'collection_policies', 'clip_annotations', 'clip_residency',
                'blob_quarantine', 'backup_state', 'import_quarantine'
            )
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lifecycle_tables, 6);
    let lifecycle_trigger: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name = 'clips_lifecycle_ai'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lifecycle_trigger, 1);
}

#[test]
fn dedup_and_cap_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();

    // Insert 10 unique clips.
    for i in 0..10 {
        store
            .insert(&make_clip(&format!("clip number {i}")))
            .unwrap();
    }
    assert_eq!(store.count().unwrap(), 10);

    // Re-insert a duplicate; count stays the same (dedup), clip floats to top.
    store.insert(&make_clip("clip number 3")).unwrap();
    assert_eq!(store.count().unwrap(), 10);
    assert_eq!(
        store.list(1).unwrap()[0].primary_text(),
        Some("clip number 3")
    );

    // Enforce a cap of 4: 6 oldest unpinned clips are evicted.
    let evicted = store.enforce_cap(4).unwrap();
    assert_eq!(evicted, 6);
    assert_eq!(store.count().unwrap(), 4);
}

#[test]
fn wal_files_are_created() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    store.insert(&make_clip("trigger wal")).unwrap();
    // WAL mode produces a sidecar `-wal` file.
    let wal = dir.path().join("history.db-wal");
    assert!(wal.exists(), "expected WAL sidecar file to exist");
}

#[cfg(unix)]
#[test]
fn database_and_cas_paths_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    store.insert(&make_clip("private permissions")).unwrap();

    assert_eq!(
        dir.path().metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(db.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    let wal = dir.path().join("history.db-wal");
    assert_eq!(wal.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    let blobs = dir.path().join("blobs");
    assert_eq!(
        blobs.metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn expired_sensitive_clip_is_scrubbed_from_database_and_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let canary = "VBUFF_EXPIRING_CANARY_7F3D9A";
    let mut clip = make_clip(canary);
    clip.meta.sensitive = true;
    clip.meta.sync_eligible = false;
    clip.meta.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
    let store = Store::open(&db).unwrap();
    store.insert(&clip).unwrap();

    // The sensitive row never persists the normalized-text correlation token:
    // neither in the normalized_hash column nor as raw bytes in the files
    // scanned below.
    let correlation_token = crate::normalized_text_fingerprint(canary)
        .expect("canary text normalizes to a fingerprint");
    {
        let inspection = rusqlite::Connection::open(&db).unwrap();
        let stored: Option<Vec<u8>> = inspection
            .query_row("SELECT normalized_hash FROM clips", [], |row| row.get(0))
            .unwrap();
        assert!(stored.is_none(), "sensitive row kept a normalized_hash");
    }

    assert_eq!(store.purge_expired().unwrap(), 1);
    // The deletion only marked the WAL dirty; run the deferred scrub so the
    // canary's pre-delete frames are truncated away before the scan below.
    assert!(store.scrub_wal_if_dirty().unwrap());
    drop(store);

    for path in [db.clone(), dir.path().join("history.db-wal")] {
        if path.exists() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(canary.len())
                    .any(|window| window == canary.as_bytes()),
                "sensitive canary remained in {}",
                path.display()
            );
            assert!(
                !bytes
                    .windows(correlation_token.len())
                    .any(|window| window == correlation_token.as_slice()),
                "normalized-hash correlation token remained in {}",
                path.display()
            );
        }
    }
}

#[test]
fn open_scrubs_stale_normalized_hash_from_sensitive_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let clip = make_clip("stale correlation token");
    {
        let store = Store::open(&db).unwrap();
        store.insert(&clip).unwrap();
    }

    // Simulate a row reclassified sensitive by an older binary whose clawback
    // left normalized_hash intact.
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute(
                "UPDATE clips SET metadata_json = json_set(metadata_json, '$.sensitive', 1)",
                [],
            )
            .unwrap();
        let stale: Option<Vec<u8>> = connection
            .query_row("SELECT normalized_hash FROM clips", [], |row| row.get(0))
            .unwrap();
        assert!(stale.is_some(), "fixture must start with a stale hash");
    }

    let reopened = Store::open(&db).unwrap();
    let inspection = rusqlite::Connection::open(&db).unwrap();
    let scrubbed: Option<Vec<u8>> = inspection
        .query_row("SELECT normalized_hash FROM clips", [], |row| row.get(0))
        .unwrap();
    assert!(
        scrubbed.is_none(),
        "open-time scrub must null the stale normalized_hash"
    );
    assert!(
        reopened
            .near_duplicate_group(clip.id, 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn bundled_sqlite_includes_the_wal_reset_fix() {
    let version = rusqlite::version_number();
    let fixed_350_backport = (3_050_007..3_051_000).contains(&version);
    let fixed_mainline = version >= 3_051_003;

    assert!(
        fixed_350_backport || fixed_mainline,
        "SQLite {} is in or predates the WAL-reset bug range",
        rusqlite::version()
    );
}

#[test]
fn large_bodies_use_sharded_refcounted_cas_and_hydrate_on_read() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let bytes = vec![37_u8; 300 * 300 * 4];
    let flavors = vec![Flavor::inline(
        "image/x-vbuff-rgba;width=300;height=300",
        bytes.clone(),
    )];
    let clip = Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(ContentKind::Image, bytes.len() as u64, None),
        pinned: false,
        favorite: false,
    };

    store.insert(&clip).unwrap();
    let files = regular_files(&dir.path().join("blobs"));
    assert_eq!(files.len(), 1);
    let relative = files[0]
        .strip_prefix(dir.path().join("blobs"))
        .unwrap()
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(relative.len(), 4);
    assert_eq!(relative[0], "image");
    assert_eq!(relative[1].len(), 2);
    assert_eq!(relative[2].len(), 2);
    assert_eq!(relative[3].len(), 64);

    let loaded = store.list(1).unwrap().pop().unwrap();
    assert_eq!(
        loaded.flavors[0].body.inline_bytes(),
        Some(bytes.as_slice())
    );
    assert_eq!(store.gc_blobs().unwrap(), 0);
    store.delete(clip.id).unwrap();
    assert_eq!(store.gc_blobs().unwrap(), 1);
    assert!(regular_files(&dir.path().join("blobs")).is_empty());
}

#[test]
fn gc_dry_run_and_blob_scrubber_report_before_mutating() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();

    let orphan_hash = blake3::hash(b"orphan preview").to_hex().to_string();
    let orphan = dir
        .path()
        .join("blobs")
        .join("text")
        .join(&orphan_hash[0..2])
        .join(&orphan_hash[2..4])
        .join(&orphan_hash);
    std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    std::fs::write(&orphan, b"orphan preview").unwrap();
    let preview = store.gc_dry_run().unwrap();
    assert_eq!(preview.blob_count, 1);
    assert_eq!(preview.reclaimable_bytes, 14);
    assert!(orphan.exists());
    assert_eq!(store.gc_blobs().unwrap(), 1);

    let live = large_clip(ContentKind::Image, "image/png", vec![83_u8; 300 * 300 * 4]);
    store.insert(&live).unwrap();
    let live_path = regular_files(&dir.path().join("blobs"))
        .into_iter()
        .find(|path| !path.to_string_lossy().contains("quarantine"))
        .unwrap();
    std::fs::write(&live_path, b"damaged").unwrap();
    let report = store.scrub_blobs(16).unwrap();
    assert_eq!(report.checked, 1);
    assert_eq!(report.quarantined, 1);
    assert!(!live_path.exists());
}

#[test]
fn blob_scrubber_cursor_advances_past_a_healthy_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    store
        .insert(&large_clip(
            ContentKind::Image,
            "image/png",
            vec![11_u8; 300 * 300 * 4],
        ))
        .unwrap();
    store
        .insert(&large_clip(
            ContentKind::Image,
            "image/png",
            vec![12_u8; 300 * 300 * 4],
        ))
        .unwrap();

    let mut files = regular_files(&dir.path().join("blobs"));
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    assert_eq!(files.len(), 2);
    std::fs::write(&files[1], b"damaged second blob").unwrap();

    let first = store.scrub_blobs(1).unwrap();
    assert_eq!(first.checked, 1);
    assert_eq!(first.healthy, 1);
    assert_eq!(first.remaining, 1);
    assert!(files[0].exists());

    let second = store.scrub_blobs(1).unwrap();
    assert_eq!(second.checked, 1);
    assert_eq!(second.quarantined, 1);
    assert_eq!(second.remaining, 0);
    assert!(!files[1].exists());
}

#[test]
fn encrypted_grace_bin_is_self_contained_and_scrubs_large_cas_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let canary = "VBUFF_GRACE_CAS_CANARY_91D4";
    let text = canary.repeat(40_000);
    let clip = make_clip(&text);
    let key = [41_u8; 32];

    store.insert(&clip).unwrap();
    assert_eq!(regular_files(&dir.path().join("blobs")).len(), 1);
    let recovery_id = store
        .delete_with_grace(
            clip.id,
            &key,
            std::time::Duration::from_secs(60),
            DeletionReason::User,
        )
        .unwrap();
    assert_eq!(store.gc_blobs().unwrap(), 1);
    assert!(regular_files(&dir.path().join("blobs")).is_empty());
    // The grace delete only marked the WAL dirty; run the deferred scrub so
    // pre-delete frames are truncated away before the plaintext scan below.
    assert!(store.scrub_wal_if_dirty().unwrap());

    for path in [db.clone(), dir.path().join("history.db-wal")] {
        if path.exists() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(canary.len())
                    .any(|window| window == canary.as_bytes()),
                "grace-bin plaintext remained in {}",
                path.display()
            );
        }
    }

    assert_eq!(
        store.restore_from_grace(&recovery_id, &key).unwrap(),
        clip.id
    );
    assert_eq!(
        store.list(1).unwrap()[0].primary_text(),
        Some(text.as_str())
    );
}

fn large_clip(kind: ContentKind, mime: &str, bytes: Vec<u8>) -> Clip {
    let flavors = vec![Flavor::inline(mime, bytes.clone())];
    Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(kind, bytes.len() as u64, None),
        pinned: false,
        favorite: false,
    }
}

#[test]
fn cas_refcounts_are_scoped_by_kind() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let bytes = vec![42_u8; 300 * 300 * 4];
    let blob_hash = blake3::hash(&bytes).to_hex().to_string();
    let image = large_clip(ContentKind::Image, "image/png", bytes.clone());
    let file = large_clip(ContentKind::File, "application/octet-stream", bytes);

    store.insert(&image).unwrap();
    store.insert(&file).unwrap();
    let inspection = rusqlite::Connection::open(&db).unwrap();
    let rows: i64 = inspection
        .query_row(
            "SELECT COUNT(*) FROM blob_refs WHERE hash = ?1",
            [&blob_hash],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 2);
    assert_eq!(regular_files(&dir.path().join("blobs")).len(), 2);

    store.delete(image.id).unwrap();
    assert_eq!(store.gc_blobs().unwrap(), 1);
    assert_eq!(store.list(1).unwrap()[0].id, file.id);
    assert_eq!(regular_files(&dir.path().join("blobs")).len(), 1);
}

#[test]
fn cas_refcount_tracks_repeated_flavors_and_collects_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let bytes = vec![17_u8; 300 * 300 * 4];
    let blob_hash = blake3::hash(&bytes).to_hex().to_string();
    let flavors = vec![
        Flavor::inline("image/png", bytes.clone()),
        Flavor::inline("image/x-identical-copy", bytes.clone()),
    ];
    let clip = Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(ContentKind::Image, (bytes.len() * 2) as u64, None),
        pinned: false,
        favorite: false,
    };

    store.insert(&clip).unwrap();
    let inspection = rusqlite::Connection::open(&db).unwrap();
    let refcount: i64 = inspection
        .query_row(
            "SELECT refcount FROM blob_refs WHERE hash = ?1 AND kind = 3",
            [&blob_hash],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(refcount, 2);
    store.delete(clip.id).unwrap();
    assert_eq!(store.gc_blobs().unwrap(), 1);
    assert!(regular_files(&dir.path().join("blobs")).is_empty());
}

#[test]
fn startup_collects_blob_stranded_before_database_commit() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    drop(Store::open(&db).unwrap());
    let hash = blake3::hash(b"stranded").to_hex().to_string();
    let orphan = dir
        .path()
        .join("blobs")
        .join("text")
        .join(&hash[0..2])
        .join(&hash[2..4])
        .join(&hash);
    std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    std::fs::write(&orphan, b"stranded").unwrap();
    assert!(orphan.exists());

    let _store = Store::open(&db).unwrap();
    assert!(!orphan.exists());
}

#[test]
fn sensitive_large_bodies_never_spill() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let bytes = vec![91_u8; 300 * 300 * 4];
    let flavors = vec![Flavor::inline(
        "image/x-vbuff-rgba;width=300;height=300",
        bytes.clone(),
    )];
    let mut meta = ClipMeta::now(ContentKind::Image, bytes.len() as u64, None);
    meta.sensitive = true;
    meta.sync_eligible = false;
    let clip = Clip {
        id: ClipId::new(),
        content_hash: content_hash_from_flavors(&flavors),
        flavors,
        meta,
        pinned: false,
        favorite: false,
    };

    store.insert(&clip).unwrap();
    assert!(regular_files(&dir.path().join("blobs")).is_empty());
}

fn regular_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    if !root.exists() {
        return Vec::new();
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn on_disk_migration_verifies_then_removes_plaintext_rollback_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let clip = make_clip("v1 row survives");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE clips (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                content_hash BLOB NOT NULL,
                flavors TEXT NOT NULL,
                kind INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                byte_size INTEGER NOT NULL,
                source_app TEXT,
                preview TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0,
                favorite INTEGER NOT NULL DEFAULT 0
            );
            PRAGMA user_version = 1;
            "#,
        )
        .unwrap();
    connection
        .execute(
            r#"
            INSERT INTO clips
                (id, content_hash, flavors, kind, created_at, updated_at,
                 byte_size, source_app, preview, pinned, favorite)
            VALUES (?1, ?2, ?3, 0, ?4, ?4, ?5, ?6, ?7, 0, 0)
            "#,
            rusqlite::params![
                clip.id.to_string_repr(),
                clip.content_hash.as_slice(),
                serde_json::to_string(&clip.flavors).unwrap(),
                clip.meta.created_at.timestamp_millis(),
                clip.meta.byte_size as i64,
                clip.meta.source_app,
                clip.preview(512),
            ],
        )
        .unwrap();
    drop(connection);

    let store = Store::open(&db).unwrap();
    assert_eq!(store.count().unwrap(), 1);
    assert_eq!(
        store.list(1).unwrap()[0].primary_text(),
        Some("v1 row survives")
    );

    let backup = db.with_extension("migration-v1.bak");
    let manifest = db.with_extension("migration.json");
    assert!(!backup.exists());
    assert!(!manifest.exists());
    assert!(!db.with_extension("migration-dry-run.db").exists());
}

#[test]
fn current_schema_open_removes_interrupted_plaintext_migration_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    store.insert(&make_clip("current schema")).unwrap();
    drop(store);

    let backup = db.with_extension("migration-v6.bak");
    let backup_wal = std::path::PathBuf::from(format!("{}-wal", backup.to_string_lossy()));
    let manifest = db.with_extension("migration.json");
    let dry_run = db.with_extension("migration-dry-run.db");
    for artifact in [&backup, &backup_wal, &manifest, &dry_run] {
        std::fs::write(artifact, b"stale plaintext migration artifact").unwrap();
    }

    let reopened = Store::open(&db).unwrap();
    assert_eq!(reopened.count().unwrap(), 1);
    for artifact in [&backup, &backup_wal, &manifest, &dry_run] {
        assert!(!artifact.exists(), "stale artifact survived: {artifact:?}");
    }
}

#[test]
fn failed_current_schema_open_preserves_interrupted_migration_backup() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    store.insert(&make_clip("before interrupted open")).unwrap();
    drop(store);

    let backup = db.with_extension("migration-v6.bak");
    std::fs::copy(&db, &backup).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&db)
        .unwrap()
        .set_len(100)
        .unwrap();

    assert!(Store::open(&db).is_err());
    assert!(backup.exists());
    assert!(backup.metadata().unwrap().len() > 100);
}

fn pending_wal_scrub(db: &std::path::Path) -> i64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT value FROM maintenance_state WHERE key = 'pending_wal_scrub'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn wal_scrub_is_deferred_and_survives_a_busy_reader() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let clip = make_clip("busy wal scrub");
    store.insert(&clip).unwrap();
    assert_eq!(pending_wal_scrub(&db), 0);

    // A second connection pins the WAL with an open read transaction.
    let reader = rusqlite::Connection::open(&db).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))
        .unwrap();

    // The delete commits without attempting an inline checkpoint...
    store.delete(clip.id).unwrap();
    assert_eq!(pending_wal_scrub(&db), 1);
    // ...and the deferred scrub reports busy instead of failing, keeping the
    // dirty marker for a later retry.
    assert!(!store.scrub_wal_if_dirty().unwrap());
    assert_eq!(pending_wal_scrub(&db), 1);

    // Once the reader is gone, the scrub truncates the WAL and resets.
    drop(reader);
    assert!(store.scrub_wal_if_dirty().unwrap());
    assert_eq!(pending_wal_scrub(&db), 0);
    assert!(!store.scrub_wal_if_dirty().unwrap());
}

#[test]
fn grace_delete_commits_while_a_reader_pins_the_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Store::open(&db).unwrap();
    let clip = make_clip("grace under busy wal");
    store.insert(&clip).unwrap();
    let key = [7_u8; 32];

    let reader = rusqlite::Connection::open(&db).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))
        .unwrap();

    // Previously the inline truncate checkpoint could fail this committed
    // delete with a spurious busy error.
    let recovery_id = store
        .delete_with_grace(
            clip.id,
            &key,
            std::time::Duration::from_secs(60),
            DeletionReason::User,
        )
        .unwrap();
    assert!(!store.scrub_wal_if_dirty().unwrap());

    drop(reader);
    assert!(store.scrub_wal_if_dirty().unwrap());
    let entries = store.grace_bin(10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].recovery_id, recovery_id);
}

#[test]
fn guard_mode_clip_survives_insert_and_load_with_flags() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let mut clip = make_clip("guarded source privacy");
    // The capture gate's guard decision: unknown provenance with armed rules
    // is captured masked, local-only, AI-disabled, and TTL-bounded.
    clip.meta.sensitive = true;
    clip.meta.sensitivity_reason = Some(vbuff_types::SensitivityReason::OperatingSystemHint);
    clip.meta.sync_eligible = false;
    clip.meta.ai_allowed = false;
    clip.meta.expires_at = Some(chrono::Utc::now() + chrono::Duration::minutes(10));
    clip.meta.provenance_confidence = vbuff_types::ProvenanceConfidence::Unknown;

    store_roundtrip_guard_flags(&db, &clip);
}

fn store_roundtrip_guard_flags(db: &std::path::Path, clip: &Clip) {
    {
        let store = Store::open(db).unwrap();
        store.insert(clip).unwrap();
    }
    let store = Store::open(db).unwrap();
    let stored = store.list(1).unwrap().pop().unwrap();
    assert!(stored.meta.sensitive);
    assert_eq!(
        stored.meta.sensitivity_reason,
        Some(vbuff_types::SensitivityReason::OperatingSystemHint)
    );
    assert!(!stored.meta.sync_eligible);
    assert!(!stored.meta.ai_allowed);
    assert!(stored.meta.expires_at.is_some());
    assert_eq!(
        stored.meta.provenance_confidence,
        vbuff_types::ProvenanceConfidence::Unknown
    );

    // A proven-confidence re-copy of the same content keeps the guard flags.
    let mut proven = make_clip("guarded source privacy");
    proven.meta.provenance_confidence = vbuff_types::ProvenanceConfidence::Proven;
    store.insert(&proven).unwrap();
    let stored = store.list(1).unwrap().pop().unwrap();
    assert!(stored.meta.sensitive);
    assert!(!stored.meta.sync_eligible);
    assert_eq!(
        stored.meta.provenance_confidence,
        vbuff_types::ProvenanceConfidence::Proven
    );
}
