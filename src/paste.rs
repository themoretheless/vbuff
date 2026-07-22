//! Clipboard write and delayed paste-back coordination.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow};
use vbuff_core::capture::SelfWriteLedger;
use vbuff_core::content_hash_from_flavors;
use vbuff_core::intelligence::{PasteGuardDecision, PasteGuardFingerprint};
use vbuff_platform::{
    ArboardClipboard, ClipboardBackend, ClipboardRetention, ClipboardWriteReceipt, EnigoPaste,
    PasteBackend, PastePermissionSelfCheck,
};
use vbuff_types::{CaptureLineage, ClipId, Flavor};

const PASTE_DELAY: Duration = Duration::from_millis(120);

/// Result of selecting a clip when paste injection is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteOutcome {
    Scheduled,
    CopiedOnly,
}

/// Owns the reusable clipboard writer and paste backend.
pub(crate) struct PasteCoordinator<C = ArboardClipboard, P = EnigoPaste> {
    clipboard: Option<C>,
    paste: Option<P>,
    pending_at: Option<Instant>,
    pending_guard: Option<PasteGuardFingerprint>,
    self_writes: Arc<Mutex<SelfWriteLedger>>,
    permission_check: PastePermissionSelfCheck,
}

impl PasteCoordinator<ArboardClipboard, EnigoPaste> {
    pub(crate) fn system(self_writes: Arc<Mutex<SelfWriteLedger>>) -> Self {
        let session = vbuff_platform::lifecycle::SessionContext::detect();
        let clipboard = ArboardClipboard::new().map_err(|error| {
            tracing::warn!("clipboard writer unavailable: {error}");
            error
        });
        // Enigo can emit a shortcut, but it cannot prove that focus returned to
        // the window which was active before vbuff opened. Until a native
        // target-confirmation adapter is wired, automatic injection fails
        // closed and selection remains a reliable copy-only workflow.
        let target_confirmation_available = false;
        let automatic_paste_allowed = target_confirmation_available
            && session.input_injection_allowed
            && session.display_server != vbuff_platform::lifecycle::DisplayServer::Wayland;
        let paste = automatic_paste_allowed
            .then(|| {
                EnigoPaste::new().map_err(|error| {
                    tracing::warn!(
                        "paste backend unavailable; selections will only be copied: {error}"
                    );
                    error
                })
            })
            .transpose()
            .ok()
            .flatten();
        if !automatic_paste_allowed {
            tracing::info!(
                display_server = ?session.display_server,
                remote = session.remote,
                "automatic paste disabled because the destination window cannot be confirmed"
            );
        }

        Self::with_backends_and_ledger_for_session(clipboard.ok(), paste, self_writes, &session)
    }
}

impl<C: ClipboardBackend, P: PasteBackend> PasteCoordinator<C, P> {
    #[cfg(test)]
    fn with_backends(clipboard: Option<C>, paste: Option<P>) -> Self {
        Self::with_backends_and_ledger(
            clipboard,
            paste,
            Arc::new(Mutex::new(SelfWriteLedger::default())),
        )
    }

    #[cfg(test)]
    fn with_backends_and_ledger(
        clipboard: Option<C>,
        paste: Option<P>,
        self_writes: Arc<Mutex<SelfWriteLedger>>,
    ) -> Self {
        let session = vbuff_platform::lifecycle::SessionContext::detect();
        Self::with_backends_and_ledger_for_session(clipboard, paste, self_writes, &session)
    }

    fn with_backends_and_ledger_for_session(
        clipboard: Option<C>,
        paste: Option<P>,
        self_writes: Arc<Mutex<SelfWriteLedger>>,
        session: &vbuff_platform::lifecycle::SessionContext,
    ) -> Self {
        let permission_check = PastePermissionSelfCheck::evaluate_session(session, paste.is_some());
        Self {
            clipboard,
            paste,
            pending_at: None,
            pending_guard: None,
            self_writes,
            permission_check,
        }
    }

    pub(crate) const fn permission_check(&self) -> PastePermissionSelfCheck {
        self.permission_check
    }

    /// Write a clip without injecting a paste keystroke.
    pub(crate) fn copy(&mut self, flavors: &[Flavor], sensitive: bool) -> anyhow::Result<()> {
        // Any explicit clipboard write supersedes a pending paste. This also
        // guarantees that a failed replacement write cannot leave an older
        // keystroke armed against stale clipboard contents.
        self.pending_at = None;
        self.pending_guard = None;
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
        let receipt = if sensitive {
            clipboard
                .write_sensitive_excluding_history(flavors, &lineage)
                .context("writing sensitive clip with OS history exclusion")?
        } else {
            clipboard
                .write_tagged_with_retention(
                    flavors,
                    &lineage,
                    ClipboardRetention::ExcludeFromSystemHistory,
                )
                .context("writing selected clip to clipboard")?
        };
        if receipt == ClipboardWriteReceipt::RetentionHintUnsupported {
            if sensitive {
                return Err(anyhow!(
                    "sensitive clipboard write requires OS history exclusion"
                ));
            }
            tracing::debug!("OS clipboard-history exclusion hint is unavailable");
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
        let guard = self
            .paste
            .is_some()
            .then(|| {
                PasteGuardFingerprint::from_flavors(flavors)
                    .ok_or_else(|| anyhow!("selected clip cannot be verified before paste"))
            })
            .transpose()?;
        let write_started_at = Instant::now();
        self.copy(flavors, sensitive)?;
        let write_duration = Instant::now().saturating_duration_since(write_started_at);

        if let Some(guard) = guard {
            self.pending_guard = Some(guard);
            self.pending_at = Some(now + write_duration + PASTE_DELAY);
            Ok(PasteOutcome::Scheduled)
        } else {
            self.pending_at = None;
            Ok(PasteOutcome::CopiedOnly)
        }
    }

    /// Fire a due paste exactly once.
    pub(crate) fn poll(&mut self, now: Instant) -> Option<anyhow::Result<()>> {
        let due = self.pending_at?;
        if now < due {
            return None;
        }

        self.pending_at = None;
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
            return Some(Err(anyhow!(
                "paste guard blocked changed clipboard: {decision:?}"
            )));
        }
        Some(
            self.paste
                .as_mut()
                .ok_or_else(|| anyhow!("paste backend unavailable"))
                .and_then(|backend| backend.paste().context("injecting paste keystroke")),
        )
    }

    pub(crate) fn wait_duration(&self, now: Instant) -> Option<Duration> {
        self.pending_at
            .map(|due| due.saturating_duration_since(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_platform::{CapturedClipboard, PlatformError, Result as PlatformResult};

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

        fn write(&mut self, flavors: &[Flavor]) -> PlatformResult<()> {
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

    #[derive(Default)]
    struct FakePaste {
        calls: usize,
    }

    impl PasteBackend for FakePaste {
        fn paste(&mut self) -> PlatformResult<()> {
            self.calls += 1;
            Ok(())
        }
    }

    fn flavors() -> Vec<Flavor> {
        vec![Flavor::inline("text/plain", b"hello".to_vec())]
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

        coordinator.schedule(&flavors(), false, now).unwrap();
        coordinator.clipboard.as_mut().unwrap().fail = true;
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

        assert_eq!(
            coordinator.schedule(&flavors(), false, now).unwrap(),
            PasteOutcome::Scheduled
        );
        let due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(due - PASTE_DELAY / 2).is_none());
        assert!(coordinator.poll(due).unwrap().is_ok());
        assert!(coordinator.poll(due).is_none());
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
        coordinator.schedule(&flavors(), false, now).unwrap();
        coordinator.clipboard.as_mut().unwrap().current = vec![Flavor::inline(
            "text/plain",
            b"0x2222222222222222222222222222222222222222".to_vec(),
        )];

        let due = coordinator.pending_at.unwrap();
        assert!(coordinator.poll(due).unwrap().is_err());
        assert_eq!(coordinator.paste.as_ref().unwrap().calls, 0);
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
}
