//! Platform backend traits and their cross-platform MVP implementations.
//!
//! The architecture funnels all OS variance through a small set of traits so
//! the rest of the app never names an operating system. For the MVP each trait
//! has a single cross-platform implementation built on mature crates
//! (`arboard`, `global-hotkey`, `enigo`). Native per-OS backends can replace
//! these later without touching callers.
//!
//! * [`ClipboardBackend`] - read/write clipboard flavors (`arboard`). The one
//!   implementation a build ships is named by [`SystemClipboard`] and opened by
//!   [`system_clipboard`]; callers never name a concrete clipboard backend.
//! * [`HotkeyBackend`] - register global hotkeys (`global-hotkey`).
//! * [`PasteBackend`] - simulate a paste keystroke (`enigo`).
//! * [`TrayBackend`] - a status-bar/tray icon (`tray-icon`, app crate).

pub mod capabilities;
pub mod cf_html;
mod clipboard;
mod confirmed_paste;
pub mod desktop;
pub mod desktop_policy;
mod error;
pub mod format_map;
pub mod geometry;
mod hotkey;
pub mod lifecycle;
mod paste;
pub mod paste_fidelity;
pub mod permission;
pub mod security;
pub mod traits;
pub mod wayland;
pub mod windows;

pub use error::PlatformError;
pub use traits::{
    CapturedClipboard, ClipboardBackend, ClipboardRetention, ClipboardSelection,
    ClipboardWriteReceipt, ConfirmedPasteBackend, HotkeyBackend, KeyCombo, Modifier, PasteBackend,
    WriteOptions,
};

pub use capabilities::{CapabilityLevel, CapabilitySeverity, FeatureCapability, SecurityPosture};
pub use cf_html::{CfHtml, CfHtmlError, parse_cf_html};
pub use desktop::{
    DesktopShell, LinuxTrayFallback, PastePermissionLevel, PastePermissionSelfCheck,
    QuickMenuLabels, ResidentStatus, current_desktop_shell,
};
pub use desktop_policy::{
    EffectiveDesktopPolicy, HotkeyResolution, LayoutAwareAccelerator, LinuxDesktop,
    ManagedInstallPolicy, NativeTheme, NativeThemeState, PermissionRepairAction,
    PermissionRepairKind, PermissionRepairPlan, ProfileLocation, ResidentAccessMode,
    linux_environment_note, permission_repair_plan, resolve_hotkey_conflict,
};
pub use format_map::{FormatFamily, FormatKey, canonical_format};
pub use paste_fidelity::{PasteConformanceIssue, PasteConformanceReport, PasteTrace};
pub use permission::{PermissionEvent, PermissionKind, PermissionState, PermissionWatchdog};
pub use security::{ProcessHardeningReport, harden_current_process};

pub use confirmed_paste::ConfirmedPaste;
pub use hotkey::{GlobalHotkeyBackend, parse_combo};
pub use paste::EnigoPaste;

/// Result type for platform operations.
pub type Result<T> = std::result::Result<T, PlatformError>;

/// Name of the clipboard backend compiled into this build.
///
/// Carried in logs so a bug report says which backend produced the behavior
/// being reported. It is a name and nothing more: what a backend can *prove*
/// is stated per read on [`CapturedClipboard`] and per write on
/// [`ClipboardWriteReceipt`], and this constant must never restate any of it.
pub const SYSTEM_CLIPBOARD_BACKEND: &str = "arboard";

/// The clipboard backend every part of this process reads and writes through.
///
/// Two production handles exist - the capture worker polls one, the paste
/// coordinator writes and re-reads through another - and they must be the same
/// backend. A process where they differ would judge capture on one backend's
/// per-read evidence while a different backend's write receipt decides whether
/// a sensitive clip may reach the clipboard at all, so the two surfaces could
/// describe different clipboards without anything failing. Naming the backend
/// once is what makes a half-finished swap impossible rather than merely
/// unlikely, which is also why the concrete `ArboardClipboard` type is not
/// exported: there is no second way to reach it.
///
/// This is an alias and not a selector, because there is nothing to select:
/// exactly one clipboard backend is compiled in. When a native backend lands,
/// the choice appears here - as a `cfg` on this alias - and both call sites
/// follow without being edited. It is deliberately not a configuration key
/// either: the user has no second backend to point one at, and a knob with a
/// single legal value is a knob that lies.
pub type SystemClipboard = clipboard::ArboardClipboard;

/// Open the process clipboard backend.
///
/// The single construction site for [`SystemClipboard`]. It reports what was
/// opened and decides nothing: callers keep their own failure policy, so this
/// cannot disagree with what a caller does about an unavailable clipboard.
pub fn system_clipboard() -> Result<SystemClipboard> {
    let backend = SystemClipboard::new()?;
    tracing::debug!(
        backend = SYSTEM_CLIPBOARD_BACKEND,
        "opened the clipboard backend compiled into this build"
    );
    Ok(backend)
}

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
