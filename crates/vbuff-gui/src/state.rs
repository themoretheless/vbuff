//! Shared state and action types exchanged between the GUI and the app wiring.

use std::sync::{Arc, Mutex};

use vbuff_types::{CaptureHealth, Clip, ClipId, CommandNotice, NoticeLevel};

/// The live state the GUI renders. Owned behind a [`SharedState`] lock so the
/// background capture thread can push new clips while the GUI reads them.
#[derive(Default)]
pub struct AppState {
    /// The current clip list, already ordered (pinned first, then recency).
    pub clips: Vec<Clip>,
    /// True if clipboard capture is currently paused.
    pub paused: bool,
    /// Current health of the resident capture worker.
    pub capture_health: CaptureHealth,
    /// Latest redacted command result, dismissible from the popup.
    pub notice: Option<CommandNotice>,
    /// Set to true by the wiring when the popup should be shown/focused.
    pub show_requested: bool,
    /// A monotonically increasing revision; bumped when `clips` changes so the
    /// GUI can cheaply detect updates.
    pub revision: u64,
}

impl AppState {
    /// Construct the initial state from the persisted history snapshot.
    pub fn with_clips(clips: Vec<Clip>) -> Self {
        Self {
            clips,
            ..Default::default()
        }
    }

    /// Replace the clip list and bump the revision.
    pub fn set_clips(&mut self, clips: Vec<Clip>) {
        self.clips = clips;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Request the popup be shown and focused on the next frame.
    pub fn request_show(&mut self) {
        self.show_requested = true;
    }

    /// Publish capture health, returning true only when it changed.
    pub fn set_capture_health(&mut self, health: CaptureHealth) -> bool {
        if self.capture_health == health {
            return false;
        }
        self.capture_health = health;
        true
    }

    /// Replace the current command notice with a redacted message.
    pub fn set_notice(&mut self, level: NoticeLevel, message: impl Into<String>) {
        self.notice = Some(CommandNotice {
            level,
            message: message.into(),
        });
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
    }
}

/// A thread-safe handle to [`AppState`].
pub type SharedState = Arc<Mutex<AppState>>;

/// A high-level user action emitted by the GUI, drained and handled by the app
/// wiring (which owns the store, clipboard, and paste backends).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    /// Paste the given clip back into the previously focused app.
    Paste(ClipId),
    /// Pin or unpin a clip.
    SetPinned(ClipId, bool),
    /// Delete a single clip.
    Delete(ClipId),
    /// Clear history while preserving pinned clips.
    ClearHistory,
    /// Toggle capture pause.
    TogglePause,
    /// Dismiss the current command result.
    DismissNotice,
    /// Hide the popup (Esc / focus loss).
    Hide,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_starts_capture_without_a_notice() {
        let state = AppState::with_clips(Vec::new());

        assert_eq!(state.capture_health, CaptureHealth::Starting);
        assert!(state.notice.is_none());
        assert!(!state.paused);
    }

    #[test]
    fn health_changes_are_deduplicated() {
        let mut state = AppState::default();

        assert!(state.set_capture_health(CaptureHealth::Watching));
        assert!(!state.set_capture_health(CaptureHealth::Watching));
        assert_eq!(state.capture_health.label(), "Capture active");
    }

    #[test]
    fn command_notice_can_be_replaced_and_cleared() {
        let mut state = AppState::default();
        state.set_notice(NoticeLevel::Warning, "Copy-only mode");
        assert_eq!(state.notice.as_ref().unwrap().level, NoticeLevel::Warning);

        state.clear_notice();
        assert!(state.notice.is_none());
    }
}
