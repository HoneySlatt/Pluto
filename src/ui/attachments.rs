//! Image attachments of user messages (`UserTimelineRow` in
//! `chat/MessagesTimeline.tsx`, `selectMessageImageResources` in
//! `session-logic.ts`): `assets.createUrl` with an `attachment` resource
//! issues a signed URL, fetched over HTTP on a background thread like the
//! project favicons, then decoded and drawn cropped to its cell
//! (`object-cover`), downscaled on the CPU to the drawn pixel size.
//!
//! Only plain-HTTP servers are fetched; an HTTPS server's images keep their
//! name placeholder. Expanding an image (the lightbox) is not ported.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, CornerRadius, Rect, TextureHandle, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::ui::emoji::premultiply;
use crate::ui::favicon::http_get;

#[derive(Clone)]
enum Loaded {
    Pending,
    Missing,
    Image(Arc<image::RgbaImage>),
}

/// Decoded images by attachment id.
fn cache() -> &'static Mutex<HashMap<String, Loaded>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Loaded>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Decodes image bytes into premultiplied RGBA.
pub(crate) fn decode(bytes: &[u8]) -> Option<image::RgbaImage> {
    let mut image = image::load_from_memory(bytes).ok()?.to_rgba8();
    premultiply(&mut image);
    Some(image)
}

/// `source` cropped to `aspect` (width / height) around its center, then
/// resized to `width`x`height` pixels (`object-cover`).
fn cover(source: &image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    let (sw, sh) = (source.width() as f32, source.height() as f32);
    let aspect = width as f32 / height as f32;
    let (cw, ch) = if sw / sh > aspect {
        (sh * aspect, sh)
    } else {
        (sw, sw / aspect)
    };
    let x = ((sw - cw) / 2.0).round() as u32;
    let y = ((sh - ch) / 2.0).round() as u32;
    let cropped = image::imageops::crop_imm(
        source,
        x,
        y,
        (cw.round() as u32).max(1),
        (ch.round() as u32).max(1),
    )
    .to_image();
    image::imageops::resize(
        &cropped,
        width.max(1),
        height.max(1),
        image::imageops::FilterType::Triangle,
    )
}

/// Paints `image` into `rect`, cropped to fill it, with rounded corners.
pub(crate) fn paint_cover(
    ui: &Ui,
    rect: Rect,
    key: &str,
    image: &image::RgbaImage,
    corner_radius: CornerRadius,
) {
    type Textures = HashMap<(String, u32, u32), TextureHandle>;
    static TEXTURES: OnceLock<Mutex<Textures>> = OnceLock::new();
    let ppp = ui.ctx().pixels_per_point();
    let width = (rect.width() * ppp).round() as u32;
    let height = (rect.height() * ppp).round() as u32;
    let Ok(mut textures) = TEXTURES.get_or_init(Default::default).lock() else {
        return;
    };
    let texture = textures
        .entry((key.to_owned(), width, height))
        .or_insert_with(|| {
            let resized = cover(image, width, height);
            ui.ctx().load_texture(
                format!("cover:{key}:{width}x{height}"),
                egui::ColorImage::from_rgba_premultiplied(
                    [resized.width() as usize, resized.height() as usize],
                    resized.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            )
        })
        .clone();
    egui::Image::new((texture.id(), rect.size()))
        .corner_radius(corner_radius)
        .paint_at(ui, rect);
}

/// The message's image attachments (`isImageAttachment`).
pub(crate) fn images(attachments: &[Value]) -> Vec<&Value> {
    attachments
        .iter()
        .filter(|a| a["type"] == "image")
        .collect()
}

/// The size of the images grid at most `max_width` wide: two columns of
/// 4:3 cells (`grid max-w-[210px] grid-cols-2 gap-2`, `aspect-[4/3]`).
pub(crate) fn grid_size(count: usize, max_width: f32) -> (f32, Vec2) {
    let width = max_width.min(210.0);
    let cell = Vec2::new((width - 8.0) / 2.0, (width - 8.0) / 2.0 * 3.0 / 4.0);
    let rows = count.div_ceil(2) as f32;
    (rows * cell.y + (rows - 1.0).max(0.0) * 8.0, cell)
}

impl App {
    /// The attachment's decoded image, asking for it the first time.
    pub(crate) fn attachment_image(&mut self, attachment: &Value) -> Option<Arc<image::RgbaImage>> {
        let id = attachment["id"].as_str()?;
        let known = cache().lock().ok()?.get(id).cloned();
        match known {
            Some(Loaded::Image(image)) => Some(image),
            Some(_) => None,
            None => {
                cache().lock().ok()?.insert(id.to_owned(), Loaded::Pending);
                self.send_request(
                    format!("attachment:{id}"),
                    "assets.createUrl",
                    json!({"resource": {"_tag": "attachment", "attachmentId": id}}),
                );
                None
            }
        }
    }

    /// The signed URL of an attachment: fetch and decode it off the UI thread.
    pub(crate) fn attachment_url(
        &mut self,
        id: &str,
        result: Result<Value, String>,
        ctx: &egui::Context,
    ) {
        let base = url::Url::parse(self.connect.server.trim()).ok();
        let url = result.ok().and_then(|r| {
            let relative = r["relativeUrl"].as_str()?.to_owned();
            base?.join(&relative).ok()
        });
        let Some(url) = url else {
            if let Ok(mut cache) = cache().lock() {
                cache.insert(id.to_owned(), Loaded::Missing);
            }
            return;
        };
        let id = id.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let state = http_get(&url)
                .and_then(|bytes| decode(&bytes))
                .map_or(Loaded::Missing, |image| Loaded::Image(Arc::new(image)));
            if let Ok(mut cache) = cache().lock() {
                cache.insert(id, state);
            }
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_follow_upstream() {
        let (height, cell) = grid_size(1, 500.0);
        assert_eq!(cell, Vec2::new(101.0, 75.75));
        assert_eq!(height, 75.75);
        let (height, _) = grid_size(3, 500.0);
        assert_eq!(height, 75.75 * 2.0 + 8.0);
    }

    #[test]
    fn cover_crops_the_long_side() {
        let source = image::RgbaImage::from_pixel(400, 100, image::Rgba([1, 2, 3, 255]));
        let out = cover(&source, 40, 30);
        assert_eq!((out.width(), out.height()), (40, 30));
    }
}
