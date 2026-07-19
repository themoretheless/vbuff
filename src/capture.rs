//! The background clipboard-capture thread.
//!
//! Owns the poll loop, the capture-gate rules (whitespace-only skip, app
//! exclusion), dedup-hash comparison, and inserting new clips into the store.
//! No GUI/tray/hotkey code lives here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vbuff_core::{content_hash_from_flavors, detect_kind};
use vbuff_gui::SharedState;
use vbuff_platform::{ArboardClipboard, ClipboardBackend};
use vbuff_store::Store;
use vbuff_types::{Clip, ClipId, ClipMeta};

use crate::config::Config;
use crate::constants::GUI_LIMIT;

/// Spawn the background thread that polls the clipboard and inserts new clips.
pub(crate) fn spawn_capture_thread(
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
