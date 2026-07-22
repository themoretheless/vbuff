//! Bounded image decoding for native egui previews.

use std::io::Cursor;

use egui::TextureHandle;
use vbuff_types::Body;

const MAX_DECODE_DIMENSION: u32 = 4_096;
const MAX_DECODE_RGBA_BYTES: u64 = 64 * 1024 * 1024;
const TEXTURE_EDGE: u32 = 320;

pub(crate) fn build_thumbnail(
    ctx: &egui::Context,
    flavor: &vbuff_types::Flavor,
    key: &str,
) -> Option<TextureHandle> {
    let color_image = decode_thumbnail(flavor)?;
    Some(ctx.load_texture(key, color_image, egui::TextureOptions::LINEAR))
}

fn decode_thumbnail(flavor: &vbuff_types::Flavor) -> Option<egui::ColorImage> {
    let bytes = match &flavor.body {
        Body::Inline(bytes) => bytes,
        Body::Spilled { .. } => return None,
    };

    if raw_rgba_mime(&flavor.mime) {
        let (width, height) = parse_rgba_dims(&flavor.mime)?;
        let required = width.checked_mul(height)?.checked_mul(4)?;
        if width == 0
            || height == 0
            || required != bytes.len()
            || u64::try_from(required).ok()? > MAX_DECODE_RGBA_BYTES
        {
            return None;
        }
        if width > TEXTURE_EDGE as usize || height > TEXTURE_EDGE as usize {
            let source = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                width as u32,
                height as u32,
                bytes.as_slice(),
            )?;
            let (target_width, target_height) =
                fit_dimensions(width as u32, height as u32, TEXTURE_EDGE, TEXTURE_EDGE);
            let rgba = image::imageops::thumbnail(&source, target_width, target_height);
            return Some(egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            ));
        }
        return Some(egui::ColorImage::from_rgba_unmultiplied(
            [width, height],
            bytes,
        ));
    }

    let dimensions_reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = dimensions_reader.into_dimensions().ok()?;
    let decoded_bytes = u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(4)?;
    if width == 0
        || height == 0
        || width > MAX_DECODE_DIMENSION
        || height > MAX_DECODE_DIMENSION
        || decoded_bytes > MAX_DECODE_RGBA_BYTES
    {
        return None;
    }

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_RGBA_BYTES);
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let rgba = reader
        .decode()
        .ok()?
        .thumbnail(TEXTURE_EDGE, TEXTURE_EDGE)
        .to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

fn fit_dimensions(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    if width <= max_width && height <= max_height {
        return (width, height);
    }
    let width_scale = f64::from(max_width) / f64::from(width);
    let height_scale = f64::from(max_height) / f64::from(height);
    let scale = width_scale.min(height_scale);
    (
        (f64::from(width) * scale).round().max(1.0) as u32,
        (f64::from(height) * scale).round().max(1.0) as u32,
    )
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

    #[test]
    fn raw_thumbnail_decode_is_bounded_and_downscaled() {
        let valid =
            vbuff_types::Flavor::inline("IMAGE/X-VBUFF-RGBA;width=1;height=1", vec![0, 0, 0, 255]);
        let invalid = vbuff_types::Flavor::inline(
            "image/x-vbuff-rgba;width=18446744073709551615;height=2",
            vec![0; 4],
        );
        let large = vbuff_types::Flavor::inline(
            "image/x-vbuff-rgba;width=640;height=320",
            vec![127; 640 * 320 * 4],
        );

        assert_eq!(decode_thumbnail(&valid).unwrap().size, [1, 1]);
        assert!(decode_thumbnail(&invalid).is_none());
        assert_eq!(decode_thumbnail(&large).unwrap().size, [320, 160]);
    }
}
