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

pub mod capabilities;
pub mod cf_html;
mod clipboard;
pub mod desktop;
mod error;
pub mod format_map;
pub mod geometry;
mod hotkey;
pub mod keymap;
pub mod lifecycle;
mod paste;
pub mod paste_fidelity;
pub mod permission;
pub mod security;
pub mod traits;
pub mod tripwire;
pub mod wayland;
pub mod windows;

pub use error::PlatformError;
pub use traits::{
    CapturedClipboard, ClipboardBackend, ClipboardRetention, ClipboardSelection,
    ClipboardWriteReceipt, HotkeyBackend, KeyCombo, Modifier, PasteBackend,
};

pub use capabilities::{CapabilityLevel, FeatureCapability, SecurityPosture};
pub use cf_html::{CfHtml, CfHtmlError, parse_cf_html};
pub use desktop::{
    DesktopShell, LinuxTrayFallback, PastePermissionLevel, PastePermissionSelfCheck,
    QuickMenuLabels, ResidentStatus, current_desktop_shell,
};
pub use format_map::{FormatFamily, FormatKey, canonical_format};
pub use keymap::{CanonicalAction, KeyBinding, KeymapTarget, canonical_keymap};
pub use paste_fidelity::{PasteConformanceIssue, PasteConformanceReport, PasteTrace};
pub use permission::{PermissionEvent, PermissionKind, PermissionState, PermissionWatchdog};
pub use security::{ProcessHardeningReport, harden_current_process};
pub use tripwire::{ClipboardReadObservation, ScrapeTripwire, TripwireAlert};

pub use clipboard::ArboardClipboard;
pub use hotkey::{GlobalHotkeyBackend, parse_combo};
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
