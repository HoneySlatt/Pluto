//! Toasts (`components/ui/toast.tsx`): cards at the window's top right, under
//! the top bar, with the type's icon (a spinner while loading), a title, an
//! optional description and action, and the corner dismiss orb. The newest
//! is in front; up to two older ones peek below it, and hovering the stack
//! spreads it out. A toast closes after its timeout (5s by default, paused
//! while hovered), loading ones never. Plain status messages become toasts
//! without an icon.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const WIDTH: f32 = 360.0; // max-w-90
const INSET: f32 = 32.0; // sm:--toast-inset
const TOPBAR: f32 = 52.0; // --workspace-topbar-height
const GAP: f32 = 12.0; // --toast-gap
const PEEK: f32 = 12.0; // --toast-peek
const DEFAULT_TIMEOUT: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Plain,
    Info,
    Success,
    Error,
    Loading,
}

/// What a toast's button does.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::enum_variant_names)] // `GitAction` names what it runs.
pub(crate) enum Action {
    /// Runs a stacked git action (`GitRunStackedActionToastCta.run_action`).
    GitAction(String),
    OpenUrl(String),
    /// Opens a thread (`ThreadNotificationCoordinator`'s "Open thread").
    OpenThread(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Toast {
    pub kind: Kind,
    pub title: String,
    pub description: Option<String>,
    pub action: Option<(String, Action)>,
    /// Seconds left before it closes (None: until dismissed).
    pub timeout: Option<f64>,
}

impl Toast {
    pub(crate) fn new(kind: Kind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            description: None,
            action: None,
            timeout: (kind != Kind::Loading).then_some(DEFAULT_TIMEOUT),
        }
    }

    pub(crate) fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into()).filter(|d: &String| !d.is_empty());
        self
    }
}

#[derive(Default)]
pub(crate) struct Toasts {
    /// Oldest first.
    list: Vec<(u64, Toast)>,
    next_id: u64,
    /// The time of the last pass, to count timeouts down.
    last_time: Option<f64>,
}

impl Toasts {
    pub(crate) fn add(&mut self, toast: Toast) -> u64 {
        self.next_id += 1;
        self.list.push((self.next_id, toast));
        self.next_id
    }

    /// Replaces a toast in place (a loading toast settling); adds it back
    /// if it was dismissed meanwhile.
    pub(crate) fn update(&mut self, id: u64, toast: Toast) {
        match self.list.iter_mut().find(|(other, _)| *other == id) {
            Some((_, existing)) => *existing = toast,
            None => self.list.push((id, toast)),
        }
    }

    pub(crate) fn close(&mut self, id: u64) {
        self.list.retain(|(other, _)| *other != id);
    }

    /// A status message, once: repeating the newest one restarts its timer.
    pub(crate) fn status(&mut self, message: String) {
        if let Some((_, last)) = self.list.last_mut()
            && last.kind == Kind::Plain
            && last.title == message
        {
            last.timeout = Some(DEFAULT_TIMEOUT);
            return;
        }
        self.add(Toast::new(Kind::Plain, message));
    }

    /// Draws the stack in `area`; returns the action clicked.
    pub(crate) fn show(&mut self, ui: &mut Ui, area: Rect) -> Option<Action> {
        let now = ui.input(|i| i.time);
        let elapsed = self.last_time.map_or(0.0, |last| (now - last).max(0.0));
        self.last_time = Some(now);
        if self.list.is_empty() {
            return None;
        }
        let right = area.right() - INSET;
        let top = INSET + TOPBAR;
        let heights: Vec<f32> = self
            .list
            .iter()
            .map(|(_, toast)| card_height(ui, toast))
            .collect();
        // Hovering the stack spreads it out (`data-expanded`).
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let front_height = *heights.last().unwrap_or(&0.0);
        let stack_rect = Rect::from_min_size(
            Pos2::new(right - WIDTH, top - 8.0),
            Vec2::new(WIDTH + 8.0, front_height + 2.0 * PEEK + 16.0),
        );
        let expanded_rect = Rect::from_min_size(
            Pos2::new(right - WIDTH, top - 8.0),
            Vec2::new(
                WIDTH + 8.0,
                heights.iter().rev().take(3).map(|h| h + GAP).sum::<f32>() + 8.0,
            ),
        );
        let expanded = pointer.is_some_and(|p| {
            let id = egui::Id::new("toasts-expanded");
            let was = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
            let now = if was {
                expanded_rect.contains(p)
            } else {
                stack_rect.contains(p)
            };
            ui.data_mut(|d| d.insert_temp(id, now));
            now
        });
        if !expanded {
            for (_, toast) in &mut self.list {
                if let Some(left) = &mut toast.timeout {
                    *left -= elapsed;
                }
            }
        }
        self.list
            .retain(|(_, toast)| toast.timeout.is_none_or(|left| left > 0.0));
        if self.list.iter().any(|(_, t)| t.timeout.is_some()) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("toasts"),
        ));
        let mut clicked = None;
        let mut dismissed = None;
        let count = self.list.len();
        let visible: Vec<usize> = (0..count).rev().take(3).collect();
        // Back to front, so the newest paints last.
        let mut offsets = Vec::new();
        let mut y = top;
        for (depth, &index) in visible.iter().enumerate() {
            let height = card_height(ui, &self.list[index].1);
            if expanded {
                offsets.push((index, depth, y, 1.0));
                y += height + GAP;
            } else {
                let scale = 1.0 - depth as f32 * 0.1;
                offsets.push((index, depth, top + depth as f32 * PEEK, scale));
            }
        }
        for &(index, depth, card_top, scale) in offsets.iter().rev() {
            let (id, toast) = &self.list[index];
            let front = depth == 0 || expanded;
            let height = if front {
                card_height(ui, toast)
            } else {
                front_height
            };
            let width = WIDTH * scale;
            let rect = Rect::from_min_size(
                Pos2::new(right - WIDTH + (WIDTH - width) / 2.0, card_top),
                Vec2::new(width, height),
            );
            painter.add(
                egui::epaint::Shadow {
                    offset: [0, 10],
                    blur: 25,
                    spread: 0,
                    color: Color32::from_black_alpha(60),
                }
                .as_shape(rect, CornerRadius::same(8)),
            );
            painter.rect(
                rect,
                CornerRadius::same(8),
                theme::popover(),
                Stroke::new(1.0, theme::overlay(0.1)),
                egui::StrokeKind::Inside,
            );
            if !front {
                continue;
            }
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .layer_id(painter.layer_id())
                    .max_rect(rect),
            );
            if let Some(action) = paint_card(&mut child, rect, toast, *id) {
                clicked = Some((*id, action));
            }
            // The corner dismiss orb (`-top-1.5 -right-1.5`).
            let orb = Rect::from_center_size(
                Pos2::new(rect.right() - 6.0, rect.top() + 6.0),
                Vec2::splat(24.0),
            );
            let response = child.interact(orb, egui::Id::new(("toast-close", *id)), Sense::click());
            painter.circle(
                orb.center(),
                12.0,
                if response.hovered() {
                    theme::popover()
                } else {
                    theme::alpha(theme::popover(), 0.92)
                },
                Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
            );
            theme::paint_icon(
                &child,
                "x",
                Rect::from_center_size(orb.center(), Vec2::splat(12.0)),
                12.0,
                if response.hovered() {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
            );
            if response.clicked() {
                dismissed = Some(*id);
            }
        }
        if let Some(id) = dismissed {
            self.close(id);
        }
        clicked.map(|(id, action)| {
            self.close(id);
            action
        })
    }
}

fn icon_of(kind: Kind) -> Option<(&'static str, Color32)> {
    match kind {
        Kind::Plain | Kind::Loading => None,
        Kind::Info => Some(("info", theme::BLUE_500)),
        Kind::Success => Some(("circle-check", Color32::from_rgb(0, 188, 125))),
        Kind::Error => Some(("circle-alert", Color32::from_rgb(251, 44, 54))),
    }
}

fn has_icon(kind: Kind) -> bool {
    kind == Kind::Loading || icon_of(kind).is_some()
}

/// The text column's width: padding (`pl-3.5 pr-3.5`), the action's room.
fn text_width(ui: &Ui, toast: &Toast) -> f32 {
    let action = toast.action.as_ref().map_or(0.0, |(label, _)| {
        widgets::text_width(ui, label, font(12.0, Weight::Medium)) + 16.0 + 6.0
    });
    let icon = if has_icon(toast.kind) { 24.0 } else { 0.0 };
    WIDTH - 28.0 - action - icon
}

fn card_height(ui: &Ui, toast: &Toast) -> f32 {
    let width = text_width(ui, toast);
    let title = widgets::galley(
        ui,
        &toast.title,
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
        width,
    );
    let description = toast.description.as_ref().map_or(0.0, |d| {
        2.0 + widgets::galley(
            ui,
            d,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
            width + if has_icon(toast.kind) { 24.0 } else { 0.0 },
        )
        .size()
        .y
    });
    24.0 + title.size().y + description
}

/// The card's icon, title, description and action; returns a click on the
/// action.
fn paint_card(ui: &mut Ui, rect: Rect, toast: &Toast, id: u64) -> Option<Action> {
    let width = text_width(ui, toast);
    let mut x = rect.left() + 14.0;
    let top = rect.top() + 12.0;
    if toast.kind == Kind::Loading {
        crate::ui::content_search::paint_spinner(ui, Pos2::new(x + 8.0, top + 10.0), 16.0);
        x += 24.0;
    } else if let Some((icon, color)) = icon_of(toast.kind) {
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(Pos2::new(x, top + 2.0), Vec2::splat(16.0)),
            16.0,
            color,
        );
        x += 24.0;
    }
    let title = widgets::galley(
        ui,
        &toast.title,
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
        width,
    );
    let title_height = title.size().y;
    ui.painter()
        .galley(Pos2::new(x, top), title, theme::foreground());
    if let Some(description) = &toast.description {
        let galley = widgets::galley(
            ui,
            description,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
            width + if has_icon(toast.kind) { 24.0 } else { 0.0 },
        );
        widgets::selectable_galley(
            ui,
            egui::Id::new(("toast-description", id)),
            Pos2::new(
                rect.left() + 14.0 + if has_icon(toast.kind) { 24.0 } else { 0.0 },
                top + title_height + 2.0,
            ),
            galley,
            theme::muted_foreground(),
        );
    }
    let (label, action) = toast.action.as_ref()?;
    let label_width = widgets::text_width(ui, label, font(12.0, Weight::Medium));
    let button = Rect::from_min_size(
        Pos2::new(
            rect.right() - 14.0 - label_width - 16.0,
            rect.center().y - 12.0,
        ),
        Vec2::new(label_width + 16.0, 24.0),
    );
    let response = ui.interact(button, egui::Id::new(("toast-action", id)), Sense::click());
    ui.painter().rect_filled(
        button,
        CornerRadius::same(6),
        if response.hovered() {
            theme::primary_hover()
        } else {
            theme::primary()
        },
    );
    widgets::text_in(
        ui,
        Rect::from_center_size(button.center(), Vec2::new(label_width + 1.0, 24.0)),
        label,
        font(12.0, Weight::Medium),
        theme::primary_foreground(),
    );
    response.clicked().then(|| action.clone())
}
