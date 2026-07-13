//! Hybrid logical clocks with deterministic device tie-breaking.

use serde::{Deserialize, Serialize};

/// Remote wall time may lead local time only by this bounded tolerance.
pub const MAX_REMOTE_FUTURE_MS: u64 = 5 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HybridLogicalClock {
    pub physical_ms: u64,
    pub logical: u32,
    pub node_id: String,
}

impl HybridLogicalClock {
    pub fn new(node_id: impl Into<String>, now_ms: u64) -> Self {
        Self {
            physical_ms: now_ms,
            logical: 0,
            node_id: node_id.into(),
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> Self {
        if now_ms > self.physical_ms {
            self.physical_ms = now_ms;
            self.logical = 0;
        } else {
            self.advance_logical();
        }
        self.clone()
    }

    pub fn observe(&mut self, remote: &Self, now_ms: u64) -> Self {
        let remote = remote.bounded_at(now_ms);
        let local_physical = self.physical_ms;
        let max_physical = now_ms.max(local_physical).max(remote.physical_ms);
        let base_logical = if max_physical == local_physical && max_physical == remote.physical_ms {
            Some(self.logical.max(remote.logical))
        } else if max_physical == local_physical {
            Some(self.logical)
        } else if max_physical == remote.physical_ms {
            Some(remote.logical)
        } else {
            None
        };
        self.physical_ms = max_physical;
        if let Some(logical) = base_logical {
            self.logical = logical;
            self.advance_logical();
        } else {
            self.logical = 0;
        }
        self.clone()
    }

    /// Clamp an untrusted clock received from another device.
    pub fn bounded_at(&self, now_ms: u64) -> Self {
        let mut bounded = self.clone();
        bounded.physical_ms = bounded
            .physical_ms
            .min(now_ms.saturating_add(MAX_REMOTE_FUTURE_MS));
        bounded
    }

    fn advance_logical(&mut self) {
        if let Some(next) = self.logical.checked_add(1) {
            self.logical = next;
        } else if let Some(next_physical) = self.physical_ms.checked_add(1) {
            self.physical_ms = next_physical;
            self.logical = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_stays_monotonic_across_skew_and_merge() {
        let mut local = HybridLogicalClock::new("a", 1_000);
        let first = local.tick(900);
        assert_eq!((first.physical_ms, first.logical), (1_000, 1));
        let remote = HybridLogicalClock {
            physical_ms: 5_000,
            logical: 7,
            node_id: "b".into(),
        };
        let merged = local.observe(&remote, 1_100);
        assert_eq!((merged.physical_ms, merged.logical), (5_000, 8));
        assert!(merged > first);

        let hostile = HybridLogicalClock {
            physical_ms: u64::MAX,
            logical: u32::MAX,
            node_id: "hostile".into(),
        };
        let bounded = local.observe(&hostile, 2_000);
        assert_eq!(bounded.physical_ms, 2_000 + MAX_REMOTE_FUTURE_MS + 1);
        assert_eq!(bounded.logical, 0);

        let after_clock_rollback = local.tick(0);
        assert!(after_clock_rollback >= bounded);
        assert_eq!(after_clock_rollback.physical_ms, bounded.physical_ms);
    }

    #[test]
    fn logical_counter_rolls_into_physical_time() {
        let mut clock = HybridLogicalClock {
            physical_ms: 10,
            logical: u32::MAX,
            node_id: "a".into(),
        };

        assert_eq!((clock.tick(9).physical_ms, clock.logical), (11, 0));
    }
}
