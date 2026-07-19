//! The eframe event loop: wires the hotkey receiver, the tray, the popup, and
//! pending paste-back together, delegating every side effect to
//! [`crate::actions`]. No capture-thread or tray-menu-building code lives
//! here.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::App as _;
use global_hotkey::GlobalHotKeyEvent;
use vbuff_gui::{PopupApp, SharedState};
use vbuff_platform::{EnigoPaste, GlobalHotkeyBackend, HotkeyBackend, PasteBackend};
use vbuff_store::Store;

use crate::actions::{PendingPaste, handle_action};
#[cfg(feature = "tray")]
use crate::actions::{copy_latest_clip, refresh_snapshot, toggle_pause};

/// Launch the eframe popup and run the main loop.
pub(crate) fn run_gui(
    store: Arc<Mutex<Store>>,
    shared: SharedState,
    paused: Arc<AtomicBool>,
    mut hotkey_backend: GlobalHotkeyBackend,
    hotkey_id: Option<u32>,
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
    let mut tray: Option<crate::tray::Tray> = None;

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
                tray = crate::tray::Tray::new().ok();
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
                t.sync_state(s.paused, s.clips.len());
            }

            for action in t.poll() {
                match action {
                    crate::tray::TrayAction::Show => {
                        shared.lock().unwrap().request_show();
                    }
                    crate::tray::TrayAction::CopyLatest => {
                        copy_latest_clip(&shared);
                    }
                    crate::tray::TrayAction::ClearHistory => {
                        if let Ok(store) = store.lock() {
                            let _ = store.clear();
                            refresh_snapshot(&store, &shared);
                        }
                    }
                    crate::tray::TrayAction::TogglePause => {
                        toggle_pause(&paused, &shared);
                    }
                    crate::tray::TrayAction::Quit => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        std::process::exit(0);
                    }
                }
            }
        }

        // 3. Run the popup, then handle the actions it produced.
        popup.update(ctx, _frame);
        for action in popup.take_actions() {
            handle_action(action, &store, &shared, &paused, &mut pending_paste, ctx);
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
    if let Some(id) = hotkey_id {
        let _ = hotkey_backend.unregister(id);
    }

    result.map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
