//! Redacted runtime status publication for popup and tray consumers.

use vbuff_gui::SharedState;
use vbuff_types::{CaptureHealth, NoticeLevel};

/// Narrow publisher used by capture and command handling.
#[derive(Clone)]
pub(crate) struct Diagnostics {
    shared: SharedState,
}

impl Diagnostics {
    pub(crate) fn new(shared: SharedState) -> Self {
        Self { shared }
    }

    /// Publish capture health, returning true only for a state transition.
    pub(crate) fn capture_health(&self, health: CaptureHealth) -> bool {
        match self.shared.lock() {
            Ok(mut state) => state.set_capture_health(health),
            Err(_) => {
                tracing::error!("cannot publish capture health: GUI state mutex poisoned");
                false
            }
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
