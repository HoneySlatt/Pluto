//! Settings → Scheduled Tasks (`ScheduledTasksSettings.tsx`,
//! `scheduledTasksSettings.logic.ts`): the environment's tasks, followed live
//! (`scheduledTasks.subscribe`), each with its schedule, next run and last
//! outcome, a switch that pauses it (`scheduledTasks.setEnabled`), and an
//! actions menu that runs it now (`scheduledTasks.runNow`) or deletes it
//! (`scheduledTasks.delete`). A project scope keeps its project's tasks.
//!
//! Not ported: "New task" and Edit (the editor dialog; agents create tasks
//! with `schedule_task`), and a webhook task's deliveries and endpoint.

use serde_json::{Value, json};

use crate::app::App;
use crate::ui::context_menu;
use crate::ui::settings::{Control, Row, RowAction, Section};

/// The page's index among the settings pages (after Pluto's own).
pub const SCHEDULED_TASKS_PAGE: usize = 12;

const WEEKDAY_LABELS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The page's state: the live list, the stream asked, an open actions menu.
#[derive(Default)]
pub(crate) struct ScheduledTasks {
    tasks: Option<Result<Vec<Value>, String>>,
    streaming: bool,
    menu: Option<(String, context_menu::State)>,
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
    pub(crate) fn scheduled_tasks_sections(&mut self) -> Vec<Section> {
        if !self.scheduled.streaming && self.connected() {
            self.scheduled.streaming = true;
            self.send(crate::protocol::Action::Stream {
                key: "scheduled-tasks".into(),
                method: "scheduledTasks.subscribe",
                payload: json!({}),
            });
        }
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
        let mut items = Vec::new();
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
}
