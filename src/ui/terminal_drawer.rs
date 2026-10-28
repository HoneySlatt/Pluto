//! The bottom terminal drawer (`ThreadTerminalDrawer.tsx`): terminal groups
//! under the chat, toggled by `terminal.toggle` (Ctrl+J). One session shows
//! alone with the floating toolbar; several add the groups sidebar (Single,
//! Side by side, Stacked) with its split, new and close buttons.
//!
//! Not ported: persisting the groups across restarts, renaming and the
//! close confirmation.
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::terminal_surface::{next_terminal_id, terminal_label, terminal_toolbar};
use crate::ui::widgets;

const DEFAULT_HEIGHT: f32 = 280.0;
const SIDEBAR: f32 = 144.0;
const MAX_PER_GROUP: usize = 4;

/// One drawer's terminal groups (`terminalUiStateStore`).
pub struct Drawer {
    pub open: bool,
    pub height: f32,
    pub groups: Vec<Group>,
    pub active: String,
}

pub struct Group {
    pub ids: Vec<String>,
    pub vertical: bool,
}

/// Drawer actions gathered while painting.
enum DrawerAction {
    Split(bool),
    New,
    Close(String),
    Activate(String),
}

impl App {
    /// `terminal.toggle`: opens the drawer with its default session, or
    /// shows and hides it.
    pub(crate) fn toggle_terminal_drawer(&mut self) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        let drawer = self.drawers.entry(thread).or_insert_with(|| Drawer {
            open: false,
            height: DEFAULT_HEIGHT,
            groups: vec![Group {
                ids: vec!["default".into()],
                vertical: false,
            }],
            active: "default".into(),
        });
        drawer.open = !drawer.open;
    }

    /// Opens the drawer of `thread` (`setTerminalOpen(true)`); returns its
    /// active terminal.
    pub(crate) fn open_terminal_drawer(&mut self, thread: &str) -> String {
        let drawer = self
            .drawers
            .entry(thread.to_owned())
            .or_insert_with(|| Drawer {
                open: false,
                height: DEFAULT_HEIGHT,
                groups: vec![Group {
                    ids: vec!["default".into()],
                    vertical: false,
                }],
                active: "default".into(),
            });
        drawer.open = true;
        drawer.active.clone()
    }

    /// The open drawer's height for the current thread.
    pub(crate) fn drawer_height(&self, available: f32) -> Option<f32> {
        let drawer = self.drawers.get(self.selected.as_ref()?)?;
        drawer
            .open
            .then(|| drawer.height.clamp(120.0, (available - 220.0).max(120.0)))
    }

    pub(crate) fn terminal_drawer(&mut self, ui: &mut Ui, rect: Rect) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        ui.painter().rect_filled(rect, 0.0, theme::background());
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
            0.0,
            theme::overlay(0.048),
        );
        // The top edge resizes the drawer.
        let handle =
            Rect::from_min_size(rect.min - Vec2::new(0.0, 3.0), Vec2::new(rect.width(), 6.0));
        let response = ui.interact(handle, ui.id().with("drawer-resize"), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if response.dragged()
            && let Some(drawer) = self.drawers.get_mut(&thread)
        {
            drawer.height = (rect.height() - response.drag_delta().y).max(120.0);
        }

        let Some(drawer) = self.drawers.get(&thread) else {
            return;
        };
        let total: usize = drawer.groups.iter().map(|g| g.ids.len()).sum();
        let group_index = drawer
            .groups
            .iter()
            .position(|g| g.ids.contains(&drawer.active))
            .unwrap_or(0);
        let Some(group) = drawer.groups.get(group_index) else {
            return;
        };
        let (ids, vertical) = (group.ids.clone(), group.vertical);
        let active = drawer.active.clone();
        let body = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + 1.0), rect.max);
        let terminals = if total > 1 {
            Rect::from_min_max(
                body.min,
                Pos2::new(body.right() - SIDEBAR - 6.0, body.bottom()),
            )
        } else {
            body
        };
        let mut action = None;
        let count = ids.len() as f32;
        for (index, id) in ids.iter().enumerate() {
            let cell = if vertical {
                let height = terminals.height() / count;
                Rect::from_min_size(
                    Pos2::new(terminals.left(), terminals.top() + index as f32 * height),
                    Vec2::new(terminals.width(), height),
                )
            } else {
                let width = terminals.width() / count;
                Rect::from_min_size(
                    Pos2::new(terminals.left() + index as f32 * width, terminals.top()),
                    Vec2::new(width, terminals.height()),
                )
            };
            self.terminal_view(ui, cell, id);
            if index > 0 {
                let border = if *id == active {
                    theme::border()
                } else {
                    theme::alpha(theme::border(), 0.7)
                };
                let edge = if vertical {
                    Rect::from_min_size(cell.min, Vec2::new(cell.width(), 1.0))
                } else {
                    Rect::from_min_size(cell.min, Vec2::new(1.0, cell.height()))
                };
                ui.painter().rect_filled(edge, 0.0, border);
            }
            if ids.len() > 1
                && *id != active
                && ui.input(|i| i.pointer.any_pressed())
                && ui
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|pos| cell.contains(pos))
            {
                action = Some(DrawerAction::Activate(id.clone()));
            }
        }
        if total > 1 {
            let aside = Rect::from_min_max(Pos2::new(body.right() - SIDEBAR, body.top()), body.max);
            if let Some(chosen) = self.drawer_sidebar(ui, aside, &thread, ids.len()) {
                action = Some(chosen);
            }
        } else {
            match terminal_toolbar(ui, terminals, ids.len()) {
                Some(0) => action = Some(DrawerAction::Split(false)),
                Some(1) => action = Some(DrawerAction::Split(true)),
                Some(2) => action = Some(DrawerAction::New),
                Some(3) => action = Some(DrawerAction::Close(active.clone())),
                _ => {}
            }
        }
        if let Some(action) = action {
            self.drawer_action(&thread, action);
        }
    }

    /// The groups sidebar: action buttons, then each group's header (when
    /// there is more than one group or a split) and its sessions.
    fn drawer_sidebar(
        &mut self,
        ui: &mut Ui,
        aside: Rect,
        thread: &str,
        sessions: usize,
    ) -> Option<DrawerAction> {
        let drawer = self.drawers.get(thread)?;
        ui.painter().rect(
            aside,
            CornerRadius::ZERO,
            theme::overlay(0.003),
            Stroke::new(1.0, theme::overlay(0.042)),
            StrokeKind::Inside,
        );
        let header = Rect::from_min_size(
            aside.min + Vec2::splat(1.0),
            Vec2::new(aside.width() - 2.0, 22.0),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(header.left(), header.bottom() - 1.0),
                Vec2::new(header.width(), 1.0),
            ),
            0.0,
            theme::overlay(0.042),
        );
        let mut action = None;
        let split_disabled = sessions >= MAX_PER_GROUP;
        for (index, (icon, tip)) in [
            ("square-split-horizontal", "Split Terminal Horizontally"),
            ("square-split-vertical", "Split Terminal Vertically"),
            ("plus", "New Terminal"),
            ("trash", "Close Terminal"),
        ]
        .into_iter()
        .enumerate()
        {
            let button = Rect::from_min_size(
                Pos2::new(header.right() - (4 - index) as f32 * 22.0, header.top()),
                Vec2::new(22.0, 21.0),
            );
            ui.painter().rect_filled(
                Rect::from_min_size(button.min, Vec2::new(1.0, button.height())),
                0.0,
                theme::overlay(0.042),
            );
            let disabled = index < 2 && split_disabled;
            let response = ui
                .interact(
                    button,
                    ui.id().with(("drawer-action", index)),
                    if disabled {
                        Sense::hover()
                    } else {
                        Sense::click()
                    },
                )
                .on_hover_text(tip);
            if response.hovered() && !disabled {
                ui.painter()
                    .rect_filled(button, 0.0, theme::alpha(theme::accent(), 0.7));
            }
            theme::paint_icon(
                ui,
                icon,
                button,
                13.0,
                theme::alpha(theme::foreground(), if disabled { 0.58 } else { 0.9 }),
            );
            if response.clicked() {
                action = Some(match index {
                    0 => DrawerAction::Split(false),
                    1 => DrawerAction::Split(true),
                    2 => DrawerAction::New,
                    _ => DrawerAction::Close(drawer.active.clone()),
                });
            }
        }
        let show_headers = drawer.groups.len() > 1 || drawer.groups.iter().any(|g| g.ids.len() > 1);
        let left = aside.left() + 5.0;
        let width = aside.width() - 10.0;
        let mut y = header.bottom() + 4.0;
        for (index, group) in drawer.groups.iter().enumerate() {
            let group_active = group.ids.contains(&drawer.active);
            if show_headers {
                let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 22.0));
                let response =
                    ui.interact(row, ui.id().with(("drawer-group", index)), Sense::click());
                if group_active {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(4),
                        theme::alpha(theme::accent(), 0.5),
                    );
                } else if response.hovered() {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(4),
                        theme::alpha(theme::accent(), 0.4),
                    );
                }
                let (icon, label) = match (group.ids.len() > 1, group.vertical) {
                    (false, _) => ("square", "Single"),
                    (true, true) => ("square-split-vertical", "Stacked"),
                    (true, false) => ("square-split-horizontal", "Side by side"),
                };
                let color = if group_active || response.hovered() {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                };
                theme::paint_icon(
                    ui,
                    icon,
                    Rect::from_min_size(
                        Pos2::new(row.left() + 6.0, row.center().y - 6.0),
                        Vec2::splat(12.0),
                    ),
                    12.0,
                    color,
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(row.left() + 22.0, row.top()), row.max),
                    label,
                    font(11.0, Weight::Regular),
                    color,
                );
                let count = group.ids.len().to_string();
                let count_width = widgets::text_width(ui, &count, font(10.0, Weight::Regular));
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(row.right() - 6.0 - count_width, row.top()),
                        row.max,
                    ),
                    &count,
                    font(10.0, Weight::Regular),
                    theme::alpha(theme::muted_foreground(), 0.7),
                );
                if response.clicked() && !group_active {
                    action = group.ids.first().cloned().map(DrawerAction::Activate);
                }
                y = row.bottom();
            }
            for id in &group.ids {
                let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 24.0));
                let response =
                    ui.interact(row, ui.id().with(("drawer-terminal", id)), Sense::click());
                let is_active = *id == drawer.active;
                if is_active {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(8), theme::accent());
                } else if response.hovered() {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(8),
                        theme::alpha(theme::accent(), 0.6),
                    );
                }
                let color = if is_active || response.hovered() {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                };
                // `PanelTabCloseButton`: the glyph turns into a close button on hover.
                let glyph = Rect::from_min_size(
                    Pos2::new(row.left() + 8.0, row.center().y - 6.0),
                    Vec2::splat(12.0),
                );
                let close = ui.interact(
                    glyph.expand(2.0),
                    ui.id().with(("drawer-close", id)),
                    Sense::click(),
                );
                theme::paint_icon(
                    ui,
                    if response.hovered() || close.hovered() {
                        "x"
                    } else {
                        "square-terminal"
                    },
                    glyph,
                    12.0,
                    color,
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(row.left() + 24.0, row.top()),
                        Pos2::new(row.right() - 8.0, row.bottom()),
                    ),
                    &terminal_label(id),
                    font(12.0, Weight::Regular),
                    color,
                );
                if close.clicked() {
                    action = Some(DrawerAction::Close(id.clone()));
                } else if response.clicked() {
                    action = Some(DrawerAction::Activate(id.clone()));
                }
                y = row.bottom() + 2.0;
            }
            y += 2.0;
        }
        action
    }

    /// `terminal.split`, `terminal.splitVertical`, `terminal.new` and
    /// `terminal.close` on the drawer, which opens for the first three.
    pub(crate) fn terminal_command(&mut self, command: &str) -> bool {
        let Some(thread) = self.selected.clone() else {
            return false;
        };
        let open = self.drawers.get(&thread).is_some_and(|d| d.open);
        let action = match command {
            "terminal.close" => {
                if !open {
                    return false;
                }
                match self.drawers.get(&thread) {
                    Some(drawer) => DrawerAction::Close(drawer.active.clone()),
                    None => return false,
                }
            }
            "terminal.new" => DrawerAction::New,
            vertical => DrawerAction::Split(vertical == "terminal.splitVertical"),
        };
        if !open {
            self.open_terminal_drawer(&thread);
        }
        self.drawer_action(&thread, action);
        true
    }

    fn drawer_action(&mut self, thread: &str, action: DrawerAction) {
        let next = next_terminal_id(&self.used_terminal_ids());
        let Some(drawer) = self.drawers.get_mut(thread) else {
            return;
        };
        match action {
            DrawerAction::Activate(id) => drawer.active = id,
            DrawerAction::Split(vertical) => {
                if let Some(group) = drawer
                    .groups
                    .iter_mut()
                    .find(|g| g.ids.contains(&drawer.active))
                    && group.ids.len() < MAX_PER_GROUP
                {
                    group.ids.push(next.clone());
                    group.vertical = vertical;
                    drawer.active = next;
                }
            }
            DrawerAction::New => {
                drawer.groups.push(Group {
                    ids: vec![next.clone()],
                    vertical: false,
                });
                drawer.active = next;
            }
            DrawerAction::Close(id) => {
                let order: Vec<String> = drawer.groups.iter().flat_map(|g| g.ids.clone()).collect();
                let position = order.iter().position(|t| *t == id).unwrap_or(0);
                for group in drawer.groups.iter_mut() {
                    group.ids.retain(|t| *t != id);
                }
                drawer.groups.retain(|g| !g.ids.is_empty());
                if drawer.active == id {
                    let remaining: Vec<String> =
                        drawer.groups.iter().flat_map(|g| g.ids.clone()).collect();
                    drawer.active = remaining
                        .get(position.min(remaining.len().saturating_sub(1)))
                        .cloned()
                        .unwrap_or_default();
                }
                let empty = drawer.groups.is_empty();
                if empty {
                    self.drawers.remove(thread);
                }
                self.close_terminal(thread, &id);
            }
        }
    }
}
