//! Messages queued while a turn runs (`queuedMessageStore.ts`,
//! `QueuedMessageSender.tsx`, `QueuedMessageTimelineRow` in
//! `chat/MessagesTimeline.tsx`): with the "Queue" follow-up behavior, a
//! message sent during a turn waits in memory and goes out once a tool call
//! finishes after it was queued, or when the turn is over; approvals and
//! questions hold the queue, and each send waits for the server to take the
//! previous one. The rows are dashed user bubbles at the end of the timeline
//! with "Send now" and "Cancel and return to the composer"; Stop returns every
//! queued message to the composer.
//!
//! Off-screen threads go out when their turn ends (their tool calls are not
//! followed). Rendered from the upstream source; not compared with the
//! reference, which cannot show a running turn on the fixture.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::Value;

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::ui::attachments::ComposerImage;
use crate::ui::widgets;

/// A message waiting for the running turn (`QueuedComposerMessage`).
pub(crate) struct Queued {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) images: Vec<ComposerImage>,
    /// The composer's send settings when it was queued.
    pub(crate) model: Value,
    pub(crate) runtime: String,
    pub(crate) interaction: String,
    /// The newest finished tool call at queue time.
    pub(crate) queued_after: Option<String>,
}

/// Queues by thread id, and per thread the user message of the last queued
/// send the server has not shown yet.
#[derive(Default)]
pub(crate) struct Queues {
    pub(crate) by_thread: HashMap<String, Vec<Queued>>,
    waiting: HashMap<String, String>,
}

/// `latestCompletedToolActivityId`: the newest finished tool call.
pub(crate) fn latest_tool_call(thread: &Thread) -> Option<String> {
    thread
        .activities
        .iter()
        .filter(|a| a.kind == "tool.completed")
        .max_by(|a, b| {
            a.sequence
                .unwrap_or(0)
                .cmp(&b.sequence.unwrap_or(0))
                .then_with(|| a.created_at.cmp(&b.created_at))
        })
        .map(|a| a.id.clone())
}

/// `isQueuedMessageDue`: mid-turn once a tool call finished after it was
/// queued; at once when the turn is over; never while connecting.
fn is_due(status: Option<&str>, latest_tool: Option<&str>, queued_after: Option<&str>) -> bool {
    match status {
        Some("starting") => false,
        Some("running") => latest_tool != queued_after,
        _ => true,
    }
}

impl App {
    /// Queues the composer's content for the open thread.
    pub(crate) fn enqueue_composer(&mut self, thread: &Thread, model: Value, runtime: String) {
        let queued = Queued {
            id: uuid::Uuid::new_v4().to_string(),
            text: self.composer.trim().to_owned(),
            images: std::mem::take(&mut self.composer_images),
            model,
            runtime,
            interaction: self.composer_interaction_mode(false),
            queued_after: latest_tool_call(thread),
        };
        self.composer.clear();
        self.queues
            .by_thread
            .entry(thread.id.clone())
            .or_default()
            .push(queued);
    }

    /// Sends each thread's next queued message once it is due.
    pub(crate) fn send_due_queued(&mut self) {
        let threads: Vec<String> = self.queues.by_thread.keys().cloned().collect();
        for thread_id in threads {
            let detail = self.state.detail.clone().filter(|t| t.id == thread_id);
            let Some(thread) = detail.clone().or_else(|| {
                self.state
                    .threads
                    .iter()
                    .find(|t| t.id == thread_id)
                    .cloned()
            }) else {
                continue;
            };
            // The previous send has not reached the thread yet.
            if let Some(message_id) = self.queues.waiting.get(&thread_id) {
                let arrived = detail
                    .as_ref()
                    .is_some_and(|t| t.messages.iter().any(|m| &m.id == message_id))
                    || detail.is_none() && thread.is_running();
                if !arrived {
                    continue;
                }
                self.queues.waiting.remove(&thread_id);
            }
            if thread.has_pending_approvals || thread.has_pending_user_input || !self.connected() {
                continue;
            }
            let latest = detail.as_deref().and_then(latest_tool_call);
            let status = thread.session.as_ref().map(|s| s.status.as_str());
            let Some(next) = self
                .queues
                .by_thread
                .get(&thread_id)
                .and_then(|q| q.first())
            else {
                continue;
            };
            // Off-screen threads only go out once their turn is over.
            let due = if detail.is_some() {
                is_due(status, latest.as_deref(), next.queued_after.as_deref())
            } else {
                !thread.is_running()
            };
            if due {
                let id = next.id.clone();
                self.send_queued(&thread_id, &id, latest);
            }
        }
    }

    /// `sendQueuedMessage`: dispatches one queued message; the rest of the
    /// queue waits for the next tool boundary after it.
    pub(crate) fn send_queued(&mut self, thread_id: &str, id: &str, latest: Option<String>) {
        let Some(queue) = self.queues.by_thread.get_mut(thread_id) else {
            return;
        };
        let Some(index) = queue.iter().position(|q| q.id == id) else {
            return;
        };
        let queued = queue.remove(index);
        for other in queue.iter_mut() {
            other.queued_after = latest.clone();
        }
        if queue.is_empty() {
            self.queues.by_thread.remove(thread_id);
        }
        let command = crate::ui::composer::turn_start_command(
            thread_id,
            &queued.text,
            &queued.images,
            &queued.model,
            &queued.runtime,
            &queued.interaction,
        );
        if let Some(message_id) = command["message"]["messageId"].as_str() {
            self.queues
                .waiting
                .insert(thread_id.to_owned(), message_id.to_owned());
        }
        self.dispatch(command);
    }

    /// `restoreQueuedMessagesToComposer`: the texts join the draft (blank
    /// lines between), the images follow.
    pub(crate) fn restore_queued(&mut self, messages: Vec<Queued>) {
        let prompts: Vec<String> = std::iter::once(self.composer.clone())
            .chain(messages.iter().map(|m| m.text.clone()))
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        self.composer = prompts.join("\n\n");
        for message in messages {
            self.composer_images.extend(message.images);
        }
    }

    /// Stop also cancels the queue: its messages return to the composer.
    pub(crate) fn drain_queue(&mut self, thread_id: &str) {
        if let Some(messages) = self.queues.by_thread.remove(thread_id) {
            self.restore_queued(messages);
        }
    }

    /// The open thread's queued rows (`QueuedMessageTimelineRow`).
    pub(crate) fn queued_rows(&mut self, ui: &mut Ui, thread: &Thread, width: f32) {
        let Some(queue) = self.queues.by_thread.get(&thread.id) else {
            return;
        };
        let rows: Vec<(String, String, usize)> = queue
            .iter()
            .map(|q| (q.id.clone(), q.text.clone(), q.images.len()))
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
                "Sends after the next tool call or when the turn ends"
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
            if widgets::icon_button(
                ui,
                send_rect,
                ("queued-send", id),
                "arrow-up",
                14.0,
                theme::muted_foreground(),
                "Send now",
            )
            .clicked()
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
                "Cancel and return to the composer",
            )
            .clicked()
            {
                cancel = Some(id.clone());
            }
        }
        // Send now holds while a request blocks the agent.
        if let Some(id) = send_now
            && !thread.has_pending_approvals
            && !thread.has_pending_user_input
        {
            self.send_queued(&thread.id, &id, latest_tool_call(thread));
        }
        if let Some(id) = cancel
            && let Some(queue) = self.queues.by_thread.get_mut(&thread.id)
            && let Some(index) = queue.iter().position(|q| q.id == id)
        {
            let message = queue.remove(index);
            if queue.is_empty() {
                self.queues.by_thread.remove(&thread.id);
            }
            self.restore_queued(vec![message]);
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
    fn queued_messages_wait_for_a_tool_boundary() {
        assert!(!is_due(Some("running"), Some("a"), Some("a")));
        assert!(is_due(Some("running"), Some("b"), Some("a")));
        assert!(is_due(Some("running"), Some("a"), None));
        assert!(!is_due(Some("starting"), None, None));
        assert!(is_due(Some("ready"), Some("a"), Some("a")));
        assert!(is_due(None, None, None));
    }
}
