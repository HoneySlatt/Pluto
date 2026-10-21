//! Color emoji for project icons. epaint draws no color fonts, so the emoji's
//! bitmap is read from the system's color emoji font (located through
//! fontconfig, as the browser's would be) and drawn as an image; without one,
//! egui's monochrome emoji font is used. Also `firstEmoji`
//! (`projectIconOptions.ts`).
//!
//! Only the sequence's base glyph is looked up: ZWJ sequences and skin tones
//! show their first emoji. The 128px bitmap is downscaled on the CPU to the
//! drawn pixel size (area filtering, like the browser's), since sampling it
//! straight down to icon sizes aliases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, Ui, Vec2};
use skrifa::bitmap::{BitmapData, BitmapStrikes};
use skrifa::instance::Size;
use skrifa::{FontRef, MetadataProvider};
use unicode_segmentation::UnicodeSegmentation;

/// The color emoji font's bytes, read once.
fn font_bytes() -> Option<&'static [u8]> {
    static FONT: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let output = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "emoji:color=true"])
            .output()
            .ok()?;
        let path = String::from_utf8(output.stdout).ok()?;
        let bytes = std::fs::read(path.trim()).ok()?;
        // Only a font with bitmap strikes (CBDT or sbix) can be drawn here.
        let font = FontRef::new(&bytes).ok()?;
        (!BitmapStrikes::new(&font).is_empty()).then_some(bytes)
    })
    .as_deref()
}

/// The emoji's bitmap as premultiplied RGBA, from its first glyph the font
/// maps, and its height in ems (Noto's 128px rows at 109 ppem).
fn decode(emoji: &str) -> Option<(image::RgbaImage, f32)> {
    let font = FontRef::new(font_bytes()?).ok()?;
    let charmap = font.charmap();
    let glyph = emoji
        .chars()
        .filter(|c| !matches!(*c as u32, 0xFE0F | 0x200D))
        .find_map(|c| charmap.map(c))?;
    let bitmap = BitmapStrikes::new(&font).glyph_for_size(Size::new(128.0), glyph)?;
    let BitmapData::Png(png) = bitmap.data else {
        return None;
    };
    let mut image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    premultiply(&mut image);
    let ems = bitmap.height as f32 / bitmap.ppem_y.max(1.0);
    Some((image, ems))
}

/// Premultiplies alpha, so filtering does not bleed transparent black in.
pub(crate) fn premultiply(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let scale = |c: u8| ((c as u16 * a as u16 + 127) / 255) as u8;
        pixel.0 = [scale(r), scale(g), scale(b), a];
    }
}

/// The decoded bitmap and em height, once per emoji.
fn decoded(emoji: &str) -> Option<(Arc<image::RgbaImage>, f32)> {
    type Decoded = Option<(Arc<image::RgbaImage>, f32)>;
    static DECODED: OnceLock<Mutex<HashMap<String, Decoded>>> = OnceLock::new();
    let mut decoded = DECODED.get_or_init(Default::default).lock().ok()?;
    decoded
        .entry(emoji.to_owned())
        .or_insert_with(|| decode(emoji).map(|(image, ems)| (Arc::new(image), ems)))
        .clone()
}

/// The emoji's texture at `height` physical pixels (None without a color
/// font), resized once per size.
fn texture(ui: &Ui, emoji: &str, grayscale: bool, height: u32) -> Option<TextureHandle> {
    type Textures = HashMap<(String, bool, u32), Option<TextureHandle>>;
    static TEXTURES: OnceLock<Mutex<Textures>> = OnceLock::new();
    let (source, _) = decoded(emoji)?;
    let mut textures = TEXTURES.get_or_init(Default::default).lock().ok()?;
    textures
        .entry((emoji.to_owned(), grayscale, height))
        .or_insert_with(|| {
            let width = (source.width() as f32 * height as f32 / source.height().max(1) as f32)
                .round()
                .max(1.0) as u32;
            let resized = if height < source.height() {
                image::imageops::resize(
                    source.as_ref(),
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                )
            } else {
                source.as_ref().clone()
            };
            let mut image = egui::ColorImage::from_rgba_premultiplied(
                [resized.width() as usize, resized.height() as usize],
                resized.as_raw(),
            );
            if grayscale {
                // CSS `grayscale`: Rec. 709 luma.
                for pixel in &mut image.pixels {
                    let [r, g, b, a] = pixel.to_array();
                    let luma =
                        (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32).round() as u8;
                    *pixel = Color32::from_rgba_premultiplied(luma, luma, luma, a);
                }
            }
            Some(ui.ctx().load_texture(
                format!("emoji:{emoji}:{grayscale}:{height}"),
                image,
                egui::TextureOptions::LINEAR,
            ))
        })
        .clone()
}

/// Paints `emoji` at the font size `size`, centered on `center`.
pub fn paint(ui: &Ui, center: Pos2, size: f32, emoji: &str) {
    paint_faded(ui, center, size, emoji, 1.0);
}

/// `paint` at an opacity (the text color's alpha applies to emoji).
pub fn paint_faded(ui: &Ui, center: Pos2, size: f32, emoji: &str, alpha: f32) {
    paint_with(ui, center, size, emoji, alpha, false);
}

/// `opacity-40 grayscale`: a parked sidebar row's icon.
pub fn paint_dimmed(ui: &Ui, center: Pos2, size: f32, emoji: &str) {
    paint_with(ui, center, size, emoji, 0.4, true);
}

fn paint_with(ui: &Ui, center: Pos2, size: f32, emoji: &str, alpha: f32, grayscale: bool) {
    let tint = crate::theme::alpha(Color32::WHITE, alpha);
    let ppp = ui.ctx().pixels_per_point();
    let texture = decoded(emoji).and_then(|(_, ems)| {
        let pixels = (size * ems * ppp).round().max(1.0) as u32;
        texture(ui, emoji, grayscale, pixels)
    });
    if let Some(texture) = texture {
        let [w, h] = texture.size();
        // One texel per physical pixel, on the pixel grid.
        let size = Vec2::new(w as f32, h as f32);
        let origin = center.to_vec2() * ppp - size / 2.0;
        // A half-pixel offset rounds up, like the browser places the glyph.
        let min = Pos2::new(origin.x.round(), origin.y.floor()) / ppp;
        let rect = Rect::from_min_size(min, size / ppp);
        ui.painter().image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            tint,
        );
        return;
    }
    let galley =
        ui.painter()
            .layout_no_wrap(emoji.to_owned(), egui::FontId::proportional(size), tint);
    let pos = center - galley.size() / 2.0;
    ui.painter().galley(pos, galley, tint);
}

/// `\p{Extended_Pictographic}`, by its Unicode ranges.
fn pictographic(c: char) -> bool {
    matches!(c as u32,
        0x00A9 | 0x00AE | 0x203C | 0x2049 | 0x2122 | 0x2139 | 0x2194..=0x2199 | 0x21A9..=0x21AA
        | 0x231A..=0x231B | 0x2328 | 0x2388 | 0x23CF | 0x23E9..=0x23F3 | 0x23F8..=0x23FA
        | 0x24C2 | 0x25AA..=0x25AB | 0x25B6 | 0x25C0 | 0x25FB..=0x25FE | 0x2600..=0x27BF
        | 0x2934..=0x2935 | 0x2B05..=0x2B07 | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55 | 0x3030
        | 0x303D | 0x3297 | 0x3299 | 0x1F000..=0x1FAFF)
}

/// `firstEmoji`: the first grapheme when it is an emoji, a flag or a keycap.
pub fn first_emoji(value: &str) -> Option<String> {
    let segment = value.trim().graphemes(true).next()?;
    let chars: Vec<char> = segment.chars().collect();
    let flag = chars.len() == 2
        && chars
            .iter()
            .all(|c| (0x1F1E6..=0x1F1FF).contains(&(*c as u32)));
    let keycap = matches!(chars.first(), Some('#' | '*' | '0'..='9'))
        && chars.last() == Some(&'\u{20E3}')
        && chars.len() <= 3;
    (chars.iter().any(|c| pictographic(*c)) || flag || keycap).then(|| segment.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_emoji_follows_upstream() {
        assert_eq!(first_emoji(" 🚀 rocket").as_deref(), Some("🚀"));
        assert_eq!(first_emoji("🛠️").as_deref(), Some("🛠️"));
        assert_eq!(first_emoji("🇫🇷").as_deref(), Some("🇫🇷"));
        assert_eq!(first_emoji("1️⃣").as_deref(), Some("1️⃣"));
        assert_eq!(first_emoji("abc"), None);
        assert_eq!(first_emoji("  "), None);
    }
}
