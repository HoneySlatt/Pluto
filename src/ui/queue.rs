//! Messages queued behind the running turn. Protocol 2 queues them on the
//! server as `queued` runs (`client-runtime/state/threadWorkflows.ts`,
//! `deriveThreadQueueWorkflowState`); "Steer" sends one into the running
//! turn (`queued-message.promote-to-steer`) and "Remove from queue" cancels
//! it (`queued-run.cancel`). They still show as v0.0.45's dashed user
//! bubbles at the end of the timeline (`QueuedMessageTimelineRow`); v0.1.0's
//! banner above the composer (`QueuedRunsControl.tsx`), with drag reorder and
//! editing in the composer, is not ported.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Detail, Run};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `QueuedThreadRun`.
pub(crate) struct QueuedRun {
    pub(crate) run_id: String,
    pub(crate) text: String,
    pub(crate) attachments: usize,
}

/// `resolveActiveThreadRun`.
pub(crate) fn active_run(detail: &Detail) -> Option<&Run> {
    detail.runs.iter().rev().find(|run| {
        matches!(
            run.status.as_str(),
            "preparing" | "starting" | "running" | "waiting"
        )
    })
}

/// `resolveThreadProviderSession`.
fn provider_session(detail: &Detail) -> Option<&Value> {
    let provider_thread = active_run(detail)
        .and_then(|run| run.provider_thread_id.clone())
        .or_else(|| {
            detail.thread["activeProviderThreadId"]
                .as_str()
                .map(str::to_owned)
        });
    let attached = provider_thread
        .and_then(|id| {
            detail
                .provider_threads
                .iter()
                .find(|t| t["id"] == id.as_str())
        })
        .or_else(|| {
            detail.provider_threads.iter().find(|t| {
                t["appThreadId"] == detail.thread["id"] && !t["providerSessionId"].is_null()
            })
        });
    match attached.and_then(|t| t["providerSessionId"].as_str()) {
        Some(id) => detail.provider_sessions.iter().find(|s| s["id"] == id),
        None => detail
            .provider_sessions
            .iter()
            .rev()
            .find(|s| s["status"] != "stopped" && s["status"] != "error"),
    }
}

/// `deriveThreadQueueWorkflowState`: the user's queued runs in queue order
/// (automatic completion and notification runs are not messages), and
/// whether one can steer the running turn instead.
pub(crate) fn queued_runs(detail: &Detail) -> (Vec<QueuedRun>, bool) {
    let automatic: Vec<&str> = detail
        .messages
        .iter()
        .filter(|m| m.get("delegatedCompletion").is_some() || m.get("notification").is_some())
        .filter_map(|m| m["id"].as_str())
        .collect();
    let mut runs: Vec<&Run> = detail
        .runs
        .iter()
        .filter(|run| run.status == "queued" && !automatic.contains(&run.user_message_id.as_str()))
        .collect();
    runs.sort_by_key(|run| (run.queue_position.unwrap_or(run.ordinal), run.ordinal));
    let queued = runs
        .into_iter()
        .map(|run| {
            let message = detail
                .messages
                .iter()
                .find(|m| m["id"] == run.user_message_id.as_str());
            QueuedRun {
                run_id: run.id.clone(),
                text: message
                    .and_then(|m| m["text"].as_str())
                    .unwrap_or("Queued message")
                    .to_owned(),
                attachments: message
                    .and_then(|m| m["attachments"].as_array())
                    .map_or(0, Vec::len),
            }
        })
        .collect();
    let active = active_run(detail);
    let steerable_turn = active.is_some_and(|run| {
        run.status == "running"
            && run.active_attempt_id.as_deref().is_some_and(|attempt| {
                detail
                    .provider_turns
                    .iter()
                    .any(|t| t["runAttemptId"] == attempt && t["status"] == "running")
            })
    });
    let turns = provider_session(detail).map(|s| &s["capabilities"]["turns"]);
    let can_steer = steerable_turn
        && turns.is_some_and(|t| {
            t["supportsActiveSteering"] == true || t["supportsSteeringByInterruptRestart"] == true
        });
    (queued, can_steer)
}

impl App {
    /// The open thread's queued rows (`QueuedMessageTimelineRow`).
    pub(crate) fn queued_rows(&mut self, ui: &mut Ui, detail: &Detail, width: f32) {
        let (queued, can_steer) = queued_runs(detail);
        let thread_id = detail.id().to_owned();
        let rows: Vec<(String, String, usize)> = queued
            .into_iter()
            .map(|q| (q.run_id, q.text, q.attachments))
            .collect();
        let mut send_now = None;
        let mut cancel = None;
        for (index, (id, text, attachments)) in rows.iter().enumerate() {
            let max_width = (width * 0.8).floor();
            let color = theme::alpha(theme::foreground(), 0.8);
            let galley = (!text.is_empty()).then(|| {
                widgets::galley(
                    ui,
                    text,
                    font(14.0, Weight::Regular),
                    color,
                    22.75,
                    max_width - 24.0,
                )
            });
            let text_height = galley.as_ref().map_or(0.0, |g| g.size().y);
            let attachments_label = (*attachments > 0).then(|| {
                format!(
                    "{attachments} attachment{}",
                    if *attachments == 1 { "" } else { "s" }
                )
            });
            let attachments_height = attachments_label
                .as_ref()
                .map_or(0.0, |_| if text.is_empty() { 16.0 } else { 6.0 + 16.0 });
            let footer = 8.0 + 24.0;
            // The footer's own width: the clock, "Queued", `gap-4`, two buttons.
            let footer_width =
                18.0 + widgets::text_width(ui, "Queued", font(12.0, Weight::Regular)) + 16.0 + 50.0;
            let attachments_width = attachments_label.as_ref().map_or(0.0, |label| {
                widgets::text_width(ui, label, font(12.0, Weight::Regular))
            });
            let content_width = galley
                .as_ref()
                .map_or(0.0, |g| g.size().x)
                .max(footer_width)
                .max(attachments_width)
                .min(max_width - 24.0);
            let size = Vec2::new(
                content_width + 24.0,
                12.0 + text_height + attachments_height + footer + 12.0,
            );
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, size.y + 16.0), Sense::hover());
            let bubble = Rect::from_min_size(Pos2::new(row.right() - size.x, row.top()), size);
            dashed_rounded_rect(ui, bubble, 18.0, theme::border());
            let mut y = bubble.top() + 12.0;
            if let Some(galley) = galley {
                ui.painter()
                    .galley(Pos2::new(bubble.left() + 12.0, y), galley, color);
                y += text_height;
            }
            if let Some(label) = &attachments_label {
                if !text.is_empty() {
                    y += 6.0;
                }
                widgets::text(
                    ui,
                    Pos2::new(bubble.left() + 12.0, y),
                    label,
                    font(12.0, Weight::Regular),
                    theme::secondary_label(),
                    16.0,
                );
                y += 16.0;
            }
            // Footer: the clock and "Queued", then Send now and Cancel.
            let footer_top = y + 8.0;
            let muted = theme::secondary_label();
            theme::paint_icon(
                ui,
                "clock",
                Rect::from_min_size(
                    Pos2::new(bubble.left() + 12.0, footer_top + 5.0),
                    Vec2::splat(14.0),
                ),
                14.0,
                muted,
            );
            let status = if index == 0 {
                "Sends when the running turn ends"
            } else {
                "Sends after the messages above it"
            };
            let label_rect = Rect::from_min_size(
                Pos2::new(bubble.left() + 12.0 + 18.0, footer_top),
                Vec2::new(60.0, 24.0),
            );
            widgets::text_in(ui, label_rect, "Queued", font(12.0, Weight::Regular), muted);
            ui.interact(
                label_rect,
                ui.id().with(("queued-status", id)),
                Sense::hover(),
            )
            .on_hover_text(status);
            let cancel_rect = Rect::from_min_size(
                Pos2::new(bubble.right() - 12.0 - 24.0, footer_top),
                Vec2::splat(24.0),
            );
            let send_rect = Rect::from_min_size(
                Pos2::new(cancel_rect.left() - 2.0 - 24.0, footer_top),
                Vec2::splat(24.0),
            );
            let steer_tip = if can_steer {
                "Send as a steer instead"
            } else {
                "There is no active run to steer"
            };
            if widgets::icon_button(
                ui,
                send_rect,
                ("queued-send", id),
                "corner-up-right",
                14.0,
                theme::muted_foreground(),
                steer_tip,
            )
            .clicked()
                && can_steer
            {
                send_now = Some(id.clone());
            }
            if widgets::icon_button(
                ui,
                cancel_rect,
                ("queued-cancel", id),
                "x",
                14.0,
                theme::muted_foreground(),
                "Remove from queue",
            )
            .clicked()
            {
                cancel = Some(id.clone());
            }
        }
        if let Some(id) = send_now
            && let Some(active) = active_run(detail)
        {
            let mut command = crate::model::command("queued-message.promote-to-steer", &thread_id);
            command["queuedRunId"] = json!(id);
            command["targetRunId"] = json!(active.id);
            self.dispatch(command);
        }
        if let Some(id) = cancel {
            let mut command = crate::model::command("queued-run.cancel", &thread_id);
            command["runId"] = json!(id);
            self.dispatch(command);
        }
    }
}

/// A 1px dashed outline (`border border-dashed`) around a rounded rect.
fn dashed_rounded_rect(ui: &Ui, rect: Rect, radius: f32, color: Color32) {
    let rect = rect.shrink(0.5);
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut points = Vec::new();
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -90.0_f32),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0),
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0),
    ];
    points.push(Pos2::new(rect.left() + r, rect.top()));
    for (center, start) in corners {
        for step in 0..=8 {
            let angle = (start + step as f32 * 90.0 / 8.0).to_radians();
            points.push(center + Vec2::new(angle.cos(), angle.sin()) * r);
        }
    }
    points.push(Pos2::new(rect.left() + r, rect.top()));
    ui.painter().extend(egui::Shape::dashed_line(
        &points,
        Stroke::new(1.0, color),
        3.0,
        3.0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_runs_follow_the_queue_order() {
        let run = |id: &str, ordinal: u64, position: Option<u64>, message: &str| {
            json!({"id":id,"threadId":"t","ordinal":ordinal,"status":"queued",
                "queuePosition":position,"userMessageId":message,"requestedAt":"1"})
        };
        let detail: Detail = serde_json::from_value(json!({
            "thread": {"id": "t"},
            "runs": [run("a", 2, Some(2), "m1"), run("b", 3, Some(1), "m2"), run("w", 4, None, "m3")],
            "messages": [
                {"id": "m1", "text": "first", "attachments": []},
                {"id": "m2", "text": "second", "attachments": [{"type": "image"}]},
                {"id": "m3", "text": "wake", "attachments": [], "notification": {}},
            ],
        }))
        .unwrap();
        let (queued, can_steer) = queued_runs(&detail);
        let texts: Vec<&str> = queued.iter().map(|q| q.text.as_str()).collect();
        assert_eq!(texts, ["second", "first"]);
        assert_eq!(queued[0].attachments, 1);
        assert!(!can_steer);
    }
}
