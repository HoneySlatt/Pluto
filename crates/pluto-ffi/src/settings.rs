//! Settings (the Dioxus app's `pages.rs`, after the desktop's `settings.rs`
//! labels, kept to what a phone needs): the server card, Chat's default
//! model, New threads' model, permissions and workspace, the display
//! choices, the confirmations, and the choice sheets (`Sheet::Setting`)
//! that write them, to the server (`server.updateSettings`, the chat
//! space's `project.update`) or to the phone's own preferences.

use pluto_core::model::ModelSelection;
use pluto_core::spaces::Space;
use serde_json::json;

use crate::session::{Net, Session};
use crate::views::Dot;
use crate::{App, Core, catalog};

const TIME_FORMATS: [(&str, &str, &str); 3] = [
    (
        "locale",
        "System default",
        "Follows the phone’s clock preference.",
    ),
    ("12-hour", "12-hour", ""),
    ("24-hour", "24-hour", ""),
];
const STREAMING: [(&str, &str, &str); 3] = [
    (
        "turn",
        "Wait for the full response",
        "Text appears once the agent finishes its turn.",
    ),
    (
        "paragraph",
        "Show finished paragraphs",
        "Each paragraph or code block appears as soon as it is complete.",
    ),
    (
        "token",
        "Token by token (legacy)",
        "Every token repaints the answer as it arrives. Slower and harder to read.",
    ),
];
const DIFF_STATES: [(&str, &str, &str); 2] =
    [("expanded", "Expanded", ""), ("collapsed", "Collapsed", "")];

fn label_of(list: &[(&str, &'static str, &str)], value: &str) -> &'static str {
    list.iter()
        .find(|o| o.0 == value)
        .map(|o| o.1)
        .unwrap_or(list[0].1)
}

/// A model setting's value: its label and its provider's mark.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SettingModelValue {
    pub label: String,
    pub mark: Option<String>,
}

/// The Settings page.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SettingsView {
    pub host: String,
    pub state: String,
    pub dot: Dot,
    pub chat_model: SettingModelValue,
    pub code_model: SettingModelValue,
    pub access: String,
    pub workspace: String,
    pub time_format: String,
    pub streaming: String,
    pub hide_whitespace: bool,
    pub diff_state: String,
    pub confirm_delete: bool,
    /// The server's version and protocol.
    pub version: String,
}

/// An option of a choice sheet.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SettingChoice {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub checked: bool,
}

/// What a choice sheet offers: options, or models by provider; `note`
/// when there is nothing to pick.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SettingSheetView {
    pub choices: Vec<SettingChoice>,
    pub providers: Vec<catalog::ModelGroup>,
    pub note: Option<String>,
}

/// The title of a Settings choice.
pub fn choice_title(key: &str) -> &'static str {
    match key {
        "chatModel" => "Default chat model",
        "codeModel" => "Default model for new threads",
        "codeAccess" => "Permissions",
        "workspace" => "Workspace",
        "timeFormat" => "Time format",
        "streaming" => "Response streaming",
        "diffState" => "Default diff file state",
        _ => "",
    }
}

/// The chat space's default model, when one was set.
fn chat_model(session: &Session) -> Option<ModelSelection> {
    session
        .space_project(Space::Chats)?
        .default_model_selection
        .as_ref()
        .and_then(catalog::selection)
}

fn code_model(session: &Session) -> Option<ModelSelection> {
    catalog::selection(&session.settings()["defaultModelSelection"])
}

fn model_value(session: &Session, model: Option<ModelSelection>) -> SettingModelValue {
    match model {
        Some(model) => {
            let (label, driver) = catalog::label(session.providers(), &model);
            SettingModelValue {
                label,
                mark: catalog::mark(&driver).map(str::to_owned),
            }
        }
        None => SettingModelValue {
            label: "Automatic".into(),
            mark: None,
        },
    }
}

fn setting<'a>(session: &'a Session, key: &str, default: &'a str) -> &'a str {
    session.settings()[key].as_str().unwrap_or(default)
}

fn pref<'a>(session: &'a Session, key: &str, default: &'a str) -> &'a str {
    session.prefs.get(key).as_str().unwrap_or(default)
}

fn settings_view(session: &Session) -> SettingsView {
    let config = &session.state.config;
    let (dot, state) = match session.net {
        Net::Reconnecting { .. } => (Dot::Warn, "Reconnecting…"),
        Net::Offline => (Dot::Bad, "Offline"),
        _ => (Dot::Ok, "Connected"),
    };
    SettingsView {
        host: session.host(),
        state: state.into(),
        dot,
        chat_model: model_value(session, chat_model(session)),
        code_model: model_value(session, code_model(session)),
        access: catalog::access(setting(session, "defaultRuntimeMode", "full-access"))
            .1
            .into(),
        workspace: label_of(
            &catalog::WORKSPACES,
            setting(session, "defaultThreadEnvMode", "local"),
        )
        .into(),
        time_format: label_of(&TIME_FORMATS, pref(session, "timestampFormat", "locale")).into(),
        streaming: label_of(
            &STREAMING,
            setting(session, "responseStreamingMode", "paragraph"),
        )
        .into(),
        hide_whitespace: session.prefs.flag("diffIgnoreWhitespace"),
        diff_state: if session.prefs.flag("diffFilesCollapsed") {
            "Collapsed"
        } else {
            "Expanded"
        }
        .into(),
        confirm_delete: session.prefs.flag("confirmThreadDelete"),
        version: config["environment"]["serverVersion"]
            .as_str()
            .or(config["serverVersion"].as_str())
            .map(|v| format!("{v} · protocol 2"))
            .unwrap_or_else(|| "protocol 2".into()),
    }
}

fn options(list: &[(&str, &str, &str)], current: &str) -> Vec<SettingChoice> {
    list.iter()
        .map(|(value, label, detail)| SettingChoice {
            value: (*value).into(),
            label: (*label).into(),
            detail: (*detail).into(),
            checked: *value == current,
        })
        .collect()
}

fn models(session: &Session, chat: bool, current: &ModelSelection) -> Vec<catalog::ModelGroup> {
    catalog::model_groups(session.providers(), chat, Some(current))
}

fn sheet_view(session: &Session, key: &str) -> SettingSheetView {
    let mut view = SettingSheetView {
        choices: Vec::new(),
        providers: Vec::new(),
        note: None,
    };
    match key {
        "chatModel" => {
            if session.space_project(Space::Chats).is_none() {
                view.note = Some(
                    "Open Chat once in Pluto on the computer: it creates the chat space.".into(),
                );
                return view;
            }
            let current = chat_model(session).unwrap_or_default();
            view.providers = models(session, true, &current);
        }
        "codeModel" => {
            let current = code_model(session).unwrap_or_default();
            view.providers = models(session, false, &current);
        }
        "codeAccess" => {
            let current = setting(session, "defaultRuntimeMode", "full-access");
            view.choices = catalog::ACCESS
                .iter()
                .map(|(value, label, _, detail)| SettingChoice {
                    value: (*value).into(),
                    label: (*label).into(),
                    detail: (*detail).into(),
                    checked: *value == current,
                })
                .collect();
        }
        "workspace" => {
            view.choices = options(
                &catalog::WORKSPACES,
                setting(session, "defaultThreadEnvMode", "local"),
            )
        }
        "streaming" => {
            view.choices = options(
                &STREAMING,
                setting(session, "responseStreamingMode", "paragraph"),
            )
        }
        "timeFormat" => {
            view.choices = options(&TIME_FORMATS, pref(session, "timestampFormat", "locale"))
        }
        "diffState" => {
            let current = if session.prefs.flag("diffFilesCollapsed") {
                "collapsed"
            } else {
                "expanded"
            };
            view.choices = options(&DIFF_STATES, current);
        }
        _ => {}
    }
    if view.choices.is_empty() && view.providers.is_empty() && view.note.is_none() {
        view.note = Some("No provider is ready on the server.".into());
    }
    view
}

/// Applies a choice of the sheet `key`.
fn pick(app: &mut App, key: &str, value: &str) {
    let session = &mut app.session;
    match key {
        "codeAccess" => session.update_setting("defaultRuntimeMode", json!(value)),
        "workspace" => session.update_setting("defaultThreadEnvMode", json!(value)),
        "streaming" => session.update_setting("responseStreamingMode", json!(value)),
        "timeFormat" => session.prefs.set("timestampFormat", json!(value)),
        "diffState" => session
            .prefs
            .set("diffFilesCollapsed", json!(value == "collapsed")),
        _ => {}
    }
}

/// Applies a model choice of the sheet `key`.
fn pick_model(app: &mut App, key: &str, instance_id: String, model: String) {
    let session = &mut app.session;
    let picked = ModelSelection {
        instance_id,
        model,
        options: None,
    };
    match key {
        "chatModel" => {
            let Some(project) = session.space_project(Space::Chats).map(|p| p.id.clone()) else {
                return;
            };
            session.dispatch(json!({
                "type": "project.update",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": project,
                "defaultModelSelection": catalog::wire(&picked),
            }));
        }
        "codeModel" => session.update_setting("defaultModelSelection", catalog::wire(&picked)),
        _ => {}
    }
}

#[uniffi::export]
impl Core {
    /// The Settings page.
    pub fn settings_view(&self) -> SettingsView {
        self.read(|app| settings_view(&app.session))
    }

    /// The title of the Settings choice `key`.
    pub fn setting_title(&self, key: String) -> String {
        choice_title(&key).into()
    }

    /// What the Settings choice `key` offers.
    pub fn setting_sheet(&self, key: String) -> SettingSheetView {
        self.read(|app| sheet_view(&app.session, &key))
    }

    /// Picks `value` in the Settings choice `key`.
    pub fn pick_setting(&self, key: String, value: String) {
        self.update(|app, _| pick(app, &key, &value));
    }

    /// Picks a model in the Settings choice `key` (`chatModel`, `codeModel`).
    pub fn pick_setting_model(&self, key: String, instance_id: String, model: String) {
        self.update(|app, _| pick_model(app, &key, instance_id, model));
    }

    /// A Settings toggle: `diffIgnoreWhitespace`, `confirmThreadDelete`.
    pub fn set_setting_flag(&self, key: String, on: bool) {
        self.update(|app, _| {
            if matches!(key.as_str(), "diffIgnoreWhitespace" | "confirmThreadDelete") {
                app.session.prefs.set(&key, json!(on));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Prefs;

    fn session() -> Session {
        let mut session = Session::new(Prefs::load(None));
        session.state.config_event(&json!({"type": "snapshot", "config": {
            "settings": {"defaultRuntimeMode": "approval-required", "defaultThreadEnvMode": "worktree",
                "defaultModelSelection": {"instanceId": "claude", "model": "opus"}},
            "providers": [{"instanceId": "claude", "driver": "claudeAgent", "status": "ready",
                "displayName": "Claude", "models": [{"slug": "opus", "name": "Opus 5.5"}]}]}}));
        session
    }

    #[test]
    fn the_page_reads_the_server_and_the_phone() {
        let view = settings_view(&session());
        assert_eq!(view.access, "Supervised");
        assert_eq!(view.workspace, "New worktree");
        assert_eq!(view.code_model.label, "Opus 5.5");
        assert_eq!(view.code_model.mark.as_deref(), Some("claude"));
        assert_eq!(view.chat_model.label, "Automatic");
        assert_eq!(view.time_format, "System default");
        assert_eq!(view.streaming, "Show finished paragraphs");
        assert_eq!(view.diff_state, "Collapsed");
        assert_eq!(view.version, "protocol 2");
    }

    #[test]
    fn sheets_check_the_current_choice() {
        let session = session();
        let access = sheet_view(&session, "codeAccess");
        assert_eq!(
            access
                .choices
                .iter()
                .find(|c| c.checked)
                .map(|c| c.value.as_str()),
            Some("approval-required")
        );
        let models = sheet_view(&session, "codeModel");
        assert!(models.providers[0].models[0].checked);
        // No chat space yet: the sheet says where it comes from.
        assert!(sheet_view(&session, "chatModel").note.is_some());
        assert_eq!(choice_title("diffState"), "Default diff file state");
    }
}
