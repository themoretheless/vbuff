use std::fmt;

use serde::{Deserialize, Serialize};

use super::{MAX_DEVICE_ID_BYTES, all_zero, valid_identifier};
use crate::clock::HybridLogicalClock;
use crate::conflict::{ConflictCandidate, ConflictReason, resolve};
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictTimelineOutcome {
    Winner,
    Alternative,
    Identical,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictTimelinePoint {
    pub content_hash: [u8; 32],
    pub clock: HybridLogicalClock,
    pub outcome: ConflictTimelineOutcome,
    pub reason: ConflictReason,
}

impl fmt::Debug for ConflictTimelinePoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConflictTimelinePoint")
            .field("content_hash", &"[redacted]")
            .field("clock", &"[redacted]")
            .field("outcome", &self.outcome)
            .field("reason", &self.reason)
            .finish()
    }
}

pub fn conflict_timeline(
    left: ConflictCandidate<[u8; 32]>,
    right: ConflictCandidate<[u8; 32]>,
) -> Result<Vec<ConflictTimelinePoint>> {
    if !valid_identifier(&left.clock.node_id, MAX_DEVICE_ID_BYTES)
        || !valid_identifier(&right.clock.node_id, MAX_DEVICE_ID_BYTES)
        || all_zero(&left.value)
        || all_zero(&right.value)
    {
        return Err(SyncError::Invalid("invalid conflict candidates".into()));
    }
    let resolution = resolve(&left, &right);
    if resolution.reason == ConflictReason::Identical {
        return Ok(vec![ConflictTimelinePoint {
            content_hash: resolution.winner.value,
            clock: resolution.winner.clock,
            outcome: ConflictTimelineOutcome::Identical,
            reason: resolution.reason,
        }]);
    }
    let alternative = resolution
        .alternative
        .expect("non-identical conflicts keep the alternative");
    Ok(vec![
        ConflictTimelinePoint {
            content_hash: alternative.value,
            clock: alternative.clock,
            outcome: ConflictTimelineOutcome::Alternative,
            reason: resolution.reason,
        },
        ConflictTimelinePoint {
            content_hash: resolution.winner.value,
            clock: resolution.winner.clock,
            outcome: ConflictTimelineOutcome::Winner,
            reason: resolution.reason,
        },
    ])
}
