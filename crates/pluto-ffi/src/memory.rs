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
//!
//! The page's state lives here while the app runs; the instructions being
//! typed stay in the interface until Save.

use std::sync::Mutex;

use pluto_core::spaces::Space;
use serde_json::{Value, json};

use crate::{App, Core};

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

/// The page's state: the memory file, the saved instructions.
#[derive(Clone, Debug, Default, PartialEq)]
struct Personalization {
    /// `None` while it loads; an error when the file could not be read or
    /// is not memory JSON (the page then changes nothing).
    memory: Option<Result<Value, String>>,
    /// `None` while they load; an error blocks Save.
    saved: Option<Result<[String; 2], String>>,
    /// Counts the reads of the instructions: the fields start over with
    /// the text read.
    generation: u64,
    /// The instructions being written, and the first failure.
    writing: Vec<String>,
    pending: [String; 2],
    write_error: Option<String>,
    /// The memory change on its way (read again, then written).
    change: Option<MemoryChange>,
}

static STATE: Mutex<Option<Personalization>> = Mutex::new(None);

fn with_state<R>(change: impl FnOnce(&mut Personalization) -> R) -> R {
    let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
    change(state.get_or_insert_with(Default::default))
}

/// A change of the memory file, applied to its latest contents.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum MemoryChange {
    Add { text: String },
    Edit { id: String, text: String },
    Forget { id: String },
    Clear,
    Enabled { on: bool },
    Learn { on: bool },
}

impl MemoryChange {
    fn apply(&self, memory: &mut Value) {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let entries = memory["memories"].as_array_mut();
        match (self, entries) {
            (MemoryChange::Add { text }, Some(entries)) => {
                let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
                entries.push(json!({"id": id, "text": text, "createdAt": now, "updatedAt": now}));
            }
            (MemoryChange::Edit { id, text }, Some(entries)) => {
                for entry in entries.iter_mut().filter(|e| e["id"] == id.as_str()) {
                    entry["text"] = json!(text);
                    entry["updatedAt"] = json!(now);
                }
            }
            (MemoryChange::Forget { id }, Some(entries)) => {
                entries.retain(|entry| entry["id"] != id.as_str());
            }
            (MemoryChange::Clear, _) => memory["memories"] = json!([]),
            (MemoryChange::Enabled { on }, _) => memory["enabled"] = json!(on),
            (MemoryChange::Learn { on }, _) => memory["auto"] = json!(on),
            _ => {}
        }
    }

    /// What the toast says once it is written.
    fn done(&self) -> &'static str {
        match self {
            MemoryChange::Add { .. } => "Memory added",
            MemoryChange::Edit { .. } => "Memory saved",
            MemoryChange::Forget { .. } => "Memory forgotten",
            MemoryChange::Clear => "All memories deleted",
            MemoryChange::Enabled { .. } | MemoryChange::Learn { .. } => "",
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

fn chats_root(app: &App) -> Option<String> {
    app.session
        .space_project(Space::Chats)
        .map(|p| p.workspace_root.clone())
}

/// Reads the chat space's memories and instructions.
fn load(app: &mut App) {
    with_state(|state| {
        let generation = state.generation + 1;
        *state = Personalization {
            generation,
            ..Default::default()
        };
    });
    let Some(root) = chats_root(app) else {
        return;
    };
    app.session.request(
        "chat-memory:read",
        "projects.readFile",
        json!({"cwd": root, "relativePath": MEMORY_PATH}),
    );
    app.session.request(
        "chat-instructions:read",
        "projects.readFile",
        json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH}),
    );
}

/// Follows the files' replies: the first reads, a change's read and write,
/// the instructions' writes. True when something changed.
fn follow(app: &mut App) -> bool {
    let root = chats_root(app);
    let session = &mut app.session;
    let reply = |key: &str| session.replies.get(key).cloned();
    let (memory, instructions, fresh, written) = (
        reply("chat-memory:read"),
        reply("chat-instructions:read"),
        reply("chat-memory:fresh"),
        reply("chat-memory:write"),
    );
    let writes: Vec<(String, Result<Value, String>)> = with_state(|state| {
        state
            .writing
            .iter()
            .filter_map(|key| Some((key.clone(), session.replies.get(key)?.clone())))
            .collect()
    });
    if memory.is_none()
        && instructions.is_none()
        && fresh.is_none()
        && written.is_none()
        && writes.is_empty()
    {
        return false;
    }
    let mut forget = Vec::new();
    let mut toast = None;
    let mut write = None;
    with_state(|state| {
        if let Some(result) = memory {
            forget.push("chat-memory:read".to_owned());
            state.memory = Some(match read_result(&result) {
                Ok(Some(contents)) => parse(&contents),
                Ok(None) => Ok(json!({"version": 1, "memories": []})),
                Err(error) => Err(error),
            });
        }
        if let Some(result) = instructions {
            forget.push("chat-instructions:read".to_owned());
            let saved = read_result(&result)
                .map(|contents| contents.map(|c| parse_instructions(&c)).unwrap_or_default());
            state.saved = Some(saved);
            state.generation += 1;
        }
        if let Some(result) = fresh {
            forget.push("chat-memory:fresh".to_owned());
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
            forget.push("chat-memory:write".to_owned());
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
                        state.saved = Some(Ok(state.pending.clone()));
                        toast = Some("Instructions saved".into());
                    }
                    Some(error) => {
                        toast = Some(format!("The instructions were not saved: {error}"))
                    }
                }
            }
        }
    });
    for key in forget {
        session.replies.remove(&key);
    }
    for (key, _) in writes {
        session.replies.remove(&key);
    }
    if let Some(payload) = write {
        session.request("chat-memory:write", "projects.writeFile", payload);
    }
    if let Some(text) = toast {
        session.show_toast(text);
    }
    true
}

/// One custom instruction: its title, help and saved text.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Instruction {
    pub title: String,
    pub description: String,
    pub saved: String,
}

/// A saved memory, with when it was saved or updated.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MemoryEntry {
    pub id: String,
    pub text: String,
    pub when: String,
}

/// The Personalization page.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PersonalizationView {
    /// Chat was opened once on the computer (its space exists).
    pub has_chats: bool,
    pub instructions_loading: bool,
    pub instructions_error: Option<String>,
    pub instructions: Vec<Instruction>,
    /// Counts the reads: the fields start over with the saved text.
    pub generation: u64,
    pub saving: bool,
    pub memory_loading: bool,
    pub memory_error: Option<String>,
    pub enabled: bool,
    pub learn: bool,
    /// The newest first.
    pub entries: Vec<MemoryEntry>,
    /// A change is on its way.
    pub changing: bool,
}

fn view(app: &App, state: &Personalization) -> PersonalizationView {
    let now = chrono::Utc::now();
    let (enabled, learn, entries) = match &state.memory {
        Some(Ok(memory)) => (
            memory["enabled"] != false,
            memory["auto"] != false,
            memory["memories"].as_array().cloned().unwrap_or_default(),
        ),
        _ => (true, true, Vec::new()),
    };
    let saved = match &state.saved {
        Some(Ok(saved)) => saved.clone(),
        _ => Default::default(),
    };
    PersonalizationView {
        has_chats: chats_root(app).is_some(),
        instructions_loading: state.saved.is_none(),
        instructions_error: state.saved.clone().and_then(Result::err),
        instructions: FIELDS
            .iter()
            .zip(saved)
            .map(|((_, title, description), saved)| Instruction {
                title: (*title).into(),
                description: (*description).into(),
                saved,
            })
            .collect(),
        generation: state.generation,
        saving: !state.writing.is_empty(),
        memory_loading: state.memory.is_none(),
        memory_error: state.memory.clone().and_then(Result::err),
        enabled,
        learn,
        entries: entries
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
                MemoryEntry {
                    id: entry["id"].as_str().unwrap_or_default().to_owned(),
                    text: entry["text"].as_str().unwrap_or_default().to_owned(),
                    when: format!("{verb} {}", relative(updated, now)),
                }
            })
            .collect(),
        changing: state.change.is_some(),
    }
}

#[uniffi::export]
impl Core {
    /// Reads the chat space's memories and instructions (the page opened,
    /// or Retry).
    pub fn personalization_load(&self) {
        self.update(|app, _| load(app));
    }

    /// Follows the files' replies; tells the screens only when something
    /// changed.
    pub fn personalization_follow(&self) {
        let changed = follow(&mut self.hub.lock());
        if changed {
            self.hub.listener.changed();
        }
    }

    pub fn personalization(&self) -> PersonalizationView {
        self.read(|app| with_state(|state| view(app, state)))
    }

    /// Writes the instructions to every chat space.
    pub fn save_instructions(&self, about: String, style: String) {
        self.update(|app, _| {
            let [about, style] = [about, style].map(|t| t.trim().to_owned());
            let contents = format!(
                "{}\n",
                serde_json::to_string_pretty(
                    &json!({"version": 1, "about": about, "style": style})
                )
                .unwrap_or_default()
            );
            let session = &mut app.session;
            let roots: Vec<String> = session
                .state
                .projects
                .iter()
                .filter(|p| session.project_space(p).is_chat())
                .map(|p| p.workspace_root.clone())
                .collect();
            with_state(|state| {
                state.pending = [about, style];
                state.write_error = None;
                state.writing = roots
                    .iter()
                    .map(|root| format!("instructions-write:{root}"))
                    .collect();
            });
            for root in roots {
                session.request(
                    format!("instructions-write:{root}"),
                    "projects.writeFile",
                    json!({"cwd": root, "relativePath": INSTRUCTIONS_PATH, "contents": contents}),
                );
            }
        });
    }

    /// Reads the memory file again, applies `change` to it and writes it
    /// back; one change at a time.
    pub fn change_memory(&self, change: MemoryChange) {
        self.update(|app, _| {
            let Some(root) = chats_root(app) else {
                return;
            };
            let started = with_state(|state| {
                if state.change.is_some() || !matches!(state.memory, Some(Ok(_))) {
                    return false;
                }
                state.change = Some(change);
                true
            });
            if started {
                app.session.request(
                    "chat-memory:fresh",
                    "projects.readFile",
                    json!({"cwd": root, "relativePath": MEMORY_PATH}),
                );
            }
        });
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
        MemoryChange::Edit {
            id: "a".into(),
            text: "new".into(),
        }
        .apply(&mut memory);
        assert_eq!(memory["memories"][0]["text"], "new");
        MemoryChange::Add {
            text: "more".into(),
        }
        .apply(&mut memory);
        assert_eq!(memory["memories"].as_array().unwrap().len(), 2);
        MemoryChange::Forget { id: "a".into() }.apply(&mut memory);
        assert_eq!(memory["memories"][0]["text"], "more");
        MemoryChange::Enabled { on: false }.apply(&mut memory);
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
