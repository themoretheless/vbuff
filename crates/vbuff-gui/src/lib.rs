//! egui/eframe popup UI for vbuff.
//!
//! The GUI is intentionally decoupled from storage and platform code. It holds
//! a snapshot of clips ([`AppState`]) behind a shared lock and emits high-level
//! [`UiAction`]s that the app crate translates into store/clipboard/paste calls.
//! This keeps the view unit-friendly and lets the wiring own all side effects.

mod app;
mod state;
mod view;

pub use app::PopupApp;
pub use state::{AppState, SharedState, UiAction};
