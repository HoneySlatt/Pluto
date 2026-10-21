//! The settings scope: "Applying settings for <project> across <environment>"
//! and what it changes. Ported from `components/settings/SettingsScopeSentence.tsx`,
//! `settingsScope.ts`, `settingsScopeAxis.ts`, `scopedSettings.ts` and
//! `packages/shared/src/projectSettings.ts`.
//!
//! At a project scope, rows of project-scopable server keys read the project's
//! effective value and write its override entry (`projectSettingsOverrides`);
//! their reset arrow clears the override. Other server rows stay visible but
//! inert ("Environment-wide setting"). Client rows are unaffected.
//!
//! This client talks to one environment, so the environment axis only
//! switches between "across All environments" and "on <machine>".

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Map, Value, json};

use crate::app::App;
use crate::model::Project;
use crate::theme::{self, Weight, font};
use crate::ui::{project_file, project_icon, widgets};

/// `PROJECT_SCOPED_SERVER_SETTING_KEYS`.
pub(crate) const PROJECT_SCOPED_KEYS: [&str; 18] = [
    "worktreeCleanup",
    "defaultModelSelection",
    "defaultRuntimeMode",
    "defaultThreadEnvMode",
    "newWorktreesStartFromOrigin",
    "worktreeSubmodules",
    "defaultAutoPull",
    "defaultProjectScripts",
    "enableAgentBrowserAccess",
    "enableAgentDeviceAccess",
    "textGenerationModelSelection",
    "sourceControlWriterModelSelection",
    "sourceControlWritingStyle",
    "pullRequestMergeMethod",
    "sidebarAutoSettleOnMerge",
    "sidebarAutoSettleAfterDays",
    "continueThreadsAfterServerUpdate",
    "responseStreamingMode",
];

/// `NULLABLE_PROJECT_SETTINGS_OVERRIDES`: overrides that may store null.
const NULLABLE_OVERRIDES: [&str; 4] = [
    "defaultModelSelection",
    "sourceControlWriterModelSelection",
    "pullRequestMergeMethod",
    "sidebarAutoSettleAfterDays",
];

pub(crate) fn is_project_scoped(key: &str) -> bool {
    PROJECT_SCOPED_KEYS.contains(&key)
}

/// The server keys behind a settings row (the rows' `settingKeys`), for
/// the project-scope rules. Rows not listed are client rows.
pub(crate) fn row_server_keys(id: &str) -> &'static [&'static str] {
    match id {
        "default-model" => &["defaultModelSelection"],
        "default-permissions" => &["defaultRuntimeMode"],
        "default-workspace" => &["defaultThreadEnvMode"],
        "worktree-submodules" => &["worktreeSubmodules"],
        "auto-settle-merged-threads" => &["sidebarAutoSettleOnMerge"],
        "auto-settle-inactive-threads" | "days-before-auto-settle" => {
            &["sidebarAutoSettleAfterDays"]
        }
        "response-streaming" => &["responseStreamingMode"],
        "provider-update-checks" => &["enableProviderUpdateChecks"],
        "continue-threads-after-server-update" => &["continueThreadsAfterServerUpdate"],
        "background-activity" => &["backgroundActivity"],
        "start-from-origin" => &["newWorktreesStartFromOrigin"],
        "add-project-starts-in" => &["addProjectBaseDirectory"],
        "text-generation-model" => &["textGenerationModelSelection"],
        "automatic-pull" => &["defaultAutoPull"],
        "pull-request-merge-method" => &["pullRequestMergeMethod"],
        "source-control-writing-style" | "follow-change-request-templates" => {
            &["sourceControlWritingStyle"]
        }
        "source-control-writer-model" => &["sourceControlWriterModelSelection"],
        "project-actions" => &["defaultProjectScripts"],
        "agent-browser-access" => &["enableAgentBrowserAccess"],
        "agent-device-access" => &["enableAgentDeviceAccess"],
        "device-hub" => &["enableDeviceSupport"],
        // Storage's retention and worktree rules (`storageCleanup`); the
        // project-scope worktree mode of `StorageSettings` is not ported.
        "worktreeOnDelete"
        | "worktreeAfterDays"
        | "worktreeOnMerge"
        | "worktreeUnchanged"
        | "browserArtifactsAfterDays"
        | "logsAfterDays" => &["storageCleanup"],
        _ => &[],
    }
}

/// The selected scope (`SettingsScopeSearch`): a project id and whether the
/// environment axis names the machine.
#[derive(Default, Clone)]
pub(crate) struct SettingsScope {
    pub(crate) project: Option<String>,
    pub(crate) machine: bool,
}

const PROJECT_MENU: &str = "settings-scope-project";
const ENVIRONMENT_MENU: &str = "settings-scope-environment";

impl App {
    /// The project of a project scope, while it exists.
    pub(crate) fn settings_project(&self) -> Option<Project> {
        self.settings_scope
            .project
            .as_deref()
            .and_then(|id| self.state.project(id))
            .cloned()
    }

    pub(crate) fn is_project_scope(&self) -> bool {
        self.settings_project().is_some()
    }

    /// `resolveProjectSettings`: the environment settings with the scoped
    /// project's overrides applied (a model on a disabled provider falls
    /// back to the environment value); once the repository's t3.json is
    /// read, file-backed keys left unset take its value, else the built-in.
    pub(crate) fn scoped_settings(&self) -> Value {
        let settings = self.state.config["settings"].clone();
        let Some(project) = self.settings_project() else {
            return settings;
        };
        let empty = Map::new();
        let entries = settings["projectSettingsOverrides"][&project.id]
            .as_object()
            .unwrap_or(&empty);
        let mut effective = settings.clone();
        for key in PROJECT_SCOPED_KEYS {
            let Some(value) = entries.get(key) else {
                continue;
            };
            if matches!(
                key,
                "textGenerationModelSelection" | "defaultModelSelection"
            ) && !value.is_null()
                && !self.model_provider_enabled(value)
            {
                continue;
            }
            effective[key] = value.clone();
        }
        if let Some(file) = self.project_file(&project.workspace_root) {
            for key in PROJECT_SCOPED_KEYS {
                let Some((field, builtin)) = project_file::file_backed(key) else {
                    continue;
                };
                if effective[key].is_null() {
                    effective[key] = file
                        .as_ref()
                        .and_then(|f| f.get(field).cloned())
                        .unwrap_or_else(|| Value::from(builtin));
                }
            }
        }
        effective
    }

    /// Whether `key`'s effective value comes from the repository's t3.json
    /// (`resolveProjectFileBackedSetting`'s "t3.json" source).
    pub(crate) fn project_file_source(&self, key: &str) -> bool {
        let Some(project) = self.settings_project() else {
            return false;
        };
        let Some((field, _)) = project_file::file_backed(key) else {
            return false;
        };
        let settings = &self.state.config["settings"];
        let set = |value: Option<&Value>| value.is_some_and(|v| !v.is_null());
        !set(settings["projectSettingsOverrides"][&project.id].get(key))
            && !set(settings.get(key))
            && self
                .project_file(&project.workspace_root)
                .flatten()
                .is_some_and(|file| file.get(field).is_some())
    }

    /// `isModelSelectionProviderEnabled`.
    fn model_provider_enabled(&self, selection: &Value) -> bool {
        let id = selection["instanceId"].as_str().unwrap_or_default();
        self.providers()
            .iter()
            .find(|p| p["instanceId"] == id)
            .is_none_or(|p| p["enabled"] != false)
    }

    /// Whether `key`'s effective value comes from the scoped project's
    /// override (`resolveProjectSettings`' "project" source).
    pub(crate) fn project_source(&self, key: &str) -> bool {
        let Some(project) = self.settings_project() else {
            return false;
        };
        let entry = &self.state.config["settings"]["projectSettingsOverrides"][&project.id];
        entry.get(key).is_some_and(|value| {
            !matches!(
                key,
                "textGenerationModelSelection" | "defaultModelSelection"
            ) || value.is_null()
                || self.model_provider_enabled(value)
        })
    }

    /// Whether the scoped project overrides any of `keys`.
    pub(crate) fn project_overrides(&self, keys: &[&str]) -> bool {
        let Some(project) = self.settings_project() else {
            return false;
        };
        let entry = &self.state.config["settings"]["projectSettingsOverrides"][&project.id];
        keys.iter().any(|key| entry.get(*key).is_some())
    }

    /// Writes `value` into the scoped project's override entry
    /// (`planScopedSettingsPatch` at a project scope): a picker's null removes
    /// the override where it cannot store null, and object values complete
    /// from the effective value.
    pub(crate) fn write_project_override(&mut self, key: &str, value: Value) {
        let Some(project) = self.settings_project() else {
            return;
        };
        let effective = self.scoped_settings();
        let mut entry = self.state.config["settings"]["projectSettingsOverrides"][&project.id]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if value.is_null() && !NULLABLE_OVERRIDES.contains(&key) {
            entry.remove(key);
        } else if let (Some(patch), Some(base)) = (value.as_object(), effective[key].as_object()) {
            let mut merged = base.clone();
            merged.extend(patch.clone());
            entry.insert(key.to_owned(), Value::Object(merged));
        } else {
            entry.insert(key.to_owned(), value);
        }
        self.save_project_overrides(&project.id, entry);
    }

    /// `clearProjectSettingsOverrides`: the keys follow the environment again.
    pub(crate) fn clear_project_overrides(&mut self, keys: &[&str]) {
        let Some(project) = self.settings_project() else {
            return;
        };
        // Resetting the actions drops the shortcuts of actions that go away
        // (`persist(() => null)`), unless another list still uses them.
        if keys.contains(&"defaultProjectScripts") {
            let settings = &self.state.config["settings"];
            let ids = |list: &Value| -> Vec<String> {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s["id"].as_str().map(str::to_owned))
                    .collect()
            };
            let current =
                ids(&settings["projectSettingsOverrides"][&project.id]["defaultProjectScripts"]);
            let defaults = ids(&settings["defaultProjectScripts"]);
            let others: Vec<String> = settings["projectSettingsOverrides"]
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(id, _)| **id != project.id)
                .flat_map(|(_, entry)| ids(&entry["defaultProjectScripts"]))
                .collect();
            let bindings = self.state.config["keybindings"].clone();
            for id in current
                .iter()
                .filter(|id| !defaults.contains(id) && !others.contains(id))
            {
                if let Some(command) = crate::ui::action_editor::script_command(id)
                    && let Some(key) = crate::ui::action_editor::keybinding_for(&bindings, &command)
                {
                    self.send_request(
                        "keybinding-remove".into(),
                        "server.removeKeybinding",
                        json!({"key": key, "command": command}),
                    );
                }
            }
        }
        let mut entry = self.state.config["settings"]["projectSettingsOverrides"][&project.id]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for key in keys {
            entry.remove(*key);
        }
        self.save_project_overrides(&project.id, entry);
    }

    /// `planProjectOverridesClear`: drops `keys` from the named projects'
    /// entries (no side effects, unlike a project scope's reset).
    pub(crate) fn clear_overrides_of(&mut self, project_ids: &[String], keys: &[&str]) {
        for project_id in project_ids {
            let mut entry = self.state.config["settings"]["projectSettingsOverrides"][project_id]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for key in keys {
                entry.remove(*key);
            }
            self.save_project_overrides(project_id, entry);
        }
    }

    /// Replaces the project's entry (the server replaces entries rather than
    /// merging them); an empty entry is dropped.
    pub(crate) fn save_project_overrides(&mut self, project_id: &str, entry: Map<String, Value>) {
        let value = if entry.is_empty() {
            Value::Null
        } else {
            Value::Object(entry)
        };
        let settings = &mut self.state.config["settings"];
        if !settings["projectSettingsOverrides"].is_object() {
            settings["projectSettingsOverrides"] = json!({});
        }
        if let Some(all) = settings["projectSettingsOverrides"].as_object_mut() {
            if value.is_null() {
                all.remove(project_id);
            } else {
                all.insert(project_id.to_owned(), value.clone());
            }
        }
        self.send_request(
            "settings:projectSettingsOverrides".into(),
            "server.updateSettings",
            json!({"patch": {"projectSettingsOverrides": {project_id: value}}}),
        );
    }

    /// "Applying settings for <project ▾> across <environments ▾>"; returns
    /// the top of the page content.
    pub(crate) fn scope_sentence(&mut self, ui: &mut Ui, left: f32, top: f32) -> f32 {
        let y = top + 23.0;
        let mut x = left + 16.0;
        let muted = theme::muted_foreground();
        let text = |ui: &Ui, x: &mut f32, value: &str, weight: Weight, color| {
            let width = widgets::text_width(ui, value, font(16.0, weight));
            widgets::text(ui, Pos2::new(*x, y), value, font(16.0, weight), color, 24.0);
            *x += width;
        };
        text(ui, &mut x, "Applying settings for", Weight::Regular, muted);
        x += 6.0;
        let project = self.settings_project();
        let project_label = project
            .as_ref()
            .map_or("All projects".to_owned(), |p| p.title.clone());
        let project_trigger = self.scope_trigger(
            ui,
            &mut x,
            y,
            PROJECT_MENU,
            &project_label,
            project.as_ref(),
            None,
        );
        x += 6.0;
        let machine = self.settings_scope.machine;
        text(
            ui,
            &mut x,
            if machine { "on" } else { "across" },
            Weight::Regular,
            muted,
        );
        x += 6.0;
        let environment = self.state.config["environment"]["label"]
            .as_str()
            .unwrap_or("This machine")
            .to_owned();
        let environment_label = if machine {
            environment.clone()
        } else {
            "All environments".to_owned()
        };
        let environment_trigger = self.scope_trigger(
            ui,
            &mut x,
            y,
            ENVIRONMENT_MENU,
            &environment_label,
            None,
            machine.then_some("monitor"),
        );

        // The project menu: All projects, then the projects.
        // `useSettingsProjectGroups`: sorted by display name (`localeCompare`).
        let mut projects = self.state.projects.clone();
        projects.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.title.cmp(&b.title))
        });
        let mut items: Vec<MenuRow> = vec![MenuRow::Item {
            label: "All projects".into(),
            icon: Icon::None,
            checked: project.is_none(),
        }];
        items.push(MenuRow::Separator);
        for candidate in &projects {
            items.push(MenuRow::Item {
                label: candidate.title.clone(),
                icon: Icon::Project(Box::new(candidate.clone())),
                checked: project.as_ref().is_some_and(|p| p.id == candidate.id),
            });
        }
        if let Some(index) = scope_menu(
            ui,
            &mut self.open_menu,
            PROJECT_MENU,
            project_trigger,
            &items,
        ) {
            // `selectProjectAxis` keeps the environment axis.
            self.settings_scope.project = match index {
                0 => None,
                index => projects.get(index - 2).map(|p| p.id.clone()),
            };
        }
        let items = vec![
            MenuRow::Item {
                label: "All environments".into(),
                icon: Icon::Named("layers"),
                checked: !machine,
            },
            MenuRow::Separator,
            MenuRow::Item {
                label: environment,
                icon: Icon::Named("monitor"),
                checked: machine,
            },
        ];
        if let Some(index) = scope_menu(
            ui,
            &mut self.open_menu,
            ENVIRONMENT_MENU,
            environment_trigger,
            &items,
        ) {
            self.settings_scope.machine = index == 2;
        }
        y + 25.0 + 32.0
    }

    /// An `InlineButton tone="picker"`: optional icon, label, chevron;
    /// toggles its menu and returns its rect.
    #[allow(clippy::too_many_arguments)]
    fn scope_trigger(
        &mut self,
        ui: &Ui,
        x: &mut f32,
        y: f32,
        menu: &str,
        label: &str,
        project: Option<&Project>,
        icon: Option<&str>,
    ) -> Rect {
        let start = *x;
        if let Some(project) = project {
            project_icon::paint(
                ui,
                Rect::from_min_size(Pos2::new(*x, y + 5.0), Vec2::splat(14.0)),
                project,
            );
            *x += 20.0;
        } else if let Some(icon) = icon {
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(*x, y + 5.0), Vec2::splat(14.0)),
                14.0,
                theme::foreground(),
            );
            *x += 20.0;
        }
        let width = widgets::text_width(ui, label, font(16.0, Weight::Medium));
        widgets::text(
            ui,
            Pos2::new(*x, y),
            label,
            font(16.0, Weight::Medium),
            theme::foreground(),
            24.0,
        );
        let open = self.open_menu.as_deref() == Some(menu);
        let rect = Rect::from_min_max(Pos2::new(start, y), Pos2::new(*x + width + 20.0, y + 24.0));
        let response = ui.interact(rect, ui.id().with(("scope-trigger", menu)), Sense::click());
        // `tone="picker"`: a dotted `foreground/30` underline, solid while
        // hovered or open.
        if open || response.hovered() {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(*x, y + 22.0), Vec2::new(width, 1.0)),
                0.0,
                theme::foreground(),
            );
        } else {
            // 1px dots every 2px, as an unfeathered mesh on the pixel grid
            // (epaint feathers 1px shapes away).
            let mut mesh = eframe::egui::Mesh::default();
            let color = theme::alpha(theme::foreground(), 0.3);
            let mut dot = x.round();
            while dot < *x + width {
                mesh.add_colored_rect(
                    Rect::from_min_size(Pos2::new(dot, (y + 22.0).round()), Vec2::splat(1.0)),
                    color,
                );
                dot += 2.0;
            }
            ui.painter().add(eframe::egui::Shape::mesh(mesh));
        }
        *x += width;
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(Pos2::new(*x + 6.0, y + 5.0), Vec2::splat(14.0)),
            14.0,
            theme::muted_foreground(),
        );
        *x += 20.0;
        if response.clicked() {
            self.open_menu = if open { None } else { Some(menu.to_owned()) };
        }
        rect
    }
}

enum Icon {
    None,
    Named(&'static str),
    Project(Box<Project>),
}

enum MenuRow {
    Item {
        label: String,
        icon: Icon,
        checked: bool,
    },
    Separator,
}

/// Where an item's label starts: `px-2`, then an icon and `gap-2` (menu
/// SVG icons take `-mx-0.5`; the project favicon does not).
fn label_offset(icon: &Icon) -> f32 {
    match icon {
        Icon::None => 8.0,
        Icon::Named(_) => 26.0,
        Icon::Project(_) => 30.0,
    }
}

/// `MenuPopup` with `MenuRadioItem`s (checked one tinted, its check at the
/// end); returns the clicked row's index.
fn scope_menu(
    ui: &mut Ui,
    open: &mut Option<String>,
    id: &str,
    anchor: Rect,
    rows: &[MenuRow],
) -> Option<usize> {
    if open.as_deref() != Some(id) {
        return None;
    }
    let label_font = font(14.0, Weight::Regular);
    let content = rows
        .iter()
        .map(|row| match row {
            MenuRow::Item { label, icon, .. } => {
                widgets::text_width(ui, label, label_font.clone()) + label_offset(icon)
            }
            MenuRow::Separator => 0.0,
        })
        .fold(0.0, f32::max);
    // The item: label offset and label, `gap-2`, the check, `pe-2`.
    let width = (content + 8.0 + 16.0 + 8.0 + 10.0).max(128.0).ceil();
    let height = 2.0
        + 8.0
        + rows
            .iter()
            .map(|row| {
                if matches!(row, MenuRow::Separator) {
                    9.0
                } else {
                    28.0
                }
            })
            .sum::<f32>();
    let rect = Rect::from_min_size(
        Pos2::new(anchor.left(), anchor.bottom() + 5.0),
        Vec2::new(width, height),
    );
    let mut chosen = None;
    widgets::popup(ui, open, id, rect, anchor, |ui, inner| {
        let mut y = inner.top() + 5.0;
        for (index, row) in rows.iter().enumerate() {
            match row {
                MenuRow::Separator => {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(inner.left() + 13.0, y + 4.0),
                            Vec2::new(inner.width() - 26.0, 1.0),
                        ),
                        0.0,
                        theme::overlay(0.06),
                    );
                    y += 9.0;
                }
                MenuRow::Item {
                    label,
                    icon,
                    checked,
                } => {
                    let item = Rect::from_min_size(
                        Pos2::new(inner.left() + 5.0, y),
                        Vec2::new(inner.width() - 10.0, 28.0),
                    );
                    let response = ui.interact(
                        item,
                        ui.id().with(("scope-menu", id, index)),
                        Sense::click(),
                    );
                    if response.hovered() || *checked {
                        ui.painter()
                            .rect_filled(item, CornerRadius::same(6), theme::overlay(0.08));
                    }
                    let x = item.left() + label_offset(icon);
                    let icon_rect = |left: f32| {
                        Rect::from_min_size(
                            Pos2::new(item.left() + left, item.center().y - 7.0),
                            Vec2::splat(14.0),
                        )
                    };
                    match icon {
                        Icon::None => {}
                        Icon::Named(name) => {
                            theme::paint_icon(ui, name, icon_rect(6.0), 14.0, theme::foreground());
                        }
                        Icon::Project(project) => {
                            project_icon::paint(ui, icon_rect(8.0), project);
                        }
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(x, item.top()),
                            Pos2::new(item.right() - 28.0, item.bottom()),
                        ),
                        label,
                        label_font.clone(),
                        theme::foreground(),
                    );
                    if *checked {
                        theme::paint_icon(
                            ui,
                            "check",
                            Rect::from_min_size(
                                Pos2::new(item.right() - 24.0, item.center().y - 8.0),
                                Vec2::splat(16.0),
                            ),
                            16.0,
                            theme::foreground(),
                        );
                    }
                    if response.clicked() {
                        chosen = Some(index);
                    }
                    y += 28.0;
                }
            }
        }
    });
    if chosen.is_some() {
        *open = None;
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_keys_cover_scoped_rows() {
        assert!(is_project_scoped("defaultRuntimeMode"));
        assert!(!is_project_scoped("backgroundActivity"));
        assert_eq!(
            row_server_keys("days-before-auto-settle"),
            &["sidebarAutoSettleAfterDays"]
        );
        assert!(row_server_keys("project-grouping").is_empty());
    }
}
