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
mod heartbeat;
mod history;
mod logging;
mod maintenance;
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
use vbuff_platform::{GlobalHotkeyBackend, HotkeyBackend, parse_combo};
use vbuff_store::Store;
use vbuff_types::ClientIntent;

use config::Config;
use diagnostics::Diagnostics;
use history::History;
use single_instance::LaunchOutcome;

/// How many clips to keep loaded in the GUI snapshot.
const GUI_LIMIT: usize = 1000;

fn main() -> anyhow::Result<()> {
    logging::init();

    let (_instance_guard, instance_intents) =
        match single_instance::acquire_or_forward(ClientIntent::ShowPopup)
            .context("acquiring single-instance endpoint")?
        {
            LaunchOutcome::Primary { guard, intents } => (guard, intents),
            LaunchOutcome::Forwarded => return Ok(()),
        };

    let config = Config::load_or_create().context("loading config")?;
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
    let shared: SharedState = Arc::new(Mutex::new(AppState::with_clips(recent)));
    let history = History::new(store, Arc::clone(&shared), GUI_LIMIT);
    let diagnostics = Diagnostics::new(Arc::clone(&shared));
    diagnostics.install_panic_hook();
    let _heartbeat_thread = heartbeat::spawn(diagnostics.clone());
    let _maintenance_thread = maintenance::spawn(history.clone(), diagnostics.clone());
    let paused = Arc::new(AtomicBool::new(false));
    let self_writes = Arc::new(Mutex::new(SelfWriteLedger::default()));

    let _capture_thread = capture::spawn(
        history.clone(),
        diagnostics.clone(),
        Arc::clone(&paused),
        config.clone(),
        Arc::clone(&self_writes),
    );

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

    let app_services = app::AppServices::new(
        history,
        shared,
        diagnostics,
        instance_intents,
        paused,
        config,
        self_writes,
    );
    app::run(app_services, hotkey_backend, hotkey_id)
}
