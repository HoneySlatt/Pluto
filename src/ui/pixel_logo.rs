//! Pluto's pixel logo in the active theme's colors (`theme::logo_colors`):
//! the planet's SVG with the colors filled in, rasterized at the exact pixel
//! size it is shown at and cached per context, and the moon painted over it.
//! The logo never moves: an animated moon drew six frames every 1.25 s at
//! rest (egui shortens each delayed repaint by a predicted frame), against the
//! rule that nothing redraws while there is nothing to do.
use std::collections::HashMap;

use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, TextureOptions, Ui, Vec2};

use crate::theme::{self, LogoColors};

/// The artwork's box in SVG units (the viewBox, cropped to the planet).
const VIEW: Rect = Rect {
    min: Pos2::new(72.0, 72.0),
    max: Pos2::new(456.0, 392.0),
};
const MOON: Rect = Rect {
    min: Pos2::new(392.0, 144.0),
    max: Pos2::new(420.0, 172.0),
};
/// Width over height.
pub const ASPECT: f32 = 384.0 / 320.0;
/// `assets/pluto.svg` without the moon, its colors left to fill in.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="72 72 384 320" width="384" height="320">
<path d="M88 256v-24h24v-24h48v-24h48v-24h80v24h48v24h48v24h24v48h-24v24h-48v24h-48v24h-80v-24h-48v-24h-48v-24H88v-48Z" fill="none" stroke="{ring}" stroke-width="18"/>
<path d="M192 120h128v24h24v24h24v24h24v24h24v80h-24v24h-24v24h-24v24h-24v24H192v-24h-24v-24h-24v-24h-24v-24h-24v-80h24v-24h24v-24h24v-24h24v-24Z" fill="{planet}"/>
<path d="M208 80h40v24h-40ZM192 104h80v24h-80Z" fill="{planet}"/>
<path d="M120 288h24v24h24v24h24v24h128v-24h24v-24h24v-24h24v-24H120Z" fill="{band}"/>
<path d="M184 192h48v24h-48Zm128 88h48v24h-48Z" fill="{band}"/>
<path d="M168 160h48v16h-48Zm-24 24h24v24h-24Zm72-88h16v16h-16Z" fill="{highlight}"/>
<path d="M88 256v48h24v24h48v24h80v-24h48v-24h48v-24h24v-48" fill="none" stroke="{front}" stroke-width="20"/>
</svg>"##;

type Cache = HashMap<(LogoColors, [u32; 2]), TextureHandle>;

fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// The planet in `colors` at `size` physical pixels, rasterized once.
fn texture(ctx: &egui::Context, colors: LogoColors, size: [u32; 2]) -> Option<TextureHandle> {
    let id = egui::Id::new("pluto-logo-textures");
    let key = (colors, size);
    if let Some(texture) =
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(id).get(&key).cloned())
    {
        return Some(texture);
    }
    let svg = SVG
        .replace("{ring}", &hex(colors.ring))
        .replace("{planet}", &hex(colors.planet))
        .replace("{band}", &hex(colors.band))
        .replace("{highlight}", &hex(colors.highlight))
        .replace("{front}", &hex(colors.front));
    let image = egui_extras::image::load_svg_bytes_with_size(
        svg.as_bytes(),
        egui::SizeHint::Size {
            width: size[0],
            height: size[1],
            maintain_aspect_ratio: true,
        },
        &Default::default(),
    )
    .ok()?;
    let texture = ctx.load_texture(
        format!("pluto-logo-{}x{}", size[0], size[1]),
        image,
        TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Cache>(id);
        // A theme change leaves the old colors behind.
        if cache.len() >= 12 {
            cache.clear();
        }
        cache.insert(key, texture.clone());
    });
    Some(texture)
}

/// Paints the logo centered in `rect` at the largest size that fits, its box
/// snapped to physical pixels so the cells stay sharp.
pub fn paint(ui: &Ui, rect: Rect) {
    let size = if rect.width() / rect.height() > ASPECT {
        Vec2::new(rect.height() * ASPECT, rect.height())
    } else {
        Vec2::new(rect.width(), rect.width() / ASPECT)
    };
    let ppp = ui.ctx().pixels_per_point();
    let pixels = [(size.x * ppp).round() as u32, (size.y * ppp).round() as u32];
    if pixels[0] == 0 || pixels[1] == 0 {
        return;
    }
    let size = Vec2::new(pixels[0] as f32, pixels[1] as f32) / ppp;
    let min = rect.center() - size / 2.0;
    let logo = Rect::from_min_size(
        Pos2::new((min.x * ppp).round() / ppp, (min.y * ppp).round() / ppp),
        size,
    );
    let colors = theme::logo_colors();
    if let Some(texture) = texture(ui.ctx(), colors, pixels) {
        ui.painter().image(
            texture.id(),
            logo,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let scale = logo.width() / VIEW.width();
    let to_screen = |p: Pos2| logo.min + (p - VIEW.min) * scale;
    let moon_rect = Rect::from_min_max(to_screen(MOON.min), to_screen(MOON.max));
    ui.painter().rect_filled(moon_rect, 0.0, colors.moon);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The draft hero shows the logo while the client rests: once egui's first
    /// passes have settled, drawing it must ask for no further frame.
    #[test]
    fn the_logo_asks_for_no_repaint() {
        let ctx = egui::Context::default();
        let mut delay = std::time::Duration::ZERO;
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                paint(ui, Rect::from_min_size(Pos2::ZERO, Vec2::new(72.0, 60.0)));
            });
            output.textures_delta.clear();
            delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        }
        assert_eq!(delay, std::time::Duration::MAX);
    }

    #[test]
    fn the_template_fills_every_color() {
        let colors = theme::LogoColors::for_palette(&theme::pluto_stock());
        let svg = SVG
            .replace("{ring}", &hex(colors.ring))
            .replace("{planet}", &hex(colors.planet))
            .replace("{band}", &hex(colors.band))
            .replace("{highlight}", &hex(colors.highlight))
            .replace("{front}", &hex(colors.front));
        assert!(!svg.contains('{'), "a color was left out");
        let image = egui_extras::image::load_svg_bytes_with_size(
            svg.as_bytes(),
            egui::SizeHint::Size {
                width: 72,
                height: 60,
                maintain_aspect_ratio: true,
            },
            &Default::default(),
        )
        .expect("the logo rasterizes");
        assert_eq!(image.size, [72, 60]);
    }
}
