//! Shared state and action types exchanged between the GUI and the app wiring.

use std::sync::{Arc, Mutex};

use vbuff_types::{Clip, ClipId};

/// The live state the GUI renders. Owned behind a [`SharedState`] lock so the
/// background capture thread can push new clips while the GUI reads them.
#[derive(Default)]
pub struct AppState {
    /// The current clip list, already ordered (pinned first, then recency).
    pub clips: Vec<Clip>,
    /// True if clipboard capture is currently paused.
    pub paused: bool,
    /// Set to true by the wiring when the popup should be shown/focused.
    pub show_requested: bool,
    /// A monotonically increasing revision; bumped when `clips` changes so the
    /// GUI can cheaply detect updates.
    pub revision: u64,
}

impl AppState {
    /// Replace the clip list and bump the revision.
    pub fn set_clips(&mut self, clips: Vec<Clip>) {
        self.clips = clips;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Request the popup be shown and focused on the next frame.
    pub fn request_show(&mut self) {
        self.show_requested = true;
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
    /// Hide the popup (Esc / focus loss).
    Hide,
}
