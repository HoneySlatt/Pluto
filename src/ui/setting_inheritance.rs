//! The settings rows' inheritance indicator (`settings/SettingInheritance.tsx`
//! and the `inheritance` summary of `settingsLayout.tsx`): the layers button
//! beside a server row's title, its summary tooltip, and the popover showing
//! where the value comes from, top-down (the project override at a project
//! scope, the environment, the repository's t3.json for file-backed keys, the
//! built-in default), with the effective layer checked. At the environment
//! scope it lists the projects overriding the row and resets them.
//!
//! The t3.json layer always reads "Inherits": repository files are not read
//! (upstream shows the same until it has read them).

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::settings::default_server_setting;
use crate::ui::settings_scope::is_project_scoped;
use crate::ui::widgets;

const WIDTH: f32 = 320.0;
const ROW: f32 = 28.0;

/// One layer of the chain (`InheritanceLayer`).
#[derive(Debug, PartialEq)]
struct Layer {
    label: &'static str,
    value: String,
    effective: bool,
    set: bool,
}

/// `PROJECT_FILE_BACKED_SETTINGS`: a file-backed key's built-in value.
fn file_backed_builtin(key: &str) -> Option<Value> {
    match key {
        "defaultThreadEnvMode" => Some(Value::from("local")),
        "worktreeSubmodules" => Some(Value::from("recursive")),
        _ => None,
    }
}

/// `formatValue`: the human label of a value in the chain.
fn format_value(key: &str, value: &Value) -> String {
    match value {
        Value::Null => match key {
            "pullRequestMergeMethod" => "Last selected",
            "sidebarAutoSettleAfterDays" => "Never",
            "defaultModelSelection" => "Automatic",
            "sourceControlWriterModelSelection" => "Text generation model",
            "defaultThreadEnvMode" | "worktreeSubmodules" => "Inherit",
            _ => "Not set",
        }
        .to_owned(),
        Value::Bool(on) => if *on { "On" } else { "Off" }.to_owned(),
        Value::Number(n) if key == "sidebarAutoSettleAfterDays" => {
            let days = n.as_f64().unwrap_or_default();
            format!("{n} {}", if days == 1.0 { "day" } else { "days" })
        }
        Value::Number(n) => n.to_string(),
        Value::String(s) => match (key, s.as_str()) {
            ("defaultThreadEnvMode", "worktree") => "New worktree".to_owned(),
            ("defaultThreadEnvMode", "local") => "Current checkout".to_owned(),
            ("worktreeSubmodules", "recursive") => "Recursive".to_owned(),
            ("worktreeSubmodules", "top-level") => "Top level only".to_owned(),
            ("worktreeSubmodules", "none") => "Skip".to_owned(),
            ("pullRequestMergeMethod", "merge") => "Merge".to_owned(),
            ("pullRequestMergeMethod", "squash") => "Squash and merge".to_owned(),
            ("pullRequestMergeMethod", "rebase") => "Rebase and merge".to_owned(),
            (_, "") => "Empty".to_owned(),
            _ => s.clone(),
        },
        Value::Array(items) => format!(
            "{} {}",
            items.len(),
            if items.len() == 1 { "item" } else { "items" }
        ),
        Value::Object(object) => {
            if let Some(model) = object.get("model").and_then(Value::as_str) {
                model.to_owned()
            } else if let Some(mode) = object.get("mode").and_then(Value::as_str) {
                match mode {
                    "repo_conventions" => "Repository conventions",
                    "conventional_commits" => "Conventional Commits",
                    "custom" => "Custom instructions",
                    other => other,
                }
                .to_owned()
            } else {
                "Custom".to_owned()
            }
        }
    }
}

/// `settingInheritanceLayers` for one target: `project` is whether the
/// target is a project, `from_project` whether its value is the project's
/// own, `value` its effective value.
fn layers(
    key: &str,
    project: bool,
    from_project: bool,
    value: &Value,
    environment: &Value,
) -> Vec<Layer> {
    let scoped = is_project_scoped(key);
    let from_project = scoped && from_project;
    let environment_set = *environment != default_server_setting(key);
    let builtin = file_backed_builtin(key);
    let mut layers = Vec::new();
    if project && scoped {
        layers.push(Layer {
            label: "Project",
            value: if from_project {
                format_value(key, value)
            } else {
                "Inherits".to_owned()
            },
            effective: from_project,
            set: from_project,
        });
    }
    layers.push(Layer {
        label: "Environment",
        value: if environment_set {
            format_value(key, environment)
        } else {
            "Inherits".to_owned()
        },
        effective: !from_project && environment_set,
        set: environment_set,
    });
    if builtin.is_some() && project {
        layers.push(Layer {
            label: "t3.json",
            value: "Inherits".to_owned(),
            effective: false,
            set: false,
        });
    }
    layers.push(Layer {
        label: "Default",
        value: format_value(key, &builtin.unwrap_or_else(|| default_server_setting(key))),
        effective: !from_project && !environment_set,
        set: true,
    });
    layers
}

/// `EnvironmentMachineIcon` for the kinds drawn with Lucide glyphs; the
/// Linux and Mac kinds (custom upstream glyphs) show the monitor.
fn machine_icon(config: &Value) -> &'static str {
    let kind = config["settings"]["environmentIcon"]
        .as_str()
        .or(config["environment"]["platform"]["machine"].as_str())
        .unwrap_or("server");
    match kind {
        "server" => "server",
        "cloud" => "cloud",
        "laptop" => "laptop",
        _ => "monitor",
    }
}

/// An `InlineButton`: medium text that underlines on hover.
fn inline_button(ui: &mut Ui, pos: Pos2, id: impl egui::AsIdSalt, label: &str, size: f32) -> bool {
    let label_font = font(size, Weight::Medium);
    let line = widgets::line_height_for(size);
    let width = widgets::text_width(ui, label, label_font.clone());
    let rect = Rect::from_min_size(pos, Vec2::new(width, line));
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    widgets::text_in(
        ui,
        rect.expand2(Vec2::new(1.0, 0.0)),
        label,
        label_font,
        theme::foreground(),
    );
    if response.hovered() {
        let y = (rect.center().y + size * 0.5).round() + 0.5;
        ui.painter().line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            egui::Stroke::new(1.0, theme::foreground()),
        );
    }
    response.clicked()
}

impl App {
    /// The layers button at `slot` for a row reading `keys`, and its
    /// popover; `overridden` tints it like the project override state.
    pub(crate) fn setting_inheritance(
        &mut self,
        ui: &mut Ui,
        slot: Rect,
        row_id: &str,
        keys: &[&str],
        overridden: bool,
    ) {
        let Some(&key) = keys.first() else {
            return;
        };
        let project = self.settings_project();
        let settings = self.state.config["settings"].clone();
        let scoped: Vec<&str> = keys
            .iter()
            .copied()
            .filter(|k| is_project_scoped(k))
            .collect();
        let environment_label = self.state.config["environment"]["label"]
            .as_str()
            .unwrap_or("This machine")
            .to_owned();

        // At the environment scope, the projects whose own value hides it.
        let mut overriding: Vec<(String, String)> = Vec::new();
        if project.is_none() && !scoped.is_empty() {
            for (id, entry) in settings["projectSettingsOverrides"]
                .as_object()
                .into_iter()
                .flatten()
            {
                if scoped.iter().any(|k| entry.get(*k).is_some())
                    && let Some(known) = self.state.project(id)
                {
                    overriding.push((id.clone(), known.title.clone()));
                }
            }
        }

        let from_project = project.is_some() && scoped.iter().all(|k| self.project_source(k));
        let customized = keys
            .iter()
            .any(|k| settings[*k] != default_server_setting(k));
        let summary = if project.is_some() && !scoped.is_empty() && from_project {
            "Overridden for this project".to_owned()
        } else if project.is_some() && !scoped.is_empty() {
            format!("Inherited from {environment_label}")
        } else if customized {
            "Set on the environment".to_owned()
        } else {
            "Built-in default".to_owned()
        };
        let summary = match overriding.len() {
            0 => summary,
            1 => format!("{summary} · 1 project override"),
            n => format!("{summary} · {n} project overrides"),
        };

        let menu_id = format!("setting-inheritance:{row_id}");
        let open = self.open_menu.as_deref() == Some(menu_id.as_str());
        let response = ui.interact(slot, ui.id().with(&menu_id), Sense::click());
        let active = response.hovered() || open;
        if active {
            ui.painter()
                .rect_filled(slot, CornerRadius::same(4), theme::accent());
        }
        let tint = if overridden {
            theme::primary()
        } else if active {
            theme::foreground()
        } else {
            theme::muted_foreground()
        };
        theme::paint_icon(ui, "layers", slot, 12.0, tint);
        let response = response.on_hover_text(&summary);
        if response.clicked() {
            self.open_menu = if open { None } else { Some(menu_id.clone()) };
        }
        if self.open_menu.as_deref() != Some(menu_id.as_str()) {
            return;
        }

        let target_value = if project.is_some() {
            self.scoped_settings()[key].clone()
        } else {
            settings[key].clone()
        };
        let chain = layers(
            key,
            project.is_some(),
            project.as_ref().is_some_and(|_| self.project_source(key)),
            &target_value,
            &settings[key],
        );
        let overridden_by = if overriding.is_empty() {
            0.0
        } else {
            8.0 + 1.0 + 8.0 + 16.0 + 2.0 + overriding.len() as f32 * ROW
        };
        let height = 1.0 + 10.0 + 22.0 + chain.len() as f32 * ROW + overridden_by + 10.0 + 1.0;
        let screen = ui.ctx().content_rect();
        let mut top = slot.bottom() + 4.0;
        if top + height > screen.bottom() - 8.0 {
            top = slot.top() - 4.0 - height;
        }
        let rect = Rect::from_min_size(Pos2::new(slot.left(), top), Vec2::new(WIDTH, height));
        let machine = machine_icon(&self.state.config);
        let mut open_project: Option<String> = None;
        let mut reset = false;
        let mut open_menu = self.open_menu.clone();
        widgets::popup(ui, &mut open_menu, &menu_id, rect, slot, |ui, inner| {
            let left = inner.left() + 1.0 + 12.0;
            let right = inner.right() - 1.0 - 12.0;
            let mut y = inner.top() + 1.0 + 10.0;
            let muted = theme::muted_foreground();
            theme::paint_icon(
                ui,
                machine,
                Rect::from_min_size(Pos2::new(left, y + 1.0), Vec2::splat(14.0)),
                14.0,
                muted,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(left + 20.0, y), Pos2::new(right, y + 16.0)),
                &environment_label,
                font(12.0, Weight::Medium),
                muted,
            );
            y += 22.0;
            for layer in &chain {
                let row = Rect::from_min_max(Pos2::new(left, y), Pos2::new(right, y + ROW));
                if layer.effective {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(6),
                        theme::alpha(theme::foreground(), 0.06),
                    );
                }
                let (label_font, label_color) = if layer.effective {
                    (font(14.0, Weight::Medium), theme::foreground())
                } else {
                    (font(14.0, Weight::Regular), muted)
                };
                let value_color: Color32 = if layer.effective {
                    theme::foreground()
                } else if layer.set {
                    muted
                } else {
                    theme::alpha(muted, 0.6)
                };
                let check = Rect::from_min_size(
                    Pos2::new(row.right() - 8.0 - 14.0, row.center().y - 7.0),
                    Vec2::splat(14.0),
                );
                let value_font = font(14.0, Weight::Regular);
                let value_width =
                    widgets::text_width(ui, &layer.value, value_font.clone()).min(128.0);
                let value_left = check.left() - 6.0 - value_width;
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(row.left() + 8.0, row.top()),
                        Pos2::new(value_left - 12.0, row.bottom()),
                    ),
                    layer.label,
                    label_font,
                    label_color,
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(value_left, row.top()),
                        Pos2::new(check.left() - 6.0 + 1.0, row.bottom()),
                    ),
                    &layer.value,
                    value_font,
                    value_color,
                );
                if layer.effective {
                    theme::paint_icon(ui, "check", check, 14.0, theme::primary());
                }
                y += ROW;
            }
            if overriding.is_empty() {
                return;
            }
            y += 8.0;
            ui.painter().rect_filled(
                Rect::from_min_max(Pos2::new(left, y), Pos2::new(right, y + 1.0)),
                0.0,
                theme::alpha(theme::border(), 0.6),
            );
            y += 1.0 + 8.0;
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(left + 8.0, y), Pos2::new(right, y + 16.0)),
                "Overridden by",
                font(12.0, Weight::Regular),
                muted,
            );
            let label = if overriding.len() == 1 {
                "Reset it"
            } else {
                "Reset all"
            };
            let width = widgets::text_width(ui, label, font(12.0, Weight::Medium));
            reset = inline_button(
                ui,
                Pos2::new(right - 8.0 - width, y),
                "inheritance-reset",
                label,
                12.0,
            );
            y += 16.0 + 2.0;
            for (id, title) in &overriding {
                let value = format_value(key, &settings["projectSettingsOverrides"][id][key]);
                let value_font = font(14.0, Weight::Regular);
                let width = widgets::text_width(ui, &value, value_font.clone()).min(128.0);
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(right - 8.0 - width, y),
                        Pos2::new(right - 8.0 + 1.0, y + ROW),
                    ),
                    &value,
                    value_font,
                    muted,
                );
                if inline_button(
                    ui,
                    Pos2::new(left + 8.0, y + 4.0),
                    ("inheritance-project", id),
                    title,
                    14.0,
                ) {
                    open_project = Some(id.clone());
                }
                y += ROW;
            }
        });
        self.open_menu = open_menu;
        if reset {
            let ids: Vec<String> = overriding.iter().map(|(id, _)| id.clone()).collect();
            self.clear_overrides_of(&ids, &scoped);
            self.open_menu = None;
        }
        if let Some(id) = open_project {
            // `selectScope` keeps the environment axis.
            self.settings_scope.project = Some(id);
            self.open_menu = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layers_follow_upstream() {
        let chain = layers(
            "defaultModelSelection",
            true,
            false,
            &Value::Null,
            &Value::Null,
        );
        let summary: Vec<(&str, &str, bool)> = chain
            .iter()
            .map(|l| (l.label, l.value.as_str(), l.effective))
            .collect();
        assert_eq!(
            summary,
            [
                ("Project", "Inherits", false),
                ("Environment", "Inherits", false),
                ("Default", "Automatic", true),
            ]
        );
        let chain = layers(
            "defaultThreadEnvMode",
            true,
            true,
            &json!("worktree"),
            &Value::Null,
        );
        assert_eq!(chain[0].value, "New worktree");
        assert!(chain[0].effective);
        assert_eq!(chain[2].label, "t3.json");
        assert_eq!(chain[3].value, "Current checkout");
        // Environment-wide keys have no project layer.
        let chain = layers(
            "enableProviderUpdateChecks",
            true,
            false,
            &json!(false),
            &json!(false),
        );
        assert_eq!(chain[0].label, "Environment");
        assert_eq!(chain[0].value, "Off");
        assert!(chain[0].effective);
    }

    #[test]
    fn values_are_labelled_like_upstream() {
        assert_eq!(
            format_value("sidebarAutoSettleAfterDays", &json!(1)),
            "1 day"
        );
        assert_eq!(
            format_value("sidebarAutoSettleAfterDays", &json!(3)),
            "3 days"
        );
        assert_eq!(format_value("defaultProjectScripts", &json!([])), "0 items");
        assert_eq!(
            format_value(
                "sourceControlWritingStyle",
                &json!({"mode": "repo_conventions"})
            ),
            "Repository conventions"
        );
        assert_eq!(
            format_value("backgroundActivity", &json!({"profile": "balanced"})),
            "Custom"
        );
        assert_eq!(format_value("addProjectBaseDirectory", &json!("")), "Empty");
    }
}
