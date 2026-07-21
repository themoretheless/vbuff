//! Clipboard capture supervision and capture-policy evaluation.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vbuff_core::capture::{
    AdaptivePollScheduler, CaptureAction, CaptureDecision, CaptureInput, CaptureLossLedger,
    CaptureOutcome, CapturePolicy, CaptureRule, DropClass, DropReason, GenerationObservation,
    GenerationTracker, PollObservation, SelectionSource, SelfTestObservation, SelfTestState,
    SelfWriteLedger, SkippedCapture, SkippedCaptureRing, SourcePredicate, SubsystemBudget,
    annotate_integrity, prune_redundant_flavors, verify_integrity,
};
use vbuff_core::observability::RedactedClipFields;
use vbuff_core::reliability::{
    Admission, ByteBackpressure, CaptureForensicEvent, CaptureForensicRing, CaptureSupervisor,
    RecoveryAction, SupervisorObservation, shed_to_text_preview,
};
use vbuff_core::{content_hash_from_flavors, detect_kind};
use vbuff_platform::{
    ArboardClipboard, CapturedClipboard, ClipboardBackend, ClipboardRetention, ClipboardSelection,
};
use vbuff_types::{CaptureHealth, Clip, ClipId, ClipMeta};

use crate::config::{Config, SourceRuleAction};
use crate::diagnostics::Diagnostics;
use crate::history::History;

const MIN_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(5);
const SKIP_RECOVERY_WINDOW_MS: u64 = 30_000;
const MAX_CONSECUTIVE_READ_FAILURES: u8 = 5;
const SHED_RETRY_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerExit {
    BackendUnavailable,
    RepeatedReadFailure,
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
    self_writes: Arc<Mutex<SelfWriteLedger>>,
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
        let supervisor = Arc::new(Mutex::new(CaptureSupervisor::default()));
        loop {
            let result = catch_unwind(AssertUnwindSafe({
                let history = history.clone();
                let diagnostics = diagnostics.clone();
                let paused = Arc::clone(&paused);
                let config = config.clone();
                let heartbeat = Arc::clone(&heartbeat);
                let self_writes = Arc::clone(&self_writes);
                let supervisor = Arc::clone(&supervisor);
                move || {
                    run_worker(
                        history,
                        diagnostics,
                        paused,
                        config,
                        heartbeat,
                        self_writes,
                        supervisor,
                    )
                }
            }));
            let observation = match result {
                Ok(WorkerExit::BackendUnavailable | WorkerExit::RepeatedReadFailure) => {
                    SupervisorObservation::SubscriptionLost
                }
                Err(_) => {
                    tracing::error!("capture worker panicked; supervisor will restart it");
                    SupervisorObservation::BackendFailed
                }
            };
            let action = supervisor
                .lock()
                .map(|mut supervisor| supervisor.observe(observation))
                .unwrap_or(RecoveryAction::EnterDegradedMode {
                    retry_after: Duration::from_secs(30),
                });
            let delay = match action {
                RecoveryAction::None => Duration::ZERO,
                RecoveryAction::Resubscribe => {
                    diagnostics.capture_health(CaptureHealth::ClipboardReadError);
                    Duration::from_millis(250)
                }
                RecoveryAction::RestartBackend => {
                    diagnostics.capture_health(CaptureHealth::Stalled);
                    Duration::from_secs(1)
                }
                RecoveryAction::EnterDegradedMode { retry_after } => {
                    diagnostics.capture_health(CaptureHealth::ClipboardUnavailable);
                    retry_after
                }
            };
            tracing::warn!(?action, ?delay, "capture backend recovery scheduled");
            sleep_with_heartbeat(delay, &heartbeat);
        }
    })
}

fn run_worker(
    history: History,
    diagnostics: Diagnostics,
    paused: Arc<AtomicBool>,
    config: Config,
    heartbeat: Arc<Heartbeat>,
    self_writes: Arc<Mutex<SelfWriteLedger>>,
    supervisor: Arc<Mutex<CaptureSupervisor>>,
) -> WorkerExit {
    heartbeat.beat();
    let mut clipboard = match ArboardClipboard::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            diagnostics.capture_health(CaptureHealth::ClipboardUnavailable);
            tracing::error!("clipboard backend unavailable: {error}");
            return WorkerExit::BackendUnavailable;
        }
    };

    if capture_self_test(&mut clipboard, &self_writes) {
        diagnostics.capture_health(CaptureHealth::Watching);
    } else {
        diagnostics.capture_health(CaptureHealth::SelfTestFailed);
        tracing::warn!("clipboard capture self-test failed; continuing in degraded mode");
    }
    let policy = capture_policy(&config);
    let initial_interval = Duration::from_millis(config.poll_interval_ms.max(50));
    let mut scheduler = AdaptivePollScheduler::new(
        Duration::from_millis(config.poll_interval_ms.clamp(50, 120)),
        initial_interval,
        Duration::from_millis(config.poll_interval_ms.max(900)),
    );
    diagnostics.poll_interval(scheduler.interval());
    let mut last_hash: Option<[u8; 32]> = None;
    let mut health_state = CaptureHealthState::default();
    let mut generation_tracker = GenerationTracker::default();
    let mut loss_ledger = CaptureLossLedger::default();
    let mut skipped = SkippedCaptureRing::new(8);
    let mut forensic = CaptureForensicRing::new(64);
    let mut backpressure = ByteBackpressure::new(
        config.capture_soft_limit_bytes,
        config.capture_hard_limit_bytes,
        1,
        config.capture_preview_bytes,
    );
    let mut consecutive_read_failures = 0_u8;
    let mut last_shed: Option<([u8; 32], Instant)> = None;
    let budget = Arc::new(Mutex::new(SubsystemBudget::new(
        Duration::from_secs(60),
        Duration::from_millis(120),
        600,
    )));
    let budget_tripped = Arc::new(AtomicBool::new(false));
    let request_backoff = Arc::new(AtomicBool::new(false));

    loop {
        if request_backoff.swap(false, Ordering::AcqRel) {
            diagnostics.poll_interval(scheduler.back_off());
        }
        heartbeat.beat();
        std::thread::sleep(scheduler.interval());
        heartbeat.beat();
        let _budget_guard = CaptureBudgetGuard::new(
            Arc::clone(&budget),
            Arc::clone(&budget_tripped),
            Arc::clone(&request_backoff),
            diagnostics.clone(),
        );

        if paused.load(Ordering::Relaxed) {
            observe_scheduler(&mut scheduler, &diagnostics, PollObservation::Stable);
            continue;
        }

        let mut captured = match clipboard.read() {
            Ok(captured) => {
                consecutive_read_failures = 0;
                if let Ok(mut supervisor) = supervisor.lock() {
                    supervisor.observe(SupervisorObservation::RecoverySucceeded);
                }
                if let Some(health) = health_state.read_succeeded() {
                    diagnostics.capture_health(health);
                }
                if captured.is_empty() {
                    observe_scheduler(&mut scheduler, &diagnostics, PollObservation::Stable);
                    continue;
                }
                captured
            }
            Err(error) => {
                consecutive_read_failures = consecutive_read_failures.saturating_add(1);
                if let Some(health) = health_state.read_failed()
                    && diagnostics.capture_health(health)
                {
                    tracing::warn!("clipboard read failed; retrying: {error}");
                }
                observe_scheduler(&mut scheduler, &diagnostics, PollObservation::MissRisk);
                if consecutive_read_failures >= MAX_CONSECUTIVE_READ_FAILURES {
                    tracing::warn!(
                        consecutive_read_failures,
                        "capture backend exceeded read-failure threshold"
                    );
                    return WorkerExit::RepeatedReadFailure;
                }
                continue;
            }
        };
        forensic.push(CaptureForensicEvent {
            observed_at: Instant::now(),
            generation: captured.generation.map(|generation| generation.sequence),
            flavor_count: captured.flavors.len().min(usize::from(u16::MAX)) as u16,
            total_bytes: captured
                .flavors
                .iter()
                .map(|flavor| flavor.body.byte_size())
                .fold(0_u64, u64::saturating_add),
            owner_changed: !captured.coherent_generation,
            coherent: captured.coherent_generation,
        });
        annotate_integrity(&mut captured.flavors);
        if !verify_integrity(&captured.flavors).is_empty() {
            if let Some(event) = forensic.entries().last() {
                tracing::warn!(
                    generation = event.generation,
                    flavor_count = event.flavor_count,
                    total_bytes = event.total_bytes,
                    owner_changed = event.owner_changed,
                    "content-free torn-read evidence retained"
                );
            }
            record_drop(
                &history,
                &diagnostics,
                &mut loss_ledger,
                DropReason::TornRead,
                1,
            );
            observe_scheduler(&mut scheduler, &diagnostics, PollObservation::MissRisk);
            continue;
        }

        let observed_hash = content_hash_from_flavors(&captured.flavors);
        let observed_at = Instant::now();
        let recovery_requested = diagnostics.take_skipped_recovery();
        let explicit_recovery = recovery_requested
            && skipped.latest().is_some_and(|entry| {
                entry.reason.is_recoverable()
                    && observed_at.saturating_duration_since(entry.observed_at)
                        <= Duration::from_millis(SKIP_RECOVERY_WINDOW_MS)
                    && entry.provenance == captured.provenance
                    && entry.content_hash == observed_hash
                    && match (entry.generation, captured.generation) {
                        (Some(expected), Some(actual)) => expected == actual,
                        (None, None) => true,
                        _ => false,
                    }
            });

        if last_hash == Some(observed_hash) && !explicit_recovery {
            observe_scheduler(&mut scheduler, &diagnostics, PollObservation::Stable);
            continue;
        }
        if !explicit_recovery && shed_retry_suppressed(&mut last_shed, observed_hash, observed_at) {
            observe_scheduler(&mut scheduler, &diagnostics, PollObservation::Stable);
            continue;
        }
        observe_scheduler(
            &mut scheduler,
            &diagnostics,
            PollObservation::ClipboardChanged,
        );

        if !explicit_recovery && let Some(generation) = captured.generation {
            match generation_tracker.observe(generation) {
                GenerationObservation::Gap { missed } => record_drop(
                    &history,
                    &diagnostics,
                    &mut loss_ledger,
                    DropReason::GenerationGap,
                    missed,
                ),
                GenerationObservation::Stale => {
                    record_drop(
                        &history,
                        &diagnostics,
                        &mut loss_ledger,
                        DropReason::GenerationStale,
                        1,
                    );
                    continue;
                }
                GenerationObservation::First
                | GenerationObservation::Consecutive
                | GenerationObservation::EpochChanged => {}
            }
        }

        let self_write = self_writes
            .lock()
            .map(|mut ledger| ledger.matches(observed_hash, &captured.lineage, Instant::now()))
            .unwrap_or_else(|_| {
                tracing::error!("self-write ledger mutex poisoned");
                false
            });
        let policy_input = CaptureInput {
            flavors: &captured.flavors,
            provenance: &captured.provenance,
            source: match captured.selection {
                ClipboardSelection::Clipboard => SelectionSource::Clipboard,
                ClipboardSelection::Primary => SelectionSource::Primary,
            },
            primary_intended: captured.primary_intended,
            coherent_generation: captured.coherent_generation,
            concealed: captured.concealed,
            self_write,
        };
        let decision = if explicit_recovery
            && !self_write
            && captured.coherent_generation
            && captured
                .flavors
                .iter()
                .any(vbuff_types::Flavor::is_realized)
        {
            CaptureDecision::Capture {
                action: CaptureAction::Capture,
                sensitive: true,
                sync_eligible: false,
                ai_allowed: false,
                expires_after: None,
            }
        } else {
            policy.decide(policy_input)
        };

        let CaptureDecision::Capture {
            action,
            sensitive,
            sync_eligible,
            ai_allowed,
            expires_after,
        } = decision
        else {
            let CaptureDecision::Skip(reason) = decision else {
                unreachable!()
            };
            skipped.push(SkippedCapture {
                observed_at,
                reason,
                provenance: captured.provenance.clone(),
                generation: captured.generation,
                content_hash: observed_hash,
            });
            if reason.is_recoverable() {
                diagnostics.offer_skipped_recovery(Duration::from_millis(SKIP_RECOVERY_WINDOW_MS));
            } else {
                diagnostics.clear_skipped_recovery();
            }
            record_drop(&history, &diagnostics, &mut loss_ledger, reason, 1);
            if reason.class() == DropClass::Intentional {
                last_hash = Some(observed_hash);
            } else {
                observe_scheduler(&mut scheduler, &diagnostics, PollObservation::MissRisk);
            }
            continue;
        };

        apply_action(&mut captured.flavors, action);
        prune_redundant_flavors(&mut captured.flavors);
        if !captured
            .flavors
            .iter()
            .any(vbuff_types::Flavor::is_realized)
        {
            record_drop(
                &history,
                &diagnostics,
                &mut loss_ledger,
                DropReason::NoRealizedFlavor,
                1,
            );
            continue;
        }

        let memory_response = crate::memory_pressure::response(&config);
        let pressure_hard_limit = memory_response
            .reject_large_capture_bytes
            .unwrap_or(config.capture_hard_limit_bytes)
            .min(config.capture_hard_limit_bytes);
        let pressure_soft_limit = if memory_response.defer_background_work {
            config
                .capture_soft_limit_bytes
                .min(pressure_hard_limit / 2)
                .max(1)
        } else {
            config.capture_soft_limit_bytes
        };
        let _ = backpressure.update_limits(
            pressure_soft_limit,
            pressure_hard_limit,
            config.capture_preview_bytes,
        );
        let payload_bytes = captured
            .flavors
            .iter()
            .map(|flavor| flavor.body.byte_size())
            .fold(0_u64, u64::saturating_add);
        let payload_bytes = usize::try_from(payload_bytes).unwrap_or(usize::MAX);
        let admission = backpressure.admit(payload_bytes);
        let accounted_bytes = match admission {
            Admission::Full => payload_bytes,
            Admission::Preview { max_bytes } => {
                let Some(preview) = shed_to_text_preview(&captured.flavors, max_bytes) else {
                    backpressure.release(max_bytes);
                    last_shed = Some((observed_hash, observed_at));
                    record_drop(
                        &history,
                        &diagnostics,
                        &mut loss_ledger,
                        DropReason::Backpressure,
                        1,
                    );
                    continue;
                };
                captured.flavors = preview;
                diagnostics.capture_budget_alert(vbuff_types::CaptureBudgetAlert::PreviewOnly);
                record_drop(
                    &history,
                    &diagnostics,
                    &mut loss_ledger,
                    DropReason::TruncatedFlavor,
                    1,
                );
                max_bytes
            }
            Admission::Shed => {
                last_shed = Some((observed_hash, observed_at));
                diagnostics.capture_budget_alert(vbuff_types::CaptureBudgetAlert::Skipped);
                record_drop(
                    &history,
                    &diagnostics,
                    &mut loss_ledger,
                    DropReason::Backpressure,
                    1,
                );
                continue;
            }
        };

        let stored_hash = content_hash_from_flavors(&captured.flavors);
        let clip = build_clip(
            captured,
            stored_hash,
            sensitive,
            sync_eligible,
            ai_allowed,
            expires_after,
        );
        let fields = RedactedClipFields::from(&clip);
        let span = tracing::info_span!(
            "capture_commit",
            clip_id = %fields.clip_id,
            byte_size = fields.byte_size,
            kind = ?fields.kind,
            source_app = fields.source_app.unwrap_or("[redacted]"),
            sensitive = fields.sensitive,
        );
        let _entered = span.enter();
        diagnostics.write_queue_depth(1);
        let insert_started = Instant::now();
        let insert_result = history.insert(&clip, config.max_history);
        diagnostics.latency("capture_insert", insert_started.elapsed());
        diagnostics.write_queue_depth(0);
        match insert_result {
            Ok(()) => {
                last_shed = None;
                diagnostics.clear_skipped_recovery();
                last_hash = Some(observed_hash);
                loss_ledger.captured();
                diagnostics.capture_outcome(CaptureOutcome::Captured, 1);
                if let Err(error) = history.record_capture_outcome(CaptureOutcome::Captured, 1) {
                    tracing::warn!("capture accounting write failed: {error}");
                }
                diagnostics.capture_health(health_state.store_succeeded());
            }
            Err(error) => {
                // Keep the previous hash so the same clipboard is retried on
                // the next poll instead of being silently lost.
                if diagnostics.capture_health(health_state.store_failed()) {
                    tracing::warn!("capture insert failed; retrying: {error}");
                }
                record_drop(
                    &history,
                    &diagnostics,
                    &mut loss_ledger,
                    DropReason::StoreFailure,
                    1,
                );
            }
        }
        backpressure.release(accounted_bytes);
    }
}

fn shed_retry_suppressed(
    last_shed: &mut Option<([u8; 32], Instant)>,
    observed_hash: [u8; 32],
    now: Instant,
) -> bool {
    let suppressed = last_shed.is_some_and(|(hash, shed_at)| {
        hash == observed_hash && now.saturating_duration_since(shed_at) < SHED_RETRY_INTERVAL
    });
    if !suppressed {
        *last_shed = None;
    }
    suppressed
}

struct CaptureBudgetGuard {
    started: cpu_time::ThreadTime,
    budget: Arc<Mutex<SubsystemBudget>>,
    tripped: Arc<AtomicBool>,
    request_backoff: Arc<AtomicBool>,
    diagnostics: Diagnostics,
}

impl CaptureBudgetGuard {
    fn new(
        budget: Arc<Mutex<SubsystemBudget>>,
        tripped: Arc<AtomicBool>,
        request_backoff: Arc<AtomicBool>,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            started: cpu_time::ThreadTime::now(),
            budget,
            tripped,
            request_backoff,
            diagnostics,
        }
    }
}

impl Drop for CaptureBudgetGuard {
    fn drop(&mut self) {
        let observation = self
            .budget
            .lock()
            .map(|mut budget| budget.record(Instant::now(), self.started.elapsed(), 1));
        let Ok(observation) = observation else {
            return;
        };
        let exceeded = observation != vbuff_core::capture::BudgetObservation::WithinBudget;
        if exceeded {
            self.request_backoff.store(true, Ordering::Release);
            if !self.tripped.swap(true, Ordering::AcqRel) {
                self.diagnostics.budget_trip();
                tracing::warn!(
                    ?observation,
                    "capture subsystem exceeded its rolling budget"
                );
            }
        } else {
            self.tripped.store(false, Ordering::Release);
        }
    }
}

fn observe_scheduler(
    scheduler: &mut AdaptivePollScheduler,
    diagnostics: &Diagnostics,
    observation: PollObservation,
) {
    let interval = scheduler.observe(observation, Instant::now());
    diagnostics.poll_interval(interval);
}

fn capture_self_test(
    clipboard: &mut impl ClipboardBackend,
    self_writes: &Arc<Mutex<SelfWriteLedger>>,
) -> bool {
    let Ok(original) = clipboard.read() else {
        return false;
    };
    let nonce = ClipId::new().to_string_repr();
    let probe = vec![vbuff_types::Flavor::inline(
        "text/plain;charset=utf-8",
        format!("vbuff-self-test-{nonce}").into_bytes(),
    )];
    let probe_hash = content_hash_from_flavors(&probe);
    let lineage = vbuff_types::CaptureLineage {
        origin_device: None,
        write_nonce: Some(nonce.clone()),
    };
    let mut state = SelfTestState::start(probe_hash);

    let probe_ok = clipboard
        .write_tagged_with_retention(
            &probe,
            &lineage,
            ClipboardRetention::ExcludeFromSystemHistory,
        )
        .is_ok()
        && clipboard.read().is_ok_and(|observed| {
            let observed_hash = content_hash_from_flavors(&observed.flavors);
            let suppressed = self_writes
                .lock()
                .map(|mut ledger| {
                    ledger.register(probe_hash, nonce, Instant::now());
                    ledger.matches(observed_hash, &observed.lineage, Instant::now())
                })
                .unwrap_or(false);
            observed_hash == probe_hash && suppressed
        });
    state = state.observe(if probe_ok {
        SelfTestObservation::EchoConfirmed
    } else {
        SelfTestObservation::UnexpectedEcho
    });

    let restored = if original.is_empty() {
        clipboard.clear().is_ok()
    } else {
        let restore_nonce = ClipId::new().to_string_repr();
        let restore_lineage = vbuff_types::CaptureLineage {
            origin_device: None,
            write_nonce: Some(restore_nonce.clone()),
        };
        let restored = clipboard
            .write_tagged_with_retention(
                &original.flavors,
                &restore_lineage,
                ClipboardRetention::ExcludeFromSystemHistory,
            )
            .is_ok();
        if restored {
            let restore_hash = content_hash_from_flavors(&original.flavors);
            if let Ok(mut ledger) = self_writes.lock() {
                ledger.register(restore_hash, restore_nonce, Instant::now());
            }
        }
        restored
    };
    state = state.observe(if restored {
        SelfTestObservation::RestoreConfirmed
    } else {
        SelfTestObservation::TimedOut
    });

    state == SelfTestState::Passed
}

fn capture_policy(config: &Config) -> CapturePolicy {
    let rules = config
        .source_rules
        .iter()
        .filter_map(|rule| {
            let predicate = SourcePredicate::try_new(
                rule.app_contains.clone(),
                rule.title_regex.as_deref(),
                rule.url_host_suffix.clone(),
            )
            .map_err(|error| {
                tracing::warn!(
                    pattern_bytes = rule.title_regex.as_ref().map_or(0, String::len),
                    "invalid capture-rule regex: {error}"
                );
            })
            .ok()?;
            let action = match rule.action {
                SourceRuleAction::Capture => CaptureAction::Capture,
                SourceRuleAction::Skip => CaptureAction::Skip,
                SourceRuleAction::PlainTextOnly => CaptureAction::PlainTextOnly,
                SourceRuleAction::StripImages => CaptureAction::StripImages,
                SourceRuleAction::CaptureSensitive => CaptureAction::CaptureSensitive,
            };
            Some(CaptureRule { predicate, action })
        })
        .collect();
    CapturePolicy {
        skip_whitespace_only: config.skip_whitespace_only,
        detect_secrets: config.detect_secrets,
        secret_ttl: Duration::from_secs(config.secret_ttl_seconds.max(1)),
        excluded_apps: config
            .excluded_apps
            .iter()
            .filter(|app| !app.is_empty())
            .cloned()
            .collect(),
        rules,
        ..CapturePolicy::default()
    }
}

fn apply_action(flavors: &mut Vec<vbuff_types::Flavor>, action: CaptureAction) {
    match action {
        CaptureAction::PlainTextOnly => flavors.retain(vbuff_types::Flavor::is_plain_text),
        CaptureAction::StripImages => flavors.retain(|flavor| !flavor.is_image()),
        CaptureAction::Capture | CaptureAction::CaptureSensitive => {}
        CaptureAction::Skip => flavors.clear(),
    }
}

fn record_drop(
    history: &History,
    diagnostics: &Diagnostics,
    ledger: &mut CaptureLossLedger,
    reason: DropReason,
    count: u64,
) {
    ledger.dropped_n(reason, count);
    diagnostics.capture_outcome(CaptureOutcome::Dropped(reason), count);
    if let Err(error) = history.record_capture_outcome(CaptureOutcome::Dropped(reason), count) {
        tracing::warn!(
            reason = reason.as_str(),
            "capture accounting write failed: {error}"
        );
    }
}

fn watchdog_timeout(config: &Config) -> Duration {
    let poll_budget = Duration::from_millis(config.poll_interval_ms.max(50).saturating_mul(8));
    poll_budget.max(MIN_WATCHDOG_TIMEOUT)
}

fn sleep_with_heartbeat(delay: Duration, heartbeat: &Heartbeat) {
    let deadline = Instant::now()
        .checked_add(delay)
        .unwrap_or_else(Instant::now);
    while Instant::now() < deadline {
        heartbeat.beat();
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(500)),
        );
    }
    heartbeat.beat();
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

fn build_clip(
    captured: CapturedClipboard,
    content_hash: [u8; 32],
    sensitive: bool,
    sync_eligible: bool,
    ai_allowed: bool,
    expires_after: Option<Duration>,
) -> Clip {
    let kind = detect_kind(&captured.flavors);
    let byte_size = captured
        .flavors
        .iter()
        .map(|flavor| flavor.body.byte_size())
        .sum();

    let source_app = captured.provenance.app_id.clone();
    let mut meta = ClipMeta::now(kind, byte_size, source_app);
    meta.provenance = captured.provenance;
    meta.generation = captured.generation;
    meta.lineage = captured.lineage;
    meta.sensitive = sensitive;
    meta.sync_eligible = sync_eligible;
    meta.ai_allowed = ai_allowed;
    meta.expires_at = expires_after
        .and_then(|ttl| chrono::Duration::from_std(ttl).ok())
        .map(|ttl| chrono::Utc::now() + ttl);

    Clip {
        id: ClipId::new(),
        flavors: captured.flavors,
        content_hash,
        meta,
        pinned: false,
        favorite: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_platform::{PlatformError, Result as PlatformResult};
    use vbuff_types::Flavor;

    #[derive(Clone)]
    struct SelfTestClipboard {
        current: CapturedClipboard,
    }

    impl ClipboardBackend for SelfTestClipboard {
        fn read(&mut self) -> PlatformResult<CapturedClipboard> {
            Ok(self.current.clone())
        }

        fn write(&mut self, flavors: &[Flavor]) -> PlatformResult<()> {
            if flavors.is_empty() {
                return Err(PlatformError::Empty);
            }
            self.current = CapturedClipboard {
                flavors: flavors.to_vec(),
                ..CapturedClipboard::default()
            };
            Ok(())
        }

        fn clear(&mut self) -> PlatformResult<()> {
            self.current = CapturedClipboard::default();
            Ok(())
        }
    }

    fn captured(text: &str, source_app: Option<&str>) -> CapturedClipboard {
        CapturedClipboard {
            flavors: vec![Flavor::inline("text/plain", text.as_bytes().to_vec())],
            provenance: vbuff_types::CaptureProvenance {
                app_id: source_app.map(str::to_owned),
                ..Default::default()
            },
            ..CapturedClipboard::default()
        }
    }

    fn decision(policy: &CapturePolicy, captured: &CapturedClipboard) -> CaptureDecision {
        policy.decide(CaptureInput {
            flavors: &captured.flavors,
            provenance: &captured.provenance,
            source: SelectionSource::Clipboard,
            primary_intended: true,
            coherent_generation: true,
            concealed: false,
            self_write: false,
        })
    }

    #[test]
    fn policy_rejects_whitespace_and_excluded_apps() {
        let config = Config {
            excluded_apps: vec!["onepassword".into()],
            ..Default::default()
        };
        let policy = capture_policy(&config);

        assert_eq!(
            decision(&policy, &captured("  \n", None)),
            CaptureDecision::Skip(DropReason::WhitespaceOnly)
        );
        assert_eq!(
            decision(
                &policy,
                &captured("secret", Some("com.AgileBits.OnePassword7"))
            ),
            CaptureDecision::Skip(DropReason::ExcludedSource)
        );
        assert!(matches!(
            decision(&policy, &captured("hello", Some("com.apple.Safari"))),
            CaptureDecision::Capture { .. }
        ));
    }

    #[test]
    fn plain_text_action_drops_html_and_images() {
        let mut flavors = vec![
            Flavor::inline("text/html", b"<b>safe</b>".to_vec()),
            Flavor::inline("text/plain;charset=utf-8", b"safe".to_vec()),
            Flavor::inline("image/png", vec![1, 2, 3]),
        ];

        apply_action(&mut flavors, CaptureAction::PlainTextOnly);

        assert_eq!(flavors.len(), 1);
        assert!(flavors[0].is_plain_text());
    }

    #[test]
    fn build_clip_preserves_source_and_byte_count() {
        let captured = captured("hello", Some("editor.app"));
        let hash = content_hash_from_flavors(&captured.flavors);
        let clip = build_clip(
            captured,
            hash,
            true,
            false,
            false,
            Some(Duration::from_secs(90)),
        );

        assert_eq!(clip.meta.byte_size, 5);
        assert_eq!(clip.meta.source_app.as_deref(), Some("editor.app"));
        assert_eq!(clip.meta.provenance.app_id.as_deref(), Some("editor.app"));
        assert!(clip.meta.sensitive);
        assert!(!clip.meta.sync_eligible);
        assert!(clip.meta.expires_at.is_some());
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

    #[test]
    fn repeated_shed_is_throttled_but_retried_after_the_window() {
        let now = Instant::now();
        let hash = [7; 32];
        let mut last_shed = Some((hash, now));
        assert!(shed_retry_suppressed(
            &mut last_shed,
            hash,
            now + Duration::from_secs(4)
        ));
        assert!(!shed_retry_suppressed(
            &mut last_shed,
            hash,
            now + Duration::from_secs(5)
        ));
    }

    #[test]
    fn self_test_restores_and_suppresses_the_original_clipboard() {
        let original = captured("do not recapture me", Some("secret.app"));
        let original_hash = content_hash_from_flavors(&original.flavors);
        let mut clipboard = SelfTestClipboard { current: original };
        let ledger = Arc::new(Mutex::new(SelfWriteLedger::default()));

        assert!(capture_self_test(&mut clipboard, &ledger));
        assert_eq!(
            content_hash_from_flavors(&clipboard.current.flavors),
            original_hash
        );
        assert!(ledger.lock().unwrap().matches(
            original_hash,
            &vbuff_types::CaptureLineage::default(),
            Instant::now(),
        ));
    }
}
