//! Messages queued behind the running turn (`QueuedRunsControl.tsx`,
//! `queuedMessageEdit.ts`). Protocol 2 queues them on the server as `queued`
//! runs (`client-runtime/state/threadWorkflows.ts`,
//! `deriveThreadQueueWorkflowState`); they list in a banner attached above
//! the composer, where the grip drags one to another place
//! (`queued-run.reorder`), the pencil edits one in the composer
//! (`queued-run.edit`), "Steer" sends one into the running turn
//! (`queued-message.promote-to-steer`) and the cross cancels it
//! (`queued-run.cancel`). Stop holds the queue; Resume releases it
//! (`queue.resume`, in `composer.rs`).
//!
//! Not ported: the grip's arrow keys, the rows of messages still on their
//! way to the server, and changing a queued message's attachments (an edit
//! sends its text and keeps them).

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Detail, Run};
use crate::theme::{self, Weight, font};
use crate::ui::requests_panel::OVERLAP;
use crate::ui::widgets;

/// A row, the header's and the queued messages' (`size-6` controls).
const ROW: f32 = 24.0;
/// `ComposerBanner.Scroll`'s `max-h-32`.
const LIST_MAX: f32 = 128.0;
/// `--chat-composer-drawer-inset`.
const INSET: f32 = 22.0;
/// `ATTACHMENT_ONLY_BOOTSTRAP_PROMPT`.
const ATTACHMENT_ONLY_PROMPT: &str = "[User attached one or more files without additional text. Respond using the conversation context and the attached files.]";

/// `QueuedThreadRun`.
pub(crate) struct QueuedRun {
    pub(crate) run_id: String,
    pub(crate) text: String,
    pub(crate) attachments: Vec<Value>,
}

/// `ThreadQueueWorkflowState`.
pub(crate) struct Workflow {
    pub(crate) queued: Vec<QueuedRun>,
    /// Stop held the queue; Resume releases it.
    pub(crate) held: bool,
    pub(crate) can_reorder: bool,
    pub(crate) can_steer: bool,
}

/// The banner's state across frames.
#[derive(Default)]
pub(crate) struct QueueUi {
    pub(crate) collapsed: bool,
    /// The run whose grip is dragged, and the queue index it would land at.
    drag: Option<(String, Option<usize>)>,
    pub(crate) editing: Option<QueuedEdit>,
    /// A resume's continuation (its command id) and its thread, whose queue
    /// is released once the server takes it.
    resume_after: Option<(String, String)>,
}

/// A queued message open in the composer; the thread's own draft waits.
pub(crate) struct QueuedEdit {
    thread_id: String,
    pub(crate) run_id: String,
    original: String,
    attachments: usize,
    saved: (
        String,
        Vec<crate::ui::attachments::ComposerImage>,
        Vec<crate::ui::attachments::ComposerFile>,
        Vec<Value>,
    ),
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
/// (automatic completion and notification runs are not messages), and what
/// the provider lets one do with them.
pub(crate) fn workflow(detail: &Detail) -> Workflow {
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
                    .cloned()
                    .unwrap_or_default(),
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
    Workflow {
        queued,
        held: detail
            .runs
            .iter()
            .any(|run| run.status == "queued" && run.queue_held == Some(true)),
        can_reorder: turns.is_some_and(|t| t["supportsQueuedMessages"] == true),
        can_steer: steerable_turn
            && turns.is_some_and(|t| {
                t["supportsActiveSteering"] == true
                    || t["supportsSteeringByInterruptRestart"] == true
            }),
    }
}

fn millis(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso).map_or(0, |t| t.timestamp_millis())
}

/// `latestExecutedRun` (`runRanAfter`): the run that last ran, an unfinished
/// one first; queued runs and those cancelled before starting never ran.
fn latest_executed_run(runs: &[Run]) -> Option<&Run> {
    let end = |run: &Run| run.completed_at.as_deref().map_or(i64::MAX, millis);
    let mut latest: Option<&Run> = None;
    for run in runs {
        if run.status == "queued" || (run.status == "cancelled" && run.started_at.is_none()) {
            continue;
        }
        let after = latest.is_none_or(|other| {
            if end(run) == end(other) {
                run.ordinal > other.ordinal
            } else {
                end(run) > end(other)
            }
        });
        if after {
            latest = Some(run);
        }
    }
    latest
}

/// `latestRootProviderFailure`'s class: the run's latest failed error item
/// on its root node.
fn root_failure_class(run: &Run, detail: &Detail) -> Option<String> {
    if run.status != "failed" {
        return None;
    }
    detail
        .turn_items
        .iter()
        .filter(|item| {
            item.kind == "error"
                && item.status == "failed"
                && item.run_id.as_deref() == Some(run.id.as_str())
                && item.node_id == run.root_node_id
        })
        .max_by(|a, b| {
            (millis(&a.updated_at), a.ordinal, &a.id).cmp(&(
                millis(&b.updated_at),
                b.ordinal,
                &b.id,
            ))
        })
        .and_then(|item| item.get("failure")["class"].as_str().map(str::to_owned))
}

/// `resumableRunId`: the last run, when Stop interrupted it or the
/// subscription's usage limit failed it.
pub(crate) fn resumable_run(detail: &Detail, last_error_class: Option<&str>) -> Option<String> {
    let run = latest_executed_run(&detail.runs)?;
    let usage_limit = run.status == "failed"
        && last_error_class == Some("usage_limit")
        && root_failure_class(run, detail).as_deref() == Some("usage_limit");
    (run.status == "interrupted" || usage_limit).then(|| run.id.clone())
}

/// The row's text: context references read as their labels, images as
/// their thumbnails.
fn preview_text(run: &QueuedRun, thumbnails: bool) -> String {
    crate::ui::prompt_history::replace_context_references(&run.text, |kind, label| {
        if kind == "image" && thumbnails {
            String::new()
        } else {
            label.to_owned()
        }
    })
    .trim()
    .to_owned()
}

/// The queued list's height, scrolled past `max-h-32`.
fn list_height(count: usize) -> f32 {
    (count as f32 * ROW + count.saturating_sub(1) as f32).min(LIST_MAX)
}

/// A `size="xs"` text button (`h-6 px-[7px] text-xs`), with an optional
/// 14px icon; disabled ones fade to 64% and take no clicks.
fn xs_button(
    ui: &Ui,
    rect: Rect,
    id: impl egui::AsIdSalt,
    icon: Option<&str>,
    label: &str,
    tint: Color32,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, ui.id().with(id), sense);
    let hovered = response.hovered() && enabled;
    if hovered {
        ui.painter().rect_filled(
            rect,
            widgets::radius(theme::CONTROL_RADIUS as f32),
            theme::accent(),
        );
    }
    let color = theme::alpha(
        if hovered { theme::foreground() } else { tint },
        if enabled { 1.0 } else { 0.64 },
    );
    let mut x = rect.left() + 7.0;
    if let Some(icon) = icon {
        theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(Pos2::new(x + 5.0, rect.center().y), Vec2::splat(14.0)),
            14.0,
            color,
        );
        x += 10.0 + 4.0;
    }
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
        label,
        font(12.0, Weight::Medium),
        color,
    );
    response
}

fn xs_button_width(ui: &Ui, icon: bool, label: &str) -> f32 {
    let text = widgets::text_width(ui, label, font(12.0, Weight::Medium));
    (14.0 + if icon { 14.0 } else { 0.0 } + text).ceil()
}

impl App {
    /// The open thread's queue.
    pub(crate) fn queue_workflow(&self) -> Option<Workflow> {
        Some(workflow(&*self.current_detail()?))
    }

    /// The banner's height, its overlap included; nothing without a queue.
    pub(crate) fn queue_banner_height(&self, workflow: Option<&Workflow>) -> f32 {
        match workflow {
            Some(w) if !w.queued.is_empty() => {
                let list = if self.queue.collapsed {
                    0.0
                } else {
                    list_height(w.queued.len())
                };
                4.0 + ROW + list + 4.0 + OVERLAP
            }
            _ => 0.0,
        }
    }

    /// The banner, its bottom `OVERLAP` tucked under whatever sits at
    /// `bottom` (the next banner or the composer surface).
    pub(crate) fn queue_banner(&mut self, ui: &mut Ui, surface: Rect, bottom: f32) {
        let Some(workflow) = self.queue_workflow() else {
            return;
        };
        let height = self.queue_banner_height(Some(&workflow));
        if height == 0.0 {
            return;
        }
        let thread_id = self.selected.clone().unwrap_or_default();
        let rect = Rect::from_min_max(
            Pos2::new(surface.left() + INSET, bottom + OVERLAP - height),
            Pos2::new(surface.right() - INSET, bottom + OVERLAP),
        );
        crate::ui::requests_panel::plan_banner_surface(ui, rect);
        let muted = theme::muted_foreground();
        let count = workflow.queued.len();

        // The header, a disclosure button: the icon, "Queued", the count and
        // the chevron.
        let header = Rect::from_min_size(
            Pos2::new(rect.left() + 4.0, rect.top() + 4.0),
            Vec2::new(rect.width() - 8.0, ROW),
        );
        let label = if self.queue.collapsed {
            "Expand queued messages"
        } else {
            "Collapse queued messages"
        };
        let response = ui
            .interact(header, ui.id().with("queued-header"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.clicked() {
            self.queue.collapsed = !self.queue.collapsed;
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        theme::paint_icon(
            ui,
            "list-ordered",
            Rect::from_center_size(
                Pos2::new(header.left() + ROW / 2.0, header.center().y),
                Vec2::splat(12.0),
            ),
            12.0,
            muted,
        );
        let chevron = Rect::from_min_size(
            Pos2::new(header.right() - ROW, header.top()),
            Vec2::splat(ROW),
        );
        theme::paint_icon(
            ui,
            if self.queue.collapsed {
                "chevron-up"
            } else {
                "chevron-down"
            },
            Rect::from_center_size(chevron.center(), Vec2::splat(14.0)),
            14.0,
            muted,
        );
        let count_text = count.to_string();
        let count_font = font(12.0, Weight::Medium);
        let count_width = widgets::tabular_width(ui, &count_text, count_font.clone());
        let count_box = count_width.max(ROW);
        widgets::tabular_text(
            ui,
            Pos2::new(
                chevron.left() - 4.0 - count_box + (count_box - count_width) / 2.0,
                header.top() + 4.0,
            ),
            &count_text,
            count_font,
            muted,
            16.0,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(header.left() + ROW + 4.0, header.top()),
                Pos2::new(chevron.left() - 8.0 - count_box, header.bottom()),
            ),
            "Queued",
            font(12.0, Weight::Regular),
            muted,
        );
        if self.queue.collapsed {
            return;
        }

        // The queued messages, scrolled past 8rem.
        let list = Rect::from_min_size(
            Pos2::new(header.left(), header.bottom()),
            Vec2::new(header.width(), list_height(count)),
        );
        let overflow = count as f32 * ROW + count.saturating_sub(1) as f32 > LIST_MAX;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        let mut actions = Actions::default();
        egui::ScrollArea::vertical()
            .id_salt(("queued-list", &thread_id))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let top = ui.max_rect().top();
                let width = list.width() - if overflow { 8.0 } else { 0.0 };
                for (index, run) in workflow.queued.iter().enumerate() {
                    let row = Rect::from_min_size(
                        Pos2::new(list.left(), top + index as f32 * (ROW + 1.0)),
                        Vec2::new(width, ROW),
                    );
                    self.queued_row(ui, row, index, run, &workflow, &mut actions);
                }
                ui.allocate_rect(
                    Rect::from_min_size(
                        ui.max_rect().min,
                        Vec2::new(width, count as f32 * (ROW + 1.0) - 1.0),
                    ),
                    Sense::hover(),
                );
            });
        self.run_queue_actions(ui, actions, &workflow, &thread_id);
    }

    fn queued_row(
        &mut self,
        ui: &mut Ui,
        row: Rect,
        index: usize,
        run: &QueuedRun,
        workflow: &Workflow,
        actions: &mut Actions,
    ) {
        let editing = self
            .queue
            .editing
            .as_ref()
            .is_some_and(|e| e.run_id == run.run_id);
        let dragged = self
            .queue
            .drag
            .as_ref()
            .is_some_and(|(id, _)| *id == run.run_id);
        let opacity = if dragged { 0.5 } else { 1.0 };
        if editing {
            ui.painter()
                .rect_filled(row, CornerRadius::same(4), theme::accent());
        }
        // While a grip drags, the pointer's half of a row picks the slot.
        if let Some((_, insert)) = &mut self.queue.drag
            && let Some(pointer) = ui.ctx().pointer_latest_pos()
            && pointer.y >= row.top()
            && pointer.y < row.bottom() + 1.0
        {
            *insert = Some(if pointer.y < row.center().y {
                index
            } else {
                index + 1
            });
        }
        if let Some((_, Some(insert))) = &self.queue.drag {
            let line = |y: f32| {
                Rect::from_min_max(Pos2::new(row.left(), y), Pos2::new(row.right(), y + 2.0))
            };
            let primary = theme::alpha(theme::primary(), 0.7);
            if *insert == index {
                ui.painter()
                    .rect_filled(line(row.top()), CornerRadius::same(1), primary);
            }
            if index + 1 == workflow.queued.len() && *insert == index + 1 {
                ui.painter()
                    .rect_filled(line(row.bottom() - 2.0), CornerRadius::same(1), primary);
            }
        }

        // The grip.
        if workflow.can_reorder {
            let grip = Rect::from_min_size(row.min, Vec2::splat(ROW));
            let response = ui
                .interact(
                    grip,
                    ui.id().with(("queued-grip", &run.run_id)),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(CursorIcon::Grab);
            let hovered = response.hovered() || response.dragged();
            if hovered {
                ui.painter().rect_filled(
                    grip,
                    widgets::radius(theme::CONTROL_RADIUS as f32),
                    theme::accent(),
                );
            }
            theme::paint_icon(
                ui,
                "grip-vertical",
                grip,
                14.0,
                theme::alpha(
                    if hovered {
                        theme::foreground()
                    } else {
                        theme::muted_foreground()
                    },
                    opacity,
                ),
            );
            if response.drag_started() {
                self.queue.drag = Some((run.run_id.clone(), None));
            }
            if response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            }
            if response.drag_stopped()
                && let Some((id, insert)) = self.queue.drag.take()
            {
                actions.drop = insert.map(|insert| (id, insert));
            }
            if !hovered {
                response.on_hover_text("Reorder queued message (drag)");
            }
        }

        // Actions, right to left.
        let mut right = row.right();
        let muted = theme::muted_foreground();
        if editing {
            let width = xs_button_width(ui, false, "Cancel");
            let cancel =
                Rect::from_min_size(Pos2::new(right - width, row.top()), Vec2::new(width, ROW));
            if xs_button(
                ui,
                cancel,
                ("queued-edit-cancel", &run.run_id),
                None,
                "Cancel",
                theme::foreground(),
                true,
            )
            .clicked()
            {
                actions.cancel_edit = true;
            }
            right = cancel.left() - 4.0;
        } else {
            let dismiss = Rect::from_min_size(Pos2::new(right - ROW, row.top()), Vec2::splat(ROW));
            if widgets::icon_button(
                ui,
                dismiss,
                ("queued-cancel", &run.run_id),
                "x",
                14.0,
                muted,
                "Remove from queue",
            )
            .clicked()
            {
                actions.remove = Some(run.run_id.clone());
            }
            let width = xs_button_width(ui, true, "Steer");
            let steer = Rect::from_min_size(
                Pos2::new(dismiss.left() - 4.0 - width, row.top()),
                Vec2::new(width, ROW),
            );
            let tip = if self
                .current_detail()
                .as_deref()
                .and_then(active_run)
                .is_none()
            {
                "There is no active run to steer".to_owned()
            } else {
                let shortcut = (index == 0)
                    .then(|| self.queue_shortcut("thread.steerQueuedMessage"))
                    .flatten();
                match shortcut {
                    Some(label) => format!("Send as a steer instead ({label})"),
                    None => "Send as a steer instead".to_owned(),
                }
            };
            if xs_button(
                ui,
                steer,
                ("queued-steer", &run.run_id),
                Some("corner-up-right"),
                "Steer",
                muted,
                workflow.can_steer,
            )
            .on_hover_text(tip)
            .clicked()
            {
                actions.steer = Some(run.run_id.clone());
            }
            let edit = Rect::from_min_size(
                Pos2::new(steer.left() - 4.0 - ROW, row.top()),
                Vec2::splat(ROW),
            );
            let shortcut = (index + 1 == workflow.queued.len())
                .then(|| self.queue_shortcut("thread.editQueuedMessage"))
                .flatten();
            let tip = match shortcut {
                Some(label) => format!("Edit in the composer ({label})"),
                None => "Edit in the composer".to_owned(),
            };
            if widgets::icon_button(
                ui,
                edit,
                ("queued-edit", &run.run_id),
                "pencil",
                14.0,
                muted,
                &tip,
            )
            .clicked()
            {
                actions.edit = Some(run.run_id.clone());
            }
            right = edit.left() - 4.0;
        }

        // The content: thumbnails, then the text.
        let mut x = row.left() + ROW + 4.0;
        let images: Vec<&Value> = run
            .attachments
            .iter()
            .filter(|a| a["type"] == "image")
            .collect();
        for attachment in &images {
            let thumb = Rect::from_min_size(Pos2::new(x, row.center().y - 8.0), Vec2::splat(16.0));
            let radius = CornerRadius::same(4);
            match self.attachment_image(attachment) {
                Some(image) => crate::ui::attachments::paint_cover(
                    ui,
                    thumb.shrink(1.0),
                    attachment["id"].as_str().unwrap_or_default(),
                    &image,
                    CornerRadius::same(3),
                ),
                None => {
                    ui.painter()
                        .rect_filled(thumb, radius, theme::alpha(theme::muted(), 0.6));
                }
            }
            ui.painter().rect_stroke(
                thumb,
                radius,
                egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
                egui::StrokeKind::Inside,
            );
            x = thumb.right() + 2.0;
        }
        if !images.is_empty() {
            x += 2.0;
        }
        let text = preview_text(run, !images.is_empty());
        let text_rect = Rect::from_min_max(Pos2::new(x, row.top()), Pos2::new(right, row.bottom()));
        widgets::text_in(
            ui,
            text_rect,
            &text,
            font(12.0, Weight::Regular),
            theme::alpha(theme::foreground(), 0.8 * opacity),
        );
        if !text.is_empty() {
            ui.interact(
                text_rect,
                ui.id().with(("queued-text", &run.run_id)),
                Sense::hover(),
            )
            .on_hover_ui(|ui| {
                ui.set_max_width(384.0);
                ui.label(&text);
            });
        }
    }

    fn run_queue_actions(
        &mut self,
        ui: &Ui,
        actions: Actions,
        workflow: &Workflow,
        thread_id: &str,
    ) {
        if let Some((run_id, insert)) = actions.drop {
            let dragged = workflow.queued.iter().position(|q| q.run_id == run_id);
            // Landing just before or after itself moves nothing.
            if let Some(dragged) = dragged
                && insert != dragged
                && insert != dragged + 1
            {
                let mut command = crate::model::command("queued-run.reorder", thread_id);
                command["runId"] = json!(run_id);
                command["beforeRunId"] = json!(workflow.queued.get(insert).map(|q| &q.run_id));
                self.dispatch(command);
            }
        }
        if let Some(run_id) = actions.steer {
            self.steer_queued(&run_id);
        }
        if let Some(run_id) = actions.remove {
            let mut command = crate::model::command("queued-run.cancel", thread_id);
            command["runId"] = json!(run_id);
            self.dispatch(command);
        }
        if let Some(run_id) = actions.edit
            && let Some(run) = workflow.queued.iter().find(|q| q.run_id == run_id)
        {
            self.begin_queued_edit(ui.ctx(), thread_id, run);
        }
        if actions.cancel_edit {
            self.end_queued_edit(ui.ctx());
        }
    }

    /// The label of a queue command's shortcut (`shortcutLabelForCommand`).
    fn queue_shortcut(&self, command: &str) -> Option<String> {
        let context: &[(&str, bool)] = if command == "thread.editQueuedMessage" {
            &[("composerFocus", true)]
        } else {
            &[]
        };
        crate::ui::shortcuts::effective_shortcut_label(
            &self.state.config["keybindings"],
            command,
            context,
        )
    }

    /// `queued-message.promote-to-steer` into the active run.
    fn steer_queued(&mut self, run_id: &str) {
        let Some(detail) = self.current_detail() else {
            return;
        };
        let Some(active) = active_run(&detail) else {
            return;
        };
        let mut command = crate::model::command("queued-message.promote-to-steer", detail.id());
        command["queuedRunId"] = json!(run_id);
        command["targetRunId"] = json!(active.id);
        self.dispatch(command);
    }

    /// The thread can resume (`canResume`): its last run stopped, or its
    /// queue is held.
    pub(crate) fn can_resume(&self) -> bool {
        let (Some(thread), Some(detail)) = (self.current_thread(), self.current_detail()) else {
            return false;
        };
        let class = thread
            .runtime
            .as_ref()
            .and_then(|r| r.last_error_class.as_deref());
        resumable_run(&detail, class).is_some() || workflow(&detail).held
    }

    /// `onResume`: a stopped run continues ("Continue where you left off."),
    /// then a held queue is released (`queue.resume`).
    pub(crate) fn resume_thread(&mut self) {
        let (Some(thread), Some(detail)) = (self.current_thread(), self.current_detail()) else {
            return;
        };
        let class = thread
            .runtime
            .as_ref()
            .and_then(|r| r.last_error_class.as_deref());
        let held = workflow(&detail).held;
        let resume = crate::model::command("queue.resume", &thread.id);
        let Some(run_id) = resumable_run(&detail, class) else {
            if held {
                self.dispatch(resume);
            }
            return;
        };
        let runtime = self.composer_runtime_mode(false);
        let interaction = self.composer_interaction_mode(false);
        let model = self.composer_model_json(false);
        let mut message = crate::ui::composer::message_dispatch(
            &thread.id,
            "Continue where you left off.",
            &model,
            "start",
        );
        message["manualContinuationOfRunId"] = json!(run_id);
        if held {
            let id = message["commandId"].as_str().unwrap_or_default().to_owned();
            self.queue.resume_after = Some((id, thread.id.clone()));
        }
        let before = self.mode_commands(&thread, &runtime, &interaction);
        self.start_turn(crate::ui::composer::TurnSend {
            thread_id: thread.id.clone(),
            before,
            message,
            launch: false,
            uploads: Vec::new(),
        });
    }

    /// The continuation a resume sent was received: its held queue goes on.
    pub(crate) fn resume_queue_after(&mut self, command_id: &str, ok: bool) {
        if self
            .queue
            .resume_after
            .as_ref()
            .is_some_and(|(id, _)| id == command_id)
            && let Some((_, thread_id)) = self.queue.resume_after.take()
            && ok
        {
            self.dispatch(crate::model::command("queue.resume", &thread_id));
        }
    }

    /// `thread.steerQueuedMessage`: the first queued message steers.
    pub(crate) fn steer_next_queued(&mut self) -> bool {
        let Some(workflow) = self.queue_workflow() else {
            return false;
        };
        let Some(next) = workflow.queued.first() else {
            return false;
        };
        if !workflow.can_steer {
            return false;
        }
        let run_id = next.run_id.clone();
        self.steer_queued(&run_id);
        true
    }

    /// `thread.editQueuedMessage`: the last queued message opens in the
    /// composer, unless one is already being edited.
    pub(crate) fn edit_latest_queued(&mut self, ctx: &egui::Context) -> bool {
        if self.queue.editing.is_some() {
            return false;
        }
        let Some(workflow) = self.queue_workflow() else {
            return false;
        };
        let Some(latest) = workflow.queued.last() else {
            return false;
        };
        let thread_id = self.selected.clone().unwrap_or_default();
        self.queue.collapsed = false;
        self.begin_queued_edit(ctx, &thread_id, latest);
        true
    }

    /// `beginEditingQueuedRun`: the message's text replaces the composer,
    /// whose draft waits until the edit ends.
    fn begin_queued_edit(&mut self, ctx: &egui::Context, thread_id: &str, run: &QueuedRun) {
        let saved = match self.queue.editing.take() {
            Some(previous) => previous.saved,
            None => (
                std::mem::take(&mut self.composer),
                std::mem::take(&mut self.composer_images),
                std::mem::take(&mut self.composer_files),
                std::mem::take(&mut self.composer_contexts),
            ),
        };
        self.composer = run.text.clone();
        self.composer_images.clear();
        self.composer_files.clear();
        self.composer_contexts.clear();
        self.queue.editing = Some(QueuedEdit {
            thread_id: thread_id.to_owned(),
            run_id: run.run_id.clone(),
            original: run.text.clone(),
            attachments: run.attachments.len(),
            saved,
        });
        self.place_composer_caret_at_end(ctx);
    }

    /// `cancelEditingQueuedRun`: the thread's draft comes back.
    pub(crate) fn end_queued_edit(&mut self, ctx: &egui::Context) {
        let Some(edit) = self.queue.editing.take() else {
            return;
        };
        (
            self.composer,
            self.composer_images,
            self.composer_files,
            self.composer_contexts,
        ) = edit.saved;
        self.place_composer_caret_at_end(ctx);
    }

    fn place_composer_caret_at_end(&self, ctx: &egui::Context) {
        let Some(id) = self.cite.editor_id else {
            return;
        };
        let end = crate::ui::cite::collapse(&self.composer)
            .display
            .chars()
            .count();
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(end),
            )));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// The queued message being edited, while its run is still queued.
    pub(crate) fn editing_queued(&self) -> bool {
        self.queue.editing.is_some()
    }

    /// Whether the edit has something to save: text, or the message's
    /// attachments.
    pub(crate) fn queued_edit_sendable(&self) -> bool {
        self.queue
            .editing
            .as_ref()
            .is_some_and(|e| !self.composer.trim().is_empty() || e.attachments > 0)
    }

    /// Saves the composer's text into the queued message (`queued-run.edit`)
    /// and brings the thread's draft back.
    pub(crate) fn save_queued_edit(&mut self, ctx: &egui::Context) {
        let Some(edit) = self.queue.editing.as_ref() else {
            return;
        };
        if !self.composer_images.is_empty() || !self.composer_files.is_empty() {
            self.toasts.status(
                "A queued message's attachments can't be changed here; remove the new ones to save."
                    .into(),
            );
            return;
        }
        let text = self.composer.trim();
        if text.is_empty() && edit.attachments == 0 {
            return;
        }
        let mut command = crate::model::command("queued-run.edit", &edit.thread_id);
        command["runId"] = json!(edit.run_id);
        command["text"] = json!(if text.is_empty() {
            ATTACHMENT_ONLY_PROMPT
        } else {
            text
        });
        self.dispatch(command);
        self.end_queued_edit(ctx);
    }

    /// Ends the edit once its run leaves the queue (it started, or another
    /// client cancelled it) or the thread changes. A changed edit moves into
    /// the thread's draft when that is empty, else it is dropped
    /// (`recoverQueuedMessageEdit`).
    pub(crate) fn check_queued_edit(&mut self, ctx: &egui::Context) {
        let Some(edit) = self.queue.editing.as_ref() else {
            return;
        };
        if self.selected.as_deref() != Some(edit.thread_id.as_str()) {
            self.end_queued_edit(ctx);
            return;
        }
        let Some(detail) = self.current_detail() else {
            return;
        };
        if detail
            .runs
            .iter()
            .any(|run| run.id == edit.run_id && run.status == "queued")
        {
            return;
        }
        let dirty = self.composer != edit.original
            || !self.composer_images.is_empty()
            || !self.composer_files.is_empty();
        let (text, images, files, _) = &edit.saved;
        let draft_empty = text.trim().is_empty() && images.is_empty() && files.is_empty();
        use crate::ui::toasts::{Kind, Toast};
        if dirty && draft_empty {
            self.queue.editing = None;
            self.toasts.add(
                Toast::new(Kind::Info, "Queued message is no longer queued")
                    .description("Your unsaved edit was kept in the composer."),
            );
            return;
        }
        self.end_queued_edit(ctx);
        if dirty {
            self.toasts.add(
                Toast::new(Kind::Warning, "Queued message is no longer queued")
                    .description("Your unsaved edit was discarded."),
            );
        }
    }
}

/// What the rows asked for this frame.
#[derive(Default)]
struct Actions {
    drop: Option<(String, usize)>,
    steer: Option<String>,
    remove: Option<String>,
    edit: Option<String>,
    cancel_edit: bool,
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
        let mut held = run("w", 4, None, "m3");
        held["queueHeld"] = json!(true);
        let detail: Detail = serde_json::from_value(json!({
            "thread": {"id": "t"},
            "runs": [run("a", 2, Some(2), "m1"), run("b", 3, Some(1), "m2"), held],
            "messages": [
                {"id": "m1", "text": "first", "attachments": []},
                {"id": "m2", "text": "second", "attachments": [{"type": "image", "id": "i"}]},
                {"id": "m3", "text": "wake", "attachments": [], "notification": {}},
            ],
        }))
        .unwrap();
        let workflow = workflow(&detail);
        let texts: Vec<&str> = workflow.queued.iter().map(|q| q.text.as_str()).collect();
        assert_eq!(texts, ["second", "first"]);
        assert_eq!(workflow.queued[0].attachments.len(), 1);
        assert!(workflow.held);
        assert!(!workflow.can_steer);
        assert!(!workflow.can_reorder);
    }

    #[test]
    fn previews_read_references_as_labels() {
        let run = QueuedRun {
            run_id: "r".into(),
            text: "see ![shot.png](t3-context://v1/image/a1) and [Terminal 1](t3-context://v1/terminal/t1)".into(),
            attachments: Vec::new(),
        };
        assert_eq!(preview_text(&run, true), "see  and Terminal 1");
        assert_eq!(preview_text(&run, false), "see shot.png and Terminal 1");
    }
}
