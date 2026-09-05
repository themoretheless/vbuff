//! Bounded image decoding for native egui previews.

use std::io::Cursor;

use egui::TextureHandle;
use vbuff_types::{Body, is_rgba_mime, parse_rgba_dims_checked};

const MAX_DECODE_DIMENSION: u32 = 4_096;
const MAX_DECODE_RGBA_BYTES: u64 = 64 * 1024 * 1024;
const TEXTURE_EDGE: u32 = 320;

/// Decode on one background worker, with bounded pending work and texture memory.
/// No worker is started until the first visible image is requested.
#[derive(Default)]
pub(crate) struct ThumbnailCache {
    entries: std::collections::HashMap<String, (Option<TextureHandle>, u64)>,
    pending: std::collections::HashSet<String>,
    worker: Option<ThumbnailWorker>,
    tick: u64,
    loader: Option<ThumbnailLoader>,
}

pub type ThumbnailLoader =
    std::sync::Arc<dyn Fn(vbuff_types::ClipId) -> Option<vbuff_types::Flavor> + Send + Sync>;

struct ThumbnailWorker {
    sender: std::sync::mpsc::SyncSender<(String, vbuff_types::Flavor, egui::Context)>,
    receiver: std::sync::mpsc::Receiver<(String, Option<egui::ColorImage>)>,
}

impl ThumbnailCache {
    // 32 RGBA textures of at most 320x320 pixels occupy at most 12.5 MiB.
    const MAX_ENTRIES: usize = 32;
    const MAX_PENDING: usize = 2;

    pub(crate) fn set_loader(&mut self, loader: ThumbnailLoader) {
        self.loader = Some(loader);
        self.worker = None;
        self.pending.clear();
        self.entries.clear();
    }

    pub(crate) fn retain(&mut self, live: &std::collections::HashSet<String>) {
        self.entries.retain(|key, _| live.contains(key));
        // Pending completions are checked against live IDs before uploading.
    }

    pub(crate) fn poll(&mut self, ctx: &egui::Context, live: &std::collections::HashSet<String>) {
        let Some(worker) = &self.worker else { return };
        while let Ok((key, decoded)) = worker.receiver.try_recv() {
            self.pending.remove(&key);
            if !live.contains(&key) {
                continue;
            }
            let texture =
                decoded.map(|pixels| ctx.load_texture(&key, pixels, egui::TextureOptions::LINEAR));
            self.entries.insert(key, (texture, self.tick));
        }
        self.evict();
    }

    fn evict(&mut self) {
        while self.entries.len() > Self::MAX_ENTRIES {
            let key = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone());
            if let Some(key) = key {
                self.entries.remove(&key);
            }
        }
    }

    pub(crate) fn get(
        &mut self,
        ctx: &egui::Context,
        flavor: &vbuff_types::Flavor,
        key: String,
    ) -> Option<TextureHandle> {
        self.tick = self.tick.wrapping_add(1);
        if let Some((texture, used)) = self.entries.get_mut(&key) {
            *used = self.tick;
            return texture.clone();
        }
        if self.pending.contains(&key) || self.pending.len() >= Self::MAX_PENDING {
            return None;
        }
        let loader = self.loader.clone();
        let worker = self.worker.get_or_insert_with(|| {
            let (sender, jobs) =
                std::sync::mpsc::sync_channel::<(String, vbuff_types::Flavor, egui::Context)>(
                    Self::MAX_PENDING,
                );
            let (completed, receiver) = std::sync::mpsc::sync_channel(Self::MAX_PENDING);
            std::thread::spawn(move || {
                while let Ok((key, flavor, ctx)) = jobs.recv() {
                    let loaded = if matches!(flavor.body, Body::Spilled { .. }) {
                        vbuff_types::ClipId::parse(&key)
                            .ok()
                            .and_then(|id| loader.as_ref().and_then(|load| load(id)))
                    } else {
                        Some(flavor)
                    };
                    let decoded = loaded.as_ref().and_then(decode_thumbnail);
                    if completed.send((key, decoded)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
            ThumbnailWorker { sender, receiver }
        });
        if worker
            .sender
            .try_send((key.clone(), flavor.clone(), ctx.clone()))
            .is_ok()
        {
            self.pending.insert(key);
        }
        None
    }
}

fn decode_thumbnail(flavor: &vbuff_types::Flavor) -> Option<egui::ColorImage> {
    let bytes = match &flavor.body {
        Body::Inline(bytes) => bytes,
        Body::Spilled { .. } => return None,
    };

    if is_rgba_mime(&flavor.mime) {
        let (width, height, required) = parse_rgba_dims_checked(&flavor.mime)?;
        if required != bytes.len() || u64::try_from(required).ok()? > MAX_DECODE_RGBA_BYTES {
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
