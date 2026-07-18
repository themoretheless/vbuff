use serde::Deserialize;
use vbuff_core::format_fidelity::{FidelityLevel, compare_flavors};
use vbuff_platform::{FormatFamily, FormatKey, canonical_format};
use vbuff_types::{Body, Flavor};

#[derive(Deserialize)]
struct Corpus {
    schema: u16,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    family: String,
    native: String,
    expected: FormatKey,
    mime: String,
    payload_hex: String,
}

#[test]
fn every_backend_shares_one_versioned_format_oracle() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("corpus/format-fidelity-v1.json")).unwrap();
    assert_eq!(corpus.schema, 1);
    assert!(corpus.cases.len() >= 8);
    for case in corpus.cases {
        let family = match case.family.as_str() {
            "mac_uti" => FormatFamily::MacUti,
            "windows" => FormatFamily::Windows,
            "mime" => FormatFamily::Mime,
            _ => panic!("unknown family in {}", case.name),
        };
        assert_eq!(
            canonical_format(family, &case.native),
            Some(case.expected),
            "format mapping drifted for {}",
            case.name
        );
        let bytes = decode_hex(&case.payload_hex).expect("valid corpus hex");
        let captured = vec![Flavor::inline(&case.mime, bytes)];
        assert_eq!(
            compare_flavors(&captured, &captured.clone()).level,
            FidelityLevel::Lossless,
            "round-trip drifted for {}",
            case.name
        );
        if let Body::Inline(mut changed) = captured[0].body.clone() {
            changed.push(0xff);
            assert_eq!(
                compare_flavors(&captured, &[Flavor::inline(&case.mime, changed)]).level,
                FidelityLevel::Degraded,
                "mutation was missed for {}",
                case.name
            );
        }
    }
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !value.is_ascii() {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(pair, 16).ok()
        })
        .collect()
}
