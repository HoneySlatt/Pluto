//! The provider accent color picker, ported from
//! `components/settings/ProviderAccentColorPicker.tsx`, `components/ui/color-picker.tsx`
//! (`ColorSaturationValuePlane`, `ColorHueSlider`) and `lib/color.ts`.
//!
//! The swatch opens a popover with the saturation/brightness plane, the hue
//! slider, the hex field and "Clear color". Changes commit 120ms after the
//! last edit (`commitDelayMs`), and a pending commit is flushed on close.

use eframe::egui::{
    self, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `FALLBACK_ACCENT_COLOR`.
pub(crate) const FALLBACK: &str = "#2563eb";
const COMMIT_DELAY: f64 = 0.12;
const WIDTH: f32 = 226.0;
const PLANE_HEIGHT: f32 = 144.0;
const HEIGHT: f32 = 1.0 + PLANE_HEIGHT + 90.0 + 38.0 + 1.0;

pub(crate) struct AccentPicker {
    /// The provider instance being edited.
    pub(crate) row: String,
    hsv: [f32; 3],
    /// The hex field while it holds an incomplete value.
    hex_draft: Option<String>,
    /// The optimistic value (`optimisticValue`); empty when cleared.
    pub(crate) value: String,
    /// The value waiting for its delayed commit, and when it was set.
    pending: Option<(String, f64)>,
}

/// What the popover asks the caller to save.
pub(crate) enum PickerEvent {
    None,
    Commit(String),
    /// Commit what is pending, then close.
    Close(Option<String>),
}

impl AccentPicker {
    pub(crate) fn new(row: String, value: Option<&str>) -> Self {
        let value = value.and_then(normalize).unwrap_or_default();
        let shown = if value.is_empty() { FALLBACK } else { &value };
        Self {
            row,
            hsv: hex_to_hsv(shown),
            hex_draft: None,
            value: value.clone(),
            pending: None,
        }
    }

    /// `commitAccentColor`: normalize, show at once, commit after the delay.
    fn change(&mut self, value: &str, now: f64) {
        let normalized = normalize(value).unwrap_or_default();
        self.value = normalized.clone();
        self.pending = Some((normalized, now));
    }

    fn set_hsv(&mut self, hsv: [f32; 3], now: f64) {
        self.hsv = hsv;
        let hex = hsv_to_hex(hsv);
        self.change(&hex, now);
    }

    /// The popover below `anchor`; returns what to save this frame.
    pub(crate) fn show(&mut self, ui: &mut Ui, anchor: Rect) -> PickerEvent {
        let now = ui.input(|i| i.time);
        let rect = Rect::from_min_size(
            Pos2::new(anchor.left(), anchor.bottom() + 6.0),
            Vec2::new(WIDTH, HEIGHT),
        );
        let mut clear = false;
        let area = egui::Area::new(ui.id().with(("accent-picker", &self.row)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                let (outer, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                widgets::popup_surface(ui, outer);
                let inner = outer.shrink(1.0);
                let plane = Rect::from_min_size(inner.min, Vec2::new(inner.width(), PLANE_HEIGHT));
                self.plane(ui, plane, now);
                let slider = Rect::from_min_size(
                    Pos2::new(inner.left() + 12.0, plane.bottom() + 12.0),
                    Vec2::new(inner.width() - 24.0, 24.0),
                );
                self.hue_slider(ui, slider, now);
                let field = Rect::from_min_size(
                    Pos2::new(slider.left(), slider.bottom() + 12.0),
                    Vec2::new(slider.width(), 30.0),
                );
                self.hex_field(ui, field, now);
                let footer = plane.bottom() + 90.0;
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(inner.left(), footer),
                        Vec2::new(inner.width(), 1.0),
                    ),
                    0.0,
                    theme::alpha(theme::border(), 0.6),
                );
                let button = Rect::from_min_size(
                    Pos2::new(inner.left() + 4.0, footer + 5.0),
                    Vec2::new(inner.width() - 8.0, 28.0),
                );
                clear = clear_button(ui, button, !self.value.is_empty());
            });
        if clear {
            // "Clear color" commits at once through the delay, and the
            // `PopoverClose` closes the popover, which flushes it.
            self.change("", now);
            return PickerEvent::Close(self.pending.take().map(|(v, _)| v));
        }
        let pressed_outside = ui.input(|i| i.pointer.any_pressed())
            && !area.response.contains_pointer()
            && !ui.rect_contains_pointer(anchor);
        let anchor_clicked = ui.input(|i| i.pointer.any_pressed())
            && ui.rect_contains_pointer(anchor)
            && !area.response.contains_pointer();
        if pressed_outside || anchor_clicked || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            return PickerEvent::Close(self.pending.take().map(|(v, _)| v));
        }
        if let Some((value, at)) = &self.pending {
            if now - at >= COMMIT_DELAY {
                let value = value.clone();
                self.pending = None;
                return PickerEvent::Commit(value);
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(COMMIT_DELAY));
        }
        PickerEvent::None
    }

    /// `ColorSaturationValuePlane variant="edge"`: the hue under a white
    /// left-to-right and a black bottom-to-top gradient, with the popover's
    /// rounded top corners.
    fn plane(&mut self, ui: &Ui, rect: Rect, now: f64) {
        let response = ui.interact(rect, ui.id().with("accent-plane"), Sense::click_and_drag());
        if let Some(pos) = response
            .interact_pointer_pos()
            .filter(|_| response.is_pointer_button_down_on())
        {
            let s = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let v = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            if [s, v] != [self.hsv[1], self.hsv[2]] {
                self.set_hsv([self.hsv[0], s, v], now);
            }
        }
        let hue = hsv_color([self.hsv[0], 1.0, 1.0]);
        let color_at = |x: f32, y: f32| {
            let s = (x - rect.left()) / rect.width();
            let v = 1.0 - (y - rect.top()) / rect.height();
            let channel = |c: u8| (v * (255.0 * (1.0 - s) + c as f32 * s)).round() as u8;
            Color32::from_rgb(channel(hue.r()), channel(hue.g()), channel(hue.b()))
        };
        // Columns follow the rounded top corners (radius 9); rows are fine
        // enough that the per-triangle interpolation matches the gradient.
        let radius = 9.0;
        let top_inset = |x: f32| {
            let d = if x < rect.left() + radius {
                rect.left() + radius - x
            } else if x > rect.right() - radius {
                x - (rect.right() - radius)
            } else {
                return 0.0;
            };
            radius - (radius * radius - d * d).max(0.0).sqrt()
        };
        let mut xs: Vec<f32> = Vec::new();
        let mut x = rect.left();
        while x < rect.right() {
            xs.push(x);
            x += if x < rect.left() + radius || x > rect.right() - radius - 1.0 {
                1.0
            } else {
                8.0
            };
        }
        xs.push(rect.right());
        const ROWS: usize = 24;
        let mut mesh = Mesh::default();
        for &x in &xs {
            let top = rect.top() + top_inset(x);
            for j in 0..=ROWS {
                let y = top + (rect.bottom() - top) * j as f32 / ROWS as f32;
                mesh.colored_vertex(Pos2::new(x, y), color_at(x, y));
            }
        }
        let stride = (ROWS + 1) as u32;
        for i in 0..xs.len() as u32 - 1 {
            for j in 0..ROWS as u32 {
                let a = i * stride + j;
                let b = a + stride;
                mesh.add_triangle(a, b, a + 1);
                mesh.add_triangle(b, b + 1, a + 1);
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        // The thumb stays inside the plane at its extremes.
        let center = Pos2::new(
            rect.left() + 6.0 + self.hsv[1] * (rect.width() - 12.0),
            rect.top() + 6.0 + (1.0 - self.hsv[2]) * (rect.height() - 12.0),
        );
        thumb(ui, center, 6.0, None);
    }

    /// `ColorHueSlider`: a 10px rainbow track and a 16px thumb.
    fn hue_slider(&mut self, ui: &Ui, rect: Rect, now: f64) {
        let response = ui.interact(rect, ui.id().with("accent-hue"), Sense::click_and_drag());
        if let Some(pos) = response
            .interact_pointer_pos()
            .filter(|_| response.is_pointer_button_down_on())
        {
            let h = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * 360.0;
            if h != self.hsv[0] {
                self.set_hsv([h, self.hsv[1], self.hsv[2]], now);
            }
        }
        let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 10.0));
        let radius = track.height() / 2.0;
        let mut mesh = Mesh::default();
        let steps = track.width().round() as usize;
        for i in 0..=steps {
            let x = track.left() + track.width() * i as f32 / steps as f32;
            let d = if x < track.left() + radius {
                track.left() + radius - x
            } else if x > track.right() - radius {
                x - (track.right() - radius)
            } else {
                0.0
            };
            let half = (radius * radius - d * d).max(0.0).sqrt();
            let color = hsv_color([360.0 * i as f32 / steps as f32, 1.0, 1.0]);
            mesh.colored_vertex(Pos2::new(x, track.center().y - half), color);
            mesh.colored_vertex(Pos2::new(x, track.center().y + half), color);
            if i > 0 {
                let a = (i as u32 - 1) * 2;
                mesh.add_triangle(a, a + 2, a + 1);
                mesh.add_triangle(a + 2, a + 3, a + 1);
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        // `shadow-[inset_0_0_0_1px_rgb(0_0_0/12%)]`.
        ui.painter().rect_stroke(
            track,
            CornerRadius::same(5),
            Stroke::new(1.0, Color32::from_black_alpha(31)),
            StrokeKind::Inside,
        );
        let center = Pos2::new(
            rect.left() + 8.0 + self.hsv[0] / 360.0 * (rect.width() - 16.0),
            rect.center().y,
        );
        thumb(ui, center, 8.0, Some(hsv_color([self.hsv[0], 1.0, 1.0])));
    }

    /// The compact mono hex input: commits once it holds `#rrggbb`.
    fn hex_field(&mut self, ui: &mut Ui, rect: Rect, now: f64) {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            theme::control_surface(),
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        let current = hsv_to_hex(self.hsv);
        let mut text = self.hex_draft.clone().unwrap_or_else(|| current.clone());
        let field = Rect::from_min_max(
            Pos2::new(rect.left() + 10.0, rect.top() + 1.0),
            Pos2::new(rect.right() - 10.0, rect.bottom() - 1.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
        let response = child.add(
            egui::TextEdit::singleline(&mut text)
                .id(egui::Id::new("accent-hex"))
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(theme::mono(12.0))
                .text_color(theme::foreground())
                .vertical_align(egui::Align::Center)
                .desired_width(field.width())
                .min_size(Vec2::new(0.0, field.height())),
        );
        if response.changed() {
            let valid = text.len() == 7
                && text.starts_with('#')
                && text[1..].chars().all(|c| c.is_ascii_hexdigit());
            if valid {
                self.hsv = hex_to_hsv(&text);
                self.change(&text, now);
            }
            self.hex_draft = Some(text);
        }
        if response.lost_focus() {
            self.hex_draft = None;
        }
    }
}

/// `normalizeProviderAccentColor`: only `#rrggbb` renders.
pub(crate) fn normalize(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let valid = trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit());
    valid.then(|| trimmed.to_owned())
}

/// Parses a normalized `#rrggbb` color.
pub(crate) fn parse(value: &str) -> Option<Color32> {
    let hex = normalize(value)?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color32::from_rgb(byte(1)?, byte(3)?, byte(5)?))
}

/// `hexToHsv`: hue in degrees, saturation and value in 0..1.
pub(crate) fn hex_to_hsv(hex: &str) -> [f32; 3] {
    let color = parse(hex).unwrap_or(Color32::BLACK);
    let [r, g, b] = [color.r(), color.g(), color.b()].map(|c| c as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut hue = 0.0;
    if delta != 0.0 {
        hue = if max == r {
            ((g - b) / delta) % 6.0
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        } * 60.0;
        if hue < 0.0 {
            hue += 360.0;
        }
    }
    [hue, if max == 0.0 { 0.0 } else { delta / max }, max]
}

fn hsv_color([hue, saturation, value]: [f32; 3]) -> Color32 {
    let hue = hue.rem_euclid(360.0);
    let chroma = value * saturation;
    let x = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = value - chroma;
    let (r, g, b) = match hue {
        h if h < 60.0 => (chroma, x, 0.0),
        h if h < 120.0 => (x, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, x),
        h if h < 240.0 => (0.0, x, chroma),
        h if h < 300.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let channel = |c: f32| ((c + m) * 255.0).round() as u8;
    Color32::from_rgb(channel(r), channel(g), channel(b))
}

/// `hsvToHex`.
pub(crate) fn hsv_to_hex(hsv: [f32; 3]) -> String {
    let color = hsv_color(hsv);
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// A round thumb: 2px white border and a 1px `rgb(0 0 0/0.4)` ring.
fn thumb(ui: &Ui, center: Pos2, radius: f32, fill: Option<Color32>) {
    let painter = ui.painter();
    painter.circle_stroke(
        center,
        radius + 0.5,
        Stroke::new(1.0, Color32::from_black_alpha(102)),
    );
    if let Some(fill) = fill {
        painter.circle_filled(center, radius, fill);
    }
    painter.circle_stroke(center, radius - 1.0, Stroke::new(2.0, Color32::WHITE));
}

/// "Clear color": a compact ghost-muted button, disabled without a value.
fn clear_button(ui: &Ui, rect: Rect, enabled: bool) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, ui.id().with("accent-clear"), sense);
    let opacity = if enabled { 1.0 } else { 0.64 };
    let color = if enabled && response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::accent());
        theme::foreground()
    } else {
        theme::muted_foreground()
    };
    let color = theme::alpha(color, opacity);
    theme::paint_icon(
        ui,
        "x",
        Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.center().y - 8.0),
            Vec2::splat(16.0),
        ),
        16.0,
        color,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 24.0, rect.top()), rect.max),
        "Clear color",
        font(14.0, Weight::Regular),
        color,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_hsv_round_trip_like_upstream() {
        let hsv = hex_to_hsv(FALLBACK);
        assert!((hsv[0] - 221.2).abs() < 0.1);
        assert_eq!(hsv_to_hex(hsv), FALLBACK);
        assert_eq!(hsv_to_hex([0.0, 1.0, 1.0]), "#ff0000");
        assert_eq!(hsv_to_hex([120.0, 0.5, 0.5]), "#408040");
        assert_eq!(normalize(" #A1b2C3 ").as_deref(), Some("#A1b2C3"));
        assert_eq!(normalize("#abc"), None);
        assert_eq!(normalize(""), None);
    }
}
