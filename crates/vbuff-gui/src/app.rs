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
use std::time::Instant;

use chrono::Utc;
use egui::{Color32, Key, RichText, TextureHandle, ViewportCommand};
use vbuff_core::{SearchResult, search};
use vbuff_types::{Body, Clip, ClipId};

use crate::state::{SharedState, UiAction};
use crate::view::{relative_time, short_app_name};

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

        // Reset selection when the underlying data changes.
        if revision != self.last_revision {
            self.last_revision = revision;
            self.selected = 0;
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
                ui.small(format!("{total} items"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear all").clicked() {
                        self.actions.push_back(UiAction::ClearAll);
                    }
                    let pause_label = if paused { "Resume" } else { "Pause" };
                    if ui.small_button(pause_label).clicked() {
                        self.actions.push_back(UiAction::TogglePause);
                    }
                });
            });
            ui.separator();

            // Virtualized results list.
            let row_height = 52.0;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, total, |ui, row_range| {
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
        });

        // Keep repainting while visible so focus/typing feels live.
        ctx.request_repaint();
    }
}

impl PopupApp {
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

        let frame = egui::Frame::new().fill(bg).inner_margin(6.0);
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Quick-pick number badge for the first nine rows.
                if row < 9 {
                    ui.label(RichText::new(format!("{}", row + 1)).weak().monospace());
                }

                // Kind icon / color swatch / thumbnail.
                if clip.meta.kind == vbuff_types::ContentKind::Color {
                    if let Some(text) = clip.primary_text() {
                        draw_color_swatch(ui, text.trim());
                    }
                } else if let Some(tex) = self.thumbnail(ctx, clip) {
                    let size = egui::vec2(40.0, 40.0);
                    ui.add(egui::Image::from_texture(&tex).fit_to_exact_size(size));
                } else {
                    ui.label(RichText::new(clip.meta.kind.icon()).size(20.0));
                }

                ui.vertical(|ui| {
                    // Preview line.
                    let preview = clip.preview(80);
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
                        meta.push(relative_time(clip.meta.created_at, Utc::now()));
                        ui.small(meta.join(" · "));
                    });
                });

                // Right-aligned actions.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                        self.actions.push_back(UiAction::Delete(clip.id));
                    }
                    let pin_icon = if clip.pinned { "📌" } else { "📍" };
                    let pin_hover = if clip.pinned { "Unpin" } else { "Pin" };
                    if ui.small_button(pin_icon).on_hover_text(pin_hover).clicked() {
                        self.actions
                            .push_back(UiAction::SetPinned(clip.id, !clip.pinned));
                    }
                });
            });
        });
    }

    /// Get or build a thumbnail texture for an image clip.
    fn thumbnail(&mut self, ctx: &egui::Context, clip: &Clip) -> Option<TextureHandle> {
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

/// Draw a small filled rectangle for a color clip.
fn draw_color_swatch(ui: &mut egui::Ui, text: &str) {
    let color = parse_hex_color(text).unwrap_or(Color32::GRAY);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color);
}

/// Parse `#rgb` / `#rrggbb` / `#rrggbbaa` into a Color32.
fn parse_hex_color(s: &str) -> Option<Color32> {
    let hex = s.strip_prefix('#')?;
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
    let bytes = match &flavor.body {
        Body::Inline(b) => b,
        Body::Spilled { .. } => return None,
    };

    let color_image = if flavor.mime.starts_with("image/x-vbuff-rgba") {
        let (w, h) = parse_rgba_dims(&flavor.mime)?;
        if w == 0 || h == 0 || bytes.len() < w * h * 4 {
            return None;
        }
        egui::ColorImage::from_rgba_unmultiplied([w, h], &bytes[..w * h * 4])
    } else {
        let img = image::load_from_memory(bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw())
    };

    Some(ctx.load_texture(key, color_image, egui::TextureOptions::LINEAR))
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
    }
}
