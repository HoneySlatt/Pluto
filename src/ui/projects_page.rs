//! The Project settings page for the scoped project
//! (`components/settings/ProjectsSettings.tsx`, `ProjectSettingsPanel.tsx`):
//! the scope hint, name and icon, the project's new-thread defaults
//! (`ProjectDefaultsSettings category="project"`), its actions, and removal.
//!
//! The icon pickers (`ProjectIconPickerDialog`, `ProjectFaviconPickerDialog`)
//! and editing actions (`ProjectActionsSettings`) are not ported.

use eframe::egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::Project;
use crate::theme::{self, Weight, font};
use crate::ui::settings::{
    Control, Row, RowAction, card_border, card_fill, row_divider, row_height,
};
use crate::ui::thread_menu::Confirm;
use crate::ui::widgets;

/// The page index, after the entries of `PAGES`.
pub const PROJECT_PAGE: usize = 10;
const CARD_WIDTH: f32 = 848.0;

/// The icon row's description: the chosen icon, the favicon path, or "Automatic".
fn icon_description(project: &Project) -> String {
    let icon = project.project_icon.as_ref();
    let text = |key: &str| {
        icon.and_then(|i| i[key].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    match icon.and_then(|i| i["kind"].as_str()) {
        Some("lucide") => format!("{} · {}", text("name"), text("color")),
        Some("monogram") => format!("{} · {}", text("text"), text("color")),
        Some("emoji") => text("emoji"),
        _ => project
            .favicon_path
            .clone()
            .unwrap_or_else(|| "Automatic".into()),
    }
}

impl App {
    /// Opens the Project page for `project_id` (`/projects/$projectKey`).
    pub(crate) fn open_project_settings(&mut self, project_id: &str) {
        self.settings_scope.project = Some(project_id.to_owned());
        self.open_settings_page(PROJECT_PAGE);
    }

    /// The page body from `top`; returns its bottom.
    pub(crate) fn projects_page(&mut self, ui: &mut Ui, left: f32, top: f32) -> f32 {
        let Some(project) = self.settings_project() else {
            // A removed project leaves for the app (`navigate({ to: "/" })`).
            if self.settings_scope.project.take().is_some() {
                self.settings_page = None;
                return top;
            }
            // `SettingsScopeNotice target="project"`.
            widgets::text(
                ui,
                Pos2::new(left + 16.0, top),
                "Choose a project to manage its name, icon, checkouts and actions.",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
            return top + 20.0;
        };
        // `Alert variant="info"`.
        let alert = Rect::from_min_size(Pos2::new(left, top), Vec2::new(CARD_WIDTH, 46.0));
        ui.painter().rect(
            alert,
            CornerRadius::same(14),
            theme::alpha(theme::primary(), 0.04),
            Stroke::new(1.0, theme::alpha(theme::primary(), 0.32)),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "info",
            Rect::from_min_size(
                Pos2::new(alert.left() + 15.0, alert.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::primary(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(alert.left() + 39.0, alert.top()), alert.max),
            "Can't find a setting? Keep this project picked above and hop to any other settings page.",
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
        );
        let mut y = alert.bottom() + 24.0;
        let mut actions: Vec<(String, RowAction)> = Vec::new();

        // Name and icon (`SettingsSection hideTitle`).
        let overview = vec![
            Row {
                id: "project-name".into(),
                title: "Name".into(),
                description:
                    "The shared name for this project group in the sidebar and thread lists.".into(),
                status: None,
                layers: false,
                reset: false,
                control: Control::Field {
                    value: project.title.clone(),
                    placeholder: String::new(),
                    width: 256.0,
                },
            },
            Row {
                id: "project-icon".into(),
                title: "Project icon".into(),
                description: icon_description(&project),
                status: None,
                layers: false,
                reset: project.project_icon.is_some() || project.favicon_path.is_some(),
                control: Control::ProjectIcon(project.clone()),
            },
        ];
        y = self.project_card(ui, left, y, None, &overview, &mut actions) + 24.0;

        // `ProjectDefaultsSettings category="project"`: model and workspace.
        let defaults: Vec<Row> = self
            .general_rows()
            .into_iter()
            .filter(|row| row.id == "default-model" || row.id == "default-workspace")
            .collect();
        y = self.project_card(ui, left, y, Some("New threads"), &defaults, &mut actions) + 24.0;

        // `ProjectActionsSettings`: the project's actions (read-only here).
        let scripts: Vec<Value> = self.scoped_settings()["defaultProjectScripts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut rows = vec![Row {
            id: "project-actions".into(),
            title: "Actions".into(),
            description: "Commands that run in this project's checkout or its worktree, with optional shortcuts.".into(),
            status: None,
            layers: true,
            reset: false,
            control: Control::OutlineXs { label: "Add action", icon: "plus" },
        }];
        if scripts.is_empty() {
            rows.push(Row {
                id: "project-actions-empty".into(),
                title: String::new(),
                description: String::new(),
                status: None,
                layers: false,
                reset: false,
                control: Control::None,
            });
        }
        for script in &scripts {
            rows.push(Row {
                id: format!(
                    "project-action:{}",
                    script["id"].as_str().unwrap_or_default()
                ),
                title: script["name"].as_str().unwrap_or_default().to_owned(),
                description: script["command"].as_str().unwrap_or_default().to_owned(),
                status: None,
                layers: false,
                reset: false,
                control: Control::None,
            });
        }
        y = self.project_card(ui, left, y, Some("Actions"), &rows, &mut actions) + 24.0;

        // Danger.
        let danger = vec![Row {
            id: "remove-project".into(),
            title: "Remove project".into(),
            description:
                "Deletes the project entry and its threads. Files on disk are not touched.".into(),
            status: None,
            layers: false,
            reset: false,
            control: Control::Destructive {
                label: "Remove project",
                icon: "trash-2",
            },
        }];
        y = self.project_card(ui, left, y, Some("Danger"), &danger, &mut actions);

        for (id, action) in actions {
            self.apply_project(&project, &id, action);
        }
        y
    }

    /// A settings section on this page: optional title, then its card.
    fn project_card(
        &mut self,
        ui: &mut Ui,
        left: f32,
        mut y: f32,
        title: Option<&str>,
        rows: &[Row],
        actions: &mut Vec<(String, RowAction)>,
    ) -> f32 {
        if let Some(title) = title {
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(CARD_WIDTH, 28.0)),
                title,
                font(14.0, Weight::Regular),
                theme::alpha(theme::foreground(), 0.7),
            );
            y += 28.0 + 10.0;
        }
        let heights: Vec<f32> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let base = if row.id == "project-actions-empty" {
                    37.0
                } else {
                    row_height(ui, row, CARD_WIDTH)
                };
                base - if i == 0 { 1.0 } else { 0.0 }
            })
            .collect();
        let card = Rect::from_min_size(
            Pos2::new(left, y),
            Vec2::new(CARD_WIDTH, heights.iter().sum::<f32>() + 2.0),
        );
        ui.painter().rect(
            card,
            CornerRadius::same(14),
            card_fill(),
            Stroke::new(1.0, card_border()),
            StrokeKind::Inside,
        );
        let mut row_top = card.top() + 1.0;
        for (index, (row, height)) in rows.iter().zip(&heights).enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(card.left() + 1.0, row_top),
                Vec2::new(CARD_WIDTH - 2.0, *height),
            );
            if index > 0 {
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
                    0.0,
                    row_divider(),
                );
            }
            if row.id == "project-actions-empty" {
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(rect.left() + 16.0, rect.top() + 1.0), rect.max),
                    "No actions configured.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                );
            } else if let Some(action) = self.settings_row(ui, rect, row, index > 0) {
                actions.push((row.id.clone(), action));
            }
            row_top += height;
        }
        card.bottom()
    }

    fn apply_project(&mut self, project: &Project, id: &str, action: RowAction) {
        match (id, action) {
            // `renameGroup`: a trimmed, non-empty, changed title.
            ("project-name", RowAction::Text(title)) => {
                let title = title.trim().to_owned();
                if title.is_empty() {
                    self.status = Some("Project title cannot be empty".into());
                } else if title != project.title {
                    self.dispatch(json!({
                        "type": "project.meta.update",
                        "commandId": uuid::Uuid::new_v4().to_string(),
                        "projectId": project.id,
                        "title": title,
                    }));
                }
            }
            ("project-icon", RowAction::Reset) => self.dispatch(json!({
                "type": "project.meta.update",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": project.id,
                "faviconPath": null,
                "projectIcon": null,
            })),
            ("project-icon", RowAction::Select(_)) => {
                self.status = Some("Choosing a project icon is not implemented yet".into());
            }
            ("project-actions", RowAction::Click) => {
                self.status = Some("Editing project actions is not implemented yet".into());
            }
            ("remove-project", RowAction::Click) => {
                // `removeMembers` for the whole (single-member) project.
                let threads = self
                    .state
                    .threads
                    .iter()
                    .filter(|t| t.project_id == project.id && t.archived_at.is_none())
                    .count();
                let title = if threads > 0 {
                    format!(
                        "Remove project \"{}\" and delete its {threads} thread{}?",
                        project.title,
                        if threads == 1 { "" } else { "s" }
                    )
                } else {
                    format!("Remove project \"{}\"?", project.title)
                };
                let mut lines = vec![format!("Path: {}", project.workspace_root)];
                if let Some(label) = self.state.config["environment"]["label"].as_str() {
                    lines.push(format!("Environment: {label}"));
                }
                lines.push(if threads > 0 {
                    "This permanently clears conversation history for those threads and any archived threads.".into()
                } else {
                    "This permanently clears any archived conversation history.".into()
                });
                lines.push("This removes only the project entries, not the files on disk.".into());
                lines.push("This action cannot be undone.".into());
                self.confirm = Some(Confirm {
                    title,
                    message: lines.join("\n"),
                    confirm_label: "Remove",
                    command: json!({
                        "type": "project.delete",
                        "commandId": uuid::Uuid::new_v4().to_string(),
                        "projectId": project.id,
                        "force": true,
                    }),
                });
            }
            (id, action) => self.apply_general(id, action),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_descriptions_follow_upstream() {
        let mut project = Project::default();
        assert_eq!(icon_description(&project), "Automatic");
        project.favicon_path = Some("public/icon.svg".into());
        assert_eq!(icon_description(&project), "public/icon.svg");
        project.project_icon = Some(json!({"kind": "monogram", "text": "T3", "color": "indigo"}));
        assert_eq!(icon_description(&project), "T3 · indigo");
    }
}
