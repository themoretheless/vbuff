//! Cross-platform clipboard backend built on `arboard`.
//!
//! `arboard` reads text and image flavors. It cannot enumerate every MIME
//! flavor or read concealed-type markers (that is the job of the future native
//! backends), but it is enough for the MVP: capture text and images, and write
//! them back for paste.

use std::borrow::Cow;

use arboard::{Clipboard, ImageData};
use vbuff_types::{Body, Flavor, RGBA_MIME_PREFIX, parse_rgba_dims, rgba_mime};

use crate::traits::{CapturedClipboard, ClipboardBackend};
use crate::{PlatformError, Result};

/// An `arboard`-backed clipboard.
pub struct ArboardClipboard {
    clipboard: Clipboard,
}

impl ArboardClipboard {
    /// Create a new clipboard handle.
    pub fn new() -> Result<Self> {
        let clipboard = Clipboard::new().map_err(|e| PlatformError::Clipboard(e.to_string()))?;
        Ok(ArboardClipboard { clipboard })
    }
}

impl ClipboardBackend for ArboardClipboard {
    fn read(&mut self) -> Result<CapturedClipboard> {
        let mut flavors = Vec::new();

        // Text flavor.
        match self.clipboard.get_text() {
            Ok(text) if !text.is_empty() => {
                flavors.push(Flavor::inline(
                    "text/plain;charset=utf-8",
                    text.into_bytes(),
                ));
            }
            Ok(_) | Err(arboard::Error::ContentNotAvailable) => {}
            Err(error) => return Err(PlatformError::Clipboard(error.to_string())),
        }

        // Image flavor (raw RGBA). Only attempt if no text was present, to
        // avoid spurious image reads on text-only clipboards on some platforms.
        if flavors.is_empty() {
            match self.clipboard.get_image() {
                Ok(img) => {
                    let mime = format!(
                        "{RGBA_MIME_PREFIX};width={};height={}",
                        img.width, img.height
                    );
                    flavors.push(Flavor::inline(mime, img.bytes.into_owned()));
                }
                Err(arboard::Error::ContentNotAvailable) => {}
                Err(error) => return Err(PlatformError::Clipboard(error.to_string())),
            }
        }

        Ok(CapturedClipboard {
            flavors,
            ..CapturedClipboard::default()
        })
    }

    fn write(&mut self, flavors: &[Flavor]) -> Result<()> {
        // Prefer text; fall back to image. arboard can only hold one kind at a
        // time via its high-level API, so we pick the richest single flavor.
        if let Some(text) = flavors
            .iter()
            .find(|f| f.is_text())
            .and_then(|f| f.as_text())
        {
            return self
                .clipboard
                .set_text(text.to_owned())
                .map_err(|e| PlatformError::Clipboard(e.to_string()));
        }

        if let Some(flavor) = flavors
            .iter()
            .find(|f| f.mime.starts_with(RGBA_MIME_PREFIX))
            && let Body::Inline(bytes) = &flavor.body
        {
            let (w, h) = parse_rgba_dims(&flavor.mime)
                .ok_or_else(|| PlatformError::Clipboard("rgba flavor missing dimensions".into()))?;
            if !rgba_dimensions_match(w, h, bytes.len()) {
                return Err(PlatformError::Clipboard(
                    "rgba flavor byte length does not match its dimensions".into(),
                ));
            }
            let image = ImageData {
                width: w,
                height: h,
                bytes: Cow::Borrowed(bytes),
            };
            return self
                .clipboard
                .set_image(image)
                .map_err(|e| PlatformError::Clipboard(e.to_string()));
        }

        Err(PlatformError::Empty)
    }

    fn clear(&mut self) -> Result<()> {
        self.clipboard
            .clear()
            .map_err(|error| PlatformError::Clipboard(error.to_string()))
    }
}

fn rgba_dimensions_match(width: usize, height: usize, byte_len: usize) -> bool {
    width > 0
        && height > 0
        && width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            == Some(byte_len)
}

/// Parse `width=W;height=H` out of an RGBA MIME string.
fn parse_rgba_dims(mime: &str) -> Option<(usize, usize)> {
    let mut width = None;
    let mut height = None;
    for part in mime.split(';') {
        if let Some(v) = part.trim().strip_prefix("width=") {
            width = v.parse().ok();
        } else if let Some(v) = part.trim().strip_prefix("height=") {
            height = v.parse().ok();
        }
    }
    Some((width?, height?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgba_dims() {
        let mime = "image/x-vbuff-rgba;width=4;height=2";
        assert_eq!(parse_rgba_dims(mime), Some((4, 2)));
    }

    #[test]
    fn missing_dims_is_none() {
        assert_eq!(parse_rgba_dims("image/x-vbuff-rgba"), None);
    }

    #[test]
    fn rgba_dimensions_reject_overflow_and_wrong_lengths() {
        assert!(rgba_dimensions_match(4, 2, 32));
        assert!(!rgba_dimensions_match(4, 2, 31));
        assert!(!rgba_dimensions_match(usize::MAX, 2, 0));
        assert!(!rgba_dimensions_match(0, 2, 0));
    }
}
