//! Edit from here (`MessagesTimeline.tsx`'s `RevertUserMessageButton`,
//! `ChatView.tsx`'s `onRevertToTurnCount` and its "Edit from here?" dialog,
//! `deriveRevertTurnCountByUserMessageId`): a user message that started a
//! turn with a ready checkpoint rewinds the thread to before it
//! (`checkpoint.rollback`), restoring the workspace's files too or keeping
//! the changes, and its prompt returns to the composer once the server took
//! the rollback.
//!
//! Not ported: the message's attachments returning to the composer.

use eframe::egui::{
    self, Color32, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use serde_json::json;

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::timeline::Message;
use crate::ui::widgets;

/// A rewind asked for, then on its way.
pub(crate) enum Rewind {
    Confirm {
        thread: String,
        checkpoint: String,
        scope: String,
        text: String,
    },
    Pending {
        command: String,
        thread: String,
        text: String,
    },
}

impl App {
    /// The checkpoint a user message rewinds to: the ready checkpoint of the
    /// run it started (`deriveRevertTurnCountByUserMessageId`), when the
    /// thread's provider can roll its conversation back.
    pub(crate) fn rewind_checkpoint(&self, message: &Message) -> Option<(String, String)> {
        if !matches!(
            message.input_intent.as_deref(),
            Some("turn_start" | "queued_turn")
        ) {
            return None;
        }
        let thread = self.current_thread()?;
        let supported = self
            .providers()
            .iter()
            .find(|p| p["instanceId"] == thread.model_selection.instance_id.as_str())
            .is_none_or(|p| p["supportsConversationRollback"] != false);
        if !supported {
            return None;
        }
        let run = message.run_id.as_deref()?;
        let detail = self.current_detail()?;
        detail
            .checkpoints
            .iter()
            .rev()
            .find(|c| {
                c.status == "ready"
                    && c.run_id.as_deref() == Some(run)
                    && c.app_run_ordinal.is_some()
            })
            .map(|c| (c.id.clone(), c.scope_id.clone()))
    }

    /// The Edit from here button: the dialog, unless a turn is under way.
    pub(crate) fn request_rewind(&mut self, message: &Message) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        if thread.is_running() {
            self.notify(
                crate::ui::toasts::Kind::Warning,
                "Interrupt the current turn before reverting checkpoints.",
            );
            return;
        }
        if !self.connected() {
            self.notify(
                crate::ui::toasts::Kind::Warning,
                "Reconnect the server before reverting checkpoints.",
            );
            return;
        }
        let Some((checkpoint, scope)) = self.rewind_checkpoint(message) else {
            return;
        };
        self.rewind = Some(Rewind::Confirm {
            thread: thread.id.clone(),
            checkpoint,
            scope,
            text: message.text.clone(),
        });
    }

    /// The server took (or refused) a command: a rewind on its way ends.
    pub(crate) fn rewind_done(&mut self, command: &str, error: Option<&str>) {
        let Some(Rewind::Pending {
            command: pending,
            thread,
            text,
        }) = &self.rewind
        else {
            return;
        };
        if pending != command {
            return;
        }
        let (thread, text) = (thread.clone(), text.clone());
        self.rewind = None;
        if let Some(error) = error {
            self.notify(crate::ui::toasts::Kind::Error, error.to_owned());
            return;
        }
        // The prompt returns after what the composer already holds.
        if self.selected.as_deref() == Some(thread.as_str()) && !text.trim().is_empty() {
            if self.composer.trim().is_empty() {
                self.composer = text;
            } else {
                self.composer = format!("{}\n\n{text}", self.composer);
            }
        }
    }

    /// Whether a rewind is on its way (the buttons wait).
    pub(crate) fn rewinding(&self) -> bool {
        matches!(self.rewind, Some(Rewind::Pending { .. }))
    }

    /// "Edit from here?": Cancel, Revert files too, Revert and keep changes.
    pub(crate) fn rewind_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        if !matches!(self.rewind, Some(Rewind::Confirm { .. })) {
            return;
        }
        let mut decision: Option<Option<bool>> = None;
        egui::Area::new(ui.id().with("rewind-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(backdrop, 0.0, Color32::from_black_alpha(140));
                let width = 460.0;
                let title = widgets::galley(
                    ui,
                    "Edit from here?",
                    font(16.0, Weight::SemiBold),
                    theme::foreground(),
                    24.0,
                    width - 48.0,
                );
                let message = widgets::galley(
                    ui,
                    "Rewind chat to before this message. Your prompt and attachments return to the composer.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    width - 48.0,
                );
                let height = 24.0 + title.size().y + 8.0 + message.size().y + 24.0 + 32.0 + 24.0;
                let dialog = Rect::from_center_size(backdrop.center(), Vec2::new(width, height));
                ui.painter().rect(
                    dialog,
                    CornerRadius::same(14),
                    theme::card(),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let title_height = title.size().y;
                ui.painter()
                    .galley(dialog.min + Vec2::new(24.0, 24.0), title, theme::foreground());
                ui.painter().galley(
                    dialog.min + Vec2::new(24.0, 24.0 + title_height + 8.0),
                    message,
                    theme::muted_foreground(),
                );
                // From the right: the primary keep, the destructive revert,
                // then Cancel.
                let label_font = font(14.0, Weight::Medium);
                let mut right = dialog.right() - 24.0;
                let top = dialog.bottom() - 24.0 - 32.0;
                for (index, label) in ["Revert and keep changes", "Revert files too", "Cancel"]
                    .into_iter()
                    .enumerate()
                {
                    let button_width =
                        (widgets::text_width(ui, label, label_font.clone()) + 24.0).ceil();
                    let rect = Rect::from_min_size(
                        Pos2::new(right - button_width, top),
                        Vec2::new(button_width, 32.0),
                    );
                    let response = ui.interact(rect, ui.id().with(("rewind", index)), Sense::click());
                    let destructive = Color32::from_rgb(251, 65, 74);
                    let (fill, color, stroke) = match index {
                        0 => (
                            if response.hovered() {
                                theme::primary_hover()
                            } else {
                                theme::primary()
                            },
                            theme::primary_foreground(),
                            Stroke::NONE,
                        ),
                        1 => (
                            if response.hovered() {
                                destructive
                            } else {
                                theme::alpha(destructive, 0.9)
                            },
                            Color32::WHITE,
                            Stroke::NONE,
                        ),
                        _ => (
                            if response.hovered() {
                                theme::overlay(0.06)
                            } else {
                                theme::control_surface()
                            },
                            theme::foreground(),
                            Stroke::new(1.0, theme::input()),
                        ),
                    };
                    ui.painter()
                        .rect(rect, CornerRadius::same(8), fill, stroke, StrokeKind::Inside);
                    widgets::text_in(
                        ui,
                        Rect::from_min_max(rect.min + Vec2::new(12.0, 0.0), rect.max),
                        label,
                        label_font.clone(),
                        color,
                    );
                    if response.clicked() {
                        decision = Some(match index {
                            0 => Some(false),
                            1 => Some(true),
                            _ => None,
                        });
                    }
                    right = rect.left() - 8.0;
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    decision = Some(None);
                }
            });
        let Some(decision) = decision else {
            return;
        };
        let Some(Rewind::Confirm {
            thread,
            checkpoint,
            scope,
            text,
        }) = self.rewind.take()
        else {
            return;
        };
        let Some(restore_files) = decision else {
            return;
        };
        let command_id = uuid::Uuid::new_v4().to_string();
        self.dispatch(json!({
            "type": "checkpoint.rollback",
            "commandId": command_id,
            "threadId": thread,
            "scopeId": scope,
            "checkpointId": checkpoint,
            "restoreFiles": restore_files,
        }));
        self.rewind = Some(Rewind::Pending {
            command: command_id,
            thread,
            text,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Options;

    #[test]
    fn a_turn_with_a_ready_checkpoint_rewinds_and_its_prompt_returns() {
        let mut harness = egui_kittest::Harness::builder().build_eframe(|cc| {
            App::new(
                cc,
                Options {
                    server: None,
                    token: None,
                    client_settings_path: None,
                    block_turn_starts: true,
                },
            )
        });
        let app = harness.state_mut();
        let at = "2026-10-01T00:00:00.000Z";
        let config = json!({"type": "snapshot", "config": {"providers": [], "settings": {}}});
        let shell = json!({"kind": "snapshot", "snapshot": {"snapshotSequence": 1,
            "projects": [{"id": "p", "title": "Demo", "workspaceRoot": "/fixture/demo"}],
            "threads": [{"id": "a", "projectId": "p", "title": "T", "createdAt": at,
                "updatedAt": at, "modelSelection": {"instanceId": "codex", "model": "gpt"}}]}});
        app.load_offline(&config, &shell).unwrap();
        app.select_thread(Some("a".into()));
        let detail = json!({"kind": "snapshot", "snapshotSequence": 2, "projection": {
            "thread": {"id": "a", "projectId": "p", "title": "T"},
            "checkpoints": [{"id": "c1", "scopeId": "s", "runId": "r", "nodeId": "n",
                "ordinalWithinScope": 1, "appRunOrdinal": 1, "ref": "x", "status": "ready",
                "files": [], "capturedAt": at}],
            "visibleTurnItems": [], "updatedAt": at}});
        app.load_offline_thread(&detail).unwrap();
        let message = Message {
            id: "u".into(),
            role: "user".into(),
            text: "Fix the bug".into(),
            run_id: Some("r".into()),
            input_intent: Some("turn_start".into()),
            ..Default::default()
        };
        assert_eq!(
            app.rewind_checkpoint(&message),
            Some(("c1".into(), "s".into()))
        );
        let steer = Message {
            input_intent: Some("steer".into()),
            ..message.clone()
        };
        assert_eq!(app.rewind_checkpoint(&steer), None);
        // Offline the rollback cannot go.
        app.request_rewind(&message);
        assert!(app.rewind.is_none());
        // The server took it: the prompt comes back after the composer's text.
        app.composer = "Draft".into();
        app.rewind = Some(Rewind::Pending {
            command: "cmd".into(),
            thread: "a".into(),
            text: "Fix the bug".into(),
        });
        app.rewind_done("cmd", None);
        assert_eq!(app.composer, "Draft\n\nFix the bug");
    }
}
