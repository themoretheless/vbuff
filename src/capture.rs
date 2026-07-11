//! Clipboard capture supervision and capture-policy evaluation.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vbuff_core::{content_hash_from_flavors, detect_kind};
use vbuff_platform::{ArboardClipboard, CapturedClipboard, ClipboardBackend};
use vbuff_types::{CaptureHealth, Clip, ClipId, ClipMeta};

use crate::config::Config;
use crate::diagnostics::Diagnostics;
use crate::history::History;

const MIN_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(5);

/// The pure, inexpensive rules that run before a clip reaches storage.
#[derive(Clone, Debug)]
struct CapturePolicy {
    skip_whitespace_only: bool,
    excluded_apps: Vec<String>,
}

impl From<&Config> for CapturePolicy {
    fn from(config: &Config) -> Self {
        Self {
            skip_whitespace_only: config.skip_whitespace_only,
            excluded_apps: config
                .excluded_apps
                .iter()
                .filter(|app| !app.is_empty())
                .map(|app| app.to_lowercase())
                .collect(),
        }
    }
}

impl CapturePolicy {
    fn accepts(&self, captured: &CapturedClipboard) -> bool {
        if self.skip_whitespace_only
            && captured
                .flavors
                .iter()
                .find_map(|flavor| flavor.as_text())
                .is_some_and(|text| text.trim().is_empty())
        {
            return false;
        }

        if captured.source_app.as_ref().is_some_and(|source| {
            let source = source.to_lowercase();
            self.excluded_apps
                .iter()
                .any(|excluded| source.contains(excluded))
        }) {
            return false;
        }

        true
    }
}

/// Keeps storage failures dominant until a later write proves recovery.
#[derive(Default)]
struct CaptureHealthState {
    storage_degraded: bool,
}

impl CaptureHealthState {
    fn read_succeeded(&self) -> Option<CaptureHealth> {
        (!self.storage_degraded).then_some(CaptureHealth::Watching)
    }

    fn read_failed(&self) -> Option<CaptureHealth> {
        (!self.storage_degraded).then_some(CaptureHealth::ClipboardReadError)
    }

    fn store_succeeded(&mut self) -> CaptureHealth {
        self.storage_degraded = false;
        CaptureHealth::Watching
    }

    fn store_failed(&mut self) -> CaptureHealth {
        self.storage_degraded = true;
        CaptureHealth::StorageError
    }
}

/// Monotonic heartbeat shared only by the worker and its watchdog.
struct Heartbeat {
    last: Mutex<Instant>,
}

impl Heartbeat {
    fn new(now: Instant) -> Self {
        Self {
            last: Mutex::new(now),
        }
    }

    fn beat(&self) {
        if let Ok(mut last) = self.last.lock() {
            *last = Instant::now();
        }
    }

    fn is_stale(&self, now: Instant, timeout: Duration) -> bool {
        self.last
            .lock()
            .map_or(true, |last| now.saturating_duration_since(*last) >= timeout)
    }
}

/// Start the one background capture worker used by the single-process MVP.
pub(crate) fn spawn(
    history: History,
    diagnostics: Diagnostics,
    paused: Arc<AtomicBool>,
    config: Config,
) -> std::thread::JoinHandle<()> {
    let heartbeat = Arc::new(Heartbeat::new(Instant::now()));
    let running = Arc::new(AtomicBool::new(true));
    let timeout = watchdog_timeout(&config);
    spawn_watchdog(
        Arc::clone(&heartbeat),
        Arc::clone(&running),
        Arc::clone(&paused),
        diagnostics.clone(),
        timeout,
    );

    std::thread::spawn(move || {
        let panic_diagnostics = diagnostics.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            run_worker(history, diagnostics, paused, config, heartbeat)
        }));
        running.store(false, Ordering::Release);
        if result.is_err() {
            panic_diagnostics.capture_health(CaptureHealth::Stalled);
            tracing::error!("capture worker panicked; watchdog marked it stalled");
        }
    })
}

fn run_worker(
    history: History,
    diagnostics: Diagnostics,
    paused: Arc<AtomicBool>,
    config: Config,
    heartbeat: Arc<Heartbeat>,
) {
    heartbeat.beat();
    let mut clipboard = match ArboardClipboard::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            diagnostics.capture_health(CaptureHealth::ClipboardUnavailable);
            tracing::error!("clipboard backend unavailable: {error}");
            return;
        }
    };

    diagnostics.capture_health(CaptureHealth::Watching);
    let policy = CapturePolicy::from(&config);
    let interval = Duration::from_millis(config.poll_interval_ms.max(50));
    let mut last_hash: Option<[u8; 32]> = None;
    let mut health_state = CaptureHealthState::default();

    loop {
        heartbeat.beat();
        std::thread::sleep(interval);
        heartbeat.beat();

        if paused.load(Ordering::Relaxed) {
            continue;
        }

        let captured = match clipboard.read() {
            Ok(captured) => {
                if let Some(health) = health_state.read_succeeded() {
                    diagnostics.capture_health(health);
                }
                if captured.is_empty() {
                    continue;
                }
                captured
            }
            Err(error) => {
                if let Some(health) = health_state.read_failed()
                    && diagnostics.capture_health(health)
                {
                    tracing::warn!("clipboard read failed; retrying: {error}");
                }
                continue;
            }
        };
        let hash = content_hash_from_flavors(&captured.flavors);
        if last_hash == Some(hash) {
            continue;
        }

        if !policy.accepts(&captured) {
            last_hash = Some(hash);
            continue;
        }

        let clip = build_clip(captured, hash);
        match history.insert(&clip, config.max_history) {
            Ok(()) => {
                last_hash = Some(hash);
                diagnostics.capture_health(health_state.store_succeeded());
            }
            Err(error) => {
                // Keep the previous hash so the same clipboard is retried on
                // the next poll instead of being silently lost.
                if diagnostics.capture_health(health_state.store_failed()) {
                    tracing::warn!("capture insert failed; retrying: {error}");
                }
            }
        }
    }
}

fn watchdog_timeout(config: &Config) -> Duration {
    let poll_budget = Duration::from_millis(config.poll_interval_ms.max(50).saturating_mul(8));
    poll_budget.max(MIN_WATCHDOG_TIMEOUT)
}

fn spawn_watchdog(
    heartbeat: Arc<Heartbeat>,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    diagnostics: Diagnostics,
    timeout: Duration,
) {
    std::thread::spawn(move || {
        let cadence = (timeout / 2).min(Duration::from_secs(1));
        while running.load(Ordering::Acquire) {
            std::thread::sleep(cadence);
            if !running.load(Ordering::Acquire) {
                break;
            }
            if let Some(health) = watchdog_health(
                &heartbeat,
                paused.load(Ordering::Relaxed),
                Instant::now(),
                timeout,
            ) && diagnostics.capture_health(health)
            {
                tracing::error!(?timeout, "capture heartbeat stalled");
            }
        }
    });
}

fn watchdog_health(
    heartbeat: &Heartbeat,
    paused: bool,
    now: Instant,
    timeout: Duration,
) -> Option<CaptureHealth> {
    (!paused && heartbeat.is_stale(now, timeout)).then_some(CaptureHealth::Stalled)
}

fn build_clip(captured: CapturedClipboard, content_hash: [u8; 32]) -> Clip {
    let kind = detect_kind(&captured.flavors);
    let byte_size = captured
        .flavors
        .iter()
        .map(|flavor| flavor.body.byte_size())
        .sum();

    Clip {
        id: ClipId::new(),
        flavors: captured.flavors,
        content_hash,
        meta: ClipMeta::now(kind, byte_size, captured.source_app),
        pinned: false,
        favorite: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::Flavor;

    fn captured(text: &str, source_app: Option<&str>) -> CapturedClipboard {
        CapturedClipboard {
            flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
            source_app: source_app.map(str::to_owned),
        }
    }

    #[test]
    fn policy_rejects_whitespace_and_excluded_apps() {
        let config = Config {
            excluded_apps: vec!["onepassword".into()],
            ..Default::default()
        };
        let policy = CapturePolicy::from(&config);

        assert!(!policy.accepts(&captured("  \n", None)));
        assert!(!policy.accepts(&captured("secret", Some("com.AgileBits.OnePassword7"))));
        assert!(policy.accepts(&captured("hello", Some("com.apple.Safari"))));
    }

    #[test]
    fn build_clip_preserves_source_and_byte_count() {
        let captured = captured("hello", Some("editor.app"));
        let hash = content_hash_from_flavors(&captured.flavors);
        let clip = build_clip(captured, hash);

        assert_eq!(clip.meta.byte_size, 5);
        assert_eq!(clip.meta.source_app.as_deref(), Some("editor.app"));
        assert_eq!(clip.content_hash, hash);
    }

    #[test]
    fn storage_failure_stays_visible_until_a_successful_write() {
        let mut state = CaptureHealthState::default();

        assert_eq!(state.store_failed(), CaptureHealth::StorageError);
        assert_eq!(state.read_succeeded(), None);
        assert_eq!(state.read_failed(), None);
        assert_eq!(state.store_succeeded(), CaptureHealth::Watching);
        assert_eq!(state.read_succeeded(), Some(CaptureHealth::Watching));
    }

    #[test]
    fn heartbeat_becomes_stale_at_the_timeout() {
        let started_at = Instant::now();
        let heartbeat = Heartbeat::new(started_at);
        let timeout = Duration::from_secs(5);

        assert!(!heartbeat.is_stale(started_at + timeout / 2, timeout));
        assert!(heartbeat.is_stale(started_at + timeout, timeout));
        assert_eq!(
            watchdog_health(&heartbeat, false, started_at + timeout, timeout),
            Some(CaptureHealth::Stalled)
        );
        assert_eq!(
            watchdog_health(&heartbeat, true, started_at + timeout, timeout),
            None
        );
    }

    #[test]
    fn watchdog_budget_has_a_safe_floor_and_scales_with_polling() {
        let fast = Config {
            poll_interval_ms: 50,
            ..Default::default()
        };
        let slow = Config {
            poll_interval_ms: 1_000,
            ..Default::default()
        };

        assert_eq!(watchdog_timeout(&fast), Duration::from_secs(5));
        assert_eq!(watchdog_timeout(&slow), Duration::from_secs(8));
    }
}
