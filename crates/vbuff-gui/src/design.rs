//! Shared layout tokens and small native icon buttons for the popup.

use egui::{
    Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2,
    WidgetInfo, WidgetType,
};

// Unified Style Guide tokens: warm neutrals (canvas / surface / raised),
// a terracotta accent, IBM Plex-style sizes, radius 8 for cards and 12 for the
// window. Three History layouts share them:
//   Cockpit (1b): list + permanent 320px preview column, segmented pills.
//   Rail (1c): 44px icon rail, search in the header, one-line rows, 300px preview.
//   Cards (1d): underlined tabs, search as a heading, card rows, 340px code card.
// Window 860x560; the preview column hides below 720.
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
/// Rail layout: a narrower 300px preview column next to the 44px rail.
pub(crate) const RAIL_PREVIEW_PANEL_WIDTH: f32 = 300.0;
/// Cards layout: the floating 340px code card (12px margin included).
pub(crate) const CARDS_PREVIEW_PANEL_WIDTH: f32 = 340.0;
/// Width of the vertical navigation rail in the Rail layout.
pub(crate) const RAIL_WIDTH: f32 = 44.0;
/// Rail-layout navigation buttons.
pub(crate) const RAIL_BUTTON_SIZE: f32 = 30.0;
/// Below this window width the preview side panel is hidden.
pub(crate) const PREVIEW_PANEL_MIN_WINDOW: f32 = 720.0;
pub(crate) const WARNING: Color32 = Color32::from_rgb(0xD9, 0xA0, 0x57);
pub(crate) const DANGER: Color32 = Color32::from_rgb(0xF0, 0x70, 0x5E);

/// Text-safe accent: terracotta on dark, a darker ink on the light canvas.
pub(crate) fn accent(ui: &Ui) -> Color32 {
    accent_for(ui.visuals().dark_mode)
}

/// Solid accent fill for primary buttons and active pills (pair with
/// [`on_accent`]). Follows the guide's `accent` token in both themes.
pub(crate) fn accent_fill(ui: &Ui) -> Color32 {
    accent_fill_for(ui.visuals().dark_mode)
}

/// 13%-alpha accent wash behind accent-coloured badges.
pub(crate) fn accent_wash(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x3A, 0x2C, 0x22)
    } else {
        Color32::from_rgb(0xF1, 0xDF, 0xCF)
    }
}

/// Hover fill for rows and rail buttons (`surfaceHover` in the guide).
pub(crate) fn surface_hover(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x3A, 0x37, 0x33)
    } else {
        Color32::from_rgb(0xEE, 0xE9, 0xDF)
    }
}

/// Raised card fill (`surfaceRaised`): search field, active pill, cards.
pub(crate) fn surface_raised(ui: &Ui) -> Color32 {
    sunken_bg_for(ui.visuals().dark_mode)
}

/// Text inside the dark code block; the same cream in both themes.
pub(crate) fn code_text(_ui: &Ui) -> Color32 {
    Color32::from_rgb(0xF1, 0xE6, 0xC4)
}

/// 13%-alpha success wash behind `read ok` style badges.
pub(crate) fn success_wash(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x28, 0x35, 0x2E)
    } else {
        Color32::from_rgb(0xDC, 0xE8, 0xE0)
    }
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
        Color32::from_rgb(0x4A, 0x32, 0x20)
    } else {
        Color32::from_rgb(0xF3, 0xDC, 0xC8)
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
        Color32::from_rgb(0x41, 0x38, 0x28)
    } else {
        Color32::from_rgb(0xEC, 0xE1, 0xD1)
    }
}

pub(crate) fn warning_border(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x5C, 0x4A, 0x2E)
    } else {
        Color32::from_rgb(0xE0, 0xC9, 0xA6)
    }
}

/// The code block is a dark cream-on-charcoal card in both themes.
pub(crate) fn code_bg(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x15, 0x14, 0x12)
    } else {
        Color32::from_rgb(0x2B, 0x2A, 0x26)
    }
}

pub(crate) fn code_border(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x54, 0x50, 0x4A)
    } else {
        Color32::from_rgb(0x2B, 0x2A, 0x26)
    }
}

pub(crate) fn segment_active_bg(ui: &Ui) -> Color32 {
    sunken_bg_for(ui.visuals().dark_mode)
}

pub(crate) fn on_accent(ui: &Ui) -> Color32 {
    on_accent_for(ui.visuals().dark_mode)
}

/// `canvas`: the window background.
const fn window_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x1F, 0x1E, 0x1B)
    } else {
        Color32::from_rgb(0xF4, 0xF1, 0xEA)
    }
}

/// `surface`: header, footer and preview column fill.
const fn panel_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x2A, 0x28, 0x25)
    } else {
        Color32::from_rgb(0xFB, 0xF9, 0xF4)
    }
}

/// `surfaceRaised`: search field, active pill, cards.
const fn sunken_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x33, 0x30, 0x2C)
    } else {
        Color32::WHITE
    }
}

const fn border_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x54, 0x50, 0x4A)
    } else {
        Color32::from_rgb(0xDE, 0xD7, 0xCA)
    }
}

const fn text_primary_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xF1, 0xE6, 0xC4)
    } else {
        Color32::from_rgb(0x2B, 0x2A, 0x26)
    }
}

/// `textDisabled`.
const fn faint_text_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x7A, 0x73, 0x6A)
    } else {
        Color32::from_rgb(0xA3, 0x9C, 0x90)
    }
}

const fn tile_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x33, 0x30, 0x2C)
    } else {
        Color32::from_rgb(0xEE, 0xE9, 0xDF)
    }
}

/// Accent selection wash (38% on dark, 22% on light) flattened over the canvas.
const fn selected_row_bg_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x5E, 0x3B, 0x24)
    } else {
        Color32::from_rgb(0xEF, 0xD3, 0xBC)
    }
}

const fn selected_row_border_for(dark: bool) -> Color32 {
    accent_fill_for(dark)
}

const fn accent_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xF2, 0x85, 0x3A)
    } else {
        Color32::from_rgb(0xA5, 0x4E, 0x1F)
    }
}

const fn accent_fill_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xF2, 0x85, 0x3A)
    } else {
        Color32::from_rgb(0xDE, 0x6A, 0x19)
    }
}

const fn success_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x6F, 0xB0, 0x8F)
    } else {
        Color32::from_rgb(0x2F, 0x6A, 0x50)
    }
}

const fn warning_for(dark: bool) -> Color32 {
    if dark {
        WARNING
    } else {
        Color32::from_rgb(0x8A, 0x5A, 0x1A)
    }
}

const fn danger_for(dark: bool) -> Color32 {
    if dark {
        DANGER
    } else {
        Color32::from_rgb(0xB9, 0x2E, 0x1E)
    }
}

const fn info_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x7E, 0xC6, 0xEE)
    } else {
        Color32::from_rgb(0x2C, 0x5F, 0x8A)
    }
}

/// `textMuted`.
const fn secondary_text_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xB8, 0xAA, 0x8F)
    } else {
        Color32::from_rgb(0x6B, 0x66, 0x5C)
    }
}

const fn selected_secondary_text_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0xF1, 0xE6, 0xC4)
    } else {
        Color32::from_rgb(0x5C, 0x4A, 0x3A)
    }
}

const fn border_strong_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x7A, 0x73, 0x6A)
    } else {
        Color32::from_rgb(0x8A, 0x83, 0x77)
    }
}

const fn on_accent_for(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x1F, 0x1E, 0x1B)
    } else {
        Color32::WHITE
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Delete,
    Pin {
        filled: bool,
    },
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
    /// Rail navigation: a clock face for History.
    History,
    /// Rail navigation: stacked layers for Stack.
    Stack,
    /// Rail navigation: a shield for Privacy.
    Shield,
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
    style.visuals.selection.stroke = Stroke::new(1.0_f32, accent_fill_for(dark));
    style.visuals.hyperlink_color = accent_for(dark);
    style.visuals.code_bg_color = tile_bg_for(dark);
    style.visuals.widgets.inactive.weak_bg_fill = tile_bg_for(dark);
    style.visuals.widgets.hovered.weak_bg_fill = if dark {
        Color32::from_rgb(0x3A, 0x37, 0x33)
    } else {
        Color32::from_rgb(0xEE, 0xE9, 0xDF)
    };
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, border_strong_for(dark));
    style.visuals.widgets.active.weak_bg_fill = selected_row_bg_for(dark);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent_fill_for(dark));
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border_for(dark));
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text_primary_for(dark));
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, text_primary_for(dark));
    style.visuals.widgets.active.fg_stroke = Stroke::new(2.0_f32, text_primary_for(dark));
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text_primary_for(dark));
    style.visuals.override_text_color = None;
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
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, label));
    response
}

/// A small rounded status pill: `Protection partial · 54`, `lossless`, `local`.
pub(crate) fn badge_pill(
    ui: &mut Ui,
    label: &str,
    fg: Color32,
    bg: Color32,
    border: Option<Color32>,
) {
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
    ui.painter().rect_filled(rect, 6.0, accent_fill(ui));
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
        IconButtonKind::Primary => accent_fill(ui),
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
        Icon::History => draw_history(ui, center, stroke),
        Icon::Stack => draw_stack(ui, center, stroke),
        Icon::Shield => draw_shield(ui, center, stroke),
    }

    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, tooltip)
    });
    response.on_hover_text(tooltip)
}

/// Rail layout: a 30px icon button with a 2px accent bar on its left when
/// selected (the `r.bar` marker from 1c).
pub(crate) fn rail_button(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &'static str,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(RAIL_BUTTON_SIZE), Sense::click());
    let hovered = response.hovered() || response.has_focus();
    let fill = if selected {
        accent_wash(ui)
    } else if hovered {
        surface_hover(ui)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 7.0, fill);
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            7.0,
            Stroke::new(1.5_f32, accent_fill(ui)),
            StrokeKind::Inside,
        );
    }
    if selected {
        let bar = Rect::from_min_size(
            Pos2::new(rect.left() - 7.0, rect.top() + 8.0),
            Vec2::new(2.0, 14.0),
        );
        ui.painter().rect_filled(bar, 1.0, accent_fill(ui));
    }
    let color = if selected {
        accent(ui)
    } else if hovered {
        text_primary(ui)
    } else {
        secondary_text(ui)
    };
    let stroke = Stroke::new(1.5_f32, color);
    let center = rect.center();
    match icon {
        Icon::History => draw_history(ui, center, stroke),
        Icon::Stack => draw_stack(ui, center, stroke),
        Icon::Shield => draw_shield(ui, center, stroke),
        Icon::Settings => draw_settings(ui, center, stroke),
        Icon::Close => draw_close(ui, center, stroke),
        Icon::Menu => draw_menu(ui, center, stroke),
        _ => draw_add(ui, center, stroke),
    }
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, tooltip)
    });
    response.on_hover_text(tooltip)
}

/// Cards layout: a text tab with a 2px accent underline when selected.
pub(crate) fn underline_tab(ui: &mut Ui, label: &'static str, selected: bool) -> Response {
    let font = FontId::proportional(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::PLACEHOLDER);
    let size = Vec2::new(galley.rect.width() + 4.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hovered = response.hovered() || response.has_focus();
    let color = if selected || hovered {
        text_primary(ui)
    } else {
        secondary_text(ui)
    };
    ui.painter().text(
        Pos2::new(rect.left() + 2.0, rect.top() + 9.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    if selected {
        let underline = Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() - 2.0),
            Vec2::new(rect.width(), 2.0),
        );
        ui.painter().rect_filled(underline, 1.0, accent_fill(ui));
    } else if response.has_focus() {
        let underline = Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() - 2.0),
            Vec2::new(rect.width(), 2.0),
        );
        ui.painter().rect_filled(underline, 1.0, border_strong(ui));
    }
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, label));
    response
}

/// A small uppercase mono section label with a hairline and a trailing count:
/// `TODAY ─────── 3` from the Rail layout.
pub(crate) fn section_rule(ui: &mut Ui, title: &str, count: usize) {
    let font = FontId::monospace(10.0);
    let color = secondary_text(ui);
    let title = title.to_uppercase();
    let count = count.to_string();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let title_galley = ui
        .painter()
        .layout_no_wrap(title.clone(), font.clone(), color);
    let count_galley = ui
        .painter()
        .layout_no_wrap(count.clone(), font.clone(), color);
    ui.painter().text(
        Pos2::new(rect.left() + SPACE_M, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        font.clone(),
        color,
    );
    ui.painter().text(
        Pos2::new(rect.right() - SPACE_M, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        count,
        font,
        color,
    );
    let line_left = rect.left() + SPACE_M + title_galley.rect.width() + 6.0;
    let line_right = rect.right() - SPACE_M - count_galley.rect.width() - 6.0;
    if line_right > line_left {
        ui.painter().hline(
            line_left..=line_right,
            rect.center().y,
            Stroke::new(1.0_f32, border(ui)),
        );
    }
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

fn draw_history(ui: &Ui, center: Pos2, stroke: Stroke) {
    ui.painter().circle_stroke(center, 6.0, stroke);
    ui.painter().line_segment(
        [
            center + egui::vec2(0.0, -3.5),
            center + egui::vec2(0.0, 0.5),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [center + egui::vec2(0.0, 0.5), center + egui::vec2(2.5, 2.0)],
        stroke,
    );
}

fn draw_stack(ui: &Ui, center: Pos2, stroke: Stroke) {
    for offset in [-3.5, 0.0, 3.5] {
        let points = vec![
            center + egui::vec2(-6.0, offset),
            center + egui::vec2(0.0, offset - 3.0),
            center + egui::vec2(6.0, offset),
            center + egui::vec2(0.0, offset + 3.0),
        ];
        ui.painter().add(Shape::closed_line(points, stroke));
    }
}

fn draw_shield(ui: &Ui, center: Pos2, stroke: Stroke) {
    let points = vec![
        center + egui::vec2(0.0, -6.5),
        center + egui::vec2(5.5, -4.0),
        center + egui::vec2(5.0, 2.0),
        center + egui::vec2(0.0, 6.5),
        center + egui::vec2(-5.0, 2.0),
        center + egui::vec2(-5.5, -4.0),
    ];
    ui.painter().add(Shape::closed_line(points, stroke));
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
            center + egui::vec2(3.0, -2.0),
        ],
        stroke,
    );
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
        for (dark, background) in [(true, window_bg_for(true)), (false, window_bg_for(false))] {
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
        for (dark, background) in [(true, window_bg_for(true)), (false, window_bg_for(false))] {
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
        for (dark, background) in [(true, window_bg_for(true)), (false, window_bg_for(false))] {
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
