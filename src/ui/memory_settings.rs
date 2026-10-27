//! Settings → Memory (Pluto, not upstream): what Chat mode remembers about
//! the user across chats, after the providers' own memory settings. The
//! memories live in the chat space's `.pluto/memory.json` (the server's
//! `chatMemory.ts`); chats save and change them with the server's
//! `memory_*` tools, and this page lists them, forgets one or all of them,
//! and turns memory off. Read and written through `projects.readFile` and
//! `projects.writeFile`, so it works with a remote server too.

use serde_json::{Value, json};

use crate::app::App;
use crate::ui::settings::{Control, Row, RowAction, Section, relative_time_label};
use crate::ui::thread_menu::Confirm;

/// Index of the Memory page: past `PAGES` and the Project page.
pub const MEMORY_PAGE: usize = 11;
const MEMORY_PATH: &str = ".pluto/memory.json";
/// "Delete all"'s confirmation command: handled here, never dispatched.
pub(crate) const CLEAR_COMMAND: &str = "pluto.memory.clear";

#[derive(Default)]
pub(crate) struct MemorySettings {
    /// The memory file: `None` while it loads, an error when it holds
    /// something else than memory JSON (the page then changes nothing).
    loaded: Option<Result<Value, String>>,
}

/// A memory file's contents, checked enough not to overwrite a stranger.
fn parse(contents: &str) -> Result<Value, String> {
    serde_json::from_str::<Value>(contents)
        .ok()
        .filter(|memory| memory["version"] == 1 && memory["memories"].is_array())
        .ok_or_else(|| format!("{MEMORY_PATH} in the chat space is not a memory file."))
}

fn note(id: &str, title: &str, description: &str) -> Row {
    Row {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        status: None,
        layers: false,
        reset: false,
        control: Control::None,
    }
}

fn section(title: &str, rows: Vec<Row>) -> Section {
    Section {
        title: title.into(),
        project: None,
        unavailable: None,
        rows,
    }
}

impl App {
    /// Reads the chat space's memories, once it exists.
    pub(crate) fn request_memory(&mut self) {
        self.memory.loaded = None;
        let Some(root) = self.chats_project().map(|chats| chats.workspace_root) else {
            return;
        };
        self.send_request(
            "chat-memory:read".into(),
            "projects.readFile",
            json!({"cwd": root, "relativePath": MEMORY_PATH}),
        );
    }

    pub(crate) fn memory_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        match key {
            "chat-memory:read" => {
                // The server reports a missing file like any read failure;
                // a chat space without one has saved nothing yet.
                self.memory.loaded = Some(match result {
                    Ok(file) => parse(file["contents"].as_str().unwrap_or_default()),
                    Err(_) => Ok(json!({"version": 1, "memories": []})),
                });
                true
            }
            "chat-memory:write" => {
                if let Err(error) = result {
                    self.status = Some(format!("Could not save the memories: {error}"));
                    self.request_memory();
                }
                true
            }
            _ => false,
        }
    }

    /// The page: the memory switch and Delete all, then the saved
    /// memories, newest first.
    pub(crate) fn memory_sections(&self) -> Vec<Section> {
        let memory = match &self.memory.loaded {
            None if self.chats_project().is_none() => {
                return vec![section(
                    "Memory",
                    vec![note(
                        "memory-status",
                        "No chats yet",
                        "Memory belongs to Chat mode: what your chats save about you will appear here.",
                    )],
                )];
            }
            None => {
                return vec![section(
                    "Memory",
                    vec![note(
                        "memory-status",
                        "Loading memories",
                        "Reading the chat space.",
                    )],
                )];
            }
            Some(Err(error)) => {
                return vec![section(
                    "Memory",
                    vec![note("memory-status", "Could not read the memories", error)],
                )];
            }
            Some(Ok(memory)) => memory,
        };
        let entries: Vec<&Value> = memory["memories"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let mut settings = vec![Row {
            id: "memory-enabled".into(),
            title: "Use memory".into(),
            description: "Chats remember what you share about yourself and use it in later \
                          conversations. You can also ask a chat to remember, correct or forget \
                          something."
                .into(),
            status: None,
            layers: false,
            reset: false,
            control: Control::Switch(memory["enabled"] != false),
        }];
        if !entries.is_empty() {
            settings.push(Row {
                id: "memory-clear".into(),
                title: "Delete all memories".into(),
                description: "Forget everything your chats saved.".into(),
                status: None,
                layers: false,
                reset: false,
                control: Control::Destructive {
                    label: "Delete all",
                    icon: "trash-2",
                },
            });
        }
        let saved = if entries.is_empty() {
            vec![note(
                "memory-none",
                "No saved memories",
                "Tell a chat to remember something, or share what it should know about you.",
            )]
        } else {
            entries
                .iter()
                .rev()
                .map(|entry| {
                    let created = entry["createdAt"].as_str().unwrap_or_default();
                    let updated = entry["updatedAt"].as_str().unwrap_or(created);
                    let verb = if updated == created {
                        "Saved"
                    } else {
                        "Updated"
                    };
                    Row {
                        id: format!("memory:{}", entry["id"].as_str().unwrap_or_default()),
                        title: format!("{verb} {}", relative_time_label(updated, self.now)),
                        description: entry["text"].as_str().unwrap_or_default().to_owned(),
                        status: None,
                        layers: false,
                        reset: false,
                        control: Control::OutlineXs {
                            label: "Delete",
                            icon: "trash-2",
                        },
                    }
                })
                .collect()
        };
        vec![
            section("Memory", settings),
            section("Saved memories", saved),
        ]
    }

    pub(crate) fn apply_memory(&mut self, id: &str, action: RowAction) {
        match action {
            RowAction::Toggle(on) if id == "memory-enabled" => {
                self.change_memory(|memory| memory["enabled"] = json!(on));
            }
            RowAction::Click if id == "memory-clear" => {
                self.confirm = Some(Confirm {
                    title: "Delete all memories?".into(),
                    message: "Your chats will forget everything they saved about you. This \
                              cannot be undone."
                        .into(),
                    confirm_label: "Delete all",
                    command: json!({"type": CLEAR_COMMAND}),
                    more: Vec::new(),
                });
            }
            RowAction::Click => {
                if let Some(memory_id) = id.strip_prefix("memory:") {
                    self.change_memory(|memory| {
                        if let Some(entries) = memory["memories"].as_array_mut() {
                            entries.retain(|entry| entry["id"] != memory_id);
                        }
                    });
                }
            }
            _ => {}
        }
    }

    /// "Delete all", once confirmed.
    pub(crate) fn clear_memories(&mut self) {
        self.change_memory(|memory| memory["memories"] = json!([]));
    }

    /// Changes the loaded memories and writes them back.
    fn change_memory(&mut self, change: impl FnOnce(&mut Value)) {
        let Some(root) = self.chats_project().map(|chats| chats.workspace_root) else {
            return;
        };
        let Some(Ok(memory)) = &mut self.memory.loaded else {
            return;
        };
        change(memory);
        let contents = format!(
            "{}\n",
            serde_json::to_string_pretty(memory).unwrap_or_default()
        );
        self.send_request(
            "chat-memory:write".into(),
            "projects.writeFile",
            json!({"cwd": root, "relativePath": MEMORY_PATH, "contents": contents}),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_memory_files() {
        let memory = parse(r#"{"version":1,"enabled":false,"memories":[]}"#).unwrap();
        assert_eq!(memory["enabled"], false);
        assert!(parse(r#"{"version":2,"memories":[]}"#).is_err());
        assert!(parse(r#"{"version":1}"#).is_err());
        assert!(parse("not json").is_err());
    }
}
