//! vbuff - cross-platform clipboard manager (MVP single-process build).
//!
//! The root binary is composition only. Capture, history, commands, paste-back,
//! tray integration, configuration, and the eframe loop each live in a focused
//! module so the implementation can be read in small pieces.

mod app;
mod autostart;
mod capture;
mod commands;
mod config;
mod history;
mod paste;
#[cfg(feature = "tray")]
mod tray;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use vbuff_gui::{AppState, SharedState};
use vbuff_platform::{GlobalHotkeyBackend, HotkeyBackend, parse_combo};
use vbuff_store::Store;

use config::Config;
use history::History;

/// How many clips to keep loaded in the GUI snapshot.
const GUI_LIMIT: usize = 1000;

fn main() -> anyhow::Result<()> {
    init_tracing();

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
    let shared: SharedState = Arc::new(Mutex::new(AppState {
        clips: recent,
        paused: false,
        show_requested: false,
        revision: 0,
    }));
    let history = History::new(store, Arc::clone(&shared), GUI_LIMIT);
    let paused = Arc::new(AtomicBool::new(false));

    let _capture_thread = capture::spawn(history.clone(), Arc::clone(&paused), config.clone());

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

    app::run(history, shared, paused, config, hotkey_backend, hotkey_id)
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
