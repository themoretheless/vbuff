use vbuff_store::{RetentionScope, Store, normalized_text_fingerprint};
use vbuff_types::ContentKind;

#[test]
fn data_contract_v2_lifecycle_vectors_are_frozen() {
    assert_eq!(vbuff_store::DATA_CONTRACT_V2_SCHEMA_VERSION, 6);
    const {
        assert!(vbuff_store::SCHEMA_VERSION >= vbuff_store::DATA_CONTRACT_V2_SCHEMA_VERSION);
    }
    assert_eq!(
        normalized_text_fingerprint("Hello,\nworld -- next").unwrap(),
        [
            168, 27, 50, 31, 186, 80, 208, 131, 26, 49, 101, 226, 38, 100, 219, 5, 117, 159, 65,
            246, 240, 185, 191, 52, 113, 201, 12, 135, 0, 28, 227, 29,
        ]
    );

    let store = Store::open_in_memory().unwrap();
    let rules = store.retention_rules().unwrap();
    assert_eq!(rules.len(), 10);
    assert!(rules.iter().any(|rule| {
        rule.scope == RetentionScope::Kind(ContentKind::Image) && rule.max_items == Some(500)
    }));
    assert!(
        rules
            .iter()
            .any(|rule| { rule.scope == RetentionScope::Sensitive && rule.grace_window.is_zero() })
    );
}
