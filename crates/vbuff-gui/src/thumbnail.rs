//! Image-thumbnail texture cache for the popup's list rows.
//!
//! Building a GPU texture from an image flavor is comparatively expensive, so
//! results are cached by clip id. The cache is explicitly pruned (see
//! [`ThumbnailCache::retain_only`]) whenever the underlying clip list
//! changes, so a texture for a deleted or evicted clip does not sit in GPU
//! memory for the rest of the process lifetime.

use std::collections::{HashMap, HashSet};

use vbuff_types::{Body, Clip, Flavor, parse_rgba_dims};

/// A pruned-on-change cache of decoded image thumbnails, keyed by clip id.
#[derive(Default)]
pub(crate) struct ThumbnailCache {
    cache: HashMap<String, Option<egui::TextureHandle>>,
}

impl ThumbnailCache {
    /// Get (building and caching on first use) the thumbnail texture for a
    /// clip's primary image flavor, or `None` if the clip has no image.
    pub(crate) fn get_or_build(
        &mut self,
        ctx: &egui::Context,
        clip: &Clip,
    ) -> Option<egui::TextureHandle> {
        let image = clip.primary_image()?;
        let key = clip.id.to_string_repr();
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let tex = build_thumbnail(ctx, image, &key);
        self.cache.insert(key, tex.clone());
        tex
    }

    /// Drop every cached texture whose clip id is not in `live_ids`. Call
    /// this whenever the popup's clip snapshot changes (e.g. on every
    /// revision bump) so deleted/evicted clips do not keep their thumbnail
    /// texture alive forever.
    pub(crate) fn retain_only(&mut self, live_ids: &HashSet<String>) {
        self.cache.retain(|key, _| live_ids.contains(key));
    }
}

/// Build a small egui texture from an image flavor.
///
/// Handles both encoded images (PNG/JPEG/BMP) and the raw RGBA flavor that
/// arboard produces (`image/x-vbuff-rgba;width=...;height=...`).
fn build_thumbnail(ctx: &egui::Context, flavor: &Flavor, key: &str) -> Option<egui::TextureHandle> {
    let bytes = match &flavor.body {
        Body::Inline(b) => b,
        Body::Spilled { .. } => return None,
    };

    let color_image = if flavor.mime.starts_with("image/x-vbuff-rgba") {
        let (w, h) = parse_rgba_dims(&flavor.mime)?;
        if w == 0 || h == 0 || bytes.len() < w * h * 4 {
            return None;
        }
        egui::ColorImage::from_rgba_unmultiplied([w, h], &bytes[..w * h * 4])
    } else {
        let img = image::load_from_memory(bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw())
    };

    Some(ctx.load_texture(key, color_image, egui::TextureOptions::LINEAR))
}
