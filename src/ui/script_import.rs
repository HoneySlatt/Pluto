//! Importing the actions a repository's t3.json declares
//! (`settings/ProjectActionsSettings.tsx`'s "Import scripts" menu and the
//! "From t3.json" items of `ProjectScriptsControl.tsx`): scripts whose
//! command or name (ignoring case) is not already an action, added as
//! actions without a shortcut (`importFileScript`).

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::action_editor::script_icon;
use crate::ui::widgets;

const MENU_ID: &str = "import-scripts-menu";
const DESCRIPTION: &str = "Add actions declared by this checkout without editing them first.";

/// `importableScripts`: the file's scripts not already among `scripts`.
pub(crate) fn importable(file_scripts: &[Value], scripts: &[Value]) -> Vec<Value> {
    let text =
        |script: &Value, key: &str| script[key].as_str().unwrap_or_default().trim().to_owned();
    file_scripts
        .iter()
        .filter(|file_script| {
            let command = text(file_script, "command");
            let name = text(file_script, "name").to_lowercase();
            !scripts
                .iter()
                .any(|s| text(s, "command") == command || text(s, "name").to_lowercase() == name)
        })
        .cloned()
        .collect()
}

/// A "From t3.json" menu item: the script's icon and name, and the download
/// mark (`MenuShortcut`); returns whether it was clicked.
pub(crate) fn import_item(ui: &mut Ui, item: Rect, index: usize, script: &Value) -> bool {
    let response = ui.interact(item, ui.id().with(("import-item", index)), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(item, CornerRadius::same(6), theme::accent());
    }
    theme::paint_icon(
        ui,
        script_icon(script["icon"].as_str().unwrap_or("play")),
        Rect::from_min_size(
            Pos2::new(item.left() + 6.0, item.center().y - 8.0),
            Vec2::splat(16.0),
        ),
        16.0,
        theme::alpha(theme::foreground(), 0.8),
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(item.left() + 28.0, item.top()),
            Pos2::new(item.right() - 30.0, item.bottom()),
        ),
        script["name"].as_str().unwrap_or_default().trim(),
        font(14.0, Weight::Regular),
        theme::foreground(),
    );
    theme::paint_icon(
        ui,
        "download",
        Rect::from_min_size(
            Pos2::new(item.right() - 8.0 - 14.0, item.center().y - 7.0),
            Vec2::splat(14.0),
        ),
        14.0,
        theme::muted_foreground(),
    );
    response.clicked()
}

/// The width a "From t3.json" item needs.
pub(crate) fn import_item_width(ui: &Ui, script: &Value) -> f32 {
    let name = script["name"].as_str().unwrap_or_default().trim();
    28.0 + widgets::text_width(ui, name, font(14.0, Weight::Regular)) + 8.0 + 14.0 + 8.0
}

impl App {
    /// The project's t3.json scripts that can be imported (asks for the file).
    pub(crate) fn importable_scripts(&mut self, project_id: &str) -> Vec<Value> {
        let Some(root) = self
            .state
            .project(project_id)
            .map(|p| p.workspace_root.clone())
        else {
            return Vec::new();
        };
        self.request_project_file(&root);
        let file_scripts = self
            .project_file(&root)
            .flatten()
            .and_then(|file| file["scripts"].as_array().cloned())
            .unwrap_or_default();
        importable(&file_scripts, &self.project_scripts(project_id))
    }

    /// The Actions row's "Import scripts" button ending at `right` (a ghost
    /// `xs` button with a chevron) and its menu of the importable scripts,
    /// each with its command in mono.
    pub(crate) fn import_scripts_control(
        &mut self,
        ui: &mut Ui,
        right: f32,
        center_y: f32,
        project_id: &str,
        scripts: &[Value],
    ) {
        let label_font = font(12.0, Weight::Medium);
        let label = "Import scripts";
        let width = 8.0 + widgets::text_width(ui, label, label_font.clone()) + 1.0 + 14.0 + 6.0;
        let button = Rect::from_min_size(
            Pos2::new(right - width, center_y - 12.0),
            Vec2::new(width.round(), 24.0),
        );
        let response = ui.interact(button, ui.id().with(MENU_ID), Sense::click());
        let open = self.open_menu.as_deref() == Some(MENU_ID);
        if response.hovered() || open {
            ui.painter()
                .rect_filled(button, CornerRadius::same(8), theme::accent());
        }
        let text_width = widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(button.left() + 8.0, button.top()), button.max),
            label,
            label_font,
            theme::foreground(),
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(button.left() + 8.0 + text_width + 1.0, button.top() + 5.0),
                Vec2::splat(14.0),
            ),
            14.0,
            theme::muted_foreground(),
        );
        if response.clicked() {
            self.open_menu = if open { None } else { Some(MENU_ID.to_owned()) };
        }
        if self.open_menu.as_deref() != Some(MENU_ID) {
            return;
        }

        let body = font(14.0, Weight::Regular);
        let mono = theme::mono(14.0);
        let content = scripts
            .iter()
            .map(|s| {
                let name = s["name"].as_str().unwrap_or_default().trim();
                let command = s["command"].as_str().unwrap_or_default().trim();
                28.0 + widgets::text_width(ui, name, font(14.0, Weight::Medium))
                    .max(widgets::text_width(ui, command, mono.clone()))
                    + 8.0
            })
            .fold(
                widgets::text_width(ui, DESCRIPTION, body.clone()) + 16.0,
                f32::max,
            );
        let width = (2.0 + 8.0 + content).ceil();
        let height = 1.0 + 4.0 + 28.0 + 28.0 + 9.0 + scripts.len() as f32 * 48.0 + 4.0 + 1.0;
        let rect = Rect::from_min_size(
            Pos2::new(button.right() - width, button.bottom() + 4.0),
            Vec2::new(width, height),
        );
        let mut picked: Option<usize> = None;
        let mut open_menu = self.open_menu.clone();
        widgets::popup(ui, &mut open_menu, MENU_ID, rect, button, |ui, inner| {
            let left = inner.left() + 5.0;
            let right = inner.right() - 5.0;
            let muted = theme::muted_foreground();
            let mut y = inner.top() + 5.0;
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(left + 8.0, y), Pos2::new(right, y + 28.0)),
                "Import from t3.json",
                font(12.0, Weight::Medium),
                muted,
            );
            y += 28.0;
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(left + 8.0, y), Pos2::new(right, y + 20.0)),
                DESCRIPTION,
                body.clone(),
                muted,
            );
            y += 28.0 + 4.0;
            ui.painter().rect_filled(
                Rect::from_min_max(Pos2::new(left + 8.0, y), Pos2::new(right - 8.0, y + 1.0)),
                0.0,
                theme::border(),
            );
            y += 5.0;
            for (index, script) in scripts.iter().enumerate() {
                let item = Rect::from_min_max(Pos2::new(left, y), Pos2::new(right, y + 48.0));
                let response =
                    ui.interact(item, ui.id().with(("import-script", index)), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(item, CornerRadius::same(6), theme::accent());
                }
                theme::paint_icon(
                    ui,
                    script_icon(script["icon"].as_str().unwrap_or("play")),
                    Rect::from_min_size(
                        Pos2::new(item.left() + 8.0, item.center().y - 8.0),
                        Vec2::splat(16.0),
                    ),
                    16.0,
                    theme::alpha(theme::foreground(), 0.8),
                );
                let text_left = item.left() + 28.0;
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(text_left, item.top() + 4.0),
                        Pos2::new(item.right() - 8.0, item.top() + 24.0),
                    ),
                    script["name"].as_str().unwrap_or_default().trim(),
                    font(14.0, Weight::Medium),
                    theme::foreground(),
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(text_left, item.top() + 24.0),
                        Pos2::new(item.right() - 8.0, item.top() + 44.0),
                    ),
                    script["command"].as_str().unwrap_or_default().trim(),
                    mono.clone(),
                    muted,
                );
                if response.clicked() {
                    picked = Some(index);
                }
                y += 48.0;
            }
        });
        self.open_menu = open_menu;
        if let Some(index) = picked {
            self.open_menu = None;
            self.import_file_script(project_id, &scripts[index]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn existing_actions_are_not_offered_again() {
        let file = vec![
            json!({"name": "Test", "command": "cargo test"}),
            json!({"name": "Lint", "command": "cargo clippy"}),
            json!({"name": "Build", "command": "cargo build"}),
        ];
        let scripts = vec![
            json!({"id": "check", "name": "Check", "command": "cargo test"}),
            json!({"id": "lint", "name": "lint", "command": "other"}),
        ];
        let offered = importable(&file, &scripts);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0]["name"], "Build");
    }
}
