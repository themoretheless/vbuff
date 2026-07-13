//! Structured logging with field-name redaction at the formatter boundary.

use std::fmt;

use tracing::field::Field;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;

pub(crate) fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fields = tracing_subscriber::fmt::format::debug_fn(format_field);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .fmt_fields(fields)
        .init();
}

fn format_field(writer: &mut Writer<'_>, field: &Field, value: &dyn fmt::Debug) -> fmt::Result {
    write!(writer, "{}=", field.name())?;
    if is_sensitive_field(field.name()) {
        writer.write_str("[redacted] ")
    } else {
        let rendered = format!("{value:?}");
        for character in rendered.chars() {
            for escaped in character.escape_default() {
                writer.write_char(escaped)?;
            }
        }
        writer.write_char(' ')
    }
}

fn is_sensitive_field(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "clipboard",
        "content",
        "flavor",
        "payload",
        "preview",
        "text",
        "secret",
        "token",
        "key",
        "nonce",
        "source_url",
        "window_title",
        "document_path",
    ]
    .iter()
    .any(|sensitive| name.contains(sensitive))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_data_fields_are_redacted_by_name() {
        for field in [
            "clipboard_text",
            "payload",
            "source_url",
            "wrapped_key",
            "write_nonce",
        ] {
            assert!(is_sensitive_field(field), "{field}");
        }
        for field in ["clip_id", "byte_size", "kind", "source_app"] {
            assert!(!is_sensitive_field(field), "{field}");
        }
    }
}
