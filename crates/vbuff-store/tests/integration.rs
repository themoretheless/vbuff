//! Integration tests for `vbuff-store` against a real on-disk SQLite database.

use vbuff_core::content_hash_from_flavors;
use vbuff_store::Store;
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

    assert_eq!(store.purge_expired().unwrap(), 1);
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
        }
    }
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
fn on_disk_migration_preflights_and_writes_verified_manifest() {
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
    assert!(backup.exists());
    assert!(manifest.exists());
    assert!(!db.with_extension("migration-dry-run.db").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            backup.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            manifest.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(manifest["pre_version"], 1);
    assert_eq!(manifest["target_version"], vbuff_store::SCHEMA_VERSION);
    assert_eq!(manifest["pre_rows"], 1);
    assert_eq!(manifest["post_rows"], 1);
    assert_eq!(manifest["backup_blake3"].as_str().unwrap().len(), 64);
}
