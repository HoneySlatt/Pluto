//! Context menu (`contextMenuFallback.ts`, the DOM menu upstream shows when
//! no native desktop menu is available): nested submenus that open on hover,
//! a 24rem height cap with a scrollbar, and positions clamped to the window.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const ROW: f32 = 28.0; // sm:min-h-7
const SEPARATOR: f32 = 9.0; // my-1 around 1px
const PADDING: f32 = 4.0; // p-1
const MIN_WIDTH: f32 = 128.0; // min-w-32
const MAX_WIDTH: f32 = 384.0; // max-w-sm
const MAX_HEIGHT: f32 = 384.0; // max-h-[min(24rem,70vh)]
const SCROLLBAR: f32 = 6.0; // --app-scrollbar-width
const SURFACE: Color32 = Color32::from_rgb(16, 16, 16); // dropdown glass over the app
const OUTLINE: Color32 = Color32::from_rgba_premultiplied(25, 25, 25, 26); // white 10%
const THUMB: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20); // #ffffff14
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74); // --destructive

/// One entry of a `ContextMenuItem` tree.
#[derive(Clone, Default)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub icon: Option<&'static str>,
    pub destructive: bool,
    pub disabled: bool,
    /// Option rows: a check (or an empty slot) takes the icon's place.
    pub checked: Option<bool>,
    pub separator_before: bool,
    /// Radio value (`data-checked`): a faint surface instead of a check.
    pub selected: bool,
    /// Right-aligned muted text (`ml-auto text-xs`), e.g. a turn's time.
    pub detail: Option<String>,
    pub children: Vec<Item>,
}

impl Item {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ..Default::default()
        }
    }

    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn separated(mut self) -> Self {
        self.separator_before = true;
        self
    }
}

/// An open menu: where it was requested, the submenu opened at each level,
/// the focused row and each level's scroll offset.
pub struct State {
    pub at: Pos2,
    path: Vec<usize>,
    focus: Option<(usize, usize)>,
    scroll: Vec<f32>,
    pointer_level: Option<usize>,
    min_width: f32,
    /// Base UI submenus open flush against their trigger row, raised so
    /// their first item lines up with it; the DOM fallback leaves a 4px gap.
    dropdown: bool,
}

impl State {
    pub fn new(at: Pos2) -> Self {
        Self {
            at,
            path: Vec::new(),
            focus: None,
            scroll: vec![0.0],
            pointer_level: None,
            min_width: MIN_WIDTH,
            dropdown: false,
        }
    }

    /// A dropdown (`MenuPopup`, `min-w-[10rem]`) opened below a trigger.
    pub fn dropdown(at: Pos2) -> Self {
        Self {
            min_width: 160.0,
            dropdown: true,
            ..Self::new(at)
        }
    }
}

struct Level<'a> {
    items: &'a [Item],
    rect: Rect,
    content_height: f32,
}

fn content_height(items: &[Item]) -> f32 {
    let separators = items
        .iter()
        .enumerate()
        .filter(|(i, item)| item.separator_before && *i > 0)
        .count();
    PADDING * 2.0 + items.len() as f32 * ROW + separators as f32 * SEPARATOR
}

/// `clampMenuPosition`: keeps the menu 4px inside the window.
fn clamp(screen: Rect, at: Pos2, size: Vec2) -> Pos2 {
    Pos2::new(
        at.x.max(4.0).min((screen.width() - size.x - 4.0).max(4.0)),
        at.y.max(4.0).min((screen.height() - size.y - 4.0).max(4.0)),
    )
}

fn menu_size(ui: &Ui, items: &[Item], screen: Rect, min_width: f32) -> (Vec2, f32) {
    let label_font = font(14.0, Weight::Regular);
    let detail_font = font(12.0, Weight::Regular);
    let row_content = items
        .iter()
        .map(|item| {
            let slot = if item.icon.is_some() || item.checked.is_some() {
                24.0
            } else {
                0.0
            };
            let chevron = if item.children.is_empty() { 0.0 } else { 22.0 };
            let detail = item.detail.as_ref().map_or(0.0, |d| {
                8.0 + widgets::text_width(ui, d, detail_font.clone())
            });
            slot + widgets::text_width(ui, &item.label, label_font.clone()) + chevron + detail
        })
        .fold(0.0_f32, f32::max);
    let content = content_height(items);
    let height = content.min(MAX_HEIGHT.min(screen.height() * 0.7));
    let scrollbar = if content > height { SCROLLBAR } else { 0.0 };
    // Fractional like Chrome's layout; `show` snaps the painted edges.
    let width = (row_content + 16.0 + PADDING * 2.0 + scrollbar + 2.0).clamp(min_width, MAX_WIDTH);
    (Vec2::new(width, height + 2.0), content)
}

/// Row rectangles of a level in screen space (before scrolling).
fn rows(level: &Level, scroll: f32) -> Vec<(usize, Rect)> {
    let mut y = level.rect.top() + 1.0 + PADDING - scroll;
    let scrollbar = if level.content_height + 2.0 > level.rect.height() + 0.5 {
        SCROLLBAR
    } else {
        0.0
    };
    let mut out = Vec::new();
    for (index, item) in level.items.iter().enumerate() {
        if item.separator_before && index > 0 {
            y += SEPARATOR;
        }
        out.push((
            index,
            Rect::from_min_size(
                Pos2::new(level.rect.left() + 1.0 + PADDING, y),
                Vec2::new(level.rect.width() - 2.0 - PADDING * 2.0 - scrollbar, ROW),
            ),
        ));
        y += ROW;
    }
    out
}

/// Shows the menu while `state` is open; returns the id of a clicked leaf
/// and closes the menu on a click, Escape or a press outside it.
pub fn show(ui: &mut Ui, open: &mut Option<State>, items: &[Item]) -> Option<String> {
    let state = open.as_mut()?;
    let screen = ui.ctx().content_rect();

    // Lay out the open levels: the root at the pointer, each submenu next to
    // its trigger row (`rect.right + 4`, flipped left when it overflows).
    let mut levels: Vec<Level> = Vec::new();
    let (size, content) = menu_size(ui, items, screen, state.min_width);
    levels.push(Level {
        items,
        rect: Rect::from_min_size(clamp(screen, state.at, size), size),
        content_height: content,
    });
    state.scroll.resize(state.path.len() + 1, 0.0);
    for (depth, &index) in state.path.iter().enumerate() {
        let parent = &levels[depth];
        let Some(item) = parent.items.get(index) else {
            break;
        };
        if item.children.is_empty() {
            break;
        }
        let Some((_, trigger)) = rows(parent, state.scroll[depth])
            .into_iter()
            .find(|(i, _)| *i == index)
        else {
            break;
        };
        let (size, content) = menu_size(ui, &item.children, screen, state.min_width);
        let (gap, rise) = if state.dropdown {
            (0.0, 1.0 + PADDING)
        } else {
            (4.0, 0.0)
        };
        let mut at = Pos2::new(trigger.right() + gap, trigger.top() - rise);
        if at.x + size.x > screen.right() {
            at.x = trigger.left() - size.x - gap;
        }
        levels.push(Level {
            items: &item.children,
            rect: Rect::from_min_size(clamp(screen, at, size), size),
            content_height: content,
        });
    }
    state.path.truncate(levels.len() - 1);

    // Pointer: the deepest level under it drives hover, focus and submenus.
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let level_under = pointer.and_then(|p| levels.iter().rposition(|l| l.rect.contains(p)));
    let entered = level_under.is_some() && level_under != state.pointer_level;
    state.pointer_level = level_under;
    let mut hovered: Option<(usize, usize)> = None;
    if let (Some(depth), Some(p)) = (level_under, pointer) {
        let level = &levels[depth];
        hovered = rows(level, state.scroll[depth])
            .into_iter()
            .find(|(i, r)| r.contains(p) && !level.items[*i].disabled)
            .map(|(i, _)| (depth, i));
        match hovered {
            Some((depth, index)) => {
                state.focus = Some((depth, index));
                state.path.truncate(depth);
                if !levels[depth].items[index].children.is_empty() {
                    state.path.push(index);
                }
            }
            // Entering a menu closes its submenus (`menu.mouseenter`).
            None if entered => state.path.truncate(depth),
            None => {}
        }
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            let max = (level.content_height + 2.0 - level.rect.height()).max(0.0);
            state.scroll[depth] = (state.scroll[depth] - wheel).clamp(0.0, max);
        }
    }
    if state
        .focus
        .is_some_and(|(depth, _)| depth > state.path.len())
    {
        state.focus = None;
    }

    let mut clicked: Option<String> = None;
    let mut close = ui.input(|i| i.key_pressed(Key::Escape));
    let pressed = ui.input(|i| i.pointer.primary_pressed() || i.pointer.secondary_pressed());
    if pressed && level_under.is_none() {
        close = true;
    }
    let released = ui.input(|i| i.pointer.primary_released());

    for (depth, level) in levels.iter().enumerate().take(state.path.len() + 1) {
        let scroll = state.scroll[depth];
        let focus = state.focus;
        egui::Area::new(ui.id().with(("context-menu", depth)))
            .order(egui::Order::Foreground)
            .fixed_pos(level.rect.min.round())
            .interactable(true)
            .show(ui.ctx(), |ui| {
                // Chrome snaps each box edge to the nearest pixel.
                let snapped = Rect::from_min_max(level.rect.min.round(), level.rect.max.round());
                let (rect, _) = ui.allocate_exact_size(snapped.size(), Sense::click());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(10),
                    SURFACE,
                    Stroke::new(1.0, OUTLINE),
                    StrokeKind::Inside,
                );
                let viewport = rect.shrink(1.0);
                ui.set_clip_rect(viewport);
                let painter = ui.painter().clone();
                let level_rows = rows(level, scroll);
                for (index, row) in &level_rows {
                    let item = &level.items[*index];
                    if item.separator_before && *index > 0 {
                        painter.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(row.left() + 8.0, row.top() - 5.0),
                                Vec2::new(row.width() - 16.0, 1.0),
                            ),
                            0.0,
                            theme::BORDER,
                        );
                    }
                    let destructive =
                        item.children.is_empty() && (item.destructive || item.id == "delete");
                    let highlighted = !item.disabled
                        && (focus == Some((depth, *index)) || hovered == Some((depth, *index)));
                    if item.selected && !highlighted {
                        painter.rect_filled(
                            *row,
                            CornerRadius::same(6),
                            theme::alpha(theme::FOREGROUND, 0.08),
                        );
                    }
                    if highlighted {
                        painter.rect_filled(
                            *row,
                            CornerRadius::same(6),
                            if destructive {
                                theme::alpha(DESTRUCTIVE, 0.1)
                            } else {
                                theme::ACCENT
                            },
                        );
                    }
                    let (text_color, icon_color) = if item.disabled {
                        let muted = theme::alpha(theme::MUTED_FOREGROUND, 0.64);
                        (muted, muted)
                    } else if destructive {
                        (theme::ERROR_FOREGROUND, theme::ERROR_FOREGROUND)
                    } else {
                        (theme::FOREGROUND, theme::MUTED_FOREGROUND)
                    };
                    let mut x = row.left() + 8.0;
                    let icon = match item.checked {
                        Some(true) => Some("check"),
                        Some(false) => None,
                        None => item.icon,
                    };
                    if let Some(icon) = icon {
                        let icon_rect = Rect::from_min_size(
                            Pos2::new(x, row.center().y - 8.0),
                            Vec2::splat(16.0),
                        );
                        theme::paint_icon(ui, icon, icon_rect, 16.0, icon_color);
                    }
                    if item.icon.is_some() || item.checked.is_some() {
                        x += 24.0;
                    }
                    let chevron = if item.children.is_empty() { 0.0 } else { 22.0 };
                    let mut label_right = row.right() - 8.0 - chevron;
                    if let Some(detail) = &item.detail {
                        let detail_font = font(12.0, Weight::Regular);
                        let width = widgets::text_width(ui, detail, detail_font.clone());
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(
                                Pos2::new(label_right - width, row.top()),
                                Pos2::new(label_right, row.bottom()),
                            ),
                            detail,
                            detail_font,
                            theme::MUTED_FOREGROUND,
                        );
                        label_right -= width + 8.0;
                    }
                    let galley = widgets::galley_truncated(
                        ui,
                        &item.label,
                        font(14.0, Weight::Regular),
                        text_color,
                        20.0,
                        label_right - x,
                    );
                    painter.galley(Pos2::new(x, row.center().y - 10.0), galley, text_color);
                    if !item.children.is_empty() {
                        let chevron_rect = Rect::from_min_size(
                            Pos2::new(row.right() - 6.0 - 16.0, row.center().y - 8.0),
                            Vec2::splat(16.0),
                        );
                        theme::paint_icon(
                            ui,
                            "chevron-right",
                            chevron_rect,
                            16.0,
                            theme::alpha(theme::MUTED_FOREGROUND, 0.8),
                        );
                    }
                    if released
                        && hovered == Some((depth, *index))
                        && item.children.is_empty()
                        && !item.disabled
                    {
                        clicked = Some(item.id.clone());
                    }
                }
                // Classic scrollbar: a 6px rounded thumb, no track.
                if level.content_height > viewport.height() + 0.5 {
                    let ratio = viewport.height() / level.content_height;
                    let thumb_height = viewport.height() * ratio;
                    let travel = viewport.height() - thumb_height;
                    let max_scroll = level.content_height - viewport.height();
                    let top = viewport.top() + travel * (scroll / max_scroll);
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(viewport.right() - SCROLLBAR, top),
                            Vec2::new(SCROLLBAR, thumb_height),
                        ),
                        CornerRadius::same(3),
                        THUMB,
                    );
                }
            });
    }

    if clicked.is_some() || close {
        *open = None;
    }
    clicked
}
