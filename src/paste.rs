//! Clipboard write and delayed paste-back coordination.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow};
use vbuff_core::capture::SelfWriteLedger;
use vbuff_core::content_hash_from_flavors;
use vbuff_core::intelligence::{PasteGuardDecision, PasteGuardFingerprint};
use vbuff_platform::lifecycle::{DisplayServer, SessionContext};
use vbuff_platform::{
    ClipboardBackend, ClipboardRetention, ClipboardWriteReceipt, ConfirmedPaste,
    ConfirmedPasteBackend, PastePermissionSelfCheck, SYSTEM_CLIPBOARD_BACKEND, SystemClipboard,
    WriteOptions, system_clipboard,
};
use vbuff_types::{CaptureLineage, ClipId, Flavor};

const PASTE_DELAY: Duration = Duration::from_millis(120);
const FOCUS_CONFIRM_DELAY: Duration = Duration::from_millis(35);
const FOCUS_CONFIRM_TIMEOUT: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingStage {
    RestoreTarget,
    ConfirmTarget { deadline: Instant },
}

/// Result of selecting a clip when paste injection is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteOutcome {
    Scheduled,
    CopiedOnly,
}

/// Owns the reusable clipboard writer and paste backend.
///
/// The clipboard default is the process-wide [`SystemClipboard`] rather than a
/// concrete backend name, so this coordinator and the capture worker cannot
/// end up on different clipboards. The paste default is still a concrete type
/// on purpose: `ConfirmedPaste` is constructed at exactly one site, so there is
/// no second place for it to diverge from, and giving it a seam it does not
/// need would only imply a choice that does not exist.
pub(crate) struct PasteCoordinator<C = SystemClipboard, P = ConfirmedPaste> {
    clipboard: Option<C>,
    paste: Option<P>,
    pending_at: Option<Instant>,
    pending_stage: Option<PendingStage>,
    pending_guard: Option<PasteGuardFingerprint>,
    target_ready: bool,
    self_writes: Arc<Mutex<SelfWriteLedger>>,
    permission_check: PastePermissionSelfCheck,
}

impl PasteCoordinator<SystemClipboard, ConfirmedPaste> {
    /// Build the resident coordinator against the process session snapshot the
    /// caller already holds. Taking the session as an argument is what keeps
    /// the "automatic paste" claim in the GUI and the session `doctor` reports
    /// from being two independent readings of the environment.
    pub(crate) fn system(
        self_writes: Arc<Mutex<SelfWriteLedger>>,
        session: &SessionContext,
    ) -> Self {
        // Same seam the capture worker opens, with the degradation policy
        // unchanged: an unavailable clipboard is logged and the coordinator
        // keeps running without a writer.
        let clipboard = system_clipboard().map_err(|error| {
            tracing::warn!(
                backend = SYSTEM_CLIPBOARD_BACKEND,
                "clipboard writer unavailable: {error}"
            );
            error
        });
        // Injection is attempted only where the backend can confirm the target
        // window immediately before sending keystrokes. Every other session
        // stays reliably copy-only rather than firing into the wrong window.
        let session_allows_paste = session.input_injection_allowed
            && !matches!(
                session.display_server,
                DisplayServer::Wayland
                    | DisplayServer::X11
                    | DisplayServer::Headless
                    | DisplayServer::Unknown
            );
        let paste = session_allows_paste
            .then(|| {
                ConfirmedPaste::new().map_err(|error| {
                    tracing::warn!(
                        "confirmed paste backend unavailable; selections will only be copied: {error}"
                    );
                    error
                })
            })
            .transpose()
            .ok()
            .flatten();
        if paste.is_none() {
            tracing::info!(
                display_server = ?session.display_server,
                remote = session.remote,
                "automatic paste disabled because target confirmation is unavailable"
            );
        }

        Self::with_backends_and_ledger_for_session(clipboard.ok(), paste, self_writes, session)
    }
}

impl<C: ClipboardBackend, P: ConfirmedPasteBackend> PasteCoordinator<C, P> {
    #[cfg(test)]
    fn with_backends(clipboard: Option<C>, paste: Option<P>) -> Self {
        Self::with_backends_and_ledger(
            clipboard,
            paste,
            Arc::new(Mutex::new(SelfWriteLedger::default())),
        )
    }

    /// Tests name the session instead of inheriting the test host's: paste
    /// scheduling must be judged against a stated session, not against
    /// whichever display server happens to run CI.
    #[cfg(test)]
    fn with_backends_and_ledger(
        clipboard: Option<C>,
        paste: Option<P>,
        self_writes: Arc<Mutex<SelfWriteLedger>>,
    ) -> Self {
        let session = SessionContext {
            display_server: DisplayServer::X11,
            remote: false,
            seat: None,
            input_injection_allowed: true,
        };
        Self::with_backends_and_ledger_for_session(clipboard, paste, self_writes, &session)
    }

    fn with_backends_and_ledger_for_session(
        clipboard: Option<C>,
        paste: Option<P>,
        self_writes: Arc<Mutex<SelfWriteLedger>>,
        session: &SessionContext,
    ) -> Self {
        let permission_check = PastePermissionSelfCheck::evaluate_session(session, paste.is_some());
        Self {
            clipboard,
            paste,
            pending_at: None,
            pending_stage: None,
            pending_guard: None,
            target_ready: false,
            self_writes,
            permission_check,
        }
    }

    pub(crate) const fn permission_check(&self) -> PastePermissionSelfCheck {
        self.permission_check
    }

    /// Capture the active application before the popup takes focus.
    pub(crate) fn prepare_target(&mut self) -> bool {
        self.cancel_pending();
        self.target_ready = false;
        let Some(paste) = self.paste.as_mut() else {
            return false;
        };
        paste.clear_target();
        match paste.capture_target() {
            Ok(()) => {
                self.target_ready = true;
                true
            }
            Err(error) => {
                tracing::debug!("paste target was not captured: {error}");
                false
            }
        }
    }

    /// Forget a target when the picker is dismissed without delivery.
    pub(crate) fn cancel_target(&mut self) {
        self.target_ready = false;
        if let Some(paste) = self.paste.as_mut() {
            paste.clear_target();
        }
    }

    /// Write a clip without injecting a paste keystroke.
    pub(crate) fn copy(&mut self, flavors: &[Flavor], sensitive: bool) -> anyhow::Result<()> {
        // Any explicit clipboard write supersedes a pending paste. This also
        // guarantees that a failed replacement write cannot leave an older
        // keystroke armed against stale clipboard contents.
        self.cancel_pending();
        let clipboard = self
            .clipboard
            .as_mut()
            .ok_or_else(|| anyhow!("clipboard writer unavailable"))?;
        let hash = content_hash_from_flavors(flavors);
        let nonce = ClipId::new().to_string_repr();
        let lineage = CaptureLineage {
            origin_device: None,
            write_nonce: Some(nonce.clone()),
        };
        let mut ledger = self
            .self_writes
            .lock()
            .map_err(|_| anyhow!("self-write ledger mutex poisoned"))?;
        // A sensitive payload may only reach the system clipboard when the
        // backend can prove OS-history exclusion; a normal one takes the same
        // hint as a preference and is written either way.
        let options = WriteOptions::tagged(lineage).with_retention(if sensitive {
            ClipboardRetention::RequireExcludeFromSystemHistory
        } else {
            ClipboardRetention::PreferExcludeFromSystemHistory
        });
        let receipt = clipboard.write(flavors, &options).with_context(|| {
            if sensitive {
                "writing sensitive clip with OS history exclusion"
            } else {
                "writing selected clip to clipboard"
            }
        })?;
        match receipt {
            // A sensitive write clears only on positive evidence that the hint
            // was applied, never on the absence of a refusal.
            ClipboardWriteReceipt::RetentionHintApplied => {}
            _ if sensitive => {
                return Err(anyhow!(
                    "sensitive clipboard write requires OS history exclusion"
                ));
            }
            _ => tracing::debug!("OS clipboard-history exclusion hint is unavailable"),
        }
        ledger.register(hash, nonce, Instant::now());
        Ok(())
    }

    /// Write first, then schedule paste-back. A failed write never sends paste.
    pub(crate) fn schedule(
        &mut self,
        flavors: &[Flavor],
        sensitive: bool,
        now: Instant,
    ) -> anyhow::Result<PasteOutcome> {
        let automatic_paste = self.paste.is_some() && std::mem::take(&mut self.target_ready);
        let guard = match automatic_paste
            .then(|| {
                PasteGuardFingerprint::from_flavors(flavors)
                    .ok_or_else(|| anyhow!("selected clip cannot be verified before paste"))
            })
            .transpose()
        {
            Ok(guard) => guard,
            Err(error) => {
                self.cancel_target();
                return Err(error);
            }
        };
        let write_started_at = Instant::now();
        if let Err(error) = self.copy(flavors, sensitive) {
            self.cancel_target();
            return Err(error);
        }
        let write_duration = Instant::now().saturating_duration_since(write_started_at);

        if let Some(guard) = guard {
            self.pending_guard = Some(guard);
            self.pending_stage = Some(PendingStage::RestoreTarget);
            self.pending_at = Some(now + write_duration + PASTE_DELAY);
            Ok(PasteOutcome::Scheduled)
        } else {
            self.cancel_target();
            Ok(PasteOutcome::CopiedOnly)
        }
    }

    /// Fire a due paste exactly once.
    pub(crate) fn poll(&mut self, now: Instant) -> Option<anyhow::Result<()>> {
        let due = self.pending_at?;
        if now < due {
            return None;
        }

        match self.pending_stage? {
            PendingStage::RestoreTarget => {
                let restore = self
                    .paste
                    .as_mut()
                    .ok_or_else(|| anyhow!("paste backend unavailable"))
                    .and_then(|backend| backend.restore_target().context("restoring paste target"));
                if let Err(error) = restore {
                    self.cancel_pending();
                    self.cancel_target();
                    return Some(Err(error));
                }
                self.pending_stage = Some(PendingStage::ConfirmTarget {
                    deadline: now + FOCUS_CONFIRM_TIMEOUT,
                });
                self.pending_at = Some(now + FOCUS_CONFIRM_DELAY);
                return None;
            }
            PendingStage::ConfirmTarget { deadline } => {
                let foreground = self
                    .paste
                    .as_mut()
                    .ok_or_else(|| anyhow!("paste backend unavailable"))
                    .and_then(|backend| {
                        backend
                            .target_is_foreground()
                            .context("confirming paste target")
                    });
                match foreground {
                    Ok(true) => {}
                    Ok(false) if now < deadline => {
                        self.pending_at = Some(now + FOCUS_CONFIRM_DELAY);
                        return None;
                    }
                    Ok(false) => {
                        self.cancel_pending();
                        self.cancel_target();
                        return Some(Err(anyhow!(
                            "captured paste target did not become foreground"
                        )));
                    }
                    Err(error) => {
                        self.cancel_pending();
                        self.cancel_target();
                        return Some(Err(error));
                    }
                }
            }
        }

        self.pending_at = None;
        self.pending_stage = None;
        let expected = self.pending_guard.take();
        let observed = self
            .clipboard
            .as_mut()
            .ok_or_else(|| anyhow!("clipboard writer unavailable"))
            .and_then(|clipboard| {
                clipboard
                    .read()
                    .map_err(anyhow::Error::from)
                    .context("verifying clipboard before paste")
            })
            .ok()
            .and_then(|captured| PasteGuardFingerprint::from_flavors(&captured.flavors));
        let decision = expected
            .as_ref()
            .map_or(PasteGuardDecision::BlockUnreadable, |expected| {
                expected.compare(observed.as_ref())
            });
        if decision != PasteGuardDecision::Allow {
            self.cancel_target();
            return Some(Err(anyhow!(
                "paste guard blocked changed clipboard: {decision:?}"
            )));
        }
        let result = self
            .paste
            .as_mut()
            .ok_or_else(|| anyhow!("paste backend unavailable"))
            .and_then(|backend| backend.paste().context("injecting paste keystroke"));
        self.cancel_target();
        Some(result)
    }

    pub(crate) fn wait_duration(&self, now: Instant) -> Option<Duration> {
        self.pending_at
            .map(|due| due.saturating_duration_since(now))
    }

    fn cancel_pending(&mut self) {
        self.pending_at = None;
        self.pending_stage = None;
        self.pending_guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_platform::{
        CapturedClipboard, PasteBackend, PastePermissionLevel, PlatformError,
        Result as PlatformResult,
    };

    struct FakeClipboard {
        fail: bool,
        writes: usize,
        current: Vec<Flavor>,
    }

    impl ClipboardBackend for FakeClipboard {
        fn read(&mut self) -> PlatformResult<CapturedClipboard> {
            Ok(CapturedClipboard {
                flavors: self.current.clone(),
                ..CapturedClipboard::default()
            })
        }

        fn place_flavors(
            &mut self,
            flavors: &[Flavor],
            _options: &WriteOptions,
        ) -> PlatformResult<()> {
            self.writes += 1;
            if self.fail {
                Err(PlatformError::Clipboard("test failure".into()))
            } else {
                self.current = flavors.to_vec();
                Ok(())
            }
        }

        fn clear(&mut self) -> PlatformResult<()> {
            Ok(())
        }
    }

    /// Records what the coordinator asked of the backend. Kept separate from
    /// `FakeClipboard` so the scheduling tests keep their literals. Like every
    /// backend shipping today it has no OS retention control, so it inherits
    /// the fail-closed default `write`.
    #[derive(Default)]
    struct RecordingClipboard {
        requested: Vec<WriteOptions>,
        current: Vec<Flavor>,
    }

    impl ClipboardBackend for RecordingClipboard {
        fn read(&mut self) -> PlatformResult<CapturedClipboard> {
            Ok(CapturedClipboard {
                flavors: self.current.clone(),
                ..CapturedClipboard::default()
            })
        }

        fn place_flavors(
            &mut self,
            flavors: &[Flavor],
            options: &WriteOptions,
        ) -> PlatformResult<()> {
            self.requested.push(options.clone());
            self.current = flavors.to_vec();
            Ok(())
        }

        fn clear(&mut self) -> PlatformResult<()> {
            Ok(())
        }
    }

    /// Stand-in for a future native backend that can prove exclusion from OS
    /// clipboard history.
    #[derive(Default)]
    struct ExcludingClipboard {
        requested: Vec<WriteOptions>,
    }

    impl ClipboardBackend for ExcludingClipboard {
        fn read(&mut self) -> PlatformResult<CapturedClipboard> {
            Ok(CapturedClipboard::default())
        }

        fn place_flavors(
            &mut self,
            _flavors: &[Flavor],
            options: &WriteOptions,
        ) -> PlatformResult<()> {
            self.requested.push(options.clone());
            Ok(())
        }

        fn write(
            &mut self,
            flavors: &[Flavor],
            options: &WriteOptions,
        ) -> PlatformResult<ClipboardWriteReceipt> {
            self.place_flavors(flavors, options)?;
            Ok(ClipboardWriteReceipt::RetentionHintApplied)
        }

        fn clear(&mut self) -> PlatformResult<()> {
            Ok(())
        }
    }

    struct FakePaste {
        calls: usize,
        captures: usize,
        restores: usize,
        foreground: bool,
    }

    impl Default for FakePaste {
        fn default() -> Self {
            Self {
                calls: 0,
                captures: 0,
                restores: 0,
                foreground: true,
            }
        }
    }

    impl PasteBackend for FakePaste {
        fn paste(&mut self) -> PlatformResult<()> {
            self.calls += 1;
            Ok(())
        }
    }

    impl ConfirmedPasteBackend for FakePaste {
        fn clear_target(&mut self) {}

        fn capture_target(&mut self) -> PlatformResult<()> {
            self.captures += 1;
            Ok(())
        }

        fn restore_target(&mut self) -> PlatformResult<()> {
            self.restores += 1;
            Ok(())
        }

        fn target_is_foreground(&mut self) -> PlatformResult<bool> {
            Ok(self.foreground)
        }
    }

    fn flavors() -> Vec<Flavor> {
        vec![Flavor::inline("text/plain", b"hello".to_vec())]
    }

    /// The resident coordinator and the capture worker must hold the *same*
    /// clipboard backend. The worker judges capture on that backend's per-read
    /// evidence, while this coordinator's write receipt decides whether a
    /// sensitive clip may reach the clipboard at all; two backends in one
    /// process would let those verdicts describe different clipboards with
    /// nothing failing to announce it.
    ///
    /// Pinned as a type identity rather than a runtime assertion, because the
    /// hazard is a swap that updates one construction site and forgets the
    /// other — that stops compiling here instead of shipping.
    #[test]
    fn the_resident_coordinator_holds_the_process_clipboard_backend() {
        // What the capture worker opens.
        let _seam: fn() -> PlatformResult<SystemClipboard> = system_clipboard;
        // What `app.rs` stores, spelled the way `app.rs` spells it.
        let _resident: Option<PasteCoordinator<SystemClipboard, ConfirmedPaste>> =
            None::<PasteCoordinator>;
    }

    /// The permission the popup shows is decided by the session snapshot the
    /// coordinator was handed, not by a private re-detection inside it.
    #[test]
    fn permission_check_follows_the_supplied_session() {
        let remote = SessionContext {
            display_server: DisplayServer::X11,
            remote: true,
            seat: None,
            input_injection_allowed: false,
        };
        let coordinator = PasteCoordinator::with_backends_and_ledger_for_session(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
            Arc::new(Mutex::new(SelfWriteLedger::default())),
            &remote,
        );

        let check = coordinator.permission_check();
        assert_eq!(check.level, PastePermissionLevel::CopyOnly);
        assert!(check.detail.contains("remote session"));
    }

    #[test]
    fn failed_clipboard_write_never_schedules_paste() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
        );
        let now = Instant::now();

        assert!(coordinator.prepare_target());
        coordinator.schedule(&flavors(), false, now).unwrap();
        coordinator.clipboard.as_mut().unwrap().fail = true;
        assert!(coordinator.prepare_target());
        assert!(coordinator.schedule(&flavors(), false, now).is_err());
        assert_eq!(coordinator.wait_duration(now), None);
        assert!(coordinator.poll(now + PASTE_DELAY).is_none());
    }

    #[test]
    fn successful_write_fires_one_delayed_paste() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
        );
        let now = Instant::now();

        assert!(coordinator.prepare_target());
        assert_eq!(
            coordinator.schedule(&flavors(), false, now).unwrap(),
            PasteOutcome::Scheduled
        );
        let restore_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(restore_due - PASTE_DELAY / 2).is_none());
        assert!(coordinator.poll(restore_due).is_none());
        let confirm_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(confirm_due).unwrap().is_ok());
        assert!(coordinator.poll(confirm_due).is_none());
        assert_eq!(coordinator.paste.as_ref().unwrap().restores, 1);
        assert_eq!(coordinator.paste.as_ref().unwrap().calls, 1);
    }

    #[test]
    fn missing_paste_backend_degrades_to_copy_only() {
        let mut coordinator = PasteCoordinator::<_, FakePaste>::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            None,
        );

        assert_eq!(
            coordinator
                .schedule(&flavors(), false, Instant::now())
                .unwrap(),
            PasteOutcome::CopiedOnly
        );
    }

    #[test]
    fn backend_without_a_captured_target_degrades_to_copy_only() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
        );

        assert_eq!(
            coordinator
                .schedule(&flavors(), false, Instant::now())
                .unwrap(),
            PasteOutcome::CopiedOnly
        );
        assert_eq!(coordinator.clipboard.as_ref().unwrap().writes, 1);
        assert!(coordinator.pending_at.is_none());
    }

    #[test]
    fn changed_clipboard_blocks_the_injection_keystroke() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
        );
        let now = Instant::now();
        assert!(coordinator.prepare_target());
        coordinator.schedule(&flavors(), false, now).unwrap();
        coordinator.clipboard.as_mut().unwrap().current = vec![Flavor::inline(
            "text/plain",
            b"0x2222222222222222222222222222222222222222".to_vec(),
        )];

        let restore_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(restore_due).is_none());
        let confirm_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(confirm_due).unwrap().is_err());
        assert_eq!(coordinator.paste.as_ref().unwrap().calls, 0);
    }

    #[test]
    fn unconfirmed_target_times_out_without_injection() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste {
                foreground: false,
                ..FakePaste::default()
            }),
        );
        let now = Instant::now();
        assert!(coordinator.prepare_target());
        assert_eq!(
            coordinator.schedule(&flavors(), false, now).unwrap(),
            PasteOutcome::Scheduled
        );

        let restore_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(restore_due).is_none());
        let timeout = restore_due + FOCUS_CONFIRM_DELAY + FOCUS_CONFIRM_TIMEOUT;
        assert!(coordinator.poll(timeout).unwrap().is_err());
        assert_eq!(coordinator.paste.as_ref().unwrap().calls, 0);
        assert!(coordinator.pending_at.is_none());
    }

    #[test]
    fn captured_target_is_single_use() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
        );
        let now = Instant::now();
        assert!(coordinator.prepare_target());
        coordinator.schedule(&flavors(), false, now).unwrap();
        let restore_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(restore_due).is_none());
        let confirm_due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(confirm_due).unwrap().is_ok());

        assert_eq!(
            coordinator
                .schedule(&flavors(), false, confirm_due)
                .unwrap(),
            PasteOutcome::CopiedOnly
        );
        assert_eq!(coordinator.paste.as_ref().unwrap().calls, 1);
    }

    #[test]
    fn unverifiable_payload_is_rejected_before_clipboard_write() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            Some(FakePaste::default()),
        );
        let opaque = vec![Flavor::inline("application/octet-stream", vec![0xff, 0xfe])];

        assert!(coordinator.prepare_target());
        assert!(
            coordinator
                .schedule(&opaque, false, Instant::now())
                .is_err()
        );
        assert_eq!(coordinator.clipboard.as_ref().unwrap().writes, 0);
        assert!(coordinator.pending_at.is_none());
    }

    #[test]
    fn sensitive_payload_is_rejected_before_unsupported_clipboard_write() {
        let mut coordinator = PasteCoordinator::<_, FakePaste>::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
                current: Vec::new(),
            }),
            None,
        );

        let error = coordinator
            .schedule(&flavors(), true, Instant::now())
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires OS history exclusion"));
        assert_eq!(coordinator.clipboard.as_ref().unwrap().writes, 0);
        assert!(coordinator.pending_at.is_none());
    }

    /// A normal copy asks for history exclusion as a preference and carries the
    /// self-write sentinel; the backend sees both in one options value.
    #[test]
    fn normal_copy_prefers_history_exclusion_and_tags_the_write() {
        let mut coordinator = PasteCoordinator::<_, FakePaste>::with_backends(
            Some(RecordingClipboard::default()),
            None,
        );

        coordinator.copy(&flavors(), false).unwrap();

        let requested = &coordinator.clipboard.as_ref().unwrap().requested;
        assert_eq!(requested.len(), 1);
        assert_eq!(
            requested[0].retention,
            ClipboardRetention::PreferExcludeFromSystemHistory
        );
        assert!(
            requested[0].lineage.write_nonce.is_some(),
            "the self-write nonce must travel with the write"
        );
    }

    #[test]
    fn sensitive_copy_requires_exclusion_and_clears_on_a_proving_backend() {
        let mut coordinator = PasteCoordinator::<_, FakePaste>::with_backends(
            Some(ExcludingClipboard::default()),
            None,
        );

        coordinator.copy(&flavors(), true).unwrap();

        let requested = &coordinator.clipboard.as_ref().unwrap().requested;
        assert_eq!(requested.len(), 1);
        assert_eq!(
            requested[0].retention,
            ClipboardRetention::RequireExcludeFromSystemHistory
        );
    }

    #[test]
    fn sensitive_copy_never_reaches_a_backend_that_cannot_exclude() {
        let mut coordinator = PasteCoordinator::<_, FakePaste>::with_backends(
            Some(RecordingClipboard::default()),
            None,
        );

        let error = coordinator.copy(&flavors(), true).unwrap_err().to_string();

        assert!(error.contains("requires OS history exclusion"));
        let clipboard = coordinator.clipboard.as_ref().unwrap();
        assert!(clipboard.requested.is_empty());
        assert!(clipboard.current.is_empty());
    }
}
