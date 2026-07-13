//! Clipboard write and delayed paste-back coordination.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow};
use vbuff_core::capture::SelfWriteLedger;
use vbuff_core::content_hash_from_flavors;
use vbuff_platform::{
    ArboardClipboard, ClipboardBackend, ClipboardRetention, ClipboardWriteReceipt, EnigoPaste,
    PasteBackend,
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
    self_writes: Arc<Mutex<SelfWriteLedger>>,
}

impl PasteCoordinator<ArboardClipboard, EnigoPaste> {
    pub(crate) fn system(self_writes: Arc<Mutex<SelfWriteLedger>>) -> Self {
        let clipboard = ArboardClipboard::new().map_err(|error| {
            tracing::warn!("clipboard writer unavailable: {error}");
            error
        });
        let paste = EnigoPaste::new().map_err(|error| {
            tracing::warn!("paste backend unavailable; selections will only be copied: {error}");
            error
        });

        Self::with_backends_and_ledger(clipboard.ok(), paste.ok(), self_writes)
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

    fn with_backends_and_ledger(
        clipboard: Option<C>,
        paste: Option<P>,
        self_writes: Arc<Mutex<SelfWriteLedger>>,
    ) -> Self {
        Self {
            clipboard,
            paste,
            pending_at: None,
            self_writes,
        }
    }

    /// Write a clip without injecting a paste keystroke.
    pub(crate) fn copy(&mut self, flavors: &[Flavor]) -> anyhow::Result<()> {
        // Any explicit clipboard write supersedes a pending paste. This also
        // guarantees that a failed replacement write cannot leave an older
        // keystroke armed against stale clipboard contents.
        self.pending_at = None;
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
        let receipt = self
            .clipboard
            .as_mut()
            .ok_or_else(|| anyhow!("clipboard writer unavailable"))?
            .write_tagged_with_retention(
                flavors,
                &lineage,
                ClipboardRetention::ExcludeFromSystemHistory,
            )
            .context("writing selected clip to clipboard")?;
        if receipt == ClipboardWriteReceipt::RetentionHintUnsupported {
            tracing::debug!("OS clipboard-history exclusion hint is unavailable");
        }
        ledger.register(hash, nonce, Instant::now());
        Ok(())
    }

    /// Write first, then schedule paste-back. A failed write never sends paste.
    pub(crate) fn schedule(
        &mut self,
        flavors: &[Flavor],
        now: Instant,
    ) -> anyhow::Result<PasteOutcome> {
        self.copy(flavors)?;

        if self.paste.is_some() {
            self.pending_at = Some(now + PASTE_DELAY);
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
    }

    impl ClipboardBackend for FakeClipboard {
        fn read(&mut self) -> PlatformResult<CapturedClipboard> {
            Ok(CapturedClipboard::default())
        }

        fn write(&mut self, _flavors: &[Flavor]) -> PlatformResult<()> {
            self.writes += 1;
            if self.fail {
                Err(PlatformError::Clipboard("test failure".into()))
            } else {
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
            }),
            Some(FakePaste::default()),
        );
        let now = Instant::now();

        coordinator.schedule(&flavors(), now).unwrap();
        coordinator.clipboard.as_mut().unwrap().fail = true;
        assert!(coordinator.schedule(&flavors(), now).is_err());
        assert_eq!(coordinator.wait_duration(now), None);
        assert!(coordinator.poll(now + PASTE_DELAY).is_none());
    }

    #[test]
    fn successful_write_fires_one_delayed_paste() {
        let mut coordinator = PasteCoordinator::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
            }),
            Some(FakePaste::default()),
        );
        let now = Instant::now();

        assert_eq!(
            coordinator.schedule(&flavors(), now).unwrap(),
            PasteOutcome::Scheduled
        );
        assert!(coordinator.poll(now + PASTE_DELAY / 2).is_none());
        assert!(coordinator.poll(now + PASTE_DELAY).unwrap().is_ok());
        assert!(coordinator.poll(now + PASTE_DELAY).is_none());
    }

    #[test]
    fn missing_paste_backend_degrades_to_copy_only() {
        let mut coordinator = PasteCoordinator::<_, FakePaste>::with_backends(
            Some(FakeClipboard {
                fail: false,
                writes: 0,
            }),
            None,
        );

        assert_eq!(
            coordinator.schedule(&flavors(), Instant::now()).unwrap(),
            PasteOutcome::CopiedOnly
        );
    }
}
