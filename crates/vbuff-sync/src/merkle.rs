//! Merkle range reconciliation for offline-device catch-up.

use serde::{Deserialize, Serialize};

use crate::chain::Preimage;
use crate::clock::HybridLogicalClock;

/// Domain of a leaf commitment. See [`leaf_hash`] for why it is `v3`.
const LEAF_DOMAIN: &[u8] = b"vbuff-merkle-leaf-v3";

/// Domain of an interior node commitment.
const NODE_DOMAIN: &[u8] = b"vbuff-merkle-node-v2";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleRecord {
    pub clock: HybridLogicalClock,
    pub record_id: String,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct MerkleTree {
    records: Vec<MerkleRecord>,
    leaf_hashes: Vec<[u8; 32]>,
}

impl MerkleTree {
    pub fn new(mut records: Vec<MerkleRecord>) -> Self {
        records.sort_by(|left, right| {
            left.clock
                .cmp(&right.clock)
                .then_with(|| left.record_id.cmp(&right.record_id))
        });
        let leaf_hashes = records.iter().map(leaf_hash).collect();
        Self {
            records,
            leaf_hashes,
        }
    }

    pub fn root(&self) -> [u8; 32] {
        let size = self.leaf_hashes.len().max(1).next_power_of_two();
        range_hash(&self.leaf_hashes, 0, size)
    }

    pub fn differing_indices(&self, other: &Self) -> Vec<usize> {
        let size = self
            .leaf_hashes
            .len()
            .max(other.leaf_hashes.len())
            .max(1)
            .next_power_of_two();
        let mut differences = Vec::new();
        diff_range(
            &self.leaf_hashes,
            &other.leaf_hashes,
            0,
            size,
            &mut differences,
        );
        differences
    }

    pub fn record(&self, index: usize) -> Option<&MerkleRecord> {
        self.records.get(index)
    }
}

/// Leaf commitment over one record.
///
/// The two variable-length fields carry an explicit length prefix. Without
/// it the concatenation was ambiguous: `record_id` and `node_id` sandwich
/// the fixed-width clock, so bytes could be shifted from one field into the
/// other and produce an identical preimage. Peer-supplied ids are not
/// validated here, so that ambiguity was reachable with ordinary values, and
/// two peers holding *different* records would compute the same root, making
/// the difference invisible to reconciliation forever.
///
/// The domain is `v3` because the framing changed twice: `v1` had no length
/// prefixes at all, and `v2` had hand-rolled ones. `v1` and `v2` trees must
/// not be compared against these.
///
/// A record whose ids exceed the builder's length prefix hashes to zero
/// rather than to an ambiguous preimage. That is a deliberate dead end: such
/// a leaf can never match anything, so reconciliation reports a permanent
/// difference instead of silently agreeing, which is the failure this domain
/// exists to prevent.
fn leaf_hash(record: &MerkleRecord) -> [u8; 32] {
    let mut preimage = Preimage::new(LEAF_DOMAIN);
    preimage
        .fixed(&record.digest)
        .var(record.record_id.as_bytes())
        .fixed(&record.clock.physical_ms.to_le_bytes())
        .fixed(&record.clock.logical.to_le_bytes())
        .var(record.clock.node_id.as_bytes());
    preimage
        .finish()
        .map_or([0; 32], |bytes| *blake3::hash(&bytes).as_bytes())
}

fn range_hash(leaves: &[[u8; 32]], start: usize, size: usize) -> [u8; 32] {
    if size == 1 {
        return leaves.get(start).copied().unwrap_or([0; 32]);
    }
    let half = size / 2;
    let left = range_hash(leaves, start, half);
    let right = range_hash(leaves, start + half, half);
    // Two fixed-width children, so the concatenation is unambiguous on its
    // own; the builder is used for the domain terminator, which the previous
    // constant lacked.
    let mut preimage = Preimage::new(NODE_DOMAIN);
    preimage.fixed(&left).fixed(&right);
    preimage
        .finish()
        .map_or([0; 32], |bytes| *blake3::hash(&bytes).as_bytes())
}

fn diff_range(
    left: &[[u8; 32]],
    right: &[[u8; 32]],
    start: usize,
    size: usize,
    differences: &mut Vec<usize>,
) {
    if range_hash(left, start, size) == range_hash(right, start, size) {
        return;
    }
    if size == 1 {
        differences.push(start);
        return;
    }
    let half = size / 2;
    diff_range(left, right, start, half, differences);
    diff_range(left, right, start + half, half, differences);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, digest: u8) -> MerkleRecord {
        MerkleRecord {
            clock: HybridLogicalClock::new("a", u64::from(digest)),
            record_id: id.into(),
            digest: [digest; 32],
        }
    }

    /// Pins the framing so a future edit cannot quietly change what a leaf
    /// commits to. Both peers must derive the same bytes or reconciliation
    /// compares roots that were never comparable.
    #[test]
    fn leaf_preimage_framing_is_pinned() {
        let mut expected = Vec::new();
        expected.extend_from_slice(b"vbuff-merkle-leaf-v3");
        expected.push(0);
        expected.extend_from_slice(&[7; 32]);
        expected.extend_from_slice(&1_u32.to_be_bytes());
        expected.extend_from_slice(b"r");
        expected.extend_from_slice(&5_u64.to_le_bytes());
        expected.extend_from_slice(&0_u32.to_le_bytes());
        expected.extend_from_slice(&4_u32.to_be_bytes());
        expected.extend_from_slice(b"node");

        let mut preimage = Preimage::new(LEAF_DOMAIN);
        preimage
            .fixed(&[7; 32])
            .var(b"r")
            .fixed(&5_u64.to_le_bytes())
            .fixed(&0_u32.to_le_bytes())
            .var(b"node");
        assert_eq!(preimage.finish().unwrap(), expected);

        let record = MerkleRecord {
            clock: HybridLogicalClock::new("node", 5),
            record_id: "r".into(),
            digest: [7; 32],
        };
        assert_eq!(leaf_hash(&record), *blake3::hash(&expected).as_bytes());
    }

    #[test]
    fn shifting_bytes_between_id_fields_cannot_forge_the_same_leaf() {
        // Against the unframed v1 preimage these two records hashed
        // identically: a trailing NUL moved out of `record_id`, the physical
        // clock shifted one byte up, and a leading NUL appeared in
        // `node_id`, leaving the concatenation byte-for-byte unchanged. Two
        // peers holding these different records agreed on the root, so
        // reconciliation reported no difference and the record never synced.
        let left = MerkleRecord {
            clock: HybridLogicalClock::new("node", 1),
            record_id: "r\0".into(),
            digest: [7; 32],
        };
        let right = MerkleRecord {
            clock: HybridLogicalClock::new("\0node", 256),
            record_id: "r".into(),
            digest: [7; 32],
        };
        assert_ne!(leaf_hash(&left), leaf_hash(&right));
        assert_ne!(
            MerkleTree::new(vec![left]).root(),
            MerkleTree::new(vec![right]).root()
        );
    }

    #[test]
    fn localizes_changed_and_missing_leaves() {
        let left = MerkleTree::new(vec![record("a", 1), record("b", 2), record("c", 3)]);
        let right = MerkleTree::new(vec![record("a", 1), record("b", 9)]);
        assert_ne!(left.root(), right.root());
        assert_eq!(left.differing_indices(&right), vec![1, 2]);
        assert_eq!(left.record(2).unwrap().record_id, "c");
    }
}
