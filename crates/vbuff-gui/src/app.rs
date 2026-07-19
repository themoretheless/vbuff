//! The eframe popup application: orchestration only.
//!
//! `PopupApp` owns the popup's state and wires the per-frame update loop
//! together, but the actual work is split by responsibility across sibling
//! modules: [`crate::input`] turns key presses into actions/selection
//! changes, [`crate::render`] draws the panel and its rows, and
//! [`crate::thumbnail`] owns the image-texture cache. This file should stay
//! thin - if a change here is not "which step runs in what order," it
//! probably belongs in one of those modules instead.
//!
//! The app does not perform side effects itself. It pushes [`UiAction`]s into
//! a queue, which the wiring drains each frame via [`PopupApp::take_actions`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use egui::ViewportCommand;
use vbuff_core::{SearchResult, search};
use vbuff_types::{Clip, ClipId};

use crate::state::{SharedState, UiAction};
use crate::thumbnail::ThumbnailCache;

/// The eframe application driving the popup.
pub struct PopupApp {
    state: SharedState,
    /// Current search query. Read/written by [`crate::render`]'s search box.
    pub(crate) query: String,
    /// Index of the selected row within the filtered results. Read/written by
    /// both [`crate::input`] (navigation) and [`crate::render`] (highlight).
    pub(crate) selected: usize,
    /// Pending actions for the wiring to drain. Pushed to by both
    /// [`crate::input`] and [`crate::render`].
    pub(crate) actions: VecDeque<UiAction>,
    /// Whether the popup is currently visible.
    visible: bool,
    /// Track the state revision we last rendered so we can reset selection on
    /// new data.
    last_revision: u64,
    /// Cached image-thumbnail textures, pruned as clips disappear. Used by
    /// [`crate::render`] when drawing a row's thumbnail.
    pub(crate) thumbnails: ThumbnailCache,
    /// When the popup was last shown; used to ignore the initial focus-loss
    /// event that can fire during show.
    shown_at: Option<Instant>,
    /// Set when we want the wiring to know we just emitted a Paste so it can
    /// sequence the hide + keystroke. Mirrors the action queue but is simpler
    /// to consume.
    pub(crate) request_focus_next_frame: bool,
}

impl PopupApp {
    /// Construct the popup over shared state, starting hidden.
    pub fn new(state: SharedState) -> Self {
        PopupApp {
            state,
            query: String::new(),
            selected: 0,
            actions: VecDeque::new(),
            visible: false,
            last_revision: u64::MAX,
            thumbnails: ThumbnailCache::default(),
            shown_at: None,
            request_focus_next_frame: false,
        }
    }

    /// Drain queued user actions. The wiring calls this each frame.
    pub fn take_actions(&mut self) -> Vec<UiAction> {
        self.actions.drain(..).collect()
    }

    /// Show the popup (called by the wiring on hotkey/tray).
    fn show(&mut self, ctx: &egui::Context) {
        self.visible = true;
        self.query.clear();
        self.selected = 0;
        self.shown_at = Some(Instant::now());
        self.request_focus_next_frame = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Hide the popup.
    pub(crate) fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    /// Build the current filtered view of clips.
    fn filtered(&self, clips: &[Clip]) -> Vec<ClipId> {
        let results: Vec<SearchResult<'_>> = search(clips, &self.query);
        results.into_iter().map(|r| r.clip.id).collect()
    }
}

impl eframe::App for PopupApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Transparent so the borderless window blends; egui draws its own panel.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Light/dark follows the system theme via `NativeOptions` in the app
        // wiring; nothing to set per-frame here.

        // 1. Check for a show request from the wiring.
        let (clips, paused, show_requested, revision) = {
            let mut s = self.state.lock().unwrap();
            let show = std::mem::take(&mut s.show_requested);
            (s.clips.clone(), s.paused, show, s.revision)
        };

        if show_requested {
            self.show(ctx);
        }

        // Reset selection when the underlying data changes, and prune any
        // thumbnail textures for clips that no longer exist so the cache
        // cannot grow without bound over the life of the process.
        if revision != self.last_revision {
            self.last_revision = revision;
            self.selected = 0;
            let live_ids: HashSet<String> = clips.iter().map(|c| c.id.to_string_repr()).collect();
            self.thumbnails.retain_only(&live_ids);
        }

        if !self.visible {
            // Nothing to draw; keep the window hidden.
            return;
        }

        // 2. Hide on focus loss (after a short grace period post-show).
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let grace_elapsed = self
            .shown_at
            .map(|t| t.elapsed().as_millis() > 250)
            .unwrap_or(true);
        if !focused && grace_elapsed {
            self.actions.push_back(UiAction::Hide);
            self.hide(ctx);
            return;
        }

        // 3. Compute the filtered list.
        let filtered = self.filtered(&clips);
        let total = filtered.len();
        if total == 0 {
            self.selected = 0;
        } else if self.selected >= total {
            self.selected = total - 1;
        }

        // 4. Global key handling (crate::input).
        self.handle_keys(ctx, &filtered, total);

        // If Esc requested a hide, do it now.
        if self.actions.iter().any(|a| *a == UiAction::Hide) {
            self.hide(ctx);
        }

        // 5. Render the panel (crate::render).
        let clip_by_id: HashMap<ClipId, &Clip> = clips.iter().map(|c| (c.id, c)).collect();
        self.render_panel(ctx, paused, total, &filtered, &clip_by_id);

        // Keep repainting while visible so focus/typing feels live.
        ctx.request_repaint();
    }
}
