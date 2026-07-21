//! The eframe popup application.
//!
//! Renders a borderless, always-on-top popup: a search box at the top that
//! filters as you type, and a virtualized results list below. Keyboard-driven:
//! Up/Down to move the selection, Enter to paste the selected clip, Esc to hide,
//! and Cmd/Ctrl+1..9 to quick-pick the first nine rows.
//!
//! The app does not perform side effects itself. It pushes [`UiAction`]s into a
//! queue, which the wiring drains each frame via [`PopupApp::take_actions`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Cursor;
use std::time::Duration;

use chrono::Utc;
use egui::{Color32, Key, RichText, TextureHandle, ViewportCommand};
use vbuff_core::compose::{MergeTemplate, PasteStack, PasteStackItemId, merge_text};
use vbuff_core::feedback::FeedbackEnvironment;
use vbuff_core::onboarding::DefaultProfile;
use vbuff_core::workflow::{
    TextTransform, TransformOverlay, clean_link, expiry_label, recent_source_apps,
    stale_pin_candidates,
};
use vbuff_core::{SearchResult, search};
use vbuff_types::{
    Body, CapabilityView, CapabilityViewLevel, CaptureBudgetAlert, CaptureHealth,
    CapturePauseReason, Clip, ClipId, ClipboardHealthDigest, CommandNotice, ContentKind,
    NoticeLevel, PrivacyDecisionLevel, PrivacyLedgerSummary, SecurityPostureLevel,
    SecurityPostureSummary, SloMetricState, SloStatusSummary,
};
use web_time::Instant;

use crate::design::{self, Icon};
use crate::experience::{
    ClipBadge, DensityMode, FocusLossGuard, FocusLossState, HandedMode, HistoryScope, MotionBudget,
    NearDuplicateDelta, ScrollTuner, UiPreferences, clip_badges, contextual_search_hint,
    contrast_ratio, match_highlight_alpha, recency_strength,
};
use crate::state::{SharedState, StarterPack, UiAction};
use crate::view::{relative_time, short_app_name};

const MAX_THUMBNAIL_DIMENSION: u32 = 16_384;
const MAX_THUMBNAIL_RGBA_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PopupSurface {
    History,
    Compose,
    Trust,
    Settings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PreviewTransform {
    #[default]
    Original,
    Trim,
    Uppercase,
    PrettyJson,
    CleanLink,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ColorOutputFormat {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UndoAction {
    Pin {
        id: ClipId,
        previous: bool,
    },
    Stack {
        clip_id: ClipId,
        item_id: PasteStackItemId,
    },
    Delete(Box<Clip>),
}

#[derive(Clone, Debug)]
struct UndoSlot {
    action: UndoAction,
    expires_at: Instant,
}

#[derive(Clone, Copy, Debug)]
struct FilteredClip {
    id: ClipId,
    score: i64,
    duplicate_delta: Option<NearDuplicateDelta>,
    hidden_variants: usize,
    variant_of: Option<ClipId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteCommand {
    History,
    Compose,
    Trust,
    Settings,
    PasteSelected,
    PinSelected,
    AddSelectedToStack,
    PeekSelected,
    ToggleCapture,
    TogglePreview,
    ToggleMotionInspector,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ComposeMode {
    #[default]
    Stack,
    Form,
    Merge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackRowAction {
    Up(PasteStackItemId),
    Down(PasteStackItemId),
    Duplicate(PasteStackItemId),
    Delete(PasteStackItemId),
}

/// The eframe application driving the popup.
pub struct PopupApp {
    state: SharedState,
    /// Current search query.
    query: String,
    /// Index of the selected row within the filtered results.
    selected: usize,
    /// Pending actions for the wiring to drain.
    actions: VecDeque<UiAction>,
    /// Whether the popup is currently visible.
    visible: bool,
    /// Track the state revision we last rendered so we can reset selection on
    /// new data.
    last_revision: u64,
    /// Cached image-thumbnail textures, keyed by clip id string.
    thumbnails: std::collections::HashMap<String, Option<TextureHandle>>,
    /// Set when we want the wiring to know we just emitted a Paste so it can
    /// sequence the hide + keystroke. Mirrors the action queue but is simpler
    /// to consume.
    request_focus_next_frame: bool,
    /// Set after global spacing and interaction tokens are installed.
    design_applied: bool,
    /// True while the destructive clear-history confirmation is open.
    confirm_clear_history: bool,
    /// Clip awaiting explicit delete confirmation.
    confirm_delete: Option<ClipId>,
    /// Default profile awaiting an explicit review/apply decision.
    confirm_profile: Option<DefaultProfile>,
    /// Compact history and inspectable trust surfaces share one popup.
    surface: PopupSurface,
    /// Ephemeral, local-only composition scratchpad.
    paste_stack: PasteStack,
    compose_mode: ComposeMode,
    merge_template: MergeTemplate,
    feedback_preview: bool,
    preferences: UiPreferences,
    scroll_tuner: ScrollTuner,
    focus_guard: FocusLossGuard,
    motion_budget: MotionBudget,
    peek_sensitive: Option<(ClipId, Instant)>,
    preview_transform: PreviewTransform,
    history_scope: HistoryScope,
    color_output_format: ColorOutputFormat,
    command_palette_open: bool,
    command_query: String,
    command_focus_next_frame: bool,
    action_flyout: Option<ClipId>,
    undo_slot: Option<UndoSlot>,
    expanded_duplicates: HashSet<ClipId>,
    last_announcement_revision: u64,
    preview_clip_id: Option<ClipId>,
    preview_fade_started: Instant,
}

impl PopupApp {
    /// Construct the popup over shared state, starting hidden.
    pub fn new(state: SharedState) -> Self {
        let now = Instant::now();
        PopupApp {
            state,
            query: String::new(),
            selected: 0,
            actions: VecDeque::new(),
            visible: false,
            last_revision: u64::MAX,
            thumbnails: std::collections::HashMap::new(),
            request_focus_next_frame: false,
            design_applied: false,
            confirm_clear_history: false,
            confirm_delete: None,
            confirm_profile: None,
            surface: PopupSurface::History,
            paste_stack: PasteStack::default(),
            compose_mode: ComposeMode::Stack,
            merge_template: MergeTemplate::Bullets,
            feedback_preview: false,
            preferences: UiPreferences::default(),
            scroll_tuner: ScrollTuner::new(now),
            focus_guard: FocusLossGuard::default(),
            motion_budget: MotionBudget::new(now),
            peek_sensitive: None,
            preview_transform: PreviewTransform::Original,
            history_scope: HistoryScope::All,
            color_output_format: ColorOutputFormat::Hex,
            command_palette_open: false,
            command_query: String::new(),
            command_focus_next_frame: false,
            action_flyout: None,
            undo_slot: None,
            expanded_duplicates: HashSet::new(),
            last_announcement_revision: 0,
            preview_clip_id: None,
            preview_fade_started: now,
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
        self.confirm_clear_history = false;
        self.confirm_delete = None;
        self.confirm_profile = None;
        self.surface = PopupSurface::History;
        self.feedback_preview = false;
        self.command_palette_open = false;
        self.command_query.clear();
        self.command_focus_next_frame = false;
        self.action_flyout = None;
        self.peek_sensitive = None;
        self.focus_guard.reset();
        self.request_focus_next_frame = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Open the popup directly on the clear-history confirmation.
    pub fn request_clear_history_confirmation(&mut self, ctx: &egui::Context) {
        self.show(ctx);
        self.confirm_clear_history = true;
    }

    /// Open the popup directly on its trust surface.
    pub fn request_trust_view(&mut self, ctx: &egui::Context) {
        self.show(ctx);
        self.surface = PopupSurface::Trust;
        self.request_focus_next_frame = false;
    }

    /// Open the popup directly on its settings surface.
    pub fn request_settings_view(&mut self, ctx: &egui::Context) {
        self.show(ctx);
        self.surface = PopupSurface::Settings;
        self.request_focus_next_frame = false;
    }

    /// Apply the persisted health-digest visibility preference.
    pub fn set_health_digest_visible(&mut self, visible: bool) {
        self.preferences.show_health_digest = visible;
    }

    /// Open the popup directly on the composition scratchpad.
    pub fn request_compose_view(&mut self, ctx: &egui::Context) {
        self.show(ctx);
        self.surface = PopupSurface::Compose;
        self.request_focus_next_frame = false;
    }

    /// Add one explicit text draft to the local composition scratchpad.
    pub fn add_compose_item(&mut self, label: impl Into<String>, text: impl Into<String>) -> bool {
        self.paste_stack.add(label, text).is_ok()
    }

    /// Hide the popup.
    fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.peek_sensitive = None;
        self.undo_slot = None;
        self.focus_guard.reset();
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    /// Build the current filtered view of clips.
    fn filtered(&self, clips: &[Clip]) -> Vec<FilteredClip> {
        let results: Vec<SearchResult<'_>> = search(clips, &self.query)
            .into_iter()
            .filter(|result| self.history_scope.matches(result.clip))
            .collect();
        let mut filtered: Vec<FilteredClip> = Vec::with_capacity(results.len());
        let mut root: Option<(ClipId, vbuff_types::ContentKind, String)> = None;
        for result in results {
            let text = (!result.clip.meta.sensitive)
                .then(|| result.clip.primary_text())
                .flatten()
                .map(str::to_owned);
            let duplicate = root.as_ref().and_then(|(root_id, kind, root_text)| {
                (result.clip.meta.kind == *kind)
                    .then_some(text.as_deref())
                    .flatten()
                    .and_then(|text| NearDuplicateDelta::between(text, root_text))
                    .map(|delta| (*root_id, delta))
            });
            if let Some((root_id, delta)) = duplicate {
                if self.expanded_duplicates.contains(&root_id) {
                    if let Some(root_hit) = filtered.iter_mut().rev().find(|hit| hit.id == root_id)
                    {
                        root_hit.hidden_variants = root_hit.hidden_variants.saturating_add(1);
                        root_hit.duplicate_delta.get_or_insert(delta);
                    }
                    filtered.push(FilteredClip {
                        id: result.clip.id,
                        score: result.score,
                        duplicate_delta: Some(delta),
                        hidden_variants: 0,
                        variant_of: Some(root_id),
                    });
                } else if let Some(root_hit) =
                    filtered.iter_mut().rev().find(|hit| hit.id == root_id)
                {
                    root_hit.hidden_variants = root_hit.hidden_variants.saturating_add(1);
                    root_hit.duplicate_delta.get_or_insert(delta);
                }
                continue;
            }
            filtered.push(FilteredClip {
                id: result.clip.id,
                score: result.score,
                duplicate_delta: None,
                hidden_variants: 0,
                variant_of: None,
            });
            root = text.map(|text| (result.clip.id, result.clip.meta.kind, text));
        }
        filtered
    }
}

impl eframe::App for PopupApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Transparent so the borderless window blends; egui draws its own panel.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.motion_budget.begin_frame(now);
        let scroll_delta = ctx.input(|input| input.smooth_scroll_delta.y);
        self.scroll_tuner.sample(scroll_delta, now);
        if !self.design_applied {
            design::apply(ctx);
            self.design_applied = true;
        }

        // 1. Check for a show request from the wiring.
        let (
            clips,
            paused,
            pause_reason,
            capture_health,
            capture_stats,
            health_alert,
            size_budget_alert,
            health_digest,
            session_protected,
            default_profile,
            security_posture,
            capabilities,
            privacy_ledger,
            slo_status,
            recoverable_skip,
            notice,
            accessibility_announcement,
            announcement_revision,
            hotkey_label,
            show_hotkey_coachmark,
            show_requested,
            revision,
        ) = {
            let Ok(mut s) = self.state.lock() else {
                tracing::error!("GUI state mutex poisoned");
                return;
            };
            let show = std::mem::take(&mut s.show_requested);
            #[cfg(not(target_arch = "wasm32"))]
            let recoverable_skip = s.skipped_recovery_available(std::time::Instant::now());
            #[cfg(target_arch = "wasm32")]
            let recoverable_skip = false;
            (
                s.clips.clone(),
                s.paused,
                s.pause_reason,
                s.capture_health,
                s.capture_stats,
                s.health_alert,
                s.size_budget_alert,
                s.health_digest,
                s.session_protected.clone(),
                s.default_profile,
                s.security_posture,
                s.capabilities.clone(),
                s.privacy_ledger.clone(),
                s.slo_status.clone(),
                recoverable_skip,
                s.notice.clone(),
                s.accessibility_announcement.clone(),
                s.announcement_revision,
                s.hotkey_label.clone(),
                s.show_hotkey_coachmark,
                show,
                s.revision,
            )
        };

        if show_requested {
            self.show(ctx);
        }

        // Reset selection when the underlying data changes.
        if revision != self.last_revision {
            self.last_revision = revision;
            self.selected = 0;
            let live_ids = clips
                .iter()
                .filter(|clip| !clip.meta.sensitive)
                .map(|clip| clip.id.to_string_repr())
                .collect::<std::collections::HashSet<_>>();
            self.thumbnails.retain(|id, _| live_ids.contains(id));
        }

        if !self.visible {
            self.render_accessibility_announcement(
                ctx,
                accessibility_announcement.as_deref(),
                announcement_revision,
            );
            return;
        }

        // 2. Hide only after a recoverable focus-loss grace period.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let focus_loss = self.focus_guard.update(focused, now);
        if focus_loss == FocusLossState::Expired {
            self.actions.push_back(UiAction::Hide);
            self.hide(ctx);
            return;
        }
        let focus_grace_fraction = match focus_loss {
            FocusLossState::Grace {
                fraction,
                remaining,
            } => {
                ctx.request_repaint_after(remaining.min(Duration::from_millis(16)));
                Some(fraction)
            }
            FocusLossState::Focused | FocusLossState::Expired => None,
        };

        // 3. Compute the filtered list.
        let filtered = self.filtered(&clips);
        let total = filtered.len();
        if total == 0 {
            self.selected = 0;
        } else if self.selected >= total {
            self.selected = total - 1;
        }
        if self
            .peek_sensitive
            .is_some_and(|(_, expires_at)| now >= expires_at)
        {
            self.peek_sensitive = None;
        }
        if self
            .undo_slot
            .as_ref()
            .is_some_and(|undo| now >= undo.expires_at)
        {
            self.undo_slot = None;
        }

        // 4. Global key handling.
        let modifier_down = ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
        if !self.confirm_clear_history
            && self.confirm_delete.is_none()
            && self.confirm_profile.is_none()
        {
            ctx.input(|i| {
                if i.key_pressed(Key::Escape) {
                    if self.command_palette_open {
                        self.command_palette_open = false;
                    } else {
                        let closed_flyout = self.action_flyout.take().is_some();
                        if !closed_flyout
                            && self.surface == PopupSurface::History
                            && (self.history_scope != HistoryScope::All
                                || !self.query.trim().is_empty())
                        {
                            self.history_scope = HistoryScope::All;
                            self.query.clear();
                            self.selected = 0;
                        } else if !closed_flyout {
                            self.actions.push_back(UiAction::Hide);
                        }
                    }
                }
                if modifier_down && i.key_pressed(Key::K) {
                    self.command_palette_open = true;
                    self.command_query.clear();
                    self.command_focus_next_frame = true;
                }
                if modifier_down && i.key_pressed(Key::Comma) {
                    self.surface = PopupSurface::Settings;
                    self.request_focus_next_frame = false;
                }
                if i.modifiers.shift
                    && i.key_pressed(Key::F10)
                    && let Some(hit) = filtered.get(self.selected)
                {
                    self.action_flyout = Some(hit.id);
                }
                if self.surface == PopupSurface::History
                    && !self.command_palette_open
                    && self.action_flyout.is_none()
                    && i.key_pressed(Key::ArrowDown)
                    && total > 0
                {
                    self.selected = (self.selected + 1).min(total - 1);
                }
                if self.surface == PopupSurface::History && i.key_pressed(Key::ArrowUp) && total > 0
                {
                    self.selected = self.selected.saturating_sub(1);
                }
                if self.surface == PopupSurface::History
                    && i.key_pressed(Key::Enter)
                    && total > 0
                    && let Some(hit) = filtered.get(self.selected)
                {
                    self.actions.push_back(UiAction::Paste(hit.id));
                }
                // Cmd/Ctrl + 1..9 quick select.
                if self.surface == PopupSurface::History && modifier_down {
                    for (n, key) in [
                        (1, Key::Num1),
                        (2, Key::Num2),
                        (3, Key::Num3),
                        (4, Key::Num4),
                        (5, Key::Num5),
                        (6, Key::Num6),
                        (7, Key::Num7),
                        (8, Key::Num8),
                        (9, Key::Num9),
                    ] {
                        if i.key_pressed(key)
                            && let Some(hit) = filtered.get(n - 1)
                        {
                            self.actions.push_back(UiAction::Paste(hit.id));
                        }
                    }
                }
                if self.surface == PopupSurface::History && i.modifiers.alt && total > 0 {
                    let (previous, paste, next) = match self.preferences.handed_mode {
                        HandedMode::Left => (Key::Q, Key::W, Key::E),
                        HandedMode::Right => (Key::I, Key::O, Key::P),
                        HandedMode::Off => (Key::F20, Key::F20, Key::F20),
                    };
                    if i.key_pressed(previous) {
                        self.selected = self.selected.saturating_sub(1);
                    }
                    if i.key_pressed(next) {
                        self.selected = (self.selected + 1).min(total - 1);
                    }
                    if i.key_pressed(paste)
                        && let Some(hit) = filtered.get(self.selected)
                    {
                        self.actions.push_back(UiAction::Paste(hit.id));
                    }
                }
            });
        }

        // If Esc requested a hide, do it now.
        if self.actions.iter().any(|a| *a == UiAction::Hide) {
            self.hide(ctx);
        }

        // 5. Render the panel.
        let clip_by_id: HashMap<ClipId, &Clip> = clips.iter().map(|c| (c.id, c)).collect();
        let selected_clip = filtered
            .get(self.selected)
            .and_then(|hit| clip_by_id.get(&hit.id))
            .copied();
        let selected_id = selected_clip.map(|clip| clip.id);
        if selected_id != self.preview_clip_id {
            self.preview_clip_id = selected_id;
            self.preview_fade_started = now;
            self.preview_transform = PreviewTransform::Original;
        }
        let viewport = logical_viewport_size(ctx);
        let wide_preview = self.preferences.large_preview
            && self.surface == PopupSurface::History
            && viewport.x >= 720.0;
        if wide_preview && let Some(clip) = selected_clip {
            egui::SidePanel::right("large_clip_preview")
                .exact_width(300.0)
                .resizable(false)
                .show(ctx, |ui| self.render_preview_pane(ui, ctx, clip));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(fraction) = focus_grace_fraction {
                ui.set_opacity(0.62 + 0.38 * fraction);
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width(ui.available_width())
                        .desired_height(2.0),
                );
            }
            self.render_surface_header(ui, paused, &clips);

            if show_hotkey_coachmark && let Some(hotkey) = hotkey_label.as_deref() {
                self.render_hotkey_coachmark(ui, hotkey);
            }

            match self.surface {
                PopupSurface::History => {
                    ui.horizontal(|ui| {
                        render_capture_status(ui, paused, pause_reason, capture_health);
                        ui.separator();
                        render_security_status(ui, security_posture);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.undo_slot.is_some()
                                && design::icon_button(ui, Icon::Undo, "Undo last action", false)
                                    .clicked()
                            {
                                self.apply_undo();
                            }
                            ui.add_enabled_ui(!clips.is_empty(), |ui| {
                                if design::icon_button(ui, Icon::Delete, "Clear history", false)
                                    .clicked()
                                {
                                    self.confirm_clear_history = true;
                                    self.confirm_delete = None;
                                }
                            });
                            let (pause_icon, pause_tooltip) = if paused {
                                (Icon::Resume, "Resume capture")
                            } else {
                                (Icon::Pause, "Pause capture")
                            };
                            if design::icon_button(ui, pause_icon, pause_tooltip, paused).clicked()
                            {
                                self.actions.push_back(UiAction::TogglePause);
                            }
                        });
                    });
                    if let Some(health) = health_alert {
                        self.render_health_alert(ui, health);
                    }
                    if let Some(alert) = size_budget_alert {
                        self.render_size_budget_alert(ui, alert);
                    }
                    if recoverable_skip {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Current copy was skipped")
                                    .small()
                                    .color(Color32::from_rgb(210, 144, 32)),
                            );
                            if ui.small_button("Keep current copy").clicked() {
                                self.actions.push_back(UiAction::RecoverSkipped);
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.small(format!("{total} items"));
                            ui.separator();
                            ui.small(format!(
                                "{} saved · {} skipped",
                                compact_count(capture_stats.captured),
                                compact_count(capture_stats.intentionally_skipped)
                            ));
                            let loss_color = if capture_stats.lost == 0 {
                                ui.visuals().weak_text_color()
                            } else {
                                Color32::from_rgb(194, 64, 72)
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{} lost",
                                    compact_count(capture_stats.lost)
                                ))
                                .small()
                                .color(loss_color),
                            );
                        });
                    }
                    if let Some(notice) = &notice {
                        self.render_notice(ui, notice);
                    }
                    self.render_history_filters(ui, &clips);
                    ui.separator();

                    if total == 0 {
                        self.render_empty_history(ui, clips.is_empty());
                    } else {
                        // Stable-height virtualized rows keep controls from shifting.
                        let row_height = self
                            .preferences
                            .density
                            .row_height(viewport.y, ctx.pixels_per_point());
                        let cheap_rows = self.scroll_tuner.rapid();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show_rows(ui, row_height, total, |ui, row_range| {
                                for row in row_range {
                                    let Some(hit) = filtered.get(row) else {
                                        continue;
                                    };
                                    let Some(clip) = clip_by_id.get(&hit.id) else {
                                        continue;
                                    };
                                    let selected = row == self.selected;
                                    self.render_row(
                                        ui,
                                        ctx,
                                        row,
                                        clip,
                                        *hit,
                                        selected,
                                        cheap_rows,
                                        modifier_down,
                                        row_height,
                                        session_protected.contains(&clip.id),
                                    );
                                }
                            });
                    }
                }
                PopupSurface::Compose => self.render_compose_surface(ui),
                PopupSurface::Trust => self.render_trust_surface(
                    ui,
                    security_posture,
                    &capabilities,
                    &privacy_ledger,
                    &slo_status,
                ),
                PopupSurface::Settings => {
                    self.render_settings_surface(ui, &clips, health_digest, default_profile)
                }
            }
        });

        self.render_clear_history_confirmation(ctx);
        self.render_delete_confirmation(ctx);
        self.render_profile_confirmation(ctx, health_digest.stored_items);
        self.render_feedback_preview(ctx, &capabilities);
        self.render_command_palette(ctx, paused, &filtered, &clip_by_id);
        self.render_action_flyout(ctx, &clip_by_id, &session_protected);
        self.render_accessibility_announcement(
            ctx,
            accessibility_announcement.as_deref(),
            announcement_revision,
        );
        self.render_motion_inspector(ctx);

        // Input events repaint immediately; one low-frequency visible refresh
        // keeps expiry labels and background capture state current.
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

impl PopupApp {
    fn render_surface_header(&mut self, ui: &mut egui::Ui, paused: bool, clips: &[Clip]) {
        ui.horizontal(|ui| match self.surface {
            PopupSurface::History => {
                let hint = if paused {
                    "Search paused history..."
                } else {
                    contextual_search_hint(clips)
                };
                let search_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 3.0 - 24.0).max(160.0);
                let edit = egui::TextEdit::singleline(&mut self.query).hint_text(hint);
                let response = ui.add_sized([search_width, design::ICON_BUTTON_SIZE], edit);
                if self.request_focus_next_frame {
                    response.request_focus();
                    self.request_focus_next_frame = false;
                } else if !response.has_focus() && self.actions.is_empty() {
                    response.request_focus();
                }
                if design::icon_button(ui, Icon::Shield, "Trust and privacy", false).clicked() {
                    self.surface = PopupSurface::Trust;
                }
                if design::icon_button(ui, Icon::Compose, "Compose clips", false).clicked() {
                    self.surface = PopupSurface::Compose;
                }
                self.render_action_menu(ui, paused);
            }
            PopupSurface::Compose => {
                let title_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 3.0 - 24.0).max(160.0);
                ui.add_sized(
                    [title_width, design::ICON_BUTTON_SIZE],
                    egui::Label::new(RichText::new("Compose").strong()),
                );
                if design::icon_button(ui, Icon::Shield, "Trust and privacy", false).clicked() {
                    self.surface = PopupSurface::Trust;
                }
                if design::icon_button(ui, Icon::History, "Clipboard history", false).clicked() {
                    self.surface = PopupSurface::History;
                    self.request_focus_next_frame = true;
                }
                self.render_action_menu(ui, paused);
            }
            PopupSurface::Trust => {
                let title_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 4.0 - 32.0).max(160.0);
                ui.add_sized(
                    [title_width, design::ICON_BUTTON_SIZE],
                    egui::Label::new(RichText::new("Trust and privacy").strong()),
                );
                if design::icon_button(ui, Icon::History, "Clipboard history", false).clicked() {
                    self.surface = PopupSurface::History;
                    self.request_focus_next_frame = true;
                }
                if design::icon_button(ui, Icon::Compose, "Compose clips", false).clicked() {
                    self.surface = PopupSurface::Compose;
                }
                if design::icon_button(ui, Icon::Feedback, "Preview feedback report", false)
                    .clicked()
                {
                    self.feedback_preview = true;
                }
                self.render_action_menu(ui, paused);
            }
            PopupSurface::Settings => {
                let title_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 3.0 - 24.0).max(160.0);
                ui.add_sized(
                    [title_width, design::ICON_BUTTON_SIZE],
                    egui::Label::new(RichText::new("Settings").strong()),
                );
                if design::icon_button(ui, Icon::History, "Clipboard history", false).clicked() {
                    self.surface = PopupSurface::History;
                    self.request_focus_next_frame = true;
                }
                if design::icon_button(ui, Icon::Shield, "Trust and privacy", false).clicked() {
                    self.surface = PopupSurface::Trust;
                }
                self.render_action_menu(ui, paused);
            }
        });
    }

    fn render_action_menu(&mut self, ui: &mut egui::Ui, paused: bool) {
        let response = design::icon_button(ui, Icon::Menu, "Actions", false);
        let _ = egui::Popup::menu(&response).width(190.0).show(|ui| {
            if ui.button("Command palette").clicked() {
                self.command_palette_open = true;
                self.command_query.clear();
                self.command_focus_next_frame = true;
                ui.close();
            }
            if ui.button("Settings").clicked() {
                self.surface = PopupSurface::Settings;
                ui.close();
            }
            if ui.button("Trust and privacy").clicked() {
                self.surface = PopupSurface::Trust;
                ui.close();
            }
            if ui
                .button(if paused {
                    "Resume capture"
                } else {
                    "Pause capture"
                })
                .clicked()
            {
                self.actions.push_back(UiAction::TogglePause);
                ui.close();
            }
            ui.separator();
            if ui
                .checkbox(&mut self.preferences.large_preview, "Large preview")
                .changed()
            {
                ui.close();
            }
        });
    }

    fn render_history_filters(&mut self, ui: &mut egui::Ui, clips: &[Clip]) {
        if clips.is_empty() {
            return;
        }
        let recent_apps = recent_source_apps(clips, 3);
        ui.horizontal_wrapped(|ui| {
            let previous_scope = self.history_scope.clone();
            egui::ComboBox::from_id_salt("history_kind_scope")
                .selected_text(self.history_scope.label())
                .width(116.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.history_scope, HistoryScope::All, "All kinds");
                    for kind in [
                        ContentKind::Url,
                        ContentKind::Image,
                        ContentKind::Code,
                        ContentKind::File,
                        ContentKind::Color,
                        ContentKind::Text,
                    ] {
                        ui.selectable_value(
                            &mut self.history_scope,
                            HistoryScope::Kind(kind),
                            kind.label(),
                        );
                    }
                    ui.selectable_value(
                        &mut self.history_scope,
                        HistoryScope::Snippets,
                        "Snippets",
                    );
                });
            if self.history_scope != previous_scope {
                self.selected = 0;
            }
            for app in recent_apps {
                let selected = self.history_scope == HistoryScope::Source(app.clone());
                if ui
                    .selectable_label(selected, short_app_name(&app))
                    .clicked()
                {
                    self.history_scope = if selected {
                        HistoryScope::All
                    } else {
                        HistoryScope::Source(app)
                    };
                    self.selected = 0;
                }
            }
            if self.history_scope != HistoryScope::All
                && design::icon_button(ui, Icon::Close, "Clear history filter", false).clicked()
            {
                self.history_scope = HistoryScope::All;
                self.selected = 0;
            }
        });
    }

    fn render_health_alert(&mut self, ui: &mut egui::Ui, health: CaptureHealth) {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(194, 64, 72, 24))
            .inner_margin(5.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    design::status_dot(ui, Color32::from_rgb(194, 64, 72));
                    ui.label(RichText::new(health.label()).small().strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if design::icon_button(ui, Icon::Close, "Dismiss capture alert", false)
                            .clicked()
                        {
                            self.actions.push_back(UiAction::DismissHealthAlert);
                        }
                        if ui.small_button("Review").clicked() {
                            self.surface = PopupSurface::Trust;
                            self.actions.push_back(UiAction::DismissHealthAlert);
                        }
                    });
                });
            });
    }

    fn render_size_budget_alert(&mut self, ui: &mut egui::Ui, alert: CaptureBudgetAlert) {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(210, 144, 32, 24))
            .inner_margin(5.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    design::status_dot(ui, Color32::from_rgb(210, 144, 32));
                    ui.label(RichText::new(alert.label()).small().strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if design::icon_button(ui, Icon::Close, "Dismiss size-budget alert", false)
                            .clicked()
                        {
                            self.actions.push_back(UiAction::DismissSizeBudgetAlert);
                        }
                        if ui.small_button("Settings").clicked() {
                            self.surface = PopupSurface::Settings;
                            self.actions.push_back(UiAction::DismissSizeBudgetAlert);
                        }
                    });
                });
            });
    }

    fn render_hotkey_coachmark(&mut self, ui: &mut egui::Ui, hotkey: &str) {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    design::status_dot(ui, Color32::from_rgb(45, 126, 183));
                    ui.label(RichText::new("Summon key").small().strong());
                    ui.label(RichText::new(hotkey).small().monospace());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if design::icon_button(ui, Icon::Close, "Dismiss", false).clicked() {
                            self.actions.push_back(UiAction::DismissHotkeyCoachmark);
                        }
                    });
                });
            });
    }

    fn render_settings_surface(
        &mut self,
        ui: &mut egui::Ui,
        clips: &[Clip],
        digest: ClipboardHealthDigest,
        default_profile: Option<DefaultProfile>,
    ) {
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Defaults");
                ui.horizontal(|ui| {
                    for profile in [
                        DefaultProfile::Casual,
                        DefaultProfile::Developer,
                        DefaultProfile::PrivacyMax,
                    ] {
                        if ui
                            .selectable_label(default_profile == Some(profile), profile.label())
                            .clicked()
                            && default_profile != Some(profile)
                        {
                            self.confirm_profile = Some(profile);
                        }
                    }
                });

                ui.add_space(12.0);
                ui.heading("Clipboard health");
                ui.checkbox(
                    &mut self.preferences.show_health_digest,
                    "Show health digest",
                );
                if self.preferences.show_health_digest {
                    egui::Grid::new("clipboard_health_digest")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            digest_row(ui, "Database", human_bytes(digest.database_bytes));
                            digest_row(ui, "Items", digest.stored_items.to_string());
                            digest_row(ui, "Largest clip", human_bytes(digest.largest_clip_bytes));
                            digest_row(
                                ui,
                                "Expiring this week",
                                digest.expiring_within_week.to_string(),
                            );
                            digest_row(ui, "Sensitive", digest.sensitive_items.to_string());
                            digest_row(ui, "Suggested pins", digest.suggested_pins.to_string());
                            digest_row(ui, "Stale pins", digest.stale_pins.to_string());
                        });
                }

                ui.add_space(12.0);
                ui.heading("Pinned items to review");
                let stale = stale_pin_candidates(
                    clips,
                    Utc::now(),
                    std::time::Duration::from_secs(90 * 24 * 60 * 60),
                    5,
                );
                if stale.is_empty() {
                    ui.label(RichText::new("No pins older than 90 days").small().weak());
                } else {
                    for candidate in stale {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Pinned item · {}d · {}",
                                    candidate.age_days,
                                    human_bytes(candidate.byte_size)
                                ))
                                .small(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if design::icon_button(
                                        ui,
                                        Icon::Delete,
                                        "Delete stale pin",
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.confirm_delete = Some(candidate.clip_id);
                                    }
                                    if design::icon_button(
                                        ui,
                                        Icon::Pin { filled: true },
                                        "Unpin stale item",
                                        true,
                                    )
                                    .clicked()
                                    {
                                        self.actions.push_back(UiAction::SetPinned(
                                            candidate.clip_id,
                                            false,
                                        ));
                                    }
                                },
                            );
                        });
                    }
                }

                ui.add_space(12.0);
                ui.heading("Appearance");
                ui.horizontal(|ui| {
                    ui.label("Density");
                    ui.selectable_value(&mut self.preferences.density, DensityMode::Auto, "Auto");
                    ui.selectable_value(
                        &mut self.preferences.density,
                        DensityMode::Compact,
                        "Compact",
                    );
                    ui.selectable_value(
                        &mut self.preferences.density,
                        DensityMode::Comfortable,
                        "Comfortable",
                    );
                });
                ui.checkbox(&mut self.preferences.large_preview, "Large preview pane");
                ui.checkbox(&mut self.preferences.reduced_motion, "Reduced motion");

                ui.add_space(12.0);
                ui.heading("Ergonomics");
                ui.horizontal(|ui| {
                    ui.label("One-handed");
                    ui.selectable_value(&mut self.preferences.handed_mode, HandedMode::Off, "Off");
                    ui.selectable_value(
                        &mut self.preferences.handed_mode,
                        HandedMode::Left,
                        "Left",
                    );
                    ui.selectable_value(
                        &mut self.preferences.handed_mode,
                        HandedMode::Right,
                        "Right",
                    );
                });
                ui.checkbox(
                    &mut self.preferences.motion_inspector,
                    "Motion budget inspector",
                );

                ui.add_space(12.0);
                ui.heading("Contrast audit");
                let foreground = ui.visuals().text_color();
                let background = ui.visuals().panel_fill;
                let ratio = contrast_ratio(
                    [foreground.r(), foreground.g(), foreground.b()],
                    [background.r(), background.g(), background.b()],
                );
                let (label, color) = if ratio >= 7.0 {
                    ("AAA", Color32::from_rgb(28, 126, 82))
                } else if ratio >= 4.5 {
                    ("AA", Color32::from_rgb(45, 105, 174))
                } else {
                    ("FAIL", Color32::from_rgb(194, 48, 58))
                };
                ui.horizontal(|ui| {
                    design::status_dot(ui, color);
                    ui.label(RichText::new(label).strong().color(color));
                    ui.label(RichText::new(format!("{ratio:.1}:1")).monospace());
                });

                ui.add_space(12.0);
                ui.heading("Text gallery");
                egui::Grid::new("multilingual_sample_gallery")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for sample in crate::experience::MULTILINGUAL_SAMPLES {
                            ui.label(RichText::new(sample.language).small().weak());
                            ui.label(sample.text);
                            ui.end_row();
                        }
                    });
            });
    }

    fn render_command_palette(
        &mut self,
        ctx: &egui::Context,
        paused: bool,
        filtered: &[FilteredClip],
        clip_by_id: &HashMap<ClipId, &Clip>,
    ) {
        if !self.command_palette_open {
            return;
        }
        let entries = [
            (PaletteCommand::History, "Clipboard history"),
            (PaletteCommand::Compose, "Compose clips"),
            (PaletteCommand::Trust, "Trust and diagnostics"),
            (PaletteCommand::Settings, "Settings"),
            (PaletteCommand::PasteSelected, "Paste selected clip"),
            (PaletteCommand::PinSelected, "Pin selected clip"),
            (
                PaletteCommand::AddSelectedToStack,
                "Add selected clip to stack",
            ),
            (PaletteCommand::PeekSelected, "Peek selected clip"),
            (
                PaletteCommand::ToggleCapture,
                if paused {
                    "Resume capture"
                } else {
                    "Pause capture"
                },
            ),
            (PaletteCommand::TogglePreview, "Toggle large preview"),
            (
                PaletteCommand::ToggleMotionInspector,
                "Toggle motion inspector",
            ),
        ];
        let mut chosen = None;
        let response = egui::Modal::new(egui::Id::new("vbuff_command_palette")).show(ctx, |ui| {
            ui.set_width(420.0);
            let search = ui.add_sized(
                [ui.available_width(), 32.0],
                egui::TextEdit::singleline(&mut self.command_query).hint_text("Command"),
            );
            if self.command_focus_next_frame {
                search.request_focus();
                self.command_focus_next_frame = false;
            }
            ui.separator();
            let query = self.command_query.trim().to_lowercase();
            for (command, label) in entries {
                if (query.is_empty() || label.to_lowercase().contains(&query))
                    && ui.selectable_label(false, label).clicked()
                {
                    chosen = Some(command);
                }
            }
        });
        if let Some(command) = chosen {
            self.execute_palette_command(command, filtered, clip_by_id);
            self.command_palette_open = false;
        } else if response.should_close() {
            self.command_palette_open = false;
        }
    }

    fn execute_palette_command(
        &mut self,
        command: PaletteCommand,
        filtered: &[FilteredClip],
        clip_by_id: &HashMap<ClipId, &Clip>,
    ) {
        let selected = filtered.get(self.selected).map(|hit| hit.id);
        match command {
            PaletteCommand::History => {
                self.surface = PopupSurface::History;
                self.request_focus_next_frame = true;
            }
            PaletteCommand::Compose => self.surface = PopupSurface::Compose,
            PaletteCommand::Trust => self.surface = PopupSurface::Trust,
            PaletteCommand::Settings => self.surface = PopupSurface::Settings,
            PaletteCommand::PasteSelected => {
                if let Some(id) = selected {
                    self.actions.push_back(UiAction::Paste(id));
                }
            }
            PaletteCommand::PinSelected => {
                if let Some(id) = selected
                    && let Some(clip) = clip_by_id.get(&id)
                {
                    self.actions
                        .push_back(UiAction::SetPinned(id, !clip.pinned));
                    self.undo_slot = Some(UndoSlot {
                        action: UndoAction::Pin {
                            id,
                            previous: clip.pinned,
                        },
                        expires_at: Instant::now() + Duration::from_secs(5),
                    });
                }
            }
            PaletteCommand::AddSelectedToStack => {
                if let Some(id) = selected
                    && let Some(clip) = clip_by_id.get(&id)
                    && !clip.meta.sensitive
                    && let Some(text) = clip.primary_text()
                    && let Ok(item_id) = self.paste_stack.add(clip.meta.kind.label(), text)
                {
                    self.undo_slot = Some(UndoSlot {
                        action: UndoAction::Stack {
                            clip_id: id,
                            item_id,
                        },
                        expires_at: Instant::now() + Duration::from_secs(5),
                    });
                }
            }
            PaletteCommand::PeekSelected => {
                if let Some(id) = selected
                    && clip_by_id.get(&id).is_some_and(|clip| clip.meta.sensitive)
                {
                    self.peek_sensitive = Some((id, Instant::now() + Duration::from_secs(2)));
                }
            }
            PaletteCommand::ToggleCapture => self.actions.push_back(UiAction::TogglePause),
            PaletteCommand::TogglePreview => {
                self.preferences.large_preview = !self.preferences.large_preview;
            }
            PaletteCommand::ToggleMotionInspector => {
                self.preferences.motion_inspector = !self.preferences.motion_inspector;
            }
        }
    }

    fn render_action_flyout(
        &mut self,
        ctx: &egui::Context,
        clip_by_id: &HashMap<ClipId, &Clip>,
        session_protected: &HashSet<ClipId>,
    ) {
        let Some(id) = self.action_flyout else {
            return;
        };
        let Some(clip) = clip_by_id.get(&id).copied() else {
            self.action_flyout = None;
            return;
        };
        let protected = session_protected.contains(&id);
        let mut command = None;
        let mut request_delete = false;
        ctx.input(|input| {
            if input.key_pressed(Key::Enter) {
                command = Some(PaletteCommand::PasteSelected);
            } else if input.key_pressed(Key::P) {
                command = Some(PaletteCommand::PinSelected);
            } else if input.key_pressed(Key::D) {
                request_delete = true;
            } else if input.key_pressed(Key::A) {
                command = Some(PaletteCommand::AddSelectedToStack);
            } else if input.key_pressed(Key::V) {
                command = Some(PaletteCommand::TogglePreview);
            } else if input.key_pressed(Key::T) {
                self.preview_transform = PreviewTransform::Trim;
                self.preferences.large_preview = true;
                self.action_flyout = None;
            }
        });
        let response = egui::Modal::new(egui::Id::new("row_action_flyout")).show(ctx, |ui| {
            ui.set_width(240.0);
            ui.heading("Actions");
            if ui.button("Paste").clicked() {
                command = Some(PaletteCommand::PasteSelected);
            }
            if ui
                .button(if clip.pinned { "Unpin" } else { "Pin" })
                .clicked()
            {
                command = Some(PaletteCommand::PinSelected);
            }
            if clip.meta.sensitive {
                if ui.button("Peek").clicked() {
                    command = Some(PaletteCommand::PeekSelected);
                }
            } else if clip.primary_text().is_some() && ui.button("Add to stack").clicked() {
                command = Some(PaletteCommand::AddSelectedToStack);
            }
            if clip.primary_text().is_some() && ui.button("Create plain-text clone").clicked() {
                self.actions.push_back(UiAction::CreatePlainTextClone(id));
                self.action_flyout = None;
            }
            if ui
                .button(if protected {
                    "Remove session protection"
                } else {
                    "Protect for this session"
                })
                .clicked()
            {
                self.actions
                    .push_back(UiAction::SetSessionProtected(id, !protected));
                self.action_flyout = None;
            }
            if ui.button("Preview").clicked() {
                self.preferences.large_preview = true;
                self.action_flyout = None;
            }
            if ui.button("Delete").clicked() {
                request_delete = true;
            }
        });

        if request_delete {
            self.confirm_delete = Some(id);
            self.action_flyout = None;
        } else if let Some(command) = command {
            let selected = [FilteredClip {
                id,
                score: 0,
                duplicate_delta: None,
                hidden_variants: 0,
                variant_of: None,
            }];
            let previous_selection = self.selected;
            self.selected = 0;
            self.execute_palette_command(command, &selected, clip_by_id);
            self.selected = previous_selection;
            self.action_flyout = None;
        } else if response.should_close() {
            self.action_flyout = None;
        }
    }

    fn render_accessibility_announcement(
        &mut self,
        ctx: &egui::Context,
        announcement: Option<&str>,
        revision: u64,
    ) {
        if revision == 0 || revision == self.last_announcement_revision {
            return;
        }
        let Some(announcement) = announcement else {
            return;
        };
        ctx.enable_accesskit();
        egui::Area::new(egui::Id::new("paste_live_region"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                ui.set_opacity(0.0);
                let response = ui.label(announcement);
                ctx.accesskit_node_builder(response.id, |node| {
                    node.set_live(egui::accesskit::Live::Polite);
                });
            });
        self.last_announcement_revision = revision;
    }

    fn render_empty_history(&mut self, ui: &mut egui::Ui, history_is_empty: bool) {
        ui.with_layout(
            egui::Layout::top_down_justified(egui::Align::Center),
            |ui| {
                ui.add_space(56.0);
                let truly_empty = history_is_empty && self.query.trim().is_empty();
                ui.label(
                    RichText::new(if truly_empty {
                        "No clipboard history yet"
                    } else {
                        "No matching clips"
                    })
                    .strong(),
                );
                if truly_empty {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Add local examples").small().weak());
                    ui.horizontal(|ui| {
                        const PACK_BUTTON_WIDTH: f32 = 88.0;
                        let row_width = PACK_BUTTON_WIDTH * 2.0 + ui.spacing().item_spacing.x;
                        ui.add_space(((ui.available_width() - row_width) / 2.0).max(0.0));
                        if ui
                            .add_sized([PACK_BUTTON_WIDTH, 28.0], egui::Button::new("Developer"))
                            .clicked()
                        {
                            self.actions
                                .push_back(UiAction::InstallStarterPack(StarterPack::Developer));
                        }
                        if ui
                            .add_sized([PACK_BUTTON_WIDTH, 28.0], egui::Button::new("Writing"))
                            .clicked()
                        {
                            self.actions
                                .push_back(UiAction::InstallStarterPack(StarterPack::Writing));
                        }
                    });
                }
            },
        );
    }

    fn render_compose_surface(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.compose_mode, ComposeMode::Stack, "Stack");
            ui.selectable_value(&mut self.compose_mode, ComposeMode::Form, "Form");
            ui.selectable_value(&mut self.compose_mode, ComposeMode::Merge, "Merge");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!self.paste_stack.items().is_empty(), |ui| {
                    if design::icon_button(ui, Icon::Delete, "Clear paste stack", false).clicked() {
                        self.paste_stack.clear();
                    }
                });
                ui.small(format!("{} items", self.paste_stack.items().len()));
            });
        });
        ui.separator();

        if self.paste_stack.items().is_empty() {
            ui.add_space(72.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Paste stack is empty").strong());
            });
            return;
        }

        match self.compose_mode {
            ComposeMode::Stack => self.render_stack_editor(ui, false),
            ComposeMode::Form => self.render_stack_editor(ui, true),
            ComposeMode::Merge => self.render_merge_editor(ui),
        }
    }

    fn render_stack_editor(&mut self, ui: &mut egui::Ui, named_slots: bool) {
        let item_ids = self
            .paste_stack
            .items()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>();
        let mut row_actions = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, id) in item_ids.iter().copied().enumerate() {
                    let Ok(item) = self.paste_stack.item(id).cloned() else {
                        continue;
                    };
                    let mut label = item.label.clone();
                    let mut text = item.text.clone();
                    egui::Frame::new()
                        .inner_margin(6.0)
                        .fill(if index % 2 == 0 {
                            ui.visuals().faint_bg_color
                        } else {
                            Color32::TRANSPARENT
                        })
                        .show(ui, |ui| {
                            if named_slots {
                                let response = ui.add_sized(
                                    [ui.available_width(), 24.0],
                                    egui::TextEdit::singleline(&mut label).char_limit(80),
                                );
                                if response.changed() {
                                    let _ = self.paste_stack.rename(item.id, label.clone());
                                }
                            } else {
                                ui.label(RichText::new(&item.label).small().weak());
                            }
                            let editor_height = if named_slots { 28.0 } else { 52.0 };
                            let response = if named_slots {
                                ui.add_sized(
                                    [ui.available_width(), editor_height],
                                    egui::TextEdit::singleline(&mut text).char_limit(262_144),
                                )
                            } else {
                                ui.add_sized(
                                    [ui.available_width(), editor_height],
                                    egui::TextEdit::multiline(&mut text)
                                        .desired_rows(2)
                                        .char_limit(262_144),
                                )
                            };
                            if response.changed() {
                                let _ = self.paste_stack.edit(item.id, text.clone());
                            }
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if design::icon_button(
                                            ui,
                                            Icon::Delete,
                                            "Remove item",
                                            false,
                                        )
                                        .clicked()
                                        {
                                            row_actions.push(StackRowAction::Delete(item.id));
                                        }
                                        if design::icon_button(
                                            ui,
                                            Icon::Duplicate,
                                            "Duplicate item",
                                            false,
                                        )
                                        .clicked()
                                        {
                                            row_actions.push(StackRowAction::Duplicate(item.id));
                                        }
                                        ui.add_enabled_ui(index + 1 < item_ids.len(), |ui| {
                                            if design::icon_button(
                                                ui,
                                                Icon::Down,
                                                "Move down",
                                                false,
                                            )
                                            .clicked()
                                            {
                                                row_actions.push(StackRowAction::Down(item.id));
                                            }
                                        });
                                        ui.add_enabled_ui(index > 0, |ui| {
                                            if design::icon_button(ui, Icon::Up, "Move up", false)
                                                .clicked()
                                            {
                                                row_actions.push(StackRowAction::Up(item.id));
                                            }
                                        });
                                        if design::icon_button(
                                            ui,
                                            Icon::Paste,
                                            "Paste this item",
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.actions
                                                .push_back(UiAction::PasteText(text.clone()));
                                        }
                                    },
                                );
                            });
                        });
                }
            });
        for action in row_actions {
            match action {
                StackRowAction::Up(id) => {
                    let _ = self.paste_stack.move_up(id);
                }
                StackRowAction::Down(id) => {
                    let _ = self.paste_stack.move_down(id);
                }
                StackRowAction::Duplicate(id) => {
                    let _ = self.paste_stack.duplicate(id);
                }
                StackRowAction::Delete(id) => {
                    let _ = self.paste_stack.remove(id);
                }
            }
        }
    }

    fn render_merge_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("compose_merge_template")
                .selected_text(merge_template_label(self.merge_template))
                .show_ui(ui, |ui| {
                    for template in [
                        MergeTemplate::Bullets,
                        MergeTemplate::NumberedCitations,
                        MergeTemplate::CsvRows,
                        MergeTemplate::MarkdownTable,
                    ] {
                        ui.selectable_value(
                            &mut self.merge_template,
                            template,
                            merge_template_label(template),
                        );
                    }
                });
        });
        let values = self
            .paste_stack
            .items()
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>();
        let Ok(mut merged) = merge_text(&values, self.merge_template) else {
            ui.colored_label(ui.visuals().error_fg_color, "Merge exceeds the size limit");
            return;
        };
        ui.add(
            egui::TextEdit::multiline(&mut merged)
                .desired_rows(16)
                .interactive(false),
        );
        ui.horizontal(|ui| {
            if design::icon_button(ui, Icon::Paste, "Paste merged text", false).clicked() {
                self.actions.push_back(UiAction::PasteText(merged.clone()));
            }
            if design::icon_button(ui, Icon::Add, "Add merged text to stack", false).clicked() {
                let _ = self.paste_stack.add("Merged", merged);
                self.compose_mode = ComposeMode::Stack;
            }
        });
    }

    fn render_feedback_preview(&mut self, ctx: &egui::Context, capabilities: &[CapabilityView]) {
        if !self.feedback_preview {
            return;
        }
        let environment = FeedbackEnvironment {
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            session: std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into()),
            capabilities: capabilities
                .iter()
                .map(|capability| {
                    (
                        capability.feature.clone(),
                        format!("{} - {}", capability.level.label(), capability.detail),
                    )
                })
                .collect(),
        };
        let mut preview = environment.redacted_preview();
        let response = egui::Modal::new(egui::Id::new("feedback_preview")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading("Feedback report preview");
            ui.add(
                egui::TextEdit::multiline(&mut preview)
                    .desired_rows(16)
                    .interactive(false),
            );
            ui.horizontal(|ui| {
                let cancel = ui.button("Cancel").clicked();
                let open = ui.button("Open issue draft").clicked();
                (cancel, open)
            })
            .inner
        });
        let (cancel, open) = response.inner;
        if open {
            if let Some(url) = environment
                .github_issue_draft_url("themoretheless/vbuff", "Feedback / capture report")
            {
                ctx.open_url(egui::OpenUrl::new_tab(url));
            }
            self.feedback_preview = false;
        } else if cancel || response.should_close() {
            self.feedback_preview = false;
        }
    }

    fn render_trust_surface(
        &mut self,
        ui: &mut egui::Ui,
        posture: SecurityPostureSummary,
        capabilities: &[CapabilityView],
        ledger: &PrivacyLedgerSummary,
        slo: &SloStatusSummary,
    ) {
        let posture_color = security_color(posture.level);
        egui::Frame::new()
            .fill(posture_color.gamma_multiply(0.10))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    design::status_dot(ui, posture_color);
                    ui.label(RichText::new(posture.level.label()).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(format!(
                            "{} active · {} degraded · {} unavailable",
                            posture.active, posture.degraded, posture.unavailable
                        ));
                    });
                });
            });
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(RichText::new("Release SLOs").strong());
                egui::Grid::new("trust_slo_grid")
                    .num_columns(3)
                    .striped(true)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        render_slo_row(ui, "Zero lost captures", slo.zero_loss, "budget 0");
                        render_slo_row(ui, "Search p99", slo.search_latency, "budget 16 ms");
                        render_slo_row(ui, "Idle CPU", slo.idle_cpu, "budget 0.5%");
                        render_slo_row(ui, "Login ready", slo.login_ready, "budget 500 ms");
                    });

                ui.add_space(12.0);
                ui.label(RichText::new("Platform capabilities").strong());
                if capabilities.is_empty() {
                    ui.label(RichText::new("No capability evidence published").weak());
                } else {
                    for capability in capabilities {
                        let color = capability_color(capability.level);
                        ui.horizontal_wrapped(|ui| {
                            design::status_dot(ui, color);
                            ui.label(RichText::new(capability.feature.replace('_', " ")).strong());
                            ui.label(RichText::new(capability.level.label()).small().color(color));
                            ui.label(RichText::new(&capability.detail).small().weak());
                        });
                    }
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Privacy decisions").strong());
                    let color = if ledger.chain_valid {
                        Color32::from_rgb(44, 156, 103)
                    } else {
                        Color32::from_rgb(194, 64, 72)
                    };
                    ui.label(
                        RichText::new(if ledger.chain_valid {
                            "Chain verified"
                        } else {
                            "Chain invalid"
                        })
                        .small()
                        .color(color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("head {}", ledger.head_hash_prefix))
                                .small()
                                .monospace()
                                .weak(),
                        );
                    });
                });
                if ledger.recent.is_empty() {
                    ui.label(RichText::new("No capture decisions in this session").weak());
                } else {
                    for event in &ledger.recent {
                        let color = privacy_color(event.decision);
                        let time = chrono::DateTime::<Utc>::from_timestamp_millis(
                            event.timestamp_ms as i64,
                        )
                        .map(|value| value.format("%H:%M:%S").to_string())
                        .unwrap_or_else(|| "--:--:--".into());
                        ui.horizontal(|ui| {
                            design::status_dot(ui, color);
                            ui.monospace(format!("#{:04}", event.sequence));
                            ui.label(RichText::new(event.decision.label()).color(color));
                            ui.label(format!("{} ×{}", event.reason, event.count));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(RichText::new(time).small().weak());
                                },
                            );
                        });
                    }
                }

                ui.add_space(12.0);
                ui.label(RichText::new("Release trust").strong());
                ui.horizontal(|ui| {
                    ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                    ui.separator();
                    ui.label(RichText::new("Offline checksum verification").small());
                    ui.separator();
                    ui.label(RichText::new("Signed provenance workflow").small());
                });
            });
    }

    fn render_notice(&mut self, ui: &mut egui::Ui, notice: &CommandNotice) {
        let (accent, fill) = match notice.level {
            NoticeLevel::Info => (
                Color32::from_rgb(48, 132, 190),
                Color32::from_rgba_unmultiplied(48, 132, 190, 24),
            ),
            NoticeLevel::Warning => (
                Color32::from_rgb(210, 144, 32),
                Color32::from_rgba_unmultiplied(210, 144, 32, 24),
            ),
            NoticeLevel::Error => (
                Color32::from_rgb(194, 64, 72),
                Color32::from_rgba_unmultiplied(194, 64, 72, 24),
            ),
        };

        egui::Frame::new()
            .fill(fill)
            .inner_margin(5.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    design::status_dot(ui, accent);
                    let width = (ui.available_width() - design::ICON_BUTTON_SIZE - 12.0).max(120.0);
                    ui.add_sized(
                        [width, design::ICON_BUTTON_SIZE],
                        egui::Label::new(&notice.message).truncate(),
                    )
                    .on_hover_text(&notice.message);
                    if design::icon_button(ui, Icon::Close, "Dismiss message", false).clicked() {
                        self.actions.push_back(UiAction::DismissNotice);
                    }
                });
            });
    }

    fn render_preview_pane(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, clip: &Clip) {
        let fade = if self.preferences.reduced_motion {
            1.0
        } else {
            (self.preview_fade_started.elapsed().as_secs_f32() / 0.12).clamp(0.0, 1.0)
        };
        if fade < 1.0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        ui.set_opacity(fade);
        ui.horizontal(|ui| {
            ui.heading(clip.meta.kind.label());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if design::icon_button(ui, Icon::Close, "Close preview", false).clicked() {
                    self.preferences.large_preview = false;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            for badge in clip_badges(clip) {
                let color = badge_color(badge);
                design::status_dot(ui, color);
                ui.label(RichText::new(badge.label()).small().color(color));
            }
        });
        ui.separator();

        let peeking = self.is_peeking(clip.id);
        if clip.meta.sensitive && !peeking {
            ui.add_space(24.0);
            ui.label(RichText::new("Sensitive content").strong());
            if design::icon_button(ui, Icon::Eye, "Peek", false).clicked() {
                self.peek_sensitive = Some((clip.id, Instant::now() + Duration::from_secs(2)));
            }
            return;
        }

        if let Some(texture) = self.thumbnail(ctx, clip) {
            let available = egui::vec2(ui.available_width(), 260.0);
            ui.add(
                egui::Image::from_texture(&texture)
                    .max_size(available)
                    .maintain_aspect_ratio(true),
            );
            return;
        }

        let Some(canonical) = clip.primary_text() else {
            ui.label(RichText::new("No text preview").weak());
            return;
        };
        if clip.meta.kind == vbuff_types::ContentKind::Color
            && let Some(color) = parse_hex_color(canonical.trim())
        {
            let response = draw_color_swatch(ui, canonical.trim(), self.color_output_format);
            if response.clicked() {
                self.color_output_format = next_color_format(self.color_output_format);
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.color_output_format, ColorOutputFormat::Hex, "HEX");
                ui.selectable_value(&mut self.color_output_format, ColorOutputFormat::Rgb, "RGB");
                ui.selectable_value(&mut self.color_output_format, ColorOutputFormat::Hsl, "HSL");
            });
            let output = format_color(color, self.color_output_format);
            ui.label(RichText::new(&output).monospace().strong());
            let black = contrast_ratio([color.r(), color.g(), color.b()], [0, 0, 0]);
            let white = contrast_ratio([color.r(), color.g(), color.b()], [255, 255, 255]);
            ui.label(
                RichText::new(format!("Contrast B {black:.1}:1 · W {white:.1}:1"))
                    .small()
                    .weak(),
            );
            if ui.button("Paste color").clicked() {
                self.actions.push_back(UiAction::PasteText(output));
            }
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.preview_transform,
                PreviewTransform::Original,
                "Original",
            );
            ui.selectable_value(&mut self.preview_transform, PreviewTransform::Trim, "Trim");
            ui.selectable_value(
                &mut self.preview_transform,
                PreviewTransform::Uppercase,
                "Uppercase",
            );
            ui.selectable_value(
                &mut self.preview_transform,
                PreviewTransform::PrettyJson,
                "JSON",
            );
            if clip.meta.kind == ContentKind::Url {
                ui.selectable_value(
                    &mut self.preview_transform,
                    PreviewTransform::CleanLink,
                    "Clean link",
                );
            }
        });
        let transformed = match self.preview_transform {
            PreviewTransform::Original => Ok(canonical.to_owned()),
            PreviewTransform::Trim => TransformOverlay::preview(canonical, TextTransform::Trim)
                .map(|overlay| overlay.output().to_owned())
                .map_err(|_| ()),
            PreviewTransform::Uppercase => {
                TransformOverlay::preview(canonical, TextTransform::Uppercase)
                    .map(|overlay| overlay.output().to_owned())
                    .map_err(|_| ())
            }
            PreviewTransform::PrettyJson => {
                TransformOverlay::preview(canonical, TextTransform::PrettyJson)
                    .map(|overlay| overlay.output().to_owned())
                    .map_err(|_| ())
            }
            PreviewTransform::CleanLink => clean_link(canonical).map_err(|_| ()),
        };
        match transformed {
            Ok(output) => {
                let mut bounded = bounded_preview(&output, 32 * 1024);
                ui.add(
                    egui::TextEdit::multiline(&mut bounded)
                        .desired_rows(20)
                        .interactive(false)
                        .code_editor(),
                );
                if ui.button("Paste preview").clicked() {
                    if self.preview_transform == PreviewTransform::Original {
                        self.actions.push_back(UiAction::Paste(clip.id));
                    } else {
                        self.actions.push_back(UiAction::PasteText(output));
                    }
                }
            }
            Err(_) => {
                ui.label(
                    RichText::new("Transform unavailable for this clip")
                        .small()
                        .color(Color32::from_rgb(194, 64, 72)),
                );
            }
        }
    }

    fn render_motion_inspector(&self, ctx: &egui::Context) {
        if !self.preferences.motion_inspector {
            return;
        }
        egui::Area::new(egui::Id::new("motion_budget_inspector"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new("Motion budget").strong());
                    ui.monospace(format!(
                        "frame {:>5.1} ms",
                        self.motion_budget.last_frame_ms()
                    ));
                    ui.monospace(format!("scroll {:>4.0} pt/s", self.scroll_tuner.velocity()));
                    ui.monospace(format!(
                        "dropped {}",
                        compact_count(self.motion_budget.dropped_frames())
                    ));
                    ui.monospace(if self.preferences.reduced_motion {
                        "crossfade 0 ms"
                    } else {
                        "crossfade 120 ms"
                    });
                });
            });
    }

    fn is_peeking(&self, id: ClipId) -> bool {
        self.peek_sensitive
            .is_some_and(|(candidate, expires_at)| candidate == id && Instant::now() < expires_at)
    }

    fn undo_applies_to(&self, id: ClipId) -> bool {
        self.undo_slot
            .as_ref()
            .is_some_and(|slot| match &slot.action {
                UndoAction::Pin { id: target, .. } => *target == id,
                UndoAction::Stack { clip_id, .. } => *clip_id == id,
                UndoAction::Delete(clip) => clip.id == id,
            })
    }

    fn apply_undo(&mut self) {
        let Some(slot) = self.undo_slot.take() else {
            return;
        };
        match slot.action {
            UndoAction::Pin { id, previous } => {
                self.actions.push_back(UiAction::SetPinned(id, previous));
            }
            UndoAction::Stack { item_id, .. } => {
                let _ = self.paste_stack.remove(item_id);
            }
            UndoAction::Delete(clip) => self.actions.push_back(UiAction::RestoreClip(clip)),
        }
    }

    /// Render a single clip row.
    #[allow(clippy::too_many_arguments)]
    fn render_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        row: usize,
        clip: &Clip,
        hit: FilteredClip,
        selected: bool,
        cheap: bool,
        quick_pick: bool,
        row_height: f32,
        session_protected: bool,
    ) {
        let freshness = recency_strength(clip.meta.created_at, Utc::now());
        let bg = if selected {
            ui.visuals().selection.bg_fill
        } else {
            Color32::from_rgba_unmultiplied(31, 142, 104, (freshness * 22.0) as u8)
        };

        let frame = egui::Frame::new().fill(bg).inner_margin(design::ROW_MARGIN);
        frame.show(ui, |ui| {
            ui.set_min_height((row_height - design::ROW_MARGIN * 2.0).max(32.0));
            ui.horizontal(|ui| {
                if row < 9 && quick_pick {
                    ui.add_sized(
                        [18.0, design::THUMBNAIL_SIZE],
                        egui::Label::new(
                            RichText::new(format!("{}", row + 1))
                                .strong()
                                .monospace()
                                .color(ui.visuals().selection.bg_fill),
                        ),
                    );
                } else {
                    ui.allocate_space(egui::vec2(18.0, design::THUMBNAIL_SIZE));
                }

                if clip.meta.kind == vbuff_types::ContentKind::Color && !clip.meta.sensitive {
                    if let Some(text) = clip.primary_text() {
                        let response = draw_color_swatch(ui, text.trim(), self.color_output_format);
                        if response.clicked() {
                            self.color_output_format = next_color_format(self.color_output_format);
                        }
                    }
                } else if !cheap {
                    if let Some(tex) = self.thumbnail(ctx, clip) {
                        let size = egui::Vec2::splat(design::THUMBNAIL_SIZE);
                        ui.add(egui::Image::from_texture(&tex).fit_to_exact_size(size));
                    } else {
                        render_kind_icon(ui, clip);
                    }
                } else {
                    render_kind_icon(ui, clip);
                }

                let action_width = design::ICON_BUTTON_SIZE * 4.0 + 28.0;
                let content_width = (ui.available_width() - action_width).max(120.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(content_width, design::THUMBNAIL_SIZE),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let preview = row_preview_with_peek(clip, self.is_peeking(clip.id));
                        let label =
                            if !cheap && !clip.meta.sensitive && !self.query.trim().is_empty() {
                                egui::Label::new(highlighted_preview(
                                    ui,
                                    &preview,
                                    &self.query,
                                    hit.score,
                                ))
                            } else {
                                egui::Label::new(RichText::new(preview).strong())
                            };
                        let response = ui.add(label.truncate().sense(egui::Sense::click()));
                        if response.clicked() {
                            self.actions.push_back(UiAction::Paste(clip.id));
                        }

                        ui.horizontal(|ui| {
                            let mut meta = vec![clip.meta.kind.label().to_string()];
                            if let Some(app) = &clip.meta.source_app {
                                meta.push(short_app_name(app));
                            }
                            for badge in clip_badges(clip).into_iter().take(2) {
                                meta.push(badge.label().into());
                            }
                            if session_protected {
                                meta.push("Protected this session".into());
                            } else if clip.meta.expires_at.is_some() {
                                meta.push(expiry_label(clip, Utc::now(), None));
                            }
                            if let Some(delta) = hit.duplicate_delta {
                                meta.push(format!(
                                    "+{} -{} · {:.0}%",
                                    delta.added_chars,
                                    delta.removed_chars,
                                    delta.similarity * 100.0
                                ));
                            }
                            if hit.variant_of.is_some() {
                                meta.push("Variant".into());
                            }
                            meta.push(relative_time(clip.meta.created_at, Utc::now()));
                            ui.add(
                                egui::Label::new(RichText::new(meta.join(" · ")).small())
                                    .truncate(),
                            );
                            if hit.hidden_variants > 0
                                && ui
                                    .small_button(format!("{} variants", hit.hidden_variants + 1))
                                    .clicked()
                                && !self.expanded_duplicates.insert(clip.id)
                            {
                                self.expanded_duplicates.remove(&clip.id);
                            }
                        });
                    },
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.undo_applies_to(clip.id) {
                        if design::icon_button(ui, Icon::Undo, "Undo row action", false).clicked() {
                            self.apply_undo();
                        }
                    } else if clip.meta.sensitive {
                        let response = design::icon_button(ui, Icon::Eye, "Peek", false);
                        if response.clicked() || response.is_pointer_button_down_on() {
                            self.peek_sensitive =
                                Some((clip.id, Instant::now() + Duration::from_secs(2)));
                        }
                    } else {
                        ui.add_enabled_ui(clip.primary_text().is_some(), |ui| {
                            if design::icon_button(ui, Icon::Add, "Add to paste stack", false)
                                .clicked()
                                && let Some(text) = clip.primary_text()
                                && let Ok(item_id) = self
                                    .paste_stack
                                    .add(format!("{} {}", clip.meta.kind.label(), row + 1), text)
                            {
                                self.undo_slot = Some(UndoSlot {
                                    action: UndoAction::Stack {
                                        clip_id: clip.id,
                                        item_id,
                                    },
                                    expires_at: Instant::now() + Duration::from_secs(5),
                                });
                            }
                        });
                    }
                    if design::icon_button(ui, Icon::Preview, "Preview clip", selected).clicked() {
                        self.selected = row;
                        self.preferences.large_preview = true;
                    }
                    let pin_hover = if clip.pinned { "Unpin" } else { "Pin" };
                    if design::icon_button(
                        ui,
                        Icon::Pin {
                            filled: clip.pinned,
                        },
                        pin_hover,
                        clip.pinned,
                    )
                    .clicked()
                    {
                        self.actions
                            .push_back(UiAction::SetPinned(clip.id, !clip.pinned));
                        self.undo_slot = Some(UndoSlot {
                            action: UndoAction::Pin {
                                id: clip.id,
                                previous: clip.pinned,
                            },
                            expires_at: Instant::now() + Duration::from_secs(5),
                        });
                    }
                    if design::icon_button(ui, Icon::Delete, "Delete clip", false).clicked() {
                        self.confirm_delete = Some(clip.id);
                        self.confirm_clear_history = false;
                    }
                });
            });
        });
    }

    fn render_clear_history_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_clear_history {
            return;
        }

        let response =
            egui::Modal::new(egui::Id::new("clear_history_confirmation")).show(ctx, |ui| {
                ui.set_width(300.0);
                ui.heading("Clear clipboard history?");
                ui.label("Pinned and session-protected clips will be kept.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let cancel = ui.button("Cancel").clicked();
                    let clear = ui
                        .add(
                            egui::Button::new(RichText::new("Clear history").color(Color32::WHITE))
                                .fill(Color32::from_rgb(176, 48, 56)),
                        )
                        .clicked();
                    (cancel, clear)
                })
                .inner
            });

        let (cancel, clear) = response.inner;
        if clear {
            self.actions.push_back(UiAction::ClearHistory);
            self.confirm_clear_history = false;
        } else if cancel || response.should_close() {
            self.confirm_clear_history = false;
        }
    }

    fn render_delete_confirmation(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else {
            return;
        };

        let response =
            egui::Modal::new(egui::Id::new("delete_clip_confirmation")).show(ctx, |ui| {
                ui.set_width(300.0);
                ui.heading("Delete this clip?");
                ui.label("This removes it from local history.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let cancel = ui.button("Cancel").clicked();
                    let delete = ui
                        .add(
                            egui::Button::new(RichText::new("Delete clip").color(Color32::WHITE))
                                .fill(Color32::from_rgb(176, 48, 56)),
                        )
                        .clicked();
                    (cancel, delete)
                })
                .inner
            });

        let (cancel, delete) = response.inner;
        if delete {
            let deleted_clip = self
                .state
                .lock()
                .ok()
                .and_then(|state| state.clips.iter().find(|clip| clip.id == id).cloned());
            self.actions.push_back(UiAction::Delete(id));
            if let Some(clip) = deleted_clip {
                self.undo_slot = Some(UndoSlot {
                    action: UndoAction::Delete(Box::new(clip)),
                    expires_at: Instant::now() + Duration::from_secs(5),
                });
            }
            self.confirm_delete = None;
        } else if cancel || response.should_close() {
            self.confirm_delete = None;
        }
    }

    fn render_profile_confirmation(&mut self, ctx: &egui::Context, current_items: usize) {
        let Some(profile) = self.confirm_profile else {
            return;
        };
        let defaults = profile.defaults();
        let response =
            egui::Modal::new(egui::Id::new("profile_confirmation")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading(format!("Apply {} defaults?", profile.label()));
                egui::Grid::new("profile_default_preview")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        digest_row(ui, "History cap", defaults.max_history.to_string());
                        digest_row(
                            ui,
                            "Secret retention",
                            human_duration(defaults.secret_ttl_seconds),
                        );
                        digest_row(
                            ui,
                            "Full capture up to",
                            human_bytes(
                                u64::try_from(defaults.capture_hard_limit_bytes)
                                    .unwrap_or(u64::MAX),
                            ),
                        );
                        digest_row(
                            ui,
                            "Pause after idle",
                            human_duration(defaults.auto_pause_idle_seconds),
                        );
                    });
                if defaults.max_history < current_items {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "The lower history cap may remove older unpinned items during automatic cleanup.",
                        )
                        .small()
                        .color(Color32::from_rgb(210, 144, 32)),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let cancel = ui.button("Cancel").clicked();
                    let apply = ui.button("Apply defaults").clicked();
                    (cancel, apply)
                })
                .inner
            });
        let (cancel, apply) = response.inner;
        if apply {
            self.actions
                .push_back(UiAction::ApplyDefaultProfile(profile));
            self.confirm_profile = None;
        } else if cancel || response.should_close() {
            self.confirm_profile = None;
        }
    }

    /// Get or build a thumbnail texture for an image clip.
    fn thumbnail(&mut self, ctx: &egui::Context, clip: &Clip) -> Option<TextureHandle> {
        if clip.meta.sensitive {
            return None;
        }
        let image = clip.primary_image()?;
        let key = clip.id.to_string_repr();
        if let Some(cached) = self.thumbnails.get(&key) {
            return cached.clone();
        }
        let tex = build_thumbnail(ctx, image, &key);
        self.thumbnails.insert(key, tex.clone());
        tex
    }
}

fn render_capture_status(
    ui: &mut egui::Ui,
    paused: bool,
    reason: Option<CapturePauseReason>,
    health: CaptureHealth,
) {
    let (label, color) = if paused {
        (
            reason.map_or("Capture paused", CapturePauseReason::label),
            Color32::from_rgb(210, 144, 32),
        )
    } else {
        let color = match health {
            CaptureHealth::Starting => Color32::from_rgb(112, 120, 132),
            CaptureHealth::Watching => Color32::from_rgb(44, 156, 103),
            CaptureHealth::ClipboardUnavailable
            | CaptureHealth::ClipboardReadError
            | CaptureHealth::StorageError
            | CaptureHealth::Stalled
            | CaptureHealth::SelfTestFailed => Color32::from_rgb(194, 64, 72),
        };
        (health.label(), color)
    };

    design::status_dot(ui, color);
    ui.label(RichText::new(label).small().color(color));
}

fn digest_row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(RichText::new(label).small().weak());
    ui.label(RichText::new(value).small().monospace());
    ui.end_row();
}

fn human_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn human_duration(seconds: u64) -> String {
    if seconds.is_multiple_of(60 * 60) {
        format!("{}h", seconds / (60 * 60))
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn render_security_status(ui: &mut egui::Ui, posture: SecurityPostureSummary) {
    let color = security_color(posture.level);
    design::status_dot(ui, color);
    ui.label(RichText::new(posture.level.label()).small().color(color))
        .on_hover_text(format!(
            "{} active, {} degraded, {} unavailable{}",
            posture.active,
            posture.degraded,
            posture.unavailable,
            if posture.strict_mode {
                " · strict mode"
            } else {
                ""
            }
        ));
}

fn security_color(level: SecurityPostureLevel) -> Color32 {
    match level {
        SecurityPostureLevel::Protected => Color32::from_rgb(44, 156, 103),
        SecurityPostureLevel::Partial => Color32::from_rgb(210, 144, 32),
        SecurityPostureLevel::Blocked => Color32::from_rgb(194, 64, 72),
    }
}

fn capability_color(level: CapabilityViewLevel) -> Color32 {
    match level {
        CapabilityViewLevel::Active => Color32::from_rgb(44, 156, 103),
        CapabilityViewLevel::Degraded => Color32::from_rgb(210, 144, 32),
        CapabilityViewLevel::Unavailable => Color32::from_rgb(194, 64, 72),
        CapabilityViewLevel::NotApplicable => Color32::from_rgb(112, 120, 132),
    }
}

fn privacy_color(level: PrivacyDecisionLevel) -> Color32 {
    match level {
        PrivacyDecisionLevel::Captured => Color32::from_rgb(44, 156, 103),
        PrivacyDecisionLevel::Skipped => Color32::from_rgb(210, 144, 32),
        PrivacyDecisionLevel::Lost => Color32::from_rgb(194, 64, 72),
    }
}

fn render_slo_row(ui: &mut egui::Ui, label: &str, state: SloMetricState, budget: &str) {
    let color = match state {
        SloMetricState::Met => Color32::from_rgb(44, 156, 103),
        SloMetricState::Breached => Color32::from_rgb(194, 64, 72),
        SloMetricState::Unknown => Color32::from_rgb(112, 120, 132),
    };
    ui.label(label);
    ui.label(RichText::new(state.label()).small().color(color));
    ui.label(RichText::new(budget).small().weak());
    ui.end_row();
}

#[cfg(test)]
fn row_preview(clip: &Clip) -> String {
    row_preview_with_peek(clip, false)
}

fn row_preview_with_peek(clip: &Clip, peeking: bool) -> String {
    if clip.meta.sensitive && !peeking {
        "Sensitive content".to_owned()
    } else {
        clip.preview(80)
    }
}

fn bounded_preview(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let mut output = value[..boundary].to_owned();
    output.push_str("...");
    output
}

fn highlighted_preview(
    ui: &egui::Ui,
    preview: &str,
    query: &str,
    score: i64,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat {
        font_id: egui::TextStyle::Body.resolve(ui.style()),
        color: ui.visuals().strong_text_color(),
        ..Default::default()
    };
    let query = query.trim();
    let lower_preview = preview.to_lowercase();
    let lower_query = query.to_lowercase();
    let range = lower_preview.find(&lower_query).and_then(|start| {
        let end = start + lower_query.len();
        (preview.is_char_boundary(start) && preview.is_char_boundary(end)).then_some(start..end)
    });
    if let Some(range) = range {
        job.append(&preview[..range.start], 0.0, normal.clone());
        let mut highlighted = normal.clone();
        highlighted.background =
            Color32::from_rgba_unmultiplied(242, 177, 52, match_highlight_alpha(score));
        job.append(&preview[range.clone()], 0.0, highlighted);
        job.append(&preview[range.end..], 0.0, normal);
    } else {
        job.append(preview, 0.0, normal);
    }
    job
}

fn badge_color(badge: ClipBadge) -> Color32 {
    match badge {
        ClipBadge::Verified => Color32::from_rgb(28, 132, 86),
        ClipBadge::Lossless => Color32::from_rgb(55, 112, 173),
        ClipBadge::Partial => Color32::from_rgb(202, 132, 32),
        ClipBadge::Sensitive => Color32::from_rgb(176, 58, 73),
        ClipBadge::LocalOnly => Color32::from_rgb(112, 98, 147),
    }
}

fn render_kind_icon(ui: &mut egui::Ui, clip: &Clip) {
    ui.add_sized(
        [design::THUMBNAIL_SIZE, design::THUMBNAIL_SIZE],
        egui::Label::new(RichText::new(clip.meta.kind.icon()).size(20.0)),
    );
}

fn logical_viewport_size(ctx: &egui::Context) -> egui::Vec2 {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window()
            && let (Ok(width), Ok(height)) = (window.inner_width(), window.inner_height())
            && let (Some(width), Some(height)) = (width.as_f64(), height.as_f64())
        {
            return egui::vec2(width as f32, height as f32);
        }
    }
    ctx.screen_rect().size()
}

fn compact_count(count: u64) -> String {
    const UNITS: &[(u64, &str)] = &[
        (1_000_000_000_000_000_000, "E"),
        (1_000_000_000_000_000, "P"),
        (1_000_000_000_000, "T"),
        (1_000_000_000, "B"),
        (1_000_000, "M"),
        (1_000, "K"),
    ];
    for &(divisor, suffix) in UNITS {
        if count >= divisor {
            return format!("{:.1}{suffix}", count as f64 / divisor as f64);
        }
    }
    count.to_string()
}

/// Draw a swatch with a three-segment output-format ring.
fn draw_color_swatch(
    ui: &mut egui::Ui,
    text: &str,
    selected_format: ColorOutputFormat,
) -> egui::Response {
    let color = parse_hex_color(text).unwrap_or(Color32::GRAY);
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::splat(design::THUMBNAIL_SIZE),
        egui::Sense::click(),
    );
    ui.painter().rect_filled(rect.shrink(4.0), 4.0, color);
    for (index, format) in [
        ColorOutputFormat::Hex,
        ColorOutputFormat::Rgb,
        ColorOutputFormat::Hsl,
    ]
    .into_iter()
    .enumerate()
    {
        let start = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::TAU / 3.0;
        let end = start + std::f32::consts::TAU / 3.0 - 0.12;
        let stroke = egui::Stroke::new(
            if format == selected_format {
                2.6_f32
            } else {
                1.2_f32
            },
            match format {
                ColorOutputFormat::Hex => Color32::from_rgb(45, 125, 190),
                ColorOutputFormat::Rgb => Color32::from_rgb(216, 92, 75),
                ColorOutputFormat::Hsl => Color32::from_rgb(61, 154, 103),
            },
        );
        let points = (0..=8)
            .map(|step| {
                let angle = egui::lerp(start..=end, step as f32 / 8.0);
                rect.center() + egui::vec2(angle.cos(), angle.sin()) * (rect.width() / 2.0 - 1.5)
            })
            .collect();
        ui.painter().add(egui::Shape::line(points, stroke));
    }
    response.on_hover_text("Change color output format")
}

fn next_color_format(format: ColorOutputFormat) -> ColorOutputFormat {
    match format {
        ColorOutputFormat::Hex => ColorOutputFormat::Rgb,
        ColorOutputFormat::Rgb => ColorOutputFormat::Hsl,
        ColorOutputFormat::Hsl => ColorOutputFormat::Hex,
    }
}

fn format_color(color: Color32, format: ColorOutputFormat) -> String {
    match format {
        ColorOutputFormat::Hex => format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b()),
        ColorOutputFormat::Rgb => format!("rgb({}, {}, {})", color.r(), color.g(), color.b()),
        ColorOutputFormat::Hsl => {
            let (hue, saturation, lightness) = rgb_to_hsl(color.r(), color.g(), color.b());
            format!(
                "hsl({:.0}, {:.0}%, {:.0}%)",
                hue,
                saturation * 100.0,
                lightness * 100.0
            )
        }
    }
}

fn rgb_to_hsl(red: u8, green: u8, blue: u8) -> (f32, f32, f32) {
    let red = f32::from(red) / 255.0;
    let green = f32::from(green) / 255.0;
    let blue = f32::from(blue) / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    (hue, saturation, lightness)
}

/// Parse `#rgb` / `#rrggbb` / `#rrggbbaa` into a Color32.
fn parse_hex_color(s: &str) -> Option<Color32> {
    let hex = s.strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let bytes = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok();
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(Color32::from_rgb(r, g, b))
        }
        6 => Some(Color32::from_rgb(bytes(0, 2)?, bytes(2, 2)?, bytes(4, 2)?)),
        8 => Some(Color32::from_rgba_unmultiplied(
            bytes(0, 2)?,
            bytes(2, 2)?,
            bytes(4, 2)?,
            bytes(6, 2)?,
        )),
        _ => None,
    }
}

/// Build a small egui texture from an image flavor.
///
/// Handles both encoded images (PNG/JPEG/BMP) and the raw RGBA flavor that
/// arboard produces (`image/x-vbuff-rgba;width=…;height=…`).
fn build_thumbnail(
    ctx: &egui::Context,
    flavor: &vbuff_types::Flavor,
    key: &str,
) -> Option<TextureHandle> {
    let color_image = decode_thumbnail(flavor)?;
    Some(ctx.load_texture(key, color_image, egui::TextureOptions::LINEAR))
}

fn decode_thumbnail(flavor: &vbuff_types::Flavor) -> Option<egui::ColorImage> {
    let bytes = match &flavor.body {
        Body::Inline(b) => b,
        Body::Spilled { .. } => return None,
    };

    let color_image = if raw_rgba_mime(&flavor.mime) {
        let (w, h) = parse_rgba_dims(&flavor.mime)?;
        let required = w.checked_mul(h)?.checked_mul(4)?;
        if w == 0
            || h == 0
            || required != bytes.len()
            || u64::try_from(required).ok()? > MAX_THUMBNAIL_RGBA_BYTES
        {
            return None;
        }
        egui::ColorImage::from_rgba_unmultiplied([w, h], bytes)
    } else {
        let dimensions_reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let (width, height) = dimensions_reader.into_dimensions().ok()?;
        let decoded_bytes = u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(4)?;
        if width == 0
            || height == 0
            || width > MAX_THUMBNAIL_DIMENSION
            || height > MAX_THUMBNAIL_DIMENSION
            || decoded_bytes > MAX_THUMBNAIL_RGBA_BYTES
        {
            return None;
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_THUMBNAIL_DIMENSION);
        limits.max_image_height = Some(MAX_THUMBNAIL_DIMENSION);
        limits.max_alloc = Some(MAX_THUMBNAIL_RGBA_BYTES);
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        reader.limits(limits);
        let img = reader.decode().ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw())
    };

    Some(color_image)
}

fn raw_rgba_mime(mime: &str) -> bool {
    mime.split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("image/x-vbuff-rgba"))
}

/// Parse `width=W;height=H` from the RGBA MIME string.
fn parse_rgba_dims(mime: &str) -> Option<(usize, usize)> {
    let mut width = None;
    let mut height = None;
    for part in mime.split(';') {
        if let Some(v) = part.trim().strip_prefix("width=") {
            width = v.parse().ok();
        } else if let Some(v) = part.trim().strip_prefix("height=") {
            height = v.parse().ok();
        }
    }
    Some((width?, height?))
}

fn merge_template_label(template: MergeTemplate) -> &'static str {
    match template {
        MergeTemplate::Bullets => "Bullets",
        MergeTemplate::NumberedCitations => "Numbered citations",
        MergeTemplate::CsvRows => "CSV rows",
        MergeTemplate::MarkdownTable => "Markdown table",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_clip(text: &str) -> Clip {
        let flavors = vec![vbuff_types::Flavor::inline(
            "text/plain",
            text.as_bytes().to_vec(),
        )];
        Clip {
            id: ClipId::new(),
            content_hash: vbuff_core::content_hash_from_flavors(&flavors),
            meta: vbuff_types::ClipMeta::now(
                vbuff_types::ContentKind::Text,
                text.len() as u64,
                None,
            ),
            flavors,
            pinned: false,
            favorite: false,
        }
    }

    #[test]
    fn parses_short_hex() {
        assert_eq!(
            parse_hex_color("#fff"),
            Some(Color32::from_rgb(255, 255, 255))
        );
    }

    #[test]
    fn parses_long_hex() {
        assert_eq!(
            parse_hex_color("#ff8800"),
            Some(Color32::from_rgb(255, 136, 0))
        );
    }

    #[test]
    fn rejects_bad_hex() {
        assert_eq!(parse_hex_color("nope"), None);
        assert_eq!(parse_hex_color("#é"), None);
    }

    #[test]
    fn status_counts_stay_compact() {
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_250), "1.2K");
        assert_eq!(compact_count(u64::MAX), "18.4E");
    }

    #[test]
    fn sensitive_rows_never_render_clip_text() {
        let mut meta = vbuff_types::ClipMeta::now(vbuff_types::ContentKind::Text, 6, None);
        meta.sensitive = true;
        let clip = Clip {
            id: ClipId::new(),
            flavors: vec![vbuff_types::Flavor::inline(
                "text/plain",
                b"123456".to_vec(),
            )],
            content_hash: [0; 32],
            meta,
            pinned: false,
            favorite: false,
        };

        assert_eq!(row_preview(&clip), "Sensitive content");
        assert!(!row_preview(&clip).contains("123456"));
    }

    #[test]
    fn raw_thumbnail_decode_is_bounded_and_dimension_checked() {
        let valid =
            vbuff_types::Flavor::inline("IMAGE/X-VBUFF-RGBA;width=1;height=1", vec![0, 0, 0, 255]);
        let invalid = vbuff_types::Flavor::inline(
            "image/x-vbuff-rgba;width=18446744073709551615;height=2",
            vec![0; 4],
        );

        assert_eq!(decode_thumbnail(&valid).unwrap().size, [1, 1]);
        assert!(decode_thumbnail(&invalid).is_none());
    }

    #[test]
    fn delete_undo_emits_a_content_redacted_restore_action() {
        let clip = text_clip("private deleted value");
        let state = std::sync::Arc::new(std::sync::Mutex::new(crate::state::AppState::with_clips(
            vec![clip.clone()],
        )));
        let mut app = PopupApp::new(state);
        app.undo_slot = Some(UndoSlot {
            action: UndoAction::Delete(Box::new(clip.clone())),
            expires_at: Instant::now() + Duration::from_secs(5),
        });

        app.apply_undo();

        let actions = app.take_actions();
        assert_eq!(actions, vec![UiAction::RestoreClip(Box::new(clip))]);
        assert!(!format!("{actions:?}").contains("private deleted value"));
    }
}
