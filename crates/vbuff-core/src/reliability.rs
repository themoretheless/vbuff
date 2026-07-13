//! Deterministic reliability policies shared by capture implementations.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use vbuff_types::Flavor;

/// Recovery requested after a capture-health observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    None,
    Resubscribe,
    RestartBackend,
    EnterDegradedMode { retry_after: Duration },
}

/// Health observations accepted by [`CaptureSupervisor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupervisorObservation {
    Healthy,
    SubscriptionLost,
    BackendFailed,
    RecoverySucceeded,
}

/// Escalates failures from re-subscription to backend restart and bounded retry.
#[derive(Clone, Debug)]
pub struct CaptureSupervisor {
    resubscribe_limit: u8,
    restart_limit: u8,
    failures: u8,
    restarts: u8,
    degraded_retry: Duration,
}

impl CaptureSupervisor {
    pub fn new(resubscribe_limit: u8, restart_limit: u8, degraded_retry: Duration) -> Self {
        Self {
            resubscribe_limit: resubscribe_limit.max(1),
            restart_limit: restart_limit.max(1),
            failures: 0,
            restarts: 0,
            degraded_retry: degraded_retry.max(Duration::from_secs(1)),
        }
    }

    pub fn observe(&mut self, observation: SupervisorObservation) -> RecoveryAction {
        match observation {
            SupervisorObservation::Healthy | SupervisorObservation::RecoverySucceeded => {
                self.failures = 0;
                self.restarts = 0;
                RecoveryAction::None
            }
            SupervisorObservation::SubscriptionLost => {
                self.failures = self.failures.saturating_add(1);
                if self.failures <= self.resubscribe_limit {
                    RecoveryAction::Resubscribe
                } else {
                    self.failures = 0;
                    self.request_restart()
                }
            }
            SupervisorObservation::BackendFailed => self.request_restart(),
        }
    }

    fn request_restart(&mut self) -> RecoveryAction {
        self.restarts = self.restarts.saturating_add(1);
        if self.restarts <= self.restart_limit {
            RecoveryAction::RestartBackend
        } else {
            self.restarts = 0;
            RecoveryAction::EnterDegradedMode {
                retry_after: self.degraded_retry,
            }
        }
    }
}

impl Default for CaptureSupervisor {
    fn default() -> Self {
        Self::new(2, 2, Duration::from_secs(30))
    }
}

/// Payload handling selected from queue occupancy measured in bytes and items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Full,
    Preview { max_bytes: usize },
    Shed,
}

/// Bounded accounting for capture queues; it never retains clipboard content.
#[derive(Clone, Debug)]
pub struct ByteBackpressure {
    soft_bytes: usize,
    hard_bytes: usize,
    max_items: usize,
    preview_bytes: usize,
    queued_bytes: usize,
    queued_items: usize,
}

impl ByteBackpressure {
    pub fn new(
        soft_bytes: usize,
        hard_bytes: usize,
        max_items: usize,
        preview_bytes: usize,
    ) -> Self {
        let soft_bytes = soft_bytes.max(1);
        Self {
            soft_bytes,
            hard_bytes: hard_bytes.max(soft_bytes),
            max_items: max_items.max(1),
            preview_bytes: preview_bytes.max(1),
            queued_bytes: 0,
            queued_items: 0,
        }
    }

    pub fn admit(&mut self, payload_bytes: usize) -> Admission {
        if self.queued_items >= self.max_items
            || self.queued_bytes.saturating_add(payload_bytes) > self.hard_bytes
        {
            return Admission::Shed;
        }

        let admission = if self.queued_bytes.saturating_add(payload_bytes) > self.soft_bytes {
            Admission::Preview {
                max_bytes: self.preview_bytes.min(payload_bytes),
            }
        } else {
            Admission::Full
        };
        let accounted = match admission {
            Admission::Full => payload_bytes,
            Admission::Preview { max_bytes } => max_bytes,
            Admission::Shed => 0,
        };
        self.queued_bytes = self.queued_bytes.saturating_add(accounted);
        self.queued_items = self.queued_items.saturating_add(1);
        admission
    }

    pub fn release(&mut self, accounted_bytes: usize) {
        self.queued_bytes = self.queued_bytes.saturating_sub(accounted_bytes);
        self.queued_items = self.queued_items.saturating_sub(1);
    }

    pub fn update_limits(
        &mut self,
        soft_bytes: usize,
        hard_bytes: usize,
        preview_bytes: usize,
    ) -> bool {
        if self.queued_items != 0 {
            return false;
        }
        self.soft_bytes = soft_bytes.max(1);
        self.hard_bytes = hard_bytes.max(self.soft_bytes);
        self.preview_bytes = preview_bytes.max(1);
        true
    }

    pub const fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }
}

/// Preserve a valid bounded text representation when the full payload cannot
/// enter the write queue. Non-text payloads return `None` rather than storing
/// corrupt partial bytes under their original MIME type.
pub fn shed_to_text_preview(flavors: &[Flavor], max_bytes: usize) -> Option<Vec<Flavor>> {
    let text = flavors
        .iter()
        .find_map(|flavor| flavor.is_text().then(|| flavor.as_text()).flatten())?;
    let mut boundary = text.len().min(max_bytes.max(1));
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    Some(vec![Flavor::derived(
        "text/plain;charset=utf-8",
        text.as_bytes()[..boundary].to_vec(),
    )])
}

/// Coarse pressure level supplied by a native or process-level sampler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    Normal,
    Elevated,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryResponse {
    pub thumbnail_budget_bytes: usize,
    pub hot_history_limit: usize,
    pub defer_background_work: bool,
    pub reject_large_capture_bytes: Option<usize>,
}

/// Converts platform pressure into explicit, testable resource limits.
#[derive(Clone, Debug)]
pub struct MemoryPressurePolicy {
    normal_thumbnail_bytes: usize,
    normal_hot_history: usize,
    large_capture_bytes: usize,
}

impl MemoryPressurePolicy {
    pub fn new(
        normal_thumbnail_bytes: usize,
        normal_hot_history: usize,
        large_capture_bytes: usize,
    ) -> Self {
        Self {
            normal_thumbnail_bytes: normal_thumbnail_bytes.max(1),
            normal_hot_history: normal_hot_history.max(1),
            large_capture_bytes: large_capture_bytes.max(1),
        }
    }

    pub fn response(&self, pressure: MemoryPressure) -> MemoryResponse {
        match pressure {
            MemoryPressure::Normal => MemoryResponse {
                thumbnail_budget_bytes: self.normal_thumbnail_bytes,
                hot_history_limit: self.normal_hot_history,
                defer_background_work: false,
                reject_large_capture_bytes: None,
            },
            MemoryPressure::Elevated => MemoryResponse {
                thumbnail_budget_bytes: self.normal_thumbnail_bytes / 2,
                hot_history_limit: (self.normal_hot_history / 2).max(1),
                defer_background_work: true,
                reject_large_capture_bytes: Some(self.large_capture_bytes),
            },
            MemoryPressure::Critical => MemoryResponse {
                thumbnail_budget_bytes: 0,
                hot_history_limit: (self.normal_hot_history / 4).max(1),
                defer_background_work: true,
                reject_large_capture_bytes: Some(self.large_capture_bytes / 2),
            },
        }
    }
}

/// Content-free evidence retained around capture races.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureForensicEvent {
    pub observed_at: Instant,
    pub generation: Option<u64>,
    pub flavor_count: u16,
    pub total_bytes: u64,
    pub owner_changed: bool,
    pub coherent: bool,
}

#[derive(Clone, Debug)]
pub struct CaptureForensicRing {
    capacity: usize,
    entries: VecDeque<CaptureForensicEvent>,
}

impl CaptureForensicRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn push(&mut self, event: CaptureForensicEvent) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(event);
    }

    pub fn entries(&self) -> impl Iterator<Item = &CaptureForensicEvent> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_escalates_and_resets_after_success() {
        let mut supervisor = CaptureSupervisor::new(2, 1, Duration::from_secs(9));
        assert_eq!(
            supervisor.observe(SupervisorObservation::SubscriptionLost),
            RecoveryAction::Resubscribe
        );
        assert_eq!(
            supervisor.observe(SupervisorObservation::SubscriptionLost),
            RecoveryAction::Resubscribe
        );
        assert_eq!(
            supervisor.observe(SupervisorObservation::SubscriptionLost),
            RecoveryAction::RestartBackend
        );
        assert_eq!(
            supervisor.observe(SupervisorObservation::BackendFailed),
            RecoveryAction::EnterDegradedMode {
                retry_after: Duration::from_secs(9)
            }
        );
        assert_eq!(
            supervisor.observe(SupervisorObservation::RecoverySucceeded),
            RecoveryAction::None
        );
    }

    #[test]
    fn backpressure_accounts_for_preview_not_original_payload() {
        let mut queue = ByteBackpressure::new(100, 200, 3, 20);
        assert_eq!(queue.admit(90), Admission::Full);
        assert_eq!(queue.admit(80), Admission::Preview { max_bytes: 20 });
        assert_eq!(queue.queued_bytes(), 110);
        assert_eq!(queue.admit(100), Admission::Shed);
        queue.release(90);
        assert_eq!(queue.admit(60), Admission::Full);
    }

    #[test]
    fn shed_preview_keeps_utf8_valid_and_refuses_partial_images() {
        let text = [Flavor::inline("text/plain", "abЖz".as_bytes().to_vec())];
        let preview = shed_to_text_preview(&text, 3).unwrap();
        assert_eq!(preview[0].as_text(), Some("ab"));
        let image = [Flavor::inline("image/png", vec![1, 2, 3])];
        assert!(shed_to_text_preview(&image, 2).is_none());
    }

    #[test]
    fn critical_memory_pressure_disables_thumbnails() {
        let policy = MemoryPressurePolicy::new(8_000_000, 1_000, 32_000_000);
        let response = policy.response(MemoryPressure::Critical);
        assert_eq!(response.thumbnail_budget_bytes, 0);
        assert_eq!(response.hot_history_limit, 250);
        assert_eq!(response.reject_large_capture_bytes, Some(16_000_000));
    }

    #[test]
    fn forensic_ring_is_bounded_and_has_no_content_field() {
        let mut ring = CaptureForensicRing::new(1);
        let now = Instant::now();
        ring.push(CaptureForensicEvent {
            observed_at: now,
            generation: Some(1),
            flavor_count: 2,
            total_bytes: 4,
            owner_changed: false,
            coherent: true,
        });
        ring.push(CaptureForensicEvent {
            observed_at: now,
            generation: Some(2),
            flavor_count: 1,
            total_bytes: 9,
            owner_changed: true,
            coherent: false,
        });
        assert_eq!(ring.entries().count(), 1);
        assert_eq!(ring.entries().next().unwrap().generation, Some(2));
    }
}
