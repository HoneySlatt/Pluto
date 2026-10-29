//! Scheduled tasks (`#tasks`, after the desktop's `scheduled_tasks.rs`):
//! the live list (`scheduledTasks.subscribe`), one card per task with its
//! schedule line, prompt, last error, project and model, its pause switch
//! (`scheduledTasks.setEnabled`) and its Run now, Edit and Delete actions;
//! "+" and Edit open the editor, saved with `scheduledTasks.upsert` with the
//! desktop's checks and messages.
//!
//! The editor's draft outlives the page and its field sheets; the app has
//! one session, so it is kept here for the process.

use std::collections::BTreeSet;
use std::sync::Mutex;

use pluto_core::model::ModelSelection;
use serde_json::{Value, json};

use crate::badge::{Badge, badge};
use crate::projects::{DefaultModelGroup, model_groups};
use crate::{App, Core, catalog};

/// `WEEKDAY_LABELS`: 0 is Sunday.
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MAX_AGE_MINUTES: u64 = 24 * 60;
const UPSERT: &str = "scheduled-upsert";

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
        let past = crate::times::relative_time(value.unwrap_or_default(), now);
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

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum TaskMode {
    Fixed,
    Interval,
    Webhook,
}

/// The editor's draft (`Draft` in scheduled_tasks.rs).
#[derive(Clone, Debug, PartialEq)]
struct TaskDraft {
    editing: Option<String>,
    title: String,
    prompt: String,
    enabled: bool,
    mode: TaskMode,
    interval: String,
    time: String,
    weekdays: BTreeSet<u64>,
    project: String,
    thread: Option<String>,
    /// `worktree` or `root`; an existing checkout keeps its strategy.
    workspace: String,
    strategy: Value,
    model: Option<ModelSelection>,
    runtime: String,
    interaction: String,
    max_age: String,
    signature: Value,
    error: Option<String>,
}

impl TaskDraft {
    fn new(project: String, model: Option<ModelSelection>) -> Self {
        Self {
            editing: None,
            title: String::new(),
            prompt: String::new(),
            enabled: true,
            mode: TaskMode::Fixed,
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
                Some("interval") => TaskMode::Interval,
                Some("webhook") => TaskMode::Webhook,
                _ => TaskMode::Fixed,
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
            TaskMode::Webhook => {
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
                Ok(json!({
                    "type": "webhook",
                    "signature": if signature.is_object() { signature } else { Value::Null },
                    "maxDeliveryAgeMinutes": max_age,
                }))
            }
            TaskMode::Interval => {
                let minutes: f64 = self.interval.trim().parse().unwrap_or(0.0);
                let every = (minutes * 60_000.0).round();
                if every.is_nan() || every < 60_000.0 {
                    return Err("Enter an interval of at least one minute.".into());
                }
                Ok(json!({"type": "interval", "everyMs": every as u64}))
            }
            TaskMode::Fixed => {
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

    /// The `scheduledTasks.upsert` input, or why it cannot be saved.
    fn input(&self, known_project: bool) -> Result<Value, String> {
        let Some(model) = self.model.clone().filter(|_| {
            !self.title.trim().is_empty() && !self.prompt.trim().is_empty() && known_project
        }) else {
            return Err("Add a title, prompt, project, and model.".into());
        };
        let schedule = self.schedule()?;
        let mut input = json!({
            "title": self.title.trim(),
            "prompt": self.prompt.trim(),
            "enabled": self.enabled,
            "schedule": schedule,
            "projectId": self.project,
            "threadId": self.thread,
            "workspaceStrategy": self.workspace_strategy(),
            "modelSelection": catalog::wire(&model),
            "runtimeMode": self.runtime,
            "interactionMode": self.interaction,
            "creationSource": "web",
        });
        if let Some(id) = &self.editing {
            input["id"] = json!(id);
            input["requireExisting"] = json!(true);
        }
        Ok(input)
    }
}

static DRAFT: Mutex<Option<TaskDraft>> = Mutex::new(None);

/// Leaving the server drops the draft.
pub fn reset() {
    *DRAFT.lock().unwrap_or_else(|p| p.into_inner()) = None;
}

fn with_draft<R>(change: impl FnOnce(&mut Option<TaskDraft>) -> R) -> R {
    change(&mut DRAFT.lock().unwrap_or_else(|p| p.into_inner()))
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TaskCard {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub enabled: bool,
    /// The schedule and its state: "Weekdays at 09:00 · Next run in 5m".
    pub when: String,
    /// The last run's status (`succeeded`, `failed`, …).
    pub last: Option<String>,
    /// The last run's error, while the task is enabled.
    pub error: Option<String>,
    pub project: Option<Badge>,
    pub model_label: String,
    pub mark: Option<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum TasksState {
    Loading,
    Failed { error: String },
    Listed { tasks: Vec<TaskCard> },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TaskMenuView {
    pub title: String,
    pub webhook: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct TaskDraftView {
    pub editing: bool,
    pub title: String,
    pub prompt: String,
    pub enabled: bool,
    pub mode: TaskMode,
    pub interval: String,
    pub time: String,
    /// The picked days, 0 for Sunday.
    pub weekdays: Vec<u32>,
    pub max_age: String,
    pub project_id: String,
    pub project: String,
    pub workspace: String,
    /// `worktree`, `root` or `existing_worktree`.
    pub workspace_value: String,
    pub model: String,
    pub mark: Option<String>,
    pub error: Option<String>,
    /// The upsert waits for the server.
    pub saving: bool,
}

fn card(app: &App, task: &Value, now: chrono::DateTime<chrono::Utc>) -> TaskCard {
    let s = &app.session;
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
    let (model_label, driver) = catalog::selection(&task["modelSelection"])
        .map(|m| catalog::label(s.providers(), &m))
        .unwrap_or_default();
    let text = |key: &str| task[key].as_str().unwrap_or_default().to_owned();
    TaskCard {
        id: text("id"),
        title: text("title"),
        prompt: text("prompt"),
        enabled,
        when: format!("{} · {when}", schedule_label(&task["schedule"])),
        last: task["lastRunStatus"]
            .as_str()
            .filter(|s| *s != "never")
            .map(str::to_owned),
        error: task["lastRunError"]
            .as_str()
            .filter(|_| enabled)
            .map(str::to_owned),
        project: task["projectId"]
            .as_str()
            .and_then(|p| s.state.project(p))
            .map(badge),
        model_label,
        mark: catalog::mark(&driver).map(str::to_owned),
    }
}

fn find_task(app: &App, id: &str) -> Option<Value> {
    app.session
        .tasks
        .as_ref()?
        .as_ref()
        .ok()?
        .iter()
        .find(|t| t["id"] == id)
        .cloned()
}

fn draft_view(app: &App, draft: &TaskDraft) -> TaskDraftView {
    let s = &app.session;
    let (model, driver) = draft
        .model
        .as_ref()
        .map(|m| catalog::label(s.providers(), m))
        .unwrap_or(("Choose".into(), String::new()));
    TaskDraftView {
        editing: draft.editing.is_some(),
        title: draft.title.clone(),
        prompt: draft.prompt.clone(),
        enabled: draft.enabled,
        mode: draft.mode,
        interval: draft.interval.clone(),
        time: draft.time.clone(),
        weekdays: draft.weekdays.iter().map(|d| *d as u32).collect(),
        max_age: draft.max_age.clone(),
        project_id: draft.project.clone(),
        project: s
            .state
            .project(&draft.project)
            .map(|p| p.title.clone())
            .unwrap_or_else(|| "Choose".into()),
        workspace: match draft.workspace.as_str() {
            "root" => "Project checkout",
            "existing_worktree" => "Existing checkout",
            _ => "New worktree",
        }
        .into(),
        workspace_value: draft.workspace.clone(),
        model,
        mark: catalog::mark(&driver).map(str::to_owned),
        error: draft.error.clone(),
        saving: s.waiting(UPSERT),
    }
}

impl Core {
    /// Changes the draft, then tells the interface.
    fn edit_draft(&self, change: impl FnOnce(&mut TaskDraft)) {
        self.update(|_, _| {
            with_draft(|draft| {
                if let Some(draft) = draft.as_mut() {
                    change(draft);
                }
            })
        });
    }
}

#[uniffi::export]
impl Core {
    /// The scheduled tasks (followed from the first showing).
    pub fn tasks(&self) -> TasksState {
        self.read(|app| {
            let now = chrono::Utc::now();
            match &app.session.tasks {
                None => TasksState::Loading,
                Some(Err(error)) => TasksState::Failed {
                    error: error.clone(),
                },
                Some(Ok(tasks)) => TasksState::Listed {
                    tasks: tasks.iter().map(|t| card(app, t, now)).collect(),
                },
            }
        })
    }

    /// Pauses or resumes a task.
    pub fn set_task_enabled(&self, id: String, enabled: bool) {
        self.update(|app, _| {
            app.session.mutate(
                "scheduledTasks.setEnabled",
                json!({"id": id, "enabled": enabled}),
            )
        });
    }

    pub fn task_menu(&self, id: String) -> Option<TaskMenuView> {
        self.read(|app| {
            let task = find_task(app, &id)?;
            Some(TaskMenuView {
                title: task["title"].as_str().unwrap_or_default().to_owned(),
                webhook: task["schedule"]["type"] == "webhook",
            })
        })
    }

    /// Run now (`run`) or Delete; the key to follow (`reply_state`).
    pub fn task_action(&self, id: String, run: bool) -> String {
        let method = if run {
            "scheduledTasks.runNow"
        } else {
            "scheduledTasks.delete"
        };
        let key = format!("task-action:{}", uuid::Uuid::new_v4());
        self.update(|app, _| app.session.request(key.clone(), method, json!({"id": id})));
        key
    }

    /// A new task's draft: the first project, its default model.
    pub fn task_new(&self) {
        let draft = self.read(|app| {
            let s = &app.session;
            let project = s.code_projects().first().map(|p| (*p).clone());
            let model = project
                .as_ref()
                .map(|p| crate::compose::project_default(s, p))
                .as_ref()
                .and_then(catalog::selection)
                .or_else(|| catalog::selection(&s.settings()["defaultModelSelection"]))
                .or_else(|| {
                    Some(catalog::first_model(s.providers(), false)).filter(|m| !m.model.is_empty())
                });
            TaskDraft::new(project.map(|p| p.id).unwrap_or_default(), model)
        });
        self.update(|app, _| {
            app.session.forget(UPSERT);
            with_draft(|d| *d = Some(draft));
        });
    }

    /// Edit: the task's draft; false once the task is gone.
    pub fn task_edit(&self, id: String) -> bool {
        let Some(task) = self.read(|app| find_task(app, &id)) else {
            return false;
        };
        self.update(|app, _| {
            app.session.forget(UPSERT);
            with_draft(|d| *d = Some(TaskDraft::from_task(&task)));
        });
        true
    }

    pub fn task_draft(&self) -> Option<TaskDraftView> {
        self.read(|app| with_draft(|d| d.as_ref().map(|d| draft_view(app, d))))
    }

    /// A typed field: `title`, `prompt`, `interval`, `time` or `max_age`.
    pub fn task_set_text(&self, field: String, value: String) {
        self.edit_draft(|d| match field.as_str() {
            "title" => d.title = value,
            "prompt" => d.prompt = value,
            "interval" => d.interval = value,
            "time" => d.time = value,
            "max_age" => d.max_age = value,
            _ => {}
        });
    }

    pub fn task_set_mode(&self, mode: TaskMode) {
        self.edit_draft(|d| d.mode = mode);
    }

    pub fn task_toggle_day(&self, day: u32) {
        self.edit_draft(|d| {
            let day = u64::from(day);
            if !d.weekdays.remove(&day) {
                d.weekdays.insert(day);
            }
        });
    }

    pub fn task_set_days(&self, days: Vec<u32>) {
        self.edit_draft(|d| d.weekdays = days.into_iter().map(u64::from).collect());
    }

    pub fn task_set_enabled(&self, enabled: bool) {
        self.edit_draft(|d| d.enabled = enabled);
    }

    pub fn task_set_project(&self, project: String) {
        self.edit_draft(|d| d.project = project);
    }

    /// `worktree` or `root`.
    pub fn task_set_workspace(&self, workspace: String) {
        self.edit_draft(|d| d.workspace = workspace);
    }

    pub fn task_set_model(&self, instance_id: String, model: String) {
        self.edit_draft(|d| {
            d.model = Some(ModelSelection {
                instance_id,
                model,
                options: None,
            })
        });
    }

    /// The task field sheet's models, the draft's checked.
    pub fn task_models(&self) -> Vec<DefaultModelGroup> {
        self.read(|app| {
            let current = with_draft(|d| d.as_ref().and_then(|d| d.model.clone()));
            model_groups(&app.session, current.as_ref())
        })
    }

    /// Saves the draft (`scheduledTasks.upsert`), or shows why it cannot.
    pub fn task_save(&self) {
        self.update(|app, _| {
            let known = |project: &str| app.session.state.projects.iter().any(|p| p.id == project);
            let input = with_draft(|d| {
                let draft = d.as_mut()?;
                match draft.input(known(&draft.project)) {
                    Ok(input) => {
                        draft.error = None;
                        Some(input)
                    }
                    Err(error) => {
                        draft.error = Some(error);
                        None
                    }
                }
            });
            if let Some(input) = input {
                app.session.request(UPSERT, "scheduledTasks.upsert", input);
            }
        });
    }

    /// The upsert's reply: true once saved (the editor closes), a failure
    /// shows on the draft. Quiet while nothing came back.
    pub fn task_saved(&self) -> bool {
        let reply = self.read(|app| app.session.replies.get(UPSERT).cloned());
        let Some(reply) = reply else {
            return false;
        };
        self.update(|app, _| {
            app.session.forget(UPSERT);
            with_draft(|d| match reply {
                Ok(_) => {
                    *d = None;
                    true
                }
                Err(error) => {
                    if let Some(draft) = d.as_mut() {
                        draft.error = Some(format!("Could not save scheduled task: {error}"));
                    }
                    false
                }
            })
        })
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
        draft.mode = TaskMode::Interval;
        draft.interval = "0".into();
        assert_eq!(
            draft.schedule().unwrap_err(),
            "Enter an interval of at least one minute."
        );
    }

    #[test]
    fn a_draft_needs_its_fields_and_keeps_its_id() {
        let model = ModelSelection {
            instance_id: "codex".into(),
            model: "gpt".into(),
            options: None,
        };
        let mut draft = TaskDraft::new("p".into(), Some(model));
        assert_eq!(
            draft.input(true).unwrap_err(),
            "Add a title, prompt, project, and model."
        );
        draft.title = "Review".into();
        draft.prompt = "Check the PRs".into();
        assert!(draft.input(false).is_err());
        let input = draft.input(true).unwrap();
        assert_eq!(input["schedule"]["weekdays"], json!([1, 2, 3, 4, 5]));
        assert_eq!(input["workspaceStrategy"]["baseRef"], "main");
        let edited = TaskDraft::from_task(&json!({"id": "t", "title": "x", "prompt": "y",
            "schedule": {"type": "interval", "everyMs": 120000}, "projectId": "p",
            "modelSelection": {"instanceId": "codex", "model": "gpt"}}));
        assert_eq!(
            (edited.mode, edited.interval.as_str()),
            (TaskMode::Interval, "2")
        );
        assert_eq!(edited.input(true).unwrap()["requireExisting"], true);
    }
}
