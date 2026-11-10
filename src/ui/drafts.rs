//! Composer drafts (`composerDraftStore.ts`): each thread's composer, and the
//! new-thread draft's, keeps its text, images, files and context records
//! while another thread is open, and finds them again at the next run
//! (`drafts.json` beside the client settings, upstream's local storage),
//! by server. A message on its way keeps its draft until the server took
//! it: refused, or lost with the connection, it comes back to its composer.
//!
//! New-thread drafts are sessions (`DraftSessionState`): starting a new
//! thread leaves a draft with content behind under its own key, with its
//! project, start and workspace, and the sidebar lists it (`draft_rows`).
//!
//! Not ported: files whose upload had not finished when the app closed
//! (dropped; upstream keeps a reattach row), and the drafts' model and
//! mode picks (kept for the run only).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};

use crate::app::App;
use crate::ui::attachments::{ComposerFile, ComposerImage};

/// The pause in typing before the drafts are written.
const SAVE_DELAY: Duration = Duration::from_millis(800);

/// What a composer holds.
#[derive(Default)]
pub(crate) struct ComposerDraft {
    pub(crate) text: String,
    pub(crate) images: Vec<ComposerImage>,
    pub(crate) files: Vec<ComposerFile>,
    pub(crate) contexts: Vec<Value>,
}

impl ComposerDraft {
    /// `composerDraftHasUserContent`.
    pub(crate) fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
            && self.images.is_empty()
            && self.files.is_empty()
            && self.contexts.is_empty()
    }
}

/// A new-thread draft's session: where it starts, and since when.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DraftSession {
    pub(crate) project_id: Option<String>,
    pub(crate) created_at: String,
    /// `local` or `worktree`, when picked.
    pub(crate) env_mode: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<String>,
}

impl DraftSession {
    fn entry(&self) -> Value {
        json!({
            "projectId": self.project_id,
            "createdAt": self.created_at,
            "envMode": self.env_mode,
            "branch": self.branch,
            "worktreePath": self.worktree,
        })
    }

    fn read(entry: &Value) -> Self {
        let text = |key: &str| entry[key].as_str().map(str::to_owned);
        DraftSession {
            project_id: text("projectId"),
            created_at: text("createdAt").unwrap_or_default(),
            env_mode: text("envMode"),
            branch: text("branch"),
            worktree: text("worktreePath"),
        }
    }
}

/// A sidebar row for a draft (`SidebarDraftRow`).
pub(crate) struct DraftRow {
    pub(crate) key: String,
    pub(crate) session: DraftSession,
    pub(crate) preview: String,
}

/// The drafts of the threads not open, sends in flight and the saving state.
#[derive(Default)]
pub(crate) struct Drafts {
    /// By server and composer key (`server\nkey`).
    stored: HashMap<String, ComposerDraft>,
    /// New-thread draft sessions, by the same scoped key.
    sessions: HashMap<String, DraftSession>,
    /// Messages on their way, by send key (`launch:<thread>`,
    /// `command:<id>`): their composer key and what it held.
    sent: HashMap<String, (String, ComposerDraft)>,
    /// Bumped by every change outside the open composer.
    revision: u64,
    /// The fingerprint last seen, when it changed, and the one written.
    seen: u64,
    changed_at: Option<Instant>,
    written: u64,
    /// Set by a background write that failed: the drafts are written again.
    write_failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Encoded images, by id, so a write does not encode them again.
    encoded: HashMap<String, String>,
}

/// `drafts.json` beside the client settings file.
fn drafts_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("drafts.json"))
}

fn image_entry(image: &ComposerImage, encoded: &mut HashMap<String, String>) -> Value {
    let data = encoded
        .entry(image.id.clone())
        .or_insert_with(|| base64::engine::general_purpose::STANDARD.encode(&*image.bytes));
    json!({"id": image.id, "name": image.name, "mimeType": image.mime_type, "data": data})
}

/// A composer's persisted form.
pub(crate) fn draft_entry(
    text: &str,
    images: &[ComposerImage],
    files: &[ComposerFile],
    contexts: &[Value],
    encoded: &mut HashMap<String, String>,
) -> Value {
    // A file goes once uploaded: one still uploading cannot come back.
    let files: Vec<Value> = files
        .iter()
        .filter_map(|file| {
            let id = file.upload.as_ref()?.as_ref().ok()?;
            Some(json!({
                "id": file.id,
                "name": file.name,
                "mimeType": file.mime_type,
                "size": file.size,
                "attachmentId": id,
            }))
        })
        .collect();
    json!({
        "prompt": text,
        "images": images.iter().map(|i| image_entry(i, encoded)).collect::<Vec<_>>(),
        "files": files,
        "contexts": contexts,
    })
}

fn stored_entry(draft: &ComposerDraft, encoded: &mut HashMap<String, String>) -> Value {
    draft_entry(
        &draft.text,
        &draft.images,
        &draft.files,
        &draft.contexts,
        encoded,
    )
}

pub(crate) fn read_draft(entry: &Value) -> ComposerDraft {
    let images = entry["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|image| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(image["data"].as_str()?)
                .ok()?;
            let preview = crate::ui::attachments::decode(&bytes)?;
            Some(ComposerImage {
                id: image["id"].as_str()?.to_owned(),
                name: image["name"].as_str().unwrap_or("image").to_owned(),
                mime_type: image["mimeType"].as_str().unwrap_or("image/png").to_owned(),
                bytes: Arc::new(bytes),
                preview: Arc::new(preview),
            })
        })
        .collect();
    let files = entry["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            Some(ComposerFile {
                id: file["id"].as_str()?.to_owned(),
                name: file["name"].as_str().unwrap_or("file").to_owned(),
                mime_type: file["mimeType"].as_str().unwrap_or_default().to_owned(),
                size: file["size"].as_u64().unwrap_or(0),
                path: std::path::PathBuf::new(),
                upload: Some(Ok(file["attachmentId"].as_str()?.to_owned())),
            })
        })
        .collect();
    ComposerDraft {
        text: entry["prompt"].as_str().unwrap_or_default().to_owned(),
        images,
        files,
        contexts: entry["contexts"].as_array().cloned().unwrap_or_default(),
    }
}

/// The drafts saved by the last run.
pub(crate) fn load(settings: Option<&std::path::Path>) -> Drafts {
    let value: Value = drafts_path(settings)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let stored = value["drafts"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, entry)| (key.clone(), read_draft(entry)))
        .filter(|(_, draft)| !draft.is_empty())
        .collect();
    let sessions = value["sessions"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, entry)| (key.clone(), DraftSession::read(entry)))
        .collect();
    Drafts {
        stored,
        sessions,
        ..Default::default()
    }
}

/// `draftPreviewText`: the prompt's first line, its references read as
/// their labels; else how many attachments it holds.
pub(crate) fn draft_preview(draft: &ComposerDraft) -> String {
    preview_text(
        &draft.text,
        draft.images.len() + draft.files.len() + draft.contexts.len(),
    )
}

fn preview_text(prompt: &str, attachments: usize) -> String {
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        match after.find("](").and_then(|close| {
            after[close + 2..]
                .find(')')
                .map(|end| (close, close + 2 + end + 1))
        }) {
            Some((close, end)) => {
                out.push_str(&rest[..open]);
                out.push_str(&after[..close]);
                rest = &after[end..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    let text = out.trim();
    if !text.is_empty() {
        return text.to_owned();
    }
    format!(
        "{attachments} attachment{}",
        if attachments == 1 { "" } else { "s" }
    )
}

impl App {
    fn draft_scope(&self, key: &str) -> String {
        format!("{}\n{key}", self.connect.server.trim())
    }

    /// Empties the composer into a draft.
    pub(crate) fn take_composer(&mut self) -> ComposerDraft {
        ComposerDraft {
            text: std::mem::take(&mut self.composer),
            images: std::mem::take(&mut self.composer_images),
            files: std::mem::take(&mut self.composer_files),
            contexts: std::mem::take(&mut self.composer_contexts),
        }
    }

    fn put_composer(&mut self, draft: ComposerDraft) {
        self.composer = draft.text;
        self.composer_images = draft.images;
        self.composer_files = draft.files;
        self.composer_contexts = draft.contexts;
    }

    fn composer_is_empty(&self) -> bool {
        self.composer.trim().is_empty()
            && self.composer_images.is_empty()
            && self.composer_files.is_empty()
            && self.composer_contexts.is_empty()
    }

    /// The open draft's session as the composer has it now.
    fn open_session(&self) -> DraftSession {
        let scope = self.draft_scope(&self.draft_key);
        let workspace = self.draft_workspace.clone();
        DraftSession {
            project_id: self
                .draft_project_id
                .clone()
                .or_else(|| self.draft_project().map(|p| p.id)),
            created_at: self
                .drafts
                .sessions
                .get(&scope)
                .map(|s| s.created_at.clone())
                .filter(|at| !at.is_empty())
                .unwrap_or_else(crate::model::now_iso),
            env_mode: self.draft_env_mode.as_ref().map(|(_, mode)| mode.clone()),
            branch: workspace.as_ref().and_then(|w| w.branch.clone()),
            worktree: workspace.and_then(|w| w.worktree),
        }
    }

    /// Leaving the new-thread draft: one with content keeps its session.
    pub(crate) fn note_draft_session(&mut self) {
        if self.selected.is_none() && !self.composer_is_empty() {
            let session = self.open_session();
            let scope = self.draft_scope(&self.draft_key);
            self.drafts.sessions.insert(scope, session);
            self.drafts.revision += 1;
        }
    }

    /// A new-thread surface leaves a draft with content behind and opens a
    /// fresh one (`Sidebar.tsx`'s new DraftId).
    pub(crate) fn leave_draft(&mut self) {
        let open = self.selected.is_none();
        let has_content = if open {
            !self.composer_is_empty()
        } else {
            self.drafts
                .stored
                .get(&self.draft_scope(&self.draft_key))
                .is_some_and(|d| !d.is_empty())
        };
        if !has_content {
            return;
        }
        self.note_draft_session();
        let old = self.draft_key.clone();
        self.draft_key = format!("draft:{}", uuid::Uuid::new_v4());
        if open {
            self.swap_drafts(&old, &self.draft_key.clone());
        }
        self.draft_listed = false;
    }

    /// Opens a listed draft, where it started.
    pub(crate) fn open_draft_session(&mut self, key: &str) {
        if self.selected.is_none() && self.draft_key == key {
            return;
        }
        self.note_draft_session();
        let scope = self.draft_scope(key);
        let session = self
            .drafts
            .sessions
            .get(&scope)
            .cloned()
            .unwrap_or_default();
        if self.selected.is_none() {
            let old = self.draft_key.clone();
            self.swap_drafts(&old, key);
            self.draft_key = key.to_owned();
        } else {
            self.draft_key = key.to_owned();
            self.select_thread(None);
        }
        self.selection.clear();
        self.draft_project_id = session.project_id.clone();
        self.draft_env_mode = session
            .env_mode
            .clone()
            .zip(session.project_id.clone())
            .map(|(mode, project)| (project, mode));
        self.draft_workspace = session.project_id.clone().and_then(|project| {
            (session.branch.is_some() || session.worktree.is_some()).then(|| {
                crate::ui::branch_picker::DraftWorkspace {
                    project,
                    branch: session.branch.clone(),
                    worktree: session.worktree.clone(),
                    from_origin: None,
                }
            })
        });
        self.draft_listed = true;
    }

    /// Discards a draft; the open one leaves an empty composer.
    pub(crate) fn discard_draft_session(&mut self, key: &str) {
        let scope = self.draft_scope(key);
        self.drafts.stored.remove(&scope);
        self.drafts.sessions.remove(&scope);
        if self.selected.is_none() && self.draft_key == key {
            let _ = self.take_composer();
            self.draft_listed = false;
        }
        self.drafts.revision += 1;
    }

    /// `SidebarDraftBlock`'s rows: new-thread drafts with content in
    /// `scope`'s project (any without one), newest first. The open draft
    /// shows only when it was opened from the list.
    pub(crate) fn draft_rows(&self, scope: Option<&str>) -> Vec<DraftRow> {
        let prefix = self.draft_scope("draft");
        let open_key = self.selected.is_none().then_some(self.draft_key.as_str());
        let mut rows: Vec<DraftRow> = Vec::new();
        for (scoped, draft) in &self.drafts.stored {
            let Some(key) = scoped
                .strip_prefix(&prefix[..prefix.len() - "draft".len()])
                .filter(|k| k.starts_with("draft"))
            else {
                continue;
            };
            if Some(key) == open_key || draft.is_empty() {
                continue;
            }
            let session = self
                .drafts
                .sessions
                .get(scoped)
                .cloned()
                .unwrap_or_default();
            rows.push(DraftRow {
                key: key.to_owned(),
                preview: draft_preview(draft),
                session,
            });
        }
        if let Some(key) = open_key.filter(|_| self.draft_listed && !self.composer_is_empty()) {
            rows.push(DraftRow {
                key: key.to_owned(),
                preview: preview_text(
                    &self.composer,
                    self.composer_images.len()
                        + self.composer_files.len()
                        + self.composer_contexts.len(),
                ),
                session: self.open_session(),
            });
        }
        rows.retain(|row| {
            scope.is_none_or(|scope| row.session.project_id.as_deref().is_none_or(|p| p == scope))
        });
        rows.sort_by(|a, b| b.session.created_at.cmp(&a.session.created_at));
        rows
    }

    /// `useThreadHasUnsentDraft`: a thread not open keeps a draft.
    pub(crate) fn has_draft(&self, thread_id: &str) -> bool {
        self.drafts
            .stored
            .get(&self.draft_scope(thread_id))
            .is_some_and(|draft| !draft.is_empty())
    }

    /// The open composer goes to `from`'s draft and `to`'s comes back.
    pub(crate) fn swap_drafts(&mut self, from: &str, to: &str) {
        if from == to {
            return;
        }
        let current = self.take_composer();
        let (from, to) = (self.draft_scope(from), self.draft_scope(to));
        if current.is_empty() {
            self.drafts.stored.remove(&from);
        } else {
            self.drafts.stored.insert(from, current);
        }
        if let Some(draft) = self.drafts.stored.remove(&to) {
            self.put_composer(draft);
        }
        self.drafts.revision += 1;
    }

    /// The open composer's saved draft, once connected (the last run's).
    pub(crate) fn restore_open_draft(&mut self) {
        let key = self.draft_scope(&self.composer_key(false));
        if self.composer.trim().is_empty()
            && self.composer_images.is_empty()
            && let Some(draft) = self.drafts.stored.remove(&key)
        {
            self.put_composer(draft);
            self.drafts.revision += 1;
        }
    }

    /// Keeps a message's draft until the server took it.
    pub(crate) fn hold_sent(&mut self, send_key: String, composer: String, draft: ComposerDraft) {
        self.drafts.sent.insert(send_key, (composer, draft));
    }

    /// The server took the message.
    pub(crate) fn sent_done(&mut self, send_key: &str) {
        self.drafts.sent.remove(send_key);
    }

    /// The message did not go: its draft comes back to its composer, unless
    /// something new was written there meanwhile. Returns whether it held one.
    pub(crate) fn give_back(&mut self, send_key: &str) -> bool {
        let Some((composer, draft)) = self.drafts.sent.remove(send_key) else {
            return false;
        };
        let open = self.composer_key(false) == composer;
        // Empty means no text, image, file or context (`is_empty`).
        let open_empty = self.composer.trim().is_empty()
            && self.composer_images.is_empty()
            && self.composer_files.is_empty()
            && self.composer_contexts.is_empty();
        let scope = self.draft_scope(&composer);
        if open && open_empty {
            self.put_composer(draft);
        } else if !open
            && self
                .drafts
                .stored
                .get(&scope)
                .is_none_or(ComposerDraft::is_empty)
        {
            self.drafts.stored.insert(scope, draft);
        } else {
            // Both hold something: the message's text comes after what was
            // written, and its images, files and contexts join the others.
            let ComposerDraft {
                text,
                images,
                files,
                contexts,
            } = draft;
            let (target, target_images, target_files, target_contexts) = if open {
                (
                    &mut self.composer,
                    &mut self.composer_images,
                    &mut self.composer_files,
                    &mut self.composer_contexts,
                )
            } else {
                let stored = self.drafts.stored.entry(scope).or_default();
                (
                    &mut stored.text,
                    &mut stored.images,
                    &mut stored.files,
                    &mut stored.contexts,
                )
            };
            if !text.trim().is_empty() {
                if !target.trim().is_empty() {
                    target.push_str("\n\n");
                }
                target.push_str(&text);
            }
            target_images.extend(images);
            target_files.extend(files);
            target_contexts.extend(contexts);
        }
        self.drafts.revision += 1;
        true
    }

    /// The connection dropped: every message still on its way comes back.
    pub(crate) fn give_back_all(&mut self) -> usize {
        let keys: Vec<String> = self.drafts.sent.keys().cloned().collect();
        keys.iter().filter(|key| self.give_back(key)).count()
    }

    fn drafts_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.composer.hash(&mut hasher);
        for image in &self.composer_images {
            image.id.hash(&mut hasher);
        }
        for file in &self.composer_files {
            file.id.hash(&mut hasher);
            matches!(file.upload, Some(Ok(_))).hash(&mut hasher);
        }
        for context in &self.composer_contexts {
            context.to_string().hash(&mut hasher);
        }
        self.composer_key(false).hash(&mut hasher);
        self.drafts.revision.hash(&mut hasher);
        hasher.finish()
    }

    /// Writes the drafts a moment after the last change (every frame).
    pub(crate) fn save_drafts_when_idle(&mut self, ctx: &eframe::egui::Context) {
        // A write that failed counts as not written: it is tried again.
        if self
            .drafts
            .write_failed
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.drafts.written = 0;
            self.drafts.changed_at = Some(Instant::now());
        }
        let print = self.drafts_fingerprint();
        if print != self.drafts.seen {
            self.drafts.seen = print;
            self.drafts.changed_at = Some(Instant::now());
        }
        if print == self.drafts.written {
            return;
        }
        match self.drafts.changed_at {
            Some(at) if at.elapsed() >= SAVE_DELAY => self.save_drafts(),
            Some(at) => ctx.request_repaint_after(SAVE_DELAY.saturating_sub(at.elapsed())),
            None => {}
        }
    }

    /// Writes the drafts now, off the UI thread.
    pub(crate) fn save_drafts(&mut self) {
        let Some((path, text)) = self.drafts_text() else {
            return;
        };
        let failed = self.drafts.write_failed.clone();
        std::thread::spawn(move || {
            if !write_drafts(&path, Some(text)) {
                failed.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }

    /// Writes the drafts on this thread, before the app goes (`on_exit`):
    /// the last change and a write still running both land.
    pub(crate) fn save_drafts_now(&mut self) {
        if let Some((path, text)) = self.drafts_text() {
            write_drafts(&path, Some(text));
        }
    }

    /// The drafts file's path and contents, marking them written.
    fn drafts_text(&mut self) -> Option<(std::path::PathBuf, String)> {
        let path = drafts_path(self.client_settings_path.as_deref())?;
        self.drafts.written = self.drafts_fingerprint();
        let mut entries = serde_json::Map::new();
        let mut encoded = std::mem::take(&mut self.drafts.encoded);
        let open_empty = self.composer.trim().is_empty()
            && self.composer_images.is_empty()
            && self.composer_files.is_empty()
            && self.composer_contexts.is_empty();
        if !open_empty {
            let entry = draft_entry(
                &self.composer,
                &self.composer_images,
                &self.composer_files,
                &self.composer_contexts,
                &mut encoded,
            );
            entries.insert(self.draft_scope(&self.composer_key(false)), entry);
        }
        for (key, draft) in &self.drafts.stored {
            if !draft.is_empty() {
                entries.insert(key.clone(), stored_entry(draft, &mut encoded));
            }
        }
        // The encodings of images no draft holds anymore go.
        let held: std::collections::HashSet<String> = entries
            .values()
            .flat_map(|e| e["images"].as_array().cloned().unwrap_or_default())
            .filter_map(|i| i["id"].as_str().map(str::to_owned))
            .collect();
        encoded.retain(|id, _| held.contains(id));
        self.drafts.encoded = encoded;
        // Sessions go with their drafts.
        let sessions: serde_json::Map<String, Value> = self
            .drafts
            .sessions
            .iter()
            .filter(|(key, _)| entries.contains_key(*key))
            .map(|(key, session)| (key.clone(), session.entry()))
            .collect();
        let open_scope = self.draft_scope(&self.draft_key);
        let mut sessions = sessions;
        if self.selected.is_none() && entries.contains_key(&open_scope) {
            sessions.insert(open_scope, self.open_session().entry());
        }
        let text = json!({"version": 1, "drafts": entries, "sessions": sessions}).to_string();
        Some((path, text))
    }
}

/// Writes the latest drafts given (one writer at a time, through one
/// temporary file, so two saves never interleave or land out of order);
/// whether the file holds them.
fn write_drafts(path: &std::path::Path, text: Option<String>) -> bool {
    use std::sync::Mutex;
    static LATEST: Mutex<Option<String>> = Mutex::new(None);
    static WRITING: Mutex<()> = Mutex::new(());
    if let Some(text) = text {
        *LATEST.lock().unwrap_or_else(|p| p.into_inner()) = Some(text);
    }
    let _writing = WRITING.lock().unwrap_or_else(|p| p.into_inner());
    let Some(text) = LATEST.lock().unwrap_or_else(|p| p.into_inner()).take() else {
        // A later save already wrote them.
        return true;
    };
    let temporary = path.with_extension("json.tmp");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&temporary, text).is_ok() && std::fs::rename(&temporary, path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_thread_drafts_stay_behind_as_rows() {
        use crate::app::{App, Options};
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
        let config = json!({"type": "snapshot", "config": {"providers": []}});
        let shell = json!({"kind": "snapshot", "snapshot": {"snapshotSequence": 1,
            "projects": [{"id": "p", "title": "Plain", "workspaceRoot": "/fixture/plain"}],
            "threads": []}});
        let app = harness.state_mut();
        app.load_offline(&config, &shell).unwrap();
        app.start_draft(Some("p".into()));
        app.composer = "First idea".into();
        // A new thread leaves the draft behind, listed.
        app.start_draft(Some("p".into()));
        assert!(app.composer.is_empty());
        let rows = app.draft_rows(None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].preview, "First idea");
        assert_eq!(rows[0].session.project_id.as_deref(), Some("p"));
        // Opening it brings its text back; it stays listed while open.
        let key = rows[0].key.clone();
        app.composer = "Second".into();
        app.open_draft_session(&key);
        assert_eq!(app.composer, "First idea");
        let keys: Vec<String> = app.draft_rows(None).into_iter().map(|r| r.key).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&key));
        app.discard_draft_session(&key);
        assert!(app.composer.is_empty());
        assert_eq!(app.draft_rows(None).len(), 1);
    }

    #[test]
    fn draft_rows_preview_their_first_line() {
        assert_eq!(
            preview_text("\n  Look at [main.rs](file:///a/main.rs) now\nmore", 0),
            "Look at main.rs now"
        );
        assert_eq!(preview_text("  ", 1), "1 attachment");
        assert_eq!(preview_text("", 3), "3 attachments");
        let session = DraftSession {
            project_id: Some("p".into()),
            created_at: "2026-10-29T10:00:00.000Z".into(),
            env_mode: Some("worktree".into()),
            branch: Some("main".into()),
            worktree: None,
        };
        assert_eq!(DraftSession::read(&session.entry()), session);
    }

    #[test]
    fn drafts_round_trip_without_unfinished_uploads() {
        let draft = ComposerDraft {
            text: "half a thought".into(),
            files: vec![
                ComposerFile {
                    id: "a".into(),
                    name: "done.txt".into(),
                    mime_type: "text/plain".into(),
                    size: 3,
                    path: std::path::PathBuf::new(),
                    upload: Some(Ok("stored".into())),
                },
                ComposerFile {
                    id: "b".into(),
                    name: "uploading.txt".into(),
                    mime_type: "text/plain".into(),
                    size: 3,
                    path: std::path::PathBuf::new(),
                    upload: None,
                },
            ],
            contexts: vec![json!({"contextId": "terminal_1"})],
            ..Default::default()
        };
        let read = read_draft(&stored_entry(&draft, &mut HashMap::new()));
        assert_eq!(read.text, "half a thought");
        assert_eq!(read.files.len(), 1);
        assert_eq!(read.files[0].upload, Some(Ok("stored".into())));
        assert_eq!(read.contexts.len(), 1);
        assert!(ComposerDraft::default().is_empty());
    }
}
