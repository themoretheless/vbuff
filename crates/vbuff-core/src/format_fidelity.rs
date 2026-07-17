//! Byte-level round-trip fidelity reports for multi-flavor clips.

use std::collections::{BTreeMap, BTreeSet};

use vbuff_types::{Body, Flavor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidelityLevel {
    Lossless,
    Degraded,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityReport {
    pub level: FidelityLevel,
    pub required_flavors: usize,
    pub missing_mime: BTreeSet<String>,
    pub changed_mime: BTreeSet<String>,
    pub unrealized_mime: BTreeSet<String>,
    pub extra_flavors: usize,
}

pub fn compare_flavors(captured: &[Flavor], served: &[Flavor]) -> FidelityReport {
    let captured = flavor_multiset(captured);
    let served = flavor_multiset(served);
    let mut missing_mime = BTreeSet::new();
    let mut changed_mime = BTreeSet::new();
    let mut unrealized_mime = BTreeSet::new();
    let mut matched = 0_usize;

    for (mime, expected) in &captured {
        let Some(actual) = served.get(mime) else {
            missing_mime.insert(mime.clone());
            continue;
        };
        matched = matched.saturating_add(expected.len().min(actual.len()));
        if expected != actual {
            changed_mime.insert(mime.clone());
        }
    }
    for flavor in served.values().flatten() {
        if !flavor.realized {
            unrealized_mime.insert(flavor.mime.clone());
        }
    }
    let served_count = served.values().map(Vec::len).sum::<usize>();
    let captured_count = captured.values().map(Vec::len).sum::<usize>();
    let extra_flavors = served_count.saturating_sub(matched);
    let level = if !missing_mime.is_empty() {
        FidelityLevel::Missing
    } else if !changed_mime.is_empty() || !unrealized_mime.is_empty() {
        FidelityLevel::Degraded
    } else {
        FidelityLevel::Lossless
    };
    FidelityReport {
        level,
        required_flavors: captured_count,
        missing_mime,
        changed_mime,
        unrealized_mime,
        extra_flavors,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FlavorEvidence {
    mime: String,
    body: BodyEvidence,
    realized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BodyEvidence {
    Inline([u8; 32], u64),
    Spilled(String, u64),
}

fn flavor_multiset(flavors: &[Flavor]) -> BTreeMap<String, Vec<FlavorEvidence>> {
    let mut grouped = BTreeMap::<String, Vec<FlavorEvidence>>::new();
    for flavor in flavors {
        let mime = canonical_mime(&flavor.mime);
        let body = match &flavor.body {
            Body::Inline(bytes) => BodyEvidence::Inline(
                flavor
                    .integrity_hash
                    .unwrap_or_else(|| *blake3::hash(bytes).as_bytes()),
                bytes.len() as u64,
            ),
            Body::Spilled {
                blob_ref,
                byte_size,
            } => BodyEvidence::Spilled(blob_ref.clone(), *byte_size),
        };
        grouped
            .entry(mime.clone())
            .or_default()
            .push(FlavorEvidence {
                mime,
                body,
                realized: flavor.is_realized(),
            });
    }
    for values in grouped.values_mut() {
        values.sort_unstable();
    }
    grouped
}

fn canonical_mime(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use vbuff_types::FlavorRealization;

    use super::*;

    #[test]
    fn reports_lossless_extra_and_missing_without_exposing_bytes() {
        let captured = vec![
            Flavor::inline("text/plain;charset=utf-8", b"hello".to_vec()),
            Flavor::inline("text/html", b"<b>hello</b>".to_vec()),
        ];
        let mut served = captured.clone();
        served.push(Flavor::derived("text/markdown", b"**hello**".to_vec()));
        let report = compare_flavors(&captured, &served);
        assert_eq!(report.level, FidelityLevel::Lossless);
        assert_eq!(report.extra_flavors, 1);
        assert!(!format!("{report:?}").contains("hello"));

        served[1].realization = FlavorRealization::Truncated;
        assert_eq!(
            compare_flavors(&captured, &served).level,
            FidelityLevel::Degraded
        );
        served.remove(1);
        assert_eq!(
            compare_flavors(&captured, &served).level,
            FidelityLevel::Missing
        );
    }
}
