//! Chat versions (Pluto, not upstream), after ChatGPT: editing a message or
//! regenerating a response keeps the conversation as it was and continues a
//! new version of it, and "‹ 2/3 ›" under a message switches between the
//! versions that branch there. The chat sidebar lists one entry per
//! conversation.
//!
//! A version is a thread. Past the first message it is a `thread.fork` of
//! the thread that owns the run before the message, at that run: the server
//! shows the shared history and hands the agent its context. A first
//! message has no run to fork from, so its versions are new chats linked to
//! the first one in the chat space's `.pluto/versions.json`.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, CornerRadius, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Detail, Thread};
use crate::theme::{self, Weight, font};
use crate::timeline::Message;
use crate::ui::composer::{TurnSend, launch_input, message_dispatch};
use crate::ui::widgets;

const VERSIONS_PATH: &str = ".pluto/versions.json";

#[derive(Default)]
pub(crate) struct ChatVersions {
    /// The user message being edited, and its text.
    pub(crate) editing: Option<(String, String)>,
    /// The editor takes the focus when it opens.
    pub(crate) focus_editor: bool,
    /// First-message versions: thread → the conversation's first chat.
    /// `None` until read.
    roots: Option<HashMap<String, String>>,
    reading: bool,
}

/// Where versions of a message branch off.
#[derive(Clone, Debug, PartialEq)]
enum BranchPoint {
    /// Before the first message of the conversation started by this chat.
    Start(String),
    /// After `run`, owned by `thread`.
    After { thread: String, run: String },
}

/// The `messageId` of a visible user message, with its row index.
fn user_message_index(detail: &Detail, message_id: &str) -> Option<usize> {
    detail.visible.iter().position(|row| {
        row.item.kind == "user_message" && row.item.str("messageId") == Some(message_id)
    })
}

/// The thread a visible row belongs to: its source for inherited rows.
fn owner(detail: &Detail, index: usize) -> String {
    let row = &detail.visible[index];
    if row.source_thread_id.is_empty() {
        detail.id().to_owned()
    } else {
        row.source_thread_id.clone()
    }
}

/// The run before the message at `index`, with its owner.
fn run_before(detail: &Detail, index: usize) -> Option<(String, String)> {
    let run = detail.visible[index].item.run_id.as_deref();
    detail.visible[..index]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(at, row)| {
            let other = row.item.run_id.as_deref()?;
            (Some(other) != run).then(|| (owner(detail, at), other.to_owned()))
        })
}

impl App {
    fn thread_index(&self) -> HashMap<&str, &Arc<Thread>> {
        self.state
            .threads
            .iter()
            .filter(|t| t.deleted_at.is_none())
            .map(|t| (t.id.as_str(), t))
            .collect()
    }

    /// The thread `thread` is a version of, if any.
    fn version_parent(&self, thread: &Thread) -> Option<String> {
        match &thread.forked_from {
            Some(from) if from["type"] == "run" => from["threadId"].as_str().map(str::to_owned),
            _ => self.chat_versions.roots.as_ref()?.get(&thread.id).cloned(),
        }
    }

    /// The first chat of the conversation `thread_id` is a version of.
    fn version_root_in(&self, threads: &HashMap<&str, &Arc<Thread>>, thread_id: &str) -> String {
        let mut id = thread_id.to_owned();
        // Forks never loop; the bound only guards against bad data.
        for _ in 0..256 {
            match threads
                .get(id.as_str())
                .and_then(|t| self.version_parent(t))
            {
                Some(parent) if threads.contains_key(parent.as_str()) => id = parent,
                _ => break,
            }
        }
        id
    }

    pub(crate) fn version_root(&self, thread_id: &str) -> String {
        self.version_root_in(&self.thread_index(), thread_id)
    }

    /// Every version of `thread_id`'s conversation, itself included; a
    /// thread that is not a chat stands alone.
    pub(crate) fn chat_family(&self, thread_id: &str) -> Vec<String> {
        let threads = self.thread_index();
        let is_chat = threads
            .get(thread_id)
            .is_some_and(|t| self.is_chat_thread(t));
        if !is_chat {
            return vec![thread_id.to_owned()];
        }
        let root = self.version_root_in(&threads, thread_id);
        threads
            .values()
            .filter(|t| self.is_chat_thread(t) && self.version_root_in(&threads, &t.id) == root)
            .map(|t| t.id.clone())
            .collect()
    }

    /// One thread per conversation among `chats`: the open one, else the
    /// one used last.
    pub(crate) fn chat_conversations(&self, chats: Vec<Arc<Thread>>) -> Vec<Arc<Thread>> {
        let threads = self.thread_index();
        let last = |t: &Thread| {
            t.latest_user_message_at
                .clone()
                .unwrap_or_else(|| t.updated_at.clone())
        };
        let mut shown: HashMap<String, Arc<Thread>> = HashMap::new();
        for thread in chats {
            let root = self.version_root_in(&threads, &thread.id);
            let keep = match shown.get(&root) {
                None => true,
                Some(kept) => {
                    let open = |t: &Thread| self.selected.as_deref() == Some(t.id.as_str());
                    !open(kept) && (open(&thread) || last(&thread) > last(kept))
                }
            };
            if keep {
                shown.insert(root, thread);
            }
        }
        shown.into_values().collect()
    }

    /// Where the open chat's message `message_id` can branch, and the
    /// version it shows there.
    fn branch_point(&self, detail: &Detail, message_id: &str) -> Option<(BranchPoint, String)> {
        let index = user_message_index(detail, message_id)?;
        let shown = owner(detail, index);
        let point = match run_before(detail, index) {
            Some((thread, run)) => BranchPoint::After { thread, run },
            None => BranchPoint::Start(self.version_root(&shown)),
        };
        Some((point, shown))
    }

    /// The versions that branch at `point`, oldest first.
    fn versions_at(&self, point: &BranchPoint, shown: &str) -> Vec<String> {
        let mut versions: Vec<&Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| t.deleted_at.is_none() && self.is_chat_thread(t))
            .filter(|t| match point {
                BranchPoint::Start(root) => {
                    &t.id == root
                        || self
                            .chat_versions
                            .roots
                            .as_ref()
                            .is_some_and(|roots| roots.get(&t.id) == Some(root))
                }
                BranchPoint::After { thread, run } => {
                    let forked = t.forked_from.as_ref().is_some_and(|from| {
                        from["type"] == "run"
                            && from["threadId"] == *thread
                            && from["runId"] == *run
                    });
                    // The thread the run belongs to is a version when it
                    // went on after it.
                    let original = &t.id == thread
                        && (t.id == shown || t.latest_run_id.as_deref() != Some(run.as_str()));
                    forked || original
                }
            })
            .collect();
        versions.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        versions.into_iter().map(|t| t.id.clone()).collect()
    }

    /// The versions of the open chat's message `message_id` and the one
    /// shown, when there are several.
    pub(crate) fn message_versions(&self, message_id: &str) -> Option<(Vec<String>, usize)> {
        if !self
            .current_thread()
            .is_some_and(|t| self.is_chat_thread(&t))
        {
            return None;
        }
        let detail = self.current_detail()?;
        let (point, shown) = self.branch_point(&detail, message_id)?;
        let versions = self.versions_at(&point, &shown);
        let at = versions.iter().position(|id| *id == shown)?;
        (versions.len() > 1).then_some((versions, at))
    }

    /// Whether the open chat's message `message_id` can be edited or
    /// regenerated now.
    pub(crate) fn can_branch(&self, message_id: &str) -> bool {
        let Some(thread) = self.current_thread() else {
            return false;
        };
        // A message sent into a running answer is part of that turn.
        let starts_turn = |detail: &Detail| {
            user_message_index(detail, message_id).is_some_and(|index| {
                !matches!(
                    detail.visible[index].item.str("inputIntent"),
                    Some("steer" | "promoted_queued_to_steer")
                )
            })
        };
        !thread.is_running()
            && self.is_chat_thread(&thread)
            && self
                .current_detail()
                .is_some_and(|detail| starts_turn(&detail))
    }

    /// The user message that started the open chat's last answer, if that
    /// answer is `message_id`.
    pub(crate) fn regenerable_prompt(&self, message_id: &str) -> Option<(String, String)> {
        let detail = self.current_detail()?;
        let last = detail
            .visible
            .iter()
            .rev()
            .find(|row| row.item.kind == "assistant_message")?;
        if last.item.str("messageId") != Some(message_id) {
            return None;
        }
        let run = last.item.run_id.as_deref()?;
        detail
            .visible
            .iter()
            .find(|row| row.item.kind == "user_message" && row.item.run_id.as_deref() == Some(run))
            .map(|row| {
                (
                    row.item.str("messageId").unwrap_or_default().to_owned(),
                    row.item.text().to_owned(),
                )
            })
    }

    /// Continues a new version of the open chat with `text` in place of
    /// the user message `message_id`, and opens it.
    pub(crate) fn branch_message(&mut self, message_id: &str, text: &str) {
        let text = text.trim();
        let (Some(thread), Some(detail)) = (self.current_thread(), self.current_detail()) else {
            return;
        };
        if text.is_empty() || thread.is_running() {
            return;
        }
        let Some((point, _)) = self.branch_point(&detail, message_id) else {
            return;
        };
        let model = self.composer_model_json(false);
        let version = uuid::Uuid::new_v4().to_string();
        let send = match point {
            BranchPoint::After { thread: owner, run } => TurnSend {
                thread_id: version.clone(),
                before: vec![json!({
                    "type": "thread.fork",
                    "commandId": uuid::Uuid::new_v4().to_string(),
                    "createdBy": "user",
                    "creationSource": "web",
                    "sourceThreadId": owner,
                    "targetThreadId": version,
                    "sourcePoint": {"type": "run", "runId": run},
                    "title": thread.title,
                })],
                message: message_dispatch(&version, text, &model, "auto"),
                launch: false,
                uploads: Vec::new(),
            },
            BranchPoint::Start(root) => {
                // Writing before the file is read would lose its links.
                let Some(roots) = &mut self.chat_versions.roots else {
                    self.status = Some("Chat versions are still loading; try again".into());
                    return;
                };
                roots.insert(version.clone(), root);
                self.write_versions(&version);
                let mut message = launch_input(
                    &version,
                    &thread.project_id,
                    &thread.title,
                    text,
                    &model,
                    &self.composer_runtime_mode(false),
                    &thread.interaction_mode,
                );
                // The conversation keeps its title.
                message["generateTitle"] = json!(false);
                TurnSend {
                    thread_id: version.clone(),
                    before: Vec::new(),
                    message,
                    launch: true,
                    uploads: Vec::new(),
                }
            }
        };
        self.chat_versions.editing = None;
        self.start_turn(send);
        self.select_thread(Some(version));
    }

    /// Reads the first-message versions once the chat space exists.
    pub(crate) fn poll_chat_versions(&mut self) {
        if self.chat_versions.roots.is_some() || self.chat_versions.reading || !self.connected() {
            return;
        }
        let Some(chats) = self.chats_project() else {
            return;
        };
        self.chat_versions.reading = true;
        self.send_request(
            "chat-versions:read".into(),
            "projects.readFile",
            json!({"cwd": chats.workspace_root, "relativePath": VERSIONS_PATH}),
        );
    }

    /// Writes the first-message versions of live threads and `fresh`, the
    /// version just made, which the shell does not list yet.
    fn write_versions(&mut self, fresh: &str) {
        let Some(chats) = self.chats_project() else {
            return;
        };
        let live = self.thread_index();
        let roots: serde_json::Map<String, Value> = self
            .chat_versions
            .roots
            .iter()
            .flatten()
            .filter(|(id, _)| live.contains_key(id.as_str()) || id.as_str() == fresh)
            .map(|(id, root)| (id.clone(), json!(root)))
            .collect();
        let contents = format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({"version": 1, "roots": roots}))
                .unwrap_or_default()
        );
        self.send_request(
            "chat-versions:write".into(),
            "projects.writeFile",
            json!({"cwd": chats.workspace_root, "relativePath": VERSIONS_PATH, "contents": contents}),
        );
    }

    pub(crate) fn chat_versions_result(
        &mut self,
        key: &str,
        result: &Result<Value, String>,
    ) -> bool {
        match key {
            "chat-versions:read" => {
                self.chat_versions.reading = false;
                // A chat space without the file has no first-message versions.
                let roots = result
                    .as_ref()
                    .ok()
                    .and_then(|file| serde_json::from_str::<Value>(file["contents"].as_str()?).ok())
                    .map(|file| parse_roots(&file))
                    .unwrap_or_default();
                self.chat_versions.roots = Some(roots);
                true
            }
            "chat-versions:write" => {
                if let Err(error) = result {
                    self.status = Some(format!("Could not save the chat versions: {error}"));
                }
                true
            }
            _ => false,
        }
    }
}

impl App {
    /// "‹ 2/3 ›" ending at `right`: the versions of a message, the arrows
    /// opening the one before or after. Returns its left edge.
    pub(crate) fn version_switcher(
        &mut self,
        ui: &mut Ui,
        right: f32,
        top: f32,
        message_id: &str,
        versions: &[String],
        at: usize,
    ) -> f32 {
        let label = format!("{}/{}", at + 1, versions.len());
        let label_font = font(12.0, Weight::Medium);
        let label_width = widgets::text_width(ui, &label, label_font.clone()).ceil();
        let next = Rect::from_min_size(Pos2::new(right - 24.0, top), Vec2::splat(24.0));
        let text = Rect::from_min_size(
            Pos2::new(next.left() - label_width - 2.0, top),
            Vec2::new(label_width + 2.0, 24.0),
        );
        let previous = Rect::from_min_size(Pos2::new(text.left() - 24.0, top), Vec2::splat(24.0));
        widgets::text_in(ui, text, &label, label_font, theme::muted_foreground());
        let mut open = None;
        for (rect, icon, tip, target) in [
            (
                previous,
                "chevron-left",
                "Previous version",
                at.checked_sub(1),
            ),
            (next, "chevron-right", "Next version", Some(at + 1)),
        ] {
            let target = target.filter(|i| *i < versions.len());
            let color = if target.is_some() {
                theme::muted_foreground()
            } else {
                theme::alpha(theme::muted_foreground(), 0.4)
            };
            let clicked = widgets::icon_button(
                ui,
                rect,
                ("version", message_id, icon),
                icon,
                14.0,
                color,
                tip,
            )
            .clicked();
            if clicked && let Some(target) = target {
                open = Some(versions[target].clone());
            }
        }
        if let Some(id) = open {
            self.chat_versions.editing = None;
            self.select_thread(Some(id));
        }
        previous.left()
    }

    /// A chat message being edited: its text in an editor, Cancel, and
    /// Send, which continues a new version with it.
    pub(crate) fn edit_user_row(&mut self, ui: &mut Ui, message: &Message, width: f32) {
        let Some((id, mut text)) = self.chat_versions.editing.clone() else {
            return;
        };
        let body = font(14.0, Weight::Regular);
        let inner = width - 32.0;
        let measured = widgets::galley(
            ui,
            if text.is_empty() { " " } else { &text },
            body.clone(),
            theme::foreground(),
            22.0,
            inner,
        )
        .size()
        .y;
        let text_height = measured.clamp(22.0, 320.0);
        let height = 14.0 + text_height + 14.0 + 32.0 + 12.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height + 16.0), Sense::hover());
        let frame = Rect::from_min_size(rect.min, Vec2::new(width, height));
        ui.painter()
            .rect_filled(frame, CornerRadius::same(18), theme::message_surface());

        let edit_id = ui.id().with(("edit-message", &message.id));
        let focused = ui.memory(|m| m.has_focus(edit_id));
        let submit = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        let cancel = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        let edit_rect = Rect::from_min_size(
            frame.min + Vec2::new(16.0, 14.0),
            Vec2::new(inner, text_height),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect));
        child.set_clip_rect(edit_rect.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt(("edit-message-scroll", &message.id))
            .max_height(text_height)
            .auto_shrink([false, true])
            .show(&mut child, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .id(edit_id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(body)
                        .text_color(theme::foreground())
                        .desired_width(inner)
                        .desired_rows(1),
                );
                if std::mem::take(&mut self.chat_versions.focus_editor) {
                    response.request_focus();
                }
            });

        // `Button size="sm"`: Send (primary), then Cancel (outline) left of it.
        let button_font = font(14.0, Weight::Medium);
        let mut right = frame.right() - 12.0;
        let mut clicked = |ui: &mut Ui, label: &str, primary: bool| {
            let label_width = widgets::text_width(ui, label, button_font.clone()).ceil();
            let button = Rect::from_min_size(
                Pos2::new(right - label_width - 24.0, frame.bottom() - 12.0 - 32.0),
                Vec2::new(label_width + 24.0, 32.0),
            );
            right = button.left() - 8.0;
            let response =
                ui.interact(button, ui.id().with(("edit-button", label)), Sense::click());
            let (fill, text_color) = if primary {
                let fill = if response.hovered() {
                    theme::alpha(theme::primary(), 0.9)
                } else {
                    theme::primary()
                };
                (fill, theme::primary_foreground())
            } else {
                let fill = if response.hovered() {
                    theme::overlay(0.06)
                } else {
                    theme::control_surface()
                };
                (fill, theme::foreground())
            };
            let stroke = if primary {
                Stroke::NONE
            } else {
                Stroke::new(1.0, theme::input())
            };
            ui.painter().rect(
                button,
                CornerRadius::same(8),
                fill,
                stroke,
                StrokeKind::Inside,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(button.min + Vec2::new(12.0, 0.0), button.max),
                label,
                button_font.clone(),
                text_color,
            );
            response.clicked()
        };
        let send = clicked(ui, "Send", true) || submit;
        let dismissed = clicked(ui, "Cancel", false) || cancel;
        if send && !text.trim().is_empty() {
            self.branch_message(&id, &text);
        } else if dismissed {
            self.chat_versions.editing = None;
        } else {
            self.chat_versions.editing = Some((id, text));
        }
    }
}

fn parse_roots(file: &Value) -> HashMap<String, String> {
    file["roots"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, root)| Some((id.clone(), root.as_str()?.to_owned())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TurnItem, VisibleItem};

    fn row(kind: &str, message: &str, run: &str, source: &str) -> VisibleItem {
        let item: TurnItem = serde_json::from_value(json!({
            "id": format!("{kind}-{message}"), "threadId": source, "runId": run, "ordinal": 0,
            "status": "completed", "updatedAt": "2026-10-27T00:00:00Z", "type": kind,
            "messageId": message, "text": message,
        }))
        .unwrap();
        VisibleItem {
            visibility: "local".into(),
            source_thread_id: source.into(),
            source_item_id: item.id.clone(),
            item: Arc::new(item),
            ..Default::default()
        }
    }

    #[test]
    fn a_message_branches_after_the_run_before_it() {
        let detail = Detail {
            thread: json!({"id": "fork"}),
            visible: vec![
                row("user_message", "u1", "r1", "chat"),
                row("assistant_message", "a1", "r1", "chat"),
                row("user_message", "u2", "r2", "fork"),
                row("assistant_message", "a2", "r2", "fork"),
            ],
            ..Default::default()
        };
        assert_eq!(run_before(&detail, 0), None);
        assert_eq!(
            run_before(&detail, 2),
            Some(("chat".to_owned(), "r1".to_owned()))
        );
        assert_eq!(owner(&detail, 2), "fork");
        assert_eq!(user_message_index(&detail, "u2"), Some(2));
        assert_eq!(user_message_index(&detail, "a2"), None);
    }

    #[test]
    fn reads_first_message_versions() {
        let roots = parse_roots(&json!({"version": 1, "roots": {"b": "a", "c": 3}}));
        assert_eq!(roots.len(), 1);
        assert_eq!(roots["b"], "a");
    }
}
