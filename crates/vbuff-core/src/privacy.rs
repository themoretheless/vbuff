//! Content-free, tamper-evident capture-decision ledger.

use std::collections::VecDeque;

use crate::capture::{CaptureOutcome, DropClass};

const DOMAIN: &[u8] = b"vbuff-privacy-ledger-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivacyDecisionKind {
    Captured,
    Skipped,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivacyLedgerEntry {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub count: u64,
    pub decision: PrivacyDecisionKind,
    pub reason: &'static str,
    pub previous_hash: [u8; 32],
    pub hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct PrivacyLedger {
    capacity: usize,
    next_sequence: u64,
    previous_hash: [u8; 32],
    entries: VecDeque<PrivacyLedgerEntry>,
}

impl PrivacyLedger {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            next_sequence: 0,
            previous_hash: [0; 32],
            entries: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn append(
        &mut self,
        timestamp_ms: u64,
        outcome: CaptureOutcome,
        count: u64,
    ) -> PrivacyLedgerEntry {
        let (decision, reason) = match outcome {
            CaptureOutcome::Captured => (PrivacyDecisionKind::Captured, "captured"),
            CaptureOutcome::Dropped(reason) if reason.class() == DropClass::Intentional => {
                (PrivacyDecisionKind::Skipped, reason.as_str())
            }
            CaptureOutcome::Dropped(reason) => (PrivacyDecisionKind::Lost, reason.as_str()),
        };
        let entry = PrivacyLedgerEntry {
            sequence: self.next_sequence,
            timestamp_ms,
            count,
            decision,
            reason,
            previous_hash: self.previous_hash,
            hash: ledger_hash(
                self.next_sequence,
                timestamp_ms,
                count,
                decision,
                reason,
                &self.previous_hash,
            ),
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.previous_hash = entry.hash;
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        entry
    }

    pub fn entries(&self) -> impl Iterator<Item = &PrivacyLedgerEntry> {
        self.entries.iter()
    }

    pub fn head_hash(&self) -> [u8; 32] {
        self.previous_hash
    }

    pub fn verify(entries: &[PrivacyLedgerEntry]) -> bool {
        let Some(first) = entries.first() else {
            return true;
        };
        let mut sequence = first.sequence;
        let mut previous_hash = first.previous_hash;
        entries.iter().all(|entry| {
            let valid = entry.sequence == sequence
                && entry.previous_hash == previous_hash
                && entry.hash
                    == ledger_hash(
                        entry.sequence,
                        entry.timestamp_ms,
                        entry.count,
                        entry.decision,
                        entry.reason,
                        &entry.previous_hash,
                    );
            sequence = sequence.saturating_add(1);
            previous_hash = entry.hash;
            valid
        })
    }
}

impl Default for PrivacyLedger {
    fn default() -> Self {
        Self::new(128)
    }
}

fn ledger_hash(
    sequence: u64,
    timestamp_ms: u64,
    count: u64,
    decision: PrivacyDecisionKind,
    reason: &str,
    previous_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN);
    hasher.update(previous_hash);
    hasher.update(&sequence.to_be_bytes());
    hasher.update(&timestamp_ms.to_be_bytes());
    hasher.update(&count.to_be_bytes());
    hasher.update(&[match decision {
        PrivacyDecisionKind::Captured => 1,
        PrivacyDecisionKind::Skipped => 2,
        PrivacyDecisionKind::Lost => 3,
    }]);
    hasher.update(reason.as_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use crate::capture::DropReason;

    use super::*;

    #[test]
    fn bounded_chain_preserves_skip_reason_and_detects_tampering() {
        let mut ledger = PrivacyLedger::new(2);
        ledger.append(10, CaptureOutcome::Captured, 1);
        ledger.append(11, CaptureOutcome::Dropped(DropReason::Concealed), 2);
        ledger.append(12, CaptureOutcome::Dropped(DropReason::StoreFailure), 1);
        let mut entries = ledger.entries().copied().collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].reason, "concealed");
        assert_eq!(entries[0].count, 2);
        assert!(PrivacyLedger::verify(&entries));
        entries[1].timestamp_ms = 99;
        assert!(!PrivacyLedger::verify(&entries));
    }
}
