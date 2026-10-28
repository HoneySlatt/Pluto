//! Personalization (`#memory`, after the desktop's `memory_settings.rs`):
//! the custom instructions every chat gets ("About you", "How chats should
//! respond", in each chat space's `.pluto/instructions.json`, the temporary
//! one included), and what Chat remembers about the user (the chat space's
//! `.pluto/memory.json`): Use memory, Learn from chats, Add a memory, Delete
//! all memories, then the saved memories with Edit and Forget. Read and
//! written with `projects.readFile` and `projects.writeFile`. A change reads
//! the file again first (the computer may have changed it), and shows as
//! done once the server wrote it; a read that failed for another reason
//! than a missing file blocks every change until it is read again.

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
    /// `None` while it loads; an error when the file could not be read or
    /// is not memory JSON (the page then changes nothing).
    pub memory: Option<Result<Value, String>>,
    /// `None` while they load; an error blocks Save.
    pub saved: Option<Result<[String; 2], String>>,
    pub draft: [String; 2],
    /// Cancel remounts the fields with the saved text.
    pub generation: u64,
    /// The memory change on its way (read again, then written).
    pub change: Option<MemoryChange>,
    /// The instruction files still being written, and the first failure.
    pub writing: Vec<String>,
    pub write_error: Option<String>,
}

/// A change of the memory file, applied to its latest contents.
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryChange {
    Add(String),
    Edit(String, String),
    Forget(String),
    Clear,
    Enabled(bool),
    Learn(bool),
}

impl MemoryChange {
    fn apply(&self, memory: &mut Value) {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let entries = memory["memories"].as_array_mut();
        match (self, entries) {
            (MemoryChange::Add(text), Some(entries)) => {
                let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
                entries.push(json!({"id": id, "text": text, "createdAt": now, "updatedAt": now}));
            }
            (MemoryChange::Edit(id, text), Some(entries)) => {
                for entry in entries.iter_mut().filter(|e| e["id"] == id.as_str()) {
                    entry["text"] = json!(text);
                    entry["updatedAt"] = json!(now);
                }
            }
            (MemoryChange::Forget(id), Some(entries)) => {
                entries.retain(|entry| entry["id"] != id.as_str());
            }
            (MemoryChange::Clear, _) => memory["memories"] = json!([]),
            (MemoryChange::Enabled(on), _) => memory["enabled"] = json!(on),
            (MemoryChange::Learn(on), _) => memory["auto"] = json!(on),
            _ => {}
        }
    }

    /// What the toast says once it is written.
    fn done(&self) -> &'static str {
        match self {
            MemoryChange::Add(_) => "Memory added",
            MemoryChange::Edit(..) => "Memory saved",
            MemoryChange::Forget(_) => "Memory forgotten",
            MemoryChange::Clear => "All memories deleted",
            MemoryChange::Enabled(_) | MemoryChange::Learn(_) => "",
        }
    }
}

/// A read's result: the server reports a missing file as a failure to read
/// it (a chat space without one has saved nothing yet); a failure of the
/// connection itself is not a missing file.
fn read_result(result: &Result<Value, String>) -> Result<Option<String>, String> {
    match result {
        Ok(file) => Ok(Some(
            file["contents"].as_str().unwrap_or_default().to_owned(),
        )),
        Err(error) if error.starts_with("Failed to read workspace file") => Ok(None),
        Err(error) => Err(error.clone()),
    }
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

/// Reads the memory file again, applies `change` to it and writes it back
/// (`Memory` follows the replies); one change at a time.
pub fn change_memory(ui: Ui, change: MemoryChange) {
    let Some(root) = chats_root(&ui) else {
        return;
    };
    let mut drafts = ui.drafts;
    {
        let mut drafts = drafts.write();
        let state = &mut drafts.personalization;
        if state.change.is_some() || !matches!(state.memory, Some(Ok(_))) {
            return;
        }
        state.change = Some(change);
    }
    ui.ctx.request(
        "chat-memory:fresh",
        "projects.readFile",
        json!({"cwd": root, "relativePath": MEMORY_PATH}),
    );
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
    let session = ui.ctx.session.peek();
    let roots: Vec<String> = session
        .state
        .projects
        .iter()
        .filter(|p| session.project_space(p).is_chat())
        .map(|p| p.workspace_root.clone())
        .collect();
    drop(session);
    {
        let mut drafts = drafts.write();
        let state = &mut drafts.personalization;
        state.draft = [about, style];
        state.write_error = None;
        state.writing = roots
            .iter()
            .map(|root| format!("instructions-write:{root}"))
            .collect();
    }
    for root in roots {
        ui.ctx.request(
            format!("instructions-write:{root}"),
            "projects.writeFile",
            json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH, "contents": contents}),
        );
    }
}

/// Follows the files' replies: the first reads, a change's read and write,
/// the instructions' writes.
fn follow_replies(ui: Ui) {
    let mut drafts = ui.drafts;
    let session = ui.ctx.session.read();
    let reply = |key: &str| session.replies.get(key).cloned();
    let (memory, instructions, fresh, written) = (
        reply("chat-memory:read"),
        reply("chat-instructions:read"),
        reply("chat-memory:fresh"),
        reply("chat-memory:write"),
    );
    let writes: Vec<(String, Result<Value, String>)> = drafts
        .peek()
        .personalization
        .writing
        .iter()
        .filter_map(|key| Some((key.clone(), session.replies.get(key)?.clone())))
        .collect();
    drop(session);
    if memory.is_none()
        && instructions.is_none()
        && fresh.is_none()
        && written.is_none()
        && writes.is_empty()
    {
        return;
    }
    let root = chats_root(&ui);
    let mut forget = Vec::new();
    let mut toast = None;
    let mut write = None;
    {
        let mut drafts = drafts.write();
        let state = &mut drafts.personalization;
        if let Some(result) = memory {
            forget.push("chat-memory:read");
            state.memory = Some(match read_result(&result) {
                Ok(Some(contents)) => parse(&contents),
                Ok(None) => Ok(json!({"version": 1, "memories": []})),
                Err(error) => Err(error),
            });
        }
        if let Some(result) = instructions {
            forget.push("chat-instructions:read");
            let saved = read_result(&result)
                .map(|contents| contents.map(|c| parse_instructions(&c)).unwrap_or_default());
            if let Ok(fields) = &saved {
                state.draft = fields.clone();
            }
            state.saved = Some(saved);
        }
        if let Some(result) = fresh {
            forget.push("chat-memory:fresh");
            let latest = read_result(&result).and_then(|contents| match contents {
                Some(contents) => parse(&contents),
                None => Ok(json!({"version": 1, "memories": []})),
            });
            match (latest, state.change.clone(), &root) {
                (Ok(mut latest), Some(change), Some(root)) => {
                    change.apply(&mut latest);
                    let contents = format!(
                        "{}\n",
                        serde_json::to_string_pretty(&latest).unwrap_or_default()
                    );
                    state.memory = Some(Ok(latest));
                    write = Some(
                        json!({"cwd": root, "relativePath": MEMORY_PATH, "contents": contents}),
                    );
                }
                (Err(error), ..) => {
                    state.change = None;
                    toast = Some(format!("Nothing changed: {error}"));
                }
                _ => state.change = None,
            }
        }
        if let Some(result) = written {
            forget.push("chat-memory:write");
            let change = state.change.take();
            match result {
                Ok(_) => {
                    toast = change
                        .map(|c| c.done().to_owned())
                        .filter(|t| !t.is_empty())
                }
                Err(error) => toast = Some(format!("The memories were not saved: {error}")),
            }
        }
        if !writes.is_empty() {
            for (key, result) in &writes {
                state.writing.retain(|k| k != key);
                if let Err(error) = result
                    && state.write_error.is_none()
                {
                    state.write_error = Some(error.clone());
                }
            }
            if state.writing.is_empty() {
                match state.write_error.take() {
                    None => {
                        state.saved = Some(Ok(state.draft.clone()));
                        toast = Some("Instructions saved".into());
                    }
                    Some(error) => {
                        toast = Some(format!("The instructions were not saved: {error}"))
                    }
                }
            }
        }
    }
    for key in forget {
        ui.ctx.forget(key);
    }
    for (key, _) in writes {
        ui.ctx.forget(&key);
    }
    if let Some(payload) = write {
        ui.ctx
            .request("chat-memory:write", "projects.writeFile", payload);
    }
    if let Some(text) = toast {
        ui.ctx.toast(text);
    }
}

#[component]
pub fn Memory() -> Element {
    let ui = use_context::<Ui>();
    let mut drafts = ui.drafts;
    use_hook(move || load(ui));
    use_effect(move || follow_replies(ui));
    let has_chats = ui.ctx.session.read().space_project(Space::Chats).is_some();
    let state = drafts.read().personalization.clone();
    let dirty = matches!(&state.saved, Some(Ok(saved)) if *saved != state.draft);
    let saving = !state.writing.is_empty();
    let changing = state.change.is_some();
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
                } else if let Some(Err(error)) = state.saved.clone() {
                    div { class: "empty-page", b { "Could not read the instructions" } "{error}"
                        div { style: "margin-top:16px", button { class: "btn", onclick: move |_| load(ui), "Retry" } }
                    }
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
                        if saving {
                            span { role: "status", "Saving…" }
                        } else if dirty {
                            button { class: "btn", onclick: move |_| {
                                    let mut drafts = drafts.write();
                                    let state = &mut drafts.personalization;
                                    state.draft = state.saved.clone().and_then(Result::ok).unwrap_or_default();
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
                    Some(Err(error)) => rsx! {
                        div { class: "empty-page", b { "Could not read the memories" } "{error}"
                            div { style: "margin-top:16px", button { class: "btn", onclick: move |_| load(ui), "Retry" } }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        div { class: "group",
                            label { class: "set",
                                Icon { name: "brain" }
                                span { class: "grow", "Use memory"
                                    small { "Chats remember what you share about yourself and use it in later conversations. You can also ask a chat to remember, correct or forget something." }
                                }
                                span { class: "toggle",
                                    input { r#type: "checkbox", checked: enabled, disabled: changing, onchange: move |e| change_memory(ui, MemoryChange::Enabled(e.checked())) }
                                    span {}
                                }
                            }
                            label { class: if enabled { "set" } else { "set is-off" },
                                Icon { name: "sparkles" }
                                span { class: "grow", "Learn from chats"
                                    small { "After a chat goes quiet, Pluto updates your memories with what it taught about you, using your text generation model." }
                                }
                                span { class: "toggle",
                                    input { r#type: "checkbox", checked: learn, disabled: !enabled || changing, onchange: move |e| change_memory(ui, MemoryChange::Learn(e.checked())) }
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
                        div { class: "glabel", if changing { "Saved memories · saving…" } else { "Saved memories" } }
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
    let changing = ui.drafts.read().personalization.change.is_some();
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
            button { class: "btn primary", disabled: changing || text.read().trim().is_empty(), onclick: move |_| {
                    let value = text.peek().trim().to_owned();
                    change_memory(ui, match &id {
                        Some(id) => MemoryChange::Edit(id.clone(), value),
                        None => MemoryChange::Add(value),
                    });
                    ui.back();
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
                change_memory(ui, MemoryChange::Forget(id.clone()));
                ui.back();
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
                    change_memory(ui, MemoryChange::Clear);
                    ui.back();
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

    /// A missing file reads as empty; a lost connection blocks changes.
    #[test]
    fn only_a_missing_file_reads_as_empty() {
        let missing = Err("Failed to read workspace file '.pluto/memory.json' in '/c'.".into());
        assert_eq!(read_result(&missing), Ok(None));
        let lost = Err("The connection dropped before the server answered.".to_owned());
        assert!(read_result(&lost).is_err());
        assert_eq!(
            read_result(&Ok(json!({"contents": "{}"}))),
            Ok(Some("{}".into()))
        );
    }

    #[test]
    fn changes_apply_to_the_latest_file() {
        let mut memory = json!({"version": 1, "memories": [{"id": "a", "text": "old"}]});
        MemoryChange::Edit("a".into(), "new".into()).apply(&mut memory);
        assert_eq!(memory["memories"][0]["text"], "new");
        MemoryChange::Add("more".into()).apply(&mut memory);
        assert_eq!(memory["memories"].as_array().unwrap().len(), 2);
        MemoryChange::Forget("a".into()).apply(&mut memory);
        assert_eq!(memory["memories"][0]["text"], "more");
        MemoryChange::Enabled(false).apply(&mut memory);
        assert_eq!(memory["enabled"], false);
        MemoryChange::Clear.apply(&mut memory);
        assert_eq!(memory["memories"], json!([]));
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
