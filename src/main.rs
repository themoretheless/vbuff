//! vbuff - cross-platform clipboard manager (MVP single-process build).
//!
//! The root binary is composition only. Single-instance handoff, capture,
//! history, commands, paste-back, tray integration, configuration, and the
//! eframe loop each live in a focused module.

mod app;
mod autostart;
mod capture;
mod commands;
mod config;
mod diagnostics;
mod doctor;
mod heartbeat;
mod history;
mod logging;
mod maintenance;
mod memory_pressure;
mod paste;
mod runtime_metrics;
mod single_instance;
#[cfg(feature = "tray")]
mod tray;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use vbuff_core::capture::SelfWriteLedger;
use vbuff_gui::{AppState, SharedState};
use vbuff_platform::{CapabilityLevel, GlobalHotkeyBackend, HotkeyBackend, parse_combo};
use vbuff_store::Store;
use vbuff_types::{ClientIntent, SecurityPostureLevel, SecurityPostureSummary};

use config::Config;
use diagnostics::Diagnostics;
use history::History;
use single_instance::LaunchOutcome;

/// How many clips to keep loaded in the GUI snapshot.
const GUI_LIMIT: usize = 1000;

fn main() -> anyhow::Result<()> {
    logging::init();
    let process_hardening = vbuff_platform::harden_current_process();
    if !process_hardening.core_dumps_blocked {
        tracing::warn!("core-dump suppression is unavailable on this platform");
    }

    if let Some(format) = doctor::requested() {
        let strict_mode = Config::load_for_inspection()
            .map(|config| config.strict_security_mode)
            .unwrap_or(false);
        return doctor::run(format, process_hardening, strict_mode);
    }

    let (_instance_guard, instance_intents) =
        match single_instance::acquire_or_forward(ClientIntent::ShowPopup)
            .context("acquiring single-instance endpoint")?
        {
            LaunchOutcome::Primary { guard, intents } => (guard, intents),
            LaunchOutcome::Forwarded => return Ok(()),
        };

    let config = Config::load_or_create().context("loading config")?;
    let security_posture = vbuff_platform::SecurityPosture::detect(
        config.strict_security_mode,
        process_hardening.core_dumps_blocked,
        process_hardening.ptrace_blocked,
    );
    let strict_capture_blocked = !security_posture.strict_allows_capture();
    tracing::info!(?config.hotkey, config.poll_interval_ms, "vbuff starting");
    if config.launch_at_login
        && let Err(error) = autostart::set_enabled(true)
    {
        tracing::warn!("failed to register launch-at-login: {error}");
    }

    let store = Store::open_default().context("opening store")?;
    store
        .enforce_cap(config.max_history)
        .context("enforcing history cap")?;
    let recent = store.load_recent(GUI_LIMIT).context("loading history")?;
    let mut initial_state = AppState::with_clips(recent);
    initial_state.paused = strict_capture_blocked;
    initial_state.security_posture = summarize_security_posture(&security_posture);
    let shared: SharedState = Arc::new(Mutex::new(initial_state));
    let history = History::new(store, Arc::clone(&shared), GUI_LIMIT);
    let diagnostics = Diagnostics::new(Arc::clone(&shared));
    if strict_capture_blocked {
        diagnostics.notice(
            vbuff_types::NoticeLevel::Warning,
            "Strict security mode blocked capture; run vbuff doctor --json",
        );
    }
    diagnostics.install_panic_hook();
    let _heartbeat_thread = heartbeat::spawn(diagnostics.clone());
    let _maintenance_thread =
        maintenance::spawn(history.clone(), diagnostics.clone(), config.clone());
    let paused = Arc::new(AtomicBool::new(strict_capture_blocked));
    let self_writes = Arc::new(Mutex::new(SelfWriteLedger::default()));

    let _capture_thread = (!strict_capture_blocked).then(|| {
        capture::spawn(
            history.clone(),
            diagnostics.clone(),
            Arc::clone(&paused),
            config.clone(),
            Arc::clone(&self_writes),
        )
    });

    let mut hotkey_backend = GlobalHotkeyBackend::new().context("creating hotkey backend")?;
    let combo = parse_combo(&config.hotkey)
        .with_context(|| format!("parsing hotkey {:?}", config.hotkey))?;
    let hotkey_id = match hotkey_backend.register(&combo) {
        Ok(id) => Some(id),
        Err(error) => {
            tracing::warn!("failed to register hotkey {:?}: {error}", config.hotkey);
            None
        }
    };

    let app_services = app::AppServices {
        history,
        shared,
        diagnostics,
        instance_intents,
        paused,
        config,
        self_writes,
        strict_capture_blocked,
    };
    app::run(app_services, hotkey_backend, hotkey_id)
}

fn summarize_security_posture(posture: &vbuff_platform::SecurityPosture) -> SecurityPostureSummary {
    let mut summary = SecurityPostureSummary {
        strict_mode: posture.strict_mode,
        ..SecurityPostureSummary::default()
    };
    for capability in &posture.capabilities {
        match capability.level {
            CapabilityLevel::Active | CapabilityLevel::NotApplicable => {
                summary.active = summary.active.saturating_add(1);
            }
            CapabilityLevel::Degraded => {
                summary.degraded = summary.degraded.saturating_add(1);
            }
            CapabilityLevel::Unavailable => {
                summary.unavailable = summary.unavailable.saturating_add(1);
            }
        }
    }
    summary.level = if posture.strict_mode && !posture.strict_allows_capture() {
        SecurityPostureLevel::Blocked
    } else if summary.degraded > 0 || summary.unavailable > 0 {
        SecurityPostureLevel::Partial
    } else {
        SecurityPostureLevel::Protected
    };
    summary
}
