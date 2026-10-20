//! Settings (`routes/settings.tsx`, `SettingsSidebarNav.tsx`,
//! `settingsLayout.tsx`, `SettingsPanels.tsx` `GeneralSettingsPanel`,
//! `ProjectDefaultsSettings.tsx`, `NotificationSettings.tsx`).
//!
//! Ported: the settings sidebar and header, and the General page for the
//! "All projects / All environments" scope. Server settings save through
//! `server.updateSettings`; client settings live in memory (upstream keeps
//! them in local storage). Not ported: the other pages, settings search, the
//! scope pickers, the default model pickers, the token-streaming warning and
//! the background activity details dialog.
use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::context_menu;
use crate::ui::widgets::{self, MenuEntry, MenuItem};

/// `SETTINGS_SECTION_LABELS` with their icons (`SETTINGS_SECTION_ICONS`).
pub const PAGES: [(&str, &str); 10] = [
    ("General", "settings-2"),
    ("Appearance", "palette"),
    ("Keybindings", "keyboard"),
    ("SnapShots", "snap-shot"),
    ("Providers", "bot"),
    ("Integrations", "blocks"),
    ("Source Control", "git-branch"),
    ("Storage", "hard-drive"),
    ("Connections", "link-2"),
    ("Archive", "archive"),
];

/// `DEFAULT_CLIENT_SETTINGS` for the client keys the General page edits.
pub fn default_client_settings() -> Value {
    json!({
        "notificationMode": "off",
        "inAppNotificationsEnabled": false,
        "timestampFormat": "locale",
        "diffIgnoreWhitespace": true,
        "diffFilesCollapsed": true,
        "diffLayout": "stacked",
        "proactivePanelsEnabled": false,
        "showSkillsInSlashMenu": true,
        "composerRichTextEnabled": true,
        "composerCollapseOnScroll": true,
        "sendShortcut": "enter",
        "followUpBehavior": "queue",
        "confirmThreadUnpin": false,
        "confirmThreadArchive": false,
        "confirmThreadDelete": true,
        "confirmQuit": "hold",
        "sidebarProjectGroupingMode": "repository",
        "sidebarWorkingShelfEnabled": false,
        "planModeEnabled": false,
        "contextWindowMeterEnabled": false,
        "legacySidebarEnabled": false,
    })
}

/// `DEFAULT_SERVER_SETTINGS` for the server keys with a reset button.
fn default_server_setting(key: &str) -> Value {
    match key {
        "responseStreamingMode" => json!("paragraph"),
        "enableProviderUpdateChecks" => json!(true),
        "continueThreadsAfterServerUpdate" => json!(false),
        "sidebarAutoSettleOnMerge" => json!(true),
        "sidebarAutoSettleAfterDays" => json!(3),
        "newWorktreesStartFromOrigin" => json!(true),
        "addProjectBaseDirectory" => json!(""),
        _ => Value::Null,
    }
}

const NOTIFICATION_MODES: [(&str, &str); 4] = [
    ("off", "Off"),
    ("notifications", "Notifications only"),
    ("sound", "Sound only"),
    ("notifications-and-sound", "Notifications with sound"),
];
const TIMESTAMP_FORMATS: [(&str, &str); 3] = [
    ("locale", "System default"),
    ("12-hour", "12-hour"),
    ("24-hour", "24-hour"),
];
const RESPONSE_STREAMING: [(&str, &str, &str); 3] = [
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
        "Every token repaints the answer as it arrives. Slower and harder to read. Thinking traces still arrive a paragraph at a time.",
    ),
];
const DIFF_FILE_STATES: [(&str, &str); 2] = [("expanded", "Expanded"), ("collapsed", "Collapsed")];
const DIFF_LAYOUTS: [(&str, &str); 2] = [("stacked", "Stacked"), ("split", "Split")];
const SEND_SHORTCUTS: [(&str, &str); 3] = [
    ("enter", "Enter"),
    ("mod-enter-multiline", "Ctrl + Enter for multiline prompts"),
    ("mod-enter", "Ctrl + Enter always"),
];
const FOLLOW_UPS: [(&str, &str); 2] = [("queue", "Queue"), ("steer", "Steer")];
const BACKGROUND_PROFILES: [(&str, &str, &str); 3] = [
    (
        "balanced",
        "Balanced",
        "Pauses probes for idle clients, locked hosts, or low power mode.",
    ),
    (
        "performance",
        "Performance",
        "Allows scoped background probes while any subscribed client remains connected.",
    ),
    (
        "battery-saver",
        "Battery saver",
        "Also pauses background probes when the host or client is on battery.",
    ),
];
const ENV_MODES: [(&str, &str); 2] = [("local", "Current checkout"), ("worktree", "New worktree")];
const SUBMODULES: [(&str, &str); 3] = [
    ("recursive", "Recursive"),
    ("top-level", "Top level only"),
    ("none", "Skip"),
];

const CARD_WIDTH: f32 = 848.0;
/// Index of the Archive page in `PAGES`.
pub const ARCHIVE_PAGE: usize = 9;
const LEGACY: &str = "Legacy features";
/// A select trigger sized to its label (`w-auto`).
const AUTO_WIDTH: f32 = 0.0;
const ROW_PAD_X: f32 = 16.0;
const ROW_PAD_Y: f32 = 12.0;
const DESCRIPTION_MAX: f32 = 576.0; // max-w-xl
const CARD_FILL: Color32 = Color32::from_rgb(13, 13, 13);
const CARD_BORDER: Color32 = Color32::from_rgb(22, 22, 22);
const ROW_DIVIDER: Color32 = Color32::from_rgb(21, 21, 21);

/// Where a setting lives.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Store {
    Client,
    Server,
}

enum Control {
    Switch(bool),
    /// Options as (value, label); `width` is the trigger width.
    Select {
        value: String,
        options: Vec<(String, String)>,
        width: f32,
        icon: Option<&'static str>,
    },
    Number(i64),
    Text(String),
    /// The default model buttons: model label with its provider glyph, traits.
    Model {
        label: String,
        icon: Option<&'static str>,
        traits: Option<String>,
    },
    Button(&'static str),
    /// `Button variant="outline" size="xs"` with a leading icon.
    OutlineXs {
        label: &'static str,
        icon: &'static str,
    },
    None,
}

struct Row {
    id: String,
    title: String,
    description: String,
    status: Option<String>,
    /// Server rows show the inheritance indicator beside the title.
    layers: bool,
    reset: bool,
    control: Control,
}

struct Section {
    title: String,
    /// Archive groups show their project's icon before the title.
    project: Option<crate::model::Project>,
    rows: Vec<Row>,
}

enum RowAction {
    Toggle(bool),
    Select(String),
    Number(i64),
    Text(String),
    Reset,
    Click,
    /// A secondary click at this position (archived rows).
    Menu(Pos2),
}

fn options(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(v, l)| ((*v).to_owned(), (*l).to_owned()))
        .collect()
}

impl App {
    pub(crate) fn open_settings(&mut self) {
        self.settings_page = Some(0);
        self.open_menu = None;
    }

    fn client_setting(&self, key: &str) -> Value {
        self.client_settings
            .get(key)
            .cloned()
            .unwrap_or_else(|| default_client_settings()[key].clone())
    }

    fn server_setting(&self, key: &str) -> Value {
        let value = &self.state.config["settings"][key];
        if value.is_null() {
            default_server_setting(key)
        } else {
            value.clone()
        }
    }

    /// Writes one setting: client keys in memory, server keys with
    /// `server.updateSettings` (applied locally right away).
    pub(crate) fn update_setting(&mut self, store: Store, key: &str, value: Value) {
        match store {
            Store::Client => {
                self.client_settings[key] = value.clone();
                // The diff panel starts from these client defaults.
                match key {
                    "diffIgnoreWhitespace" => {
                        self.diff_ignore_whitespace = value.as_bool().unwrap_or(true)
                    }
                    "diffLayout" => self.diff_split = value == "split",
                    _ => {}
                }
            }
            Store::Server => {
                if !self.state.config["settings"].is_object() {
                    self.state.config["settings"] = json!({});
                }
                self.state.config["settings"][key] = value.clone();
                self.send_request(
                    format!("settings:{key}"),
                    "server.updateSettings",
                    json!({ "patch": { key: value } }),
                );
            }
        }
    }

    fn general_sections(&self) -> Vec<Section> {
        let client_select = |key: &str, list: &[(&str, &str)], width: f32| Control::Select {
            value: self
                .client_setting(key)
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            options: options(list),
            width,
            icon: None,
        };
        let is_default = |key: &str| self.client_setting(key) == default_client_settings()[key];
        let server_default = |key: &str| self.server_setting(key) == default_server_setting(key);
        let switch = |key: &str| Control::Switch(self.client_setting(key) == true);

        // New threads (`ProjectDefaultsSettings`, category "general").
        let selection = self.composer_model_selection(true);
        let (model, model_icon) = self.model_label(&selection);
        let runtime = self.state.config["settings"]["defaultRuntimeMode"]
            .as_str()
            .unwrap_or("full-access")
            .to_owned();
        let runtime_icon = crate::ui::pickers::RUNTIME_MODES
            .iter()
            .find(|m| m.0 == runtime)
            .map(|m| m.2);
        let env_mode = self.state.config["settings"]["defaultThreadEnvMode"]
            .as_str()
            .unwrap_or("local")
            .to_owned();
        let submodules = self.state.config["settings"]["worktreeSubmodules"]
            .as_str()
            .unwrap_or("recursive")
            .to_owned();
        let new_threads = Section {
            title: "New threads".into(),
                project: None,
            rows: vec![
                Row {
                    id: "default-model".into(),
                    title: "Model".into(),
                    description: "Default model for new threads. Projects can override it.".into(),
                    status: self.state.config["settings"]["defaultModelSelection"]
                        .is_null()
                        .then(|| "Automatic".to_owned()),
                    layers: true,
                    reset: false,
                    control: Control::Model {
                        label: model,
                        icon: model_icon,
                        traits: self.traits_label(&selection),
                    },
                },
                Row {
                    id: "default-permissions".into(),
                    title: "Permissions".into(),
                    description: "Default permissions for new threads. Projects can override them."
                        .into(),
                    status: None,
                    layers: true,
                    reset: false,
                    control: Control::Select {
                        value: runtime,
                        options: crate::ui::pickers::RUNTIME_MODES
                            .iter()
                            .map(|m| (m.0.to_owned(), m.1.to_owned()))
                            .collect(),
                        width: 160.0,
                        icon: runtime_icon,
                    },
                },
                Row {
                    id: "default-workspace".into(),
                    title: "Workspace".into(),
                    description: "Where new threads start. Projects and their t3.json can override it."
                        .into(),
                    status: None,
                    layers: true,
                    reset: false,
                    control: Control::Select {
                        value: env_mode,
                        options: options(&ENV_MODES),
                        width: 160.0,
                        icon: None,
                    },
                },
                Row {
                    id: "worktree-submodules".into(),
                    title: "Submodules".into(),
                    description: "How new worktrees populate git submodules. Projects and their t3.json can override it.".into(),
                    status: None,
                    layers: true,
                    reset: !self.state.config["settings"]["worktreeSubmodules"].is_null(),
                    control: Control::Select {
                        value: submodules,
                        options: options(&SUBMODULES),
                        width: 160.0,
                        icon: None,
                    },
                },
            ],
        };

        let auto_settle_days = self.server_setting("sidebarAutoSettleAfterDays");
        let mut organization = vec![
            Row {
                id: "project-grouping".into(),
                title: "Project grouping".into(),
                description: "Combine matching repositories across environments.".into(),
                status: None,
                layers: false,
                reset: !is_default("sidebarProjectGroupingMode"),
                control: Control::Switch(self.client_setting("sidebarProjectGroupingMode") != "separate"),
            },
            Row {
                id: "working-shelf".into(),
                title: "Working section (beta)".into(),
                description: "Fold working and monitoring threads into a Working section. They return to the top of the inbox when they need you.".into(),
                status: None,
                layers: false,
                reset: !is_default("sidebarWorkingShelfEnabled"),
                control: switch("sidebarWorkingShelfEnabled"),
            },
            Row {
                id: "auto-settle-merged-threads".into(),
                title: "Auto-settle merged threads".into(),
                description: "Settle a thread when its pull request merges. Closed pull requests still settle automatically.".into(),
                status: None,
                layers: true,
                reset: !server_default("sidebarAutoSettleOnMerge"),
                control: Control::Switch(self.server_setting("sidebarAutoSettleOnMerge") == true),
            },
            Row {
                id: "auto-settle-inactive-threads".into(),
                title: "Auto-settle inactive threads".into(),
                description: "Sidebar threads with no activity for this long settle automatically."
                    .into(),
                status: None,
                layers: true,
                reset: auto_settle_days.is_null(),
                control: Control::Switch(!auto_settle_days.is_null()),
            },
        ];
        if let Some(days) = auto_settle_days.as_i64() {
            organization.push(Row {
                id: "days-before-auto-settle".into(),
                title: "Days of inactivity before auto-settle".into(),
                description: "Any new activity un-settles a thread automatically.".into(),
                status: None,
                layers: true,
                reset: days != 3,
                control: Control::Number(days),
            });
        }

        let streaming = self
            .server_setting("responseStreamingMode")
            .as_str()
            .unwrap_or("paragraph")
            .to_owned();
        let send = self.client_setting("sendShortcut");
        let follow_up_tail = if send == "mod-enter-multiline" {
            "Press Ctrl + Enter for single-line prompts or Ctrl + Shift + Enter for multiline prompts to do the opposite for one message.".to_owned()
        } else {
            format!(
                "Press Ctrl{} + Enter to do the opposite for one message.",
                if send == "mod-enter" { " + Shift" } else { "" }
            )
        };
        let profile = self.state.config["settings"]["backgroundActivity"]["profile"]
            .as_str()
            .unwrap_or("balanced")
            .to_owned();
        let behavior = vec![
            Row {
                id: "thread-notifications".into(),
                title: "Thread notifications".into(),
                description: "System alerts when a thread finishes, fails, or needs input or approval. Applies to this device while T3 Code is open.".into(),
                status: None,
                layers: false,
                reset: false,
                control: client_select("notificationMode", &NOTIFICATION_MODES, 224.0),
            },
            Row {
                id: "in-app-notifications".into(),
                title: "In-app notifications".into(),
                description: "Show a toast when another thread finishes, fails, or needs input or approval while this app has focus.".into(),
                status: None,
                layers: false,
                reset: false,
                control: switch("inAppNotificationsEnabled"),
            },
            Row {
                id: "time-format".into(),
                title: "Time format".into(),
                description: "System default follows your browser or OS clock preference.".into(),
                status: None,
                layers: false,
                reset: !is_default("timestampFormat"),
                control: client_select("timestampFormat", &TIMESTAMP_FORMATS, 160.0),
            },
            Row {
                id: "response-streaming".into(),
                title: "Response streaming".into(),
                description: RESPONSE_STREAMING
                    .iter()
                    .find(|m| m.0 == streaming)
                    .map_or("", |m| m.2)
                    .into(),
                status: None,
                layers: true,
                reset: !server_default("responseStreamingMode"),
                control: Control::Select {
                    value: streaming,
                    options: RESPONSE_STREAMING
                        .iter()
                        .map(|m| (m.0.to_owned(), m.1.to_owned()))
                        .collect(),
                    width: 224.0,
                    icon: None,
                },
            },
            Row {
                id: "hide-whitespace-changes".into(),
                title: "Hide whitespace changes".into(),
                description: "Set whether the diff panel ignores whitespace-only edits by default."
                    .into(),
                status: None,
                layers: false,
                reset: !is_default("diffIgnoreWhitespace"),
                control: switch("diffIgnoreWhitespace"),
            },
            Row {
                id: "default-diff-file-state".into(),
                title: "Default diff file state".into(),
                description: "Start with files expanded or collapsed when opening diffs or a pull request's Code tab.".into(),
                status: None,
                layers: false,
                reset: !is_default("diffFilesCollapsed"),
                control: Control::Select {
                    value: if self.client_setting("diffFilesCollapsed") == true {
                        "collapsed".into()
                    } else {
                        "expanded".into()
                    },
                    options: options(&DIFF_FILE_STATES),
                    width: 160.0,
                    icon: None,
                },
            },
            Row {
                id: "diff-layout".into(),
                title: "Diff layout".into(),
                description: "Show diffs stacked or side by side. The toggle in the diff toolbar changes this too.".into(),
                status: None,
                layers: false,
                reset: !is_default("diffLayout"),
                control: client_select("diffLayout", &DIFF_LAYOUTS, 160.0),
            },
            Row {
                id: "proactive-panels".into(),
                title: "Proactive panels".into(),
                description: "Open linked pull requests first. Otherwise, open the working tree diff for changes to at least 3 files or 50 lines.".into(),
                status: None,
                layers: false,
                reset: !is_default("proactivePanelsEnabled"),
                control: switch("proactivePanelsEnabled"),
            },
            Row {
                id: "skills-in-slash-menu".into(),
                title: "Show skills in slash menu".into(),
                description: "Also include skills in the / command menu. Skills always appear when you type $.".into(),
                status: None,
                layers: false,
                reset: !is_default("showSkillsInSlashMenu"),
                control: switch("showSkillsInSlashMenu"),
            },
            Row {
                id: "composer-rich-text".into(),
                title: "Rich text composer".into(),
                description: "Show formatted Markdown as you type.".into(),
                status: None,
                layers: false,
                reset: !is_default("composerRichTextEnabled"),
                control: switch("composerRichTextEnabled"),
            },
            Row {
                id: "composer-collapse".into(),
                title: "Collapse composer on scroll".into(),
                description: "Rest the composer of an existing thread into a single line when you scroll the conversation. Focus the composer or start typing to expand it again.".into(),
                status: None,
                layers: false,
                reset: !is_default("composerCollapseOnScroll"),
                control: switch("composerCollapseOnScroll"),
            },
            Row {
                id: "send-shortcut".into(),
                title: "Send shortcut".into(),
                description: "Choose when Enter sends a prompt or inserts a new line".into(),
                status: None,
                layers: false,
                reset: !is_default("sendShortcut"),
                control: client_select("sendShortcut", &SEND_SHORTCUTS, AUTO_WIDTH),
            },
            Row {
                id: "follow-up-behavior".into(),
                title: "Follow-up behavior".into(),
                description: format!(
                    "Queue follow-ups while the agent runs or steer the current run. {follow_up_tail}"
                ),
                status: None,
                layers: false,
                reset: !is_default("followUpBehavior"),
                control: client_select("followUpBehavior", &FOLLOW_UPS, AUTO_WIDTH),
            },
            Row {
                id: "provider-update-checks".into(),
                title: "Provider update checks".into(),
                description: "Check installed provider CLIs for newer available versions.".into(),
                status: None,
                layers: true,
                reset: !server_default("enableProviderUpdateChecks"),
                control: Control::Switch(self.server_setting("enableProviderUpdateChecks") == true),
            },
            Row {
                id: "continue-threads-after-server-update".into(),
                title: "Continue threads after restarts".into(),
                description: "Automatically resume interrupted threads after an update, crash, or machine restart on the selected environments.".into(),
                status: None,
                layers: true,
                reset: !server_default("continueThreadsAfterServerUpdate"),
                control: Control::Switch(
                    self.server_setting("continueThreadsAfterServerUpdate") == true,
                ),
            },
            Row {
                id: "background-activity".into(),
                title: "Background activity".into(),
                description: BACKGROUND_PROFILES
                    .iter()
                    .find(|p| p.0 == profile)
                    .map_or("Uses custom intervals.", |p| p.2)
                    .into(),
                status: None,
                layers: true,
                reset: profile != "balanced",
                control: Control::Select {
                    value: profile,
                    options: BACKGROUND_PROFILES
                        .iter()
                        .map(|p| (p.0.to_owned(), p.1.to_owned()))
                        .collect(),
                    width: 160.0,
                    icon: None,
                },
            },
        ];

        let projects = vec![
            Row {
                id: "start-from-origin".into(),
                title: "Start from origin".into(),
                description: "Creates the worktree from the latest matching branch on origin instead of your local branch.".into(),
                status: None,
                layers: true,
                reset: !server_default("newWorktreesStartFromOrigin"),
                control: Control::Switch(self.server_setting("newWorktreesStartFromOrigin") == true),
            },
            Row {
                id: "add-project-starts-in".into(),
                title: "Add project starts in".into(),
                description: "Leave empty to use \"~/\" when the Add Project browser opens.".into(),
                status: None,
                layers: true,
                reset: !server_default("addProjectBaseDirectory"),
                control: Control::Text(
                    self.server_setting("addProjectBaseDirectory")
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                ),
            },
        ];

        let confirmations = vec![
            Row {
                id: "unpin-confirmation".into(),
                title: "Unpin confirmation".into(),
                description: "Ask before unpinning a thread from the pinned section.".into(),
                status: None,
                layers: false,
                reset: !is_default("confirmThreadUnpin"),
                control: switch("confirmThreadUnpin"),
            },
            Row {
                id: "archive-confirmation".into(),
                title: "Archive confirmation".into(),
                description: "Require a second click on the inline archive action before a thread is archived.".into(),
                status: None,
                layers: false,
                reset: !is_default("confirmThreadArchive"),
                control: switch("confirmThreadArchive"),
            },
            Row {
                id: "delete-confirmation".into(),
                title: "Delete confirmation".into(),
                description: "Ask before deleting a thread and its chat history.".into(),
                status: None,
                layers: false,
                reset: !is_default("confirmThreadDelete"),
                control: switch("confirmThreadDelete"),
            },
        ];

        let text_generation = {
            let selection = serde_json::from_value::<crate::model::ModelSelection>(
                self.state.config["settings"]["textGenerationModelSelection"].clone(),
            )
            .ok()
            .filter(|s| !s.model.is_empty())
            .unwrap_or_else(|| self.composer_model_selection(true));
            let (label, icon) = self.model_label(&selection);
            vec![Row {
                id: "text-generation-model".into(),
                title: "Text generation model".into(),
                description: "Used for thread titles and other generated text on connected devices with this provider. Source control can override it.".into(),
                status: None,
                layers: true,
                reset: false,
                control: Control::Model {
                    label,
                    icon,
                    traits: self.traits_label(&selection),
                },
            }]
        };

        vec![
            new_threads,
            Section {
                title: "Organization".into(),
                project: None,
                rows: organization,
            },
            Section {
                title: "Behavior".into(),
                project: None,
                rows: behavior,
            },
            Section {
                title: "Projects & threads".into(),
                project: None,
                rows: projects,
            },
            Section {
                title: "Confirmations".into(),
                project: None,
                rows: confirmations,
            },
            Section {
                title: "Text generation".into(),
                project: None,
                rows: text_generation,
            },
            Section {
                title: "About".into(),
                project: None,
                rows: vec![Row {
                    id: "about-version".into(),
                    title: "Version".into(),
                    description: "Current version of the application.".into(),
                    status: None,
                    layers: false,
                    reset: false,
                    control: Control::None,
                }],
            },
            Section {
                title: "Diagnostics".into(),
                project: None,
                rows: vec![
                    Row {
                        id: "diagnostics".into(),
                        title: "Diagnostics".into(),
                        description:
                            "Inspect processes, resource use, and logs on this environment.".into(),
                        status: None,
                        layers: false,
                        reset: false,
                        control: Control::Button("View diagnostics"),
                    },
                    Row {
                        id: "open-source-licenses".into(),
                        title: "Open source licenses".into(),
                        description:
                            "Notices for dependencies, assets, and optional tools used by T3 Code."
                                .into(),
                        status: None,
                        layers: false,
                        reset: false,
                        control: Control::Button("View licenses"),
                    },
                ],
            },
            Section {
                title: LEGACY.into(),
                project: None,
                rows: if self.settings_legacy_open {
                    vec![
                        Row {
                            id: "legacy-plan-mode".into(),
                            title: "Plan mode (legacy)".into(),
                            description: "Restore Build/Plan, /plan, /default, and Shift+Tab. Off uses build mode.".into(),
                            status: None,
                            layers: false,
                            reset: false,
                            control: switch("planModeEnabled"),
                        },
                        Row {
                            id: "legacy-context-window-indicator".into(),
                            title: "Context window indicator (legacy)".into(),
                            description: "Shows context window usage as a circular indicator in the composer.".into(),
                            status: None,
                            layers: false,
                            reset: false,
                            control: switch("contextWindowMeterEnabled"),
                        },
                        Row {
                            id: "legacy-sidebar".into(),
                            title: "Sidebar (legacy)".into(),
                            description: "Restore per-project thread trees instead of the default flat sidebar.".into(),
                            status: None,
                            layers: false,
                            reset: false,
                            control: switch("legacySidebarEnabled"),
                        },
                    ]
                } else {
                    Vec::new()
                },
            },
        ]
    }

    fn apply_general(&mut self, id: &str, action: RowAction) {
        use RowAction::*;
        let client =
            |app: &mut App, key: &str, value: Value| app.update_setting(Store::Client, key, value);
        let server =
            |app: &mut App, key: &str, value: Value| app.update_setting(Store::Server, key, value);
        let client_key = match id {
            "working-shelf" => Some("sidebarWorkingShelfEnabled"),
            "in-app-notifications" => Some("inAppNotificationsEnabled"),
            "hide-whitespace-changes" => Some("diffIgnoreWhitespace"),
            "proactive-panels" => Some("proactivePanelsEnabled"),
            "skills-in-slash-menu" => Some("showSkillsInSlashMenu"),
            "composer-rich-text" => Some("composerRichTextEnabled"),
            "composer-collapse" => Some("composerCollapseOnScroll"),
            "unpin-confirmation" => Some("confirmThreadUnpin"),
            "archive-confirmation" => Some("confirmThreadArchive"),
            "delete-confirmation" => Some("confirmThreadDelete"),
            "legacy-plan-mode" => Some("planModeEnabled"),
            "legacy-context-window-indicator" => Some("contextWindowMeterEnabled"),
            "legacy-sidebar" => Some("legacySidebarEnabled"),
            "thread-notifications" => Some("notificationMode"),
            "time-format" => Some("timestampFormat"),
            "diff-layout" => Some("diffLayout"),
            "send-shortcut" => Some("sendShortcut"),
            "follow-up-behavior" => Some("followUpBehavior"),
            _ => None,
        };
        let server_key = match id {
            "auto-settle-merged-threads" => Some("sidebarAutoSettleOnMerge"),
            "response-streaming" => Some("responseStreamingMode"),
            "provider-update-checks" => Some("enableProviderUpdateChecks"),
            "continue-threads-after-server-update" => Some("continueThreadsAfterServerUpdate"),
            "start-from-origin" => Some("newWorktreesStartFromOrigin"),
            "add-project-starts-in" => Some("addProjectBaseDirectory"),
            "days-before-auto-settle" => Some("sidebarAutoSettleAfterDays"),
            _ => None,
        };
        match (id, action) {
            ("project-grouping", Toggle(on)) => client(
                self,
                "sidebarProjectGroupingMode",
                json!(if on { "repository" } else { "separate" }),
            ),
            ("project-grouping", Reset) => {
                client(self, "sidebarProjectGroupingMode", json!("repository"))
            }
            ("auto-settle-inactive-threads", Toggle(on)) => server(
                self,
                "sidebarAutoSettleAfterDays",
                if on { json!(3) } else { Value::Null },
            ),
            ("auto-settle-inactive-threads", Reset) => {
                server(self, "sidebarAutoSettleAfterDays", json!(3))
            }
            ("default-diff-file-state", Select(value)) => {
                client(self, "diffFilesCollapsed", json!(value == "collapsed"))
            }
            ("default-diff-file-state", Reset) => client(self, "diffFilesCollapsed", json!(true)),
            ("default-permissions", Select(value)) => {
                server(self, "defaultRuntimeMode", json!(value))
            }
            ("default-workspace", Select(value)) => {
                server(self, "defaultThreadEnvMode", json!(value))
            }
            ("worktree-submodules", Select(value)) => {
                server(self, "worktreeSubmodules", json!(value))
            }
            ("worktree-submodules", Reset) => server(self, "worktreeSubmodules", Value::Null),
            ("background-activity", Select(value)) => {
                let mut activity = self.state.config["settings"]["backgroundActivity"].clone();
                if !activity.is_object() {
                    activity = json!({});
                }
                activity["profile"] = json!(value);
                server(self, "backgroundActivity", activity);
            }
            ("background-activity", Reset) => {
                let mut activity = self.state.config["settings"]["backgroundActivity"].clone();
                if activity.is_object() {
                    activity["profile"] = json!("balanced");
                    server(self, "backgroundActivity", activity);
                }
            }
            (_, Click) | ("default-model", _) | ("text-generation-model", _) => {
                self.status = Some("This settings control is not implemented yet".into())
            }
            (_, action) => {
                if let Some(key) = client_key {
                    let value = match action {
                        Toggle(on) => json!(on),
                        Select(value) | Text(value) => json!(value),
                        Number(n) => json!(n),
                        Reset => default_client_settings()[key].clone(),
                        Click | Menu(_) => return,
                    };
                    client(self, key, value);
                } else if let Some(key) = server_key {
                    let value = match action {
                        Toggle(on) => json!(on),
                        Select(value) | Text(value) => json!(value),
                        Number(n) => json!(n.max(1)),
                        Reset => default_server_setting(key),
                        Click | Menu(_) => return,
                    };
                    server(self, key, value);
                }
            }
        }
    }

    /// The settings screen: its sidebar and the selected page.
    pub(crate) fn settings_screen(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(page) = self.settings_page else {
            return;
        };
        let overlay =
            self.open_menu.is_some() || self.archive_menu.is_some() || self.confirm.is_some();
        if ui.input(|i| i.key_pressed(Key::Escape)) && !overlay {
            self.settings_page = None;
            return;
        }
        let sidebar =
            Rect::from_min_size(screen.min, Vec2::new(theme::SIDEBAR_WIDTH, screen.height()));
        let main = Rect::from_min_max(Pos2::new(sidebar.right(), screen.top()), screen.max);
        ui.painter().rect_filled(main, 0.0, theme::BACKGROUND);
        self.settings_page_view(ui, main, page);
        self.settings_sidebar(ui, sidebar, page);
        self.archive_context_menu(ui);
    }

    fn settings_sidebar(&mut self, ui: &mut Ui, rect: Rect, page: usize) {
        self.sidebar_chrome(ui, rect);
        let left = rect.left();
        // Search: the settings search is not ported; the field keeps its look.
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 16.0, 68.0), Vec2::splat(16.0)),
            16.0,
            theme::alpha(theme::SIDEBAR_MUTED, 0.8),
        );
        let input_rect = Rect::from_min_size(Pos2::new(left + 40.0, 66.5), Vec2::new(170.0, 21.0));
        let mut search_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        search_ui.add(
            egui::TextEdit::singleline(&mut self.settings_search)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::SIDEBAR_FOREGROUND)
                .hint_text(
                    egui::RichText::new("Search")
                        .font(font(14.0, Weight::Medium))
                        .color(theme::SIDEBAR_MUTED),
                )
                .desired_width(170.0),
        );
        let kbd = Rect::from_min_size(Pos2::new(left + 219.0, 66.0), Vec2::splat(20.0));
        ui.painter()
            .rect_filled(kbd, CornerRadius::same(4), theme::BACKGROUND);
        let key_width = widgets::text_width(ui, "/", font(12.0, Weight::Medium));
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(kbd.center().x - key_width / 2.0, kbd.top()),
                kbd.max,
            ),
            "/",
            font(12.0, Weight::Medium),
            theme::SIDEBAR_MUTED,
        );

        for (index, (label, icon)) in PAGES.iter().enumerate() {
            let item = Rect::from_min_size(
                Pos2::new(left + 8.0, 100.0 + index as f32 * 36.0),
                Vec2::new(239.0, 32.0),
            );
            let response = ui.interact(item, ui.id().with(("settings-nav", index)), Sense::click());
            let active = index == page;
            if active {
                ui.painter().rect_filled(
                    item,
                    CornerRadius::same(8),
                    theme::alpha(theme::SIDEBAR_FOREGROUND, 0.07),
                );
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(item, CornerRadius::same(8), theme::ACCENT);
            }
            let (icon_color, text_color) = if active || response.hovered() {
                (theme::SIDEBAR_FOREGROUND, theme::SIDEBAR_FOREGROUND)
            } else {
                (theme::SIDEBAR_ICON, theme::alpha(theme::SIDEBAR_MUTED, 0.8))
            };
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(
                    Pos2::new(item.left() + 10.0, item.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                16.0,
                icon_color,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(item.left() + 34.0, item.top()), item.max),
                label,
                font(14.0, Weight::Medium),
                text_color,
            );
            if response.clicked() {
                self.settings_page = Some(index);
                if index == ARCHIVE_PAGE {
                    self.request_archived();
                }
            }
        }

        let back = Rect::from_min_size(
            Pos2::new(left + 8.0, rect.bottom() - 36.0),
            Vec2::new(239.0, 32.0),
        );
        let response = ui.interact(back, ui.id().with("settings-back"), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(
                back,
                CornerRadius::same(8),
                theme::alpha(theme::SIDEBAR_FOREGROUND, 0.08),
            );
        }
        // Back rests like an inactive item and lights up on hover.
        let (back_icon, back_text) = if response.hovered() {
            (theme::SIDEBAR_FOREGROUND, theme::SIDEBAR_FOREGROUND)
        } else {
            (theme::SIDEBAR_ICON, theme::alpha(theme::SIDEBAR_MUTED, 0.8))
        };
        theme::paint_icon(
            ui,
            "arrow-left",
            Rect::from_min_size(
                Pos2::new(back.left() + 10.0, back.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            back_icon,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(back.left() + 34.0, back.top()), back.max),
            "Back",
            font(14.0, Weight::Medium),
            back_text,
        );
        if response.clicked() {
            self.settings_page = None;
        }
    }

    fn settings_page_view(&mut self, ui: &mut Ui, main: Rect, page: usize) {
        let (label, _) = PAGES[page];
        // Header: "Settings / Page", and Restore device defaults on General.
        let mut x = main.left() + 20.0;
        for (text, weight, color) in [
            ("Settings", Weight::Medium, theme::MUTED_FOREGROUND),
            ("/", Weight::Regular, theme::MUTED_FOREGROUND),
            (label, Weight::Medium, theme::FOREGROUND),
        ] {
            let width = widgets::text_width(ui, text, font(14.0, weight));
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, 16.0), Pos2::new(x + width + 1.0, 36.0)),
                text,
                font(14.0, weight),
                color,
            );
            x += width + 12.0;
        }
        if page == 0 {
            let changed = self
                .client_settings
                .as_object()
                .is_some_and(|o| o.iter().any(|(k, v)| default_client_settings()[k] != *v));
            let width =
                (widgets::text_width(ui, "Restore device defaults", font(12.0, Weight::Medium))
                    + 30.0)
                    .ceil();
            let button = Rect::from_min_size(
                Pos2::new(main.right() - 20.0 - width, 14.0),
                Vec2::new(width, 24.0),
            );
            let response = ui.interact(button, ui.id().with("restore-defaults"), Sense::click());
            if response.hovered() && changed {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(8), theme::ACCENT);
            }
            let alpha = if changed { 1.0 } else { 0.64 };
            theme::paint_icon(
                ui,
                "rotate-ccw",
                Rect::from_min_size(
                    Pos2::new(button.left() + 6.0, button.top()),
                    Vec2::new(14.0, 24.0),
                ),
                14.0,
                theme::alpha(theme::MUTED_FOREGROUND, alpha),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(button.left() + 22.0, button.top()), button.max),
                "Restore device defaults",
                font(12.0, Weight::Medium),
                theme::alpha(theme::FOREGROUND, alpha),
            );
            if response.clicked() && changed {
                self.client_settings = default_client_settings();
                self.diff_ignore_whitespace = true;
                self.diff_split = false;
            }
        }

        let body = Rect::from_min_max(Pos2::new(main.left(), theme::TOPBAR_HEIGHT), main.max);
        if page != 0 && page != ARCHIVE_PAGE {
            let note = "This settings page is not ported to the native client yet.";
            let width = widgets::text_width(ui, note, font(14.0, Weight::Regular));
            widgets::text(
                ui,
                Pos2::new(body.center().x - width / 2.0, body.top() + 40.0),
                note,
                font(14.0, Weight::Regular),
                theme::MUTED_FOREGROUND,
                20.0,
            );
            return;
        }
        let sections = if page == ARCHIVE_PAGE {
            self.archive_sections()
        } else {
            self.general_sections()
        };
        let mut actions: Vec<(String, RowAction)> = Vec::new();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical()
            .id_salt("settings-general")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let left = body.center().x - CARD_WIDTH / 2.0;
                let top = ui.max_rect().top();
                // Scope sentence (`SettingsScopeSentence`); the pickers are not ported.
                let mut y = top + 23.0;
                let mut x = left + 16.0;
                for (text, weight, color, chevron) in [
                    (
                        "Applying settings for",
                        Weight::Regular,
                        theme::MUTED_FOREGROUND,
                        false,
                    ),
                    ("All projects", Weight::Medium, theme::FOREGROUND, true),
                    ("across", Weight::Regular, theme::MUTED_FOREGROUND, false),
                    ("All environments", Weight::Medium, theme::FOREGROUND, true),
                ] {
                    let width = widgets::text_width(ui, text, font(16.0, weight));
                    widgets::text(ui, Pos2::new(x, y), text, font(16.0, weight), color, 24.0);
                    x += width;
                    if chevron {
                        theme::paint_icon(
                            ui,
                            "chevron-down",
                            Rect::from_min_size(Pos2::new(x + 6.0, y + 5.0), Vec2::splat(14.0)),
                            14.0,
                            theme::MUTED_FOREGROUND,
                        );
                        x += 26.0;
                    } else {
                        x += 6.0;
                    }
                }
                y += 25.0 + 32.0;
                for section in &sections {
                    if section.title == LEGACY {
                        // A collapsible header (`Collapsible`, min-h-8) instead of a title.
                        let header =
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(CARD_WIDTH, 32.0));
                        let response =
                            ui.interact(header, ui.id().with("settings-legacy"), Sense::click());
                        let color = if response.hovered() {
                            theme::FOREGROUND
                        } else {
                            theme::alpha(theme::FOREGROUND, 0.7)
                        };
                        let width = widgets::text_in(
                            ui,
                            Rect::from_min_max(Pos2::new(left + 16.0, y), header.max),
                            LEGACY,
                            font(14.0, Weight::Regular),
                            color,
                        );
                        theme::paint_icon(
                            ui,
                            if self.settings_legacy_open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            Rect::from_min_size(
                                Pos2::new(left + 16.0 + width + 8.0, y),
                                Vec2::new(16.0, 32.0),
                            ),
                            16.0,
                            theme::MUTED_FOREGROUND,
                        );
                        if response.clicked() {
                            self.settings_legacy_open = !self.settings_legacy_open;
                        }
                        y += 32.0;
                        if section.rows.is_empty() {
                            continue;
                        }
                    } else {
                        // Archive groups lead with the project icon (`gap-2`).
                        let mut title_left = left + 16.0;
                        if let Some(project) = &section.project {
                            crate::ui::project_icon::paint(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(title_left, y + 6.0),
                                    Vec2::splat(16.0),
                                ),
                                project,
                            );
                            title_left += 24.0;
                        }
                        widgets::text_in(
                            ui,
                            Rect::from_min_size(
                                Pos2::new(title_left, y),
                                Vec2::new(CARD_WIDTH, 28.0),
                            ),
                            &section.title,
                            font(14.0, Weight::Regular),
                            theme::alpha(theme::FOREGROUND, 0.7),
                        );
                        y += 28.0 + 10.0;
                    }
                    // Every row but the first carries a 1px divider.
                    let heights: Vec<f32> = section
                        .rows
                        .iter()
                        .enumerate()
                        .map(|(i, row)| row_height(ui, row) - if i == 0 { 1.0 } else { 0.0 })
                        .collect();
                    let card = Rect::from_min_size(
                        Pos2::new(left, y),
                        Vec2::new(CARD_WIDTH, heights.iter().sum::<f32>() + 2.0),
                    );
                    ui.painter().rect(
                        card,
                        CornerRadius::same(14),
                        CARD_FILL,
                        Stroke::new(1.0, CARD_BORDER),
                        StrokeKind::Inside,
                    );
                    let mut row_top = card.top() + 1.0;
                    for (index, (row, height)) in section.rows.iter().zip(&heights).enumerate() {
                        let rect = Rect::from_min_size(
                            Pos2::new(card.left() + 1.0, row_top),
                            Vec2::new(CARD_WIDTH - 2.0, *height),
                        );
                        if index > 0 {
                            ui.painter().rect_filled(
                                Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
                                0.0,
                                ROW_DIVIDER,
                            );
                        }
                        if let Some(action) = self.settings_row(ui, rect, row, index > 0) {
                            actions.push((row.id.clone(), action));
                        }
                        row_top += height;
                    }
                    y = card.bottom() + 32.0;
                }
                ui.allocate_rect(
                    Rect::from_min_max(
                        Pos2::new(left, top),
                        Pos2::new(left + CARD_WIDTH, y + 48.0),
                    ),
                    Sense::hover(),
                );
            });
        for (id, action) in actions {
            if page == ARCHIVE_PAGE {
                self.apply_archive(&id, action);
            } else {
                self.apply_general(&id, action);
            }
        }
    }

    /// One `SettingsRow`: title (with the inheritance and reset slots),
    /// description and status on the left, the control on the right.
    fn settings_row(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        row: &Row,
        divided: bool,
    ) -> Option<RowAction> {
        let inner_top = rect.top() + if divided { 1.0 } else { 0.0 } + ROW_PAD_Y;
        let mut left = rect.left() + ROW_PAD_X;
        let base_left = left;
        // Archived rows open Unarchive / Delete on a secondary click.
        let mut menu = None;
        if row.id.starts_with("archived:") {
            let response = ui.interact(
                rect,
                ui.id().with(("settings-row", &row.id)),
                Sense::click(),
            );
            if response.secondary_clicked() {
                menu = response.interact_pointer_pos().map(RowAction::Menu);
            }
        }
        // The empty archive row leads its title with the archive icon (`gap-2`).
        if row.id == "archive-empty" && self.archived.is_some() {
            theme::paint_icon(
                ui,
                "archive",
                Rect::from_min_size(Pos2::new(left, inner_top), Vec2::new(14.0, 20.0)),
                14.0,
                theme::MUTED_FOREGROUND,
            );
            left += 22.0;
        }
        let title_width = widgets::text_width(ui, &row.title, font(14.0, Weight::Medium));
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(left, inner_top),
                Vec2::new(title_width + 1.0, 20.0),
            ),
            &row.title,
            font(14.0, Weight::Medium),
            theme::FOREGROUND,
        );
        let mut action = None;
        let mut slot_x = left + title_width + 6.0;
        // `AboutVersionTitle`: the version in small mono beside "Version".
        if row.id == "about-version" {
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(left + title_width + 8.0, inner_top + 1.0),
                    Vec2::new(80.0, 20.0),
                ),
                env!("CARGO_PKG_VERSION"),
                egui::FontId::monospace(10.0),
                theme::MUTED_FOREGROUND,
            );
        }
        // Background activity explains its shared policy in an info tooltip.
        if row.id == "background-activity" {
            let slot = Rect::from_min_size(Pos2::new(slot_x, inner_top), Vec2::splat(20.0));
            widgets::icon_button(
                ui,
                slot,
                ("settings-info", &row.id),
                "info",
                14.0,
                theme::MUTED_FOREGROUND,
                "This shared policy gates background work such as Git refreshes and provider health checks.",
            );
            slot_x += 26.0;
        }
        if row.layers {
            let slot = Rect::from_min_size(Pos2::new(slot_x, inner_top), Vec2::splat(20.0));
            theme::paint_icon(ui, "layers", slot, 12.0, theme::MUTED_FOREGROUND);
            slot_x += 26.0;
        }
        if row.reset {
            let slot = Rect::from_min_size(Pos2::new(slot_x, inner_top), Vec2::splat(20.0));
            if widgets::icon_button(
                ui,
                slot,
                ("settings-reset", &row.id),
                "undo-2",
                12.0,
                theme::MUTED_FOREGROUND,
                "Reset to default",
            )
            .clicked()
            {
                action = Some(RowAction::Reset);
            }
        }
        // A title with an inline icon is 22px tall (`inline-flex` line box).
        let title_height = if row.id == "archive-empty" {
            22.0
        } else {
            20.0
        };
        let mut y = inner_top + title_height;
        let control_width = control_width(ui, &row.control);
        let text_width =
            (rect.width() - 2.0 * ROW_PAD_X - 32.0 - control_width).min(DESCRIPTION_MAX);
        if !row.description.is_empty() {
            let galley = widgets::galley(
                ui,
                &row.description,
                font(12.0, Weight::Regular),
                theme::alpha(theme::MUTED_FOREGROUND, 0.8),
                18.0,
                text_width,
            );
            let height = galley.size().y;
            ui.painter().galley(
                Pos2::new(base_left, y + 4.0),
                galley,
                theme::alpha(theme::MUTED_FOREGROUND, 0.8),
            );
            y += 4.0 + height;
        }
        if let Some(status) = &row.status {
            widgets::text(
                ui,
                Pos2::new(base_left, y + 6.0),
                status,
                font(12.0, Weight::Regular),
                theme::MUTED_FOREGROUND,
                16.0,
            );
        }
        let right = rect.right() - ROW_PAD_X;
        let center_y = rect.top()
            + (if divided { 1.0 } else { 0.0 })
            + (rect.height() - if divided { 1.0 } else { 0.0 }) / 2.0;
        self.settings_control(ui, row, right, center_y)
            .or(action)
            .or(menu)
    }

    fn settings_control(
        &mut self,
        ui: &mut Ui,
        row: &Row,
        right: f32,
        center_y: f32,
    ) -> Option<RowAction> {
        match &row.control {
            Control::Switch(on) => {
                let track = Rect::from_min_size(
                    Pos2::new(right - 30.0, center_y - 9.0),
                    Vec2::new(30.0, 18.0),
                );
                let response = ui.interact(
                    track,
                    ui.id().with(("settings-switch", &row.id)),
                    Sense::click(),
                );
                ui.painter().rect_filled(
                    track,
                    CornerRadius::same(9),
                    if *on {
                        theme::PRIMARY
                    } else {
                        theme::white(0.08)
                    },
                );
                let thumb_x = if *on {
                    track.left() + 14.0
                } else {
                    track.left() + 2.0
                };
                ui.painter().rect_filled(
                    Rect::from_min_size(Pos2::new(thumb_x, track.top() + 2.0), Vec2::splat(14.0)),
                    CornerRadius::same(7),
                    theme::BACKGROUND,
                );
                response.clicked().then_some(RowAction::Toggle(!on))
            }
            Control::Select {
                value,
                options,
                width,
                icon,
            } => {
                let width = select_width(ui, &row.control).unwrap_or(*width);
                let trigger = Rect::from_min_size(
                    Pos2::new(right - width, center_y - 14.0),
                    Vec2::new(width, 28.0),
                );
                let menu_id = format!("settings-select:{}", row.id);
                let open = self.open_menu.as_deref() == Some(menu_id.as_str());
                let response = ui.interact(
                    trigger,
                    ui.id().with(("settings-select", &row.id)),
                    Sense::click(),
                );
                ui.painter().rect(
                    trigger,
                    CornerRadius::same(10),
                    if response.hovered() || open {
                        theme::white(0.05)
                    } else {
                        theme::CONTROL_SURFACE
                    },
                    Stroke::new(1.0, theme::INPUT),
                    StrokeKind::Inside,
                );
                let mut text_left = trigger.left() + 10.0;
                if let Some(icon) = icon {
                    theme::paint_icon(
                        ui,
                        icon,
                        Rect::from_min_size(
                            Pos2::new(text_left, trigger.top()),
                            Vec2::new(14.0, 28.0),
                        ),
                        14.0,
                        theme::MUTED_FOREGROUND,
                    );
                    text_left += 20.0;
                }
                let label = options
                    .iter()
                    .find(|(v, _)| v == value)
                    .map_or(value.as_str(), |(_, l)| l.as_str());
                let galley = widgets::galley_truncated(
                    ui,
                    label,
                    font(14.0, Weight::Regular),
                    theme::FOREGROUND,
                    20.0,
                    trigger.right() - 18.0 - text_left,
                );
                ui.painter().galley(
                    Pos2::new(text_left, trigger.center().y - 10.0),
                    galley,
                    theme::FOREGROUND,
                );
                theme::paint_icon(
                    ui,
                    "chevron-down",
                    Rect::from_min_size(
                        Pos2::new(trigger.right() - 18.0, trigger.top()),
                        Vec2::new(12.0, 28.0),
                    ),
                    12.0,
                    theme::MUTED_FOREGROUND,
                );
                if response.clicked() {
                    self.open_menu = if open { None } else { Some(menu_id.clone()) };
                }
                let entries: Vec<MenuEntry> = options
                    .iter()
                    .map(|(v, l)| {
                        MenuEntry::Item(MenuItem {
                            selected: v == value,
                            ..MenuItem::new(l)
                        })
                    })
                    .collect();
                widgets::menu_aligned(
                    ui,
                    &mut self.open_menu,
                    &menu_id,
                    trigger,
                    false,
                    true,
                    &entries,
                )
                .map(|index| RowAction::Select(options[index].0.clone()))
            }
            Control::Number(value) => {
                let field = Rect::from_min_size(
                    Pos2::new(right - 96.0, center_y - 14.0),
                    Vec2::new(96.0, 28.0),
                );
                let text = self
                    .settings_edits
                    .entry(row.id.to_owned())
                    .or_insert_with(|| value.to_string());
                let (changed, done, buffer) = number_or_text_field(ui, field, &row.id, text, "");
                *self
                    .settings_edits
                    .get_mut(&row.id)
                    .expect("inserted above") = buffer.clone();
                if done {
                    self.settings_edits.remove(&row.id);
                    return buffer
                        .trim()
                        .parse::<i64>()
                        .ok()
                        .filter(|n| n != value)
                        .map(RowAction::Number);
                }
                let _ = changed;
                None
            }
            Control::Text(value) => {
                let field = Rect::from_min_size(
                    Pos2::new(right - 288.0, center_y - 14.0),
                    Vec2::new(288.0, 28.0),
                );
                let text = self
                    .settings_edits
                    .entry(row.id.to_owned())
                    .or_insert_with(|| value.clone());
                let (_, done, buffer) = number_or_text_field(ui, field, &row.id, text, "~/");
                *self
                    .settings_edits
                    .get_mut(&row.id)
                    .expect("inserted above") = buffer.clone();
                if done {
                    self.settings_edits.remove(&row.id);
                    return (buffer.trim() != value)
                        .then(|| RowAction::Text(buffer.trim().to_owned()));
                }
                None
            }
            Control::Model {
                label,
                icon,
                traits,
            } => {
                let mut x = right;
                let mut clicked = false;
                let mut buttons: Vec<(String, Option<&'static str>)> = Vec::new();
                if let Some(traits) = traits {
                    buttons.push((traits.clone(), None));
                }
                buttons.push((label.clone(), *icon));
                for (index, (text, icon)) in buttons.iter().enumerate() {
                    let text_width = widgets::text_width(ui, text, font(14.0, Weight::Medium));
                    let width = (10.0
                        + if icon.is_some() { 22.0 } else { 0.0 }
                        + text_width
                        + 4.0
                        + 14.0
                        + 10.0)
                        .ceil();
                    let button = Rect::from_min_size(
                        Pos2::new(x - width, center_y - 14.0),
                        Vec2::new(width, 28.0),
                    );
                    x = button.left() - 6.0;
                    let response = ui.interact(
                        button,
                        ui.id().with(("settings-model", &row.id, index)),
                        Sense::click(),
                    );
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(button, CornerRadius::same(8), theme::ACCENT);
                    }
                    let mut tx = button.left() + 10.0;
                    if let Some(icon) = icon {
                        theme::paint_icon(
                            ui,
                            icon,
                            Rect::from_min_size(
                                Pos2::new(tx + 1.0, button.top()),
                                Vec2::new(16.0, 28.0),
                            ),
                            16.0,
                            theme::MUTED_FOREGROUND,
                        );
                        tx += 23.0;
                    }
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(Pos2::new(tx, button.top()), button.max),
                        text,
                        font(14.0, Weight::Medium),
                        theme::MUTED_FOREGROUND,
                    );
                    theme::paint_icon(
                        ui,
                        "chevron-down",
                        Rect::from_min_size(
                            Pos2::new(tx + text_width + 4.0, button.top()),
                            Vec2::new(14.0, 28.0),
                        ),
                        14.0,
                        theme::MUTED_FOREGROUND,
                    );
                    clicked |= response.clicked();
                }
                clicked.then_some(RowAction::Click)
            }
            Control::None => None,
            Control::OutlineXs { label, icon } => {
                // A bare label (no `ml-0.5`) starts 2px closer to the icon.
                let width = widgets::outline_button_width(ui, true, label) - 2.0;
                let button = Rect::from_min_size(
                    Pos2::new(right - width, center_y - 12.0),
                    Vec2::new(width, 24.0),
                );
                let response = widgets::outline_button(
                    ui,
                    button,
                    ("settings-outline", &row.id),
                    Some((icon, theme::MUTED_FOREGROUND)),
                    "",
                    CornerRadius::same(8),
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(button.left() + 22.0, button.top()), button.max),
                    label,
                    font(12.0, Weight::Medium),
                    theme::FOREGROUND,
                );
                response.clicked().then_some(RowAction::Click)
            }
            Control::Button(label) => {
                let width = widgets::outline_button_width(ui, false, label) + 6.0;
                let button = Rect::from_min_size(
                    Pos2::new(right - width, center_y - 14.0),
                    Vec2::new(width, 28.0),
                );
                let response = ui.interact(
                    button,
                    ui.id().with(("settings-button", &row.id)),
                    Sense::click(),
                );
                ui.painter().rect(
                    button,
                    CornerRadius::same(10),
                    if response.hovered() {
                        theme::white(0.05)
                    } else {
                        theme::CONTROL_SURFACE
                    },
                    Stroke::new(1.0, theme::INPUT),
                    StrokeKind::Inside,
                );
                let text_width = widgets::text_width(ui, label, font(14.0, Weight::Medium));
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(button.center().x - text_width / 2.0, button.top()),
                        button.max,
                    ),
                    label,
                    font(14.0, Weight::Medium),
                    theme::FOREGROUND,
                );
                response.clicked().then_some(RowAction::Click)
            }
        }
    }
}

/// The width of an auto-sized select: its label, padding and chevron.
fn select_width(ui: &Ui, control: &Control) -> Option<f32> {
    let Control::Select {
        value,
        options,
        width,
        icon,
    } = control
    else {
        return None;
    };
    if *width > AUTO_WIDTH {
        return None;
    }
    let label = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or(value.as_str(), |(_, l)| l.as_str());
    let icon = if icon.is_some() { 20.0 } else { 0.0 };
    Some((widgets::text_width(ui, label, font(14.0, Weight::Regular)) + icon + 30.0).ceil())
}

/// A bordered single-line field; returns (changed, committed, text). Enter
/// or losing focus commits.
fn number_or_text_field(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    text: &mut String,
    placeholder: &str,
) -> (bool, bool, String) {
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        theme::CONTROL_SURFACE,
        Stroke::new(1.0, theme::INPUT),
        StrokeKind::Inside,
    );
    let inner = Rect::from_min_max(
        rect.min + Vec2::new(10.0, 5.0),
        rect.max - Vec2::new(10.0, 3.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let response = child.add(
        egui::TextEdit::singleline(text)
            .id_salt(("settings-field", id))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(font(14.0, Weight::Regular))
            .text_color(theme::FOREGROUND)
            .hint_text(
                egui::RichText::new(placeholder)
                    .font(font(14.0, Weight::Regular))
                    .color(theme::MUTED_FOREGROUND),
            )
            .desired_width(rect.width() - 20.0)
            .vertical_align(egui::Align::Center),
    );
    let committed = response.lost_focus();
    (response.changed(), committed, text.clone())
}

fn control_width(ui: &Ui, control: &Control) -> f32 {
    match control {
        Control::Switch(_) => 30.0,
        Control::Select { width, .. } => select_width(ui, control).unwrap_or(*width),
        Control::Number(_) => 96.0,
        Control::Text(_) => 288.0,
        Control::Model { label, traits, .. } => {
            let w = |t: &str| widgets::text_width(ui, t, font(14.0, Weight::Medium)) + 60.0;
            w(label) + traits.as_deref().map_or(0.0, |t| w(t) + 8.0)
        }
        Control::Button(label) => widgets::outline_button_width(ui, false, label) + 6.0,
        Control::OutlineXs { label, .. } => widgets::outline_button_width(ui, true, label) - 2.0,
        Control::None => 0.0,
    }
}

/// Row height: padding, the 20px title line, the wrapped description and the
/// optional status, plus the divider above every row but the first.
fn row_height(ui: &Ui, row: &Row) -> f32 {
    let text_width = (CARD_WIDTH - 2.0 - 2.0 * ROW_PAD_X - 32.0 - control_width(ui, &row.control))
        .min(DESCRIPTION_MAX);
    let title = if row.id == "archive-empty" {
        22.0
    } else {
        20.0
    };
    let mut height = 2.0 * ROW_PAD_Y + title;
    if !row.description.is_empty() {
        let galley = widgets::galley(
            ui,
            &row.description,
            font(12.0, Weight::Regular),
            theme::MUTED_FOREGROUND,
            18.0,
            text_width,
        );
        height += 4.0 + galley.size().y;
    }
    if row.status.is_some() {
        height += 4.0 + 18.0;
    }
    height + 1.0
}

/// `formatRelativeTimeLabel`: "just now", "5m ago", "3h ago", "2d ago".
fn relative_time_label(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(date) = crate::timeline::parse_time(iso) else {
        return String::new();
    };
    let seconds = (now - date).num_seconds();
    if seconds < 60 {
        return "just now".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

impl App {
    /// Loads the archived threads (`orchestration.getArchivedShellSnapshot`).
    pub(crate) fn request_archived(&mut self) {
        self.archived = None;
        self.send_request(
            "archived".into(),
            "orchestration.getArchivedShellSnapshot",
            json!({}),
        );
    }

    /// `ArchivedThreadsPanel`: archived threads grouped by project, newest
    /// archive first, or the empty, loading or error row.
    fn archive_sections(&self) -> Vec<Section> {
        let empty = |title: &str, description: String| {
            vec![Section {
                title: "Archived threads".into(),
                project: None,
                rows: vec![Row {
                    id: "archive-empty".into(),
                    title: title.into(),
                    description,
                    status: None,
                    layers: false,
                    reset: false,
                    control: Control::None,
                }],
            }]
        };
        let snapshot = match &self.archived {
            None => {
                return empty(
                    "Loading archived threads",
                    "Checking connected environments.".into(),
                );
            }
            Some(Err(error)) => return empty("Could not load archived threads", error.clone()),
            Some(Ok(snapshot)) => snapshot,
        };
        let projects: Vec<crate::model::Project> = snapshot["projects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect();
        let threads: Vec<&Value> = snapshot["threads"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let mut sections = Vec::new();
        for project in projects {
            let mut project_threads: Vec<&&Value> = threads
                .iter()
                .filter(|t| t["projectId"] == project.id.as_str())
                .collect();
            if project_threads.is_empty() {
                continue;
            }
            let key = |t: &Value| {
                t["archivedAt"]
                    .as_str()
                    .or(t["createdAt"].as_str())
                    .unwrap_or_default()
                    .to_owned()
            };
            project_threads.sort_by(|a, b| {
                key(b)
                    .cmp(&key(a))
                    .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
            });
            let rows = project_threads
                .iter()
                .map(|thread| {
                    let created = thread["createdAt"].as_str().unwrap_or_default();
                    let archived = thread["archivedAt"].as_str().unwrap_or(created);
                    Row {
                        id: format!("archived:{}", thread["id"].as_str().unwrap_or_default()),
                        title: thread["title"].as_str().unwrap_or_default().to_owned(),
                        description: format!(
                            "Archived {} \u{b7} Created {}",
                            relative_time_label(archived, self.now),
                            relative_time_label(created, self.now)
                        ),
                        status: None,
                        layers: false,
                        reset: false,
                        control: Control::OutlineXs {
                            label: "Unarchive",
                            icon: "archive-x",
                        },
                    }
                })
                .collect();
            sections.push(Section {
                title: project.title.clone(),
                project: Some(project),
                rows,
            });
        }
        if sections.is_empty() {
            return empty(
                "No archived threads",
                "Archived threads will appear here.".into(),
            );
        }
        sections
    }

    fn apply_archive(&mut self, id: &str, action: RowAction) {
        let Some(thread_id) = id.strip_prefix("archived:") else {
            return;
        };
        match action {
            RowAction::Click => self.unarchive(thread_id),
            RowAction::Menu(at) => {
                self.archive_menu = Some((thread_id.to_owned(), context_menu::State::new(at)))
            }
            _ => {}
        }
    }

    /// Unarchives a thread and drops it from the loaded list right away.
    fn unarchive(&mut self, thread_id: &str) {
        self.dispatch(crate::model::command("thread.unarchive", thread_id));
        if let Some(Ok(snapshot)) = &mut self.archived
            && let Some(threads) = snapshot["threads"].as_array_mut()
        {
            threads.retain(|t| t["id"] != thread_id);
        }
    }

    /// The archived row's context menu: Unarchive, Delete.
    fn archive_context_menu(&mut self, ui: &mut Ui) {
        let Some((thread_id, state)) = self.archive_menu.take() else {
            return;
        };
        let items = vec![
            context_menu::Item::new("unarchive", "Unarchive"),
            context_menu::Item {
                destructive: true,
                ..context_menu::Item::new("delete", "Delete")
            },
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.archive_menu = Some((thread_id.clone(), state));
        }
        match clicked.as_deref() {
            Some("unarchive") => self.unarchive(&thread_id),
            Some("delete") => {
                let title = self
                    .archived
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .and_then(|s| {
                        s["threads"]
                            .as_array()?
                            .iter()
                            .find(|t| t["id"] == thread_id.as_str())
                            .and_then(|t| t["title"].as_str().map(str::to_owned))
                    })
                    .unwrap_or_default();
                let command = crate::model::command("thread.delete", &thread_id);
                if self.client_settings["confirmThreadDelete"] == false {
                    self.dispatch(command);
                    self.request_archived();
                } else {
                    self.confirm = Some(crate::ui::thread_menu::Confirm {
                        title: format!("Delete thread \"{title}\"?"),
                        message: "This permanently clears conversation history for this thread."
                            .into(),
                        confirm_label: "Delete",
                        command,
                    });
                }
            }
            _ => {}
        }
    }
}
