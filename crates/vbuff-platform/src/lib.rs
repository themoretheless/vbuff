//! Platform backend traits and their cross-platform MVP implementations.
//!
//! The architecture funnels all OS variance through a small set of traits so
//! the rest of the app never names an operating system. For the MVP each trait
//! has a single cross-platform implementation built on mature crates
//! (`arboard`, `global-hotkey`, `enigo`). Native per-OS backends can replace
//! these later without touching callers.
//!
//! * [`ClipboardBackend`] - read/write clipboard flavors (`arboard`).
//! * [`HotkeyBackend`] - register global hotkeys (`global-hotkey`).
//! * [`PasteBackend`] - simulate a paste keystroke (`enigo`).
//! * [`TrayBackend`] - a status-bar/tray icon (`tray-icon`, app crate).

mod clipboard;
mod error;
mod hotkey;
mod paste;
pub mod traits;

pub use error::PlatformError;
pub use traits::{
    CapturedClipboard, ClipboardBackend, HotkeyBackend, KeyCombo, Modifier, PasteBackend,
};

pub use clipboard::ArboardClipboard;
pub use hotkey::{parse_combo, GlobalHotkeyBackend};
pub use paste::EnigoPaste;

/// Result type for platform operations.
pub type Result<T> = std::result::Result<T, PlatformError>;

/// The modifier key used to trigger a paste on the current OS.
///
/// macOS uses Command; everything else uses Control.
pub fn paste_modifier() -> Modifier {
    #[cfg(target_os = "macos")]
    {
        Modifier::Meta
    }
    #[cfg(not(target_os = "macos"))]
    {
        Modifier::Control
    }
}
