//! Hex-color parsing and the small swatch drawn for `ContentKind::Color` rows.

use egui::Color32;

/// Draw a small filled rectangle for a color clip.
pub(crate) fn draw_color_swatch(ui: &mut egui::Ui, text: &str) {
    let color = parse_hex_color(text).unwrap_or(Color32::GRAY);
    let size = crate::theme::THUMBNAIL_SIZE;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color);
}

/// Parse `#rgb` / `#rrggbb` / `#rrggbbaa` into a Color32.
pub(crate) fn parse_hex_color(s: &str) -> Option<Color32> {
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
