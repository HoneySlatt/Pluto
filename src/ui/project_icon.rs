//! Project monogram tiles (`ProjectMonogram.tsx`, `projectIdentity.ts`).
//! Favicon images are not fetched yet; projects fall back to their monogram.
use eframe::egui::{self, Align, Color32, Pos2, Rect, Ui, text::LayoutJob};

use crate::{model::Project, theme};

pub struct Identity {
    pub monogram: String,
    pub color: Color32,
}

fn words(name: &str) -> Vec<Vec<char>> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn monogram(name: &str) -> String {
    let words = words(name);
    let Some(first_word) = words.first() else {
        return "PR".into();
    };
    let first = first_word[0];
    let second = first_word[1..]
        .iter()
        .copied()
        .find(|c| c.is_numeric())
        .or_else(|| {
            if words.len() > 1 {
                words.last().and_then(|w| w.first().copied())
            } else {
                first_word.last().copied()
            }
        })
        .unwrap_or(first);
    format!("{first}{second}")
        .to_uppercase()
        .chars()
        .take(2)
        .collect()
}

fn color_index(name: &str) -> usize {
    let seed = name.trim().to_lowercase();
    let seed = if seed.is_empty() {
        "project".into()
    } else {
        seed
    };
    let mut index = 0usize;
    for c in seed.chars() {
        index = (index * 31 + c as usize) % theme::PROJECT_COLORS.len();
    }
    index
}

pub fn identity(project: &Project) -> Identity {
    let icon = project.project_icon.as_ref();
    let color_of = |value: &serde_json::Value| {
        value["color"].as_str().and_then(|name| {
            theme::PROJECT_COLOR_NAMES
                .iter()
                .position(|n| *n == name)
                .map(|i| theme::PROJECT_COLORS[i])
        })
    };
    if let Some(icon) = icon.filter(|i| i["kind"] == "monogram" || i["monogramText"].is_string()) {
        let text = icon["text"]
            .as_str()
            .or(icon["monogramText"].as_str())
            .unwrap_or("PR");
        return Identity {
            monogram: text.into(),
            color: color_of(icon).unwrap_or(theme::PROJECT_COLORS[11]),
        };
    }
    Identity {
        monogram: monogram(&project.title),
        color: theme::PROJECT_COLORS[color_index(&project.title)],
    }
}

/// Paints the 16×16-viewBox monogram tile scaled into `rect`.
pub fn paint(ui: &Ui, rect: Rect, project: &Project) {
    paint_identity(ui, rect, identity(project));
}

/// `opacity-40 grayscale`: the resting favicon of parked sidebar rows.
pub fn paint_dimmed(ui: &Ui, rect: Rect, project: &Project) {
    let mut identity = identity(project);
    let [r, g, b, _] = identity.color.to_array();
    let gray = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32).round() as u8;
    identity.color = theme::alpha(Color32::from_rgb(gray, gray, gray), 0.4);
    paint_identity(ui, rect, identity);
}

fn paint_identity(ui: &Ui, rect: Rect, identity: Identity) {
    let scale = rect.width() / 16.0;
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same((rect.width() * 0.25).round() as u8),
        theme::alpha(identity.color, 0.14),
    );
    let glyphs = identity.monogram.chars().count();
    let target = if glyphs == 1 { 6.0 } else { 12.0 } * scale;
    let font = theme::mono_bold(8.25 * scale);
    let natural = {
        let mut job =
            LayoutJob::simple_singleline(identity.monogram.clone(), font.clone(), identity.color);
        job.halign = Align::LEFT;
        ui.fonts_mut(|f| f.layout_job(job)).size().x
    };
    // SVG `textLength` stretches the glyph run to a fixed width.
    let spacing = if glyphs > 1 {
        (target - natural) / glyphs as f32
    } else {
        0.0
    };
    let mut job = LayoutJob::default();
    job.append(
        &identity.monogram,
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            color: identity.color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let width = galley.size().x - if glyphs > 1 { spacing } else { 0.0 };
    // The SVG baseline sits at y = 10.8 of 16.
    let ascent = galley
        .rows
        .first()
        .map_or(font.size * 0.8, |r| r.row.height() * 0.78);
    let pos = Pos2::new(
        rect.center().x - width / 2.0,
        rect.top() + 10.8 * scale - ascent,
    );
    ui.painter().galley(pos, galley, identity.color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monograms_match_upstream() {
        assert_eq!(monogram("T3 Code Rust"), "T3");
        assert_eq!(monogram("NixOS"), "NS");
        assert_eq!(monogram("web-demo"), "WD");
        assert_eq!(monogram(""), "PR");
    }

    #[test]
    fn colors_match_upstream() {
        let name = |title: &str| theme::PROJECT_COLOR_NAMES[color_index(title)];
        assert_eq!(name("T3 Code Rust"), "indigo");
        assert_eq!(name("NixOS"), "cyan");
        assert_eq!(name("web-demo"), "pink");
    }
}
