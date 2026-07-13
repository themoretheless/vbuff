//! Hot/cold history placement policy independent of storage format.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryTier {
    Hot,
    Cold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryTierInput {
    pub age: Duration,
    pub hot_rank: usize,
    pub pinned: bool,
    pub favorite: bool,
    pub sensitive: bool,
}

#[derive(Clone, Debug)]
pub struct HistoryTierPolicy {
    pub hot_limit: usize,
    pub hot_age: Duration,
}

impl HistoryTierPolicy {
    pub fn classify(&self, input: HistoryTierInput) -> HistoryTier {
        if input.pinned || input.favorite || input.sensitive {
            return HistoryTier::Hot;
        }
        if input.hot_rank < self.hot_limit && input.age <= self.hot_age {
            HistoryTier::Hot
        } else {
            HistoryTier::Cold
        }
    }
}

impl Default for HistoryTierPolicy {
    fn default() -> Self {
        Self {
            hot_limit: 1_000,
            hot_age: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_items_move_cold_but_pinned_items_stay_hot() {
        let policy = HistoryTierPolicy::default();
        let old = HistoryTierInput {
            age: Duration::from_secs(30 * 24 * 60 * 60),
            hot_rank: 2_000,
            pinned: false,
            favorite: false,
            sensitive: false,
        };
        assert_eq!(policy.classify(old), HistoryTier::Cold);
        assert_eq!(
            policy.classify(HistoryTierInput {
                pinned: true,
                ..old
            }),
            HistoryTier::Hot
        );
    }
}
