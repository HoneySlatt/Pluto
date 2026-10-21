//! Sidebar project scope (`Sidebar.tsx`, the `projectScope` combobox): the
//! folder button opens a searchable project list under the search field and
//! the chosen project filters the thread list. Upstream persists the scope in
//! the UI state store; here it lives in memory.
use eframe::egui::{self, Key, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::Project;
use crate::theme::{self, Weight, font};
use crate::timeline::parse_time;
use crate::ui::{project_icon, widgets};

pub const MENU_ID: &str = "project-scope";
const MAX_WIDTH: f32 = 288.0; // max-w-[min(18rem, available)]
const MAX_HEIGHT: f32 = 368.0; // max-h-[min(available, 23rem)]
const SEARCH_BLOCK: f32 = 43.0; // pt-2.5 + 33px field
const ALL_ROW: f32 = 28.0; // sm:min-h-7
const PROJECT_ROW: f32 = 32.0; // py-1 around the 24px settings button

#[derive(Default)]
pub struct ScopeMenu {
    pub query: String,
    pub highlight: usize,
}

/// `sortLogicalProjectsForSidebar` with the default `updated_at` order: most
/// recent user message among a project's live threads, then title.
pub fn sorted_projects(app: &App) -> Vec<Project> {
    let ms = |iso: Option<&str>| iso.and_then(parse_time).map(|d| d.timestamp_millis());
    let mut projects: Vec<(i64, Project)> = app
        .state
        .projects
        .iter()
        .map(|project| {
            let latest = app
                .state
                .threads
                .iter()
                .filter(|t| t.project_id == project.id && t.archived_at.is_none())
                .map(|t| {
                    ms(t.latest_user_message_at.as_deref())
                        .or_else(|| ms(Some(&t.updated_at)))
                        .or_else(|| ms(Some(&t.created_at)))
                        .unwrap_or(i64::MIN)
                })
                .max()
                .unwrap_or_else(|| {
                    ms(Some(&project.updated_at))
                        .or_else(|| ms(Some(&project.created_at)))
                        .unwrap_or(i64::MIN)
                });
            (latest, project.clone())
        })
        .collect();
    projects.sort_by(|(a, pa), (b, pb)| {
        b.cmp(a)
            .then_with(|| pa.title.cmp(&pb.title))
            .then_with(|| pa.id.cmp(&pb.id))
    });
    projects.into_iter().map(|(_, p)| p).collect()
}

impl App {
    /// The scoped project, once it still exists.
    pub(crate) fn scoped_project(&self) -> Option<&Project> {
        self.project_scope
            .as_deref()
            .and_then(|id| self.state.project(id))
    }

    pub(crate) fn toggle_project_scope_menu(&mut self) {
        if self.open_menu.as_deref() == Some(MENU_ID) {
            self.open_menu = None;
            return;
        }
        let projects = sorted_projects(self);
        // The current value starts highlighted ("All projects" heads the list).
        let highlight = self
            .project_scope
            .as_ref()
            .and_then(|id| projects.iter().position(|p| &p.id == id))
            .map_or(0, |i| i + 1);
        self.scope_menu = ScopeMenu {
            query: String::new(),
            highlight,
        };
        self.open_menu = Some(MENU_ID.into());
    }

    /// The popup, anchored to the header search field (`anchor`).
    pub(crate) fn project_scope_menu(&mut self, ui: &mut Ui, anchor: Rect) {
        if self.open_menu.as_deref() != Some(MENU_ID) {
            return;
        }
        let projects = sorted_projects(self);
        let needle = self.scope_menu.query.trim().to_lowercase();
        // "All projects" is the default row, not a searchable entry.
        let mut rows: Vec<Option<&Project>> = Vec::new();
        if needle.is_empty() {
            rows.push(None);
        }
        rows.extend(
            projects
                .iter()
                .filter(|p| needle.is_empty() || p.title.to_lowercase().contains(&needle))
                .map(Some),
        );
        let label_font = font(14.0, Weight::Regular);
        let widest = projects
            .iter()
            .map(|p| widgets::text_width(ui, &p.title, label_font.clone()))
            .chain([widgets::text_width(ui, "All projects", label_font.clone())])
            .fold(0.0_f32, f32::max);
        // Border, list padding, item padding, icon, gaps and settings button.
        let width = (widest.ceil() + 82.0).clamp(anchor.width(), MAX_WIDTH);
        let list_height = if rows.is_empty() {
            36.0
        } else {
            8.0 + rows
                .iter()
                .map(|r| if r.is_some() { PROJECT_ROW } else { ALL_ROW })
                .sum::<f32>()
        };
        let top = anchor.bottom() + 4.0;
        let available = ui.ctx().content_rect().bottom() - top - 5.0;
        let height = (2.0 + SEARCH_BLOCK + list_height).min(MAX_HEIGHT.min(available));
        let rect = Rect::from_min_size(Pos2::new(anchor.left(), top), Vec2::new(width, height));

        if !rows.is_empty() {
            let last = rows.len() - 1;
            let highlight = self.scope_menu.highlight.min(last);
            self.scope_menu.highlight = ui.input(|i| {
                if i.key_pressed(Key::ArrowDown) {
                    if highlight == last { 0 } else { highlight + 1 }
                } else if i.key_pressed(Key::ArrowUp) {
                    if highlight == 0 { last } else { highlight - 1 }
                } else {
                    highlight
                }
            });
        }
        let mut chosen: Option<Option<String>> = None;
        // The project whose settings gear was clicked (id).
        let mut settings: Option<String> = None;
        if !rows.is_empty() && ui.input(|i| i.key_pressed(Key::Enter)) {
            chosen = Some(rows[self.scope_menu.highlight].map(|p| p.id.clone()));
        }

        let mut open = self.open_menu.take();
        let mut query = self.scope_menu.query.clone();
        let mut highlight = self.scope_menu.highlight;
        let scope = self.project_scope.clone();
        widgets::popup(ui, &mut open, MENU_ID, rect, anchor, |ui, rect| {
            let inner = rect.shrink(1.0);
            let search = Rect::from_min_size(
                Pos2::new(inner.left() + 12.0, inner.top() + 9.0),
                Vec2::new(inner.width() - 24.0, 33.0),
            );
            widgets::combobox_search(ui, search, &mut query, "Search projects...");
            let list = Rect::from_min_max(
                Pos2::new(inner.left(), inner.top() + SEARCH_BLOCK),
                inner.max,
            );
            if rows.is_empty() {
                let label = "No matching projects.";
                let text_width = widgets::text_width(ui, label, label_font.clone());
                widgets::text(
                    ui,
                    Pos2::new(list.center().x - text_width / 2.0, list.top() + 8.0),
                    label,
                    label_font.clone(),
                    theme::muted_foreground(),
                    20.0,
                );
                return;
            }
            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
            egui::ScrollArea::vertical()
                .id_salt("project-scope-list")
                .auto_shrink([false, false])
                .show(&mut list_ui, |ui| {
                    let (area, _) = ui
                        .allocate_exact_size(Vec2::new(list.width(), list_height), Sense::hover());
                    let mut y = area.top() + 4.0;
                    for (index, row) in rows.iter().enumerate() {
                        let row_height = if row.is_some() { PROJECT_ROW } else { ALL_ROW };
                        let item = Rect::from_min_size(
                            Pos2::new(area.left() + 4.0, y),
                            Vec2::new(area.width() - 8.0, row_height),
                        );
                        y += row_height;
                        let response =
                            ui.interact(item, ui.id().with(("scope-row", index)), Sense::click());
                        if response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                            highlight = index;
                        }
                        let selected = row.map(|p| p.id.clone()) == scope;
                        let background = if index == highlight {
                            Some(theme::accent())
                        } else if selected {
                            Some(theme::alpha(theme::foreground(), 0.08))
                        } else {
                            None
                        };
                        if let Some(color) = background {
                            ui.painter()
                                .rect_filled(item, egui::CornerRadius::same(6), color);
                        }
                        let icon = Rect::from_min_size(
                            Pos2::new(item.left() + 8.0, item.center().y - 8.0),
                            Vec2::splat(16.0),
                        );
                        let mut label_right = item.right() - 8.0;
                        match row {
                            Some(project) => {
                                project_icon::paint(ui, icon, project);
                                let gear = Rect::from_min_size(
                                    Pos2::new(item.right() - 32.0, item.center().y - 12.0),
                                    Vec2::splat(24.0),
                                );
                                label_right = gear.left() - 8.0;
                                let gear_response = ui.interact(
                                    gear,
                                    ui.id().with(("scope-settings", index)),
                                    Sense::click(),
                                );
                                if gear_response.hovered() {
                                    ui.painter().rect_filled(
                                        gear,
                                        egui::CornerRadius::same(8),
                                        theme::accent(),
                                    );
                                }
                                theme::paint_icon(
                                    ui,
                                    "settings",
                                    gear,
                                    14.0,
                                    if gear_response.hovered() {
                                        theme::foreground()
                                    } else {
                                        theme::muted_foreground()
                                    },
                                );
                                let gear_response = gear_response.on_hover_text(format!(
                                    "Project settings for {}",
                                    project.title
                                ));
                                if gear_response.clicked() {
                                    settings = Some(project.id.clone());
                                    continue;
                                }
                            }
                            None => theme::paint_icon(
                                ui,
                                "folder",
                                icon,
                                16.0,
                                theme::muted_foreground(),
                            ),
                        }
                        let label = row.map_or("All projects", |p| p.title.as_str());
                        let galley = widgets::galley_truncated(
                            ui,
                            label,
                            label_font.clone(),
                            theme::foreground(),
                            20.0,
                            label_right - (item.left() + 32.0),
                        );
                        ui.painter().galley(
                            Pos2::new(item.left() + 32.0, item.center().y - 10.0),
                            galley,
                            theme::foreground(),
                        );
                        if response.clicked() {
                            chosen = Some(row.map(|p| p.id.clone()));
                        }
                    }
                });
        });
        if query != self.scope_menu.query {
            // A new query re-highlights the first match (`autoHighlight`).
            highlight = 0;
        }
        self.scope_menu.query = query;
        self.scope_menu.highlight = highlight;
        self.open_menu = open;
        if let Some(id) = settings {
            self.open_menu = None;
            self.open_project_settings(&id);
        } else if let Some(scope) = chosen {
            self.open_menu = None;
            self.project_scope = scope;
        }
    }
}
