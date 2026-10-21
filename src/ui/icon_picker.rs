//! "Choose project icon" (`components/settings/ProjectIconPickerDialog.tsx`,
//! `projectIconOptions.ts`, `projectIconColors.ts`): a Lucide icon in a color,
//! an emoji, or a monogram. Saving sets the project's `projectIcon` (and clears
//! its favicon path) with `project.meta.update`; a monogram travels as the
//! lucide wire shape with `monogramText` (`ProjectIconOverride`).

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::Project;
use crate::theme::{self, Weight, font};
use crate::ui::{emoji, lucide, project_icon, widgets};

const WIDTH: f32 = 512.0;
const CELL: f32 = 42.2;
const GAP: f32 = 4.0;
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74);

/// `PROJECT_EMOJIS`.
const EMOJIS: [&str; 30] = [
    "💻", "🛠️", "🚀", "🤖", "✨", "⚡", "🌐", "📱", "🖥️", "⌨️", "⚙️", "🗄️", "☁️", "📦", "📚", "🧪",
    "🔒", "🎮", "🎵", "🎬", "🖼️", "🛍️", "🔥", "💡", "🧩", "📊", "🧠", "🦄", "🐙", "🌱",
];

/// The swatches' Tailwind 500 shades (`swatchClassName`), in
/// `PROJECT_COLOR_NAMES` order.
const SWATCHES: [Color32; 18] = [
    Color32::from_rgb(0x6a, 0x72, 0x82),
    Color32::from_rgb(0xfb, 0x2c, 0x36),
    Color32::from_rgb(0xff, 0x69, 0x00),
    Color32::from_rgb(0xfe, 0x9a, 0x00),
    Color32::from_rgb(0xf0, 0xb1, 0x00),
    Color32::from_rgb(0x7c, 0xcf, 0x00),
    Color32::from_rgb(0x00, 0xc9, 0x50),
    Color32::from_rgb(0x00, 0xbc, 0x7d),
    Color32::from_rgb(0x00, 0xbb, 0xa7),
    Color32::from_rgb(0x00, 0xb8, 0xdb),
    Color32::from_rgb(0x00, 0xa6, 0xf4),
    Color32::from_rgb(0x2b, 0x7f, 0xff),
    Color32::from_rgb(0x61, 0x5f, 0xff),
    Color32::from_rgb(0x8e, 0x51, 0xff),
    Color32::from_rgb(0xad, 0x46, 0xff),
    Color32::from_rgb(0xe1, 0x2a, 0xfb),
    Color32::from_rgb(0xf6, 0x33, 0x9a),
    Color32::from_rgb(0xff, 0x20, 0x56),
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Lucide,
    Emoji,
    Monogram,
}

pub(crate) struct IconPicker {
    project_id: String,
    title: String,
    mode: Mode,
    icon_name: String,
    color: String,
    letters: String,
    emoji: String,
    query: String,
    custom_emoji: String,
}

/// `ProjectMonogramText`: a letter or number, then letters, numbers, marks
/// or joiners; 32 at most.
fn valid_monogram(text: &str) -> bool {
    let mut chars = text.chars();
    text.chars().count() <= 32
        && chars.next().is_some_and(char::is_alphanumeric)
        && chars.all(|c| {
            c.is_alphanumeric()
                || matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x20D0..=0x20FF | 0x200C | 0x200D)
        })
}

/// The saved icon in its wire shape.
fn icon_payload(picker: &IconPicker, monogram: &str) -> Value {
    match picker.mode {
        Mode::Monogram => {
            json!({"kind": "lucide", "name": "folder-code", "color": picker.color, "monogramText": monogram})
        }
        Mode::Lucide => json!({"kind": "lucide", "name": picker.icon_name, "color": picker.color}),
        Mode::Emoji => json!({"kind": "emoji", "emoji": picker.emoji}),
    }
}

impl App {
    /// Opens the picker on the project's current icon (or its automatic one).
    pub(crate) fn open_icon_picker(&mut self, project: &Project) {
        let current = project.project_icon.as_ref();
        let text = |key: &str| current.and_then(|i| i[key].as_str()).map(str::to_owned);
        let monogram = text("monogramText")
            .or_else(|| text("monogram"))
            .or_else(|| {
                (current.is_some_and(|i| i["kind"] == "monogram"))
                    .then(|| text("text"))
                    .flatten()
            });
        let kind = current.and_then(|i| i["kind"].as_str()).unwrap_or("lucide");
        let mode = if monogram.is_some() {
            Mode::Monogram
        } else if kind == "emoji" {
            Mode::Emoji
        } else {
            Mode::Lucide
        };
        let automatic = project_icon::derived_color_name(&project.title);
        self.icon_picker = Some(IconPicker {
            project_id: project.id.clone(),
            title: project.title.clone(),
            mode,
            icon_name: if mode == Mode::Lucide {
                text("name")
            } else {
                None
            }
            .unwrap_or_else(|| "folder-code".into()),
            color: if kind != "emoji" { text("color") } else { None }
                .unwrap_or_else(|| automatic.to_owned()),
            letters: monogram.unwrap_or_else(|| project_icon::derived(&project.title).monogram),
            emoji: if kind == "emoji" { text("emoji") } else { None }
                .unwrap_or_else(|| "💻".into()),
            query: String::new(),
            custom_emoji: String::new(),
        });
    }

    /// The dialog over `screen`.
    pub(crate) fn icon_picker_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut picker) = self.icon_picker.take() else {
            return;
        };
        let icons = lucide::filter(&picker.query);
        let monogram = picker.letters.trim().to_uppercase();
        let valid = valid_monogram(&monogram);
        let grid_rows = |count: usize| count.div_ceil(10) as f32;
        let grid_height =
            |count: usize| (grid_rows(count) * (CELL + GAP) - GAP + 4.0).clamp(0.0, 256.0);
        let swatch_rows = 2.0;
        let colors = 16.0 + 8.0 + swatch_rows * 30.0 - 6.0 + 16.0;
        let panel = match picker.mode {
            Mode::Lucide => {
                let empty = if icons.is_empty() { 20.0 + 64.0 } else { 0.0 };
                colors + 32.0 + 16.0 + grid_height(icons.len()) + empty
            }
            Mode::Emoji => grid_height(EMOJIS.len()) + 16.0 + 16.0 + 8.0 + 32.0,
            Mode::Monogram => colors + 98.0,
        };
        let band = 89.0 + 28.0 + 16.0 + panel + 24.0 - 2.0;
        let height = band + 65.6;
        let d = Rect::from_center_size(screen.center(), Vec2::new(WIDTH, height));
        let mut close = false;
        let mut save = false;
        egui::Area::new(ui.id().with("icon-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(d) {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close_rect = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close_rect,
                    "icon-picker-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                {
                    close = true;
                }
                let left = d.left() + 25.0;
                let right = d.right() - 25.0;
                let width = right - left;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Choose project icon",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 53.0),
                    "Choose an icon, emoji, or monogram.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );

                // The segmented "Icon type" toggle.
                let mut x = left + 2.0;
                let toggle_top = d.top() + 89.0;
                let modes = [
                    (Mode::Lucide, "Icons"),
                    (Mode::Emoji, "Emoji"),
                    (Mode::Monogram, "Monogram"),
                ];
                let widths: Vec<f32> = modes
                    .iter()
                    .map(|(_, l)| widgets::text_width(ui, l, font(14.0, Weight::Medium)) + 24.0)
                    .collect();
                let group = Rect::from_min_size(
                    Pos2::new(left, toggle_top),
                    Vec2::new(widths.iter().sum::<f32>() + 2.0 * 2.0 + 4.0, 28.0),
                );
                ui.painter()
                    .rect_filled(group, CornerRadius::same(10), theme::overlay(0.032));
                for ((mode, label), w) in modes.iter().zip(&widths) {
                    let rect =
                        Rect::from_min_size(Pos2::new(x, toggle_top + 2.0), Vec2::new(*w, 24.0));
                    let response =
                        ui.interact(rect, ui.id().with(("icon-mode", label)), Sense::click());
                    let active = picker.mode == *mode;
                    if active {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(8),
                            theme::overlay(0.0576),
                        );
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(rect.left() + 12.0, rect.top()), rect.max),
                        label,
                        font(14.0, Weight::Medium),
                        if active || response.hovered() {
                            theme::foreground()
                        } else {
                            theme::muted_foreground()
                        },
                    );
                    if response.clicked() {
                        picker.mode = *mode;
                    }
                    x = rect.right() + 2.0;
                }
                let mut y = toggle_top + 28.0 + 16.0;

                // Color (icons and monograms).
                if picker.mode != Mode::Emoji {
                    widgets::text(
                        ui,
                        Pos2::new(left, y),
                        "Color",
                        font(12.0, Weight::Medium),
                        theme::muted_foreground(),
                        16.0,
                    );
                    y += 24.0;
                    for (index, name) in theme::PROJECT_COLOR_NAMES.iter().enumerate() {
                        let column = (index % 15) as f32;
                        let row = (index / 15) as f32;
                        let button = Rect::from_min_size(
                            Pos2::new(left + column * 30.0, y + row * 30.0),
                            Vec2::splat(24.0),
                        );
                        let response =
                            ui.interact(button, ui.id().with(("icon-color", name)), Sense::click());
                        if picker.color == *name {
                            ui.painter().circle_stroke(
                                button.center(),
                                11.5,
                                Stroke::new(1.0, theme::alpha(theme::foreground(), 0.64)),
                            );
                        }
                        ui.painter()
                            .circle_filled(button.center(), 8.0, SWATCHES[index]);
                        if response.clicked() {
                            picker.color = (*name).to_owned();
                        }
                    }
                    y += swatch_rows * 30.0 - 6.0 + 16.0;
                }
                let tint =
                    project_icon::color_named(&picker.color).unwrap_or(theme::PROJECT_COLORS[0]);

                match picker.mode {
                    Mode::Lucide => {
                        let search =
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0));
                        input(
                            ui,
                            search,
                            "icon-search",
                            &mut picker.query,
                            "Search all Lucide icons",
                            false,
                        );
                        y += 32.0 + 16.0;
                        let area = Rect::from_min_size(
                            Pos2::new(left, y),
                            Vec2::new(width, grid_height(icons.len())),
                        );
                        let mut chosen = None;
                        grid(ui, area, "icon-grid", icons.len(), |ui, index, cell| {
                            let name = icons[index];
                            let selected = picker.icon_name == name;
                            if cell_button(ui, cell, ("icon-cell", name), selected) {
                                chosen = Some(name);
                            }
                            lucide::paint(
                                ui,
                                Rect::from_center_size(cell.center(), Vec2::splat(20.0)),
                                name,
                                tint,
                            );
                        });
                        if let Some(name) = chosen {
                            picker.icon_name = name.to_owned();
                        }
                        if icons.is_empty() {
                            let w = widgets::text_width(
                                ui,
                                "No icons found.",
                                font(14.0, Weight::Regular),
                            );
                            widgets::text(
                                ui,
                                Pos2::new(d.center().x - w / 2.0, y + 32.0),
                                "No icons found.",
                                font(14.0, Weight::Regular),
                                theme::muted_foreground(),
                                20.0,
                            );
                        }
                    }
                    Mode::Emoji => {
                        let area = Rect::from_min_size(
                            Pos2::new(left, y),
                            Vec2::new(width, grid_height(EMOJIS.len())),
                        );
                        let mut chosen = None;
                        grid(ui, area, "emoji-grid", EMOJIS.len(), |ui, index, cell| {
                            let value = EMOJIS[index];
                            if cell_button(ui, cell, ("emoji-cell", index), picker.emoji == value) {
                                chosen = Some(value);
                            }
                            emoji::paint(ui, cell.center(), 20.0, value);
                        });
                        if let Some(value) = chosen {
                            picker.emoji = value.to_owned();
                        }
                        y = area.bottom() + 16.0;
                        widgets::text(
                            ui,
                            Pos2::new(left, y),
                            "Or paste any emoji",
                            font(12.0, Weight::Medium),
                            theme::muted_foreground(),
                            16.0,
                        );
                        y += 24.0;
                        let before = picker.custom_emoji.clone();
                        input(
                            ui,
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)),
                            "icon-custom-emoji",
                            &mut picker.custom_emoji,
                            "Paste an emoji",
                            false,
                        );
                        if picker.custom_emoji != before
                            && let Some(value) = emoji::first_emoji(&picker.custom_emoji)
                        {
                            picker.emoji = value;
                        }
                    }
                    Mode::Monogram => {
                        // `ProjectMonogram size-12`, then Letters.
                        let content = y + 8.0;
                        let shown = if valid {
                            monogram.clone()
                        } else {
                            project_icon::derived(&picker.title).monogram
                        };
                        project_icon::paint_monogram(
                            ui,
                            Rect::from_min_size(Pos2::new(left, content + 17.0), Vec2::splat(48.0)),
                            &shown,
                            tint,
                        );
                        let column = left + 48.0 + 16.0;
                        widgets::text(
                            ui,
                            Pos2::new(column, content + 3.0),
                            "Letters",
                            font(14.0, Weight::Medium),
                            theme::foreground(),
                            18.0,
                        );
                        let field = Rect::from_min_max(
                            Pos2::new(column, content + 24.0),
                            Pos2::new(right, content + 56.0),
                        );
                        input(ui, field, "icon-letters", &mut picker.letters, "", !valid);
                        widgets::text(
                            ui,
                            Pos2::new(column, content + 64.0),
                            "One or two letters or numbers.",
                            font(12.0, Weight::Regular),
                            theme::muted_foreground(),
                            16.0,
                        );
                    }
                }

                // The footer band.
                let band_rect = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.top() + band),
                    d.max - Vec2::splat(1.0),
                );
                ui.painter().rect_filled(
                    band_rect,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.0216),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(band_rect.min, Vec2::new(band_rect.width(), 1.0)),
                    0.0,
                    theme::overlay(0.07),
                );
                let footer = band_rect.top() + 17.0;
                let save_rect = footer_rect(ui, right, footer, "Save icon");
                let cancel_rect = footer_rect(ui, save_rect.left() - 8.0, footer, "Cancel");
                let can_save = picker.mode != Mode::Monogram || valid;
                if footer_button(ui, save_rect, "Save icon", true, can_save) {
                    save = true;
                }
                if footer_button(ui, cancel_rect, "Cancel", false, true) {
                    close = true;
                }
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if save {
            // \`setProjectIcon({ faviconPath: null, projectIcon })\`.
            self.dispatch(json!({
                "type": "project.meta.update",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": picker.project_id,
                "faviconPath": null,
                "projectIcon": icon_payload(&picker, &monogram),
            }));
            return;
        }
        if !close {
            self.icon_picker = Some(picker);
        }
    }
}

/// A 10-column grid of 42px cells (\`grid-cols-10 gap-1 p-0.5\`), scrolling
/// past 256px.
fn grid(
    ui: &mut Ui,
    area: Rect,
    id: &str,
    count: usize,
    mut cell: impl FnMut(&mut Ui, usize, Rect),
) {
    if count == 0 {
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(area.height())
        .auto_shrink([false, true])
        .show(&mut child, |ui| {
            let top = ui.max_rect().top() + 2.0;
            let left = area.left() + 2.0;
            for index in 0..count {
                let rect = Rect::from_min_size(
                    Pos2::new(
                        left + (index % 10) as f32 * (CELL + GAP),
                        top + (index / 10) as f32 * (CELL + GAP),
                    ),
                    Vec2::splat(CELL),
                );
                cell(ui, index, rect);
            }
            let rows = count.div_ceil(10) as f32;
            ui.allocate_rect(
                Rect::from_min_size(
                    Pos2::new(area.left(), ui.max_rect().top()),
                    Vec2::new(area.width(), rows * (CELL + GAP) - GAP + 4.0),
                ),
                Sense::hover(),
            );
        });
}

/// A grid cell: \`border-border bg-accent\` when selected, \`bg-accent\` on hover.
fn cell_button(
    ui: &Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: bool,
) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    if selected {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            theme::accent(),
            Stroke::new(1.0, theme::overlay(0.06)),
            StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::accent());
    }
    response.clicked()
}

/// The dialog \`Input\` (32px); \`invalid\` borders it in red.
fn input(ui: &mut Ui, rect: Rect, id: &str, text: &mut String, placeholder: &str, invalid: bool) {
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        theme::control_surface(),
        Stroke::new(
            1.0,
            if invalid {
                theme::alpha(DESTRUCTIVE, 0.36)
            } else {
                theme::input()
            },
        ),
        StrokeKind::Inside,
    );
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 1.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.add(
        egui::TextEdit::singleline(text)
            .id(egui::Id::new(id))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(font(14.0, Weight::Regular))
            .text_color(theme::foreground())
            .vertical_align(egui::Align::Center)
            .desired_width(inner.width())
            .min_size(Vec2::new(0.0, inner.height())),
    );
    // Painted like the settings fields: egui's hint text renders lighter.
    if text.is_empty() && !placeholder.is_empty() {
        widgets::text_in(
            ui,
            inner,
            placeholder,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
        );
    }
}

fn footer_rect(ui: &Ui, right: f32, top: f32, label: &str) -> Rect {
    let width = (widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 24.0).ceil();
    Rect::from_min_size(Pos2::new(right - width, top), Vec2::new(width, 32.0))
}

/// A dialog footer \`Button\`: primary or outline; dimmed when disabled.
fn footer_button(ui: &Ui, rect: Rect, label: &str, primary: bool, enabled: bool) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(("icon-footer", label)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let alpha = if enabled { 1.0 } else { 0.64 };
    let color = if primary {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            theme::alpha(
                if hovered {
                    theme::primary_hover()
                } else {
                    theme::primary()
                },
                alpha,
            ),
        );
        Color32::WHITE
    } else {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            if hovered {
                theme::overlay(0.06)
            } else {
                theme::control_surface()
            },
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        theme::foreground()
    };
    let width = widgets::text_width(ui, label, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        font(14.0, Weight::Medium),
        theme::alpha(color, alpha),
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monograms_and_payloads_follow_upstream() {
        assert!(
            valid_monogram("T3")
                && valid_monogram("É")
                && !valid_monogram("-A")
                && !valid_monogram("")
        );
        let picker = IconPicker {
            project_id: "p".into(),
            title: "Demo".into(),
            mode: Mode::Monogram,
            icon_name: "folder-code".into(),
            color: "indigo".into(),
            letters: "t3".into(),
            emoji: "💻".into(),
            query: String::new(),
            custom_emoji: String::new(),
        };
        assert_eq!(
            icon_payload(&picker, "T3"),
            json!({"kind": "lucide", "name": "folder-code", "color": "indigo", "monogramText": "T3"})
        );
    }
}
