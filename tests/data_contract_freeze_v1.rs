use std::collections::BTreeSet;

use vbuff_core::content_hash_from_flavors;
use vbuff_ipc::{Capability, ClientHello, ProtocolRange};
use vbuff_platform::{FormatFamily, FormatKey, canonical_format};
use vbuff_types::Flavor;

#[test]
fn data_contract_v1_golden_vectors_are_frozen() {
    assert_eq!(vbuff_store::SCHEMA_VERSION, 5);

    let flavors = [
        Flavor::inline("text/html", b"<b>hello</b>".to_vec()),
        Flavor::inline("text/plain;charset=utf-8", b"hello".to_vec()),
    ];
    assert_eq!(
        content_hash_from_flavors(&flavors),
        [
            219, 106, 6, 166, 89, 248, 150, 170, 223, 130, 242, 217, 7, 112, 79, 42, 152, 0, 116,
            139, 95, 121, 155, 62, 229, 245, 119, 207, 236, 69, 247, 131,
        ]
    );

    assert_eq!(
        canonical_format(FormatFamily::Windows, "CF_UNICODETEXT"),
        Some(FormatKey::PlainText)
    );
    assert_eq!(
        canonical_format(FormatFamily::MacUti, "org.nspasteboard.ConcealedType"),
        Some(FormatKey::Concealed)
    );

    let hello = ClientHello {
        client_name: "contract-fixture".into(),
        protocol: ProtocolRange {
            minimum: 1,
            maximum: 1,
        },
        requested: BTreeSet::from([Capability::ReadHistory, Capability::SubscribeEvents]),
    };
    assert_eq!(
        serde_json::to_string(&hello).unwrap(),
        r#"{"client_name":"contract-fixture","protocol":{"minimum":1,"maximum":1},"requested":["read_history","subscribe_events"]}"#
    );
}
