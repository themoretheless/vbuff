//! Translating [`UiAction`]s (and a couple of tray actions) into store,
//! clipboard, and paste-back side effects. No rendering and no event-loop
//! plumbing lives here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vbuff_gui::{SharedState, UiAction};
use vbuff_platform::{ArboardClipboard, ClipboardBackend};
use vbuff_store::Store;

use crate::constants::GUI_LIMIT;

/// A pending paste, sequenced across frames so the popup hides before the
/// keystroke is sent to the previously focused app.
pub(crate) struct PendingPaste {
    /// Fire the keystroke once this instant passes.
    pub(crate) at: Instant,
}

/// Toggle the capture-pause flag and mirror it into the GUI state.
pub(crate) fn toggle_pause(paused: &Arc<AtomicBool>, shared: &SharedState) {
    let now = !paused.load(Ordering::Relaxed);
    paused.store(now, Ordering::Relaxed);
    shared.lock().unwrap().paused = now;
    tracing::info!(paused = now, "capture pause toggled");
}

/// Copy the most recent clip back to the system clipboard without paste-back.
///
/// Tray-exclusive: the popup itself has no "copy latest" action of its own
/// (selecting a row is already a copy), so this only ever runs from the tray
/// menu.
#[cfg(feature = "tray")]
pub(crate) fn copy_latest_clip(shared: &SharedState) {
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

/// Translate a GUI action into store/clipboard/paste side effects.
pub(crate) fn handle_action(
    action: UiAction,
    store: &Arc<Mutex<Store>>,
    shared: &SharedState,
    paused: &Arc<AtomicBool>,
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
        }
    }
}

/// Reload the GUI snapshot from the store after a mutation.
pub(crate) fn refresh_snapshot(store: &Store, shared: &SharedState) {
    if let Ok(clips) = store.load_recent(GUI_LIMIT) {
        shared.lock().unwrap().set_clips(clips);
    }
}
