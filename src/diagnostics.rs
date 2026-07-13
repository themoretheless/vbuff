//! Redacted runtime status publication for popup and tray consumers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use vbuff_core::capture::{CaptureOutcome, DropClass};
use vbuff_gui::SharedState;
use vbuff_types::{CaptureHealth, NoticeLevel};

use crate::runtime_metrics::{RuntimeMetrics, RuntimeSnapshot};

/// Narrow publisher used by capture and command handling.
#[derive(Clone)]
pub(crate) struct Diagnostics {
    shared: SharedState,
    runtime: RuntimeMetrics,
    recover_skipped: Arc<AtomicBool>,
}

impl Diagnostics {
    pub(crate) fn new(shared: SharedState) -> Self {
        Self {
            shared,
            runtime: RuntimeMetrics::default(),
            recover_skipped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Publish capture health, returning true only for a state transition.
    pub(crate) fn capture_health(&self, health: CaptureHealth) -> bool {
        self.runtime.health(health);
        match self.shared.lock() {
            Ok(mut state) => state.set_capture_health(health),
            Err(_) => {
                tracing::error!("cannot publish capture health: GUI state mutex poisoned");
                false
            }
        }
    }

    pub(crate) fn poll_interval(&self, interval: std::time::Duration) {
        self.runtime.poll_interval(interval);
    }

    pub(crate) fn capture_outcome(&self, outcome: CaptureOutcome, count: u64) {
        self.runtime.outcome(outcome, count);
        if let Ok(mut state) = self.shared.lock() {
            match outcome {
                CaptureOutcome::Captured => state.add_capture_stats(count, 0, 0),
                CaptureOutcome::Dropped(reason) if reason.class() == DropClass::Intentional => {
                    state.add_capture_stats(0, count, 0);
                }
                CaptureOutcome::Dropped(_) => state.add_capture_stats(0, 0, count),
            }
        }
    }

    pub(crate) fn budget_trip(&self) {
        self.runtime.budget_trip();
    }

    pub(crate) fn write_queue_depth(&self, depth: u64) {
        self.runtime.write_queue_depth(depth);
    }

    pub(crate) fn latency(&self, operation: &'static str, latency: std::time::Duration) {
        self.runtime.latency(operation, latency);
    }

    pub(crate) fn offer_skipped_recovery(&self, window: Duration) {
        if let Ok(mut state) = self.shared.lock() {
            state.offer_skipped_recovery(Instant::now(), window);
        }
    }

    pub(crate) fn clear_skipped_recovery(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.clear_skipped_recovery();
        }
    }

    pub(crate) fn request_skipped_recovery(&self) -> bool {
        let available = self
            .shared
            .lock()
            .map(|mut state| state.take_skipped_recovery(Instant::now()))
            .unwrap_or(false);
        if available {
            self.recover_skipped.store(true, Ordering::Release);
        }
        available
    }

    pub(crate) fn take_skipped_recovery(&self) -> bool {
        self.recover_skipped.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn runtime_snapshot(&self) -> Option<RuntimeSnapshot> {
        self.runtime.snapshot()
    }

    pub(crate) fn install_panic_hook(&self) {
        match crate::runtime_metrics::crash_metrics_path() {
            Ok(path) => self.runtime.install_panic_hook(path),
            Err(error) => tracing::warn!("crash metrics disabled: {error}"),
        }
    }

    pub(crate) fn notice(&self, level: NoticeLevel, message: &'static str) {
        if let Ok(mut state) = self.shared.lock() {
            state.set_notice(level, message);
        } else {
            tracing::error!("cannot publish command notice: GUI state mutex poisoned");
        }
    }

    pub(crate) fn clear_notice(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.clear_notice();
        } else {
            tracing::error!("cannot clear command notice: GUI state mutex poisoned");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use vbuff_gui::AppState;

    #[test]
    fn health_and_notices_reach_the_shared_state() {
        let shared = Arc::new(Mutex::new(AppState::default()));
        let diagnostics = Diagnostics::new(Arc::clone(&shared));

        assert!(diagnostics.capture_health(CaptureHealth::Watching));
        assert!(!diagnostics.capture_health(CaptureHealth::Watching));
        diagnostics.notice(NoticeLevel::Warning, "Copy-only mode");

        let state = shared.lock().unwrap();
        assert_eq!(state.capture_health, CaptureHealth::Watching);
        assert_eq!(state.notice.as_ref().unwrap().level, NoticeLevel::Warning);
    }
}
