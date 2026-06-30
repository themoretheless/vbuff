//! Trait definitions for the four platform backends, plus shared key types.
//!
//! These traits are intentionally minimal for the MVP. They are the seam at
//! which native per-OS backends can later be swapped in.

use vbuff_types::Flavor;

use crate::Result;

/// A keyboard modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    /// Control key.
    Control,
    /// Alt / Option key.
    Alt,
    /// Shift key.
    Shift,
    /// Command (macOS) / Super / Windows key.
    Meta,
}

/// A parsed global-hotkey combination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCombo {
    /// The modifier set (e.g. Ctrl+Shift).
    pub modifiers: Vec<Modifier>,
    /// The main key, as an uppercase character or named key (e.g. `V`, `Space`).
    pub key: String,
}

/// A snapshot of the clipboard's current content, as flavors.
#[derive(Clone, Debug, Default)]
pub struct CapturedClipboard {
    /// Every flavor read from the clipboard, byte-for-byte where possible.
    pub flavors: Vec<Flavor>,
    /// The frontmost application's identifier, if the backend could learn it.
    pub source_app: Option<String>,
}

impl CapturedClipboard {
    /// True if nothing usable was captured.
    pub fn is_empty(&self) -> bool {
        self.flavors.is_empty()
    }
}

/// Reads from and writes to the system clipboard.
pub trait ClipboardBackend: Send {
    /// Read the current clipboard contents as a flavor set.
    fn read(&mut self) -> Result<CapturedClipboard>;

    /// Write a flavor set back to the clipboard (for paste-back).
    fn write(&mut self, flavors: &[Flavor]) -> Result<()>;
}

/// Registers and delivers global hotkeys.
///
/// Event delivery uses the backing crate's global receiver; callers poll it
/// from their event loop (see the app crate). The trait therefore only covers
/// (un)registration.
pub trait HotkeyBackend: Send {
    /// Register the given combo as the show/hide hotkey. Returns the opaque
    /// platform id of the registered hotkey.
    fn register(&mut self, combo: &KeyCombo) -> Result<u32>;

    /// Unregister a previously registered hotkey by id.
    fn unregister(&mut self, id: u32) -> Result<()>;
}

/// Simulates a paste keystroke into the focused application.
pub trait PasteBackend: Send {
    /// Send the platform paste combo (Cmd+V on macOS, Ctrl+V elsewhere).
    fn paste(&mut self) -> Result<()>;
}
