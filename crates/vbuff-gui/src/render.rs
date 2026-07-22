//! Drawing the popup panel and its list rows. No input handling and no state
//! snapshotting lives here - only "given this data, draw it."

use std::collections::HashMap;

use chrono::Utc;
use egui::{Color32, RichText};
use vbuff_types::{Clip, ClipId};

use crate::app::PopupApp;
use crate::color::draw_color_swatch;
use crate::state::UiAction;
use crate::theme::{
    ICON_FONT_SIZE, QUICK_PICK_BADGE_WIDTH, QUICK_PICK_SLOTS, ROW_HEIGHT, SPACING_SM, SPACING_XS,
    THUMBNAIL_SIZE,
};
use crate::view::{relative_time, short_app_name};

impl PopupApp {
    /// Draw the search box, status row, and virtualized results list.
    pub(crate) fn render_panel(
        &mut self,
        ctx: &egui::Context,
        paused: bool,
        total: usize,
        filtered: &[ClipId],
        clip_by_id: &HashMap<ClipId, &Clip>,
    ) {
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

            if total == 0 {
                self.render_empty_state(ui);
                return;
            }

            // Virtualized results list.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, ROW_HEIGHT, total, |ui, row_range| {
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
    }

    /// The empty/no-results state: distinct copy for "no history yet" vs. "no
    /// match for this query," so the user isn't left staring at a blank list
    /// wondering whether vbuff is broken.
    fn render_empty_state(&self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            if self.query.trim().is_empty() {
                ui.label(RichText::new("No clips yet").strong());
                ui.small("Copy something and it will show up here.");
            } else {
                ui.label(RichText::new("No matches").strong());
                ui.small(format!("Nothing found for \"{}\".", self.query.trim()));
            }
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

        let frame = egui::Frame::new().fill(bg).inner_margin(SPACING_SM);
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Quick-pick number badge for the first QUICK_PICK_SLOTS rows.
                // A fixed-width slot is reserved even when there is no badge
                // (row >= QUICK_PICK_SLOTS) so the icon/thumbnail column
                // lines up at the same x position on every row.
                let badge = if row < QUICK_PICK_SLOTS {
                    (row + 1).to_string()
                } else {
                    String::new()
                };
                ui.add_sized(
                    [QUICK_PICK_BADGE_WIDTH, ui.available_height()],
                    egui::Label::new(RichText::new(badge).weak().monospace()),
                );
                ui.add_space(SPACING_XS);

                // Kind icon / color swatch / thumbnail.
                if clip.meta.kind == vbuff_types::ContentKind::Color {
                    if let Some(text) = clip.primary_text() {
                        draw_color_swatch(ui, text.trim());
                    }
                } else if let Some(tex) = self.thumbnails.get_or_build(ctx, clip) {
                    let size = egui::vec2(THUMBNAIL_SIZE, THUMBNAIL_SIZE);
                    ui.add(egui::Image::from_texture(&tex).fit_to_exact_size(size));
                } else {
                    ui.label(RichText::new(clip.meta.kind.icon()).size(ICON_FONT_SIZE));
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

                    // Meta line: kind, source app, and time since last touched
                    // (updated_at, not created_at - a re-copied clip should
                    // read "just now," not its original capture time).
                    ui.horizontal(|ui| {
                        let mut meta = vec![clip.meta.kind.label().to_string()];
                        if let Some(app) = &clip.meta.source_app {
                            meta.push(short_app_name(app));
                        }
                        meta.push(relative_time(clip.meta.updated_at, Utc::now()));
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
}
