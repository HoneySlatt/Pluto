//! Pluto's backdrop and action fills: the violet glow of Pluto stock behind
//! the main area, the pixel stars around a draft's hero, and the primary
//! action buttons (send, Implement, Refine), in a vertical gradient on Pluto
//! stock. The gradients are meshes with per-vertex colors.
use std::f32::consts::{FRAC_PI_2, PI};

use eframe::egui::{self, Color32, CornerRadius, Mesh, Pos2, Rect, Ui, Vec2};

use crate::theme;

/// The glow's color (the logo's deep violet).
const GLOW: Color32 = Color32::from_rgb(0x6d, 0x28, 0xd9);
/// The action gradient, top to bottom, at rest and hovered.
const ACTION: [Color32; 2] = [
    Color32::from_rgb(0xc4, 0xb5, 0xfd),
    Color32::from_rgb(0x8b, 0x5c, 0xf6),
];
const ACTION_HOVER: [Color32; 2] = [
    Color32::from_rgb(0xdd, 0xd6, 0xfe),
    Color32::from_rgb(0xa7, 0x8b, 0xfa),
];

/// Pluto stock's glow: a half ellipse falling from the top center of `area`,
/// 30% violet at its source, 10% at 45% of the way, clear at its edge;
/// `strength` dims it (in a thread).
pub fn paint_glow(painter: &egui::Painter, area: Rect, strength: f32) {
    if !theme::palette().glow {
        return;
    }
    let center = Pos2::new(area.center().x, area.top());
    let radii = Vec2::new(area.width() * 0.55, area.height() * 0.7);
    let color = |alpha: f32| theme::alpha(GLOW, alpha * strength);
    const STEPS: u32 = 64;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color(0.3));
    for (scale, alpha) in [(0.45, 0.1), (1.0, 0.0)] {
        for i in 0..=STEPS {
            let angle = PI * i as f32 / STEPS as f32;
            let offset = Vec2::new(radii.x * angle.cos(), radii.y * angle.sin()) * scale;
            mesh.colored_vertex(center + offset, color(alpha));
        }
    }
    let ring = STEPS + 1;
    for i in 0..STEPS {
        let (a, b) = (1 + i, 2 + i);
        mesh.add_triangle(0, a, b);
        mesh.add_triangle(a, a + ring, b + ring);
        mesh.add_triangle(a, b + ring, b);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The draft's pixel stars: 64 two- or three-pixel squares from a fixed seed
/// across `body`, none in the ellipse around the hero at `hero`.
pub fn paint_sky(painter: &egui::Painter, body: Rect, hero: Pos2) {
    let palette = theme::palette();
    let color = if palette.brand_logo {
        Color32::from_rgb(0xed, 0xe9, 0xfe)
    } else {
        palette.foreground
    };
    let dim = if palette.dark { 1.0 } else { 0.6 };
    let mut seed = 11u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let mut drawn = 0;
    while drawn < 64 {
        let p = Pos2::new(
            body.left() + next() * body.width(),
            body.top() + next() * body.height(),
        );
        let opacity = (0.12 + next() * 0.3) * dim;
        let size = if next() < 0.12 { 3.0 } else { 2.0 };
        let d = Vec2::new(
            (p.x - hero.x) / (body.width() * 0.3),
            (p.y - hero.y) / (body.height() * 0.3),
        );
        if d.length_sq() < 1.0 {
            continue;
        }
        drawn += 1;
        painter.rect_filled(
            Rect::from_min_size(p.round(), Vec2::splat(size)),
            0.0,
            theme::alpha(color, opacity),
        );
    }
}

/// Fills a primary action button (`message-action`): the primary color (its
/// hover color when `hovered`), or Pluto stock's gradient; returns the color
/// for its label or icon.
pub fn fill_action(
    ui: &Ui,
    rect: Rect,
    corners: CornerRadius,
    hovered: bool,
    opacity: f32,
) -> Color32 {
    let label = theme::alpha(theme::primary_foreground(), opacity);
    if !theme::palette().glow {
        let fill = if hovered {
            theme::primary_hover()
        } else {
            theme::primary()
        };
        ui.painter()
            .rect_filled(rect, corners, theme::alpha(fill, opacity));
        return label;
    }
    let [top, bottom] = if hovered { ACTION_HOVER } else { ACTION };
    let color = |y: f32| {
        let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        theme::alpha(theme::mix(top, bottom, t), opacity)
    };
    // The outline: each corner's arc (a point for a square corner), with
    // its outward normal, feathered half a pixel each way.
    let feather = 0.5 / ui.ctx().pixels_per_point();
    let max = rect.width().min(rect.height()) / 2.0;
    let corner = |r: u8, center: Pos2, start: f32| -> Vec<(Pos2, Vec2)> {
        let r = f32::from(r).min(max);
        const STEPS: usize = 8;
        (0..=STEPS)
            .map(|i| {
                let angle = start + FRAC_PI_2 * i as f32 / STEPS as f32;
                let normal = Vec2::angled(angle);
                (center + normal * r, normal)
            })
            .collect()
    };
    let r = |v: u8| f32::from(v).min(max);
    let mut outline = Vec::new();
    outline.extend(corner(
        corners.nw,
        rect.left_top() + Vec2::splat(r(corners.nw)),
        PI,
    ));
    outline.extend(corner(
        corners.ne,
        Pos2::new(rect.right() - r(corners.ne), rect.top() + r(corners.ne)),
        1.5 * PI,
    ));
    outline.extend(corner(
        corners.se,
        rect.right_bottom() - Vec2::splat(r(corners.se)),
        0.0,
    ));
    outline.extend(corner(
        corners.sw,
        Pos2::new(rect.left() + r(corners.sw), rect.bottom() - r(corners.sw)),
        FRAC_PI_2,
    ));
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), color(rect.center().y));
    for (point, normal) in &outline {
        let inner = *point - *normal * feather;
        mesh.colored_vertex(inner, color(inner.y));
        mesh.colored_vertex(*point + *normal * feather, Color32::TRANSPARENT);
    }
    let n = outline.len() as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        let (inner_i, outer_i, inner_j, outer_j) = (1 + 2 * i, 2 + 2 * i, 1 + 2 * j, 2 + 2 * j);
        mesh.add_triangle(0, inner_i, inner_j);
        mesh.add_triangle(inner_i, outer_i, outer_j);
        mesh.add_triangle(inner_i, outer_j, inner_j);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    label
}
