use std::io::Cursor;

use vbuff_core::fingerprint::dhash_rgba;
use vbuff_types::{Body, Clip};

const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_DECODED_RGBA_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) fn clip_dhash(clip: &Clip) -> Option<u64> {
    let flavor = clip.flavors.iter().find(|flavor| flavor.is_image())?;
    let Body::Inline(bytes) = &flavor.body else {
        return None;
    };
    if raw_rgba_mime(&flavor.mime) {
        let (width, height) = parse_rgba_dims(&flavor.mime)?;
        let width_u32 = u32::try_from(width).ok()?;
        let height_u32 = u32::try_from(height).ok()?;
        let required = width.checked_mul(height)?.checked_mul(4)?;
        if width_u32 > MAX_IMAGE_DIMENSION
            || height_u32 > MAX_IMAGE_DIMENSION
            || required != bytes.len()
            || u64::try_from(required).ok()? > MAX_DECODED_RGBA_BYTES
        {
            return None;
        }
        return dhash_rgba(bytes, width, height);
    }
    let dimensions_reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = dimensions_reader.into_dimensions().ok()?;
    let rgba_bytes = u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(4)?;
    if rgba_bytes > MAX_DECODED_RGBA_BYTES {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_RGBA_BYTES);
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let rgba = reader.decode().ok()?.to_rgba8();
    dhash_rgba(rgba.as_raw(), rgba.width() as usize, rgba.height() as usize)
}

fn raw_rgba_mime(mime: &str) -> bool {
    mime.split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("image/x-vbuff-rgba"))
}

fn parse_rgba_dims(mime: &str) -> Option<(usize, usize)> {
    let mut width = None;
    let mut height = None;
    for part in mime.split(';') {
        if let Some(value) = part.trim().strip_prefix("width=") {
            width = value.parse().ok();
        } else if let Some(value) = part.trim().strip_prefix("height=") {
            height = value.parse().ok();
        }
    }
    Some((width?, height?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{ClipId, ClipMeta, ContentKind, Flavor};

    #[test]
    fn hashes_raw_rgba_clip() {
        let bytes = vec![0_u8; 9 * 8 * 4];
        let clip = Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("image/x-vbuff-rgba;width=9;height=8", bytes)],
            content_hash: [0; 32],
            meta: ClipMeta::now(ContentKind::Image, (9 * 8 * 4) as u64, None),
            pinned: false,
            favorite: false,
        };
        assert_eq!(clip_dhash(&clip), Some(0));
    }
}
