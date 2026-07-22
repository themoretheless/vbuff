//! vbuff - cross-platform clipboard manager (MVP single-process build).
//!
//! The root binary is composition only. Single-instance handoff, capture,
//! history, commands, paste-back, tray integration, configuration, and the
//! eframe loop each live in a focused module.

mod app;
mod ask;
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
mod seed_pack;
mod single_instance;
#[cfg(feature = "tray")]
mod tray;
mod verify;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use vbuff_core::capture::SelfWriteLedger;
use vbuff_core::trust::{PrivacyPostureInput, PrivacyScore};
use vbuff_gui::{AppState, SharedState};
use vbuff_platform::{CapabilityLevel, GlobalHotkeyBackend, HotkeyBackend, parse_combo};
use vbuff_store::Store;
use vbuff_types::{
    CapabilityView, CapabilityViewLevel, CapturePauseReason, ClientIntent, SecurityPostureLevel,
    SecurityPostureSummary,
};

use config::Config;
use diagnostics::Diagnostics;
use history::History;
use single_instance::LaunchOutcome;

/// How many clips to keep loaded in the GUI snapshot.
const GUI_LIMIT: usize = 1000;

fn main() -> anyhow::Result<()> {
    logging::init();
    let background_launch = autostart::background_requested();
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

    if let Some(command) = config::requested()? {
        return config::run(command);
    }

    if let Some(command) = verify::requested()? {
        return verify::run(command);
    }

    if let Some(command) = ask::requested()? {
        return ask::run(command);
    }

    let initial_intent = if background_launch {
        ClientIntent::Ping
    } else {
        ClientIntent::ShowPopup
    };
    let (_instance_guard, instance_intents) =
        match single_instance::acquire_or_forward(initial_intent)
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
    let session_context = vbuff_platform::lifecycle::SessionContext::detect();
    let remote_auto_paused = config.auto_pause_remote && session_context.remote;
    tracing::info!(?config.hotkey, config.poll_interval_ms, "vbuff starting");
    if let Err(error) = autostart::set_enabled(config.launch_at_login) {
        tracing::warn!(
            desired = config.launch_at_login,
            "failed to reconcile launch-at-login registration: {error}"
        );
    }

    let store = Store::open_default().context("opening store")?;
    store
        .enforce_cap(config.max_history)
        .context("enforcing history cap")?;
    let health_digest = store
        .clipboard_health_digest()
        .context("building clipboard health digest")?;
    let recent = store.load_recent(GUI_LIMIT).context("loading history")?;
    let mut initial_state = AppState::with_clips(recent);
    initial_state.health_digest = health_digest;
    initial_state.paused = strict_capture_blocked || remote_auto_paused;
    initial_state.pause_reason = if strict_capture_blocked {
        Some(CapturePauseReason::SecurityPolicy)
    } else if remote_auto_paused {
        Some(CapturePauseReason::RemoteControl)
    } else {
        None
    };
    initial_state.default_profile = config.default_profile;
    initial_state.launch_at_login = config.launch_at_login;
    if !background_launch {
        initial_state.request_show();
    }
    initial_state.security_posture = summarize_security_posture(&security_posture);
    initial_state.capabilities = summarize_capabilities(&security_posture);
    initial_state.privacy_score = Some(PrivacyScore::calculate(privacy_posture_input(
        &config,
        &security_posture,
    )));
    let shared: SharedState = Arc::new(Mutex::new(initial_state));
    let history = History::new(store, Arc::clone(&shared), GUI_LIMIT);
    let diagnostics = Diagnostics::new(Arc::clone(&shared));
    if strict_capture_blocked {
        diagnostics.notice(
            vbuff_types::NoticeLevel::Warning,
            "Strict security mode blocked capture; run vbuff doctor --json",
        );
    } else if remote_auto_paused {
        diagnostics.notice(
            vbuff_types::NoticeLevel::Warning,
            "Capture auto-paused for the detected remote session",
        );
    }
    diagnostics.install_panic_hook();
    let _heartbeat_thread = heartbeat::spawn(diagnostics.clone());
    let _maintenance_thread =
        maintenance::spawn(history.clone(), diagnostics.clone(), config.clone());
    let paused = Arc::new(AtomicBool::new(
        strict_capture_blocked || remote_auto_paused,
    ));
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

    let mut hotkey_backend = match GlobalHotkeyBackend::new() {
        Ok(backend) => Some(backend),
        Err(error) => {
            tracing::warn!("global hotkey backend unavailable: {error}");
            None
        }
    };
    let combo = parse_combo(&config.hotkey).map_err(|error| {
        tracing::warn!("failed to parse hotkey {:?}: {error}", config.hotkey);
        error
    });
    let hotkey_id = match (hotkey_backend.as_mut(), combo) {
        (Some(backend), Ok(combo)) => match backend.register(&combo) {
            Ok(id) => Some(id),
            Err(error) => {
                tracing::warn!("failed to register hotkey {:?}: {error}", config.hotkey);
                None
            }
        },
        _ => None,
    };
    if hotkey_id.is_none() {
        diagnostics.notice(
            vbuff_types::NoticeLevel::Warning,
            "Global shortcut unavailable; use this window, the menu bar, or relaunch vbuff",
        );
        if background_launch && let Ok(mut state) = shared.lock() {
            state.request_show();
        }
    }

    let app_services = app::AppServices {
        history,
        shared,
        diagnostics,
        instance_intents,
        paused,
        config,
        self_writes,
        strict_capture_blocked,
        automatic_pause_reason: remote_auto_paused.then_some(CapturePauseReason::RemoteControl),
        hotkey_registered: hotkey_id.is_some(),
    };
    app::run(app_services, hotkey_backend, hotkey_id)
}

fn summarize_capabilities(posture: &vbuff_platform::SecurityPosture) -> Vec<CapabilityView> {
    posture
        .capabilities
        .iter()
        .map(|capability| CapabilityView {
            feature: capability.feature.clone(),
            level: match capability.level {
                CapabilityLevel::Active => CapabilityViewLevel::Active,
                CapabilityLevel::Degraded => CapabilityViewLevel::Degraded,
                CapabilityLevel::Unavailable => CapabilityViewLevel::Unavailable,
                CapabilityLevel::NotApplicable => CapabilityViewLevel::NotApplicable,
            },
            detail: capability.detail.clone(),
        })
        .collect()
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

fn privacy_posture_input(
    config: &Config,
    posture: &vbuff_platform::SecurityPosture,
) -> PrivacyPostureInput {
    let encryption_at_rest = posture.capabilities.iter().any(|capability| {
        capability.feature == "encryption_at_rest" && capability.level == CapabilityLevel::Active
    });
    let foreground_identity_active = posture.capabilities.iter().any(|capability| {
        capability.feature == "foreground_identity" && capability.level == CapabilityLevel::Active
    });
    let denied_source_count = if foreground_identity_active {
        let denied_rules = config
            .source_rules
            .iter()
            .filter(|rule| matches!(rule.action, config::SourceRuleAction::Skip))
            .count();
        config
            .excluded_apps
            .len()
            .saturating_add(denied_rules)
            .min(u32::MAX as usize) as u32
    } else {
        0
    };
    PrivacyPostureInput {
        encryption_at_rest,
        strict_local_only: false,
        sensitive_memory_only: false,
        telemetry_enabled: false,
        sync_enabled: false,
        denied_source_count,
        retention_days: None,
    }
}
