//! Trait definitions for the four platform backends, plus shared key types.
//!
//! These traits are intentionally minimal for the MVP. They are the seam at
//! which native per-OS backends can later be swapped in.

use vbuff_types::{CaptureGeneration, CaptureLineage, CaptureProvenance, Flavor};

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

/// Which OS selection supplied the snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardSelection {
    #[default]
    Clipboard,
    Primary,
}

/// Retention request attached to a clipboard write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardRetention {
    #[default]
    SystemDefault,
    ExcludeFromSystemHistory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardWriteReceipt {
    RetentionHintApplied,
    RetentionHintUnsupported,
}

/// A coherent snapshot of one clipboard generation.
#[derive(Clone, Debug)]
pub struct CapturedClipboard {
    /// Every flavor read from the clipboard, byte-for-byte where possible.
    pub flavors: Vec<Flavor>,
    pub provenance: CaptureProvenance,
    pub generation: Option<CaptureGeneration>,
    pub lineage: CaptureLineage,
    pub selection: ClipboardSelection,
    /// Native backend confirmed the owner/generation remained stable while
    /// every flavor was materialized.
    pub coherent_generation: bool,
    /// PRIMARY has remained stable and an intent signal was observed.
    pub primary_intended: bool,
    /// Authoritative OS sensitivity marker; the gate must fail closed.
    pub concealed: bool,
}

impl Default for CapturedClipboard {
    fn default() -> Self {
        Self {
            flavors: Vec::new(),
            provenance: CaptureProvenance::default(),
            generation: None,
            lineage: CaptureLineage::default(),
            selection: ClipboardSelection::Clipboard,
            coherent_generation: true,
            primary_intended: true,
            concealed: false,
        }
    }
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

    /// Native backends may attach `lineage.write_nonce` as a private sentinel
    /// flavor. The arboard fallback uses the shared hash ledger instead.
    fn write_tagged(&mut self, flavors: &[Flavor], _lineage: &CaptureLineage) -> Result<()> {
        self.write(flavors)
    }

    /// Write with an OS history-retention hint. Generic backends preserve the
    /// write but report that the hint could not be applied.
    fn write_tagged_with_retention(
        &mut self,
        flavors: &[Flavor],
        lineage: &CaptureLineage,
        retention: ClipboardRetention,
    ) -> Result<ClipboardWriteReceipt> {
        self.write_tagged(flavors, lineage)?;
        Ok(match retention {
            ClipboardRetention::SystemDefault => ClipboardWriteReceipt::RetentionHintApplied,
            ClipboardRetention::ExcludeFromSystemHistory => {
                ClipboardWriteReceipt::RetentionHintUnsupported
            }
        })
    }

    /// Atomically write sensitive bytes only when the backend can guarantee
    /// exclusion from OS clipboard history. The default returns Unsupported
    /// without passing payload bytes to the system clipboard.
    fn write_sensitive_excluding_history(
        &mut self,
        _flavors: &[Flavor],
        _lineage: &CaptureLineage,
    ) -> Result<ClipboardWriteReceipt> {
        Ok(ClipboardWriteReceipt::RetentionHintUnsupported)
    }

    /// Clear every representation from the clipboard.
    fn clear(&mut self) -> Result<()>;
}

/// Registers and delivers global hotkeys.
///
/// Event delivery uses the backing crate's global receiver; callers poll it
/// from their event loop (see the app crate). Registration managers may wrap
/// thread-affine OS handles, so they remain on the creating event-loop thread;
/// only the event channel crosses thread boundaries.
pub trait HotkeyBackend {
    /// Register the given combo as the show/hide hotkey. Returns the opaque
    /// platform id of the registered hotkey.
    fn register(&mut self, combo: &KeyCombo) -> Result<u32>;

    /// Unregister a previously registered hotkey by id.
    fn unregister(&mut self, id: u32) -> Result<()>;
}

/// Simulates a paste keystroke into the focused application.
pub trait PasteBackend: Send {
    /// Release modifiers that may still be held from the picker hotkey.
    fn sanitize_modifiers(&mut self) -> Result<()> {
        Ok(())
    }

    /// Send the platform paste combo (Cmd+V on macOS, Ctrl+V elsewhere).
    fn paste(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    struct ThreadAffineHotkey {
        marker: Rc<()>,
    }

    impl HotkeyBackend for ThreadAffineHotkey {
        fn register(&mut self, _combo: &KeyCombo) -> Result<u32> {
            Ok(7)
        }

        fn unregister(&mut self, _id: u32) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn hotkey_backends_may_be_thread_affine() {
        let mut backend = ThreadAffineHotkey {
            marker: Rc::new(()),
        };
        let combo = KeyCombo {
            modifiers: vec![Modifier::Control],
            key: "V".to_string(),
        };

        let id = backend.register(&combo).unwrap();
        backend.unregister(id).unwrap();

        assert_eq!(id, 7);
        assert_eq!(Rc::strong_count(&backend.marker), 1);
    }
}
