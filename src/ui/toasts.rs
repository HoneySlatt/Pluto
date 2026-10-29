//! Toasts (`components/ui/toast.tsx`, `toast.logic.ts`): cards at the
//! window's top right, under the top bar, with the type's icon (a spinner
//! while loading), a title, an optional description and buttons, and the
//! corner dismiss orb. An error's description can be copied ("Copy error")
//! and is clamped to four lines past 180 characters; details open under
//! "Show details"; a toast tied to a thread shows only while that thread is
//! open. The newest is in front; up to two older ones peek below it, and
//! hovering the stack spreads it out. A toast closes after its timeout (5s
//! by default, paused while hovered or while the window is in the
//! background), loading ones never. Plain status messages become toasts
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
/// The open details' height cap (`max-h-40`).
const DETAILS_MAX: f32 = 160.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Plain,
    Info,
    Success,
    Warning,
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
    /// Opens a file in the preferred editor (`shell.openInEditor`).
    OpenInEditor(String),
    /// The permission notice answered: Open Connections (`true`) or Dismiss.
    PermissionNotice(bool),
    /// A project clone's Cancel, Retry or Remove project.
    Clone {
        project: String,
        action: &'static str,
    },
    /// Opens a new thread in a project (a clone's "Open project").
    OpenProject(String),
    /// A linked prompt replaces the composer's draft.
    ReplaceDraft(String),
    /// Closes the toast and nothing else.
    Dismiss,
}

#[derive(Clone, Debug)]
pub(crate) struct Toast {
    pub kind: Kind,
    pub title: String,
    pub description: Option<String>,
    /// The primary button (`actionProps`).
    pub action: Option<(String, Action)>,
    /// Outline buttons before it (`additionalActions`, `secondaryActionProps`).
    pub secondary: Vec<(String, Action)>,
    /// Text under "Show details" (`expandableContent`), and the toggle's
    /// labels to open and close it (`expandableLabels`).
    pub details: Option<String>,
    pub details_labels: (&'static str, &'static str),
    /// Shown only while this thread is open (`threadRef`).
    pub thread: Option<String>,
    /// No "Copy error" button (`hideCopyButton`).
    pub hide_copy: bool,
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
            secondary: Vec::new(),
            details: None,
            details_labels: ("Show details", "Hide details"),
            thread: None,
            hide_copy: false,
            timeout: (kind != Kind::Loading).then_some(DEFAULT_TIMEOUT),
        }
    }

    pub(crate) fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into()).filter(|d: &String| !d.is_empty());
        self
    }

    /// Only an error with a description offers to copy it.
    fn copyable(&self) -> bool {
        self.kind == Kind::Error && self.description.is_some() && !self.hide_copy
    }

    /// Buttons go below the text rather than beside it (`stacked-end`).
    fn stacked(&self) -> bool {
        !self.secondary.is_empty() || self.details.is_some()
    }

    /// The buttons, left to right: the outline ones, then the primary.
    fn buttons(&self) -> impl Iterator<Item = (&String, &Action, bool)> {
        self.secondary
            .iter()
            .map(|(label, action)| (label, action, false))
            .chain(
                self.action
                    .iter()
                    .map(|(label, action)| (label, action, true)),
            )
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
        self.notice(Kind::Plain, message);
    }

    /// A message of `kind`, once: repeating the newest one restarts its
    /// timer.
    pub(crate) fn notice(&mut self, kind: Kind, message: String) {
        if let Some((_, last)) = self.list.last_mut()
            && last.kind == kind
            && last.title == message
        {
            last.timeout = Some(DEFAULT_TIMEOUT);
            return;
        }
        self.add(Toast::new(kind, message));
    }

    /// Draws the stack in `area`, with the toasts of `thread` (the open
    /// one) and those of no thread; returns the action clicked.
    pub(crate) fn show(&mut self, ui: &mut Ui, area: Rect, thread: Option<&str>) -> Option<Action> {
        let now = ui.input(|i| i.time);
        let elapsed = self.last_time.map_or(0.0, |last| (now - last).max(0.0));
        self.last_time = Some(now);
        let right = area.right() - INSET;
        let top = INSET + TOPBAR;
        // Hovering the stack spreads it out (`data-expanded`).
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let expanded_id = egui::Id::new("toasts-expanded");
        let was_expanded = ui.data(|d| d.get_temp::<bool>(expanded_id).unwrap_or(false));
        let shown_heights = |list: &[(u64, Toast)]| -> Vec<(usize, f32)> {
            (0..list.len())
                .filter(|&i| {
                    list[i]
                        .1
                        .thread
                        .as_deref()
                        .is_none_or(|t| Some(t) == thread)
                })
                .map(|i| (i, card_height(ui, &list[i].1, list[i].0)))
                .collect()
        };
        let shown = shown_heights(&self.list);
        let front_height = shown.last().map_or(0.0, |(_, h)| *h);
        let stack_rect = Rect::from_min_size(
            Pos2::new(right - WIDTH, top - 8.0),
            Vec2::new(WIDTH + 8.0, front_height + 2.0 * PEEK + 16.0),
        );
        let expanded_rect = Rect::from_min_size(
            Pos2::new(right - WIDTH, top - 8.0),
            Vec2::new(
                WIDTH + 8.0,
                shown
                    .iter()
                    .rev()
                    .take(3)
                    .map(|(_, h)| h + GAP)
                    .sum::<f32>()
                    + 8.0,
            ),
        );
        let expanded = !shown.is_empty()
            && pointer.is_some_and(|p| {
                if was_expanded {
                    expanded_rect.contains(p)
                } else {
                    stack_rect.contains(p)
                }
            });
        ui.data_mut(|d| d.insert_temp(expanded_id, expanded));
        // Timers wait while the stack is hovered or the window is away.
        let focused = ui.input(|i| i.focused);
        if !expanded && focused {
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
        let shown = shown_heights(&self.list);
        if shown.is_empty() {
            return None;
        }
        let front_height = shown.last().map_or(0.0, |(_, h)| *h);

        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("toasts"),
        ));
        let mut clicked = None;
        let mut dismissed = None;
        // Back to front, so the newest paints last.
        let mut offsets = Vec::new();
        let mut y = top;
        for (depth, &(index, height)) in shown.iter().rev().take(3).enumerate() {
            if expanded {
                offsets.push((index, depth, y, 1.0, height));
                y += height + GAP;
            } else {
                let scale = 1.0 - depth as f32 * 0.1;
                offsets.push((index, depth, top + depth as f32 * PEEK, scale, height));
            }
        }
        for &(index, depth, card_top, scale, own_height) in offsets.iter().rev() {
            let (id, toast) = &self.list[index];
            let front = depth == 0 || expanded;
            let height = if front { own_height } else { front_height };
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
            let response = child
                .interact(orb, egui::Id::new(("toast-close", *id)), Sense::click())
                .on_hover_text("Dismiss notification");
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
        clicked.and_then(|(id, action)| {
            self.close(id);
            (action != Action::Dismiss).then_some(action)
        })
    }
}

fn icon_of(kind: Kind) -> Option<(&'static str, Color32)> {
    match kind {
        Kind::Plain | Kind::Loading => None,
        Kind::Info => Some(("info", theme::BLUE_500)),
        Kind::Success => Some(("circle-check", theme::EMERALD_500)),
        Kind::Warning => Some(("triangle-alert", theme::warning())),
        Kind::Error => Some(("circle-alert", Color32::from_rgb(251, 44, 54))),
    }
}

fn has_icon(kind: Kind) -> bool {
    kind == Kind::Loading || icon_of(kind).is_some()
}

fn button_width(ui: &Ui, label: &str) -> f32 {
    widgets::text_width(ui, label, font(12.0, Weight::Medium)) + 14.0
}

/// The buttons' row width (`gap-1.5`), the copy button first.
fn controls_width(ui: &Ui, toast: &Toast) -> f32 {
    let copy = if toast.copyable() { 20.0 + 6.0 } else { 0.0 };
    copy + toast
        .buttons()
        .map(|(label, _, _)| button_width(ui, label) + 6.0)
        .sum::<f32>()
}

/// The text column's width: `pl-3.5`, and beside the buttons `pr-6`
/// (`pr-10` without any), or `pr-3.5` and the stacked text's `pr-5`.
fn column_width(ui: &Ui, toast: &Toast) -> f32 {
    if toast.stacked() {
        return WIDTH - 14.0 - 14.0 - 20.0;
    }
    let controls = controls_width(ui, toast);
    let right = if controls > 0.0 { 24.0 } else { 40.0 };
    WIDTH - 14.0 - right - controls
}

/// The description, clamped to four lines for a long error
/// (`line-clamp-4` past 180 characters).
fn description_galley(
    ui: &Ui,
    toast: &Toast,
    text: &str,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        widgets::format(font(14.0, Weight::Regular), theme::muted_foreground(), 20.0),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: width,
        ..Default::default()
    };
    if toast.kind == Kind::Error && text.chars().count() >= 180 {
        job.wrap.max_rows = 4;
        job.wrap.overflow_character = Some('…');
    }
    ui.fonts_mut(|f| f.layout_job(job))
}

fn details_open(ui: &Ui, id: u64) -> bool {
    ui.data(|d| d.get_temp::<bool>(egui::Id::new(("toast-details", id))))
        .unwrap_or(false)
}

fn details_galley(ui: &Ui, text: &str, width: f32) -> std::sync::Arc<egui::Galley> {
    widgets::galley(
        ui,
        text,
        font(12.0, Weight::Regular),
        theme::muted_foreground(),
        16.0,
        width,
    )
}

/// Title, description, details toggle (and open details), buttons row.
fn card_height(ui: &Ui, toast: &Toast, id: u64) -> f32 {
    let width = column_width(ui, toast);
    let icon = if has_icon(toast.kind) { 24.0 } else { 0.0 };
    let title = widgets::galley(
        ui,
        &toast.title,
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
        width - icon,
    );
    let description = toast.description.as_ref().map_or(0.0, |d| {
        2.0 + description_galley(ui, toast, d, width).size().y
    });
    let details = toast.details.as_ref().map_or(0.0, |text| {
        let toggle = 2.0 + 20.0;
        if details_open(ui, id) {
            toggle + 8.0 + details_galley(ui, text, width).size().y.min(DETAILS_MAX)
        } else {
            toggle
        }
    });
    let text = title.size().y + description + details;
    if toast.stacked() {
        10.0 + text + 8.0 + 24.0 + 10.0
    } else {
        24.0 + text
    }
}

/// The card's icon, title, description, details and buttons; returns a
/// button's action.
fn paint_card(ui: &mut Ui, rect: Rect, toast: &Toast, id: u64) -> Option<Action> {
    let width = column_width(ui, toast);
    let left = rect.left() + 14.0;
    let top = rect.top() + if toast.stacked() { 10.0 } else { 12.0 };
    let mut x = left;
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
        width - (x - left),
    );
    let mut y = top + title.size().y;
    ui.painter()
        .galley(Pos2::new(x, top), title, theme::foreground());
    // The description starts at the column's left, under the icon.
    if let Some(description) = &toast.description {
        let galley = description_galley(ui, toast, description, width);
        let height = galley.size().y;
        widgets::selectable_galley(
            ui,
            egui::Id::new(("toast-description", id)),
            Pos2::new(left, y + 2.0),
            galley,
            theme::muted_foreground(),
        );
        y += 2.0 + height;
    }
    if let Some(details) = &toast.details {
        let open = details_open(ui, id);
        let label = if open {
            toast.details_labels.1
        } else {
            toast.details_labels.0
        };
        let label_width = widgets::text_width(ui, label, font(12.0, Weight::Medium));
        let toggle = Rect::from_min_size(
            Pos2::new(left, y + 2.0),
            Vec2::new(label_width + 18.0, 20.0),
        );
        let response = ui.interact(
            toggle,
            egui::Id::new(("toast-details-toggle", id)),
            Sense::click(),
        );
        let color = if response.hovered() {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        theme::paint_icon(
            ui,
            if open { "chevron-up" } else { "chevron-down" },
            Rect::from_min_size(Pos2::new(left, y + 5.0), Vec2::splat(14.0)),
            14.0,
            theme::alpha(color, 0.8),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(left + 18.0, toggle.top()), toggle.max),
            label,
            font(12.0, Weight::Medium),
            color,
        );
        if response.clicked() {
            ui.data_mut(|d| d.insert_temp(egui::Id::new(("toast-details", id)), !open));
        }
        y += 22.0;
        if open {
            let galley = details_galley(ui, details, width);
            let height = galley.size().y.min(DETAILS_MAX);
            let area = Rect::from_min_size(Pos2::new(left, y + 8.0), Vec2::new(width, height));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            egui::ScrollArea::vertical()
                .id_salt(("toast-details-scroll", id))
                .max_height(DETAILS_MAX)
                .show(&mut child, |ui| {
                    ui.add(egui::Label::new(galley).selectable(true));
                });
        }
    }
    // The buttons: beside the text, centred, or along the bottom.
    let row_top = if toast.stacked() {
        rect.bottom() - 10.0 - 24.0
    } else {
        rect.center().y - 12.0
    };
    let mut right = rect.right() - 14.0;
    let mut clicked = None;
    let buttons: Vec<_> = toast.buttons().collect();
    for (label, action, primary) in buttons.into_iter().rev() {
        let button_width = button_width(ui, label);
        let button = Rect::from_min_size(
            Pos2::new(right - button_width, row_top),
            Vec2::new(button_width, 24.0),
        );
        let response = ui.interact(
            button,
            egui::Id::new(("toast-action", id, label.as_str())),
            Sense::click(),
        );
        if primary {
            ui.painter().rect_filled(
                button,
                CornerRadius::same(6),
                if response.hovered() {
                    theme::primary_hover()
                } else {
                    theme::primary()
                },
            );
        } else {
            ui.painter().rect(
                button,
                CornerRadius::same(6),
                theme::alpha(theme::input(), if response.hovered() { 0.64 } else { 0.32 }),
                Stroke::new(1.0, theme::input()),
                egui::StrokeKind::Inside,
            );
        }
        widgets::text_in(
            ui,
            Rect::from_center_size(button.center(), Vec2::new(button_width - 13.0, 24.0)),
            label,
            font(12.0, Weight::Medium),
            if primary {
                theme::primary_foreground()
            } else {
                theme::foreground()
            },
        );
        if response.clicked() {
            clicked = Some(action.clone());
        }
        right = button.left() - 6.0;
    }
    if toast.copyable() {
        let description = toast.description.clone().unwrap_or_default();
        widgets::copy_button(
            ui,
            Rect::from_min_size(Pos2::new(right - 20.0, row_top + 2.0), Vec2::splat(20.0)),
            ("toast-copy", id),
            "Copy error",
            "Copied error",
            theme::EMERALD_500,
            2.0,
            false,
            move || description,
        );
    }
    clicked
}
