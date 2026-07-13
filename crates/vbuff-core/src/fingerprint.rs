//! Near-duplicate fingerprints and compact local vector features.

const EMBEDDING_DIMS: usize = 384;

pub fn simhash64(text: &str) -> u64 {
    let tokens = tokens(text);
    if tokens.is_empty() {
        return 0;
    }
    let shingles = if tokens.len() < 3 {
        tokens
    } else {
        tokens
            .windows(3)
            .map(|window| window.join("\u{1f}"))
            .collect()
    };
    let mut weights = [0_i32; 64];
    for shingle in shingles {
        let digest = blake3::hash(shingle.as_bytes());
        let hash = u64::from_le_bytes(digest.as_bytes()[0..8].try_into().unwrap());
        for (bit, weight) in weights.iter_mut().enumerate() {
            *weight += if hash & (1_u64 << bit) == 0 { -1 } else { 1 };
        }
    }
    weights
        .iter()
        .enumerate()
        .fold(0_u64, |hash, (bit, weight)| {
            hash | (u64::from(*weight >= 0) << bit)
        })
}

pub const fn hamming_distance(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

pub const fn fingerprint_bands(hash: u64) -> [u16; 4] {
    [
        hash as u16,
        (hash >> 16) as u16,
        (hash >> 32) as u16,
        (hash >> 48) as u16,
    ]
}

/// Compute a 9x8 difference hash directly from RGBA8 source pixels.
pub fn dhash_rgba(bytes: &[u8], width: usize, height: usize) -> Option<u64> {
    let required = width.checked_mul(height)?.checked_mul(4)?;
    if width == 0 || height == 0 || bytes.len() < required {
        return None;
    }
    let mut samples = [[0_u16; 9]; 8];
    for (row, values) in samples.iter_mut().enumerate() {
        let source_y = ((row * height) / 8).min(height - 1);
        for (column, value) in values.iter_mut().enumerate() {
            let source_x = ((column * width) / 9).min(width - 1);
            let offset = (source_y * width + source_x) * 4;
            let red = u16::from(bytes[offset]);
            let green = u16::from(bytes[offset + 1]);
            let blue = u16::from(bytes[offset + 2]);
            *value = (red * 77 + green * 150 + blue * 29) >> 8;
        }
    }
    let mut hash = 0_u64;
    for (row, values) in samples.iter().enumerate() {
        for column in 0..8 {
            if values[column] > values[column + 1] {
                hash |= 1_u64 << (row * 8 + column);
            }
        }
    }
    Some(hash)
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuantizedEmbedding {
    pub scale: f32,
    pub values: Vec<i8>,
}

impl QuantizedEmbedding {
    pub fn from_f32(values: &[f32]) -> Self {
        let max = values
            .iter()
            .map(|value| value.abs())
            .fold(0.0_f32, f32::max);
        let scale = if max == 0.0 { 1.0 } else { max / 127.0 };
        let values = values
            .iter()
            .map(|value| (value / scale).round().clamp(-127.0, 127.0) as i8)
            .collect();
        Self { scale, values }
    }

    pub fn cosine_similarity(&self, other: &Self) -> Option<f32> {
        if self.values.len() != other.values.len() || self.values.is_empty() {
            return None;
        }
        let mut dot = 0_f32;
        let mut left_norm = 0_f32;
        let mut right_norm = 0_f32;
        for (&left, &right) in self.values.iter().zip(&other.values) {
            let left = f32::from(left) * self.scale;
            let right = f32::from(right) * other.scale;
            dot += left * right;
            left_norm += left * left;
            right_norm += right * right;
        }
        (left_norm > 0.0 && right_norm > 0.0).then(|| dot / (left_norm.sqrt() * right_norm.sqrt()))
    }
}

pub trait EmbeddingProvider {
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> QuantizedEmbedding;
}

/// Zero-download local feature hashing provider. An ONNX MiniLM provider can
/// implement the same trait without changing storage or ranking contracts.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFeatureEmbedding;

impl EmbeddingProvider for LocalFeatureEmbedding {
    fn dimensions(&self) -> usize {
        EMBEDDING_DIMS
    }

    fn embed(&self, text: &str) -> QuantizedEmbedding {
        let mut vector = vec![0_f32; EMBEDDING_DIMS];
        for token in tokens(text) {
            let digest = blake3::hash(token.as_bytes());
            let index = u64::from_le_bytes(digest.as_bytes()[0..8].try_into().unwrap()) as usize
                % EMBEDDING_DIMS;
            let sign = if digest.as_bytes()[8] & 1 == 0 {
                -1.0
            } else {
                1.0
            };
            vector[index] += sign;
        }
        let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        QuantizedEmbedding::from_f32(&vector)
    }
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simhash_is_close_for_small_edits() {
        let first = simhash64("the quick brown fox jumps over the lazy dog");
        let second = simhash64("the quick brown fox jumped over the lazy dog");
        let unrelated = simhash64("sqlite replication and encrypted clipboard images");
        assert!(hamming_distance(first, second) < hamming_distance(first, unrelated));
        assert_eq!(fingerprint_bands(first).len(), 4);
    }

    #[test]
    fn dhash_distinguishes_horizontal_direction() {
        let mut increasing = Vec::new();
        let mut decreasing = Vec::new();
        for _ in 0..8 {
            for column in 0..9 {
                let value = (column * 20) as u8;
                increasing.extend_from_slice(&[value, value, value, 255]);
                let value = 255 - value;
                decreasing.extend_from_slice(&[value, value, value, 255]);
            }
        }
        assert_eq!(dhash_rgba(&increasing, 9, 8), Some(0));
        assert_eq!(dhash_rgba(&decreasing, 9, 8), Some(u64::MAX));
    }

    #[test]
    fn quantized_local_features_preserve_lexical_similarity() {
        let provider = LocalFeatureEmbedding;
        let rust = provider.embed("rust sqlite clipboard history");
        let similar = provider.embed("rust clipboard history search");
        let unrelated = provider.embed("banana recipe tropical fruit");
        assert_eq!(rust.values.len(), provider.dimensions());
        assert!(
            rust.cosine_similarity(&similar).unwrap() > rust.cosine_similarity(&unrelated).unwrap()
        );
    }
}
