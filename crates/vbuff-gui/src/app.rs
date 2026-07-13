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
use vbuff_core::{SearchResult, search};
use vbuff_types::{Body, CaptureHealth, Clip, ClipId, CommandNotice, NoticeLevel};

use crate::design::{self, Icon};
use crate::state::{SharedState, UiAction};
use crate::view::{relative_time, short_app_name};

const MAX_THUMBNAIL_DIMENSION: u32 = 16_384;
const MAX_THUMBNAIL_RGBA_BYTES: u64 = 128 * 1024 * 1024;

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
                if i.key_pressed(Key::ArrowDown) && total > 0 {
                    self.selected = (self.selected + 1).min(total - 1);
                }
                if i.key_pressed(Key::ArrowUp) && total > 0 {
                    self.selected = self.selected.saturating_sub(1);
                }
                if i.key_pressed(Key::Enter)
                    && total > 0
                    && let Some(id) = filtered.get(self.selected)
                {
                    self.actions.push_back(UiAction::Paste(*id));
                }
                // Cmd/Ctrl + 1..9 quick select.
                if modifier_down {
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
            // Header: search box + status.
            ui.horizontal(|ui| {
                let hint = if paused {
                    "Search (capture PAUSED)…"
                } else {
                    "Search…"
                };
                let edit = egui::TextEdit::singleline(&mut self.query)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY);
                let resp = ui.add(edit);
                if self.request_focus_next_frame {
                    resp.request_focus();
                    self.request_focus_next_frame = false;
                } else if !resp.has_focus() && self.actions.is_empty() {
                    // Keep the search box focused for type-to-filter.
                    resp.request_focus();
                }
            });

            ui.horizontal(|ui| {
                render_capture_status(ui, paused, capture_health);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!clips.is_empty(), egui::Button::new("Clear history"))
                        .clicked()
                    {
                        self.confirm_clear_history = true;
                        self.confirm_delete = None;
                    }
                    let (pause_icon, pause_tooltip) = if paused {
                        (Icon::Resume, "Resume capture")
                    } else {
                        (Icon::Pause, "Pause capture")
                    };
                    if design::icon_button(ui, pause_icon, pause_tooltip, paused).clicked() {
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
                        RichText::new(format!("{} lost", compact_count(capture_stats.lost)))
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
                ui.with_layout(
                    egui::Layout::top_down_justified(egui::Align::Center),
                    |ui| {
                        ui.add_space(72.0);
                        let message = if self.query.trim().is_empty() {
                            "No clipboard history yet"
                        } else {
                            "No matching clips"
                        };
                        ui.label(RichText::new(message).strong());
                    },
                );
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
        });

        self.render_clear_history_confirmation(ctx);
        self.render_delete_confirmation(ctx);

        // Input events repaint immediately; one low-frequency visible refresh
        // keeps expiry labels and background capture state current.
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

impl PopupApp {
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

                let action_width = design::ICON_BUTTON_SIZE * 2.0 + 12.0;
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
