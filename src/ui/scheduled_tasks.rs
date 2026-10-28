//! Settings → Scheduled Tasks (`ScheduledTasksSettings.tsx`,
//! `scheduledTasksSettings.logic.ts`): the environment's tasks, followed live
//! (`scheduledTasks.subscribe`), each with its schedule, next run and last
//! outcome, a switch that pauses it (`scheduledTasks.setEnabled`), and an
//! actions menu that runs it now (`scheduledTasks.runNow`) or deletes it
//! (`scheduledTasks.delete`). A project scope keeps its project's tasks.
//!
//! "New task" and Edit open the editor (`ScheduledTaskEditorDialog`): name,
//! project, workspace (a new worktree off a base branch, the project
//! checkout, or a given checkout), prompt, model, the schedule (at a time on
//! chosen days, every interval, or on webhook with an age limit) and
//! whether it runs, saved with `scheduledTasks.upsert`.
//!
//! Not ported: a webhook task's deliveries, endpoint and signature fields
//! (an edit keeps its signature), the base branch picker (the branch is
//! typed), and the environment selector (this client has one).

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::ModelSelection;
use crate::theme::{self, Weight, font};
use crate::ui::action_editor::{self, Kind};
use crate::ui::context_menu;
use crate::ui::settings::{Control, Row, RowAction, Section};
use crate::ui::widgets;

/// The page's index among the settings pages (after Pluto's own).
pub const SCHEDULED_TASKS_PAGE: usize = 12;

const WEEKDAY_LABELS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The page's state: the live list, the stream asked, an open actions menu.
#[derive(Default)]
pub(crate) struct ScheduledTasks {
    tasks: Option<Result<Vec<Value>, String>>,
    streaming: bool,
    menu: Option<(String, context_menu::State)>,
    pub(crate) editor: Option<Draft>,
}

/// `ScheduleMode`.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Fixed,
    Interval,
    Webhook,
}

/// `DraftState`.
pub(crate) struct Draft {
    editing: Option<String>,
    title: String,
    prompt: String,
    enabled: bool,
    mode: Mode,
    interval_minutes: String,
    time_of_day: String,
    weekdays: BTreeSet<u64>,
    project_id: String,
    thread_id: Option<String>,
    /// `root`, `worktree` or `existing_worktree`.
    workspace: &'static str,
    base_ref: String,
    start_from_origin: bool,
    existing_path: String,
    model: Option<ModelSelection>,
    runtime_mode: String,
    interaction_mode: String,
    max_age: String,
    /// A webhook task's signature, kept as stored (its secret stays on the server).
    signature: Value,
    error: Option<String>,
    saving: bool,
}

/// `DEFAULT_WEBHOOK_PROMPT`.
const DEFAULT_WEBHOOK_PROMPT: &str = "Handle this webhook:\n{{body}}";
/// `MAX_WEBHOOK_DELIVERY_AGE_MINUTES`.
const MAX_AGE_MINUTES: u64 = 24 * 60;
const WEEKDAY_ORDER: [u64; 7] = [1, 2, 3, 4, 5, 6, 0];
const WEEKDAY_SHORT: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
const DIALOG_WIDTH: f32 = 576.0;

impl Draft {
    fn new(project_id: String, model: Option<ModelSelection>) -> Self {
        Self {
            editing: None,
            title: String::new(),
            prompt: String::new(),
            enabled: true,
            mode: Mode::Fixed,
            interval_minutes: "15".into(),
            time_of_day: "09:00".into(),
            weekdays: [1, 2, 3, 4, 5].into_iter().collect(),
            project_id,
            thread_id: None,
            workspace: "worktree",
            base_ref: "main".into(),
            start_from_origin: true,
            existing_path: String::new(),
            model,
            runtime_mode: "full-access".into(),
            interaction_mode: "default".into(),
            max_age: String::new(),
            signature: Value::Null,
            error: None,
            saving: false,
        }
    }

    /// `taskToDraft`.
    fn from_task(task: &Value) -> Self {
        let schedule = &task["schedule"];
        let strategy = &task["workspaceStrategy"];
        let weekdays: BTreeSet<u64> = schedule["weekdays"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect();
        let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
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
            interval_minutes: schedule["everyMs"]
                .as_f64()
                .map(|ms| format!("{}", (ms / 60_000.0).max(1.0)))
                .unwrap_or_else(|| "15".into()),
            time_of_day: schedule["timeOfDay"].as_str().unwrap_or("09:00").to_owned(),
            weekdays: if weekdays.is_empty() {
                (0..7).collect()
            } else {
                weekdays
            },
            project_id: text(&task["projectId"]),
            thread_id: task["threadId"].as_str().map(str::to_owned),
            workspace: match strategy["type"].as_str() {
                Some("root") => "root",
                Some("existing_worktree") => "existing_worktree",
                _ => "worktree",
            },
            base_ref: strategy["baseRef"].as_str().unwrap_or("main").to_owned(),
            start_from_origin: if strategy["type"] == "worktree" {
                strategy["startFromOrigin"] == true
            } else {
                true
            },
            existing_path: text(&strategy["worktreePath"]),
            model: serde_json::from_value(task["modelSelection"].clone()).ok(),
            runtime_mode: task["runtimeMode"]
                .as_str()
                .unwrap_or("full-access")
                .to_owned(),
            interaction_mode: task["interactionMode"]
                .as_str()
                .unwrap_or("default")
                .to_owned(),
            max_age: schedule["maxDeliveryAgeMinutes"]
                .as_u64()
                .map(|m| m.to_string())
                .unwrap_or_default(),
            signature: schedule["signature"].clone(),
            error: None,
            saving: false,
        }
    }

    /// `scheduleFromDraft`; `Err` names what is wrong.
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
                let signature = if self.signature.is_object() {
                    let mut kept = self.signature.clone();
                    if let Some(map) = kept.as_object_mut() {
                        map.remove("secret");
                    }
                    kept
                } else {
                    Value::Null
                };
                Ok(
                    json!({"type": "webhook", "signature": signature, "maxDeliveryAgeMinutes": max_age}),
                )
            }
            Mode::Interval => {
                let minutes: f64 = self.interval_minutes.trim().parse().unwrap_or(0.0);
                let every = (minutes * 60_000.0).round();
                if every.is_nan() || every < 60_000.0 {
                    return Err("Enter an interval of at least one minute.".into());
                }
                Ok(json!({"type": "interval", "everyMs": every as u64}))
            }
            Mode::Fixed => {
                let time = if self.time_of_day.trim().is_empty() {
                    "09:00".to_owned()
                } else {
                    self.time_of_day.trim().to_owned()
                };
                let valid = time.len() == 5
                    && time.as_bytes()[2] == b':'
                    && time[..2].parse::<u8>().is_ok_and(|h| h < 24)
                    && time[3..].parse::<u8>().is_ok_and(|m| m < 60);
                if !valid {
                    return Err("Enter the time as HH:MM.".into());
                }
                let every_day = self.weekdays.is_empty() || self.weekdays.len() == 7;
                let mut schedule = json!({"type": "fixed_time", "timeOfDay": time});
                if !every_day {
                    schedule["weekdays"] = json!(self.weekdays.iter().collect::<Vec<_>>());
                }
                Ok(schedule)
            }
        }
    }
}

/// `scheduleLabel`.
pub(crate) fn schedule_label(schedule: &Value) -> String {
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
                    .filter_map(|d| WEEKDAY_LABELS.get(*d as usize))
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

/// `relativeLabel`: "in 5m" for an upcoming run, the past as "5m ago".
pub(crate) fn relative_label(value: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = value.and_then(crate::timeline::parse_time) else {
        return "Not scheduled".into();
    };
    let diff = (at - now).num_milliseconds();
    if diff <= 0 {
        let past = crate::ui::widgets::relative_time(value.unwrap_or_default(), now);
        return if past.is_empty() {
            "Not scheduled".into()
        } else {
            past
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

impl App {
    /// The page's rows, following the list once connected.
    /// Follows the environment's tasks (`scheduledTasksLive`) once connected.
    pub(crate) fn watch_scheduled_tasks(&mut self) {
        if !self.scheduled.streaming && self.connected() {
            self.scheduled.streaming = true;
            self.send(crate::protocol::Action::Stream {
                key: "scheduled-tasks".into(),
                method: "scheduledTasks.subscribe",
                payload: json!({}),
            });
        }
    }

    /// The live list, once it arrived.
    pub(crate) fn scheduled_task_list(&self) -> Option<&Result<Vec<Value>, String>> {
        self.scheduled.tasks.as_ref()
    }

    /// `scheduledTasks.setEnabled` or `scheduledTasks.runNow` for `task`.
    pub(crate) fn scheduled_task_action(&mut self, task: &str, method: &'static str, extra: Value) {
        let mut input = json!({"id": task});
        if let (Some(input), Some(extra)) = (input.as_object_mut(), extra.as_object()) {
            input.extend(extra.clone());
        }
        self.send_request(format!("scheduled-mutation:{task}"), method, input);
    }

    pub(crate) fn scheduled_tasks_sections(&mut self) -> Vec<Section> {
        self.watch_scheduled_tasks();
        let row =
            |id: String, title: String, description: String, status: Option<String>, control| Row {
                id,
                title,
                description,
                status,
                layers: false,
                reset: false,
                control,
            };
        let note = |title: &str, description: &str| {
            row(
                "scheduled-note".into(),
                title.into(),
                description.into(),
                None,
                Control::None,
            )
        };
        let rows = if !self.connected() {
            vec![note(
                "Environment disconnected",
                "Reconnect this environment to view its scheduled tasks.",
            )]
        } else {
            match &self.scheduled.tasks {
                None => vec![note("Loading scheduled tasks…", "")],
                Some(Err(error)) => vec![note("Could not load scheduled tasks", error)],
                Some(Ok(tasks)) => {
                    // `matchesScheduledTaskScope`.
                    let project = self.settings_project().map(|p| p.id);
                    let tasks: Vec<&Value> = tasks
                        .iter()
                        .filter(|t| {
                            project
                                .as_ref()
                                .is_none_or(|id| t["projectId"] == id.as_str())
                        })
                        .collect();
                    if tasks.is_empty() {
                        vec![note(
                            "No scheduled tasks",
                            "No tasks match this environment and project selection.",
                        )]
                    } else {
                        tasks
                            .into_iter()
                            .map(|task| {
                                let enabled = task["enabled"] == true;
                                let webhook = task["schedule"]["type"] == "webhook";
                                let when = if !enabled {
                                    "Paused".to_owned()
                                } else if webhook {
                                    "Listening".to_owned()
                                } else if let Some(next) = task["nextRunAt"].as_str() {
                                    format!("Next run {}", relative_label(Some(next), self.now))
                                } else {
                                    "Not scheduled".to_owned()
                                };
                                let mut status =
                                    format!("{} · {when}", schedule_label(&task["schedule"]));
                                if let Some(last) =
                                    task["lastRunStatus"].as_str().filter(|s| *s != "never")
                                {
                                    status.push_str(&format!(" · {last}"));
                                }
                                if let Some(error) = task["lastRunError"].as_str() {
                                    status.push_str(&format!(" · {error}"));
                                }
                                // `line-clamp-2`.
                                let prompt: String = task["prompt"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .lines()
                                    .take(2)
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                row(
                                    format!(
                                        "scheduled:{}",
                                        task["id"].as_str().unwrap_or_default()
                                    ),
                                    task["title"].as_str().unwrap_or_default().to_owned(),
                                    prompt,
                                    Some(status),
                                    Control::SwitchMenu(enabled),
                                )
                            })
                            .collect()
                    }
                }
            }
        };
        vec![Section {
            title: "Scheduled tasks".into(),
            project: None,
            unavailable: None,
            rows,
        }]
    }

    /// A row's switch or actions menu.
    pub(crate) fn apply_scheduled(&mut self, id: &str, action: RowAction) {
        let Some(task) = id.strip_prefix("scheduled:") else {
            return;
        };
        match action {
            RowAction::Toggle(on) => self.send_request(
                format!("scheduled-mutation:{task}"),
                "scheduledTasks.setEnabled",
                json!({"id": task, "enabled": on}),
            ),
            RowAction::Menu(at) => {
                self.scheduled.menu = Some((task.to_owned(), context_menu::State::new(at)));
            }
            _ => {}
        }
    }

    /// The actions menu: Run now (not for webhook tasks), Delete.
    pub(crate) fn scheduled_context_menu(&mut self, ui: &mut eframe::egui::Ui) {
        let Some((task, state)) = self.scheduled.menu.take() else {
            return;
        };
        let webhook = self
            .scheduled
            .tasks
            .as_ref()
            .and_then(|t| t.as_ref().ok())
            .and_then(|tasks| tasks.iter().find(|t| t["id"] == task.as_str()))
            .is_some_and(|t| t["schedule"]["type"] == "webhook");
        let mut items = vec![context_menu::Item::new("edit", "Edit")];
        if !webhook {
            items.push(context_menu::Item::new("run", "Run now"));
        }
        items.push(context_menu::Item {
            destructive: true,
            ..context_menu::Item::new("delete", "Delete")
        });
        let mut open = Some(state);
        let picked = context_menu::show(ui, &mut open, &items);
        match picked.as_deref() {
            Some("edit") => self.edit_scheduled_task(&task),
            Some("run") => self.send_request(
                format!("scheduled-mutation:{task}"),
                "scheduledTasks.runNow",
                json!({"id": task}),
            ),
            Some("delete") => self.send_request(
                format!("scheduled-mutation:{task}"),
                "scheduledTasks.delete",
                json!({"id": task}),
            ),
            _ => {}
        }
        if let Some(state) = open {
            self.scheduled.menu = Some((task, state));
        }
    }

    /// Opens the editor on a new task, in the scoped (or first) project.
    pub(crate) fn new_scheduled_task(&mut self) {
        let project = self
            .settings_project()
            .or_else(|| self.state.projects.first().cloned());
        let model = project
            .as_ref()
            .map(|p| self.project_default_model(p))
            .or_else(|| Some(self.state.config["settings"]["defaultModelSelection"].clone()))
            .and_then(|v| serde_json::from_value::<ModelSelection>(v).ok())
            .filter(|m| !m.model.is_empty());
        self.scheduled.editor = Some(Draft::new(project.map(|p| p.id).unwrap_or_default(), model));
    }

    pub(crate) fn edit_scheduled_task(&mut self, id: &str) {
        let task = self
            .scheduled
            .tasks
            .as_ref()
            .and_then(|t| t.as_ref().ok())
            .and_then(|tasks| tasks.iter().find(|t| t["id"] == id))
            .cloned();
        if let Some(task) = task {
            self.scheduled.editor = Some(Draft::from_task(&task));
        }
    }

    /// `submit`: checks the draft, then `scheduledTasks.upsert`.
    fn save_scheduled_task(&mut self, draft: &mut Draft) {
        if draft.saving {
            return;
        }
        let project_known = self.state.projects.iter().any(|p| p.id == draft.project_id);
        let Some(model) = draft.model.clone().filter(|_| {
            !draft.title.trim().is_empty() && !draft.prompt.trim().is_empty() && project_known
        }) else {
            draft.error = Some("Add a title, prompt, project, and model.".into());
            return;
        };
        let schedule = match draft.schedule() {
            Ok(schedule) => schedule,
            Err(error) => {
                draft.error = Some(error);
                return;
            }
        };
        if draft.workspace == "existing_worktree" && draft.existing_path.trim().is_empty() {
            draft.error = Some("Enter the path of the checkout to run in.".into());
            return;
        }
        let workspace = match draft.workspace {
            "root" => json!({"type": "root"}),
            "existing_worktree" => {
                json!({"type": "existing_worktree", "worktreePath": draft.existing_path.trim()})
            }
            _ => json!({"type": "worktree",
                "baseRef": if draft.base_ref.trim().is_empty() { "main" } else { draft.base_ref.trim() },
                "startFromOrigin": draft.start_from_origin}),
        };
        let mut input = json!({
            "title": draft.title.trim(),
            "prompt": draft.prompt.trim(),
            "enabled": draft.enabled,
            "schedule": schedule,
            "projectId": draft.project_id,
            "threadId": draft.thread_id,
            "workspaceStrategy": workspace,
            "modelSelection": model,
            "runtimeMode": draft.runtime_mode,
            "interactionMode": draft.interaction_mode,
            "creationSource": "web",
        });
        if let Some(id) = &draft.editing {
            input["id"] = json!(id);
            input["requireExisting"] = json!(true);
        }
        draft.error = None;
        draft.saving = true;
        self.send_request("scheduled-upsert".into(), "scheduledTasks.upsert", input);
    }

    /// `ScheduledTaskEditorDialog`.
    pub(crate) fn scheduled_task_editor(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut draft) = self.scheduled.editor.take() else {
            return;
        };
        let muted = theme::muted_foreground();
        let label = |ui: &Ui, pos: Pos2, text: &str| {
            widgets::text(
                ui,
                pos,
                text,
                font(14.0, Weight::Medium),
                theme::foreground(),
                16.0,
            )
        };
        // Field heights, top to bottom.
        let workspace_extra = if draft.workspace == "root" {
            0.0
        } else {
            22.0 + 32.0 + 20.0
        };
        let origin_row = if draft.workspace == "worktree" {
            32.0 + 12.0
        } else {
            0.0
        };
        let schedule_extra = match draft.mode {
            Mode::Webhook => 16.0 * 3.0 + 12.0 + 22.0 + 32.0,
            _ => 32.0,
        };
        let error: f32 = if draft.error.is_some() {
            20.0 + 12.0
        } else {
            0.0
        };
        let body = 22.0 + 32.0 + 20.0 // name
            + 22.0 + 32.0 + 20.0 // project and workspace
            + workspace_extra + origin_row
            + 22.0 + 96.0 + 20.0 // prompt
            + 22.0 + 32.0 + 20.0 // model
            + 32.0 + 12.0 + schedule_extra + 20.0 // schedule
            + 40.0 // enabled
            + error;
        let description = "Run a prompt automatically — on an interval, at a fixed time, or when a webhook is called.";
        let description_height = widgets::galley(
            ui,
            description,
            font(14.0, Weight::Regular),
            muted,
            20.0,
            DIALOG_WIDTH - 50.0,
        )
        .size()
        .y;
        let header = 25.0 + 24.0 + 8.0 + description_height + 20.0;
        let height: f32 = (header + body + 8.0 + 20.0 + 32.0 + 20.0).min(screen.height() - 32.0);
        let d = Rect::from_center_size(screen.center(), Vec2::new(DIALOG_WIDTH, height));
        let mut close = false;
        let mut save = false;
        let mut open_menu = self.open_menu.take();
        let mut pick_model: Option<Rect> = None;
        let projects: Vec<(String, String)> = self
            .state
            .projects
            .iter()
            .filter(|p| self.settings_project().is_none_or(|s| s.id == p.id))
            .map(|p| (p.id.clone(), p.title.clone()))
            .collect();
        let model_label = draft.model.as_ref().map(|m| self.model_label(m));
        egui::Area::new(ui.id().with("scheduled-task-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(backdrop, 0.0, theme::alpha(theme::background(), 200.0 / 255.0));
                if response.clicked() && !ui.rect_contains_pointer(d) && open_menu.is_none() && !draft.saving {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let left = d.left() + 25.0;
                let right = d.right() - 25.0;
                let width = right - left;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    if draft.editing.is_some() { "Edit task" } else { "New task" },
                    font(18.0, Weight::SemiBold),
                    theme::foreground(),
                    24.0,
                );
                let galley = widgets::galley(
                    ui,
                    description,
                    font(14.0, Weight::Regular),
                    muted,
                    20.0,
                    width,
                );
                ui.painter().galley(Pos2::new(left, d.top() + 57.0), galley, muted);
                let content = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.top() + header),
                    Pos2::new(d.right() - 1.0, d.bottom() - 72.0),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(content));
                child.set_clip_rect(content);
                egui::ScrollArea::vertical()
                    .id_salt("scheduled-task-editor-body")
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        let mut y = ui.max_rect().top();
                        // Name.
                        label(ui, Pos2::new(left, y), "Name");
                        y += 22.0;
                        action_editor::input(
                            ui,
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)),
                            "scheduled-task-title",
                            &mut draft.title,
                            "e.g. Check for Sentry issues",
                            false,
                        );
                        y += 32.0 + 20.0;
                        // Project and workspace, side by side.
                        let half = (width - 12.0) / 2.0;
                        label(ui, Pos2::new(left, y), "Project");
                        label(ui, Pos2::new(left + half + 12.0, y), "Workspace");
                        y += 22.0;
                        let project_rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(half, 32.0));
                        let project_title = projects
                            .iter()
                            .find(|(id, _)| *id == draft.project_id)
                            .map_or("Select a project", |(_, t)| t.as_str());
                        if select_trigger(ui, project_rect, "scheduled-project", project_title) {
                            open_menu = Some("scheduled-project".into());
                        }
                        let entries: Vec<widgets::MenuEntry> = projects
                            .iter()
                            .map(|(id, title)| {
                                widgets::MenuEntry::Item(widgets::MenuItem {
                                    selected: *id == draft.project_id,
                                    ..widgets::MenuItem::new(title)
                                })
                            })
                            .collect();
                        if let Some(index) = widgets::menu(ui, &mut open_menu, "scheduled-project", project_rect, false, &entries) {
                            draft.project_id = projects[index].0.clone();
                        }
                        let workspace_rect =
                            Rect::from_min_size(Pos2::new(left + half + 12.0, y), Vec2::new(half, 32.0));
                        let modes = [
                            ("worktree", "Create a new worktree"),
                            ("root", "Use the project checkout"),
                            ("existing_worktree", "Use a specific checkout"),
                        ];
                        let current = modes
                            .iter()
                            .find(|(m, _)| *m == draft.workspace)
                            .map_or("", |(_, l)| *l);
                        if select_trigger(ui, workspace_rect, "scheduled-workspace", current) {
                            open_menu = Some("scheduled-workspace".into());
                        }
                        let entries: Vec<widgets::MenuEntry> = modes
                            .iter()
                            .map(|(m, l)| {
                                widgets::MenuEntry::Item(widgets::MenuItem {
                                    selected: *m == draft.workspace,
                                    ..widgets::MenuItem::new(l)
                                })
                            })
                            .collect();
                        if let Some(index) = widgets::menu(ui, &mut open_menu, "scheduled-workspace", workspace_rect, false, &entries) {
                            draft.workspace = modes[index].0;
                        }
                        y += 32.0 + 20.0;
                        match draft.workspace {
                            "worktree" => {
                                label(ui, Pos2::new(left, y), "Base branch");
                                y += 22.0;
                                action_editor::input(
                                    ui,
                                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)),
                                    "scheduled-task-base-ref",
                                    &mut draft.base_ref,
                                    "main",
                                    false,
                                );
                                y += 32.0 + 12.0;
                                widgets::text_in(
                                    ui,
                                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)),
                                    "Start from origin",
                                    font(14.0, Weight::Regular),
                                    theme::foreground(),
                                );
                                if crate::ui::settings::switch(ui, right, y + 16.0, draft.start_from_origin, "scheduled-origin") {
                                    draft.start_from_origin = !draft.start_from_origin;
                                }
                                y += 32.0 + 20.0;
                            }
                            "existing_worktree" => {
                                label(ui, Pos2::new(left, y), "Checkout path");
                                y += 22.0;
                                action_editor::input(
                                    ui,
                                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)),
                                    "scheduled-task-checkout",
                                    &mut draft.existing_path,
                                    "/path/to/checkout",
                                    false,
                                );
                                y += 32.0 + 20.0;
                            }
                            _ => {}
                        }
                        // Prompt.
                        label(ui, Pos2::new(left, y), "Prompt");
                        y += 22.0;
                        action_editor::input(
                            ui,
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 96.0)),
                            "scheduled-task-prompt",
                            &mut draft.prompt,
                            "What should the agent do each time this runs?",
                            true,
                        );
                        y += 96.0 + 20.0;
                        // Model.
                        label(ui, Pos2::new(left, y), "Model");
                        y += 22.0;
                        let model_rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0));
                        let (text, icon) = model_label
                            .clone()
                            .unwrap_or_else(|| ("Select a model".to_owned(), None));
                        if select_trigger(ui, model_rect, "scheduled-model-trigger", &text) {
                            open_menu = Some("scheduled-model".into());
                        }
                        if let Some(icon) = icon {
                            theme::paint_icon(
                                ui,
                                icon,
                                Rect::from_center_size(Pos2::new(model_rect.right() - 40.0, model_rect.center().y), Vec2::splat(14.0)),
                                14.0,
                                muted,
                            );
                        }
                        pick_model = Some(model_rect);
                        y += 32.0 + 20.0;
                        // Schedule.
                        label(ui, Pos2::new(left, y + 8.0), "Schedule");
                        let tabs = [(Mode::Fixed, "At a time"), (Mode::Interval, "Every interval"), (Mode::Webhook, "On webhook")];
                        let mut x = right;
                        for (mode, text) in tabs.iter().rev() {
                            let w = widgets::text_width(ui, text, font(13.0, Weight::Medium)) + 20.0;
                            let tab = Rect::from_min_size(Pos2::new(x - w, y), Vec2::new(w, 32.0));
                            x -= w + 4.0;
                            if toggle(ui, tab, ("scheduled-mode", *text), text, draft.mode == *mode) {
                                draft.mode = *mode;
                                if *mode == Mode::Webhook && draft.prompt.trim().is_empty() {
                                    draft.prompt = DEFAULT_WEBHOOK_PROMPT.into();
                                }
                            }
                        }
                        y += 32.0 + 12.0;
                        match draft.mode {
                            Mode::Fixed => {
                                let w = widgets::text_in(ui, Rect::from_min_size(Pos2::new(left, y), Vec2::new(80.0, 32.0)), "Run at", font(14.0, Weight::Medium), theme::foreground());
                                let time = Rect::from_min_size(Pos2::new(left + w + 8.0, y), Vec2::new(96.0, 32.0));
                                action_editor::input(ui, time, "scheduled-task-time", &mut draft.time_of_day, "09:00", false);
                                let on = widgets::text_in(ui, Rect::from_min_size(Pos2::new(time.right() + 8.0, y), Vec2::new(30.0, 32.0)), "on", font(12.0, Weight::Regular), muted);
                                let mut x = time.right() + 8.0 + on + 12.0;
                                for day in WEEKDAY_ORDER {
                                    let chip = Rect::from_min_size(Pos2::new(x, y), Vec2::new(32.0, 32.0));
                                    x += 34.0;
                                    let label = WEEKDAY_SHORT[day as usize];
                                    if toggle(ui, chip, ("scheduled-day", day), label, draft.weekdays.contains(&day))
                                        && !(draft.weekdays.len() == 1 && draft.weekdays.contains(&day))
                                        && !draft.weekdays.remove(&day)
                                    {
                                        draft.weekdays.insert(day);
                                    }
                                }
                                y += 32.0;
                            }
                            Mode::Interval => {
                                let w = widgets::text_in(ui, Rect::from_min_size(Pos2::new(left, y), Vec2::new(90.0, 32.0)), "Run every", font(14.0, Weight::Medium), theme::foreground());
                                let field = Rect::from_min_size(Pos2::new(left + w + 8.0, y), Vec2::new(96.0, 32.0));
                                action_editor::input(ui, field, "scheduled-task-interval", &mut draft.interval_minutes, "15", false);
                                widgets::text_in(ui, Rect::from_min_size(Pos2::new(field.right() + 8.0, y), Vec2::new(80.0, 32.0)), "minutes", font(12.0, Weight::Regular), muted);
                                y += 32.0;
                            }
                            Mode::Webhook => {
                                let note = widgets::galley(
                                    ui,
                                    "Each request runs the prompt. Use {{body.path}}, {{headers.name}}, {{query.name}}, {{body}} or {{request}} in the prompt; only what it names reaches the agent.",
                                    font(12.0, Weight::Regular),
                                    muted,
                                    16.0,
                                    width,
                                );
                                ui.painter().galley(Pos2::new(left, y), note, muted);
                                y += 16.0 * 3.0 + 12.0;
                                label(ui, Pos2::new(left, y), "Skip requests older than");
                                widgets::text(ui, Pos2::new(right - 120.0, y + 2.0), "minutes, optional", font(11.0, Weight::Regular), theme::alpha(muted, 0.8), 14.0);
                                y += 22.0;
                                action_editor::input(ui, Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 32.0)), "scheduled-task-max-age", &mut draft.max_age, "Run every request", false);
                                y += 32.0;
                            }
                        }
                        y += 20.0;
                        // Enabled.
                        label(ui, Pos2::new(left, y), "Enabled");
                        widgets::text(ui, Pos2::new(left, y + 20.0), "Disabled tasks stay saved but do not run.", font(14.0, Weight::Regular), muted, 20.0);
                        if crate::ui::settings::switch(ui, right, y + 18.0, draft.enabled, "scheduled-enabled") {
                            draft.enabled = !draft.enabled;
                        }
                        y += 40.0;
                        if let Some(error) = &draft.error {
                            y += 12.0;
                            widgets::text(ui, Pos2::new(left, y), error, font(14.0, Weight::Regular), Color32::from_rgb(251, 65, 74), 20.0);
                            y += 20.0;
                        }
                        ui.allocate_rect(Rect::from_min_max(ui.max_rect().min, Pos2::new(content.right(), y + 4.0)), Sense::hover());
                    });
                // Footer.
                let top = d.bottom() - 20.0 - 32.0;
                let primary = if draft.saving {
                    "Saving…"
                } else if draft.editing.is_some() {
                    "Save task"
                } else {
                    "Create task"
                };
                let save_rect = action_editor::footer_rect(ui, right, top, primary);
                if action_editor::footer_button(ui, save_rect, primary, Kind::Primary) && self.connected() {
                    save = true;
                }
                let cancel_rect = action_editor::footer_rect(ui, save_rect.left() - 8.0, top, "Cancel");
                if action_editor::footer_button(ui, cancel_rect, "Cancel", Kind::Outline) && !draft.saving {
                    close = true;
                }
            });
        self.open_menu = open_menu;
        // The model picker, below its trigger.
        if let Some(anchor) = pick_model {
            let selection = draft.model.clone().unwrap_or_default();
            if let Some(chosen) =
                self.model_picker_popup(ui, anchor, "scheduled-model", selection, true, false, None)
            {
                draft.model = Some(chosen);
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape))
            && self.open_menu.is_none()
            && !draft.saving
        {
            close = true;
        }
        if save {
            self.save_scheduled_task(&mut draft);
        }
        if !close {
            self.scheduled.editor = Some(draft);
        }
    }

    /// The live list and mutation failures; true when `key` was one.
    pub(crate) fn scheduled_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if key == "scheduled-tasks" {
            self.scheduled.tasks = Some(match result {
                Ok(value) => Ok(value["tasks"].as_array().cloned().unwrap_or_default()),
                Err(error) => {
                    self.scheduled.streaming = false;
                    Err(error.clone())
                }
            });
            return true;
        }
        if key == "scheduled-upsert" {
            if let Some(draft) = &mut self.scheduled.editor {
                draft.saving = false;
                match result {
                    Ok(_) => self.scheduled.editor = None,
                    Err(error) => {
                        draft.error = Some(format!("Could not save scheduled task: {error}"))
                    }
                }
            }
            return true;
        }
        if key.starts_with("scheduled-mutation:") {
            if let Err(error) = result {
                self.toasts.add(
                    crate::ui::toasts::Toast::new(
                        crate::ui::toasts::Kind::Error,
                        "Could not update scheduled task",
                    )
                    .description(error.clone()),
                );
            }
            return true;
        }
        false
    }
}

/// A `SelectTrigger size="sm"`: the value, then the chevron.
fn select_trigger(ui: &Ui, rect: Rect, id: &str, text: &str) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        if response.hovered() {
            theme::overlay(0.06)
        } else {
            theme::control_surface()
        },
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.left() + 12.0, rect.top()),
            Pos2::new(rect.right() - 30.0, rect.bottom()),
        ),
        text,
        font(14.0, Weight::Regular),
        theme::foreground(),
    );
    theme::paint_icon(
        ui,
        "chevron-down",
        Rect::from_center_size(
            Pos2::new(rect.right() - 16.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        16.0,
        theme::muted_foreground(),
    );
    response.clicked()
}

/// A `Toggle` (`variant="outline" size="sm"`), pressed when `on`.
fn toggle(ui: &Ui, rect: Rect, id: impl egui::AsIdSalt, text: &str, on: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let fill = if on {
        theme::accent()
    } else if response.hovered() {
        theme::overlay(0.06)
    } else {
        theme::control_surface()
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        fill,
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let label_font = font(13.0, Weight::Medium);
    let width = widgets::text_width(ui, text, label_font.clone());
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - width / 2.0, rect.top()),
            rect.max,
        ),
        text,
        label_font,
        if on {
            theme::foreground()
        } else {
            theme::muted_foreground()
        },
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_read_like_upstream() {
        assert_eq!(
            schedule_label(&json!({"type": "interval", "everyMs": 900_000})),
            "Every 15 min"
        );
        assert_eq!(
            schedule_label(
                &json!({"type": "fixed_time", "timeOfDay": "09:00", "weekdays": [1, 2, 3, 4, 5]})
            ),
            "Weekdays at 09:00"
        );
        assert_eq!(
            schedule_label(
                &json!({"type": "fixed_time", "timeOfDay": "18:30", "weekdays": [0, 6]})
            ),
            "Sun, Sat at 18:30"
        );
        assert_eq!(schedule_label(&json!({"type": "webhook"})), "On webhook");
        let now = chrono::Utc::now();
        let later = |m: i64| (now + chrono::Duration::minutes(m)).to_rfc3339();
        assert_eq!(relative_label(Some(&later(30)), now), "in 30m");
        assert_eq!(relative_label(Some(&later(180)), now), "in 3h");
        assert_eq!(relative_label(None, now), "Not scheduled");
    }

    #[test]
    fn drafts_become_upstream_schedules() {
        let mut draft = Draft::new("p".into(), None);
        assert_eq!(
            draft.schedule().unwrap(),
            json!({"type": "fixed_time", "timeOfDay": "09:00", "weekdays": [1, 2, 3, 4, 5]})
        );
        draft.weekdays = (0..7).collect();
        assert_eq!(
            draft.schedule().unwrap(),
            json!({"type": "fixed_time", "timeOfDay": "09:00"})
        );
        draft.mode = Mode::Interval;
        draft.interval_minutes = "0.5".into();
        assert!(draft.schedule().is_err());
        draft.interval_minutes = "30".into();
        assert_eq!(
            draft.schedule().unwrap(),
            json!({"type": "interval", "everyMs": 1_800_000})
        );
        draft.mode = Mode::Webhook;
        draft.max_age = "abc".into();
        assert!(draft.schedule().is_err());
        draft.max_age = String::new();
        assert_eq!(
            draft.schedule().unwrap(),
            json!({"type": "webhook", "signature": null, "maxDeliveryAgeMinutes": null})
        );
        // An edit keeps its task's fields.
        let task = json!({"id": "t", "title": "Daily", "prompt": "Go", "enabled": false,
            "projectId": "p", "threadId": null,
            "schedule": {"type": "interval", "everyMs": 3_600_000},
            "workspaceStrategy": {"type": "root"},
            "modelSelection": {"instanceId": "codex", "model": "gpt"},
            "runtimeMode": "approval-required", "interactionMode": "plan"});
        let draft = Draft::from_task(&task);
        assert_eq!(draft.interval_minutes, "60");
        assert_eq!(draft.workspace, "root");
        assert_eq!(draft.runtime_mode, "approval-required");
    }
}
