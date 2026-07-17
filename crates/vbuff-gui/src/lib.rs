//! egui/eframe popup UI for vbuff.
//!
//! The GUI is intentionally decoupled from storage and platform code. It holds
//! a snapshot of clips ([`AppState`]) behind a shared lock and emits high-level
//! [`UiAction`]s that the app crate translates into store/clipboard/paste calls.
//! This keeps the view unit-friendly and lets the wiring own all side effects.

mod app;
mod design;
mod state;
mod view;

pub use app::PopupApp;
pub use state::{AppState, SharedState, StarterPack, UiAction};

/// Preferred popup size used by the root composition layer.
pub fn popup_size() -> [f32; 2] {
    design::POPUP_SIZE
}

/// Minimum usable popup size; keeps rows and actions from overlapping.
pub fn popup_min_size() -> [f32; 2] {
    design::POPUP_MIN_SIZE
}
