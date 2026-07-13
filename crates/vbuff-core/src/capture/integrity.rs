use std::collections::HashMap;

use vbuff_types::{Body, Flavor, FlavorOrigin, FlavorRealization};

type FlavorIdentity = (String, [u8; 32], usize);
type CanonicalCandidate = (usize, FlavorOrigin);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrityFailure {
    pub mime: String,
    pub expected: [u8; 32],
    pub actual: [u8; 32],
}

/// Marker retained when a representation can be synthesized from another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrunedFlavor {
    pub mime: String,
    pub derive_from_mime: String,
}

/// Stamp every realized inline flavor at the read boundary.
pub fn annotate_integrity(flavors: &mut [Flavor]) {
    for flavor in flavors {
        flavor.integrity_hash = match (&flavor.body, flavor.realization) {
            (Body::Inline(bytes), FlavorRealization::Realized) => {
                Some(*blake3::hash(bytes).as_bytes())
            }
            _ => None,
        };
    }
}

/// Recompute all stamped digests before persistence.
pub fn verify_integrity(flavors: &[Flavor]) -> Vec<IntegrityFailure> {
    flavors
        .iter()
        .filter_map(|flavor| {
            let expected = flavor.integrity_hash?;
            let bytes = flavor.body.inline_bytes()?;
            let actual = *blake3::hash(bytes).as_bytes();
            (actual != expected).then(|| IntegrityFailure {
                mime: flavor.mime.clone(),
                expected,
                actual,
            })
        })
        .collect()
}

/// Remove only byte-identical duplicates. Source representations are favored
/// over OS/vbuff synthesized forms, preserving the byte-for-byte guarantee.
pub fn prune_redundant_flavors(flavors: &mut Vec<Flavor>) -> Vec<PrunedFlavor> {
    let mut canonical: HashMap<FlavorIdentity, Vec<CanonicalCandidate>> = HashMap::new();
    let mut keep = vec![true; flavors.len()];
    let mut pruned = Vec::new();

    for (index, flavor) in flavors.iter().enumerate() {
        let Some(bytes) = flavor.body.inline_bytes() else {
            continue;
        };
        let key = (
            canonical_mime(&flavor.mime),
            *blake3::hash(bytes).as_bytes(),
            bytes.len(),
        );
        let candidates = canonical.entry(key).or_default();
        if let Some(position) = candidates.iter().position(|(prior, _)| {
            flavors[*prior]
                .body
                .inline_bytes()
                .is_some_and(|prior| prior == bytes)
        }) {
            let (prior, prior_origin) = candidates[position];
            if prior_origin != FlavorOrigin::Source && flavor.origin == FlavorOrigin::Source {
                keep[prior] = false;
                pruned.push(PrunedFlavor {
                    mime: flavors[prior].mime.clone(),
                    derive_from_mime: flavor.mime.clone(),
                });
                candidates[position] = (index, flavor.origin);
            } else {
                keep[index] = false;
                pruned.push(PrunedFlavor {
                    mime: flavor.mime.clone(),
                    derive_from_mime: flavors[prior].mime.clone(),
                });
            }
        } else {
            candidates.push((index, flavor.origin));
        }
    }

    let mut index = 0;
    flavors.retain(|_| {
        let retain = keep[index];
        index += 1;
        retain
    });
    pruned
}

fn canonical_mime(mime: &str) -> String {
    mime.split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_detects_mutation_after_capture() {
        let mut flavors = vec![Flavor::inline("text/plain", b"safe".to_vec())];
        annotate_integrity(&mut flavors);
        assert!(verify_integrity(&flavors).is_empty());

        flavors[0].body = Body::Inline(b"changed".to_vec());
        assert_eq!(verify_integrity(&flavors).len(), 1);
    }

    #[test]
    fn pruning_prefers_source_and_keeps_distinct_bytes() {
        let mut flavors = vec![
            Flavor::derived("text/plain;charset=utf-8", b"same".to_vec()),
            Flavor::inline("TEXT/PLAIN", b"same".to_vec()),
            Flavor::inline("text/html", b"<b>same</b>".to_vec()),
        ];

        let pruned = prune_redundant_flavors(&mut flavors);
        assert_eq!(flavors.len(), 2);
        assert_eq!(flavors[0].origin, FlavorOrigin::Source);
        assert_eq!(pruned.len(), 1);
    }
}
