//! The Project settings page for the scoped project
//! (`components/settings/ProjectsSettings.tsx`, `ProjectSettingsPanel.tsx`):
//! the scope hint, name and icon, the project's new-thread defaults
//! (`ProjectDefaultsSettings category="project"`), its actions, and removal.
//!
//! "Choose icon" opens `icon_picker`, "Choose file" the palette's favicon
//! mode; the desktop app's "Open in <file manager>" image picker is not
//! ported.

use eframe::egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::Project;
use crate::theme::{self, Weight, font};
use crate::ui::palette::View;
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
    // The lucide wire shape with `monogramText` decodes to a monogram.
    let monogram = icon
        .and_then(|i| i["monogramText"].as_str().or(i["monogram"].as_str()))
        .map(str::to_owned);
    if let Some(monogram) = monogram {
        return format!("{monogram} · {}", text("color"));
    }
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
                } else if row.id.starts_with("project-action:") {
                    // `ProjectActionsList` rows: `py-2`.
                    8.0 + 20.0 + 4.0 + 18.0 + 8.0 + 1.0
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
            } else if row.id.starts_with("project-action:") {
                if self.action_row(ui, rect, row, index > 0) {
                    actions.push((row.id.clone(), RowAction::Click));
                }
            } else if let Some(action) = self.settings_row(ui, rect, row, index > 0) {
                actions.push((row.id.clone(), action));
            }
            row_top += height;
        }
        card.bottom()
    }

    /// A `ProjectActionsList` row: icon, name and badges, the command in mono,
    /// the shortcut, and the edit gear (shown on hover); returns whether the
    /// gear was clicked.
    fn action_row(&mut self, ui: &mut Ui, rect: Rect, row: &Row, divided: bool) -> bool {
        let Some(project) = self.settings_project() else {
            return false;
        };
        let script_id = &row.id["project-action:".len()..];
        let script = self
            .project_scripts(&project.id)
            .into_iter()
            .find(|s| s["id"] == script_id)
            .unwrap_or(Value::Null);
        let top = rect.top() + if divided { 1.0 } else { 0.0 } + 8.0;
        let left = rect.left() + 16.0;
        let muted = theme::muted_foreground();
        theme::paint_icon(
            ui,
            crate::ui::action_editor::script_icon(script["icon"].as_str().unwrap_or("play")),
            Rect::from_min_size(Pos2::new(left, top + 2.0), Vec2::splat(16.0)),
            16.0,
            muted,
        );
        let mut x = left + 24.0;
        let name_width = widgets::text_width(ui, &row.title, font(14.0, Weight::Medium));
        widgets::text(
            ui,
            Pos2::new(x, top),
            &row.title,
            font(14.0, Weight::Medium),
            theme::foreground(),
            20.0,
        );
        x += name_width + 8.0;
        let mut badges = Vec::new();
        if script["runOnWorktreeCreate"] == true {
            badges.push("setup");
        }
        if script["previewUrl"].is_string() {
            badges.push("preview · desktop only");
        }
        for badge in badges {
            let width = widgets::text_width(ui, badge, font(11.0, Weight::Regular)) + 12.0;
            let chip = Rect::from_min_size(Pos2::new(x, top + 1.5), Vec2::new(width, 17.0));
            ui.painter().rect_stroke(
                chip,
                CornerRadius::same(4),
                Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
                StrokeKind::Inside,
            );
            widgets::text(
                ui,
                Pos2::new(chip.left() + 6.0, chip.top() + 0.5),
                badge,
                font(11.0, Weight::Regular),
                muted,
                16.0,
            );
            x = chip.right() + 8.0;
        }
        let right = rect.right() - 16.0;
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, top + 24.0),
                Pos2::new(right - 120.0, top + 42.0),
            ),
            &row.description,
            theme::mono(12.0),
            theme::alpha(muted, 0.8),
        );
        let center = rect.center().y + if divided { 0.5 } else { 0.0 };
        let gear = Rect::from_min_size(Pos2::new(right - 24.0, center - 12.0), Vec2::splat(24.0));
        let mut shortcut_right = gear.left() - 8.0;
        let hovered = ui.rect_contains_pointer(rect);
        let clicked = if hovered {
            widgets::icon_button(
                ui,
                gear,
                ("action-edit", script_id),
                "settings",
                14.0,
                muted,
                "",
            )
            .clicked()
        } else {
            false
        };
        if !hovered {
            shortcut_right = gear.left() - 8.0;
        }
        if let Some(label) =
            crate::ui::action_editor::script_command(script_id).and_then(|command| {
                crate::ui::action_editor::shortcut_label_for(
                    &self.state.config["keybindings"],
                    &command,
                )
            })
        {
            let width = widgets::text_width(ui, &label, font(12.0, Weight::Regular));
            widgets::text(
                ui,
                Pos2::new(shortcut_right - width, center - 8.0),
                &label,
                font(12.0, Weight::Regular),
                muted,
                16.0,
            );
        }
        clicked
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
            ("project-icon", RowAction::Select(button)) if button == "Choose icon" => {
                self.open_icon_picker(project);
            }
            ("project-icon", RowAction::Select(_)) => {
                self.toggle_palette(View::Favicon(project.id.clone()));
            }
            ("project-actions", RowAction::Click) => self.open_action_editor(&project.id, None),
            (id, RowAction::Click) if id.starts_with("project-action:") => {
                let script_id = &id["project-action:".len()..];
                let script = self
                    .project_scripts(&project.id)
                    .into_iter()
                    .find(|s| s["id"] == script_id);
                if let Some(script) = script {
                    self.open_action_editor(&project.id, Some(&script));
                }
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
        project.project_icon = Some(
            json!({"kind": "lucide", "name": "folder-code", "color": "pink", "monogramText": "DX"}),
        );
        assert_eq!(icon_description(&project), "DX · pink");
    }
}
