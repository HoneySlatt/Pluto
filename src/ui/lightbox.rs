//! The expanded image preview (`chat/ExpandedImageDialog.tsx`,
//! `chat/ZoomableImage.tsx`), opened from a message's or the composer's
//! image thumbnails: the image fitted to the window over a `bg-black/75`
//! backdrop with its name below; a click zooms to 2x around the pointer or
//! back to fit, the wheel zooms up to 8x, dragging pans; the arrows (or the
//! side buttons) walk the images, Escape or a click beside the image closes.
//!
//! Videos and the snap-shot accessibility views are not ported.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const MAX_ZOOM: f32 = 8.0;

/// One image of the preview.
pub(crate) struct Item {
    pub(crate) name: String,
    /// A stable texture key for the image.
    pub(crate) key: String,
    pub(crate) image: Arc<image::RgbaImage>,
}

/// The open preview (`ExpandedImagePreview`) and its zoom state.
pub(crate) struct Lightbox {
    items: Vec<Item>,
    index: usize,
    zoom: f32,
    /// The viewport's scroll offset at the current zoom.
    scroll: Vec2,
    dragging: bool,
}

impl Lightbox {
    pub(crate) fn new(items: Vec<Item>, index: usize) -> Option<Self> {
        (!items.is_empty()).then(|| Self {
            index: index.min(items.len() - 1),
            items,
            zoom: 1.0,
            scroll: Vec2::ZERO,
            dragging: false,
        })
    }

    fn navigate(&mut self, step: isize) {
        let count = self.items.len() as isize;
        self.index = ((self.index as isize + step).rem_euclid(count)) as usize;
        self.zoom = 1.0;
        self.scroll = Vec2::ZERO;
    }
}

/// The fitted size of a `natural` image in a `screen`-sized window
/// (`ZoomableImage`: 92% of the width less 96px of side buttons, and
/// min(86% of the height, the height less 160px)), and that height limit.
fn fit(natural: Vec2, screen: Vec2) -> (Vec2, f32) {
    let max_height = (screen.y * 0.86).min(screen.y - 160.0).max(1.0);
    let media_width = screen.x * 0.92 - if screen.x >= 640.0 { 96.0 } else { 0.0 };
    let scale = 1.0_f32
        .min(media_width / natural.x.max(1.0))
        .min(max_height / natural.y.max(1.0));
    (natural * scale, max_height)
}

/// A texture of `image` at the power-of-two reduction closest above
/// `display` (downscaled on the CPU, so minification never aliases).
fn level_texture(ui: &Ui, item: &Item, display: Vec2) -> egui::TextureHandle {
    let ppp = ui.ctx().pixels_per_point();
    let mut level = 0u32;
    while level < 8
        && (item.image.width() >> (level + 1)) as f32 >= display.x * ppp
        && (item.image.height() >> (level + 1)) as f32 >= display.y * ppp
    {
        level += 1;
    }
    let width = (item.image.width() >> level).max(1);
    let height = (item.image.height() >> level).max(1);
    crate::ui::attachments::sized_texture(
        ui,
        &format!("lightbox:{}", item.key),
        width,
        height,
        || {
            if level == 0 {
                item.image.as_ref().clone()
            } else {
                image::imageops::resize(
                    item.image.as_ref(),
                    width,
                    height,
                    image::imageops::FilterType::Triangle,
                )
            }
        },
    )
    .expect("texture cache")
}

impl App {
    /// Opens the preview on the `index`th image.
    pub(crate) fn open_lightbox(&mut self, items: Vec<Item>, index: usize) {
        self.lightbox = Lightbox::new(items, index);
    }

    /// The preview over everything (`DialogPopup variant="media"`).
    pub(crate) fn lightbox_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut lightbox) = self.lightbox.take() else {
            return;
        };
        let mut close = false;
        let count = lightbox.items.len();
        // The dialog holds the focus: the prompt editor stops taking keys.
        ui.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        // Keys: Escape, the arrows, and the zoom keys.
        let (escape, left, right, enter, plus, minus, zero) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowRight),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Plus)
                    || i.consume_key(egui::Modifiers::NONE, Key::Equals),
                i.consume_key(egui::Modifiers::NONE, Key::Minus),
                i.consume_key(egui::Modifiers::NONE, Key::Num0),
            )
        });
        if escape {
            close = true;
        }
        let item = &lightbox.items[lightbox.index];
        let natural = Vec2::new(item.image.width() as f32, item.image.height() as f32);
        let (fitted, max_height) = fit(natural, screen.size());
        let media_width = screen.width() * 0.92 - if screen.width() >= 640.0 { 96.0 } else { 0.0 };
        let caption = if count > 1 {
            format!("{} ({}/{count})", item.name, lightbox.index + 1)
        } else {
            item.name.clone()
        };

        egui::Area::new(ui.id().with("lightbox"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, backdrop_response) =
                    ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(backdrop, 0.0, Color32::from_black_alpha(191));

                // The viewport: the fitted image, scrolled once zoomed in.
                let display = fitted * lightbox.zoom;
                let viewport_size =
                    Vec2::new(display.x.min(media_width), display.y.min(max_height));
                let block_height = viewport_size.y + 8.0 + 16.0;
                let viewport = Rect::from_min_size(
                    Pos2::new(
                        (screen.center().x - viewport_size.x / 2.0).round(),
                        (screen.center().y - block_height / 2.0).round(),
                    ),
                    viewport_size,
                );
                let max_scroll = (display - viewport_size).max(Vec2::ZERO);
                lightbox.scroll = lightbox.scroll.clamp(Vec2::ZERO, max_scroll);

                let response = ui.interact(
                    viewport,
                    ui.id().with("lightbox-image"),
                    Sense::click_and_drag(),
                );
                // Zoom around a point of the viewport, keeping it in place.
                let zoom_to = |lightbox: &mut Lightbox, next: f32, at: Pos2| {
                    let next = next.clamp(1.0, MAX_ZOOM);
                    if next == lightbox.zoom {
                        return;
                    }
                    let local = at - viewport.min;
                    let anchor = (lightbox.scroll + local) / lightbox.zoom;
                    lightbox.zoom = next;
                    lightbox.scroll = anchor * next - local;
                };
                if response.hovered() {
                    // The wheel in pixels (upward positive); a notch is ~50px.
                    let wheel: f32 = ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|event| match event {
                                egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                                    egui::MouseWheelUnit::Point => delta.y,
                                    egui::MouseWheelUnit::Line => delta.y * 50.0,
                                    egui::MouseWheelUnit::Page => delta.y * viewport.height(),
                                }),
                                _ => None,
                            })
                            .sum()
                    });
                    if wheel != 0.0 {
                        let ctrl = ui.input(|i| i.modifiers.ctrl);
                        let factor = (wheel * if ctrl { 0.01 } else { 0.002 }).exp();
                        let at = response.hover_pos().unwrap_or(viewport.center());
                        let next = lightbox.zoom * factor;
                        zoom_to(&mut lightbox, next, at);
                    }
                }
                if response.drag_started() && lightbox.zoom > 1.0 {
                    lightbox.dragging = true;
                }
                if lightbox.dragging {
                    lightbox.scroll -= response.drag_delta();
                    if response.drag_stopped() {
                        lightbox.dragging = false;
                    }
                }
                if response.clicked()
                    && let Some(at) = response.interact_pointer_pos()
                {
                    let next = if lightbox.zoom > 1.0 { 1.0 } else { 2.0 };
                    zoom_to(&mut lightbox, next, at);
                }
                if enter {
                    let next = if lightbox.zoom > 1.0 { 1.0 } else { 2.0 };
                    zoom_to(&mut lightbox, next, viewport.center());
                }
                if plus {
                    let next = lightbox.zoom * 1.5;
                    zoom_to(&mut lightbox, next, viewport.center());
                }
                if minus {
                    let next = lightbox.zoom / 1.5;
                    zoom_to(&mut lightbox, next, viewport.center());
                }
                if zero {
                    zoom_to(&mut lightbox, 1.0, viewport.center());
                }
                ui.ctx().set_cursor_icon(if !response.hovered() {
                    egui::CursorIcon::Default
                } else if lightbox.zoom > 1.0 {
                    if lightbox.dragging {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    }
                } else {
                    egui::CursorIcon::ZoomIn
                });
                let display = fitted * lightbox.zoom;
                let max_scroll = (display - viewport_size).max(Vec2::ZERO);
                lightbox.scroll = lightbox.scroll.clamp(Vec2::ZERO, max_scroll);

                // `rounded-lg bg-background shadow-2xl ring-1 ring-border/70`.
                let radius = CornerRadius::same(8);
                ui.painter()
                    .rect_filled(viewport, radius, theme::background());
                let item = &lightbox.items[lightbox.index];
                let texture = level_texture(ui, item, display);
                if lightbox.zoom <= 1.0 {
                    egui::Image::new((texture.id(), display))
                        .corner_radius(radius)
                        .paint_at(ui, viewport);
                } else {
                    // Zoomed: the scrolled image, clipped to the viewport.
                    let image_rect = Rect::from_min_size(viewport.min - lightbox.scroll, display);
                    ui.painter_at(viewport).image(
                        texture.id(),
                        image_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                ui.painter().rect_stroke(
                    viewport,
                    radius,
                    Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
                    StrokeKind::Outside,
                );

                // The close button above the image's top right corner.
                let close_rect = Rect::from_min_size(
                    Pos2::new(viewport.right() - 24.0, viewport.top() - 40.0),
                    Vec2::splat(24.0),
                );
                let close_response = ui
                    .interact(close_rect, ui.id().with("lightbox-close"), Sense::click())
                    .on_hover_text("Close image preview");
                let fill = if close_response.hovered() { 0.8 } else { 0.65 };
                ui.painter().rect(
                    close_rect,
                    CornerRadius::same(6),
                    Color32::from_black_alpha((fill * 255.0) as u8),
                    Stroke::new(1.0, theme::alpha(Color32::WHITE, 0.2)),
                    StrokeKind::Outside,
                );
                theme::paint_icon(ui, "x", close_rect, 14.0, Color32::WHITE);
                if close_response.clicked() {
                    close = true;
                }

                // The name below (`mt-2 text-xs text-white/80`).
                let caption_font = font(12.0, Weight::Regular);
                let caption_width =
                    widgets::text_width(ui, &caption, caption_font.clone()).min(media_width);
                widgets::text_in(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(
                            viewport.center().x - caption_width / 2.0,
                            viewport.bottom() + 8.0,
                        ),
                        Vec2::new(caption_width + 1.0, 16.0),
                    ),
                    &caption,
                    caption_font,
                    theme::alpha(Color32::WHITE, 0.8),
                );

                // Previous / next at the popup's sides (`media-navigation`).
                let mut step = 0;
                if count > 1 {
                    let popup_width = screen.width() * 0.92;
                    for (direction, icon, x) in [
                        (-1, "chevron-left", screen.center().x - popup_width / 2.0),
                        (
                            1,
                            "chevron-right",
                            screen.center().x + popup_width / 2.0 - 32.0,
                        ),
                    ] {
                        let rect = Rect::from_min_size(
                            Pos2::new(x, viewport.center().y - 16.0),
                            Vec2::splat(32.0),
                        );
                        let response = ui
                            .interact(
                                rect,
                                ui.id().with(("lightbox-nav", direction)),
                                Sense::click(),
                            )
                            .on_hover_text(if direction < 0 {
                                "Previous media"
                            } else {
                                "Next media"
                            });
                        if response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(8),
                                theme::alpha(Color32::WHITE, 0.1),
                            );
                        }
                        let tint = theme::alpha(
                            Color32::WHITE,
                            if response.hovered() { 1.0 } else { 0.9 },
                        );
                        theme::paint_icon(ui, icon, rect, 20.0, tint);
                        if response.clicked() {
                            step = direction;
                        }
                    }
                }
                if step != 0 {
                    lightbox.navigate(step);
                } else if count > 1 && (left || right) && max_scroll.x <= 0.0 {
                    lightbox.navigate(if left { -1 } else { 1 });
                } else if lightbox.zoom > 1.0 && (left || right) {
                    lightbox.scroll.x += if left { -40.0 } else { 40.0 };
                }
                // A click beside the image closes the preview.
                if backdrop_response.clicked() && !response.hovered() && !close_response.hovered() {
                    close = true;
                }
            });
        if !close {
            self.lightbox = Some(lightbox);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_fit_like_upstream() {
        let screen = Vec2::new(1440.0, 900.0);
        // Small images keep their size.
        assert_eq!(
            fit(Vec2::new(320.0, 240.0), screen).0,
            Vec2::new(320.0, 240.0)
        );
        // Tall ones fit min(86%, height - 160) = 740.
        let (size, max_height) = fit(Vec2::new(1000.0, 2000.0), screen);
        assert_eq!(max_height, 740.0);
        assert_eq!(size, Vec2::new(370.0, 740.0));
        // Wide ones fit 92% of the width less 96px.
        let (size, _) = fit(Vec2::new(4000.0, 1000.0), screen);
        assert!((size.x - (1440.0 * 0.92 - 96.0)).abs() < 0.01);
    }
}
