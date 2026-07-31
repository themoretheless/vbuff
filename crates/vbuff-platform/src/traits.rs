//! Trait definitions for the four platform backends, plus shared key types.
//!
//! These traits are intentionally minimal for the MVP. They are the seam at
//! which native per-OS backends can later be swapped in.

use vbuff_types::{
    CaptureGeneration, CaptureLineage, CaptureProvenance, ConcealmentSignal, Flavor,
    GenerationCoherence, ProvenanceConfidence, SelectionIntent,
};

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

/// Which OS selection supplied the snapshot. Historical platform-side name
/// for the shared [`vbuff_types::SelectionSource`] vocabulary: the capture
/// gate consumes exactly this value, so there is nothing to translate.
pub use vbuff_types::SelectionSource as ClipboardSelection;

/// Retention request attached to a clipboard write.
///
/// The variants are a strength ladder, and every step of it is expressed in
/// this one type rather than in a boolean pair or in *which write method the
/// caller reached for*:
///
/// * `Unspecified` - the caller made no retention decision at all. This is the
///   [`Default`], so a [`WriteOptions::default()`] write asks for no privilege
///   and is granted none: byte-for-byte the plain untagged write.
/// * `SystemDefault` - the caller deliberately accepted OS clipboard history.
///   Kept distinct from `Unspecified` on purpose: "nobody decided" and "someone
///   decided that history is fine" are different facts, and only the first may
///   ever be tightened by a future default without overriding a caller.
/// * `PreferExcludeFromSystemHistory` - best effort. The payload is written
///   even when the hint cannot be applied; the receipt reports which happened.
/// * `RequireExcludeFromSystemHistory` - all or nothing. A backend that cannot
///   prove exclusion must not place the payload on the clipboard at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipboardRetention {
    /// No retention decision was made by the caller (fail-closed default).
    #[default]
    Unspecified,
    /// OS clipboard history is explicitly acceptable for this write.
    SystemDefault,
    /// Ask the OS to keep this write out of clipboard history if it can.
    PreferExcludeFromSystemHistory,
    /// Write only if exclusion from OS clipboard history is guaranteed.
    RequireExcludeFromSystemHistory,
}

/// What a backend can actually attest about a completed write.
///
/// As with the per-read evidence on [`CapturedClipboard`], this receipt is the
/// *only* channel through which a backend states what a write achieved: there
/// is deliberately no static write-capability declaration that could drift away
/// from what individual writes report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardWriteReceipt {
    /// The payload was written and no retention hint was requested. This is
    /// explicitly *not* a claim that the payload stayed out of OS history.
    NoRetentionRequested,
    /// The payload was written and the requested retention was honored.
    RetentionHintApplied,
    /// The requested retention could not be honored. A `Prefer` request still
    /// placed the payload; a `Require` request placed nothing.
    RetentionHintUnsupported,
}

/// Everything a caller can ask of a clipboard write.
///
/// One options struct instead of a family of `write_*` methods: a new request
/// becomes a field with a default, not a fifth method that every backend must
/// learn about at once.
///
/// [`Default`] is the fail-closed position - no lineage to attach and no
/// retention decision - and every field added later must preserve it: a default
/// write may never ask for, or be granted, more than the plain untagged write
/// of a single flavor set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Lineage of the clip being written back. Native backends attach
    /// `write_nonce` as a private sentinel flavor; the generic backend relies on
    /// the shared hash ledger instead. The default carries no nonce, which is
    /// the same "nothing to attach" the untagged write expressed - there is no
    /// "unset versus deliberately empty" distinction to lose here, because an
    /// absent nonce already *is* the deliberate absence.
    pub lineage: CaptureLineage,
    /// How this write should interact with OS clipboard history.
    pub retention: ClipboardRetention,
}

impl WriteOptions {
    /// Options that tag the write with `lineage` and request nothing else.
    pub fn tagged(lineage: CaptureLineage) -> Self {
        Self {
            lineage,
            retention: ClipboardRetention::default(),
        }
    }

    /// Attach a retention request.
    #[must_use]
    pub fn with_retention(mut self, retention: ClipboardRetention) -> Self {
        self.retention = retention;
        self
    }
}

/// A snapshot of one clipboard read, with the evidence the backend could
/// actually observe about it.
///
/// The four evidence fields are the *only* channel through which a backend
/// states what it can prove: a backend that cannot observe a signal reports
/// `Unknown` for it on every read, which is exactly the same information a
/// separate static capability declaration would carry — with no second copy
/// to drift out of sync. Every one of them defaults to `Unknown` (fail
/// closed), so a backend that forgets a field claims nothing rather than
/// claiming proof it does not have.
#[derive(Clone, Debug, Default)]
pub struct CapturedClipboard {
    /// Every flavor read from the clipboard, byte-for-byte where possible.
    pub flavors: Vec<Flavor>,
    pub provenance: CaptureProvenance,
    pub generation: Option<CaptureGeneration>,
    pub lineage: CaptureLineage,
    pub selection: ClipboardSelection,
    /// Whether the owner/generation stayed stable while every flavor was
    /// materialized. `Unknown` means the backend cannot observe clipboard
    /// ownership at all — not that the read is proven coherent.
    pub coherence: GenerationCoherence,
    /// Whether a deliberate-copy signal was observed. Consulted only for
    /// PRIMARY selections. `Unknown` means the backend cannot observe intent.
    pub intent: SelectionIntent,
    /// OS concealment evidence for this read. `Unknown` means the backend
    /// cannot read concealment markers — not that the content is proven
    /// clear; policy decides how to degrade on that uncertainty.
    pub concealment: ConcealmentSignal,
    /// Confidence that `provenance` is authoritative. `Unknown` means
    /// source-dependent rules cannot be evaluated honestly.
    pub provenance_confidence: ProvenanceConfidence,
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
    ///
    /// The returned [`CapturedClipboard`] carries the backend's evidence for
    /// *this* read, and is the single channel for it: report `Unknown` for
    /// anything the backend cannot observe rather than asserting a default.
    /// There is deliberately no separate static capability method — a second
    /// declaration of the same facts can only drift away from what reads
    /// actually report.
    fn read(&mut self) -> Result<CapturedClipboard>;

    /// Implementation primitive: place `flavors` on the system clipboard.
    ///
    /// This applies no policy of its own - [`ClipboardBackend::write`] has
    /// already decided that the payload is allowed to reach the clipboard by
    /// the time this is called. Backends implement this; callers use `write`.
    /// `options` is passed through so a backend can consult
    /// [`WriteOptions::lineage`] (sentinel flavor) and any request added later,
    /// not so it can re-decide retention.
    fn place_flavors(&mut self, flavors: &[Flavor], options: &WriteOptions) -> Result<()>;

    /// Write a flavor set back to the clipboard (for paste-back), honoring
    /// `options` as far as this backend can, and reporting what it achieved.
    ///
    /// This is the single write entry point for callers: everything a write can
    /// ask for lives in [`WriteOptions`], so a new request never forks this into
    /// another method that some backend forgets to implement.
    ///
    /// The default implementation is the honest contract of a backend with no
    /// OS retention control: it writes, and it never claims a hint it did not
    /// apply. It also fails closed on
    /// [`ClipboardRetention::RequireExcludeFromSystemHistory`] - payload bytes
    /// never reach a clipboard that cannot keep them out of OS history. A
    /// backend that overrides this method takes that guarantee over with it.
    fn write(
        &mut self,
        flavors: &[Flavor],
        options: &WriteOptions,
    ) -> Result<ClipboardWriteReceipt> {
        match options.retention {
            // Nothing was asked for, so nothing is claimed.
            ClipboardRetention::Unspecified => {
                self.place_flavors(flavors, options)?;
                Ok(ClipboardWriteReceipt::NoRetentionRequested)
            }
            // The caller explicitly accepted OS history; that is satisfiable
            // by every backend, so the request really was honored.
            ClipboardRetention::SystemDefault => {
                self.place_flavors(flavors, options)?;
                Ok(ClipboardWriteReceipt::RetentionHintApplied)
            }
            // Best effort: the write is preserved, the missing hint reported.
            ClipboardRetention::PreferExcludeFromSystemHistory => {
                self.place_flavors(flavors, options)?;
                Ok(ClipboardWriteReceipt::RetentionHintUnsupported)
            }
            // All or nothing: refuse before the payload is handed to the OS.
            ClipboardRetention::RequireExcludeFromSystemHistory => {
                Ok(ClipboardWriteReceipt::RetentionHintUnsupported)
            }
        }
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

    /// A backend with no OS retention control, i.e. everything the workspace
    /// ships today. It implements only the primitive and inherits `write`.
    #[derive(Default)]
    struct RecordingClipboard {
        placed: Vec<Vec<Flavor>>,
        seen: Vec<WriteOptions>,
    }

    impl ClipboardBackend for RecordingClipboard {
        fn read(&mut self) -> Result<CapturedClipboard> {
            Ok(CapturedClipboard::default())
        }

        fn place_flavors(&mut self, flavors: &[Flavor], options: &WriteOptions) -> Result<()> {
            self.placed.push(flavors.to_vec());
            self.seen.push(options.clone());
            Ok(())
        }

        fn clear(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// Stand-in for a future native backend that can prove exclusion from OS
    /// clipboard history: it overrides `write` and takes the guarantee with it.
    #[derive(Default)]
    struct ExcludingClipboard {
        placed: Vec<Vec<Flavor>>,
    }

    impl ClipboardBackend for ExcludingClipboard {
        fn read(&mut self) -> Result<CapturedClipboard> {
            Ok(CapturedClipboard::default())
        }

        fn place_flavors(&mut self, flavors: &[Flavor], _options: &WriteOptions) -> Result<()> {
            self.placed.push(flavors.to_vec());
            Ok(())
        }

        fn write(
            &mut self,
            flavors: &[Flavor],
            options: &WriteOptions,
        ) -> Result<ClipboardWriteReceipt> {
            self.place_flavors(flavors, options)?;
            Ok(ClipboardWriteReceipt::RetentionHintApplied)
        }

        fn clear(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn payload() -> Vec<Flavor> {
        vec![Flavor::inline(
            "text/plain;charset=utf-8",
            b"hello".to_vec(),
        )]
    }

    fn nonce_lineage() -> CaptureLineage {
        CaptureLineage {
            origin_device: None,
            write_nonce: Some("nonce-1".to_string()),
        }
    }

    #[test]
    fn default_write_options_ask_for_nothing_and_claim_nothing() {
        let options = WriteOptions::default();
        assert_eq!(options.retention, ClipboardRetention::Unspecified);
        assert_eq!(options.lineage, CaptureLineage::default());
        assert!(options.lineage.write_nonce.is_none());

        let mut backend = RecordingClipboard::default();
        let receipt = backend.write(&payload(), &options).unwrap();

        // Exactly the effect of the historical `write(flavors)`: the payload
        // lands unchanged, nothing extra is requested, nothing is asserted
        // about OS history.
        assert_eq!(backend.placed, vec![payload()]);
        assert_eq!(backend.seen, vec![WriteOptions::default()]);
        assert_eq!(receipt, ClipboardWriteReceipt::NoRetentionRequested);
    }

    #[test]
    fn backend_sees_the_options_the_caller_passed() {
        let options = WriteOptions::tagged(nonce_lineage())
            .with_retention(ClipboardRetention::PreferExcludeFromSystemHistory);
        let mut backend = RecordingClipboard::default();

        backend.write(&payload(), &options).unwrap();

        assert_eq!(backend.seen, vec![options]);
        assert_eq!(
            backend.seen[0].lineage.write_nonce.as_deref(),
            Some("nonce-1")
        );
    }

    #[test]
    fn tagged_options_request_no_retention_by_themselves() {
        let options = WriteOptions::tagged(nonce_lineage());
        assert_eq!(options.retention, ClipboardRetention::Unspecified);
    }

    #[test]
    fn required_exclusion_never_reaches_a_backend_that_cannot_exclude() {
        let mut backend = RecordingClipboard::default();
        let receipt = backend
            .write(
                &payload(),
                &WriteOptions::default()
                    .with_retention(ClipboardRetention::RequireExcludeFromSystemHistory),
            )
            .unwrap();

        assert_eq!(receipt, ClipboardWriteReceipt::RetentionHintUnsupported);
        assert!(backend.placed.is_empty(), "payload must not reach the OS");
    }

    #[test]
    fn preferred_exclusion_still_writes_and_reports_the_missing_hint() {
        let mut backend = RecordingClipboard::default();
        let receipt = backend
            .write(
                &payload(),
                &WriteOptions::default()
                    .with_retention(ClipboardRetention::PreferExcludeFromSystemHistory),
            )
            .unwrap();

        assert_eq!(receipt, ClipboardWriteReceipt::RetentionHintUnsupported);
        assert_eq!(backend.placed.len(), 1);
    }

    #[test]
    fn unspecified_retention_stays_distinguishable_from_explicit_system_default() {
        let mut backend = RecordingClipboard::default();
        let unspecified = backend.write(&payload(), &WriteOptions::default()).unwrap();
        let explicit = backend
            .write(
                &payload(),
                &WriteOptions::default().with_retention(ClipboardRetention::SystemDefault),
            )
            .unwrap();

        // Same effect on the clipboard, different fact about the caller: the
        // receipt must not report "hint applied" for a write that requested no
        // hint at all.
        assert_eq!(backend.placed.len(), 2);
        assert_eq!(unspecified, ClipboardWriteReceipt::NoRetentionRequested);
        assert_eq!(explicit, ClipboardWriteReceipt::RetentionHintApplied);
        assert_ne!(unspecified, explicit);
    }

    #[test]
    fn a_backend_that_can_exclude_accepts_a_required_write() {
        let mut backend = ExcludingClipboard::default();
        let receipt = backend
            .write(
                &payload(),
                &WriteOptions::tagged(nonce_lineage())
                    .with_retention(ClipboardRetention::RequireExcludeFromSystemHistory),
            )
            .unwrap();

        assert_eq!(receipt, ClipboardWriteReceipt::RetentionHintApplied);
        assert_eq!(backend.placed, vec![payload()]);
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
