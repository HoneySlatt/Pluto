use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Galley, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, text::LayoutJob,
};

use crate::theme;

/// Text laid out like a CSS block: `line_height` is the CSS line box.
pub fn galley(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    line_height: f32,
    wrap_width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, format(font, color, line_height));
    job.wrap.max_width = wrap_width;
    center_rows(ui.fonts_mut(|f| f.layout_job(job)))
}

/// Single-line text truncated with an ellipsis at `max_width`.
pub fn galley_truncated(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    line_height: f32,
    max_width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, format(font, color, line_height));
    job.wrap.max_width = max_width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    center_rows(ui.fonts_mut(|f| f.layout_job(job)))
}

pub fn format(font: FontId, color: Color32, line_height: f32) -> egui::TextFormat {
    egui::TextFormat {
        font_id: font,
        color,
        line_height: Some(line_height),
        valign: Align::Center,
        ..Default::default()
    }
}

/// Paints text whose CSS line box starts at `pos`; returns its size.
pub fn text(
    ui: &Ui,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    line_height: f32,
) -> Vec2 {
    let galley = galley(ui, text, font, color, line_height, f32::INFINITY);
    let size = galley.size();
    ui.painter().galley(pos, galley, color);
    size
}

/// Tailwind's paired line height for a font size (`text-xs` 12/16, `text-sm` 14/20…).
pub fn line_height_for(size: f32) -> f32 {
    match size.round() as i32 {
        11 => 16.0,
        12 => 16.0,
        13 => 18.0,
        14 => 20.0,
        16 => 24.0,
        18 => 28.0,
        _ => (size * 1.4).round(),
    }
}

/// Paints text vertically centered in `rect`, starting at its left edge.
pub fn text_in(ui: &Ui, rect: Rect, text: &str, font: FontId, color: Color32) -> f32 {
    let line = line_height_for(font.size);
    let galley = galley_truncated(ui, text, font, color, line, rect.width());
    let width = galley.size().x;
    let pos = Pos2::new(rect.left(), (rect.center().y - line / 2.0).round());
    ui.painter().galley(pos, galley, color);
    width
}

pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    galley(ui, text, font, Color32::WHITE, 20.0, f32::INFINITY)
        .size()
        .x
}

/// Geist's tabular figure advance (`tabular-nums`), in ems.
const TABULAR_ADVANCE: f32 = 0.625;

/// Lays out `text` with `tabular-nums`: each digit is centered in a fixed
/// cell. Returns the glyph offsets and the total width.
fn tabular_layout(ui: &Ui, text: &str, font: &FontId) -> (Vec<(char, f32, f32)>, f32) {
    let mut x = 0.0;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let width = text_width(ui, c.encode_utf8(&mut [0; 4]), font.clone());
        let advance = if c.is_ascii_digit() {
            font.size * TABULAR_ADVANCE
        } else {
            width
        };
        glyphs.push((c, x + (advance - width) / 2.0, width));
        x += advance;
    }
    (glyphs, x)
}

pub fn tabular_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    tabular_layout(ui, text, &font).1
}

/// `text` with `tabular-nums`, its CSS line box starting at `pos`.
pub fn tabular_text(
    ui: &Ui,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    line_height: f32,
) -> f32 {
    let (glyphs, width) = tabular_layout(ui, text, &font);
    for (c, x, _) in glyphs {
        self::text(
            ui,
            Pos2::new(pos.x + x, pos.y),
            c.encode_utf8(&mut [0; 4]),
            font.clone(),
            color,
            line_height,
        );
    }
    width
}

pub fn radius(r: f32) -> CornerRadius {
    CornerRadius::same(r.round() as u8)
}

/// Ghost icon button (`variant="ghost"`): transparent until hovered.
pub fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    id: impl egui::AsIdSalt,
    icon: &str,
    icon_size: f32,
    tint: Color32,
    tooltip: &str,
) -> Response {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, radius(theme::CONTROL_RADIUS as f32), theme::accent());
    }
    let tint = if response.hovered() {
        theme::foreground()
    } else {
        tint
    };
    theme::paint_icon(ui, icon, rect, icon_size, tint);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// Outline button used by the workspace header (`variant="outline" size="xs"`).
pub fn outline_button(
    ui: &mut Ui,
    rect: Rect,
    id: impl egui::AsIdSalt,
    icon: Option<(&str, Color32)>,
    label: &str,
    corners: CornerRadius,
) -> Response {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let fill = if response.hovered() {
        theme::overlay(0.06)
    } else {
        theme::control_surface()
    };
    ui.painter().rect(
        rect,
        corners,
        fill,
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let mut x = rect.left() + 7.0;
    if let Some((name, tint)) = icon {
        let icon_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.top()),
            Vec2::new(14.0, rect.height()),
        );
        theme::paint_icon(ui, name, icon_rect, 14.0, tint);
        x = rect.left() + 24.0;
    }
    if !label.is_empty() {
        text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x, rect.top()),
                Pos2::new(rect.right(), rect.bottom()),
            ),
            label,
            theme::font(12.0, theme::Weight::Medium),
            theme::foreground(),
        );
    }
    response
}

/// Width of an outline button for a label, matching `outline_button` padding.
pub fn outline_button_width(ui: &Ui, has_icon: bool, label: &str) -> f32 {
    let label_width = if label.is_empty() {
        0.0
    } else {
        text_width(ui, label, theme::font(12.0, theme::Weight::Medium))
    };
    ((if has_icon { 24.0 } else { 7.0 }) + label_width + 7.0).ceil()
}

/// Relative time label used by the sidebar (`formatRelativeTime`, compact).
pub fn relative_time(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(date) = crate::timeline::parse_time(iso) else {
        return String::new();
    };
    let seconds = (now - date).num_seconds();
    if seconds < 60 {
        return "now".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

/// `formatDayAwareTimestamp` with the default locale 12-hour format.
pub fn clock_time(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(date) = crate::timeline::parse_time(iso) else {
        return String::new();
    };
    let local = date.with_timezone(&chrono::Local);
    let today = now.with_timezone(&chrono::Local).date_naive();
    let time = local.format("%-I:%M %p").to_string();
    if local.date_naive() == today {
        time
    } else {
        format!("{} {time}", local.format("%b %-d"))
    }
}

/// Places glyphs like a CSS line box: the tallest font gets equal
/// half-leading above and below, and every font shares its baseline.
/// epaint puts each glyph at the top of tall rows instead.
pub fn center_rows(mut galley: Arc<Galley>) -> Arc<Galley> {
    let galley_mut = Arc::make_mut(&mut galley);
    // CSS keeps fractional line boxes (22.75px); epaint rounds every row.
    let uniform_line = galley_mut
        .job
        .sections
        .first()
        .and_then(|s| s.format.line_height)
        .filter(|line| {
            galley_mut
                .job
                .sections
                .iter()
                .all(|s| s.format.line_height == Some(*line))
        });
    if let Some(line) = uniform_line {
        for (index, placed) in galley_mut.rows.iter_mut().enumerate() {
            placed.pos.y = (index as f32 * line).round();
        }
        let rows = galley_mut.rows.len().max(1) as f32;
        galley_mut.rect.max.y = galley_mut.rect.min.y + rows * line;
    }
    for placed in &mut galley_mut.rows {
        let Some(strut) = placed
            .row
            .glyphs
            .iter()
            .max_by(|a, b| a.font_height.total_cmp(&b.font_height))
        else {
            continue;
        };
        let baseline = ((placed.row.size.y - strut.font_height) / 2.0).round() + strut.font_ascent;
        let row = Arc::make_mut(&mut placed.row);
        let vertex_end = row.visuals.mesh.vertices.len() as u32;
        let starts: Vec<u32> = row.glyphs.iter().map(|g| g.first_vertex).collect();
        let mut moved_any = false;
        for (index, glyph) in row.glyphs.iter_mut().enumerate() {
            let dy = (baseline - glyph.pos.y).round();
            if dy == 0.0 {
                continue;
            }
            moved_any = true;
            glyph.pos.y += dy;
            let end = starts
                .get(index + 1)
                .copied()
                .unwrap_or(row.visuals.glyph_vertex_range.end as u32)
                .min(vertex_end);
            for vertex in &mut row.visuals.mesh.vertices[glyph.first_vertex as usize..end as usize]
            {
                vertex.pos.y += dy;
            }
        }
        if moved_any {
            row.visuals.mesh_bounds = row
                .visuals
                .mesh
                .vertices
                .iter()
                .fold(egui::Rect::NOTHING, |r, v| {
                    r.union(egui::Rect::from_min_max(v.pos, v.pos))
                });
        }
    }
    galley
}

/// A menu row (`MenuItem`, `MenuGroupLabel`, `MenuSeparator`).
pub enum MenuEntry<'a> {
    Header(&'a str),
    Separator,
    Item(MenuItem<'a>),
}

pub struct MenuItem<'a> {
    pub label: &'a str,
    pub icon: Option<&'a str>,
    /// Current value: highlighted surface (`bg-foreground/8`).
    pub selected: bool,
    pub badge: Option<&'a str>,
    pub detail: Option<&'a str>,
    /// Bold title, as in the runtime mode menu.
    pub strong: bool,
    /// Destructive action (`data-[variant=destructive]`), drawn in red.
    pub destructive: bool,
    /// Trailing `MenuShortcut` text.
    pub shortcut: Option<&'a str>,
    /// 14px by default; plain `MenuItem` icons are 16px (`size-4 -mx-0.5`).
    pub icon_size: f32,
    /// `data-disabled`: dimmed to 64% and inert; `tooltip` says why.
    pub disabled: bool,
    pub tooltip: Option<&'a str>,
}

impl<'a> MenuItem<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            icon: None,
            selected: false,
            badge: None,
            detail: None,
            strong: false,
            destructive: false,
            shortcut: None,
            icon_size: 14.0,
            disabled: false,
            tooltip: None,
        }
    }
}

fn menu_surface() -> Color32 {
    theme::popover()
}
fn menu_outline() -> Color32 {
    theme::stock_or(Color32::from_rgba_premultiplied(25, 25, 25, 26), |_| {
        theme::overlay(0.1)
    })
}
fn menu_selected() -> Color32 {
    theme::overlay(0.078)
}

fn menu_entry_height(entry: &MenuEntry) -> f32 {
    match entry {
        MenuEntry::Header(_) => 26.0,
        MenuEntry::Separator => 9.0,
        MenuEntry::Item(item) => match (item.detail, item.icon) {
            (Some(_), Some(_)) => 46.0,
            (Some(_), None) => 44.0,
            _ => 28.0,
        },
    }
}

/// Paints the dropdown surface shared by menus and pickers.
pub fn popup_surface(ui: &Ui, rect: Rect) {
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        menu_surface(),
        Stroke::new(1.0, menu_outline()),
        StrokeKind::Inside,
    );
}

/// Shows a popup at `rect` in the foreground layer; closes `open` on an
/// outside click or Escape. Returns the area response.
pub fn popup(
    ui: &mut Ui,
    open: &mut Option<String>,
    id: &str,
    rect: Rect,
    anchor: Rect,
    content: impl FnOnce(&mut Ui, Rect),
) {
    let area = egui::Area::new(ui.id().with(("popup", id)))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            let (inner, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
            popup_surface(ui, inner);
            content(ui, inner);
        });
    let pressed_outside = ui.input(|i| i.pointer.any_pressed())
        && !area.response.contains_pointer()
        && !ui.rect_contains_pointer(anchor);
    if pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = None;
    }
}

/// `MenuShortcut`: 12px medium, `tracking-widest` (0.1em after each glyph).
fn shortcut_galley(ui: &Ui, text: &str) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            extra_letter_spacing: 1.2,
            ..format(
                theme::font(12.0, theme::Weight::Medium),
                theme::muted_foreground(),
                16.0,
            )
        },
    );
    center_rows(ui.fonts_mut(|f| f.layout_job(job)))
}

/// Dropdown menu (`MenuPopup`) above or below `anchor`, left-aligned with it.
/// Returns the index (into `entries`) of a clicked item and closes the menu.
pub fn menu(
    ui: &mut Ui,
    open: &mut Option<String>,
    id: &str,
    anchor: Rect,
    above: bool,
    entries: &[MenuEntry],
) -> Option<usize> {
    menu_aligned(ui, open, id, anchor, above, false, entries)
}

/// `menu` with `align="end"` when `end` is set: right edges line up, and the
/// menu falls back to the anchor's left edge when that overflows the window.
pub fn menu_aligned(
    ui: &mut Ui,
    open: &mut Option<String>,
    id: &str,
    anchor: Rect,
    above: bool,
    end: bool,
    entries: &[MenuEntry],
) -> Option<usize> {
    if open.as_deref() != Some(id) {
        return None;
    }
    let body = theme::font(14.0, theme::Weight::Regular);
    let strong = theme::font(14.0, theme::Weight::Medium);
    let small = theme::font(12.0, theme::Weight::Regular);
    let badge_font = theme::font(10.0, theme::Weight::Medium);
    let content_width = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Header(label) => {
                text_width(ui, label, theme::font(12.0, theme::Weight::Medium))
            }
            MenuEntry::Separator => 0.0,
            MenuEntry::Item(item) => {
                let title = if item.strong {
                    strong.clone()
                } else {
                    body.clone()
                };
                let mut width = text_width(ui, item.label, title)
                    + if item.icon.is_some() { 20.0 } else { 0.0 };
                if let Some(badge) = item.badge {
                    width += 6.0 + text_width(ui, badge, badge_font.clone()) + 8.0;
                }
                if let Some(detail) = item.detail {
                    width = width.max(text_width(ui, detail, small.clone()));
                }
                if let Some(shortcut) = item.shortcut {
                    width += 8.0 + shortcut_galley(ui, shortcut).size().x;
                }
                width
            }
        })
        .fold(0.0, f32::max);
    let width = (content_width + 26.0).max(172.0).max(anchor.width()).ceil();
    let height = entries.iter().map(menu_entry_height).sum::<f32>() + 10.0;
    // Flip to the other side when the preferred one overflows the window.
    let screen = ui.ctx().content_rect();
    let below = anchor.bottom() + 4.0;
    let over = anchor.top() - 4.0 - height;
    let y = if above {
        if over < screen.top() && below + height <= screen.bottom() {
            below
        } else {
            over
        }
    } else if below + height > screen.bottom() && over >= screen.top() {
        over
    } else {
        below
    };
    let preferred = if end && anchor.right() - width >= screen.left() + 5.0 {
        anchor.right() - width
    } else {
        anchor.left()
    };
    let x = preferred
        .min(screen.right() - width - 4.0)
        .max(screen.left() + 4.0);
    let origin = Pos2::new(x, y.max(screen.top() + 4.0));
    let mut clicked = None;
    popup(
        ui,
        open,
        id,
        Rect::from_min_size(origin, Vec2::new(width, height)),
        anchor,
        |ui, rect| {
            let mut y = rect.top() + 5.0;
            for (index, entry) in entries.iter().enumerate() {
                let row = Rect::from_min_size(
                    Pos2::new(rect.left() + 5.0, y),
                    Vec2::new(rect.width() - 10.0, menu_entry_height(entry)),
                );
                y = row.bottom();
                match entry {
                    MenuEntry::Header(label) => {
                        text(
                            ui,
                            Pos2::new(row.left() + 8.0, row.top() + 6.0),
                            label,
                            theme::font(12.0, theme::Weight::Medium),
                            theme::muted_foreground(),
                            16.0,
                        );
                    }
                    MenuEntry::Separator => {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(row.left() + 8.0, row.top() + 4.0),
                                Vec2::new(row.width() - 16.0, 1.0),
                            ),
                            0.0,
                            theme::border(),
                        );
                    }
                    MenuEntry::Item(item) => {
                        let response = ui.interact(
                            row,
                            ui.id().with(("menu-item", id, index)),
                            if item.disabled {
                                Sense::hover()
                            } else {
                                Sense::click()
                            },
                        );
                        let response = match item.tooltip {
                            Some(tip) => response.on_hover_text(tip),
                            None => response,
                        };
                        let dim = |color: Color32| {
                            if item.disabled {
                                theme::alpha(color, 0.64)
                            } else {
                                color
                            }
                        };
                        if item.selected {
                            ui.painter()
                                .rect_filled(row, CornerRadius::same(6), menu_selected());
                        } else if response.hovered() && !item.disabled {
                            ui.painter()
                                .rect_filled(row, CornerRadius::same(6), theme::accent());
                        }
                        let mut x = row.left() + 8.0;
                        let title_top = row.top() + 4.0;
                        if let Some(icon) = item.icon {
                            let size = item.icon_size;
                            theme::paint_icon(
                                ui,
                                icon,
                                Rect::from_min_size(
                                    Pos2::new(x - (size - 14.0), title_top),
                                    Vec2::new(size, 20.0),
                                ),
                                size,
                                dim(if item.destructive {
                                    theme::error_foreground()
                                } else {
                                    theme::muted_foreground()
                                }),
                            );
                            x += 20.0;
                        }
                        let title = if item.strong {
                            strong.clone()
                        } else {
                            body.clone()
                        };
                        let size = self::text(
                            ui,
                            Pos2::new(x, title_top),
                            item.label,
                            title,
                            dim(if item.destructive {
                                theme::error_foreground()
                            } else {
                                theme::foreground()
                            }),
                            20.0,
                        );
                        if let Some(badge) = item.badge {
                            let badge_width = text_width(ui, badge, badge_font.clone()) + 8.0;
                            let chip = Rect::from_min_size(
                                Pos2::new((x + size.x + 6.0).round(), title_top + 2.0),
                                Vec2::new(badge_width.ceil(), 16.0),
                            );
                            ui.painter().rect(
                                chip,
                                CornerRadius::same(4),
                                theme::control_surface(),
                                Stroke::new(1.0, theme::input()),
                                StrokeKind::Inside,
                            );
                            text_in(
                                ui,
                                Rect::from_min_max(chip.min + Vec2::new(4.0, 0.0), chip.max),
                                badge,
                                badge_font.clone(),
                                theme::foreground(),
                            );
                        }
                        if let Some(shortcut) = item.shortcut {
                            let galley = shortcut_galley(ui, shortcut);
                            let pos =
                                Pos2::new(row.right() - 8.0 - galley.size().x, title_top + 2.0);
                            ui.painter().galley_with_override_text_color(
                                pos,
                                galley,
                                dim(theme::muted_foreground()),
                            );
                        }
                        if let Some(detail) = item.detail {
                            let gap = if item.icon.is_some() { 2.0 } else { 0.0 };
                            let color = if item.icon.is_some() {
                                theme::muted_foreground()
                            } else {
                                theme::alpha(theme::muted_foreground(), 0.8)
                            };
                            text(
                                ui,
                                Pos2::new(row.left() + 8.0, title_top + 20.0 + gap),
                                detail,
                                small.clone(),
                                color,
                                16.0,
                            );
                        }
                        if response.clicked() {
                            clicked = Some(index);
                        }
                    }
                }
            }
        },
    );
    if clicked.is_some() {
        *open = None;
    }
    clicked
}

/// Filled button (`variant="default" size="xs"`), primary background.
pub fn primary_button(
    ui: &mut Ui,
    rect: Rect,
    id: impl egui::AsIdSalt,
    label: &str,
    enabled: bool,
) -> Response {
    let response = ui.interact(
        rect,
        ui.id().with(id),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if response.hovered() && enabled {
        theme::primary_hover()
    } else {
        theme::primary()
    };
    let fill = if enabled {
        fill
    } else {
        theme::alpha(fill, 0.64)
    };
    ui.painter()
        .rect_filled(rect, radius(theme::CONTROL_RADIUS as f32), fill);
    let width = text_width(ui, label, theme::font(12.0, theme::Weight::Medium));
    text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        theme::font(12.0, theme::Weight::Medium),
        Color32::WHITE,
    );
    response
}

/// Search field heading a combobox popup (`ComboboxSearchInput`): magnifier,
/// borderless input and a focus-ring underline. `rect` is the 33px field.
pub fn combobox_search(ui: &mut Ui, rect: Rect, query: &mut String, hint: &str) -> Response {
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() - 1.0),
            Vec2::new(rect.width(), 1.0),
        ),
        0.0,
        theme::primary(),
    );
    theme::paint_icon(
        ui,
        "search",
        Rect::from_min_size(rect.min, Vec2::new(16.0, rect.height())),
        16.0,
        theme::alpha(theme::muted_foreground(), 0.55),
    );
    let mut edit_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
        Pos2::new(rect.left() + 20.0, rect.top() + 4.0),
        Pos2::new(rect.right(), rect.bottom() - 8.0),
    )));
    let response = edit_ui.add(
        egui::TextEdit::singleline(query)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(theme::font(14.0, theme::Weight::Regular))
            .text_color(theme::foreground())
            .hint_text(
                egui::RichText::new(hint)
                    .font(theme::font(14.0, theme::Weight::Regular))
                    .color(theme::muted_foreground()),
            )
            .desired_width(rect.width() - 20.0),
    );
    response.request_focus();
    response
}

/// Opens an http(s) URL in the system browser (`shell.openExternal`); this
/// build has no `links` feature, so `xdg-open` stands in for it.
pub fn open_external(url: &str) {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return;
    }
    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
