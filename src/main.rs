//! vbuff - cross-platform clipboard manager (MVP single-process build).
//!
//! This binary wires the MVP subset of the full architecture into one process:
//!
//! * a background **capture thread** ([`capture`]) polls the clipboard
//!   (`arboard`) and inserts new clips into the **store** (`rusqlite`),
//!   deduplicating by BLAKE3 hash;
//! * a **global hotkey** (`global-hotkey`) shows/focuses the popup;
//! * an optional **tray/menu-bar icon** ([`tray`], behind the `tray` feature)
//!   offers Show, Pause/Resume, Copy Latest, Clear History, and Quit actions;
//! * the **popup** (`eframe`/`egui`, run by [`gui`]) lists history, filters
//!   as you type, and on pick writes the clip back to the clipboard and
//!   synthesizes a paste keystroke (`enigo`, dispatched by [`actions`]).
//!
//! All side effects flow through the eframe `update` loop, which owns the
//! event loop; the capture thread is the one true background worker. This
//! file only does startup wiring - each concern lives in its own module so
//! the whole binary can be understood one small file at a time:
//! [`config`] (policy), [`capture`] (the poll loop + capture gate),
//! [`actions`] (UI action -> side effect), [`gui`] (the eframe event loop),
//! and [`tray`] (the tray icon/menu).

mod actions;
mod capture;
mod config;
mod constants;
mod gui;
#[cfg(feature = "tray")]
mod tray;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use vbuff_gui::AppState;
use vbuff_platform::{GlobalHotkeyBackend, HotkeyBackend, parse_combo};
use vbuff_store::Store;

use capture::spawn_capture_thread;
use config::Config;
use constants::GUI_LIMIT;
use gui::run_gui;

fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::load_or_create().context("loading config")?;
    tracing::info!(?config.hotkey, config.poll_interval_ms, "vbuff starting");

    // Open the persistent store and hydrate the GUI snapshot.
    let store = Store::open_default().context("opening store")?;
    store
        .enforce_cap(config.max_history)
        .context("enforcing history cap")?;
    let recent = store.load_recent(GUI_LIMIT).context("loading history")?;
    let store = Arc::new(Mutex::new(store));

    // Shared GUI state.
    let shared = Arc::new(Mutex::new(AppState {
        clips: recent,
        paused: false,
        show_requested: false,
        revision: 0,
    }));

    // Pause flag shared with the capture thread.
    let paused = Arc::new(AtomicBool::new(false));

    // Spawn the clipboard capture thread.
    spawn_capture_thread(
        Arc::clone(&store),
        Arc::clone(&shared),
        Arc::clone(&paused),
        config.clone(),
    );

    // Register the global hotkey before the event loop starts; events are
    // polled inside the eframe update loop via GlobalHotKeyEvent::receiver().
    let mut hotkey_backend = GlobalHotkeyBackend::new().context("creating hotkey backend")?;
    let combo = parse_combo(&config.hotkey)
        .with_context(|| format!("parsing hotkey {:?}", config.hotkey))?;
    let hotkey_id = match hotkey_backend.register(&combo) {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!("failed to register hotkey {:?}: {e}", config.hotkey);
            None
        }
    };

    run_gui(store, shared, paused, hotkey_backend, hotkey_id)
}

/// Initialize tracing with an env filter (RUST_LOG), defaulting to `info`.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
