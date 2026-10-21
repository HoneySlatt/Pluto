//! Right-panel surfaces (`RightPanelTabs.tsx`): the "Open a surface" launcher
//! an empty panel shows and the tab bar's "+" menu. Only the Diff surface is
//! rendered here; the others report that they are not implemented.
use eframe::egui::{CornerRadius, Key, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets::{self, MenuEntry, MenuItem};

struct Surface {
    label: &'static str,
    icon: &'static str,
    shortcut: &'static str,
    key: Key,
    /// `SURFACE_UNAVAILABLE_HINTS`
    hint: &'static str,
}

const SURFACES: [Surface; 8] = [
    Surface {
        label: "Browser",
        icon: "earth",
        shortcut: "B",
        key: Key::B,
        hint: "Only available in the desktop app.",
    },
    Surface {
        label: "Terminal",
        icon: "square-terminal",
        shortcut: "T",
        key: Key::T,
        hint: "Available when a project is open.",
    },
    Surface {
        label: "Files",
        icon: "files",
        shortcut: "F",
        key: Key::F,
        hint: "Available when a project is open.",
    },
    Surface {
        label: "Diff",
        icon: "file-diff",
        shortcut: "D",
        key: Key::D,
        hint: "Available for Git repositories.",
    },
    Surface {
        label: "Pull request",
        icon: "git-pull-request-arrow",
        shortcut: "P",
        key: Key::P,
        hint: "No pull request on this branch yet.",
    },
    Surface {
        label: "Linked pull requests",
        icon: "link-2",
        shortcut: "L",
        key: Key::L,
        hint: "No linked pull requests available.",
    },
    Surface {
        label: "Agents",
        icon: "bot",
        shortcut: "A",
        key: Key::A,
        hint: "Available from a thread.",
    },
    Surface {
        label: "Device",
        icon: "smartphone",
        shortcut: "M",
        key: Key::M,
        hint: "Available from a thread.",
    },
];

const ADD_MENU: &str = "add-surface";
const ROW: f32 = 32.0; // h-8
const ROW_GAP: f32 = 2.0; // gap-0.5
const WIDTH: f32 = 320.0; // max-w-xs

impl App {
    /// Which surfaces can open: the browser needs the desktop app, pull
    /// request surfaces need a linked pull request, the diff a repository.
    fn surface_available(&self, surface: &Surface) -> bool {
        match surface.label {
            "Browser" | "Pull request" | "Linked pull requests" => false,
            "Diff" => self.workspace_is_repo(),
            _ => true,
        }
    }

    fn open_surface(&mut self, label: &str) {
        if label == "Diff" {
            if let Some(panel) = self.panel_mut() {
                panel.diff = true;
            }
        } else {
            self.status = Some(format!("The {label} surface is not implemented yet"));
        }
    }

    /// The empty panel's centered surface list with letter shortcuts;
    /// `body` is the panel below its `topbar`-tall tab bar.
    pub(crate) fn surface_launcher(&mut self, ui: &mut Ui, body: Rect, topbar: f32) {
        let available: Vec<usize> = (0..SURFACES.len())
            .filter(|&i| self.surface_available(&SURFACES[i]))
            .collect();
        // The list centers against the whole panel: the bottom padding
        // matches the topbar above the body.
        let content = 24.0 + 20.0 + 12.0 + SURFACES.len() as f32 * (ROW + ROW_GAP) - ROW_GAP + 24.0;
        let center_y = (body.top() + body.bottom() - topbar) / 2.0;
        let top = (center_y - content / 2.0).max(body.top());
        let left = body.center().x - WIDTH / 2.0;
        let launcher = Rect::from_min_size(Pos2::new(left, top), Vec2::new(WIDTH, content));

        // Focus on mount so the letter shortcuts work right away; a focused
        // text field keeps its typing.
        let focus_id = ui.id().with("surface-launcher");
        let focus = ui.interact(launcher, focus_id, Sense::focusable_noninteractive());
        if !self.launcher_focused {
            self.launcher_focused = true;
            self.launcher_highlight = None;
            focus.request_focus();
        }
        let typing = ui.memory(|m| m.focused().is_some_and(|id| id != focus_id));
        let mut chosen: Option<usize> = None;
        if !typing {
            ui.input(|i| {
                if i.modifiers.any() {
                    return;
                }
                for &index in &available {
                    if i.key_pressed(SURFACES[index].key) {
                        chosen = Some(index);
                    }
                }
                let count = available.len();
                if count > 0 {
                    let current = self.launcher_highlight;
                    if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowRight) {
                        self.launcher_highlight = Some(current.map_or(0, |h| (h + 1) % count));
                    } else if i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::ArrowLeft) {
                        self.launcher_highlight =
                            Some(current.map_or(count - 1, |h| (h + count - 1) % count));
                    } else if i.key_pressed(Key::Enter)
                        && let Some(h) = current
                    {
                        chosen = available.get(h).copied();
                    }
                }
            });
        }

        let title = "Open a surface";
        let title_width = widgets::text_width(ui, title, font(14.0, Weight::Medium));
        widgets::text(
            ui,
            Pos2::new(launcher.center().x - title_width / 2.0, top + 24.0),
            title,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        let mut y = top + 24.0 + 20.0 + 12.0;
        for (index, surface) in SURFACES.iter().enumerate() {
            let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(WIDTH, ROW));
            y += ROW + ROW_GAP;
            let slot = available.iter().position(|&i| i == index);
            let response = ui.interact(
                row,
                ui.id().with(("surface", index)),
                if slot.is_some() {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            if slot.is_some() && response.hovered() {
                self.launcher_highlight = slot;
            }
            let highlighted = slot.is_some() && slot == self.launcher_highlight;
            if highlighted {
                ui.painter().rect_filled(
                    row,
                    widgets::radius(theme::CONTROL_RADIUS as f32),
                    theme::overlay(0.024),
                );
            }
            let alpha = if slot.is_some() { 1.0 } else { 0.5 };
            let color = theme::alpha(theme::foreground(), alpha);
            theme::paint_icon(
                ui,
                surface.icon,
                Rect::from_min_size(
                    Pos2::new(row.left() + 10.0, row.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                color,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.left() + 36.0, row.top()),
                    Pos2::new(row.right() - 40.0, row.bottom()),
                ),
                surface.label,
                font(14.0, Weight::Regular),
                color,
            );
            let kbd = Rect::from_min_size(
                Pos2::new(row.right() - 30.0, row.center().y - 10.0),
                Vec2::splat(20.0),
            );
            ui.painter().rect_filled(
                kbd,
                CornerRadius::same(4),
                theme::alpha(theme::overlay(0.03), alpha),
            );
            let key_font = font(12.0, Weight::Medium);
            let key_width = widgets::text_width(ui, surface.shortcut, key_font.clone());
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(kbd.center().x - key_width / 2.0, kbd.top()),
                    kbd.max,
                ),
                surface.shortcut,
                key_font,
                theme::alpha(theme::muted_foreground(), alpha),
            );
            let response = if slot.is_none() {
                response.on_hover_text(surface.hint)
            } else {
                response
            };
            if response.clicked() {
                chosen = Some(index);
            }
        }
        if let Some(index) = chosen {
            self.open_surface(SURFACES[index].label);
        }
    }

    /// The tab bar's "+" button and its surface menu.
    pub(crate) fn add_surface_button(&mut self, ui: &mut Ui, button: Rect) {
        let open = self.open_menu.as_deref() == Some(ADD_MENU);
        if open {
            ui.painter()
                .rect_filled(button, CornerRadius::same(8), theme::accent());
        }
        if widgets::icon_button(
            ui,
            button,
            "panel-add-tab",
            "plus",
            14.0,
            if open {
                theme::foreground()
            } else {
                theme::muted_foreground()
            },
            "",
        )
        .clicked()
        {
            self.open_menu = if open { None } else { Some(ADD_MENU.into()) };
        }
        let entries: Vec<MenuEntry> = SURFACES
            .iter()
            .map(|surface| {
                let available = self.surface_available(surface);
                MenuEntry::Item(MenuItem {
                    icon: Some(surface.icon),
                    icon_size: 16.0,
                    shortcut: Some(surface.shortcut),
                    disabled: !available,
                    tooltip: (!available).then_some(surface.hint),
                    ..MenuItem::new(surface.label)
                })
            })
            .collect();
        // `sideOffset={6}` against the menu helper's 4px.
        let anchor = button.translate(Vec2::new(0.0, 2.0));
        if let Some(index) =
            widgets::menu(ui, &mut self.open_menu, ADD_MENU, anchor, false, &entries)
        {
            self.open_surface(SURFACES[index].label);
        }
    }
}
