//! vbuff - cross-platform clipboard manager (MVP single-process build).
//!
//! This binary wires the MVP subset of the full architecture into one process:
//!
//! * a background **capture thread** polls the clipboard (`arboard`) and inserts
//!   new clips into the **store** (`rusqlite`), deduplicating by BLAKE3 hash;
//! * a **global hotkey** (`global-hotkey`) shows/focuses the popup;
//! * an optional **tray/menu-bar icon** (`tray-icon`, behind the `tray` feature)
//!   offers Show, Pause/Resume, Copy Latest, Clear History, and Quit actions;
//! * the **popup** (`eframe`/`egui`) lists history, filters as you type, and on
//!   pick writes the clip back to the clipboard and synthesizes a paste
//!   keystroke (`enigo`).
//!
//! All side effects flow through the eframe `update` loop, which owns the event
//! loop; the capture thread is the one true background worker.

mod autostart;
mod config;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use eframe::App as _;
use global_hotkey::GlobalHotKeyEvent;
use vbuff_core::{content_hash_from_flavors, detect_kind};
use vbuff_gui::{AppState, PopupApp, SharedState, UiAction};
use vbuff_platform::{
    ArboardClipboard, ClipboardBackend, EnigoPaste, GlobalHotkeyBackend, HotkeyBackend,
    PasteBackend, parse_combo,
};
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId, ClipMeta};

use config::Config;

/// How many clips to keep loaded in the GUI snapshot.
const GUI_LIMIT: usize = 1000;

fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::load_or_create().context("loading config")?;
    tracing::info!(?config.hotkey, config.poll_interval_ms, "vbuff starting");
    if config.launch_at_login
        && let Err(e) = autostart::set_enabled(true)
    {
        tracing::warn!("failed to register launch-at-login: {e}");
    }

    // Open the persistent store and hydrate the GUI snapshot.
    let store = Store::open_default().context("opening store")?;
    store
        .enforce_cap(config.max_history)
        .context("enforcing history cap")?;
    let recent = store.load_recent(GUI_LIMIT).context("loading history")?;
    let store = Arc::new(Mutex::new(store));

    // Shared GUI state.
    let shared: SharedState = Arc::new(Mutex::new(AppState {
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

    run_gui(
        Arc::clone(&store),
        Arc::clone(&shared),
        Arc::clone(&paused),
        config,
        hotkey_backend,
        hotkey_id,
    )
}

/// Initialize tracing with an env filter (RUST_LOG), defaulting to `info`.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Spawn the background thread that polls the clipboard and inserts new clips.
fn spawn_capture_thread(
    store: Arc<Mutex<Store>>,
    shared: SharedState,
    paused: Arc<AtomicBool>,
    config: Config,
) {
    std::thread::spawn(move || {
        let mut clipboard = match ArboardClipboard::new() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("clipboard backend unavailable: {e}");
                return;
            }
        };

        let mut last_hash: Option<[u8; 32]> = None;
        let interval = Duration::from_millis(config.poll_interval_ms.max(50));

        loop {
            std::thread::sleep(interval);

            if paused.load(Ordering::Relaxed) {
                continue;
            }

            let captured = match clipboard.read() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if captured.is_empty() {
                continue;
            }

            let hash = content_hash_from_flavors(&captured.flavors);
            if last_hash == Some(hash) {
                continue; // unchanged since last poll
            }

            // Apply cheap capture-gate rules.
            if let Some(text) = captured.flavors.iter().find_map(|f| f.as_text())
                && config.skip_whitespace_only
                && text.trim().is_empty()
            {
                last_hash = Some(hash);
                continue;
            }
            if let Some(app) = &captured.source_app
                && config.is_excluded(app)
            {
                last_hash = Some(hash);
                continue;
            }

            last_hash = Some(hash);

            let kind = detect_kind(&captured.flavors);
            let byte_size: u64 = captured.flavors.iter().map(|f| f.body.byte_size()).sum();
            let clip = Clip {
                id: ClipId::new(),
                flavors: captured.flavors,
                content_hash: hash,
                meta: ClipMeta::now(kind, byte_size, captured.source_app),
                pinned: false,
                favorite: false,
            };

            // Insert (dedup-aware), enforce cap, and refresh the GUI snapshot.
            let refreshed = {
                let store = store.lock().unwrap();
                if let Err(e) = store.insert(&clip) {
                    tracing::warn!("insert failed: {e}");
                    continue;
                }
                let _ = store.enforce_cap(config.max_history);
                store.load_recent(GUI_LIMIT).ok()
            };
            if let Some(clips) = refreshed {
                let mut s = shared.lock().unwrap();
                s.set_clips(clips);
            }
        }
    });
}

/// A pending paste, sequenced across frames so the popup hides before the
/// keystroke is sent to the previously focused app.
struct PendingPaste {
    /// Fire the keystroke once this instant passes.
    at: Instant,
}

/// Launch the eframe popup and run the main loop.
fn run_gui(
    store: Arc<Mutex<Store>>,
    shared: SharedState,
    paused: Arc<AtomicBool>,
    config: Config,
    mut hotkey_backend: GlobalHotkeyBackend,
    _hotkey_id: Option<u32>,
) -> anyhow::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("vbuff")
        .with_inner_size([520.0, 600.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_visible(false);

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let mut popup = PopupApp::new(Arc::clone(&shared));
    let mut paste_backend = EnigoPaste::new().ok();
    if paste_backend.is_none() {
        tracing::warn!("paste backend unavailable; paste-back disabled");
    }

    // Tray is created lazily on the first frame (macOS requires the run loop to
    // be active and the call to be on the main thread).
    #[cfg(feature = "tray")]
    let mut tray: Option<tray_support::Tray> = None;
    #[cfg(feature = "tray")]
    let mut config = config;

    let mut pending_paste: Option<PendingPaste> = None;
    // Spawned once, on the first frame: a ticker that pings the egui context
    // from another thread. This is what keeps the update loop alive while the
    // window is hidden. A hidden winit window stops delivering redraw events on
    // some platforms, so the in-loop `request_repaint_after` cannot
    // self-perpetuate; an external `request_repaint()` wakes the event loop via
    // its proxy regardless of window visibility, so the hotkey/tray receivers
    // below are always polled.
    let mut ticker_started = false;

    let result = eframe::run_simple_native("vbuff", native_options, move |ctx, _frame| {
        if !ticker_started {
            ticker_started = true;
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    ctx.request_repaint();
                }
            });
        }

        // Lazily create the tray on the first update.
        #[cfg(feature = "tray")]
        {
            if tray.is_none() {
                tray = tray_support::Tray::new().ok();
                if tray.is_none() {
                    tracing::warn!("tray icon unavailable");
                }
            }
        }

        // Keep the event loop ticking even while the window is hidden so the
        // hotkey/tray receivers below are polled steadily. eframe would
        // otherwise sleep until a window event arrives, and a hidden window
        // receives none. 100 ms is responsive for a hotkey at negligible CPU.
        ctx.request_repaint_after(Duration::from_millis(100));

        // 1. Poll the global hotkey receiver.
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == global_hotkey::HotKeyState::Pressed {
                let mut s = shared.lock().unwrap();
                s.request_show();
            }
        }

        // 2. Poll tray menu/icon events.
        #[cfg(feature = "tray")]
        if let Some(t) = &tray {
            {
                let s = shared.lock().unwrap();
                t.sync_state(s.paused, s.clips.len(), config.launch_at_login);
            }

            for action in t.poll() {
                match action {
                    tray_support::TrayAction::Show => {
                        shared.lock().unwrap().request_show();
                    }
                    tray_support::TrayAction::CopyLatest => {
                        copy_latest_clip(&shared);
                    }
                    tray_support::TrayAction::ClearHistory => {
                        if let Ok(store) = store.lock() {
                            let _ = store.clear();
                            refresh_snapshot(&store, &shared);
                        }
                    }
                    tray_support::TrayAction::TogglePause => {
                        toggle_pause(&paused, &shared);
                    }
                    tray_support::TrayAction::ToggleAutostart => {
                        toggle_autostart(&mut config);
                    }
                    tray_support::TrayAction::Quit => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        std::process::exit(0);
                    }
                }
            }
        }

        // 3. Run the popup, then handle the actions it produced.
        popup.update(ctx, _frame);
        for action in popup.take_actions() {
            handle_action(
                action,
                &store,
                &shared,
                &paused,
                &config,
                &mut pending_paste,
                ctx,
            );
        }

        // 4. If a paste is pending and its delay has elapsed, fire it.
        if let Some(p) = &pending_paste {
            if Instant::now() >= p.at {
                if let Some(backend) = &mut paste_backend
                    && let Err(e) = backend.paste()
                {
                    tracing::warn!("paste-back failed: {e}");
                }
                pending_paste = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(20));
            }
        }
    });

    // Best-effort hotkey cleanup on exit.
    if let Some(id) = _hotkey_id {
        let _ = hotkey_backend.unregister(id);
    }

    result.map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}

/// Toggle the capture-pause flag and mirror it into the GUI state.
fn toggle_pause(paused: &Arc<AtomicBool>, shared: &SharedState) {
    let now = !paused.load(Ordering::Relaxed);
    paused.store(now, Ordering::Relaxed);
    shared.lock().unwrap().paused = now;
    tracing::info!(paused = now, "capture pause toggled");
}

/// Copy the most recent clip back to the system clipboard without paste-back.
#[cfg(feature = "tray")]
fn copy_latest_clip(shared: &SharedState) {
    let clip = {
        let s = shared.lock().unwrap();
        s.clips.first().cloned()
    };
    let Some(clip) = clip else { return };

    if let Ok(mut cb) = ArboardClipboard::new()
        && let Err(e) = cb.write(&clip.flavors)
    {
        tracing::warn!("copy latest from tray failed: {e}");
    }
}

/// Toggle launch-at-login and persist the config if OS registration succeeds.
#[cfg(feature = "tray")]
fn toggle_autostart(config: &mut Config) {
    let desired = !config.launch_at_login;
    match autostart::set_enabled(desired) {
        Ok(()) => {
            config.launch_at_login = desired;
            if let Err(e) = config.save() {
                tracing::warn!("saving launch-at-login config failed: {e}");
            }
            tracing::info!(launch_at_login = desired, "launch-at-login toggled");
        }
        Err(e) => {
            tracing::warn!("launch-at-login toggle failed: {e}");
        }
    }
}

/// Translate a GUI action into store/clipboard/paste side effects.
fn handle_action(
    action: UiAction,
    store: &Arc<Mutex<Store>>,
    shared: &SharedState,
    paused: &Arc<AtomicBool>,
    config: &Config,
    pending_paste: &mut Option<PendingPaste>,
    ctx: &egui::Context,
) {
    match action {
        UiAction::Paste(id) => {
            // Find the clip, write it to the clipboard, hide, then schedule the
            // paste keystroke for a couple of frames later.
            let clip = {
                let s = shared.lock().unwrap();
                s.clips.iter().find(|c| c.id == id).cloned()
            };
            let Some(clip) = clip else { return };

            if let Ok(mut cb) = ArboardClipboard::new()
                && let Err(e) = cb.write(&clip.flavors)
            {
                tracing::warn!("clipboard write failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            *pending_paste = Some(PendingPaste {
                at: Instant::now() + Duration::from_millis(120),
            });
            ctx.request_repaint_after(Duration::from_millis(20));
        }
        UiAction::SetPinned(id, pinned) => {
            if let Ok(store) = store.lock() {
                let _ = store.set_pinned(id, pinned);
                refresh_snapshot(&store, shared);
            }
        }
        UiAction::Delete(id) => {
            if let Ok(store) = store.lock() {
                let _ = store.delete(id);
                refresh_snapshot(&store, shared);
            }
        }
        UiAction::ClearAll => {
            if let Ok(store) = store.lock() {
                let _ = store.clear();
                refresh_snapshot(&store, shared);
            }
        }
        UiAction::TogglePause => {
            toggle_pause(paused, shared);
        }
        UiAction::Hide => {
            // The popup already hides itself; nothing extra to do.
            let _ = config;
        }
    }
}

/// Reload the GUI snapshot from the store after a mutation.
fn refresh_snapshot(store: &Store, shared: &SharedState) {
    if let Ok(clips) = store.load_recent(GUI_LIMIT) {
        shared.lock().unwrap().set_clips(clips);
    }
}

/// Tray-icon support, compiled only when the `tray` feature is enabled.
#[cfg(feature = "tray")]
mod tray_support {
    use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{TrayIcon, TrayIconBuilder};

    /// A high-level tray action.
    pub enum TrayAction {
        Show,
        CopyLatest,
        ClearHistory,
        TogglePause,
        ToggleAutostart,
        Quit,
    }

    /// Owns the tray icon and its menu item ids.
    pub struct Tray {
        _icon: TrayIcon,
        show_id: MenuId,
        copy_latest_id: MenuId,
        clear_history_id: MenuId,
        pause_id: MenuId,
        autostart_id: MenuId,
        quit_id: MenuId,
        copy_latest: MenuItem,
        clear_history: MenuItem,
        pause: MenuItem,
        autostart: MenuItem,
    }

    impl Tray {
        /// Build the tray icon and menu.
        pub fn new() -> anyhow::Result<Self> {
            let menu = Menu::new();
            let show = MenuItem::new("Show vbuff", true, None);
            let copy_latest = MenuItem::new("Copy latest clip", false, None);
            let clear_history = MenuItem::new("Clear history", false, None);
            let pause = MenuItem::new("Pause capture", true, None);
            let autostart = MenuItem::new("Start at login", true, None);
            let quit = MenuItem::new("Quit", true, None);
            menu.append(&show)?;
            menu.append(&copy_latest)?;
            menu.append(&clear_history)?;
            menu.append(&PredefinedMenuItem::separator())?;
            menu.append(&pause)?;
            menu.append(&autostart)?;
            menu.append(&PredefinedMenuItem::separator())?;
            menu.append(&quit)?;

            let icon = build_icon();
            let icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("vbuff clipboard manager")
                .with_icon(icon)
                .build()?;

            Ok(Tray {
                _icon: icon,
                show_id: show.id().clone(),
                copy_latest_id: copy_latest.id().clone(),
                clear_history_id: clear_history.id().clone(),
                pause_id: pause.id().clone(),
                autostart_id: autostart.id().clone(),
                quit_id: quit.id().clone(),
                copy_latest,
                clear_history,
                pause,
                autostart,
            })
        }

        /// Keep menu labels and disabled states in sync with the app state.
        pub fn sync_state(&self, paused: bool, clip_count: usize, launch_at_login: bool) {
            self.pause.set_text(if paused {
                "Resume capture"
            } else {
                "Pause capture"
            });
            self.autostart.set_text(if launch_at_login {
                "Don't start at login"
            } else {
                "Start at login"
            });
            self.copy_latest.set_enabled(clip_count > 0);
            self.clear_history.set_enabled(clip_count > 0);
        }

        /// Drain pending tray/menu events into high-level actions.
        pub fn poll(&self) -> Vec<TrayAction> {
            let mut out = Vec::new();
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == self.show_id {
                    out.push(TrayAction::Show);
                } else if event.id == self.copy_latest_id {
                    out.push(TrayAction::CopyLatest);
                } else if event.id == self.clear_history_id {
                    out.push(TrayAction::ClearHistory);
                } else if event.id == self.pause_id {
                    out.push(TrayAction::TogglePause);
                } else if event.id == self.autostart_id {
                    out.push(TrayAction::ToggleAutostart);
                } else if event.id == self.quit_id {
                    out.push(TrayAction::Quit);
                }
            }
            out
        }
    }

    /// A tiny solid 32x32 RGBA icon so the tray has something to show.
    fn build_icon() -> tray_icon::Icon {
        const N: usize = 32;
        let mut rgba = Vec::with_capacity(N * N * 4);
        for y in 0..N {
            for x in 0..N {
                // A simple rounded-ish blue square.
                let border = x < 2 || y < 2 || x >= N - 2 || y >= N - 2;
                if border {
                    rgba.extend_from_slice(&[0x20, 0x40, 0x80, 0xff]);
                } else {
                    rgba.extend_from_slice(&[0x3a, 0x6e, 0xd0, 0xff]);
                }
            }
        }
        tray_icon::Icon::from_rgba(rgba, N as u32, N as u32).expect("valid icon")
    }
}
