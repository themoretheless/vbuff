//! Shared layout tokens and small native icon buttons for the popup.

use egui::{
    Color32, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
    WidgetType,
};

pub(crate) const POPUP_SIZE: [f32; 2] = [820.0, 620.0];
pub(crate) const POPUP_MIN_SIZE: [f32; 2] = [520.0, 420.0];
pub(crate) const ROW_MARGIN: f32 = 7.0;
pub(crate) const THUMBNAIL_SIZE: f32 = 40.0;
pub(crate) const ICON_BUTTON_SIZE: f32 = 28.0;
pub(crate) const WARNING: Color32 = Color32::from_rgb(246, 194, 92);
pub(crate) const DANGER: Color32 = Color32::from_rgb(255, 130, 142);

pub(crate) fn accent(ui: &Ui) -> Color32 {
    accent_for(ui.visuals().dark_mode)
}

pub(crate) fn success(ui: &Ui) -> Color32 {
    success_for(ui.visuals().dark_mode)
}

pub(crate) fn warning(ui: &Ui) -> Color32 {
    warning_for(ui.visuals().dark_mode)
}

pub(crate) fn danger(ui: &Ui) -> Color32 {
    danger_for(ui.visuals().dark_mode)
}

pub(crate) fn info(ui: &Ui) -> Color32 {
    info_for(ui.visuals().dark_mode)
}

const fn accent_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(112, 207, 221)
    } else {
        Color32::from_rgb(12, 92, 108)
    }
}

const fn success_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(112, 220, 164)
    } else {
        Color32::from_rgb(18, 104, 62)
    }
}

const fn warning_for(dark: bool) -> Color32 {
    if dark {
        WARNING
    } else {
        Color32::from_rgb(128, 76, 0)
    }
}

const fn danger_for(dark: bool) -> Color32 {
    if dark {
        DANGER
    } else {
        Color32::from_rgb(151, 28, 47)
    }
}

const fn info_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(126, 198, 238)
    } else {
        Color32::from_rgb(16, 89, 135)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Delete,
    Pin { filled: bool },
    Pause,
    Resume,
    Close,
    Add,
    Paste,
    Up,
    Down,
    Duplicate,
    Menu,
    Settings,
    Eye,
    Undo,
}

pub(crate) fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 5.0);
    style.spacing.interact_size = egui::vec2(ICON_BUTTON_SIZE, ICON_BUTTON_SIZE);
    style.visuals.window_corner_radius = egui::CornerRadius::same(6);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(6);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(4);
    if style.visuals.dark_mode {
        style.visuals.selection.bg_fill = Color32::from_rgb(24, 59, 85);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
        style.visuals.hyperlink_color = Color32::from_rgb(91, 180, 199);
        style.visuals.warn_fg_color = WARNING;
        style.visuals.error_fg_color = DANGER;
        style.visuals.panel_fill = Color32::from_rgb(24, 27, 32);
        style.visuals.window_fill = Color32::from_rgb(27, 30, 36);
        style.visuals.faint_bg_color = Color32::from_rgb(32, 37, 44);
        style.visuals.extreme_bg_color = Color32::from_rgb(17, 20, 24);
        style.visuals.code_bg_color = Color32::from_rgb(20, 34, 38);
    } else {
        style.visuals.selection.bg_fill = Color32::from_rgb(220, 235, 250);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(31, 77, 108));
        style.visuals.hyperlink_color = Color32::from_rgb(17, 103, 125);
        style.visuals.warn_fg_color = Color32::from_rgb(142, 91, 0);
        style.visuals.error_fg_color = Color32::from_rgb(169, 42, 58);
        style.visuals.panel_fill = Color32::from_rgb(245, 247, 249);
        style.visuals.window_fill = Color32::WHITE;
        style.visuals.faint_bg_color = Color32::from_rgb(233, 238, 242);
        style.visuals.extreme_bg_color = Color32::WHITE;
        style.visuals.code_bg_color = Color32::from_rgb(228, 241, 243);
    }
    ctx.set_style(style);
}

pub(crate) fn navigation_tab(ui: &mut Ui, label: &'static str, selected: bool) -> Response {
    ui.add_sized(
        [68.0, ICON_BUTTON_SIZE],
        egui::Button::selectable(selected, label),
    )
}

pub(crate) fn section_heading(ui: &mut Ui, title: &str, detail: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().size(14.0));
        if let Some(detail) = detail {
            ui.label(egui::RichText::new(detail).small().weak());
        }
    });
}

/// A fixed-size symbol button with a tooltip and no font-dependent emoji.
pub(crate) fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &'static str,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_SIZE), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    ui.painter().rect(
        rect,
        4.0,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        StrokeKind::Inside,
    );

    let stroke = Stroke::new(1.6_f32, visuals.fg_stroke.color);
    let center = rect.center();
    match icon {
        Icon::Delete => draw_delete(ui, center, stroke),
        Icon::Pin { filled } => draw_pin(ui, center, stroke, filled),
        Icon::Pause => draw_pause(ui, center, stroke),
        Icon::Resume => draw_resume(ui, center, visuals.fg_stroke.color),
        Icon::Close => draw_close(ui, center, stroke),
        Icon::Add => draw_add(ui, center, stroke),
        Icon::Paste => draw_paste(ui, center, stroke),
        Icon::Up => draw_chevron(ui, center, stroke, -1.0),
        Icon::Down => draw_chevron(ui, center, stroke, 1.0),
        Icon::Duplicate => draw_duplicate(ui, center, stroke),
        Icon::Menu => draw_menu(ui, center, stroke),
        Icon::Settings => draw_settings(ui, center, stroke),
        Icon::Eye => draw_eye(ui, center, stroke),
        Icon::Undo => draw_undo(ui, center, stroke),
    }

    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, tooltip)
    });
    response.on_hover_text(tooltip)
}

pub(crate) fn status_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

fn draw_delete(ui: &Ui, center: Pos2, stroke: Stroke) {
    let body = Rect::from_center_size(center + egui::vec2(0.0, 1.5), egui::vec2(8.0, 9.0));
    ui.painter()
        .rect_stroke(body, 1.0, stroke, StrokeKind::Inside);
    ui.painter().line_segment(
        [
            center + egui::vec2(-5.0, -4.5),
            center + egui::vec2(5.0, -4.5),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-2.0, -6.5),
            center + egui::vec2(2.0, -6.5),
        ],
        stroke,
    );
}

fn draw_pin(ui: &Ui, center: Pos2, stroke: Stroke, filled: bool) {
    let head = Rect::from_center_size(center + egui::vec2(0.0, -3.0), egui::vec2(8.0, 6.0));
    if filled {
        ui.painter().rect_filled(head, 2.0, stroke.color);
    } else {
        ui.painter()
            .rect_stroke(head, 2.0, stroke, StrokeKind::Inside);
    }
    ui.painter().line_segment(
        [center + egui::vec2(0.0, 0.0), center + egui::vec2(0.0, 7.0)],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-4.5, 0.0),
            center + egui::vec2(4.5, 0.0),
        ],
        stroke,
    );
}

fn draw_pause(ui: &Ui, center: Pos2, stroke: Stroke) {
    for offset in [-3.0, 3.0] {
        ui.painter().line_segment(
            [
                center + egui::vec2(offset, -5.0),
                center + egui::vec2(offset, 5.0),
            ],
            Stroke::new(2.2_f32, stroke.color),
        );
    }
}

fn draw_resume(ui: &Ui, center: Pos2, color: Color32) {
    ui.painter().add(Shape::convex_polygon(
        vec![
            center + egui::vec2(-4.0, -6.0),
            center + egui::vec2(6.0, 0.0),
            center + egui::vec2(-4.0, 6.0),
        ],
        color,
        Stroke::NONE,
    ));
}

fn draw_close(ui: &Ui, center: Pos2, stroke: Stroke) {
    ui.painter().line_segment(
        [
            center + egui::vec2(-4.0, -4.0),
            center + egui::vec2(4.0, 4.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(4.0, -4.0),
            center + egui::vec2(-4.0, 4.0),
        ],
        stroke,
    );
}

fn draw_add(ui: &Ui, center: Pos2, stroke: Stroke) {
    ui.painter().line_segment(
        [
            center + egui::vec2(-5.0, 0.0),
            center + egui::vec2(5.0, 0.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(0.0, -5.0),
            center + egui::vec2(0.0, 5.0),
        ],
        stroke,
    );
}

fn draw_paste(ui: &Ui, center: Pos2, stroke: Stroke) {
    let board = Rect::from_center_size(center + egui::vec2(0.0, 1.0), egui::vec2(10.0, 12.0));
    ui.painter()
        .rect_stroke(board, 2.0, stroke, StrokeKind::Inside);
    let clip = Rect::from_center_size(center + egui::vec2(0.0, -5.0), egui::vec2(5.0, 3.0));
    ui.painter()
        .rect_stroke(clip, 1.0, stroke, StrokeKind::Inside);
}

fn draw_chevron(ui: &Ui, center: Pos2, stroke: Stroke, direction: f32) {
    ui.painter().line_segment(
        [
            center + egui::vec2(-4.0, 2.0 * direction),
            center + egui::vec2(0.0, -2.0 * direction),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(0.0, -2.0 * direction),
            center + egui::vec2(4.0, 2.0 * direction),
        ],
        stroke,
    );
}

fn draw_duplicate(ui: &Ui, center: Pos2, stroke: Stroke) {
    let back = Rect::from_center_size(center + egui::vec2(-2.0, -2.0), egui::vec2(8.0, 8.0));
    let front = Rect::from_center_size(center + egui::vec2(2.0, 2.0), egui::vec2(8.0, 8.0));
    ui.painter()
        .rect_stroke(back, 1.0, stroke, StrokeKind::Inside);
    ui.painter()
        .rect_filled(front, 1.0, ui.visuals().panel_fill);
    ui.painter()
        .rect_stroke(front, 1.0, stroke, StrokeKind::Inside);
}

fn draw_menu(ui: &Ui, center: Pos2, stroke: Stroke) {
    for offset in [-5.0, 0.0, 5.0] {
        ui.painter()
            .circle_filled(center + egui::vec2(offset, 0.0), 1.5, stroke.color);
    }
}

fn draw_settings(ui: &Ui, center: Pos2, stroke: Stroke) {
    ui.painter().circle_stroke(center, 3.0, stroke);
    for step in 0..8 {
        let angle = step as f32 * std::f32::consts::TAU / 8.0;
        let direction = egui::vec2(angle.cos(), angle.sin());
        ui.painter()
            .line_segment([center + direction * 5.0, center + direction * 7.0], stroke);
    }
}

fn draw_preview(ui: &Ui, center: Pos2, stroke: Stroke) {
    let points = vec![
        center + egui::vec2(-7.0, 0.0),
        center + egui::vec2(-3.0, -4.0),
        center + egui::vec2(3.0, -4.0),
        center + egui::vec2(7.0, 0.0),
        center + egui::vec2(3.0, 4.0),
        center + egui::vec2(-3.0, 4.0),
    ];
    ui.painter().add(Shape::closed_line(points, stroke));
    ui.painter().circle_stroke(center, 2.0, stroke);
}

fn draw_eye(ui: &Ui, center: Pos2, stroke: Stroke) {
    draw_preview(ui, center, stroke);
}

fn draw_undo(ui: &Ui, center: Pos2, stroke: Stroke) {
    ui.painter().add(Shape::line(
        vec![
            center + egui::vec2(5.5, 4.5),
            center + egui::vec2(4.0, -2.0),
            center + egui::vec2(-3.5, -3.0),
            center + egui::vec2(-6.0, 1.0),
        ],
        stroke,
    ));
    ui.painter().line_segment(
        [
            center + egui::vec2(-6.0, 1.0),
            center + egui::vec2(-5.5, -5.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-6.0, 1.0),
            center + egui::vec2(0.0, 0.5),
        ],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::contrast_ratio;

    #[test]
    fn semantic_foregrounds_pass_wcag_aa_on_both_panel_themes() {
        for (dark, background) in [
            (true, Color32::from_rgb(24, 27, 32)),
            (false, Color32::from_rgb(245, 247, 249)),
        ] {
            for foreground in [
                accent_for(dark),
                success_for(dark),
                warning_for(dark),
                danger_for(dark),
                info_for(dark),
            ] {
                let ratio = contrast_ratio(
                    [foreground.r(), foreground.g(), foreground.b()],
                    [background.r(), background.g(), background.b()],
                );
                assert!(ratio >= 4.5, "semantic color contrast was {ratio:.2}:1");
            }
        }
    }
}
