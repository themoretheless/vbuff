use std::fmt;

use vbuff_types::ClipId;

use crate::fingerprint::QuantizedEmbedding;

#[derive(Clone)]
pub struct ThreadCandidate {
    pub id: ClipId,
    pub captured_at_ms: i64,
    pub embedding: QuantizedEmbedding,
}

impl fmt::Debug for ThreadCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ThreadCandidate")
            .field("id", &self.id)
            .field("captured_at_ms", &self.captured_at_ms)
            .field("embedding_dimensions", &self.embedding.values.len())
            .finish()
    }
}

pub fn near_duplicate(
    left: &QuantizedEmbedding,
    right: &QuantizedEmbedding,
    minimum_similarity: f32,
) -> bool {
    minimum_similarity.is_finite()
        && left
            .cosine_similarity(right)
            .is_some_and(|score| score >= minimum_similarity.clamp(-1.0, 1.0))
}

/// Greedy temporal grouping over capture order. It never hides or deletes a
/// candidate; callers retain each clip and may render a collapsible thread.
pub fn thread_candidates(
    candidates: &[ThreadCandidate],
    max_gap_ms: i64,
    minimum_similarity: f32,
) -> Vec<Vec<ClipId>> {
    let mut groups: Vec<Vec<ClipId>> = Vec::new();
    let mut previous: Option<&ThreadCandidate> = None;
    for candidate in candidates {
        let joins_previous = previous.is_some_and(|prior| {
            candidate
                .captured_at_ms
                .saturating_sub(prior.captured_at_ms)
                <= max_gap_ms
                && candidate.captured_at_ms >= prior.captured_at_ms
                && near_duplicate(&prior.embedding, &candidate.embedding, minimum_similarity)
        });
        if joins_previous {
            if let Some(group) = groups.last_mut() {
                group.push(candidate.id);
            }
        } else {
            groups.push(vec![candidate.id]);
        }
        previous = Some(candidate);
    }
    groups
}

#[cfg(test)]
mod tests {
    use crate::fingerprint::{EmbeddingBackend, LocalFeatureEmbedding};

    use super::*;

    #[test]
    fn threading_requires_both_time_and_similarity() {
        let backend = LocalFeatureEmbedding;
        let candidates = [
            ThreadCandidate {
                id: ClipId::new(),
                captured_at_ms: 0,
                embedding: backend.embed("rust clipboard search").unwrap(),
            },
            ThreadCandidate {
                id: ClipId::new(),
                captured_at_ms: 100,
                embedding: backend.embed("rust clipboard indexing").unwrap(),
            },
            ThreadCandidate {
                id: ClipId::new(),
                captured_at_ms: 10_000,
                embedding: backend.embed("rust clipboard indexing").unwrap(),
            },
        ];
        let groups = thread_candidates(&candidates, 1_000, 0.25);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
    }
}
