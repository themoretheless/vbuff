//! Shared layout tokens and small native icon buttons for the popup.

use egui::{
    Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2,
    WidgetInfo, WidgetType,
};

// "Split Cockpit" (direction 1b): list + permanent preview panel on the
// right, segmented History/Stack pills in the header, a 32px status footer,
// and two-line 46px rows. Window 860x560; the panel hides below 720.
pub(crate) const POPUP_SIZE: [f32; 2] = [860.0, 560.0];
pub(crate) const POPUP_MIN_SIZE: [f32; 2] = [520.0, 420.0];
pub(crate) const SPACE_XS: f32 = 4.0;
pub(crate) const SPACE_S: f32 = 8.0;
pub(crate) const SPACE_M: f32 = 12.0;
pub(crate) const SPACE_L: f32 = 16.0;
pub(crate) const SPACE_XL: f32 = 24.0;
pub(crate) const THUMBNAIL_SIZE: f32 = 28.0;
pub(crate) const CONTROL_M: f32 = 28.0;
pub(crate) const CONTROL_L: f32 = 32.0;
pub(crate) const ICON_BUTTON_SIZE: f32 = CONTROL_M;
pub(crate) const RADIUS_CONTROL: f32 = 6.0;
pub(crate) const RADIUS_CARD: f32 = 8.0;
pub(crate) const RADIUS_OVERLAY: f32 = 10.0;
pub(crate) const HEADER_HEIGHT: f32 = 50.0;
pub(crate) const FOOTER_HEIGHT: f32 = 32.0;
pub(crate) const SEARCH_HEIGHT: f32 = 36.0;
pub(crate) const PREVIEW_PANEL_WIDTH: f32 = 320.0;
/// Below this window width the preview side panel is hidden.
pub(crate) const PREVIEW_PANEL_MIN_WINDOW: f32 = 720.0;
pub(crate) const WARNING: Color32 = Color32::from_rgb(239, 190, 98);
pub(crate) const DANGER: Color32 = Color32::from_rgb(249, 130, 146);

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

pub(crate) fn secondary_text(ui: &Ui) -> Color32 {
    secondary_text_for(ui.visuals().dark_mode)
}

pub(crate) fn selected_secondary_text(ui: &Ui) -> Color32 {
    selected_secondary_text_for(ui.visuals().dark_mode)
}

pub(crate) fn border_strong(ui: &Ui) -> Color32 {
    border_strong_for(ui.visuals().dark_mode)
}

pub(crate) fn sunken_bg(ui: &Ui) -> Color32 {
    sunken_bg_for(ui.visuals().dark_mode)
}

pub(crate) fn border(ui: &Ui) -> Color32 {
    border_for(ui.visuals().dark_mode)
}

pub(crate) fn text_primary(ui: &Ui) -> Color32 {
    text_primary_for(ui.visuals().dark_mode)
}

pub(crate) fn faint_text(ui: &Ui) -> Color32 {
    faint_text_for(ui.visuals().dark_mode)
}

pub(crate) fn tile_bg(ui: &Ui) -> Color32 {
    tile_bg_for(ui.visuals().dark_mode)
}

pub(crate) fn tile_bg_selected(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x15, 0x30, 0x3A)
    } else {
        Color32::from_rgb(0xCF, 0xE7, 0xEC)
    }
}

pub(crate) fn selected_row_bg(ui: &Ui) -> Color32 {
    selected_row_bg_for(ui.visuals().dark_mode)
}

pub(crate) fn selected_row_border(ui: &Ui) -> Color32 {
    selected_row_border_for(ui.visuals().dark_mode)
}

pub(crate) fn warning_bg(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x2A, 0x24, 0x18)
    } else {
        Color32::from_rgb(0xFB, 0xF3, 0xE1)
    }
}

pub(crate) fn warning_border(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x3E, 0x35, 0x22)
    } else {
        Color32::from_rgb(0xEB, 0xD9, 0xAE)
    }
}

pub(crate) fn code_bg(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x0C, 0x0F, 0x13)
    } else {
        Color32::from_rgb(0xF2, 0xF5, 0xF8)
    }
}

pub(crate) fn code_border(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x23, 0x29, 0x33)
    } else {
        Color32::from_rgb(0xE2, 0xE8, 0xEE)
    }
}

pub(crate) fn segment_active_bg(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x23, 0x2B, 0x34)
    } else {
        Color32::from_rgb(0xE7, 0xED, 0xF2)
    }
}

pub(crate) fn on_accent(ui: &Ui) -> Color32 {
    on_accent_for(ui.visuals().dark_mode)
}

const fn window_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x14, 0x17, 0x1C)
    } else {
        Color32::from_rgb(0xF7, 0xF8, 0xFA)
    }
}

const fn panel_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x17, 0x1B, 0x21)
    } else {
        Color32::WHITE
    }
}

const fn sunken_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x0F, 0x12, 0x15)
    } else {
        Color32::WHITE
    }
}

const fn border_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x26, 0x2B, 0x33)
    } else {
        Color32::from_rgb(0xE7, 0xEA, 0xEE)
    }
}

const fn text_primary_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xE9, 0xED, 0xF1)
    } else {
        Color32::from_rgb(0x17, 0x1D, 0x24)
    }
}

const fn faint_text_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x5A, 0x65, 0x72)
    } else {
        Color32::from_rgb(0x96, 0xA0, 0xAB)
    }
}

const fn tile_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x1C, 0x22, 0x2B)
    } else {
        Color32::from_rgb(0xED, 0xF1, 0xF5)
    }
}

const fn selected_row_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x1D, 0x27, 0x32)
    } else {
        Color32::from_rgb(0xE3, 0xF0, 0xF4)
    }
}

const fn selected_row_border_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x2E, 0x45, 0x5A)
    } else {
        Color32::from_rgb(0xB9, 0xDA, 0xE2)
    }
}

const fn accent_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x5C, 0xC9, 0xD8)
    } else {
        Color32::from_rgb(0x0B, 0x60, 0x72)
    }
}

const fn success_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x66, 0xD6, 0x9C)
    } else {
        Color32::from_rgb(0x12, 0x81, 0x3F)
    }
}

const fn warning_for(dark: bool) -> Color32 {
    if dark {
        WARNING
    } else {
        Color32::from_rgb(0x8A, 0x5E, 0x00)
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

const fn secondary_text_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x87, 0x92, 0xA0)
    } else {
        Color32::from_rgb(0x66, 0x71, 0x7E)
    }
}

const fn selected_secondary_text_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(205, 213, 224)
    } else {
        Color32::from_rgb(61, 72, 86)
    }
}

const fn border_strong_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(112, 122, 136)
    } else {
        Color32::from_rgb(102, 113, 126)
    }
}

const fn on_accent_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x08, 0x26, 0x2C)
    } else {
        Color32::WHITE
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Delete,
    Pin { filled: bool },
    Close,
    Add,
    Copy,
    Paste,
    Up,
    Down,
    Duplicate,
    Menu,
    Settings,
    Eye,
    Undo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconButtonKind {
    Ghost,
    Toolbar,
    Primary,
    Danger,
}

pub(crate) fn apply(ctx: &egui::Context, reduced_motion: bool) {
    let theme = ctx.theme();
    let mut style = (*ctx.style_of(theme)).clone();
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(12.5));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(12.5));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(15.0));
    style.spacing.item_spacing = egui::vec2(SPACE_S, SPACE_S);
    style.spacing.button_padding = egui::vec2(SPACE_M, SPACE_XS);
    style.spacing.interact_size = egui::vec2(ICON_BUTTON_SIZE, ICON_BUTTON_SIZE);
    style.animation_time = if reduced_motion { 0.0 } else { 1.0 / 12.0 };
    style.scroll_animation = if reduced_motion {
        egui::style::ScrollAnimation::none()
    } else {
        egui::style::ScrollAnimation::default()
    };
    style.visuals.window_corner_radius = egui::CornerRadius::same(RADIUS_OVERLAY as u8);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(RADIUS_OVERLAY as u8);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(RADIUS_CONTROL as u8);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(RADIUS_CONTROL as u8);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(RADIUS_CONTROL as u8);
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(RADIUS_CONTROL as u8);
    let dark = style.visuals.dark_mode;
    style.visuals.weak_text_color = Some(secondary_text_for(dark));
    style.visuals.selection.bg_fill = selected_row_bg_for(dark);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, selected_row_border_for(dark));
    style.visuals.warn_fg_color = warning_for(dark);
    style.visuals.error_fg_color = danger_for(dark);
    style.visuals.panel_fill = window_bg_for(dark);
    style.visuals.window_fill = panel_bg_for(dark);
    style.visuals.faint_bg_color = tile_bg_for(dark);
    style.visuals.extreme_bg_color = sunken_bg_for(dark);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border_for(dark));
    if dark {
        style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x7B, 0xD8, 0xE4));
        style.visuals.hyperlink_color = Color32::from_rgb(0x7B, 0xD8, 0xE4);
        style.visuals.code_bg_color = Color32::from_rgb(0x0C, 0x0F, 0x13);
    } else {
        style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x0B, 0x60, 0x72));
        style.visuals.hyperlink_color = Color32::from_rgb(0x0B, 0x60, 0x72);
        style.visuals.code_bg_color = Color32::from_rgb(0xF2, 0xF5, 0xF8);
    }
    ctx.set_style_of(theme, style);
}

/// One pill of the header History/Stack segmented control.
pub(crate) fn navigation_tab(ui: &mut Ui, label: &'static str, selected: bool) -> Response {
    let galley_width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(12.0),
            Color32::PLACEHOLDER,
        )
        .rect
        .width();
    let size = Vec2::new(galley_width + 28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, rect.height() / 2.0, segment_active_bg(ui));
    } else if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            rect.height() / 2.0,
            segment_active_bg(ui).gamma_multiply(0.5),
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            rect.height() / 2.0,
            Stroke::new(1.5_f32, accent(ui)),
            StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        if selected {
            text_primary(ui)
        } else {
            secondary_text(ui)
        },
    );
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, label)
    });
    response
}

/// A small rounded status pill: `Protection partial · 54`, `lossless`, `local`.
pub(crate) fn badge_pill(ui: &mut Ui, label: &str, fg: Color32, bg: Color32, border: Option<Color32>) {
    let font = FontId::proportional(10.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), fg);
    let size = Vec2::new(galley.rect.width() + 14.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, rect.height() / 2.0, bg);
    if let Some(border) = border {
        ui.painter().rect_stroke(
            rect,
            rect.height() / 2.0,
            Stroke::new(1.0_f32, border),
            StrokeKind::Inside,
        );
    }
    ui.painter()
        .text(rect.center(), egui::Align2::CENTER_CENTER, label, font, fg);
}

/// A keyboard-shortcut cap: `⌘F`, `⌘1`, `↵`.
pub(crate) fn keycap(ui: &mut Ui, label: &str, highlighted: bool) {
    let fg = if highlighted {
        accent(ui)
    } else {
        faint_text(ui)
    };
    let border = if highlighted {
        selected_row_border(ui)
    } else {
        border(ui)
    };
    let font = FontId::monospace(10.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), fg);
    let size = Vec2::new(galley.rect.width() + 10.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_stroke(rect, 4.0, Stroke::new(1.0_f32, border), StrokeKind::Inside);
    ui.painter()
        .text(rect.center(), egui::Align2::CENTER_CENTER, label, font, fg);
}

pub(crate) fn section_heading(ui: &mut Ui, title: &str, detail: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().size(15.0));
        if let Some(detail) = detail {
            ui.label(egui::RichText::new(detail).small().weak());
        }
    });
}

/// The 22px rounded accent "v" mark from the header.
pub(crate) fn logo_tile(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::drag());
    ui.painter().rect_filled(rect, 6.0, accent(ui));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "v",
        FontId::monospace(13.0),
        on_accent_for(ui.visuals().dark_mode),
    );
    response
}

/// A fixed-size symbol button with a tooltip and no font-dependent emoji.
pub(crate) fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &'static str,
    selected: bool,
) -> Response {
    icon_button_kind(ui, icon, tooltip, selected, IconButtonKind::Toolbar)
}

pub(crate) fn icon_button_kind(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &'static str,
    selected: bool,
    kind: IconButtonKind,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_SIZE), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    let semantic = match kind {
        IconButtonKind::Primary => accent(ui),
        IconButtonKind::Danger => danger(ui),
        IconButtonKind::Ghost | IconButtonKind::Toolbar => visuals.fg_stroke.color,
    };
    let bg_fill = match kind {
        IconButtonKind::Toolbar => visuals.weak_bg_fill,
        IconButtonKind::Ghost if selected || response.hovered() || response.has_focus() => {
            visuals.weak_bg_fill
        }
        IconButtonKind::Ghost => Color32::TRANSPARENT,
        IconButtonKind::Primary => semantic,
        IconButtonKind::Danger if response.hovered() || response.has_focus() => {
            Color32::from_rgba_unmultiplied(semantic.r(), semantic.g(), semantic.b(), 34)
        }
        IconButtonKind::Danger => Color32::TRANSPARENT,
    };
    let bg_stroke = if response.has_focus() {
        Stroke::new(2.0_f32, accent(ui))
    } else if kind == IconButtonKind::Toolbar {
        visuals.bg_stroke
    } else {
        Stroke::NONE
    };
    ui.painter()
        .rect(rect, RADIUS_CONTROL, bg_fill, bg_stroke, StrokeKind::Inside);

    let icon_color = match kind {
        IconButtonKind::Primary => on_accent_for(ui.visuals().dark_mode),
        IconButtonKind::Danger => semantic,
        IconButtonKind::Ghost | IconButtonKind::Toolbar => visuals.fg_stroke.color,
    };
    let stroke = Stroke::new(1.5_f32, icon_color);
    let center = rect.center();
    match icon {
        Icon::Delete => draw_delete(ui, center, stroke),
        Icon::Pin { filled } => draw_pin(ui, center, stroke, filled),
        Icon::Close => draw_close(ui, center, stroke),
        Icon::Add => draw_add(ui, center, stroke),
        Icon::Copy => draw_copy(ui, center, stroke),
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

fn draw_copy(ui: &Ui, center: Pos2, stroke: Stroke) {
    let back = Rect::from_center_size(center + egui::vec2(-2.0, -2.0), egui::vec2(8.0, 9.0));
    let front = Rect::from_center_size(center + egui::vec2(2.0, 2.0), egui::vec2(8.0, 9.0));
    ui.painter()
        .rect_stroke(back, 1.0, stroke, StrokeKind::Inside);
    ui.painter()
        .rect_stroke(front, 1.0, stroke, StrokeKind::Inside);
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
            (true, window_bg_for(true)),
            (false, window_bg_for(false)),
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

    #[test]
    fn secondary_text_passes_wcag_aa_on_both_panel_themes() {
        for (dark, background) in [
            (true, window_bg_for(true)),
            (false, window_bg_for(false)),
        ] {
            let foreground = secondary_text_for(dark);
            let ratio = contrast_ratio(
                [foreground.r(), foreground.g(), foreground.b()],
                [background.r(), background.g(), background.b()],
            );
            assert!(ratio >= 4.5, "secondary text contrast was {ratio:.2}:1");
        }
    }

    #[test]
    fn strong_borders_pass_non_text_contrast_on_both_panel_themes() {
        for (dark, background) in [
            (true, window_bg_for(true)),
            (false, window_bg_for(false)),
        ] {
            let foreground = border_strong_for(dark);
            let ratio = contrast_ratio(
                [foreground.r(), foreground.g(), foreground.b()],
                [background.r(), background.g(), background.b()],
            );
            assert!(ratio >= 3.0, "strong border contrast was {ratio:.2}:1");
        }
    }

    #[test]
    fn selected_secondary_text_passes_wcag_aa_on_selection_fills() {
        for (dark, background) in [
            (true, selected_row_bg_for(true)),
            (false, selected_row_bg_for(false)),
        ] {
            let foreground = selected_secondary_text_for(dark);
            let ratio = contrast_ratio(
                [foreground.r(), foreground.g(), foreground.b()],
                [background.r(), background.g(), background.b()],
            );
            assert!(ratio >= 4.5, "selected metadata contrast was {ratio:.2}:1");
        }
    }
}
