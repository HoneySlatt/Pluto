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

const SURFACES: [Surface; 9] = [
    Surface {
        label: "Browser",
        icon: "earth",
        shortcut: "B",
        key: Key::B,
        hint: "Needs Chromium or another Chromium-based browser.",
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
    // Pluto: the thread's published pages (`crate::ui::design`).
    Surface {
        label: "Design",
        icon: "palette",
        shortcut: "G",
        key: Key::G,
        hint: "Available once the agent publishes a page.",
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
            // The browser needs a Chromium-based browser to run headless.
            "Browser" => crate::preview::browser_binary().is_some(),
            "Pull request" => false,
            // `pullRequestsSurfaceAvailable`.
            "Linked pull requests" => {
                self.links_pull_requests() && self.linked_pull_request_count() > 0
            }
            "Diff" => self.workspace_is_repo(),
            "Design" => !self.thread_pages().is_empty(),
            _ => true,
        }
    }

    /// `view.reopenClosed`: the last closed panel tab whose thread is
    /// still there opens again, in that thread (a browser tab on its last
    /// page); false when none.
    pub(crate) fn reopen_closed_view(&mut self) -> bool {
        while !self.closed_views.is_empty() {
            let (thread, tab, url) = self.closed_views.remove(0);
            if !self.state.threads.iter().any(|t| t.id == thread) {
                continue;
            }
            if self.selected.as_deref() != Some(thread.as_str()) {
                self.select_thread(Some(thread));
            }
            if matches!(tab, crate::app::PanelTab::Browser { .. }) {
                let Some(ctx) = self.egui_ctx.clone() else {
                    return false;
                };
                self.open_browser_tab(&ctx, url);
                return true;
            }
            if let Some(panel) = self.panel_mut() {
                let wanted = tab.clone();
                panel.show(tab, |t| *t == wanted);
                panel.open = true;
            }
            return true;
        }
        false
    }

    fn open_surface(&mut self, label: &str) {
        if label == "Diff" {
            if let Some(panel) = self.panel_mut() {
                panel.show(crate::app::PanelTab::Diff, |t| {
                    *t == crate::app::PanelTab::Diff
                });
            }
        } else if label == "Terminal" {
            self.open_terminal_tab();
        } else if label == "Browser" {
            if let Some(ctx) = self.egui_ctx.clone() {
                self.open_browser_tab(&ctx, None);
            }
        } else if label == "Agents" {
            if let Some(panel) = self.panel_mut() {
                panel.show(crate::app::PanelTab::Agents, |t| {
                    *t == crate::app::PanelTab::Agents
                });
            }
        } else if label == "Linked pull requests" {
            self.show_linked_pull_requests();
        } else if label == "Design" {
            self.open_design(None);
        } else if label == "Files" {
            if let Some(panel) = self.panel_mut() {
                panel.show(crate::app::PanelTab::Files { file: None }, |t| {
                    matches!(t, crate::app::PanelTab::Files { .. })
                });
            }
        } else {
            self.notify(
                crate::ui::toasts::Kind::Info,
                format!("The {label} surface is not implemented yet"),
            );
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
    /// The panel's tab bar (`RightPanelTabs`): one tab per open surface
    /// (`pl-1.5 pr-2`, a 12px glyph that turns into a close button while the
    /// tab is hovered) and the "+" surface menu.
    pub(crate) fn panel_tab_bar(&mut self, ui: &mut Ui, inner: Rect) {
        let panel = self.panel();
        let mut x = inner.left() + 8.0;
        let mut select = None;
        let mut close = None;
        let mut menu = None;
        for (index, tab) in panel.tabs.iter().enumerate() {
            let (label, icon, tint) = match tab {
                crate::app::PanelTab::Diff => ("Diff".to_owned(), "file-diff", theme::foreground()),
                crate::app::PanelTab::Agents => ("Agents".to_owned(), "bot", theme::foreground()),
                crate::app::PanelTab::PullRequests => {
                    ("Pull requests".to_owned(), "link-2", theme::foreground())
                }
                crate::app::PanelTab::Design => {
                    ("Design".to_owned(), "palette", theme::foreground())
                }
                crate::app::PanelTab::Browser { id } => {
                    (self.browser_tab_title(*id), "earth", theme::foreground())
                }
                crate::app::PanelTab::Files { file: None } => {
                    ("Files".to_owned(), "files", theme::foreground())
                }
                crate::app::PanelTab::Terminal { number, .. } => (
                    format!("Terminal {number}"),
                    "square-terminal",
                    theme::foreground(),
                ),
                crate::app::PanelTab::Files { file: Some(path) } => {
                    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
                    let extension = name.rsplit('.').next().unwrap_or_default().to_owned();
                    let (icon, tint) = crate::ui::markdown::language_icon(&extension)
                        .unwrap_or(("file-text", theme::foreground()));
                    (name, icon, tint)
                }
            };
            let text_width = widgets::text_width(ui, &label, font(12.0, Weight::Regular));
            let rect = Rect::from_min_size(
                Pos2::new(x, 14.0),
                Vec2::new((6.0 + 12.0 + 6.0 + text_width + 8.0).round(), 24.0),
            );
            x = rect.right() + 4.0;
            let active = index == panel.active.min(panel.tabs.len() - 1);
            let response = ui.interact(rect, ui.id().with(("panel-tab", index)), Sense::click());
            if active || response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
            }
            let glyph = Rect::from_min_size(
                Pos2::new(rect.left() + 4.0, rect.center().y - 8.0),
                Vec2::splat(16.0),
            );
            let close_response = ui
                .interact(
                    glyph,
                    ui.id().with(("close-panel-tab", index)),
                    Sense::click(),
                )
                .on_hover_text(format!("Close {label}"));
            if close_response.hovered() {
                ui.painter()
                    .rect_filled(glyph, CornerRadius::same(6), theme::muted());
            }
            let hovered = ui.rect_contains_pointer(rect);
            theme::paint_icon(
                ui,
                if hovered { "x" } else { icon },
                glyph,
                12.0,
                if hovered { theme::foreground() } else { tint },
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(rect.min + Vec2::new(24.0, 0.0), rect.max),
                &label,
                font(12.0, Weight::Regular),
                if active {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
            );
            if close_response.clicked() || response.middle_clicked() {
                close = Some(index);
            } else if response.clicked() {
                select = Some(index);
            }
            if response.secondary_clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                menu = Some((index, crate::ui::context_menu::State::new(pos)));
            }
        }
        if menu.is_some() {
            self.panel_tab_menu = menu;
        }
        if let Some(index) = close {
            self.close_panel_tab(index, true);
        } else if let Some(index) = select
            && let Some(panel) = self.panel_mut()
        {
            panel.active = index;
        }
        self.panel_tab_menu(ui);
        let add = Rect::from_min_size(Pos2::new(x, 14.0), Vec2::splat(24.0));
        self.add_surface_button(ui, add);
    }

    /// Closes tab `index`: a terminal's sessions end (asking first when
    /// `confirm`), a browser tab's page and the other views can come back
    /// (`closedViewStore`, newest first, 20 at most).
    fn close_panel_tab(&mut self, index: usize, confirm: bool) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        let Some(tab) = self.panel().tabs.get(index).cloned() else {
            return;
        };
        if let crate::app::PanelTab::Terminal { id, split, .. } = &tab {
            let ids: Vec<String> = std::iter::once(id).chain(split.iter()).cloned().collect();
            if confirm {
                self.confirm_close_terminals(&thread, ids, false);
            } else {
                for id in ids {
                    self.close_terminal(&thread, &id);
                }
            }
            return;
        }
        let url = match &tab {
            crate::app::PanelTab::Browser { id } => {
                let url = self
                    .preview
                    .engine
                    .as_ref()
                    .and_then(|e| e.tab(*id))
                    .map(|state| state.url)
                    .filter(|u| !u.trim().is_empty());
                self.close_browser_tab(*id);
                url
            }
            _ => None,
        };
        if let Some(panel) = self.panel_mut() {
            panel.close(index);
        }
        self.closed_views
            .retain(|(t, v, _)| !(*t == thread && *v == tab));
        self.closed_views.insert(0, (thread, tab, url));
        self.closed_views.truncate(20);
    }

    /// The tab's right-click menu: Copy path for a file, then Close,
    /// Close others, Close to the right and Close all (terminals close
    /// without asking, as upstream's bulk closes do).
    fn panel_tab_menu(&mut self, ui: &mut Ui) {
        let Some((index, state)) = self.panel_tab_menu.take() else {
            return;
        };
        let tabs = self.panel().tabs.clone();
        let Some(tab) = tabs.get(index) else {
            return;
        };
        let mut items = Vec::new();
        if let crate::app::PanelTab::Files { file: Some(_) } = tab {
            items.push(crate::ui::context_menu::Item::new("copy-path", "Copy path"));
        }
        items.push(crate::ui::context_menu::Item::new("close", "Close"));
        items.push(crate::ui::context_menu::Item {
            disabled: tabs.len() <= 1,
            ..crate::ui::context_menu::Item::new("close-others", "Close others")
        });
        items.push(crate::ui::context_menu::Item {
            disabled: index + 1 >= tabs.len(),
            ..crate::ui::context_menu::Item::new("close-to-right", "Close to the right")
        });
        items.push(crate::ui::context_menu::Item::new("close-all", "Close all"));
        let mut open = Some(state);
        let chosen = crate::ui::context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.panel_tab_menu = Some((index, state));
        }
        let Some(chosen) = chosen else {
            return;
        };
        self.panel_tab_menu = None;
        let bulk: Vec<usize> = match chosen.as_str() {
            "copy-path" => {
                if let crate::app::PanelTab::Files { file: Some(path) } = tab {
                    ui.ctx().copy_text(path.clone());
                }
                return;
            }
            "close" => {
                self.close_panel_tab(index, true);
                return;
            }
            "close-others" => (0..tabs.len()).filter(|&i| i != index).collect(),
            "close-to-right" => (index + 1..tabs.len()).collect(),
            "close-all" => (0..tabs.len()).collect(),
            _ => return,
        };
        for i in bulk.into_iter().rev() {
            self.close_panel_tab(i, false);
        }
    }

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
