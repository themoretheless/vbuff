//! Canonical format mapping at native clipboard boundaries.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatFamily {
    MacUti,
    Windows,
    Mime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatKey {
    PlainText,
    Html,
    Rtf,
    Png,
    Tiff,
    Bitmap,
    FileList,
    Concealed,
    HistoryControl,
    Other,
}

pub fn canonical_format(family: FormatFamily, native: &str) -> Option<FormatKey> {
    let value = native.trim();
    if value.is_empty() || value.len() > 512 || value.contains('\0') {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let essence = lower.split(';').next().unwrap_or(&lower).trim();
    Some(match family {
        FormatFamily::MacUti => match essence {
            "public.utf8-plain-text" | "public.utf16-plain-text" | "public.text" => {
                FormatKey::PlainText
            }
            "public.html" => FormatKey::Html,
            "public.rtf" | "public.flat-rtfd" => FormatKey::Rtf,
            "public.png" => FormatKey::Png,
            "public.tiff" => FormatKey::Tiff,
            "public.file-url" | "nsfilenamespboardtype" => FormatKey::FileList,
            "org.nspasteboard.concealedtype" => FormatKey::Concealed,
            _ => FormatKey::Other,
        },
        FormatFamily::Windows => match essence {
            "cf_text" | "cf_oemtext" | "cf_unicodetext" => FormatKey::PlainText,
            "html format" => FormatKey::Html,
            "rich text format" | "cf_rtf" => FormatKey::Rtf,
            "png" => FormatKey::Png,
            "cf_bitmap" | "cf_dib" | "cf_dibv5" => FormatKey::Bitmap,
            "cf_hdrop" => FormatKey::FileList,
            "canincludeinclipboardhistory"
            | "canuploadtocloudclipboard"
            | "excludeclipboardcontentfrommonitorprocessing" => FormatKey::HistoryControl,
            _ => FormatKey::Other,
        },
        FormatFamily::Mime => match essence {
            "text" | "text/plain" => FormatKey::PlainText,
            "text/html" => FormatKey::Html,
            "text/rtf" | "application/rtf" => FormatKey::Rtf,
            "image/png" => FormatKey::Png,
            "image/tiff" => FormatKey::Tiff,
            "text/uri-list" | "x-special/gnome-copied-files" => FormatKey::FileList,
            _ => FormatKey::Other,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_aliases_converge_without_losing_unknown_formats() {
        assert_eq!(
            canonical_format(FormatFamily::MacUti, "public.utf8-plain-text"),
            Some(FormatKey::PlainText)
        );
        assert_eq!(
            canonical_format(FormatFamily::Windows, "HTML Format"),
            Some(FormatKey::Html)
        );
        assert_eq!(
            canonical_format(FormatFamily::Mime, "text/plain;charset=utf-8"),
            Some(FormatKey::PlainText)
        );
        assert_eq!(
            canonical_format(FormatFamily::Mime, "application/x-private"),
            Some(FormatKey::Other)
        );
        assert_eq!(
            canonical_format(FormatFamily::Windows, "CanIncludeInClipboardHistory"),
            Some(FormatKey::HistoryControl)
        );
    }
}
