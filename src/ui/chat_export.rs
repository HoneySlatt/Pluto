//! Export a chat as Markdown (Pluto, not upstream), after LibreChat's
//! conversation export: the messages in order under "You" and "Assistant",
//! with the files given to the chat and the pages it published named
//! where they appeared. Tool calls and other work are left out. The file is
//! saved through the desktop's save dialog (`media::save_text_file`).

use crate::app::App;
use crate::model::{Detail, Thread};

/// The conversation as Markdown.
fn markdown(thread: &Thread, detail: &Detail, exported: &str) -> String {
    let mut out = format!(
        "# {}\n\n_Exported from Pluto on {exported} · {}_\n",
        thread.title.trim(),
        thread.model_selection.model
    );
    let mut push = |role: &str, body: &str| {
        out.push_str(&format!("\n---\n\n**{role}**\n\n{}\n", body.trim()));
    };
    for row in &detail.visible {
        let item = &row.item;
        match item.kind.as_str() {
            "user_message" => {
                let mut body = item.text().trim().to_owned();
                let files: Vec<&str> = item
                    .get("attachments")
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|file| file["name"].as_str())
                    .collect();
                if !files.is_empty() {
                    body.push_str(&format!("\n\n_Attached: {}_", files.join(", ")));
                }
                push("You", &body);
            }
            "assistant_message" if !item.text().trim().is_empty() => {
                push("Assistant", item.text());
            }
            _ => {
                if let Some(page) = crate::html_render::from_tool_item(item) {
                    push("Assistant", &format!("_Page: {}_", page.title));
                }
            }
        }
    }
    out
}

/// A file name for `title`: no path separators or control characters.
fn file_name(title: &str) -> String {
    let name: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim_matches(['.', ' ', '-']);
    format!("{}.md", if name.is_empty() { "Chat" } else { name })
}

impl App {
    /// Exports `thread_id`, opening it first when it is not the open chat:
    /// only the open thread's history is loaded.
    pub(crate) fn request_chat_export(&mut self, thread_id: &str) {
        if self.selected.as_deref() != Some(thread_id) {
            self.select_thread(Some(thread_id.to_owned()));
        }
        self.pending_chat_export = Some(thread_id.to_owned());
        self.poll_chat_export();
    }

    /// Saves the pending export once its thread's history has arrived.
    pub(crate) fn poll_chat_export(&mut self) {
        let Some(thread_id) = self.pending_chat_export.clone() else {
            return;
        };
        if self.selected.as_deref() != Some(thread_id.as_str()) {
            // The user moved on before the history arrived.
            self.pending_chat_export = None;
            return;
        }
        let (Some(thread), Some(detail)) = (self.current_thread(), self.current_detail()) else {
            return;
        };
        if detail.id() != thread_id {
            return;
        }
        let Some(ctx) = self.egui_ctx.clone() else {
            return;
        };
        self.pending_chat_export = None;
        let exported = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        crate::ui::media::save_text_file(
            file_name(&thread.title),
            markdown(&thread, &detail, &exported),
            &ctx,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::*;
    use crate::model::{TurnItem, VisibleItem};

    fn row(kind: &str, extra: Value) -> VisibleItem {
        let mut raw = json!({"id": kind, "threadId": "t", "runId": "r", "ordinal": 0,
            "status": "completed", "updatedAt": "2026-10-27T00:00:00Z", "type": kind});
        for (key, value) in extra.as_object().unwrap() {
            raw[key] = value.clone();
        }
        let item: TurnItem = serde_json::from_value(raw).unwrap();
        VisibleItem {
            item: Arc::new(item),
            ..Default::default()
        }
    }

    #[test]
    fn exports_the_messages_and_what_they_carried() {
        let thread: Thread = serde_json::from_value(json!({"id": "t", "projectId": "c",
            "title": "Crêpes", "createdAt": "", "updatedAt": "",
            "modelSelection": {"instanceId": "claudeAgent", "model": "claude-opus-5-5"}}))
        .unwrap();
        let detail = Detail {
            visible: vec![
                row(
                    "user_message",
                    json!({"messageId": "u", "text": "Une recette ?",
                        "attachments": [{"type": "image", "name": "frigo.png"}]}),
                ),
                row(
                    "command_execution",
                    json!({"input": "ls", "title": "Ran command"}),
                ),
                row(
                    "assistant_message",
                    json!({"messageId": "a", "text": "250 g de farine.\n"}),
                ),
            ],
            ..Default::default()
        };
        assert_eq!(
            markdown(&thread, &detail, "2026-10-27 22:00"),
            "# Crêpes\n\n_Exported from Pluto on 2026-10-27 22:00 · claude-opus-5-5_\n\
             \n---\n\n**You**\n\nUne recette ?\n\n_Attached: frigo.png_\n\
             \n---\n\n**Assistant**\n\n250 g de farine.\n"
        );
    }

    #[test]
    fn names_files_after_the_title() {
        assert_eq!(file_name("Plan: a/b?"), "Plan- a-b.md");
        assert_eq!(file_name("  "), "Chat.md");
    }
}
