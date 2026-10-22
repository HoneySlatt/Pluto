//! The running turn's task list (`chat/ComposerTasksBadge.tsx`, with
//! `deriveActivePlanState` from `session-logic.ts` and
//! `activeComposerTasksProgress` from `ChatView.tsx`): while a turn runs and
//! its provider keeps a plan (`turn.plan.updated`, Codex's `update_plan` or
//! Claude's todo list), a "Tasks" tab above the composer shows the current
//! step, the count and the step segments; a click opens the drawer listing
//! every step with its status and how long it took.
use eframe::egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::{Activity, Thread};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const SUCCESS: Color32 = Color32::from_rgb(0, 188, 125); // --success

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Status {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Step {
    pub step: String,
    pub status: Status,
    pub duration_ms: Option<i64>,
}

/// `ActivePlanState`: the plan a turn last reported.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActivePlan {
    pub turn_id: Option<String>,
    pub steps: Vec<Step>,
}

/// `planStateFromActivity`: the activity's steps, or None when it carries
/// no plan (which clears the timings before it).
fn plan_steps(activity: &Activity) -> Option<Vec<Step>> {
    let steps: Vec<Step> = activity.payload["plan"]
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let step = entry["step"].as_str()?.to_owned();
            let status = match entry["status"].as_str() {
                Some("completed") => Status::Completed,
                Some("inProgress") => Status::InProgress,
                _ => Status::Pending,
            };
            Some(Step {
                step,
                status,
                duration_ms: None,
            })
        })
        .collect();
    (!steps.is_empty()).then_some(steps)
}

/// Steps keyed by their text and occurrence (`keyedSteps`).
fn step_keys(steps: &[Step]) -> Vec<String> {
    let mut seen: std::collections::HashMap<&str, usize> = Default::default();
    steps
        .iter()
        .map(|step| {
            let count = seen.entry(step.step.as_str()).or_default();
            let key = format!("{}:{count}", step.step);
            *count += 1;
            key
        })
        .collect()
}

fn millis(iso: &str) -> Option<i64> {
    crate::timeline::parse_time(iso).map(|t| t.timestamp_millis())
}

/// `deriveActivePlanState` with `addPlanStepDurations`.
pub(crate) fn active_plan(
    activities: &[Activity],
    latest_turn: Option<&str>,
) -> Option<ActivePlan> {
    let mut plans: Vec<&Activity> = activities
        .iter()
        .filter(|a| a.kind == "turn.plan.updated")
        .collect();
    // `compareActivitiesByOrder`: by sequence when both have one, else by time.
    plans.sort_by(|a, b| match (a.sequence, b.sequence) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a
            .created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id)),
    });
    let latest = latest_turn
        .and_then(|turn| {
            plans
                .iter()
                .rev()
                .find(|a| a.turn_id.as_deref() == Some(turn))
        })
        .or(plans.last())?;
    let mut steps = plan_steps(latest)?;
    let same_turn: Vec<&&Activity> = plans
        .iter()
        .filter(|a| a.turn_id == latest.turn_id)
        .collect();
    let clear = same_turn
        .iter()
        .rposition(|a| plan_steps(a).is_none())
        .map_or(0, |i| i + 1);
    // When each step started (first seen running) and completed.
    let mut timings: Vec<(String, Option<i64>, Option<i64>)> = Vec::new();
    let mut plan_started = None;
    for activity in &same_turn[clear..] {
        let (Some(snapshot), Some(at)) = (plan_steps(activity), millis(&activity.created_at))
        else {
            continue;
        };
        plan_started.get_or_insert(at);
        for (key, step) in step_keys(&snapshot).into_iter().zip(&snapshot) {
            let index = match timings.iter().position(|(k, ..)| *k == key) {
                Some(index) => index,
                None => {
                    timings.push((key, None, None));
                    timings.len() - 1
                }
            };
            let timing = &mut timings[index];
            if step.status == Status::InProgress && timing.1.is_none() {
                timing.1 = Some(at);
            }
            if step.status == Status::Completed && timing.2.is_none() {
                timing.2 = Some(at);
            }
        }
    }
    timings.sort_by_key(|(_, _, completed)| completed.unwrap_or(i64::MAX));
    let mut durations: std::collections::HashMap<String, i64> = Default::default();
    let mut previous = plan_started;
    for (key, started, completed) in timings {
        let Some(completed) = completed else {
            continue;
        };
        if let Some(started) = started.or(previous)
            && completed > started
        {
            durations.insert(key, completed - started);
        }
        previous = Some(completed);
    }
    for (key, step) in step_keys(&steps).into_iter().zip(steps.iter_mut()) {
        if step.status == Status::Completed {
            step.duration_ms = durations.get(&key).copied();
        }
    }
    Some(ActivePlan {
        turn_id: latest.turn_id.clone(),
        steps,
    })
}

/// `activeComposerTasksProgress`: the running turn's own plan, while a
/// step is left.
pub(crate) fn active_tasks(thread: &Thread) -> Option<ActivePlan> {
    let latest = thread.latest_turn.as_ref()?;
    let settled = latest.started_at.is_some()
        && latest.completed_at.is_some()
        && thread
            .session
            .as_ref()
            .is_none_or(|s| s.status != "running");
    if settled {
        return None;
    }
    let plan = active_plan(&thread.activities, Some(&latest.turn_id))?;
    let left = plan.steps.iter().any(|s| s.status != Status::Completed);
    (plan.turn_id.as_deref() == Some(latest.turn_id.as_str()) && left).then_some(plan)
}

/// The current step: the running one, else the first pending.
fn current_step(plan: &ActivePlan) -> &str {
    plan.steps
        .iter()
        .find(|s| s.status == Status::InProgress)
        .or_else(|| plan.steps.iter().find(|s| s.status == Status::Pending))
        .map_or("", |s| s.step.as_str())
}

/// A drawer row's height (`py-1`, wrapped text-xs/4) at `width`.
fn step_height(ui: &Ui, step: &Step, width: f32) -> f32 {
    let text_width = width - 8.0 - 24.0 - 4.0 - 48.0 - 8.0;
    let galley = widgets::galley(
        ui,
        &step.step,
        font(12.0, Weight::Regular),
        Color32::WHITE,
        16.0,
        text_width.max(40.0),
    );
    galley.size().y.max(16.0) + 8.0
}

/// The drawer's list height, capped like `ComposerBanner.Scroll`.
fn list_height(ui: &Ui, plan: &ActivePlan, width: f32) -> f32 {
    let rows: f32 = plan
        .steps
        .iter()
        .map(|s| step_height(ui, s, width))
        .sum::<f32>()
        + plan.steps.len().saturating_sub(1) as f32;
    rows.min((ui.ctx().content_rect().height() * 0.4).min(384.0))
}

impl App {
    /// The open thread's running task list.
    pub(crate) fn composer_tasks(&self) -> Option<ActivePlan> {
        active_tasks(&*self.current_thread()?)
    }

    /// The tab's (or open drawer's) height, its overlap included.
    pub(crate) fn tasks_banner_height(&self, ui: &Ui, plan: &ActivePlan, width: f32) -> f32 {
        use crate::ui::requests_panel::OVERLAP;
        if self.tasks_open {
            4.0 + 24.0 + list_height(ui, plan, width - 44.0) + 4.0 + OVERLAP
        } else {
            5.0 + 24.0 + 5.0 + OVERLAP
        }
    }

    /// The tab or the drawer, attached above the composer `surface`.
    pub(crate) fn tasks_banner(&mut self, ui: &mut Ui, surface: Rect, plan: &ActivePlan) {
        use crate::ui::requests_panel::{OVERLAP, plan_banner_surface};
        let height = self.tasks_banner_height(ui, plan, surface.width());
        let rect = Rect::from_min_max(
            Pos2::new(surface.left() + 22.0, surface.top() + OVERLAP - height),
            Pos2::new(surface.right() - 22.0, surface.top() + OVERLAP),
        );
        plan_banner_surface(ui, rect);
        let pad = if self.tasks_open { 4.0 } else { 5.0 };
        let row = Rect::from_min_size(
            Pos2::new(rect.left() + 4.0, rect.top() + pad),
            Vec2::new(rect.width() - 8.0, 24.0),
        );
        let response = ui
            .interact(row, ui.id().with("composer-tasks"), Sense::click())
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
        if response.clicked() {
            self.tasks_open = !self.tasks_open;
        }
        let muted = theme::muted_foreground();
        theme::paint_icon(
            ui,
            "list-todo",
            Rect::from_center_size(
                Pos2::new(row.left() + 12.0, row.center().y),
                Vec2::splat(12.0),
            ),
            12.0,
            muted,
        );
        // Actions: the count, the segments (wide composers) and the chevron.
        let completed = plan
            .steps
            .iter()
            .filter(|s| s.status == Status::Completed)
            .count();
        let total = plan.steps.len();
        let mut right = row.right();
        let chevron = Rect::from_min_size(Pos2::new(right - 24.0, row.top()), Vec2::splat(24.0));
        theme::paint_icon(
            ui,
            if self.tasks_open {
                "chevron-down"
            } else {
                "chevron-up"
            },
            Rect::from_center_size(chevron.center(), Vec2::splat(14.0)),
            14.0,
            theme::foreground(),
        );
        right = chevron.left() - 4.0;
        if rect.width() >= 560.0 && (2..=10).contains(&total) {
            let segments = Rect::from_min_size(
                Pos2::new(right - 80.0, row.center().y - 1.5),
                Vec2::new(80.0, 3.0),
            );
            let gap = 2.0;
            let width = (segments.width() - gap * (total - 1) as f32) / total as f32;
            for (index, step) in plan.steps.iter().enumerate() {
                let color = match step.status {
                    Status::Completed => SUCCESS,
                    Status::InProgress => theme::primary(),
                    Status::Pending => theme::alpha(muted, 0.25),
                };
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(
                            segments.left() + index as f32 * (width + gap),
                            segments.top(),
                        ),
                        Vec2::new(width, 3.0),
                    ),
                    1.5,
                    color,
                );
            }
            right = segments.left() - 4.0;
        }
        let count = format!("{completed}/{total}");
        let count_font = font(12.0, Weight::Medium);
        let count_width = widgets::tabular_width(ui, &count, count_font.clone()).max(24.0);
        widgets::tabular_text(
            ui,
            Pos2::new(
                right - count_width
                    + (count_width - widgets::tabular_width(ui, &count, count_font.clone())) / 2.0,
                row.top() + 4.0,
            ),
            &count,
            count_font,
            if completed >= total { SUCCESS } else { muted },
            16.0,
        );
        right -= count_width + 4.0;
        let content = Rect::from_min_max(
            Pos2::new(row.left() + 28.0, row.top()),
            Pos2::new(right, row.bottom()),
        );
        let lead = widgets::text_in(ui, content, "Tasks", font(12.0, Weight::Regular), muted);
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(content.left() + lead + 4.0, row.top()),
                content.max,
            ),
            current_step(plan),
            font(12.0, Weight::Medium),
            theme::alpha(theme::foreground(), 0.8),
        );
        if !self.tasks_open {
            return;
        }
        // The steps, scrolled past 24rem.
        let list = Rect::from_min_max(
            Pos2::new(row.left(), row.bottom()),
            Pos2::new(row.right(), rect.bottom() - OVERLAP - 4.0),
        );
        let mut child = ui.new_child(eframe::egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        eframe::egui::ScrollArea::vertical()
            .id_salt("composer-tasks-list")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let mut y = ui.max_rect().top();
                for step in &plan.steps {
                    let height = step_height(ui, step, list.width());
                    let (icon, icon_color, text_color) = match step.status {
                        Status::Completed => ("check", SUCCESS, theme::alpha(muted, 0.55)),
                        Status::InProgress => (
                            "circle-dot",
                            theme::primary(),
                            theme::alpha(theme::foreground(), 0.9),
                        ),
                        Status::Pending => {
                            ("circle", theme::alpha(muted, 0.4), theme::alpha(muted, 0.7))
                        }
                    };
                    theme::paint_icon(
                        ui,
                        icon,
                        Rect::from_center_size(
                            Pos2::new(list.left() + 12.0, y + 4.0 + 8.0),
                            Vec2::splat(12.0),
                        ),
                        12.0,
                        icon_color,
                    );
                    let galley = widgets::galley(
                        ui,
                        &step.step,
                        font(12.0, Weight::Regular),
                        text_color,
                        16.0,
                        (list.width() - 8.0 - 24.0 - 4.0 - 48.0 - 8.0).max(40.0),
                    );
                    ui.painter()
                        .galley(Pos2::new(list.left() + 28.0, y + 4.0), galley, text_color);
                    let duration = match (step.duration_ms, &step.status) {
                        (Some(ms), _) => Some(crate::timeline::format_duration(ms)),
                        (None, Status::InProgress) => Some("now".to_owned()),
                        _ => None,
                    };
                    if let Some(duration) = duration {
                        let duration_font = font(10.0, Weight::Regular);
                        let width = widgets::tabular_width(ui, &duration, duration_font.clone());
                        widgets::tabular_text(
                            ui,
                            Pos2::new(list.right() - 8.0 - width, y + 4.0 + 1.0),
                            &duration,
                            duration_font,
                            theme::alpha(muted, 0.45),
                            14.0,
                        );
                    }
                    y += height + 1.0;
                }
                ui.allocate_rect(
                    Rect::from_min_max(ui.max_rect().min, Pos2::new(list.right(), y - 1.0)),
                    Sense::hover(),
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn update(id: &str, at: &str, statuses: &[&str]) -> Activity {
        let steps = ["Read", "Port", "Check"];
        Activity {
            id: id.into(),
            kind: "turn.plan.updated".into(),
            payload: json!({"plan": steps.iter().zip(statuses).map(|(step, status)| {
                json!({"step": step, "status": status})
            }).collect::<Vec<_>>()}),
            turn_id: Some("t1".into()),
            created_at: at.into(),
            ..Default::default()
        }
    }

    #[test]
    fn plans_keep_their_latest_steps_and_durations() {
        let activities = vec![
            update(
                "a",
                "2026-01-01T00:00:00Z",
                &["inProgress", "pending", "pending"],
            ),
            update(
                "b",
                "2026-01-01T00:00:30Z",
                &["completed", "inProgress", "pending"],
            ),
            update(
                "c",
                "2026-01-01T00:01:30Z",
                &["completed", "completed", "inProgress"],
            ),
        ];
        let plan = active_plan(&activities, Some("t1")).unwrap();
        let durations: Vec<Option<i64>> = plan.steps.iter().map(|s| s.duration_ms).collect();
        assert_eq!(durations, [Some(30_000), Some(60_000), None]);
        assert_eq!(current_step(&plan), "Check");
        // An update without a plan clears the timings before it.
        let mut cleared = activities.clone();
        cleared.insert(
            2,
            Activity {
                id: "x".into(),
                kind: "turn.plan.updated".into(),
                payload: json!({}),
                turn_id: Some("t1".into()),
                created_at: "2026-01-01T00:01:00Z".into(),
                ..Default::default()
            },
        );
        let plan = active_plan(&cleared, Some("t1")).unwrap();
        assert!(plan.steps.iter().all(|s| s.duration_ms.is_none()));
    }
}
