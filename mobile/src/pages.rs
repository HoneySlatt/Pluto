//! Pages pushed from the right: Settings (the desktop's `settings.rs` labels,
//! kept to what a phone needs), Projects and a project (`projects_page.rs`),
//! and the thread, diff, task and usage pages from their own modules.

use dioxus::prelude::*;
use pluto_core::model::Project;
use serde_json::{Value, json};

use crate::badge::Badge;
use crate::catalog;
use crate::icons::Icon;
use crate::nav::{Page, Sheet, Ui};
use crate::session::Net;
use crate::sheets::{Choice, ModelList};

#[component]
pub fn PageView(page: Page) -> Element {
    match page {
        Page::Settings => rsx! { Settings {} },
        Page::Thread(id) => rsx! { crate::thread::ThreadView { id, page: true } },
        Page::Diff { thread, run, focus } => rsx! { crate::diff::DiffPage { thread, run, focus } },
        Page::Projects => rsx! { Projects {} },
        Page::Project(id) => rsx! { ProjectPage { id } },
        Page::Tasks => rsx! { crate::tasks::Tasks {} },
        Page::TaskEdit(id) => rsx! { crate::tasks::TaskEditor { id } },
        Page::Usage => rsx! { crate::usage::Usage {} },
        Page::Memory => rsx! { crate::memory::Memory {} },
        Page::Licenses => rsx! { crate::licenses::Licenses {} },
    }
}

/// A page's header: back, the title, an optional action.
#[component]
pub fn PageHead(title: String, #[props(default)] action: Option<Element>) -> Element {
    let ui = use_context::<Ui>();
    rsx! {
        header { class: "thead",
            button { class: "disc", "aria-label": "Back", onclick: move |_| ui.back(),
                Icon { name: "chevron-left" }
            }
            div { class: "ttl", b { "{title}" } }
            if let Some(action) = action {
                {action}
            } else {
                span { class: "ghost" }
            }
        }
    }
}

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
const WORKSPACES: [(&str, &str, &str); 2] = [
    (
        "local",
        "Local checkout",
        "Work in the project’s own folder",
    ),
    (
        "worktree",
        "New worktree",
        "A fresh branch in its own folder",
    ),
];
const DIFF_STATES: [(&str, &str, &str); 2] =
    [("expanded", "Expanded", ""), ("collapsed", "Collapsed", "")];

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

fn label_of(list: &[(&str, &'static str, &str)], value: &str) -> &'static str {
    list.iter()
        .find(|o| o.0 == value)
        .map(|o| o.1)
        .unwrap_or(list[0].1)
}

/// A Settings row opening a choice sheet.
#[component]
fn SetChoice(
    icon: &'static str,
    label: &'static str,
    value: String,
    #[props(default)] stacked: bool,
    #[props(default)] mark: Option<Asset>,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button { class: "set", onclick: move |_| onclick.call(()),
            Icon { name: icon }
            span { class: "grow",
                "{label}"
                if stacked {
                    small { "{value}" }
                }
            }
            if !stacked {
                span { class: "val",
                    if let Some(mark) = mark {
                        img { src: mark, alt: "" }
                    }
                    "{value}"
                }
            }
            Icon { name: "chevron-right", class: "chev" }
        }
    }
}

#[component]
fn SetToggle(
    icon: &'static str,
    label: &'static str,
    #[props(default)] detail: &'static str,
    on: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "set",
            Icon { name: icon }
            span { class: "grow",
                "{label}"
                if !detail.is_empty() {
                    small { "{detail}" }
                }
            }
            span { class: "toggle",
                input { r#type: "checkbox", checked: on, onchange: move |e| onchange.call(e.checked()) }
                span {}
            }
        }
    }
}

#[component]
fn Settings() -> Element {
    let ui = use_context::<Ui>();
    let mut session = ui.ctx.session;
    let s = session.read();
    let settings = s.settings().clone();
    let providers = s.providers().to_vec();
    let chats = s.space_project(pluto_core::spaces::Space::Chats).cloned();
    let chat_model = chats
        .as_ref()
        .and_then(|p| p.default_model_selection.as_ref())
        .and_then(catalog::selection);
    let code_model = catalog::selection(&settings["defaultModelSelection"]);
    let model_value = |m: &Option<pluto_core::model::ModelSelection>| match m {
        Some(m) => {
            let (label, driver) = catalog::label(&providers, m);
            (label, catalog::mark(&driver))
        }
        None => ("Automatic".to_owned(), None),
    };
    let (chat_label, chat_mark) = model_value(&chat_model);
    let (code_label, code_mark) = model_value(&code_model);
    let access = catalog::access(
        settings["defaultRuntimeMode"]
            .as_str()
            .unwrap_or("full-access"),
    )
    .1;
    let workspace = label_of(
        &WORKSPACES,
        settings["defaultThreadEnvMode"].as_str().unwrap_or("local"),
    );
    let time = label_of(
        &TIME_FORMATS,
        s.prefs.get("timestampFormat").as_str().unwrap_or("locale"),
    );
    let streaming = label_of(
        &STREAMING,
        settings["responseStreamingMode"]
            .as_str()
            .unwrap_or("paragraph"),
    );
    let hide_ws = s.prefs.flag("diffIgnoreWhitespace");
    let diff_state = if s.prefs.flag("diffFilesCollapsed") {
        "Collapsed"
    } else {
        "Expanded"
    };
    let confirm_delete = s.prefs.flag("confirmThreadDelete");
    let host = s.host();
    let state = match s.net {
        Net::Reconnecting(_) => "Reconnecting…",
        Net::Offline => "Offline",
        _ => "Connected",
    };
    let dot = match s.net {
        Net::Reconnecting(_) => "online warn",
        Net::Offline => "online bad",
        _ => "online",
    };
    let remembered = crate::secret::load_server().is_some();
    let version = s.state.config["environment"]["serverVersion"]
        .as_str()
        .or(s.state.config["serverVersion"].as_str())
        .map(|v| format!("{v} · protocol 2"))
        .unwrap_or_else(|| "protocol 2".into());
    drop(s);
    rsx! {
        section { class: "page",
            PageHead { title: "Settings" }
            div { class: "pscroll",
                div { class: "srv",
                    crate::Avatar {}
                    div { class: "who", b { "Pluto server" } span { i { class: dot } "{host} · {state}" } }
                }
                div { class: "glabel", "Chat" }
                div { class: "group",
                    SetChoice { icon: "message-square", label: "Default model", value: chat_label, mark: chat_mark, onclick: move |_| ui.open_sheet(Sheet::Setting("chatModel")) }
                    button { class: "set", onclick: move |_| ui.open_page(Page::Memory),
                        Icon { name: "brain" }
                        span { class: "grow", "Personalization" }
                        span { class: "val", "Memory" }
                        Icon { name: "chevron-right", class: "chev" }
                    }
                }
                div { class: "glabel", "New threads" }
                div { class: "group",
                    SetChoice { icon: "bot", label: "Model", value: code_label, mark: code_mark, onclick: move |_| ui.open_sheet(Sheet::Setting("codeModel")) }
                    SetChoice { icon: "shield", label: "Permissions", value: access.to_owned(), onclick: move |_| ui.open_sheet(Sheet::Setting("codeAccess")) }
                    SetChoice { icon: "folder", label: "Workspace", value: workspace.to_owned(), onclick: move |_| ui.open_sheet(Sheet::Setting("workspace")) }
                }
                div { class: "glabel", "Display" }
                div { class: "group",
                    SetChoice { icon: "clock", label: "Time format", value: time.to_owned(), onclick: move |_| ui.open_sheet(Sheet::Setting("timeFormat")) }
                    SetChoice { icon: "text-wrap", label: "Response streaming", value: streaming.to_owned(), stacked: true, onclick: move |_| ui.open_sheet(Sheet::Setting("streaming")) }
                    SetToggle { icon: "file-diff", label: "Hide whitespace changes", on: hide_ws, onchange: move |on| session.write().prefs.set("diffIgnoreWhitespace", json!(on)) }
                    SetChoice { icon: "files", label: "Default diff file state", value: diff_state.to_owned(), onclick: move |_| ui.open_sheet(Sheet::Setting("diffState")) }
                }
                div { class: "glabel", "Confirmations" }
                div { class: "group",
                    SetToggle { icon: "trash-2", label: "Delete confirmation", detail: "Ask before deleting a thread and its chat history.", on: confirm_delete, onchange: move |on| session.write().prefs.set("confirmThreadDelete", json!(on)) }
                }
                div { class: "glabel", "Server" }
                div { class: "group",
                    div { class: "set", Icon { name: "link-2" } span { class: "grow", "Address" } span { class: "val", "{host}" } }
                    div { class: "set", Icon { name: "key-round" }
                        span { class: "grow", "Token"
                            small { if remembered { "Stored encrypted on this phone" } else { "Kept in memory until Pluto closes" } }
                        }
                    }
                    if remembered {
                        button { class: "set", onclick: move |_| {
                                crate::secret::forget_token();
                                ui.ctx.toast("Token forgotten. Pluto will ask for it next time");
                            },
                            Icon { name: "eye-off" }
                            span { class: "grow", "Forget token" small { "Ask for it on the next launch" } }
                        }
                    }
                    button { class: "set danger", onclick: move |_| {
                            ui.disconnect();
                        },
                        Icon { name: "log-out" }
                        span { class: "grow", "Disconnect" }
                    }
                }
                p { class: "note", "Plain http is accepted on your home network only. There the token crosses the Wi-Fi unencrypted." }
                div { class: "glabel", "About" }
                div { class: "group",
                    div { class: "set", Icon { name: "server" } span { class: "grow", "Server" } span { class: "val", "{version}" } }
                    button { class: "set", onclick: move |_| ui.open_page(Page::Licenses),
                        Icon { name: "info" }
                        span { class: "grow", "Open source licenses" }
                        Icon { name: "chevron-right", class: "chev" }
                    }
                }
                p { class: "note", "Providers, appearance, keybindings and storage are set in Pluto on the computer." }
            }
        }
    }
}

/// A Settings choice (`Sheet::Setting`).
#[component]
pub fn SettingSheet(setting: &'static str) -> Element {
    let key = setting;
    let ui = use_context::<Ui>();
    let mut session = ui.ctx.session;
    let s = session.read();
    let settings = s.settings().clone();
    let chats = s.space_project(pluto_core::spaces::Space::Chats).cloned();
    let time = s
        .prefs
        .get("timestampFormat")
        .as_str()
        .unwrap_or("locale")
        .to_owned();
    let collapsed = s.prefs.flag("diffFilesCollapsed");
    drop(s);
    let options = |list: &'static [(&'static str, &'static str, &'static str)],
                   current: String,
                   apply: fn(Ui, &str)| {
        rsx! {
            for (value, label, detail) in list.iter().copied() {
                Choice { key: "{value}", label, detail, checked: current == value, onpick: move |_| { apply(ui, value); ui.back(); } }
            }
        }
    };
    match key {
        "chatModel" => {
            let Some(project) = chats else {
                return rsx! { p { class: "note", "Open Chat once in Pluto on the computer: it creates the chat space." } };
            };
            let current = project
                .default_model_selection
                .as_ref()
                .and_then(catalog::selection)
                .unwrap_or_default();
            rsx! {
                ModelList { chat: true, current, onpick: move |picked| {
                    ui.ctx.dispatch(json!({
                        "type": "project.update",
                        "commandId": uuid::Uuid::new_v4().to_string(),
                        "projectId": project.id,
                        "defaultModelSelection": catalog::wire(&picked),
                    }));
                    ui.back();
                } }
            }
        }
        "codeModel" => {
            let current =
                catalog::selection(&settings["defaultModelSelection"]).unwrap_or_default();
            rsx! {
                ModelList { chat: false, current, onpick: move |picked| {
                    ui.ctx.update_setting("defaultModelSelection", catalog::wire(&picked));
                    ui.back();
                } }
            }
        }
        "codeAccess" => {
            let current = settings["defaultRuntimeMode"]
                .as_str()
                .unwrap_or("full-access")
                .to_owned();
            rsx! {
                for (value, label, _, detail) in catalog::ACCESS {
                    Choice { key: "{value}", label, detail, checked: current == value, onpick: move |_| {
                        ui.ctx.update_setting("defaultRuntimeMode", json!(value));
                        ui.back();
                    } }
                }
            }
        }
        "workspace" => options(
            &WORKSPACES,
            settings["defaultThreadEnvMode"]
                .as_str()
                .unwrap_or("local")
                .to_owned(),
            |ui, v| ui.ctx.update_setting("defaultThreadEnvMode", json!(v)),
        ),
        "streaming" => options(
            &STREAMING,
            settings["responseStreamingMode"]
                .as_str()
                .unwrap_or("paragraph")
                .to_owned(),
            |ui, v| ui.ctx.update_setting("responseStreamingMode", json!(v)),
        ),
        "timeFormat" => options(&TIME_FORMATS, time, |ui, v| {
            ui.ctx
                .session
                .clone()
                .write()
                .prefs
                .set("timestampFormat", json!(v))
        }),
        "diffState" => {
            let current = if collapsed { "collapsed" } else { "expanded" }.to_owned();
            rsx! {
                for (value, label, _) in DIFF_STATES {
                    Choice { key: "{value}", label, checked: current == value, onpick: move |_| {
                        session.write().prefs.set("diffFilesCollapsed", json!(value == "collapsed"));
                        ui.back();
                    } }
                }
            }
        }
        _ => rsx! {},
    }
}

/// Projects (`#projects`): one row per project with its threads and state.
#[component]
fn Projects() -> Element {
    let ui = use_context::<Ui>();
    let s = ui.ctx.session.read();
    let listed = s.listed(false);
    let rows: Vec<(Project, usize, usize, usize)> = s
        .code_projects()
        .into_iter()
        .map(|p| {
            let threads: Vec<_> = listed.iter().filter(|t| t.project_id == p.id).collect();
            let waiting = threads
                .iter()
                .filter(|t| t.has_pending_approvals || t.has_pending_user_input)
                .count();
            let working = threads.iter().filter(|t| t.is_running()).count();
            (p.clone(), threads.len(), waiting, working)
        })
        .collect();
    drop(s);
    rsx! {
        section { class: "page",
            PageHead { title: "Projects" }
            div { class: "pscroll",
                if rows.is_empty() {
                    div { class: "empty-page", b { "No projects" } "Projects are added in Pluto on the computer." }
                } else {
                    div { class: "group",
                        for (project, count, waiting, working) in rows {
                            button { key: "{project.id}", class: "prow", onclick: {
                                    let id = project.id.clone();
                                    move |_| ui.open_page(Page::Project(id.clone()))
                                },
                                Badge { project: project.clone() }
                                span { class: "grow",
                                    b { "{project.title}" }
                                    small { "{project.workspace_root} · {count} thread" if count != 1 { "s" } }
                                }
                                if waiting > 0 {
                                    span { class: "pill approval", "{waiting} waiting" }
                                } else if working > 0 {
                                    span { class: "pill working", Icon { name: "loader-circle", class: "spin" } "Working" }
                                }
                                Icon { name: "chevron-right", class: "chev" }
                            }
                        }
                    }
                    p { class: "note", "Projects are added in Pluto on the computer." }
                }
            }
        }
    }
}

pub fn project_choice_title(field: &str) -> &'static str {
    match field {
        "model" => "Model",
        "access" => "Permissions",
        _ => "Workspace",
    }
}

/// A project's `projectSettingsOverrides` entry.
fn overrides(settings: &Value, project: &str) -> Value {
    let value = &settings["projectSettingsOverrides"][project];
    if value.is_object() {
        value.clone()
    } else {
        json!({})
    }
}

/// Writes the project's override `key` (or removes it for Default).
fn write_override(ui: Ui, project: &str, key: &str, value: Option<Value>) {
    let mut entry = overrides(ui.ctx.session.peek().settings(), project);
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
    {
        let mut session = ui.ctx.session;
        let mut session = session.write();
        let settings = &mut session.state.config["settings"];
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
    }
    ui.ctx.request(
        "settings:projectSettingsOverrides",
        "server.updateSettings",
        json!({"patch": {"projectSettingsOverrides": {project: entry}}}),
    );
}

/// A project (`#project`, after `projects_page.rs`).
#[component]
fn ProjectPage(id: String) -> Element {
    let ui = use_context::<Ui>();
    let s = ui.ctx.session.read();
    let Some(project) = s.state.project(&id).cloned() else {
        return rsx! { section { class: "page", PageHead { title: "Project" } div { class: "empty-page", "This project was removed." } } };
    };
    let threads: Vec<_> = s
        .listed(false)
        .into_iter()
        .filter(|t| t.project_id == id)
        .collect();
    let now = chrono::Utc::now();
    let rows: Vec<(String, String, String)> = threads
        .iter()
        .map(|t| {
            let state = if t.has_pending_approvals {
                "Approval".to_owned()
            } else if t.has_pending_user_input {
                "Input".to_owned()
            } else if t.is_running() {
                "Working".to_owned()
            } else {
                crate::relative_time(crate::session::last_activity(t), now)
            };
            let branch = t
                .branch
                .clone()
                .map(|b| format!("{b} · "))
                .unwrap_or_default();
            (t.id.clone(), t.title.clone(), format!("{branch}{state}"))
        })
        .collect();
    let entry = overrides(s.settings(), &id);
    let providers = s.providers().to_vec();
    drop(s);
    let model = catalog::selection(&entry["defaultModelSelection"])
        .map(|m| catalog::label(&providers, &m).0)
        .unwrap_or_else(|| "Default".into());
    let access = entry["defaultRuntimeMode"]
        .as_str()
        .map(|m| catalog::access(m).1)
        .unwrap_or("Default");
    let workspace = entry["defaultThreadEnvMode"]
        .as_str()
        .map(|m| label_of(&WORKSPACES, m))
        .unwrap_or("Default");
    let (p1, p2, p3, p4) = (id.clone(), id.clone(), id.clone(), id.clone());
    rsx! {
        section { class: "page",
            PageHead { title: project.title.clone() }
            div { class: "pscroll",
                div { class: "phead",
                    Badge { project: project.clone() }
                    b { "{project.title}" }
                    small { "{project.workspace_root}" }
                }
                button { class: "btn primary big", style: "margin-top:0", onclick: move |_| {
                        {
                            let mut drafts = ui.drafts;
                            let mut drafts = drafts.write();
                            drafts.project = Some(p4.clone());
                            drafts.worktree = None;
                            drafts.base = None;
                        }
                        ui.ctx.session.clone().write().chat = false;
                        ui.reset();
                        ui.ctx.select(None);
                    },
                    Icon { name: "square-pen" }
                    "New thread"
                }
                div { class: "glabel", "Threads" }
                if rows.is_empty() {
                    p { class: "note", "No threads yet." }
                } else {
                    div { class: "group",
                        for (thread, title, line) in rows {
                            button { key: "{thread}", class: "prow", onclick: move |_| ui.open_page(Page::Thread(thread.clone())),
                                span { class: "grow", span { class: "title", "{title}" } small { "{line}" } }
                                Icon { name: "chevron-right", class: "chev" }
                            }
                        }
                    }
                }
                div { class: "glabel", "New threads" }
                div { class: "group",
                    SetChoice { icon: "bot", label: "Model", value: model, onclick: move |_| ui.open_sheet(Sheet::ProjectSetting(p1.clone(), "model")) }
                    SetChoice { icon: "shield", label: "Permissions", value: access.to_owned(), onclick: move |_| ui.open_sheet(Sheet::ProjectSetting(p2.clone(), "access")) }
                    SetChoice { icon: "folder", label: "Workspace", value: workspace.to_owned(), onclick: move |_| ui.open_sheet(Sheet::ProjectSetting(p3.clone(), "workspace")) }
                }
                p { class: "note", "Overrides the defaults from Settings for this project only." }
                div { class: "glabel", "Danger" }
                div { class: "group",
                    button { class: "set danger", onclick: move |_| ui.open_sheet(Sheet::RemoveProject(id.clone())),
                        Icon { name: "trash-2" }
                        span { class: "grow", "Remove project" small { "This removes only the project entries, not the files on disk." } }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ProjectSettingSheet(project: String, field: &'static str) -> Element {
    let ui = use_context::<Ui>();
    let entry = overrides(ui.ctx.session.read().settings(), &project);
    let default = move |key: &'static str, project: String, current: bool| {
        rsx! {
            Choice { label: "Default", detail: "Use the default from Settings", checked: current, onpick: move |_| {
                write_override(ui, &project, key, None);
                ui.back();
            } }
        }
    };
    match field {
        "model" => {
            let current = catalog::selection(&entry["defaultModelSelection"]);
            let p = project.clone();
            rsx! {
                {default("defaultModelSelection", project.clone(), current.is_none())}
                ModelList { chat: false, current: current.unwrap_or_default(), onpick: move |picked| {
                    write_override(ui, &p, "defaultModelSelection", Some(catalog::wire(&picked)));
                    ui.back();
                } }
            }
        }
        "access" => {
            let current = entry["defaultRuntimeMode"].as_str().map(str::to_owned);
            rsx! {
                {default("defaultRuntimeMode", project.clone(), current.is_none())}
                for (value, label, _, detail) in catalog::ACCESS {
                    Choice { key: "{value}", label, detail, checked: current.as_deref() == Some(value), onpick: {
                        let project = project.clone();
                        move |_| { write_override(ui, &project, "defaultRuntimeMode", Some(json!(value))); ui.back(); }
                    } }
                }
            }
        }
        _ => {
            let current = entry["defaultThreadEnvMode"].as_str().map(str::to_owned);
            rsx! {
                {default("defaultThreadEnvMode", project.clone(), current.is_none())}
                for (value, label, detail) in WORKSPACES {
                    Choice { key: "{value}", label, detail, checked: current.as_deref() == Some(value), onpick: {
                        let project = project.clone();
                        move |_| { write_override(ui, &project, "defaultThreadEnvMode", Some(json!(value))); ui.back(); }
                    } }
                }
            }
        }
    }
}

#[component]
pub fn RemoveProject(project: String) -> Element {
    let ui = use_context::<Ui>();
    // `removeMembers`'s confirmation (`projects_page.rs`).
    let (title, lines) = {
        let session = ui.ctx.session.read();
        let found = session.state.project(&project);
        let title = found.map(|p| p.title.clone()).unwrap_or_default();
        let root = found.map(|p| p.workspace_root.clone()).unwrap_or_default();
        let threads = session
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
        if let Some(label) = session.state.config["environment"]["label"].as_str() {
            lines.push(format!("Environment: {label}"));
        }
        lines.push(if threads > 0 {
            "This permanently clears conversation history for those threads and any archived threads.".into()
        } else {
            "This permanently clears any archived conversation history.".into()
        });
        lines.push("This removes only the project entries, not the files on disk.".into());
        lines.push("This action cannot be undone.".into());
        (title, lines)
    };
    let removed = title.clone();
    let (mut sent, busy, error) = crate::sheets::use_outcome(ui, move || {
        ui.ctx.toast(format!("{removed} removed"));
        ui.reset();
    });
    rsx! {
        for (index, line) in lines.into_iter().enumerate() {
            p { key: "{index}", class: "confirm", style: if index == 0 { "color:var(--text)" }, "{line}" }
        }
        if let Some(error) = error {
            p { class: "err", role: "alert", "{error}" }
        }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn danger", disabled: busy, onclick: move |_| {
                    sent.set(Some(ui.ctx.dispatch_tracked(json!({
                        "type": "project.delete",
                        "commandId": uuid::Uuid::new_v4().to_string(),
                        "projectId": project,
                        "force": true,
                    }))));
                },
                if busy { "Removing…" } else { "Remove" }
            }
        }
    }
}
