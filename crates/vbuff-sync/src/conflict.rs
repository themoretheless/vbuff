//! Deterministic, user-inspectable conflict resolution plans.

use serde::{Deserialize, Serialize};

use crate::clock::HybridLogicalClock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictCandidate<T> {
    pub value: T,
    pub clock: HybridLogicalClock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    NewerPhysicalTime,
    HigherLogicalCounter,
    DeterministicDeviceTieBreak,
    Identical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictResolution<T> {
    pub winner: ConflictCandidate<T>,
    pub alternative: Option<ConflictCandidate<T>>,
    pub reason: ConflictReason,
    pub keep_both_available: bool,
}

pub fn resolve<T: Clone + PartialEq>(
    left: &ConflictCandidate<T>,
    right: &ConflictCandidate<T>,
) -> ConflictResolution<T> {
    if left.value == right.value {
        return ConflictResolution {
            winner: if left.clock >= right.clock {
                left.clone()
            } else {
                right.clone()
            },
            alternative: None,
            reason: ConflictReason::Identical,
            keep_both_available: false,
        };
    }
    let (winner, alternative) = if left.clock >= right.clock {
        (left.clone(), right.clone())
    } else {
        (right.clone(), left.clone())
    };
    let reason = if left.clock.physical_ms != right.clock.physical_ms {
        ConflictReason::NewerPhysicalTime
    } else if left.clock.logical != right.clock.logical {
        ConflictReason::HigherLogicalCounter
    } else {
        ConflictReason::DeterministicDeviceTieBreak
    };
    ConflictResolution {
        winner,
        alternative: Some(alternative),
        reason,
        keep_both_available: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_exposes_winner_loser_and_tie_break_reason() {
        let left = ConflictCandidate {
            value: "left",
            clock: HybridLogicalClock::new("a", 10),
        };
        let right = ConflictCandidate {
            value: "right",
            clock: HybridLogicalClock::new("b", 10),
        };
        let resolution = resolve(&left, &right);
        assert_eq!(resolution.winner.value, "right");
        assert_eq!(
            resolution.reason,
            ConflictReason::DeterministicDeviceTieBreak
        );
        assert!(resolution.keep_both_available);
        assert_eq!(resolution.alternative.unwrap().value, "left");
    }
}
