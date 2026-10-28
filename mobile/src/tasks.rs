//! Scheduled tasks (`#tasks`, after the desktop's `scheduled_tasks.rs`):
//! the live list (`scheduledTasks.subscribe`), one card per task with its
//! schedule line, prompt, last error, project and model, its pause switch
//! (`scheduledTasks.setEnabled`) and its Run now, Edit and Delete actions;
//! "+" and Edit open the editor, saved with `scheduledTasks.upsert` with the
//! desktop's checks and messages.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use pluto_core::model::ModelSelection;
use serde_json::{Value, json};

use crate::badge::Badge;
use crate::catalog;
use crate::icons::Icon;
use crate::nav::{Page, Sheet, Ui};
use crate::pages::PageHead;
use crate::sheets::{Choice, ModelList};

/// `WEEKDAY_LABELS`: 0 is Sunday.
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/// The day chips, Monday first.
const WEEK: [u64; 7] = [1, 2, 3, 4, 5, 6, 0];
const MAX_AGE_MINUTES: u64 = 24 * 60;

/// `scheduleLabel`.
pub fn schedule_label(schedule: &Value) -> String {
    match schedule["type"].as_str() {
        Some("webhook") => "On webhook".into(),
        Some("interval") => {
            let ms = schedule["everyMs"].as_f64().unwrap_or(0.0);
            let minutes = ms / 60_000.0;
            if minutes.fract() == 0.0 {
                format!("Every {minutes} min")
            } else {
                format!("Every {} sec", (ms / 1000.0).round())
            }
        }
        _ => {
            let weekdays: Vec<u64> = schedule["weekdays"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .collect();
            let days = if weekdays.is_empty() {
                "Daily".to_owned()
            } else if weekdays.len() == 5 && weekdays.iter().all(|d| (1..=5).contains(d)) {
                "Weekdays".to_owned()
            } else {
                weekdays
                    .iter()
                    .filter_map(|d| WEEKDAYS.get(*d as usize))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "{days} at {}",
                schedule["timeOfDay"].as_str().unwrap_or_default()
            )
        }
    }
}

/// `relativeLabel`: "in 5m" for an upcoming run.
pub fn relative_label(value: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = value.and_then(pluto_core::timeline::parse_time) else {
        return "Not scheduled".into();
    };
    let diff = (at - now).num_milliseconds();
    if diff <= 0 {
        let past = crate::relative_time(value.unwrap_or_default(), now);
        return if past.is_empty() {
            "Not scheduled".into()
        } else {
            format!("{past} ago")
        };
    }
    let minutes = (diff as f64 / 60_000.0).ceil() as i64;
    if minutes < 2 {
        return "in under a minute".into();
    }
    if minutes < 60 {
        return format!("in {minutes}m");
    }
    let hours = (minutes as f64 / 60.0).round() as i64;
    if hours < 24 {
        return format!("in {hours}h");
    }
    format!("in {}d", (hours as f64 / 24.0).round() as i64)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Fixed,
    Interval,
    Webhook,
}

/// The editor's draft (`Draft` in scheduled_tasks.rs).
#[derive(Clone, Debug, PartialEq)]
pub struct TaskDraft {
    pub editing: Option<String>,
    pub title: String,
    pub prompt: String,
    pub enabled: bool,
    pub mode: Mode,
    pub interval: String,
    pub time: String,
    pub weekdays: BTreeSet<u64>,
    pub project: String,
    pub thread: Option<String>,
    /// `worktree` or `root`; an existing checkout keeps its strategy.
    pub workspace: String,
    pub strategy: Value,
    pub model: Option<ModelSelection>,
    pub runtime: String,
    pub interaction: String,
    pub max_age: String,
    pub signature: Value,
    pub error: Option<String>,
}

impl TaskDraft {
    fn new(project: String, model: Option<ModelSelection>) -> Self {
        Self {
            editing: None,
            title: String::new(),
            prompt: String::new(),
            enabled: true,
            mode: Mode::Fixed,
            interval: "15".into(),
            time: "09:00".into(),
            weekdays: (1..=5).collect(),
            project,
            thread: None,
            workspace: "worktree".into(),
            strategy: json!({"type": "worktree", "baseRef": "main", "startFromOrigin": true}),
            model,
            runtime: "full-access".into(),
            interaction: "default".into(),
            max_age: String::new(),
            signature: Value::Null,
            error: None,
        }
    }

    /// `taskToDraft`.
    fn from_task(task: &Value) -> Self {
        let schedule = &task["schedule"];
        let weekdays: BTreeSet<u64> = schedule["weekdays"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect();
        let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        let strategy = task["workspaceStrategy"].clone();
        Self {
            editing: task["id"].as_str().map(str::to_owned),
            title: text(&task["title"]),
            prompt: text(&task["prompt"]),
            enabled: task["enabled"] == true,
            mode: match schedule["type"].as_str() {
                Some("interval") => Mode::Interval,
                Some("webhook") => Mode::Webhook,
                _ => Mode::Fixed,
            },
            interval: schedule["everyMs"]
                .as_f64()
                .map(|ms| format!("{}", (ms / 60_000.0).max(1.0)))
                .unwrap_or_else(|| "15".into()),
            time: schedule["timeOfDay"].as_str().unwrap_or("09:00").to_owned(),
            weekdays: if weekdays.is_empty() {
                (0..7).collect()
            } else {
                weekdays
            },
            project: text(&task["projectId"]),
            thread: task["threadId"].as_str().map(str::to_owned),
            workspace: strategy["type"].as_str().unwrap_or("worktree").to_owned(),
            strategy,
            model: catalog::selection(&task["modelSelection"]),
            runtime: task["runtimeMode"]
                .as_str()
                .unwrap_or("full-access")
                .to_owned(),
            interaction: task["interactionMode"]
                .as_str()
                .unwrap_or("default")
                .to_owned(),
            max_age: schedule["maxDeliveryAgeMinutes"]
                .as_u64()
                .map(|m| m.to_string())
                .unwrap_or_default(),
            signature: schedule["signature"].clone(),
            error: None,
        }
    }

    /// `scheduleFromDraft`.
    fn schedule(&self) -> Result<Value, String> {
        match self.mode {
            Mode::Webhook => {
                let max_age = match self.max_age.trim() {
                    "" => Value::Null,
                    typed => match typed.parse::<u64>() {
                        Ok(m) if (1..=MAX_AGE_MINUTES).contains(&m) => json!(m),
                        _ => {
                            return Err(format!(
                                "Enter whole minutes from 1 to {MAX_AGE_MINUTES}, or leave it blank."
                            ));
                        }
                    },
                };
                let mut signature = self.signature.clone();
                if let Some(map) = signature.as_object_mut() {
                    map.remove("secret");
                }
                Ok(
                    json!({"type": "webhook", "signature": if signature.is_object() { signature } else { Value::Null }, "maxDeliveryAgeMinutes": max_age}),
                )
            }
            Mode::Interval => {
                let minutes: f64 = self.interval.trim().parse().unwrap_or(0.0);
                let every = (minutes * 60_000.0).round();
                if every.is_nan() || every < 60_000.0 {
                    return Err("Enter an interval of at least one minute.".into());
                }
                Ok(json!({"type": "interval", "everyMs": every as u64}))
            }
            Mode::Fixed => {
                let time = if self.time.trim().is_empty() {
                    "09:00".to_owned()
                } else {
                    self.time.trim().to_owned()
                };
                let valid = time.len() == 5
                    && time.as_bytes()[2] == b':'
                    && time[..2].parse::<u8>().is_ok_and(|h| h < 24)
                    && time[3..].parse::<u8>().is_ok_and(|m| m < 60);
                if !valid {
                    return Err("Enter the time as HH:MM.".into());
                }
                if self.weekdays.is_empty() {
                    return Err("Pick at least one day.".into());
                }
                let mut schedule = json!({"type": "fixed_time", "timeOfDay": time});
                if self.weekdays.len() != 7 {
                    schedule["weekdays"] = json!(self.weekdays.iter().collect::<Vec<_>>());
                }
                Ok(schedule)
            }
        }
    }

    fn workspace_strategy(&self) -> Value {
        match self.workspace.as_str() {
            "root" => json!({"type": "root"}),
            "existing_worktree" => self.strategy.clone(),
            _ => {
                let base = self.strategy["baseRef"]
                    .as_str()
                    .filter(|b| !b.trim().is_empty())
                    .unwrap_or("main");
                let origin = if self.strategy["type"] == "worktree" {
                    self.strategy["startFromOrigin"] == true
                } else {
                    true
                };
                json!({"type": "worktree", "baseRef": base, "startFromOrigin": origin})
            }
        }
    }
}

fn tasks_list(ui: &Ui) -> Option<Result<Vec<Value>, String>> {
    ui.ctx.session.read().tasks.clone()
}

#[component]
pub fn Tasks() -> Element {
    let ui = use_context::<Ui>();
    use_hook(move || ui.ctx.watch_tasks());
    let tasks = tasks_list(&ui);
    let now = chrono::Utc::now();
    let new_task = move |_| {
        let session = ui.ctx.session.peek();
        let project = session.code_projects().first().map(|p| (*p).clone());
        let model = project
            .as_ref()
            .map(|p| crate::compose::project_default(&session, p))
            .as_ref()
            .and_then(catalog::selection)
            .or_else(|| catalog::selection(&session.settings()["defaultModelSelection"]))
            .or_else(|| {
                Some(catalog::first_model(session.providers(), false))
                    .filter(|m| !m.model.is_empty())
            });
        drop(session);
        ui.drafts.clone().write().task = Some(TaskDraft::new(
            project.map(|p| p.id).unwrap_or_default(),
            model,
        ));
        ui.open_page(Page::TaskEdit(None));
    };
    rsx! {
        section { class: "page",
            PageHead {
                title: "Scheduled tasks",
                action: rsx! {
                    button { class: "disc", "aria-label": "New task", onclick: new_task, Icon { name: "plus" } }
                },
            }
            div { class: "pscroll",
                match tasks {
                    None => rsx! { div { class: "loading-note", "Loading…" } },
                    Some(Err(error)) => rsx! { div { class: "empty-page", b { "Couldn’t load the tasks" } "{error}" } },
                    Some(Ok(tasks)) if tasks.is_empty() => rsx! {
                        div { class: "empty-page", b { "No scheduled tasks" } "Tap + to run a prompt on a schedule." }
                    },
                    Some(Ok(tasks)) => rsx! {
                        for task in tasks {
                            TaskCard { key: "{task[\"id\"]}", task, now: now.timestamp() }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn TaskCard(task: Value, now: i64) -> Element {
    let ui = use_context::<Ui>();
    let now = chrono::DateTime::from_timestamp(now, 0).unwrap_or_default();
    let id = task["id"].as_str().unwrap_or_default().to_owned();
    let enabled = task["enabled"] == true;
    let webhook = task["schedule"]["type"] == "webhook";
    let when = if !enabled {
        "Paused".to_owned()
    } else if webhook {
        "Listening".to_owned()
    } else if let Some(next) = task["nextRunAt"].as_str() {
        format!("Next run {}", relative_label(Some(next), now))
    } else {
        "Not scheduled".to_owned()
    };
    let last = task["lastRunStatus"]
        .as_str()
        .filter(|s| *s != "never")
        .map(str::to_owned);
    let error = task["lastRunError"].as_str().map(str::to_owned);
    let (project, model_label, mark) = {
        let session = ui.ctx.session.read();
        let project = task["projectId"]
            .as_str()
            .and_then(|p| session.state.project(p))
            .cloned();
        let (label, driver) = catalog::selection(&task["modelSelection"])
            .map(|m| catalog::label(session.providers(), &m))
            .unwrap_or_default();
        (project, label, catalog::mark(&driver))
    };
    let (i1, i2) = (id.clone(), id);
    rsx! {
        article { class: if enabled { "task" } else { "task off" },
            div { class: "top",
                span { class: "grow",
                    b { "{task[\"title\"].as_str().unwrap_or_default()}" }
                    span { class: "when",
                        "{schedule_label(&task[\"schedule\"])} · {when}"
                        if let Some(last) = last {
                            " · "
                            span { class: if last == "failed" { "bad" } else { "ok" }, "{last}" }
                        }
                    }
                }
                span { class: "toggle",
                    input {
                        r#type: "checkbox",
                        checked: enabled,
                        "aria-label": "Enabled",
                        onchange: move |e| ui.ctx.mutate("scheduledTasks.setEnabled", json!({"id": i1, "enabled": e.checked()})),
                    }
                    span {}
                }
            }
            p { class: "prompt", "{task[\"prompt\"].as_str().unwrap_or_default()}" }
            if let Some(error) = error.filter(|_| enabled) {
                p { class: "err", Icon { name: "circle-alert" } span { "{error}" } }
            }
            div { class: "foot",
                if let Some(project) = project {
                    Badge { project: project.clone() }
                    span { "{project.title}" }
                    span { class: "sep", "·" }
                }
                if let Some(mark) = mark {
                    img { src: mark, alt: "" }
                }
                span { "{model_label}" }
                button { class: "kebab", "aria-label": "Actions", onclick: move |_| ui.open_sheet(Sheet::TaskMenu(i2.clone())),
                    Icon { name: "ellipsis-vertical" }
                }
            }
        }
    }
}

#[component]
pub fn TaskMenu(id: String) -> Element {
    let ui = use_context::<Ui>();
    let task = tasks_list(&ui)
        .and_then(Result::ok)
        .and_then(|t| t.into_iter().find(|t| t["id"] == id.as_str()));
    let Some(task) = task else {
        return rsx! {};
    };
    let webhook = task["schedule"]["type"] == "webhook";
    let (i1, i2) = (id.clone(), id.clone());
    let title = task["title"].as_str().unwrap_or_default().to_owned();
    rsx! {
        if !webhook {
            button { class: "srow", onclick: move |_| {
                    ui.ctx.mutate("scheduledTasks.runNow", json!({"id": i1}));
                    ui.back();
                    ui.ctx.toast(format!("Running {title}"));
                },
                Icon { name: "play" } span { class: "grow", "Run now" }
            }
        }
        button { class: "srow", onclick: move |_| {
                ui.drafts.clone().write().task = Some(TaskDraft::from_task(&task));
                ui.open_page(Page::TaskEdit(Some(i2.clone())));
            },
            Icon { name: "pencil" } span { class: "grow", "Edit" }
        }
        button { class: "srow danger", onclick: move |_| {
                ui.ctx.mutate("scheduledTasks.delete", json!({"id": id}));
                ui.back();
            },
            Icon { name: "trash-2" } span { class: "grow", "Delete" }
        }
    }
}

pub fn field_title(field: &str) -> &'static str {
    match field {
        "project" => "Project",
        "workspace" => "Workspace",
        _ => "Model",
    }
}

/// A task editor field's choices.
#[component]
pub fn TaskField(field: &'static str) -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    let Some(draft) = drafts.read().task.clone() else {
        return rsx! {};
    };
    match field {
        "project" => {
            let projects: Vec<_> = ui
                .ctx
                .session
                .read()
                .code_projects()
                .into_iter()
                .cloned()
                .collect();
            rsx! {
                for project in projects {
                    button { key: "{project.id}", class: "srow", role: "menuitemradio", "aria-checked": "{draft.project == project.id}",
                        onclick: {
                            let id = project.id.clone();
                            move |_| {
                                if let Some(task) = drafts.write().task.as_mut() {
                                    task.project = id.clone();
                                }
                                ui.back();
                            }
                        },
                        Badge { project: project.clone() }
                        span { class: "grow", "{project.title}" }
                        Icon { name: "check", class: "check" }
                    }
                }
            }
        }
        "workspace" => rsx! {
            for (value, label) in [("worktree", "Create a new worktree"), ("root", "Use the project checkout")] {
                Choice { key: "{value}", label, checked: draft.workspace == value, onpick: move |_| {
                    if let Some(task) = drafts.write().task.as_mut() {
                        task.workspace = value.to_owned();
                    }
                    ui.back();
                } }
            }
        },
        _ => rsx! {
            ModelList { chat: false, current: draft.model.clone().unwrap_or_default(), onpick: move |picked| {
                if let Some(task) = drafts.write().task.as_mut() {
                    task.model = Some(picked);
                }
                ui.back();
            } }
        },
    }
}

#[component]
pub fn TaskEditor(id: Option<String>) -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    let Some(draft) = drafts.read().task.clone() else {
        return rsx! { section { class: "page", PageHead { title: "Task" } } };
    };
    let mut edit = move |change: &dyn Fn(&mut TaskDraft)| {
        if let Some(task) = drafts.write().task.as_mut() {
            change(task);
        }
    };
    let (project_name, model_label, mark) = {
        let session = ui.ctx.session.read();
        let project = session
            .state
            .project(&draft.project)
            .map(|p| p.title.clone())
            .unwrap_or_else(|| "Choose".into());
        let (label, driver) = draft
            .model
            .as_ref()
            .map(|m| catalog::label(session.providers(), m))
            .unwrap_or(("Choose".into(), String::new()));
        (project, label, catalog::mark(&driver))
    };
    let workspace = match draft.workspace.as_str() {
        "root" => "Project checkout",
        "existing_worktree" => "Existing checkout",
        _ => "New worktree",
    };
    let save =
        move |_| {
            let Some(mut draft) = drafts.peek().task.clone() else {
                return;
            };
            let session = ui.ctx.session.peek();
            let known = session.state.projects.iter().any(|p| p.id == draft.project);
            drop(session);
            let fail = |mut draft: TaskDraft, error: String| {
                draft.error = Some(error);
                ui.drafts.clone().write().task = Some(draft);
            };
            let Some(model) = draft.model.clone().filter(|_| {
                !draft.title.trim().is_empty() && !draft.prompt.trim().is_empty() && known
            }) else {
                return fail(draft, "Add a title, prompt, project, and model.".into());
            };
            let schedule = match draft.schedule() {
                Ok(schedule) => schedule,
                Err(error) => return fail(draft, error),
            };
            let mut input = json!({
                "title": draft.title.trim(),
                "prompt": draft.prompt.trim(),
                "enabled": draft.enabled,
                "schedule": schedule,
                "projectId": draft.project,
                "threadId": draft.thread,
                "workspaceStrategy": draft.workspace_strategy(),
                "modelSelection": catalog::wire(&model),
                "runtimeMode": draft.runtime,
                "interactionMode": draft.interaction,
                "creationSource": "web",
            });
            if let Some(id) = &draft.editing {
                input["id"] = json!(id);
                input["requireExisting"] = json!(true);
            }
            draft.error = None;
            ui.drafts.clone().write().task = Some(draft);
            ui.ctx
                .request("scheduled-upsert", "scheduledTasks.upsert", input);
        };
    // The upsert's reply: saved closes the editor, a failure shows.
    let reply = ui
        .ctx
        .session
        .read()
        .replies
        .get("scheduled-upsert")
        .cloned();
    use_effect(move || {
        let reply = ui
            .ctx
            .session
            .read()
            .replies
            .get("scheduled-upsert")
            .cloned();
        match reply {
            Some(Ok(_)) => {
                ui.ctx
                    .session
                    .clone()
                    .write()
                    .replies
                    .remove("scheduled-upsert");
                drafts.write().task = None;
                ui.back();
            }
            Some(Err(error)) => {
                ui.ctx
                    .session
                    .clone()
                    .write()
                    .replies
                    .remove("scheduled-upsert");
                if let Some(task) = drafts.write().task.as_mut() {
                    task.error = Some(format!("Could not save scheduled task: {error}"));
                }
            }
            None => {}
        }
    });
    let _ = reply;
    let editing = id.is_some();
    rsx! {
        section { class: "page",
            PageHead { title: (if editing { "Edit task" } else { "New task" }).to_owned() }
            div { class: "pscroll",
                div { class: "form",
                    label { class: "field", span { "Name" }
                        div { class: "inbox",
                            input { initial_value: "{draft.title}", placeholder: "Morning review", oninput: move |e| edit(&|t| t.title = e.value()) }
                        }
                    }
                    label { class: "field", span { "Prompt" }
                        div { class: "inbox area",
                            textarea { rows: "4", placeholder: "What the agent should do on each run", initial_value: "{draft.prompt}", oninput: move |e| edit(&|t| t.prompt = e.value()) }
                        }
                    }
                    div { class: "group", style: "margin:6px 0 4px",
                        button { class: "set", onclick: move |_| ui.open_sheet(Sheet::TaskField("project")),
                            Icon { name: "folder-closed" } span { class: "grow", "Project" } span { class: "val", "{project_name}" } Icon { name: "chevron-right", class: "chev" }
                        }
                        button { class: "set", onclick: move |_| ui.open_sheet(Sheet::TaskField("workspace")),
                            Icon { name: "folder-git-2" } span { class: "grow", "Workspace" } span { class: "val", "{workspace}" } Icon { name: "chevron-right", class: "chev" }
                        }
                        button { class: "set", onclick: move |_| ui.open_sheet(Sheet::TaskField("model")),
                            Icon { name: "bot" } span { class: "grow", "Model" }
                            span { class: "val",
                                if let Some(mark) = mark { img { src: mark, alt: "" } }
                                "{model_label}"
                            }
                            Icon { name: "chevron-right", class: "chev" }
                        }
                    }
                    div { class: "glabel", "Schedule" }
                    div { class: "seg", role: "radiogroup", style: "margin:0 0 12px",
                        for (label, mode) in [("At a time", Mode::Fixed), ("Every interval", Mode::Interval), ("On webhook", Mode::Webhook)] {
                            button { r#type: "button", role: "radio", "aria-checked": "{draft.mode == mode}", onclick: move |_| edit(&|t| t.mode = mode), "{label}" }
                        }
                    }
                    match draft.mode {
                        Mode::Fixed => rsx! {
                            div { class: "inline", "Run at "
                                label { class: "inbox", style: "width:150px",
                                    input { r#type: "time", initial_value: "{draft.time}", "aria-label": "Run at", oninput: move |e| edit(&|t| t.time = e.value()) }
                                }
                            }
                            div { class: "days", role: "group", "aria-label": "Days",
                                for day in WEEK {
                                    button {
                                        r#type: "button",
                                        "aria-pressed": "{draft.weekdays.contains(&day)}",
                                        "aria-label": "{WEEKDAYS[day as usize]}",
                                        onclick: move |_| edit(&|t| { if !t.weekdays.remove(&day) { t.weekdays.insert(day); } }),
                                        "{&WEEKDAYS[day as usize][..1]}"
                                    }
                                }
                            }
                            div { class: "quick",
                                button { r#type: "button", onclick: move |_| edit(&|t| t.weekdays = (1..=5).collect()), "Weekdays" }
                                button { r#type: "button", onclick: move |_| edit(&|t| t.weekdays = (0..7).collect()), "Daily" }
                            }
                        },
                        Mode::Interval => rsx! {
                            div { class: "inline", "Run every "
                                label { class: "inbox",
                                    input { r#type: "number", min: "1", inputmode: "numeric", initial_value: "{draft.interval}", "aria-label": "Minutes", oninput: move |e| edit(&|t| t.interval = e.value()) }
                                }
                                " min"
                            }
                        },
                        Mode::Webhook => rsx! {
                            div { class: "inline", "Skip requests older than "
                                label { class: "inbox",
                                    input { r#type: "number", min: "1", inputmode: "numeric", initial_value: "{draft.max_age}", "aria-label": "Minutes", oninput: move |e| edit(&|t| t.max_age = e.value()) }
                                }
                                " min"
                            }
                            p { class: "note", style: "margin-top:-4px", "The webhook URL appears in Pluto on the computer once the task is saved." }
                        },
                    }
                    label { class: "remember", style: "margin-top:8px",
                        span { class: "grow", "Enabled" small { "Disabled tasks stay saved but do not run." } }
                        span { class: "toggle",
                            input { r#type: "checkbox", checked: draft.enabled, onchange: move |e| edit(&|t| t.enabled = e.checked()) }
                            span {}
                        }
                    }
                    div { class: "formerr", role: "alert", "{draft.error.clone().unwrap_or_default()}" }
                    button { class: "btn primary big", style: "margin-top:4px", onclick: save,
                        if editing { "Save task" } else { "Create task" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_read_like_the_desktop() {
        assert_eq!(
            schedule_label(
                &json!({"type": "fixed_time", "timeOfDay": "09:00", "weekdays": [1, 2, 3, 4, 5]})
            ),
            "Weekdays at 09:00"
        );
        assert_eq!(
            schedule_label(&json!({"type": "interval", "everyMs": 3_600_000})),
            "Every 60 min"
        );
        assert_eq!(schedule_label(&json!({"type": "webhook"})), "On webhook");
        let mut draft = TaskDraft::new("p".into(), None);
        draft.weekdays.clear();
        assert_eq!(draft.schedule().unwrap_err(), "Pick at least one day.");
        draft.mode = Mode::Interval;
        draft.interval = "0".into();
        assert_eq!(
            draft.schedule().unwrap_err(),
            "Enter an interval of at least one minute."
        );
    }
}
