use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use vbuff_types::{CaptureGeneration, CaptureLineage, CaptureProvenance};

use super::{DropClass, DropReason};

#[derive(Clone, Debug)]
struct SelfWrite {
    hash: [u8; 32],
    nonce: String,
    expires_at: Instant,
}

/// Bounded hash+nonce ledger used to suppress immediate and rebroadcast echoes.
#[derive(Debug)]
pub struct SelfWriteLedger {
    ttl: Duration,
    capacity: usize,
    writes: VecDeque<SelfWrite>,
}

impl SelfWriteLedger {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            ttl,
            capacity: capacity.max(1),
            writes: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn register(&mut self, hash: [u8; 32], nonce: impl Into<String>, now: Instant) {
        self.prune(now);
        if self.writes.len() == self.capacity {
            self.writes.pop_front();
        }
        self.writes.push_back(SelfWrite {
            hash,
            nonce: nonce.into(),
            expires_at: now.checked_add(self.ttl).unwrap_or(now),
        });
    }

    pub fn matches(&mut self, hash: [u8; 32], lineage: &CaptureLineage, now: Instant) -> bool {
        self.prune(now);
        self.writes.iter().any(|write| {
            write.hash == hash
                || lineage
                    .write_nonce
                    .as_deref()
                    .is_some_and(|nonce| nonce == write.nonce)
        })
    }

    fn prune(&mut self, now: Instant) {
        while self
            .writes
            .front()
            .is_some_and(|write| write.expires_at <= now)
        {
            self.writes.pop_front();
        }
    }
}

impl Default for SelfWriteLedger {
    fn default() -> Self {
        Self::new(Duration::from_secs(2), 32)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureCounters {
    pub captured: u64,
    pub intentionally_skipped: u64,
    pub lost: u64,
    pub by_reason: BTreeMap<DropReason, u64>,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureLossLedger {
    counters: CaptureCounters,
}

impl CaptureLossLedger {
    pub fn captured(&mut self) {
        self.counters.captured = self.counters.captured.saturating_add(1);
    }

    pub fn dropped(&mut self, reason: DropReason) {
        self.dropped_n(reason, 1);
    }

    pub fn dropped_n(&mut self, reason: DropReason, count: u64) {
        let by_reason = self.counters.by_reason.entry(reason).or_default();
        *by_reason = by_reason.saturating_add(count);
        match reason.class() {
            DropClass::Intentional => {
                self.counters.intentionally_skipped =
                    self.counters.intentionally_skipped.saturating_add(count);
            }
            DropClass::Loss => self.counters.lost = self.counters.lost.saturating_add(count),
        }
    }

    pub fn snapshot(&self) -> CaptureCounters {
        self.counters.clone()
    }
}

/// Audit record intentionally contains no clipboard bytes or preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedCapture {
    pub observed_at: Instant,
    pub reason: DropReason,
    pub provenance: CaptureProvenance,
    pub generation: Option<CaptureGeneration>,
    pub content_hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct SkippedCaptureRing {
    capacity: usize,
    entries: VecDeque<SkippedCapture>,
}

impl SkippedCaptureRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn push(&mut self, entry: SkippedCapture) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &SkippedCapture> {
        self.entries.iter()
    }

    pub fn latest(&self) -> Option<&SkippedCapture> {
        self.entries.back()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationObservation {
    First,
    Consecutive,
    Gap { missed: u64 },
    EpochChanged,
    Stale,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerationTracker {
    last: Option<CaptureGeneration>,
}

impl GenerationTracker {
    pub fn observe(&mut self, generation: CaptureGeneration) -> GenerationObservation {
        let observation = match self.last {
            None => GenerationObservation::First,
            Some(last) if generation.epoch != last.epoch => GenerationObservation::EpochChanged,
            Some(last) if generation.sequence <= last.sequence => GenerationObservation::Stale,
            Some(last) if generation.sequence == last.sequence + 1 => {
                GenerationObservation::Consecutive
            }
            Some(last) => GenerationObservation::Gap {
                missed: generation.sequence - last.sequence - 1,
            },
        };
        if observation != GenerationObservation::Stale {
            self.last = Some(generation);
        }
        observation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfTestState {
    Idle,
    AwaitingEcho { hash: [u8; 32] },
    AwaitingRestore,
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfTestObservation {
    Started,
    EchoConfirmed,
    RestoreConfirmed,
    UnexpectedEcho,
    TimedOut,
}

impl SelfTestState {
    pub fn observe(self, observation: SelfTestObservation) -> Self {
        match (self, observation) {
            (Self::Idle, SelfTestObservation::Started) => Self::AwaitingEcho { hash: [0; 32] },
            (Self::AwaitingEcho { .. }, SelfTestObservation::EchoConfirmed) => {
                Self::AwaitingRestore
            }
            (Self::AwaitingRestore, SelfTestObservation::RestoreConfirmed) => Self::Passed,
            (_, SelfTestObservation::UnexpectedEcho | SelfTestObservation::TimedOut) => {
                Self::Failed
            }
            (state, _) => state,
        }
    }

    pub fn start(hash: [u8; 32]) -> Self {
        Self::AwaitingEcho { hash }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_write_match_expires_and_accepts_nonce_or_hash() {
        let start = Instant::now();
        let mut ledger = SelfWriteLedger::new(Duration::from_secs(2), 2);
        ledger.register([1; 32], "nonce-1", start);

        assert!(ledger.matches([1; 32], &CaptureLineage::default(), start));
        assert!(ledger.matches(
            [9; 32],
            &CaptureLineage {
                origin_device: None,
                write_nonce: Some("nonce-1".into()),
            },
            start + Duration::from_secs(1)
        ));
        assert!(!ledger.matches(
            [1; 32],
            &CaptureLineage::default(),
            start + Duration::from_secs(3)
        ));
    }

    #[test]
    fn loss_accounting_separates_policy_from_loss() {
        let mut ledger = CaptureLossLedger::default();
        ledger.captured();
        ledger.dropped(DropReason::SelfWriteSuppressed);
        ledger.dropped(DropReason::StoreFailure);

        assert_eq!(
            ledger.snapshot(),
            CaptureCounters {
                captured: 1,
                intentionally_skipped: 1,
                lost: 1,
                by_reason: BTreeMap::from([
                    (DropReason::SelfWriteSuppressed, 1),
                    (DropReason::StoreFailure, 1),
                ]),
            }
        );
    }

    #[test]
    fn generation_tracker_reports_exact_gap() {
        let mut tracker = GenerationTracker::default();
        assert_eq!(
            tracker.observe(CaptureGeneration {
                epoch: 1,
                sequence: 3
            }),
            GenerationObservation::First
        );
        assert_eq!(
            tracker.observe(CaptureGeneration {
                epoch: 1,
                sequence: 7
            }),
            GenerationObservation::Gap { missed: 3 }
        );
        assert_eq!(
            tracker.observe(CaptureGeneration {
                epoch: 1,
                sequence: 6
            }),
            GenerationObservation::Stale
        );
    }

    #[test]
    fn skipped_ring_is_bounded_and_byte_free() {
        let mut ring = SkippedCaptureRing::new(2);
        let started = Instant::now();
        for offset in 1..=3 {
            ring.push(SkippedCapture {
                observed_at: started + Duration::from_millis(offset),
                reason: DropReason::Concealed,
                provenance: CaptureProvenance::default(),
                generation: None,
                content_hash: [offset as u8; 32],
            });
        }
        assert_eq!(
            ring.entries()
                .map(|entry| entry.observed_at.duration_since(started).as_millis())
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
    }
}
