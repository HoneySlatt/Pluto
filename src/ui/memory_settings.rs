//! Settings → Personalization (Pluto, not upstream), after the providers'
//! own personalization settings: the custom instructions every chat gets,
//! and what Chat mode remembers about the user across chats.
//!
//! The instructions live in each chat space's `.pluto/instructions.json`
//! (the server's `chatInstructions.ts`), written to the temporary chat
//! space too. The memories live in the chat space's `.pluto/memory.json`
//! (`chatMemory.ts`); chats save and change them with the server's
//! `memory_*` tools, and this page lists them, forgets one or all of them,
//! and turns memory off. Read and written through `projects.readFile` and
//! `projects.writeFile`, so it works with a remote server too.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::settings::{
    Control, Row, RowAction, Section, card_border, card_fill, relative_time_label,
};
use crate::ui::thread_menu::Confirm;
use crate::ui::widgets;

/// Index of the Memory page: past `PAGES` and the Project page.
pub const MEMORY_PAGE: usize = 11;
const MEMORY_PATH: &str = ".pluto/memory.json";
const INSTRUCTIONS_PATH: &str = ".pluto/instructions.json";
/// The instruction fields: key, title, description.
const INSTRUCTION_FIELDS: [(&str, &str, &str); 2] = [
    (
        "about",
        "About you",
        "What chats should know about you: your name, your work, what you're into.",
    ),
    (
        "style",
        "How chats should respond",
        "Tone, length, format, language: how you like answers.",
    ),
];
/// "Delete all"'s confirmation command: handled here, never dispatched.
pub(crate) const CLEAR_COMMAND: &str = "pluto.memory.clear";

#[derive(Default)]
pub(crate) struct MemorySettings {
    /// The memory file: `None` while it loads, an error when it holds
    /// something else than memory JSON (the page then changes nothing).
    loaded: Option<Result<Value, String>>,
    /// The saved instructions (`about`, `style`), once read.
    instructions: Option<[String; 2]>,
    /// The instructions as edited.
    draft: [String; 2],
}

/// An instructions file's fields; empty when missing or unreadable.
fn parse_instructions(contents: &str) -> [String; 2] {
    let file = serde_json::from_str::<Value>(contents).unwrap_or_default();
    INSTRUCTION_FIELDS.map(|(key, ..)| file[key].as_str().unwrap_or_default().to_owned())
}

/// A memory file's contents, checked enough not to overwrite a stranger.
pub(crate) fn parse(contents: &str) -> Result<Value, String> {
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
    /// Reads the chat space's memories and instructions, once it exists.
    pub(crate) fn request_memory(&mut self) {
        self.memory.loaded = None;
        self.memory.instructions = None;
        let Some(root) = self.chats_project().map(|chats| chats.workspace_root) else {
            return;
        };
        self.send_request(
            "chat-memory:read".into(),
            "projects.readFile",
            json!({"cwd": root, "relativePath": MEMORY_PATH}),
        );
        self.send_request(
            "chat-instructions:read".into(),
            "projects.readFile",
            json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH}),
        );
    }

    /// Writes the edited instructions to every chat space.
    fn save_instructions(&mut self) {
        let [about, style] = self.memory.draft.clone().map(|text| text.trim().to_owned());
        let contents = format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({"version": 1, "about": about, "style": style}))
                .unwrap_or_default()
        );
        self.memory.instructions = Some([about, style]);
        self.write_instructions(&contents);
    }

    fn write_instructions(&mut self, contents: &str) {
        let roots: Vec<String> = self
            .state
            .projects
            .iter()
            .filter(|p| self.is_chats(p))
            .map(|p| p.workspace_root.clone())
            .collect();
        for root in roots {
            self.send_request(
                format!("chat-instructions:write:{root}"),
                "projects.writeFile",
                json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH, "contents": contents}),
            );
        }
    }

    /// Copies the chat space's instructions to another chat space (the
    /// temporary one, a project) whose profile was just written.
    pub(crate) fn share_instructions(&mut self, target: &str) {
        let Some(root) = self.chats_project().map(|chats| chats.workspace_root) else {
            return;
        };
        self.send_request(
            format!("chat-instructions:share:{target}"),
            "projects.readFile",
            json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH}),
        );
    }

    /// "Custom instructions": a card with the two instruction fields and
    /// Save, from `top`. Returns where the next section starts.
    pub(crate) fn instructions_card(
        &mut self,
        ui: &mut Ui,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        let Some(saved) = self.memory.instructions.clone() else {
            return top;
        };
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left + 16.0, top), Vec2::new(width, 28.0)),
            "Custom instructions",
            font(14.0, Weight::Regular),
            theme::alpha(theme::foreground(), 0.7),
        );
        let field_height = 4.0 * 20.0 + 16.0;
        let block = 20.0 + 18.0 + 8.0 + field_height;
        let height = 16.0 + 2.0 * (block + 16.0) + 32.0 + 16.0;
        let card = Rect::from_min_size(Pos2::new(left, top + 38.0), Vec2::new(width, height));
        ui.painter().rect(
            card,
            CornerRadius::same(14),
            card_fill(),
            Stroke::new(1.0, card_border()),
            StrokeKind::Inside,
        );
        let mut y = card.top() + 16.0;
        for (index, (key, title, description)) in INSTRUCTION_FIELDS.into_iter().enumerate() {
            let x = card.left() + 16.0;
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(width - 32.0, 20.0)),
                title,
                font(14.0, Weight::Medium),
                theme::foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(x, y + 20.0), Vec2::new(width - 32.0, 18.0)),
                description,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            let field = Rect::from_min_size(
                Pos2::new(x, y + 46.0),
                Vec2::new(width - 32.0, field_height),
            );
            ui.painter().rect(
                field,
                CornerRadius::same(8),
                theme::control_surface(),
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            let inner = field.shrink2(Vec2::new(10.0, 8.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
            child.set_clip_rect(inner.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical()
                .id_salt(("instructions", key))
                .max_height(inner.height())
                .auto_shrink([false, false])
                .show(&mut child, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.memory.draft[index])
                            .id_salt(("instructions-edit", key))
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::ZERO)
                            .font(font(14.0, Weight::Regular))
                            .text_color(theme::foreground())
                            .desired_width(inner.width())
                            .desired_rows(4),
                    );
                });
            y += block + 16.0;
        }
        // `Button size="sm"`, enabled once something changed.
        let changed = self
            .memory
            .draft
            .iter()
            .map(|t| t.trim())
            .ne(saved.iter().map(String::as_str));
        let label = "Save";
        let label_font = font(14.0, Weight::Medium);
        let button_width = widgets::text_width(ui, label, label_font.clone()).ceil() + 24.0;
        let button = Rect::from_min_size(
            Pos2::new(
                card.right() - 16.0 - button_width,
                card.bottom() - 16.0 - 32.0,
            ),
            Vec2::new(button_width, 32.0),
        );
        let response = ui.interact(button, ui.id().with("instructions-save"), Sense::click());
        let fill = if !changed {
            theme::alpha(theme::primary(), 0.5)
        } else if response.hovered() {
            theme::alpha(theme::primary(), 0.9)
        } else {
            theme::primary()
        };
        ui.painter()
            .rect_filled(button, CornerRadius::same(8), fill);
        widgets::text_in(
            ui,
            Rect::from_min_max(button.min + Vec2::new(12.0, 0.0), button.max),
            label,
            label_font,
            theme::primary_foreground(),
        );
        if changed && response.clicked() {
            self.save_instructions();
        }
        card.bottom() + 32.0
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
            // A missing file is no instructions yet.
            "chat-instructions:read" => {
                let fields = result
                    .as_ref()
                    .ok()
                    .map(|file| parse_instructions(file["contents"].as_str().unwrap_or_default()))
                    .unwrap_or_default();
                self.memory.draft = fields.clone();
                self.memory.instructions = Some(fields);
                true
            }
            key if key.starts_with("chat-instructions:share:") => {
                let root = &key["chat-instructions:share:".len()..];
                if let Ok(file) = result
                    && let Some(contents) = file["contents"].as_str()
                {
                    self.send_request(
                        format!("chat-instructions:write:{root}"),
                        "projects.writeFile",
                        json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH, "contents": contents}),
                    );
                }
                true
            }
            key if key.starts_with("chat-instructions:write:") => {
                if let Err(error) = result {
                    self.status = Some(format!("Could not save the custom instructions: {error}"));
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
    fn reads_instructions_or_none() {
        let fields = parse_instructions(r#"{"version":1,"about":"Nurse","style":""}"#);
        assert_eq!(fields, ["Nurse".to_owned(), String::new()]);
        assert_eq!(
            parse_instructions("not json"),
            [String::new(), String::new()]
        );
    }

    #[test]
    fn reads_only_memory_files() {
        let memory = parse(r#"{"version":1,"enabled":false,"memories":[]}"#).unwrap();
        assert_eq!(memory["enabled"], false);
        assert!(parse(r#"{"version":2,"memories":[]}"#).is_err());
        assert!(parse(r#"{"version":1}"#).is_err());
        assert!(parse("not json").is_err());
    }
}
