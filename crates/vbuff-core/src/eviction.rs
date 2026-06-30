//! Retention / eviction policy.
//!
//! The MVP policy is a simple count cap: keep at most N clips, deleting the
//! oldest unprotected clips first. Pinned (and favorite) clips are always
//! exempt and never count against... well, they never get evicted, but they do
//! still occupy the history. The cap applies to the *evictable* pool.

use vbuff_types::Clip;

/// Retention configuration.
#[derive(Clone, Copy, Debug)]
pub struct EvictionPolicy {
    /// Maximum number of clips to retain (across both pinned and unpinned).
    /// Pinned clips are never evicted even if this would be exceeded.
    pub max_history: usize,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        EvictionPolicy { max_history: 500 }
    }
}

/// True if a clip is protected from eviction.
fn is_protected(clip: &Clip) -> bool {
    clip.pinned || clip.favorite
}

/// Given the current set of clips, decide which ids to evict to satisfy the
/// policy.
///
/// Returns the ids of clips that should be deleted (oldest unprotected first).
/// `clips` need not be sorted; the function determines recency from the ULID.
pub fn evict(clips: &[Clip], policy: &EvictionPolicy) -> Vec<vbuff_types::ClipId> {
    if clips.len() <= policy.max_history {
        return Vec::new();
    }

    // How many we must remove overall.
    let mut overflow = clips.len() - policy.max_history;

    // Candidates: unprotected clips, oldest first (ascending ULID).
    let mut candidates: Vec<&Clip> = clips.iter().filter(|c| !is_protected(c)).collect();
    candidates.sort_by_key(|a| a.id.0);

    let mut to_evict = Vec::new();
    for clip in candidates {
        if overflow == 0 {
            break;
        }
        to_evict.push(clip.id);
        overflow -= 1;
    }
    to_evict
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

    fn clip(ts: u64, pinned: bool) -> Clip {
        Clip {
            id: ClipId(ulid::Ulid::from_parts(ts, 0)),
            flavors: vec![Flavor::inline("text/plain", b"x".to_vec())],
            content_hash: [0u8; 32],
            meta: ClipMeta::now(ContentKind::Text, 1, None),
            pinned,
            favorite: false,
        }
    }

    #[test]
    fn under_cap_evicts_nothing() {
        let clips = vec![clip(1, false), clip(2, false)];
        let policy = EvictionPolicy { max_history: 5 };
        assert!(evict(&clips, &policy).is_empty());
    }

    #[test]
    fn evicts_oldest_first() {
        let clips = vec![clip(3, false), clip(1, false), clip(2, false)];
        let policy = EvictionPolicy { max_history: 2 };
        let evicted = evict(&clips, &policy);
        assert_eq!(evicted.len(), 1);
        // Oldest is ts=1.
        assert_eq!(evicted[0], ClipId(ulid::Ulid::from_parts(1, 0)));
    }

    #[test]
    fn never_evicts_pinned() {
        // 3 clips, cap 1, but the two oldest are pinned -> only the newest
        // unprotected one is evictable, leaving the 2 pinned + maybe more.
        let clips = vec![clip(1, true), clip(2, true), clip(3, false)];
        let policy = EvictionPolicy { max_history: 1 };
        let evicted = evict(&clips, &policy);
        // Overflow is 2, but only 1 unprotected candidate exists.
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], ClipId(ulid::Ulid::from_parts(3, 0)));
    }

    #[test]
    fn all_pinned_evicts_nothing() {
        let clips = vec![clip(1, true), clip(2, true), clip(3, true)];
        let policy = EvictionPolicy { max_history: 1 };
        assert!(evict(&clips, &policy).is_empty());
    }
}
