//! Heuristic content-kind detection.
//!
//! Given a set of flavors (or just text), guess the primary [`ContentKind`]
//! used for icons and filtering. Image/file/html/rtf are decided by MIME;
//! url/color/code are text heuristics.

use vbuff_types::{ContentKind, Flavor};

/// Detect the primary content kind of a captured set of flavors.
///
/// MIME-based kinds (image/file/html/rtf) take precedence; otherwise the first
/// text flavor is classified heuristically.
pub fn detect_kind(flavors: &[Flavor]) -> ContentKind {
    // 1. Strong MIME signals first.
    for f in flavors {
        if f.is_image() {
            return ContentKind::Image;
        }
    }
    for f in flavors {
        let mime = f.mime.as_str();
        if mime == "text/uri-list"
            || mime.contains("file-url")
            || mime.contains("Filenames")
            || mime == "application/x-file-list"
        {
            return ContentKind::File;
        }
    }

    // 2. Rich text flavors, but only if there is no usable plain text to
    //    classify more specifically.
    let plain = flavors.iter().find_map(|f| {
        if f.is_text() && !f.mime.contains("html") && !f.mime.contains("rtf") {
            f.as_text()
        } else {
            None
        }
    });

    if plain.is_none() {
        for f in flavors {
            if f.mime.contains("html") {
                return ContentKind::Html;
            }
            if f.mime.contains("rtf") {
                return ContentKind::Rtf;
            }
        }
    }

    // 3. Text heuristics on the plain-text payload.
    match plain {
        Some(text) => detect_text_kind(text),
        None => ContentKind::Other,
    }
}

/// Classify a plain-text payload as Url / Color / Code / Text.
pub fn detect_text_kind(text: &str) -> ContentKind {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ContentKind::Text;
    }
    if is_url(trimmed) {
        return ContentKind::Url;
    }
    if is_color_hex(trimmed) {
        return ContentKind::Color;
    }
    if looks_like_code(trimmed) {
        return ContentKind::Code;
    }
    ContentKind::Text
}

/// True if the whole string is a single URL.
pub fn is_url(s: &str) -> bool {
    // Single token, no internal whitespace, with a recognized scheme.
    if s.split_whitespace().count() != 1 {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    const SCHEMES: &[&str] = &[
        "http://", "https://", "ftp://", "ftps://", "file://", "mailto:", "ssh://", "git://",
    ];
    if SCHEMES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    // Bare `www.example.com/...` style.
    if lower.starts_with("www.") && lower.contains('.') {
        return true;
    }
    false
}

/// True if the string is a CSS-style hex color: `#rgb`, `#rgba`, `#rrggbb`,
/// `#rrggbbaa`.
pub fn is_color_hex(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('#') else {
        return false;
    };
    matches!(rest.len(), 3 | 4 | 6 | 8) && rest.chars().all(|c| c.is_ascii_hexdigit())
}

/// Heuristic: does this text look like source code?
///
/// Intentionally conservative; favors precision over recall so ordinary prose
/// is not mislabeled.
pub fn looks_like_code(s: &str) -> bool {
    let lines: Vec<&str> = s.lines().collect();

    // Single short line: only flag if it is clearly a statement/declaration.
    let strong_tokens = [
        "fn ",
        "def ",
        "class ",
        "function ",
        "import ",
        "#include",
        "public ",
        "private ",
        "const ",
        "let ",
        "var ",
        "=> ",
        "->",
        "::",
        "</",
        "/>",
    ];
    let has_strong = strong_tokens.iter().any(|t| s.contains(t));

    let symbol_lines = lines
        .iter()
        .filter(|l| {
            let t = l.trim_end();
            t.ends_with('{') || t.ends_with('}') || t.ends_with(';') || t.ends_with(':')
        })
        .count();

    let indented_lines = lines
        .iter()
        .filter(|l| l.starts_with("    ") || l.starts_with('\t'))
        .count();

    // Multi-line with code structure, or any line with a strong token.
    if has_strong {
        return true;
    }
    if lines.len() >= 2 && (symbol_lines >= 1 || indented_lines >= 1) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::Flavor;

    #[test]
    fn detects_urls() {
        assert!(is_url("https://example.com/path?q=1"));
        assert!(is_url("http://localhost:8080"));
        assert!(is_url("mailto:a@b.com"));
        assert!(is_url("www.example.com"));
        assert!(!is_url("not a url"));
        assert!(!is_url("see https://x.com here"));
    }

    #[test]
    fn detects_colors() {
        assert!(is_color_hex("#fff"));
        assert!(is_color_hex("#FF8800"));
        assert!(is_color_hex("#ff8800aa"));
        assert!(!is_color_hex("#ggg"));
        assert!(!is_color_hex("ff8800"));
        assert!(!is_color_hex("#12345"));
    }

    #[test]
    fn detects_code() {
        assert!(looks_like_code("fn main() {}"));
        assert!(looks_like_code("def foo():\n    return 1"));
        assert!(looks_like_code("let x = 5;\nlet y = 6;"));
        assert!(!looks_like_code("This is just a sentence about cats."));
        assert!(!looks_like_code("hello world"));
    }

    #[test]
    fn classifies_plain_text() {
        assert_eq!(detect_text_kind("just some words here"), ContentKind::Text);
    }

    #[test]
    fn mime_image_wins() {
        let flavors = vec![
            Flavor::inline("text/plain", b"https://x.com".to_vec()),
            Flavor::inline("image/png", vec![0x89, 0x50]),
        ];
        assert_eq!(detect_kind(&flavors), ContentKind::Image);
    }

    #[test]
    fn url_in_text_flavor_detected() {
        let flavors = vec![Flavor::inline("text/plain", b"https://x.com".to_vec())];
        assert_eq!(detect_kind(&flavors), ContentKind::Url);
    }

    #[test]
    fn html_only_falls_back_to_html() {
        let flavors = vec![Flavor::inline("text/html", b"<b>hi</b>".to_vec())];
        assert_eq!(detect_kind(&flavors), ContentKind::Html);
    }
}
