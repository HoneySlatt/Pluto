//! Personalization (`#memory`, after the desktop's `memory_settings.rs`):
//! the custom instructions every chat gets ("About you", "How chats should
//! respond", in each chat space's `.pluto/instructions.json`, the temporary
//! one included), and what Chat remembers about the user (the chat space's
//! `.pluto/memory.json`): Use memory, Learn from chats, Add a memory, Delete
//! all memories, then the saved memories with Edit and Forget. Read and
//! written with `projects.readFile` and `projects.writeFile`.

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::icons::Icon;
use crate::nav::{Sheet, Ui};
use crate::pages::PageHead;
use pluto_core::spaces::Space;

const MEMORY_PATH: &str = ".pluto/memory.json";
const INSTRUCTIONS_PATH: &str = ".pluto/instructions.json";
/// `INSTRUCTION_FIELDS`: key, title, description.
const FIELDS: [(&str, &str, &str); 2] = [
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

/// The page's state: the memory file, the saved and edited instructions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Personalization {
    /// `None` while it loads; an error when the file is not memory JSON
    /// (the page then changes nothing).
    pub memory: Option<Result<Value, String>>,
    pub saved: Option<[String; 2]>,
    pub draft: [String; 2],
    /// Cancel remounts the fields with the saved text.
    pub generation: u64,
}

/// An instructions file's fields; empty when missing or unreadable.
fn parse_instructions(contents: &str) -> [String; 2] {
    let file = serde_json::from_str::<Value>(contents).unwrap_or_default();
    FIELDS.map(|(key, ..)| file[key].as_str().unwrap_or_default().to_owned())
}

/// A memory file's contents, checked enough not to overwrite a stranger.
fn parse(contents: &str) -> Result<Value, String> {
    serde_json::from_str::<Value>(contents)
        .ok()
        .filter(|memory| memory["version"] == 1 && memory["memories"].is_array())
        .ok_or_else(|| format!("{MEMORY_PATH} in the chat space is not a memory file."))
}

/// `relative_time_label`.
fn relative(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(date) = pluto_core::timeline::parse_time(iso) else {
        return String::new();
    };
    let seconds = (now - date).num_seconds();
    match seconds {
        ..60 => "just now".into(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86_400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

fn chats_root(ui: &Ui) -> Option<String> {
    ui.ctx
        .session
        .peek()
        .space_project(Space::Chats)
        .map(|p| p.workspace_root.clone())
}

/// Reads the chat space's memories and instructions.
pub fn load(ui: Ui) {
    let mut drafts = ui.drafts;
    drafts.write().personalization = Personalization::default();
    let Some(root) = chats_root(&ui) else {
        return;
    };
    ui.ctx.request(
        "chat-memory:read",
        "projects.readFile",
        json!({"cwd": root, "relativePath": MEMORY_PATH}),
    );
    ui.ctx.request(
        "chat-instructions:read",
        "projects.readFile",
        json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH}),
    );
}

/// Changes the loaded memories and writes them back.
fn change_memory(ui: Ui, change: impl FnOnce(&mut Value)) {
    let Some(root) = chats_root(&ui) else {
        return;
    };
    let mut drafts = ui.drafts;
    let contents = {
        let mut drafts = drafts.write();
        let Some(Ok(memory)) = drafts.personalization.memory.as_mut() else {
            return;
        };
        change(memory);
        format!(
            "{}\n",
            serde_json::to_string_pretty(memory).unwrap_or_default()
        )
    };
    ui.ctx.request(
        "mutation:chat-memory",
        "projects.writeFile",
        json!({"cwd": root, "relativePath": MEMORY_PATH, "contents": contents}),
    );
}

/// Saves a memory's new text (`Some`), adds one (empty `id`), or forgets
/// it (`None`).
pub fn edit_memory(ui: Ui, id: &str, text: Option<&str>) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    change_memory(ui, |memory| {
        let Some(entries) = memory["memories"].as_array_mut() else {
            return;
        };
        match text {
            Some(text) if id.is_empty() => {
                let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
                entries.push(json!({"id": id, "text": text, "createdAt": now, "updatedAt": now}));
            }
            Some(text) => {
                for entry in entries.iter_mut().filter(|e| e["id"] == id) {
                    entry["text"] = json!(text);
                    entry["updatedAt"] = json!(now);
                }
            }
            None => entries.retain(|entry| entry["id"] != id),
        }
    });
}

pub fn clear_memories(ui: Ui) {
    change_memory(ui, |memory| memory["memories"] = json!([]));
}

/// Writes the edited instructions to every chat space.
fn save_instructions(ui: Ui) {
    let mut drafts = ui.drafts;
    let [about, style] = drafts
        .peek()
        .personalization
        .draft
        .clone()
        .map(|t| t.trim().to_owned());
    let contents = format!(
        "{}\n",
        serde_json::to_string_pretty(&json!({"version": 1, "about": about, "style": style}))
            .unwrap_or_default()
    );
    {
        let mut drafts = drafts.write();
        drafts.personalization.saved = Some([about.clone(), style.clone()]);
        drafts.personalization.draft = [about, style];
    }
    let session = ui.ctx.session.peek();
    let roots: Vec<String> = session
        .state
        .projects
        .iter()
        .filter(|p| session.project_space(p).is_chat())
        .map(|p| p.workspace_root.clone())
        .collect();
    drop(session);
    for root in roots {
        ui.ctx.request(
            format!("mutation:instructions:{root}"),
            "projects.writeFile",
            json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH, "contents": contents}),
        );
    }
    ui.ctx.toast("Instructions saved");
}

#[component]
pub fn Memory() -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    use_hook(move || load(ui));
    // The files' replies fill the page once.
    use_effect(move || {
        let session = ui.ctx.session.read();
        let memory = session.replies.get("chat-memory:read").cloned();
        let instructions = session.replies.get("chat-instructions:read").cloned();
        drop(session);
        let waiting = {
            let state = &drafts.peek().personalization;
            (state.memory.is_none() && memory.is_some())
                || (state.saved.is_none() && instructions.is_some())
        };
        if !waiting {
            return;
        }
        let mut drafts = drafts.write();
        let state = &mut drafts.personalization;
        if state.memory.is_none()
            && let Some(result) = memory
        {
            // The server reports a missing file like any read failure: a
            // chat space without one has saved nothing yet.
            state.memory = Some(match result {
                Ok(file) => parse(file["contents"].as_str().unwrap_or_default()),
                Err(_) => Ok(json!({"version": 1, "memories": []})),
            });
        }
        if state.saved.is_none()
            && let Some(result) = instructions
        {
            let fields = result
                .ok()
                .map(|file| parse_instructions(file["contents"].as_str().unwrap_or_default()))
                .unwrap_or_default();
            state.draft = fields.clone();
            state.saved = Some(fields);
        }
    });
    let has_chats = ui.ctx.session.read().space_project(Space::Chats).is_some();
    let state = drafts.read().personalization.clone();
    let dirty = state
        .saved
        .as_ref()
        .is_some_and(|saved| *saved != state.draft);
    let now = chrono::Utc::now();
    if !has_chats {
        return rsx! {
            section { class: "page",
                PageHead { title: "Personalization" }
                div { class: "pscroll",
                    div { class: "empty-page", b { "No chats yet" } "Memory belongs to Chat: open Chat once in Pluto on the computer, then what your chats save about you will appear here." }
                }
            }
        };
    }
    let (enabled, learn, entries) = match &state.memory {
        Some(Ok(memory)) => (
            memory["enabled"] != false,
            memory["auto"] != false,
            memory["memories"].as_array().cloned().unwrap_or_default(),
        ),
        _ => (true, true, Vec::new()),
    };
    let generation = state.generation;
    rsx! {
        section { class: "page",
            PageHead { title: "Personalization" }
            div { class: "pscroll",
                div { class: "glabel", style: "padding-top:8px", "Custom instructions" }
                if state.saved.is_none() {
                    div { class: "loading-note", "Loading…" }
                } else {
                    for (index, (_, title, description)) in FIELDS.into_iter().enumerate() {
                        label { class: "instr", key: "{index}-{generation}",
                            b { "{title}" }
                            small { "{description}" }
                            textarea {
                                rows: "3",
                                initial_value: "{state.draft[index]}",
                                oninput: move |e| drafts.write().personalization.draft[index] = e.value(),
                            }
                        }
                    }
                    div { class: "savebar",
                        if dirty {
                            button { class: "btn", onclick: move |_| {
                                    let mut drafts = drafts.write();
                                    let state = &mut drafts.personalization;
                                    state.draft = state.saved.clone().unwrap_or_default();
                                    state.generation += 1;
                                },
                                "Cancel"
                            }
                            button { class: "btn primary", onclick: move |_| save_instructions(ui), "Save" }
                        } else {
                            span { "Saved" }
                        }
                    }
                }
                div { class: "glabel", "Memory" }
                match &state.memory {
                    None => rsx! { div { class: "loading-note", "Loading memories" } },
                    Some(Err(error)) => rsx! { div { class: "empty-page", b { "Could not read the memories" } "{error}" } },
                    Some(Ok(_)) => rsx! {
                        div { class: "group",
                            label { class: "set",
                                Icon { name: "brain" }
                                span { class: "grow", "Use memory"
                                    small { "Chats remember what you share about yourself and use it in later conversations. You can also ask a chat to remember, correct or forget something." }
                                }
                                span { class: "toggle",
                                    input { r#type: "checkbox", checked: enabled, onchange: move |e| change_memory(ui, |m| m["enabled"] = json!(e.checked())) }
                                    span {}
                                }
                            }
                            label { class: if enabled { "set" } else { "set is-off" },
                                Icon { name: "sparkles" }
                                span { class: "grow", "Learn from chats"
                                    small { "After a chat goes quiet, Pluto updates your memories with what it taught about you, using your text generation model." }
                                }
                                span { class: "toggle",
                                    input { r#type: "checkbox", checked: learn, disabled: !enabled, onchange: move |e| change_memory(ui, |m| m["auto"] = json!(e.checked())) }
                                    span {}
                                }
                            }
                            button { class: if enabled { "set" } else { "set is-off" }, onclick: move |_| ui.open_sheet(Sheet::MemoryEdit(None)),
                                Icon { name: "plus" }
                                span { class: "grow", "Add a memory" small { "Write something your chats should know about you." } }
                                Icon { name: "chevron-right", class: "chev" }
                            }
                            if !entries.is_empty() {
                                button { class: "set danger", onclick: move |_| ui.open_sheet(Sheet::ClearMemories),
                                    Icon { name: "trash-2" }
                                    span { class: "grow", "Delete all memories" small { "Forget everything your chats saved." } }
                                }
                            }
                        }
                        div { class: "glabel", "Saved memories" }
                        if entries.is_empty() {
                            div { class: "empty-page", style: "padding:32px 16px",
                                b { "No saved memories" }
                                "Tell a chat to remember something, or share what it should know about you."
                            }
                        } else {
                            div { class: if enabled { "group mems" } else { "group mems dim" },
                                for entry in entries.iter().rev() {
                                    MemoryRow { key: "{entry[\"id\"]}", entry: entry.clone(), now: now.timestamp() }
                                }
                            }
                        }
                    },
                }
                p { class: "note", "Temporary chats never read or save memories." }
            }
        }
    }
}

#[component]
fn MemoryRow(entry: Value, now: i64) -> Element {
    let ui = use_context::<Ui>();
    let now = chrono::DateTime::from_timestamp(now, 0).unwrap_or_default();
    let created = entry["createdAt"].as_str().unwrap_or_default();
    let updated = entry["updatedAt"].as_str().unwrap_or(created);
    let verb = if updated == created {
        "Saved"
    } else {
        "Updated"
    };
    let when = relative(updated, now);
    let id = entry["id"].as_str().unwrap_or_default().to_owned();
    rsx! {
        div { class: "mem",
            span { class: "grow",
                "{entry[\"text\"].as_str().unwrap_or_default()}"
                small { "{verb} {when}" }
            }
            button { class: "kebab", "aria-label": "Actions for this memory", onclick: move |_| ui.open_sheet(Sheet::MemoryMenu(id.clone())),
                Icon { name: "ellipsis-vertical" }
            }
        }
    }
}

fn memory_text(ui: &Ui, id: &str) -> String {
    match &ui.drafts.peek().personalization.memory {
        Some(Ok(memory)) => memory["memories"]
            .as_array()
            .and_then(|entries| entries.iter().find(|e| e["id"] == id))
            .and_then(|entry| entry["text"].as_str())
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

/// "Add a memory" (no id) or "Edit memory".
#[component]
pub fn MemoryEditSheet(id: Option<String>) -> Element {
    let ui = use_context::<Ui>();
    let initial = id
        .as_deref()
        .map(|id| memory_text(&ui, id))
        .unwrap_or_default();
    let mut text = use_signal(|| initial.clone());
    let editing = id.is_some();
    rsx! {
        div { class: "instr-sheet",
            textarea {
                class: "big",
                placeholder: "What to remember",
                "aria-label": "What to remember",
                initial_value: "{initial}",
                onmounted: move |e| async move {
                    let _ = e.set_focus(true).await;
                },
                oninput: move |e| text.set(e.value()),
            }
        }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn primary", disabled: text.read().trim().is_empty(), onclick: move |_| {
                    let value = text.peek().trim().to_owned();
                    edit_memory(ui, id.as_deref().unwrap_or_default(), Some(&value));
                    ui.back();
                    ui.ctx.toast(if editing { "Memory saved" } else { "Memory added" });
                },
                if editing { "Save" } else { "Add memory" }
            }
        }
    }
}

#[component]
pub fn MemoryMenu(id: String) -> Element {
    let ui = use_context::<Ui>();
    let edit = id.clone();
    rsx! {
        button { class: "srow", onclick: move |_| ui.open_sheet(Sheet::MemoryEdit(Some(edit.clone()))),
            Icon { name: "pencil" } span { class: "grow", "Edit" }
        }
        button { class: "srow danger", onclick: move |_| {
                edit_memory(ui, &id, None);
                ui.back();
                ui.ctx.toast("Memory forgotten");
            },
            Icon { name: "trash-2" } span { class: "grow", "Forget" }
        }
    }
}

#[component]
pub fn ClearMemories() -> Element {
    let ui = use_context::<Ui>();
    rsx! {
        p { class: "confirm", "Your chats will forget everything they saved about you. This cannot be undone." }
        div { class: "actions two",
            button { class: "btn", onclick: move |_| ui.back(), "Cancel" }
            button { class: "btn danger", onclick: move |_| {
                    clear_memories(ui);
                    ui.back();
                    ui.ctx.toast("All memories deleted");
                },
                "Delete all"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_memory_files() {
        assert_eq!(
            parse(r#"{"version":1,"enabled":false,"memories":[]}"#).unwrap()["enabled"],
            false
        );
        assert!(parse(r#"{"version":2,"memories":[]}"#).is_err());
        assert!(parse(r#"{"version":1}"#).is_err());
        assert_eq!(
            parse_instructions(r#"{"version":1,"about":"Nurse","style":""}"#),
            ["Nurse".to_owned(), String::new()]
        );
        assert_eq!(
            parse_instructions("not json"),
            [String::new(), String::new()]
        );
    }

    #[test]
    fn memory_times_read_like_the_desktop() {
        let now = chrono::Utc::now();
        let ago = |s: i64| (now - chrono::Duration::seconds(s)).to_rfc3339();
        assert_eq!(relative(&ago(10), now), "just now");
        assert_eq!(relative(&ago(300), now), "5m ago");
        assert_eq!(relative(&ago(3 * 86_400), now), "3d ago");
    }
}
