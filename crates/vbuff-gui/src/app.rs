//! The eframe popup application.
//!
//! Renders a borderless, always-on-top popup: a search box at the top that
//! filters as you type, and a virtualized results list below. Keyboard-driven:
//! Up/Down to move the selection, Enter to paste the selected clip, Esc to hide,
//! and Cmd/Ctrl+1..9 to quick-pick the first nine rows.
//!
//! The app does not perform side effects itself. It pushes [`UiAction`]s into a
//! queue, which the wiring drains each frame via [`PopupApp::take_actions`].

use std::collections::VecDeque;
use std::io::Cursor;
use std::time::{Duration, Instant};

use chrono::Utc;
use egui::{Color32, Key, RichText, TextureHandle, ViewportCommand};
use vbuff_core::compose::{MergeTemplate, PasteStack, PasteStackItemId, merge_text};
use vbuff_core::feedback::FeedbackEnvironment;
use vbuff_core::{SearchResult, search};
use vbuff_types::{
    Body, CapabilityView, CapabilityViewLevel, CaptureHealth, Clip, ClipId, CommandNotice,
    NoticeLevel, PrivacyDecisionLevel, PrivacyLedgerSummary, SecurityPostureLevel,
    SecurityPostureSummary, SloMetricState, SloStatusSummary,
};

use crate::design::{self, Icon};
use crate::state::{SharedState, StarterPack, UiAction};
use crate::view::{relative_time, short_app_name};

const MAX_THUMBNAIL_DIMENSION: u32 = 16_384;
const MAX_THUMBNAIL_RGBA_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PopupSurface {
    History,
    Compose,
    Trust,
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
    /// When the popup was last shown; used to ignore the initial focus-loss
    /// event that can fire during show.
    shown_at: Option<Instant>,
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
    /// Compact history and inspectable trust surfaces share one popup.
    surface: PopupSurface,
    /// Ephemeral, local-only composition scratchpad.
    paste_stack: PasteStack,
    compose_mode: ComposeMode,
    merge_template: MergeTemplate,
    feedback_preview: bool,
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
            thumbnails: std::collections::HashMap::new(),
            shown_at: None,
            request_focus_next_frame: false,
            design_applied: false,
            confirm_clear_history: false,
            confirm_delete: None,
            surface: PopupSurface::History,
            paste_stack: PasteStack::default(),
            compose_mode: ComposeMode::Stack,
            merge_template: MergeTemplate::Bullets,
            feedback_preview: false,
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
        self.surface = PopupSurface::History;
        self.feedback_preview = false;
        self.shown_at = Some(Instant::now());
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
        if !self.design_applied {
            design::apply(ctx);
            self.design_applied = true;
        }

        // 1. Check for a show request from the wiring.
        let (
            clips,
            paused,
            capture_health,
            capture_stats,
            security_posture,
            capabilities,
            privacy_ledger,
            slo_status,
            recoverable_skip,
            notice,
            show_requested,
            revision,
        ) = {
            let Ok(mut s) = self.state.lock() else {
                tracing::error!("GUI state mutex poisoned");
                return;
            };
            let show = std::mem::take(&mut s.show_requested);
            (
                s.clips.clone(),
                s.paused,
                s.capture_health,
                s.capture_stats,
                s.security_posture,
                s.capabilities.clone(),
                s.privacy_ledger.clone(),
                s.slo_status.clone(),
                s.skipped_recovery_available(std::time::Instant::now()),
                s.notice.clone(),
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

        // 4. Global key handling.
        let modifier_down = ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
        if !self.confirm_clear_history && self.confirm_delete.is_none() {
            ctx.input(|i| {
                if i.key_pressed(Key::Escape) {
                    self.actions.push_back(UiAction::Hide);
                }
                if self.surface == PopupSurface::History
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
                    && let Some(id) = filtered.get(self.selected)
                {
                    self.actions.push_back(UiAction::Paste(*id));
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
                            && let Some(id) = filtered.get(n - 1)
                        {
                            self.actions.push_back(UiAction::Paste(*id));
                        }
                    }
                }
            });
        }

        // If Esc requested a hide, do it now.
        if self.actions.iter().any(|a| *a == UiAction::Hide) {
            self.hide(ctx);
        }

        // 5. Render the panel.
        let clip_by_id: std::collections::HashMap<ClipId, &Clip> =
            clips.iter().map(|c| (c.id, c)).collect();

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_surface_header(ui, paused);

            match self.surface {
                PopupSurface::History => {
                    ui.horizontal(|ui| {
                        render_capture_status(ui, paused, capture_health);
                        ui.separator();
                        render_security_status(ui, security_posture);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                    ui.separator();

                    if total == 0 {
                        self.render_empty_history(ui, clips.is_empty());
                    } else {
                        // Stable-height virtualized rows keep controls from shifting.
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show_rows(ui, design::ROW_HEIGHT, total, |ui, row_range| {
                                for row in row_range {
                                    let Some(id) = filtered.get(row) else {
                                        continue;
                                    };
                                    let Some(clip) = clip_by_id.get(id) else {
                                        continue;
                                    };
                                    let selected = row == self.selected;
                                    self.render_row(ui, ctx, row, clip, selected);
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
            }
        });

        self.render_clear_history_confirmation(ctx);
        self.render_delete_confirmation(ctx);
        self.render_feedback_preview(ctx, &capabilities);

        // Input events repaint immediately; one low-frequency visible refresh
        // keeps expiry labels and background capture state current.
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

impl PopupApp {
    fn render_surface_header(&mut self, ui: &mut egui::Ui, paused: bool) {
        ui.horizontal(|ui| match self.surface {
            PopupSurface::History => {
                let hint = if paused {
                    "Search (capture paused)…"
                } else {
                    "Search…"
                };
                let search_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 2.0 - 16.0).max(160.0);
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
            }
            PopupSurface::Compose => {
                let title_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 2.0 - 16.0).max(160.0);
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
            }
            PopupSurface::Trust => {
                let title_width =
                    (ui.available_width() - design::ICON_BUTTON_SIZE * 3.0 - 24.0).max(160.0);
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
            }
        });
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

    /// Render a single clip row.
    fn render_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        row: usize,
        clip: &Clip,
        selected: bool,
    ) {
        let bg = if selected {
            ui.visuals().selection.bg_fill
        } else {
            Color32::TRANSPARENT
        };

        let frame = egui::Frame::new().fill(bg).inner_margin(design::ROW_MARGIN);
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Quick-pick number badge for the first nine rows.
                if row < 9 {
                    ui.add_sized(
                        [18.0, design::THUMBNAIL_SIZE],
                        egui::Label::new(RichText::new(format!("{}", row + 1)).weak().monospace()),
                    );
                } else {
                    ui.allocate_space(egui::vec2(18.0, design::THUMBNAIL_SIZE));
                }

                // Kind icon / color swatch / thumbnail.
                if clip.meta.kind == vbuff_types::ContentKind::Color && !clip.meta.sensitive {
                    if let Some(text) = clip.primary_text() {
                        draw_color_swatch(ui, text.trim());
                    }
                } else if let Some(tex) = self.thumbnail(ctx, clip) {
                    let size = egui::Vec2::splat(design::THUMBNAIL_SIZE);
                    ui.add(egui::Image::from_texture(&tex).fit_to_exact_size(size));
                } else {
                    ui.add_sized(
                        [design::THUMBNAIL_SIZE, design::THUMBNAIL_SIZE],
                        egui::Label::new(RichText::new(clip.meta.kind.icon()).size(20.0)),
                    );
                }

                let action_width = design::ICON_BUTTON_SIZE * 3.0 + 20.0;
                let content_width = (ui.available_width() - action_width).max(120.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(content_width, design::THUMBNAIL_SIZE),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        // Preview line.
                        let preview = row_preview(clip);
                        let resp = ui.add(
                            egui::Label::new(RichText::new(preview).strong())
                                .truncate()
                                .sense(egui::Sense::click()),
                        );
                        if resp.clicked() {
                            self.actions.push_back(UiAction::Paste(clip.id));
                        }

                        // Meta line: kind, source app, relative time.
                        ui.horizontal(|ui| {
                            let mut meta = vec![clip.meta.kind.label().to_string()];
                            if let Some(app) = &clip.meta.source_app {
                                meta.push(short_app_name(app));
                            }
                            if clip.flavors.iter().any(|flavor| !flavor.is_realized()) {
                                meta.push("Incomplete".into());
                            }
                            if clip.meta.sensitive {
                                meta.push("Sensitive".into());
                            }
                            if !clip.meta.sync_eligible {
                                meta.push("Local only".into());
                            }
                            if let Some(expires_at) = clip.meta.expires_at.as_ref() {
                                let seconds = expires_at
                                    .signed_duration_since(Utc::now())
                                    .num_seconds()
                                    .max(0);
                                meta.push(format!("Expires in {seconds}s"));
                            }
                            meta.push(relative_time(clip.meta.created_at, Utc::now()));
                            ui.add(
                                egui::Label::new(RichText::new(meta.join(" · ")).small())
                                    .truncate(),
                            );
                        });
                    },
                );

                // Right-aligned actions.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if design::icon_button(ui, Icon::Delete, "Delete clip", false).clicked() {
                        self.confirm_delete = Some(clip.id);
                        self.confirm_clear_history = false;
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
                    }
                    ui.add_enabled_ui(
                        !clip.meta.sensitive && clip.primary_text().is_some(),
                        |ui| {
                            if design::icon_button(ui, Icon::Add, "Add to paste stack", false)
                                .clicked()
                                && let Some(text) = clip.primary_text()
                            {
                                let label = format!("{} {}", clip.meta.kind.label(), row + 1);
                                let _ = self.paste_stack.add(label, text);
                            }
                        },
                    );
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
                ui.label("Pinned clips will be kept.");
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
            self.actions.push_back(UiAction::Delete(id));
            self.confirm_delete = None;
        } else if cancel || response.should_close() {
            self.confirm_delete = None;
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

fn render_capture_status(ui: &mut egui::Ui, paused: bool, health: CaptureHealth) {
    let (label, color) = if paused {
        ("Capture paused", Color32::from_rgb(210, 144, 32))
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

fn row_preview(clip: &Clip) -> String {
    if clip.meta.sensitive {
        "Sensitive content".to_owned()
    } else {
        clip.preview(80)
    }
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

/// Draw a small filled rectangle for a color clip.
fn draw_color_swatch(ui: &mut egui::Ui, text: &str) {
    let color = parse_hex_color(text).unwrap_or(Color32::GRAY);
    let (rect, _) = ui.allocate_exact_size(
        egui::Vec2::splat(design::THUMBNAIL_SIZE),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 4.0, color);
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
}
