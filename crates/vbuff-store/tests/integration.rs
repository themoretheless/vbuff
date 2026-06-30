//! Integration tests for `vbuff-store` against a real on-disk SQLite database.

use vbuff_core::content_hash_from_flavors;
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

fn make_clip(text: &str) -> Clip {
    let flavors = vec![Flavor::inline("text/plain;charset=utf-8", text.as_bytes().to_vec())];
    let content_hash = content_hash_from_flavors(&flavors);
    Clip {
        id: ClipId::new(),
        flavors,
        content_hash,
        meta: ClipMeta::now(ContentKind::Text, text.len() as u64, Some("integration.test".into())),
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
        store.insert(&make_clip(&format!("clip number {i}"))).unwrap();
    }
    assert_eq!(store.count().unwrap(), 10);

    // Re-insert a duplicate; count stays the same (dedup), clip floats to top.
    store.insert(&make_clip("clip number 3")).unwrap();
    assert_eq!(store.count().unwrap(), 10);
    assert_eq!(store.list(1).unwrap()[0].primary_text(), Some("clip number 3"));

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
