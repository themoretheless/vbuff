use vbuff_core::content_hash_from_flavors;
use vbuff_store::{
    ArchiveVisibility, CollectionRecord, CollectionRetentionPolicy, ExportSchemaVersion, Store,
};
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

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
fn data_contract_v3_organization_and_portability_are_frozen() {
    assert_eq!(vbuff_store::SCHEMA_VERSION, 7);
    assert_eq!(u16::from(ExportSchemaVersion::LATEST), 2);

    let store = Store::open_in_memory().unwrap();
    let clip = clip("schema seven");
    store.insert(&clip).unwrap();
    store
        .upsert_collection(&CollectionRecord {
            id: "contract".into(),
            name: "Contract".into(),
            retention: CollectionRetentionPolicy {
                max_age_days: Some(30),
                max_items: Some(100),
                max_bytes: Some(1_048_576),
            },
        })
        .unwrap();
    store.set_collection(clip.id, Some("contract")).unwrap();
    store.set_archived(clip.id, true).unwrap();
    assert!(store.list(10).unwrap().is_empty());
    assert_eq!(
        store
            .list_with_archive(ArchiveVisibility::Archived, 10)
            .unwrap()
            .len(),
        1
    );
    let manifest = store.attachment_manifest(clip.id).unwrap();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.flavors[0].mime, "text/plain");
}
