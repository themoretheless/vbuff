//! Merkle range reconciliation for offline-device catch-up.

use serde::{Deserialize, Serialize};

use crate::clock::HybridLogicalClock;

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

fn leaf_hash(record: &MerkleRecord) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-merkle-leaf-v1");
    hasher.update(&record.digest);
    hasher.update(record.record_id.as_bytes());
    hasher.update(&record.clock.physical_ms.to_le_bytes());
    hasher.update(&record.clock.logical.to_le_bytes());
    hasher.update(record.clock.node_id.as_bytes());
    *hasher.finalize().as_bytes()
}

fn range_hash(leaves: &[[u8; 32]], start: usize, size: usize) -> [u8; 32] {
    if size == 1 {
        return leaves.get(start).copied().unwrap_or([0; 32]);
    }
    let half = size / 2;
    let left = range_hash(leaves, start, half);
    let right = range_hash(leaves, start + half, half);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-merkle-node-v1");
    hasher.update(&left);
    hasher.update(&right);
    *hasher.finalize().as_bytes()
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

    #[test]
    fn localizes_changed_and_missing_leaves() {
        let left = MerkleTree::new(vec![record("a", 1), record("b", 2), record("c", 3)]);
        let right = MerkleTree::new(vec![record("a", 1), record("b", 9)]);
        assert_ne!(left.root(), right.root());
        assert_eq!(left.differing_indices(&right), vec![1, 2]);
        assert_eq!(left.record(2).unwrap().record_id, "c");
    }
}
