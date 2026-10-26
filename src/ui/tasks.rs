//! The running turn's task list (`chat/ComposerTasksBadge.tsx`, with
//! `deriveActivePlanState` from `session-logic.ts` and
//! `activeComposerTasksProgress` from `ChatView.tsx`): while a run keeps a
//! todo list (the provider's plan, Codex's `update_plan` or Claude's todo
//! list), a "Tasks" tab above the composer shows the current step, the count
//! and the step segments; a click opens the drawer listing every step with
//! its status and how long it took.
use eframe::egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::App;
use crate::model::{Detail, Thread};
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

/// `ActivePlanState`: the todo list a run last reported.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActivePlan {
    pub run_id: Option<String>,
    pub steps: Vec<Step>,
}

/// `deriveActivePlanState`: the run's latest todo list, else the thread's.
pub(crate) fn active_plan(detail: &Detail, run: Option<&str>) -> Option<ActivePlan> {
    let plans: Vec<_> = detail
        .plans
        .iter()
        .filter(|p| p.kind == "todo_list")
        .collect();
    let plan = run
        .and_then(|run| {
            plans
                .iter()
                .rev()
                .find(|p| p.run_id.as_deref() == Some(run))
        })
        .or(plans.last())?;
    if plan.steps.is_empty() {
        return None;
    }
    let steps = plan
        .steps
        .iter()
        .map(|step| Step {
            step: step.text.clone(),
            status: match step.status.as_str() {
                "completed" => Status::Completed,
                "running" => Status::InProgress,
                _ => Status::Pending,
            },
            duration_ms: step.duration_ms.map(|ms| ms as i64),
        })
        .collect();
    Some(ActivePlan {
        run_id: plan.run_id.clone(),
        steps,
    })
}

/// `activeComposerTasksProgress`: the running run's own todo list (older
/// runs' plans must not label fresh work).
pub(crate) fn active_tasks(thread: &Thread, detail: &Detail) -> Option<ActivePlan> {
    if thread.latest_run_settled() {
        return None;
    }
    let run = thread.latest_run.as_ref()?;
    let plan = active_plan(detail, Some(&run.run_id))?;
    (plan.run_id.as_deref() == Some(run.run_id.as_str())).then_some(plan)
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
        active_tasks(&*self.current_thread()?, &*self.current_detail()?)
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

    #[test]
    fn todo_lists_follow_the_run() {
        let plan = |id: &str, run: &str, statuses: &[&str]| {
            let steps: Vec<_> = ["Read", "Port", "Check"]
                .iter()
                .zip(statuses)
                .map(|(text, status)| json!({"id": text, "text": text, "status": status, "durationMs": 1000}))
                .collect();
            json!({"id": id, "runId": run, "nodeId": "n", "status": "active", "kind": "todo_list", "steps": steps})
        };
        let detail: Detail = serde_json::from_value(json!({
            "thread": {"id": "t"},
            "plans": [
                plan("a", "r1", &["completed", "running", "pending"]),
                plan("b", "r2", &["running", "pending", "pending"]),
            ],
        }))
        .unwrap();
        let first = active_plan(&detail, Some("r1")).unwrap();
        assert_eq!(current_step(&first), "Port");
        assert_eq!(first.steps[0].duration_ms, Some(1000));
        // Without a plan of its own, a run falls back to the latest.
        assert_eq!(
            active_plan(&detail, Some("r3")).unwrap().run_id.as_deref(),
            Some("r2")
        );
    }
}
