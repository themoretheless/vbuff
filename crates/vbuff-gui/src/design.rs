//! Shared layout tokens and small native icon buttons for the popup.

use egui::{
    Color32, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
    WidgetType,
};

pub(crate) const POPUP_SIZE: [f32; 2] = [560.0, 620.0];
pub(crate) const POPUP_MIN_SIZE: [f32; 2] = [420.0, 420.0];
pub(crate) const ROW_HEIGHT: f32 = 58.0;
pub(crate) const ROW_MARGIN: f32 = 7.0;
pub(crate) const THUMBNAIL_SIZE: f32 = 40.0;
pub(crate) const ICON_BUTTON_SIZE: f32 = 28.0;

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Delete,
    Pin { filled: bool },
    Pause,
    Resume,
    Close,
    Shield,
    History,
    Compose,
    Feedback,
    Add,
    Paste,
    Up,
    Down,
    Duplicate,
}

pub(crate) fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 5.0);
    style.spacing.interact_size = egui::vec2(ICON_BUTTON_SIZE, ICON_BUTTON_SIZE);
    ctx.set_style(style);
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
        Icon::Shield => draw_shield(ui, center, stroke),
        Icon::History => draw_history(ui, center, stroke),
        Icon::Compose => draw_compose(ui, center, stroke),
        Icon::Feedback => draw_feedback(ui, center, stroke),
        Icon::Add => draw_add(ui, center, stroke),
        Icon::Paste => draw_paste(ui, center, stroke),
        Icon::Up => draw_chevron(ui, center, stroke, -1.0),
        Icon::Down => draw_chevron(ui, center, stroke, 1.0),
        Icon::Duplicate => draw_duplicate(ui, center, stroke),
    }

    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), tooltip));
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

fn draw_shield(ui: &Ui, center: Pos2, stroke: Stroke) {
    let points = vec![
        center + egui::vec2(0.0, -7.0),
        center + egui::vec2(6.0, -4.5),
        center + egui::vec2(5.0, 2.5),
        center + egui::vec2(0.0, 7.0),
        center + egui::vec2(-5.0, 2.5),
        center + egui::vec2(-6.0, -4.5),
    ];
    ui.painter().add(Shape::closed_line(
        points,
        Stroke::new(1.6_f32, stroke.color),
    ));
    ui.painter().line_segment(
        [
            center + egui::vec2(-2.5, 0.0),
            center + egui::vec2(-0.5, 2.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-0.5, 2.0),
            center + egui::vec2(3.2, -2.5),
        ],
        stroke,
    );
}

fn draw_history(ui: &Ui, center: Pos2, stroke: Stroke) {
    ui.painter().circle_stroke(center, 6.0, stroke);
    ui.painter()
        .line_segment([center, center + egui::vec2(0.0, -3.5)], stroke);
    ui.painter()
        .line_segment([center, center + egui::vec2(3.0, 1.5)], stroke);
}

fn draw_compose(ui: &Ui, center: Pos2, stroke: Stroke) {
    for offset in [-4.0, 0.0, 4.0] {
        ui.painter()
            .circle_stroke(center + egui::vec2(-5.0, offset), 1.0, stroke);
        ui.painter().line_segment(
            [
                center + egui::vec2(-2.0, offset),
                center + egui::vec2(6.0, offset),
            ],
            stroke,
        );
    }
}

fn draw_feedback(ui: &Ui, center: Pos2, stroke: Stroke) {
    let bubble = Rect::from_center_size(center + egui::vec2(0.0, -1.0), egui::vec2(13.0, 10.0));
    ui.painter()
        .rect_stroke(bubble, 3.0, stroke, StrokeKind::Inside);
    ui.painter().line_segment(
        [
            center + egui::vec2(-2.0, 4.0),
            center + egui::vec2(-4.0, 7.0),
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
