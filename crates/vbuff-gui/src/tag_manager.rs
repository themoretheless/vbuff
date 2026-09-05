//! Native tag catalog, assignment and full-history filters.
use crate::{UiAction, experience::HistoryScope};
use std::collections::VecDeque;
use vbuff_types::{ClipId, TagCommand, TagSnapshot};

pub(crate) struct TagManager {
    open: bool,
    search: String,
    name: String,
    editing: Option<String>,
    color: [u8; 3],
    selected: Vec<ClipId>,
}
impl Default for TagManager {
    fn default() -> Self {
        Self {
            open: false,
            search: String::new(),
            name: String::new(),
            editing: None,
            color: [92, 142, 220],
            selected: Vec::new(),
        }
    }
}
impl TagManager {
    pub(crate) fn render(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &TagSnapshot,
        scope: &mut HistoryScope,
        clip: Option<ClipId>,
        actions: &mut VecDeque<UiAction>,
    ) {
        {
            if ui.button("Tags").clicked() {
                self.open = true;
            }
            ui.menu_button("Filter tags", |ui| {
                let (base, mut ids, mut all) = match &*scope {
                    HistoryScope::Tagged { base, ids, all } => {
                        ((**base).clone(), ids.clone(), *all)
                    }
                    other => (other.clone(), Vec::new(), true),
                };
                ui.horizontal(|ui| {
                    ui.radio_value(&mut all, true, "All selected");
                    ui.radio_value(&mut all, false, "Any selected");
                });
                if ui.button("Clear tag filter").clicked() {
                    ids.clear();
                }
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for tag in &snapshot.tags {
                            let mut checked = ids.contains(&tag.id);
                            if ui.checkbox(&mut checked, &tag.name).changed() {
                                ids.retain(|id| id != &tag.id);
                                if checked {
                                    ids.push(tag.id.clone());
                                }
                            }
                        }
                    });
                *scope = if ids.is_empty() {
                    base
                } else {
                    HistoryScope::Tagged {
                        base: Box::new(base),
                        ids,
                        all,
                    }
                };
            });
            if let HistoryScope::Tagged { ids, all, .. } = scope {
                ui.small(format!(
                    "{} tags · {}",
                    ids.len(),
                    if *all { "all" } else { "any" }
                ));
            }
            if ui
                .add_enabled(
                    clip.is_some() && self.selected.len() < 1000,
                    egui::Button::new("Select for tagging"),
                )
                .clicked()
                && let Some(id) = clip
                && !self.selected.contains(&id)
            {
                self.selected.push(id);
            }
        }
        let mut open = self.open;
        egui::Window::new("Tag manager")
            .open(&mut open)
            .default_width(440.0)
            .show(ui.ctx(), |ui| {
                ui.heading("Organize your history");
                ui.label(
                    egui::RichText::new("Create tags, then assign them to your selected clips.")
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(if self.selected.is_empty() {
                        if clip.is_some() {
                            "Using the focused clip".into()
                        } else {
                            "Select a clip in history".into()
                        }
                    } else {
                        format!("{} selected", self.selected.len())
                    });
                    if !self.selected.is_empty() && ui.button("Clear selection").clicked() {
                        self.selected.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.name)
                            .hint_text("Tag name")
                            .char_limit(64)
                            .desired_width(180.0),
                    );
                    ui.color_edit_button_srgb(&mut self.color);
                    if ui
                        .add_enabled(
                            !self.name.trim().is_empty(),
                            egui::Button::new(if self.editing.is_some() {
                                "Save"
                            } else {
                                "Create"
                            }),
                        )
                        .clicked()
                    {
                        actions.push_back(UiAction::EditTags(TagCommand::Save {
                            id: self.editing.take(),
                            name: std::mem::take(&mut self.name),
                            color: self.color,
                        }));
                    }
                    if self.editing.is_some() && ui.button("Cancel").clicked() {
                        self.editing = None;
                        self.name.clear();
                    }
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Find tags")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for tag in snapshot
                            .tags
                            .iter()
                            .filter(|t| vbuff_core::recall::layout_contains(&t.name, &self.search))
                        {
                            ui.push_id(&tag.id, |ui| {
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(8.0, 8.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        egui::Color32::from_rgb(
                                            tag.color[0],
                                            tag.color[1],
                                            tag.color[2],
                                        ),
                                    );
                                    ui.add_sized(
                                        [150.0, 28.0],
                                        egui::Label::new(format!(
                                            "{} ({})",
                                            tag.name,
                                            tag.clips.len()
                                        ))
                                        .truncate()
                                        .halign(egui::Align::Min),
                                    )
                                    .on_hover_text(&tag.name);
                                    let clips = if self.selected.is_empty() {
                                        clip.into_iter().collect()
                                    } else {
                                        self.selected.clone()
                                    };
                                    ui.add_enabled_ui(!clips.is_empty(), |ui| {
                                        if ui.small_button("Assign").clicked() {
                                            actions.push_back(UiAction::EditTags(
                                                TagCommand::Assign {
                                                    clips: clips.clone(),
                                                    tag: tag.id.clone(),
                                                    assigned: true,
                                                },
                                            ));
                                        }
                                        if ui.small_button("Unassign").clicked() {
                                            actions.push_back(UiAction::EditTags(
                                                TagCommand::Assign {
                                                    clips: clips.clone(),
                                                    tag: tag.id.clone(),
                                                    assigned: false,
                                                },
                                            ));
                                        }
                                    });
                                    ui.menu_button("More", |ui| {
                                        if ui.small_button("Edit").clicked() {
                                            self.editing = Some(tag.id.clone());
                                            self.name = tag.name.clone();
                                            self.color = tag.color;
                                            ui.close();
                                        }
                                        ui.menu_button("Merge into…", |ui| {
                                            for target in
                                                snapshot.tags.iter().filter(|t| t.id != tag.id)
                                            {
                                                if ui.button(&target.name).clicked() {
                                                    actions.push_back(UiAction::EditTags(
                                                        TagCommand::Merge {
                                                            source: tag.id.clone(),
                                                            target: target.id.clone(),
                                                        },
                                                    ));
                                                    ui.close();
                                                }
                                            }
                                        });
                                        ui.separator();
                                        if ui
                                            .button("Delete tag")
                                            .on_hover_text("Clips are kept in history")
                                            .clicked()
                                        {
                                            actions.push_back(UiAction::EditTags(
                                                TagCommand::Delete(tag.id.clone()),
                                            ));
                                            ui.close();
                                        }
                                    });
                                });
                            });
                            ui.add_space(4.0);
                        }
                        if snapshot.tags.is_empty() {
                            ui.label("No tags yet. Create your first tag above.");
                        }
                    });
            });
        self.open = open;
    }
}
