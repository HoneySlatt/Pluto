//! Projects (`#projects`), a project (`#project`, after the desktop's
//! `projects_page.rs`): its threads, its New threads defaults
//! (`projectSettingsOverrides`) with their choice sheets, and its removal
//! (`removeMembers`'s confirmation, `project.delete`).

use serde_json::{Value, json};

use crate::badge::{Badge, badge};
use crate::session::{Session, last_activity};
use crate::{App, Core, catalog};

use crate::catalog::WORKSPACES;

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectRow {
    pub badge: Badge,
    pub root: String,
    pub threads: u32,
    /// Threads waiting on an approval or an answer.
    pub waiting: u32,
    pub working: u32,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectThread {
    pub id: String,
    pub title: String,
    /// `branch · state`.
    pub line: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectView {
    pub badge: Badge,
    pub root: String,
    pub threads: Vec<ProjectThread>,
    /// The New threads defaults' labels ("Default" when not overridden).
    pub model: String,
    pub model_mark: Option<String>,
    pub access: String,
    pub workspace: String,
}

/// A choice of a default: its value, label and detail.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DefaultOption {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub checked: bool,
}

/// A project's New threads default sheet: Default first, then the
/// options, or the models by provider.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ProjectSettingView {
    /// No override: Default is checked.
    pub default: bool,
    pub options: Vec<DefaultOption>,
    pub groups: Vec<catalog::ModelGroup>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RemoveProjectView {
    pub title: String,
    /// The confirmation's lines, the question first.
    pub lines: Vec<String>,
}

/// A project's `projectSettingsOverrides` entry.
pub fn overrides(settings: &Value, project: &str) -> Value {
    let value = &settings["projectSettingsOverrides"][project];
    if value.is_object() {
        value.clone()
    } else {
        json!({})
    }
}

fn label_of(value: &str) -> &'static str {
    WORKSPACES
        .iter()
        .find(|o| o.0 == value)
        .map(|o| o.1)
        .unwrap_or(WORKSPACES[0].1)
}

/// The code models by provider, `current` checked (`ModelList`).
pub fn model_groups(
    session: &Session,
    current: Option<&pluto_core::model::ModelSelection>,
) -> Vec<catalog::ModelGroup> {
    catalog::model_groups(session.providers(), false, current)
}

/// Writes the project's override `key` (or removes it for Default), at
/// once here and on the server.
fn write_override(session: &mut Session, project: &str, key: &str, value: Option<Value>) {
    let mut entry = overrides(session.settings(), project);
    match value {
        Some(value) => entry[key] = value,
        None => {
            if let Some(map) = entry.as_object_mut() {
                map.remove(key);
            }
        }
    }
    let entry = if entry.as_object().is_some_and(|m| m.is_empty()) {
        Value::Null
    } else {
        entry
    };
    let settings = &mut session.state.config["settings"];
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["projectSettingsOverrides"].is_object() {
        settings["projectSettingsOverrides"] = json!({});
    }
    if let Some(all) = settings["projectSettingsOverrides"].as_object_mut() {
        if entry.is_null() {
            all.remove(project);
        } else {
            all.insert(project.to_owned(), entry.clone());
        }
    }
    session.request(
        "settings:projectSettingsOverrides",
        "server.updateSettings",
        json!({"patch": {"projectSettingsOverrides": {project: entry}}}),
    );
}

fn project_view(app: &App, id: &str) -> Option<ProjectView> {
    let s = &app.session;
    let project = s.state.project(id)?;
    let now = chrono::Utc::now();
    let threads = s
        .listed(false)
        .into_iter()
        .filter(|t| t.project_id == id)
        .map(|t| {
            let state = if t.has_pending_approvals {
                "Approval".to_owned()
            } else if t.has_pending_user_input {
                "Input".to_owned()
            } else if t.is_running() {
                "Working".to_owned()
            } else {
                crate::times::relative_time(last_activity(&t), now)
            };
            let branch = t
                .branch
                .clone()
                .map(|b| format!("{b} · "))
                .unwrap_or_default();
            ProjectThread {
                id: t.id.clone(),
                title: t.title.clone(),
                line: format!("{branch}{state}"),
            }
        })
        .collect();
    let entry = overrides(s.settings(), id);
    let (model, model_mark) = catalog::selection(&entry["defaultModelSelection"])
        .map(|m| {
            let (label, driver) = catalog::label(s.providers(), &m);
            (label, catalog::mark(&driver).map(str::to_owned))
        })
        .unwrap_or_else(|| ("Default".into(), None));
    Some(ProjectView {
        badge: badge(project),
        root: project.workspace_root.clone(),
        threads,
        model,
        model_mark,
        access: entry["defaultRuntimeMode"]
            .as_str()
            .map(|m| catalog::access(m).1)
            .unwrap_or("Default")
            .into(),
        workspace: entry["defaultThreadEnvMode"]
            .as_str()
            .map(label_of)
            .unwrap_or("Default")
            .into(),
    })
}

fn remove_view(app: &App, project: &str) -> RemoveProjectView {
    let s = &app.session;
    let found = s.state.project(project);
    let title = found.map(|p| p.title.clone()).unwrap_or_default();
    let root = found.map(|p| p.workspace_root.clone()).unwrap_or_default();
    let threads = s
        .state
        .threads
        .iter()
        .filter(|t| t.project_id == project && t.archived_at.is_none())
        .count();
    let heading = if threads > 0 {
        format!(
            "Remove project \"{title}\" and delete its {threads} thread{}?",
            if threads == 1 { "" } else { "s" }
        )
    } else {
        format!("Remove project \"{title}\"?")
    };
    let mut lines = vec![heading, format!("Path: {root}")];
    if let Some(label) = s.state.config["environment"]["label"].as_str() {
        lines.push(format!("Environment: {label}"));
    }
    lines.push(if threads > 0 {
        "This permanently clears conversation history for those threads and any archived threads."
            .into()
    } else {
        "This permanently clears any archived conversation history.".into()
    });
    lines.push("This removes only the project entries, not the files on disk.".into());
    lines.push("This action cannot be undone.".into());
    RemoveProjectView { title, lines }
}

#[uniffi::export]
impl Core {
    /// Projects: one row per code project with its threads and state.
    pub fn projects(&self) -> Vec<ProjectRow> {
        self.read(|app| {
            let s = &app.session;
            let listed = s.listed(false);
            s.code_projects()
                .into_iter()
                .map(|p| {
                    let threads: Vec<_> = listed.iter().filter(|t| t.project_id == p.id).collect();
                    ProjectRow {
                        badge: badge(p),
                        root: p.workspace_root.clone(),
                        threads: threads.len() as u32,
                        waiting: threads
                            .iter()
                            .filter(|t| t.has_pending_approvals || t.has_pending_user_input)
                            .count() as u32,
                        working: threads.iter().filter(|t| t.is_running()).count() as u32,
                    }
                })
                .collect()
        })
    }

    /// A project's page; `None` once it was removed.
    pub fn project(&self, id: String) -> Option<ProjectView> {
        self.read(|app| project_view(app, &id))
    }

    /// New thread from a project's page: the Code draft in that project,
    /// its workspace and base back to the defaults.
    pub fn project_new_thread(&self, id: String) {
        self.update(|app, _| {
            app.drafts.project = Some(id);
            app.drafts.worktree = None;
            app.drafts.base = None;
            app.session.chat = false;
            app.session.select(None);
        });
    }

    /// A New threads default's sheet (`model`, `access` or `workspace`).
    pub fn project_setting(&self, project: String, field: String) -> ProjectSettingView {
        self.read(|app| {
            let s = &app.session;
            let entry = overrides(s.settings(), &project);
            match field.as_str() {
                "model" => {
                    let current = catalog::selection(&entry["defaultModelSelection"]);
                    ProjectSettingView {
                        default: current.is_none(),
                        options: Vec::new(),
                        groups: model_groups(s, current.as_ref()),
                    }
                }
                "access" => {
                    let current = entry["defaultRuntimeMode"].as_str();
                    ProjectSettingView {
                        default: current.is_none(),
                        options: catalog::ACCESS
                            .iter()
                            .map(|(value, label, _, detail)| DefaultOption {
                                value: (*value).into(),
                                label: (*label).into(),
                                detail: (*detail).into(),
                                checked: current == Some(*value),
                            })
                            .collect(),
                        groups: Vec::new(),
                    }
                }
                _ => {
                    let current = entry["defaultThreadEnvMode"].as_str();
                    ProjectSettingView {
                        default: current.is_none(),
                        options: WORKSPACES
                            .iter()
                            .map(|(value, label, detail)| DefaultOption {
                                value: (*value).into(),
                                label: (*label).into(),
                                detail: (*detail).into(),
                                checked: current == Some(*value),
                            })
                            .collect(),
                        groups: Vec::new(),
                    }
                }
            }
        })
    }

    /// Sets a project's default `field` to `value` (`access`, `workspace`),
    /// or back to Default with `None`.
    pub fn set_project_setting(&self, project: String, field: String, value: Option<String>) {
        let key = match field.as_str() {
            "model" => "defaultModelSelection",
            "access" => "defaultRuntimeMode",
            _ => "defaultThreadEnvMode",
        };
        self.update(|app, _| {
            write_override(&mut app.session, &project, key, value.map(Value::String));
        });
    }

    /// Sets a project's default model.
    pub fn set_project_model(&self, project: String, instance_id: String, model: String) {
        self.update(|app, _| {
            let selection = pluto_core::model::ModelSelection {
                instance_id,
                model,
                options: None,
            };
            write_override(
                &mut app.session,
                &project,
                "defaultModelSelection",
                Some(catalog::wire(&selection)),
            );
        });
    }

    /// The removal's confirmation.
    pub fn remove_project_view(&self, project: String) -> RemoveProjectView {
        self.read(|app| remove_view(app, &project))
    }

    /// Removes a project and its threads (`project.delete`, forced); the key
    /// to follow (`reply_state`).
    pub fn remove_project(&self, project: String) -> String {
        self.update(|app, _| {
            app.session.dispatch_tracked(json!({
                "type": "project.delete",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": project,
                "force": true,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_the_last_override_clears_the_entry() {
        let mut session = Session::new(crate::prefs::Prefs::load(None));
        write_override(&mut session, "p", "defaultRuntimeMode", Some(json!("auto")));
        assert_eq!(
            session.settings()["projectSettingsOverrides"]["p"]["defaultRuntimeMode"],
            "auto"
        );
        write_override(&mut session, "p", "defaultRuntimeMode", None);
        assert!(session.settings()["projectSettingsOverrides"]["p"].is_null());
        assert_eq!(label_of("worktree"), "New worktree");
        assert_eq!(label_of("nope"), "Local checkout");
    }
}
