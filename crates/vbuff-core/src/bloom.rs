//! Small in-memory Bloom filter for negative dedup checks.

#[derive(Clone, Debug)]
pub struct BloomFilter {
    bits: Vec<u64>,
    hash_functions: u8,
}

impl BloomFilter {
    pub fn with_capacity(expected_items: usize, bits_per_item: usize) -> Self {
        let bit_count = expected_items
            .max(1)
            .saturating_mul(bits_per_item.max(4))
            .next_power_of_two();
        Self {
            bits: vec![0; bit_count.div_ceil(64)],
            hash_functions: 4,
        }
    }

    pub fn insert(&mut self, value: &[u8]) {
        for bit in self.positions(value) {
            self.bits[bit / 64] |= 1_u64 << (bit % 64);
        }
    }

    /// False is definitive; true still requires an exact DB lookup.
    pub fn might_contain(&self, value: &[u8]) -> bool {
        self.positions(value)
            .into_iter()
            .all(|bit| self.bits[bit / 64] & (1_u64 << (bit % 64)) != 0)
    }

    fn positions(&self, value: &[u8]) -> Vec<usize> {
        let digest = blake3::hash(value);
        let bytes = digest.as_bytes();
        let h1 = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let h2 = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) | 1;
        let bit_count = self.bits.len() * 64;
        (0..self.hash_functions)
            .map(|index| h1.wrapping_add(u64::from(index).wrapping_mul(h2)) as usize % bit_count)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_values_are_never_false_negatives() {
        let mut filter = BloomFilter::with_capacity(100, 10);
        for value in [b"alpha".as_slice(), b"beta", b"gamma"] {
            filter.insert(value);
        }
        assert!(filter.might_contain(b"alpha"));
        assert!(filter.might_contain(b"beta"));
        assert!(!filter.might_contain(b"definitely-not-present"));
    }
}
