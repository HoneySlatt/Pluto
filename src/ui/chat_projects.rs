//! Chat projects (Pluto, not upstream), after ChatGPT's and Claude's
//! projects: chats grouped under a name, sharing instructions and reference
//! files, with a memory of their own.
//!
//! A project is a hidden chat space of its own, a folder under
//! `chat-projects` beside the Scratch folder: `.pluto/chat.json` makes it a
//! chat space, `.pluto/project.json` holds its name and instructions, and
//! `files/` its reference files, all of which the server hands every chat in
//! it (`chatProject.ts`). The chat sidebar lists the projects above the
//! chats; a project opens on its page, a new chat's draft with the project's
//! instructions, files, memory and chats under the composer. Its chats stay
//! out of the main chat list. The project's memory (its own
//! `.pluto/memory.json`, see `memory_settings`) is listed there, each
//! memory with a remove button. Files travel as base64 through `projects.writeFile`,
//! so a remote server works too.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Project, Thread};
use crate::theme::{self, Weight, font};
use crate::ui::context_menu;
use crate::ui::widgets;

const PROFILE: &str = "{\n  \"version\": 1\n}\n";
const PROJECT_PATH: &str = ".pluto/project.json";
const MEMORY_PATH: &str = ".pluto/memory.json";
/// "Delete all"'s confirmation command: handled here, never dispatched.
pub(crate) const CLEAR_MEMORY_COMMAND: &str = "pluto.project-memory.clear";
const FILES: &str = "files";
/// The largest reference file, in bytes.
const MAX_FILE_BYTES: u64 = 30 * 1024 * 1024;
/// `FILESYSTEM_METADATA_BATCH_LIMIT`.
const METADATA_BATCH: usize = 64;

/// Files read off the UI thread for a project: its id, the file's name and
/// base64 contents, or why it was refused.
type Picked = Arc<Mutex<Vec<Result<(String, String, String), String>>>>;

#[derive(Default)]
pub(crate) struct ChatProjects {
    /// The New / Rename project dialog: the project renamed (`None` for a
    /// new one), the name typed, and whether it takes the focus.
    dialog: Option<(Option<String>, String, bool)>,
    /// Projects being created: command id → (name, folder).
    creating: HashMap<String, (String, String)>,
    /// Open the page of the project in this folder once it reaches the shell.
    open_root: Option<String>,
    /// Projects whose chat profile was written this run.
    profiled: HashSet<String>,
    /// Loaded project pages, by project id.
    pages: HashMap<String, Page>,
    picked: Picked,
    /// A project's sidebar menu.
    menu: Option<(String, context_menu::State)>,
}

#[derive(Default)]
struct Page {
    /// The saved instructions, once read, and the ones being edited.
    instructions: Option<String>,
    draft: String,
    /// The reference files, once listed: name and size.
    files: Option<Vec<(String, Option<u64>)>>,
    /// Files being written.
    uploading: HashSet<String>,
    /// The project's memory file, once read: an error when it holds
    /// something else than memory JSON.
    memory: Option<Result<Value, String>>,
}

/// A safe file name: no folders, no leading dots.
fn file_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim().trim_start_matches('.').to_owned();
    if name.is_empty() {
        "file".to_owned()
    } else {
        name
    }
}

impl App {
    /// The folder holding the chat projects, beside the Scratch folder.
    fn chat_projects_root(&self) -> Option<String> {
        let scratch = self.scratch_root()?.trim_end_matches('/');
        let (parent, _) = scratch.rsplit_once('/')?;
        Some(format!("{parent}/chat-projects"))
    }

    /// Whether `project` is a chat project.
    pub(crate) fn is_chat_project(&self, project: &Project) -> bool {
        self.chat_projects_root().is_some_and(|root| {
            project
                .workspace_root
                .strip_prefix(&root)
                .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
        })
    }

    /// The chat projects, by name.
    fn chat_project_list(&self) -> Vec<Project> {
        let mut projects: Vec<Project> = self
            .state
            .projects
            .iter()
            .filter(|p| self.is_chat_project(p))
            .cloned()
            .collect();
        projects.sort_by_key(|p| p.title.to_lowercase());
        projects
    }

    /// Whether `thread` is a chat of a project.
    pub(crate) fn is_project_chat_thread(&self, thread: &Thread) -> bool {
        self.state
            .project(&thread.project_id)
            .is_some_and(|p| self.is_chat_project(p))
    }

    /// The chat project of the open chat or draft, if any.
    pub(crate) fn open_chat_project(&self) -> Option<Project> {
        let project = match self.current_thread() {
            Some(thread) => self.state.project(&thread.project_id).cloned(),
            None => self.draft_project(),
        }?;
        self.is_chat_project(&project).then_some(project)
    }

    /// Opens a project's page: a new chat in it, with its instructions,
    /// files and chats.
    pub(crate) fn open_project_page(&mut self, project_id: &str) {
        let Some(project) = self.state.project(project_id).cloned() else {
            return;
        };
        if self.chat_projects.profiled.insert(project.id.clone()) {
            self.send_request(
                format!("chat-project-profile:{}", project.id),
                "projects.writeFile",
                json!({"cwd": project.workspace_root, "relativePath": ".pluto/chat.json",
                    "contents": PROFILE}),
            );
        }
        self.load_project_page(&project);
        self.start_draft(Some(project.id));
    }

    /// Reads a project's instructions and lists its files.
    fn load_project_page(&mut self, project: &Project) {
        self.chat_projects
            .pages
            .entry(project.id.clone())
            .or_default();
        self.send_request(
            format!("chat-project-json:{}", project.id),
            "projects.readFile",
            json!({"cwd": project.workspace_root, "relativePath": PROJECT_PATH}),
        );
        self.send_request(
            format!("chat-project-memory:{}", project.id),
            "projects.readFile",
            json!({"cwd": project.workspace_root, "relativePath": MEMORY_PATH}),
        );
        self.list_project_files(project);
    }

    /// "Delete all", once confirmed.
    pub(crate) fn clear_project_memory(&mut self, project_id: &str) {
        self.forget_project_memory(project_id, None);
    }

    /// Forgets one of a project's memories, or all of them (`None`).
    fn forget_project_memory(&mut self, project_id: &str, memory_id: Option<&str>) {
        self.change_project_memory(project_id, |memory| {
            if let Some(entries) = memory["memories"].as_array_mut() {
                entries.retain(|entry| memory_id.is_some_and(|id| entry["id"] != id));
            }
        });
    }

    /// Changes a project's loaded memory and writes it back.
    pub(crate) fn change_project_memory(
        &mut self,
        project_id: &str,
        change: impl FnOnce(&mut Value),
    ) {
        let Some(project) = self.state.project(project_id).cloned() else {
            return;
        };
        let Some(Some(Ok(memory))) = self
            .chat_projects
            .pages
            .get_mut(&project.id)
            .map(|page| page.memory.as_mut())
        else {
            return;
        };
        change(memory);
        let contents = format!(
            "{}\n",
            serde_json::to_string_pretty(memory).unwrap_or_default()
        );
        self.send_request(
            format!("chat-project-memory-save:{}", project.id),
            "projects.writeFile",
            json!({"cwd": project.workspace_root, "relativePath": MEMORY_PATH,
                "contents": contents}),
        );
    }

    fn list_project_files(&mut self, project: &Project) {
        self.send_request(
            format!("chat-project-files:{}", project.id),
            "projects.listEntries",
            json!({"cwd": project.workspace_root, "directoryPath": FILES}),
        );
    }

    /// Writes a project's `project.json`.
    fn write_project_json(&mut self, project: &Project, name: &str, instructions: &str) {
        let contents = format!(
            "{}\n",
            serde_json::to_string_pretty(
                &json!({"version": 1, "name": name, "instructions": instructions})
            )
            .unwrap_or_default()
        );
        self.send_request(
            format!("chat-project-save:{}", project.id),
            "projects.writeFile",
            json!({"cwd": project.workspace_root, "relativePath": PROJECT_PATH,
                "contents": contents}),
        );
    }

    /// Creates a chat project named `name` and opens its page.
    fn create_chat_project(&mut self, name: &str) {
        let Some(root) = self.chat_projects_root() else {
            self.status = Some("Projects need a connected server with a Scratch folder".into());
            return;
        };
        let folder = format!("{root}/{}", uuid::Uuid::new_v4());
        let command_id = uuid::Uuid::new_v4().to_string();
        let model = self
            .chats_project()
            .and_then(|p| p.default_model_selection)
            .unwrap_or(Value::Null);
        self.chat_projects
            .creating
            .insert(command_id.clone(), (name.to_owned(), folder.clone()));
        self.dispatch(json!({
            "type": "project.create",
            "commandId": command_id,
            "projectId": uuid::Uuid::new_v4().to_string(),
            "title": name,
            "workspaceRoot": folder,
            "createWorkspaceRootIfMissing": true,
            "defaultModelSelection": model,
        }));
    }

    fn rename_chat_project(&mut self, project_id: &str, name: &str) {
        let Some(project) = self.state.project(project_id).cloned() else {
            return;
        };
        self.dispatch(json!({
            "type": "project.update",
            "commandId": uuid::Uuid::new_v4().to_string(),
            "projectId": project.id,
            "title": name,
        }));
        // The chats read the name from project.json.
        let instructions = self
            .chat_projects
            .pages
            .get(project_id)
            .and_then(|page| page.instructions.clone());
        match instructions {
            Some(instructions) => self.write_project_json(&project, name, &instructions),
            None => self.send_request(
                format!("chat-project-rename:{project_id}:{name}"),
                "projects.readFile",
                json!({"cwd": project.workspace_root, "relativePath": PROJECT_PATH}),
            ),
        }
    }

    /// Asks before deleting a project and its chats.
    fn confirm_delete_chat_project(&mut self, project_id: &str) {
        let Some(project) = self.state.project(project_id).cloned() else {
            return;
        };
        self.confirm = Some(crate::ui::thread_menu::Confirm {
            title: format!("Delete project \"{}\"?", project.title),
            message: "Its chats, instructions and memory are deleted with it.".into(),
            confirm_label: "Delete",
            command: json!({
                "type": "project.delete",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": project.id,
                "force": true,
            }),
            more: Vec::new(),
        });
    }

    /// Reads picked files off the UI thread for `project_id`.
    fn pick_project_files(&mut self, project_id: &str, ctx: &egui::Context) {
        let picked = self.chat_projects.picked.clone();
        let project_id = project_id.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let Some(paths) = rfd::FileDialog::new().pick_files() else {
                return;
            };
            for path in paths {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let read = match std::fs::metadata(&path) {
                    Ok(meta) if meta.len() > MAX_FILE_BYTES => Err(format!(
                        "'{name}' is too large for a project (30 MB at most)."
                    )),
                    Ok(_) => std::fs::read(&path)
                        .map_err(|_| format!("'{name}' could not be read."))
                        .map(|bytes| {
                            use base64::Engine;
                            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                            (project_id.clone(), file_name(&name), data)
                        }),
                    Err(_) => Err(format!("'{name}' could not be read.")),
                };
                if let Ok(mut picked) = picked.lock() {
                    picked.push(read);
                }
            }
            ctx.request_repaint();
        });
    }

    fn delete_project_file(&mut self, project_id: &str, name: &str) {
        let Some(project) = self.state.project(project_id).cloned() else {
            return;
        };
        self.send_request(
            format!("chat-project-file-delete:{project_id}"),
            "projects.deleteFile",
            json!({"cwd": project.workspace_root, "relativePath": format!("{FILES}/{name}")}),
        );
        if let Some(files) = self
            .chat_projects
            .pages
            .get_mut(project_id)
            .and_then(|page| page.files.as_mut())
        {
            files.retain(|(file, _)| file != name);
        }
    }

    /// Opens created projects, and writes picked files.
    pub(crate) fn poll_chat_projects(&mut self) {
        if let Some(root) = self.chat_projects.open_root.clone()
            && let Some(project) = self
                .state
                .projects
                .iter()
                .find(|p| p.workspace_root == root)
                .cloned()
        {
            self.chat_projects.open_root = None;
            self.open_project_page(&project.id);
        }
        let picked: Vec<_> = match self.chat_projects.picked.lock() {
            Ok(mut picked) => picked.drain(..).collect(),
            Err(_) => return,
        };
        for file in picked {
            match file {
                Ok((project_id, name, data)) => {
                    let Some(project) = self.state.project(&project_id).cloned() else {
                        continue;
                    };
                    self.chat_projects
                        .pages
                        .entry(project_id.clone())
                        .or_default()
                        .uploading
                        .insert(name.clone());
                    self.send_request(
                        format!("chat-project-file:{project_id}:{name}"),
                        "projects.writeFile",
                        json!({"cwd": project.workspace_root,
                            "relativePath": format!("{FILES}/{name}"),
                            "contents": data, "encoding": "base64"}),
                    );
                }
                Err(error) => self.status = Some(error),
            }
        }
    }

    pub(crate) fn chat_project_result(
        &mut self,
        key: &str,
        result: &Result<Value, String>,
    ) -> bool {
        if let Some(command) = key.strip_prefix("project-mutation:")
            && let Some((name, folder)) = self.chat_projects.creating.remove(command)
        {
            match result {
                Ok(_) => {
                    for (path, contents) in [
                        (".pluto/chat.json", PROFILE.to_owned()),
                        (
                            PROJECT_PATH,
                            format!(
                                "{}\n",
                                serde_json::to_string_pretty(
                                    &json!({"version": 1, "name": name, "instructions": ""})
                                )
                                .unwrap_or_default()
                            ),
                        ),
                    ] {
                        self.send_request(
                            format!("chat-project-setup:{folder}:{path}"),
                            "projects.writeFile",
                            json!({"cwd": folder, "relativePath": path, "contents": contents}),
                        );
                    }
                    self.share_instructions(&folder);
                    self.chat_projects.open_root = Some(folder);
                }
                Err(error) => self.status = Some(format!("Could not create the project: {error}")),
            }
            return true;
        }
        let Some((kind, rest)) = key
            .strip_prefix("chat-project-")
            .and_then(|key| key.split_once(':'))
        else {
            return false;
        };
        match kind {
            "json" => {
                let instructions = result
                    .as_ref()
                    .ok()
                    .and_then(|file| file["contents"].as_str())
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .and_then(|file| file["instructions"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                let page = self.chat_projects.pages.entry(rest.to_owned()).or_default();
                page.draft = instructions.clone();
                page.instructions = Some(instructions);
            }
            "files" => {
                let names: Vec<String> = result
                    .as_ref()
                    .ok()
                    .and_then(|value| value["entries"].as_array())
                    .into_iter()
                    .flatten()
                    .filter(|entry| entry["kind"] == "file")
                    .filter_map(|entry| entry["path"].as_str())
                    .filter_map(|path| path.rsplit('/').next())
                    .filter(|name| !name.starts_with('.'))
                    .map(str::to_owned)
                    .collect();
                if let Some(project) = self.state.project(rest).cloned()
                    && !names.is_empty()
                {
                    let paths: Vec<String> = names
                        .iter()
                        .take(METADATA_BATCH)
                        .map(|name| format!("{}/{FILES}/{name}", project.workspace_root))
                        .collect();
                    self.send_request(
                        format!("chat-project-sizes:{rest}"),
                        "filesystem.getMetadata",
                        json!({"paths": paths}),
                    );
                }
                let page = self.chat_projects.pages.entry(rest.to_owned()).or_default();
                page.files = Some(names.into_iter().map(|name| (name, None)).collect());
            }
            // A missing file is no memory yet.
            "memory" => {
                let memory = match result {
                    Ok(file) => crate::ui::memory_settings::parse(
                        file["contents"].as_str().unwrap_or_default(),
                    ),
                    Err(_) => Ok(json!({"version": 1, "memories": []})),
                };
                self.chat_projects
                    .pages
                    .entry(rest.to_owned())
                    .or_default()
                    .memory = Some(memory);
            }
            "sizes" => {
                if let (Ok(value), Some(files)) = (
                    result,
                    self.chat_projects
                        .pages
                        .get_mut(rest)
                        .and_then(|page| page.files.as_mut()),
                ) {
                    let sizes = value["entries"].as_array().cloned().unwrap_or_default();
                    for ((_, size), entry) in files.iter_mut().zip(sizes) {
                        *size = entry["byteLength"].as_u64();
                    }
                }
            }
            "file" => {
                let (project_id, name) = rest.split_once(':').unwrap_or((rest, ""));
                if let Some(page) = self.chat_projects.pages.get_mut(project_id) {
                    page.uploading.remove(name);
                }
                if let Err(error) = result {
                    self.status = Some(format!("Could not add {name} to the project: {error}"));
                }
                if let Some(project) = self.state.project(project_id).cloned() {
                    self.list_project_files(&project);
                }
            }
            "rename" => {
                let (project_id, name) = rest.split_once(':').unwrap_or((rest, ""));
                let instructions = result
                    .as_ref()
                    .ok()
                    .and_then(|file| file["contents"].as_str())
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .and_then(|file| file["instructions"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                if let Some(project) = self.state.project(project_id).cloned() {
                    self.write_project_json(&project, name, &instructions);
                }
            }
            "save" | "profile" | "setup" | "file-delete" | "memory-save" => {
                if let Err(error) = result {
                    self.status = Some(format!("Could not update the project: {error}"));
                }
            }
            _ => return false,
        }
        true
    }

    /// The sidebar's Projects section, inside the chat list: a heading with
    /// New project, then the projects. Returns whether a project opened.
    pub(crate) fn chat_projects_section(&mut self, ui: &mut Ui, width: f32) {
        let projects = self.chat_project_list();
        let (header, _) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::hover());
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(header.left() + 16.0, header.top() + 8.0),
                header.max,
            ),
            "Projects",
            font(12.0, Weight::Medium),
            theme::sidebar_muted(),
        );
        if widgets::icon_button(
            ui,
            Rect::from_min_size(
                Pos2::new(header.right() - 34.0, header.top() + 4.0),
                Vec2::splat(24.0),
            ),
            "chat-project-new",
            "plus",
            14.0,
            theme::sidebar_icon(),
            "New project",
        )
        .clicked()
        {
            self.chat_projects.dialog = Some((None, String::new(), true));
        }
        let open = self.open_chat_project().map(|p| p.id);
        for project in &projects {
            let (row, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
            let item = Rect::from_min_max(
                Pos2::new(row.left() + 8.0, row.top() + 1.0),
                Pos2::new(row.right() - 8.0, row.bottom() - 1.0),
            );
            if open.as_deref() == Some(project.id.as_str()) {
                ui.painter()
                    .rect_filled(item, CornerRadius::same(8), theme::row_active());
            } else if response.hovered() {
                ui.painter().rect_filled(
                    item,
                    CornerRadius::same(8),
                    theme::alpha(theme::sidebar_foreground(), 0.06),
                );
            }
            theme::paint_icon(
                ui,
                "folder",
                Rect::from_min_size(
                    Pos2::new(item.left() + 10.0, item.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                15.0,
                theme::sidebar_icon(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(item.left() + 34.0, item.top()),
                    Pos2::new(item.right() - 10.0, item.bottom()),
                ),
                &project.title,
                font(14.0, Weight::Regular),
                theme::alpha(theme::sidebar_foreground(), 0.85),
            );
            if response.clicked() {
                self.open_project_page(&project.id);
            }
            if response.secondary_clicked()
                && let Some(at) = response.interact_pointer_pos()
            {
                self.chat_projects.menu = Some((project.id.clone(), context_menu::State::new(at)));
            }
        }
        if projects.is_empty() {
            let (row, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
            let item = row.shrink2(Vec2::new(8.0, 1.0));
            if response.hovered() {
                ui.painter().rect_filled(
                    item,
                    CornerRadius::same(8),
                    theme::alpha(theme::sidebar_foreground(), 0.06),
                );
            }
            theme::paint_icon(
                ui,
                "folder-plus",
                Rect::from_min_size(
                    Pos2::new(item.left() + 10.0, item.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                15.0,
                theme::sidebar_icon(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(item.left() + 34.0, item.top()), item.max),
                "New project",
                font(14.0, Weight::Regular),
                theme::sidebar_muted(),
            );
            if response.clicked() {
                self.chat_projects.dialog = Some((None, String::new(), true));
            }
        }
    }

    /// A project's sidebar menu: Rename, Delete.
    pub(crate) fn chat_project_menu(&mut self, ui: &mut Ui) {
        let Some((project_id, state)) = self.chat_projects.menu.take() else {
            return;
        };
        let items = vec![
            context_menu::Item::new("rename", "Rename project").icon("pencil"),
            context_menu::Item {
                destructive: true,
                ..context_menu::Item::new("delete", "Delete project").icon("trash")
            },
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.chat_projects.menu = Some((project_id.clone(), state));
        }
        match clicked.as_deref() {
            Some("rename") => {
                let name = self
                    .state
                    .project(&project_id)
                    .map(|p| p.title.clone())
                    .unwrap_or_default();
                self.chat_projects.dialog = Some((Some(project_id), name, true));
            }
            Some("delete") => self.confirm_delete_chat_project(&project_id),
            _ => {}
        }
    }

    /// The New / Rename project dialog (the `DialogPopup` of the rename
    /// dialogs).
    pub(crate) fn chat_project_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some((project_id, mut name, focus)) = self.chat_projects.dialog.clone() else {
            return;
        };
        let (title, action) = match project_id {
            Some(_) => ("Rename project", "Save"),
            None => ("New project", "Create"),
        };
        let mut decision = None;
        let dialog = Rect::from_center_size(screen.center(), Vec2::new(512.0, 220.0));
        egui::Area::new(ui.id().with("chat-project-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(dialog) {
                    decision = Some(false);
                }
                ui.painter().rect(
                    dialog,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let left = dialog.left() + 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, dialog.top() + 25.0),
                    title,
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                widgets::text(
                    ui,
                    Pos2::new(left, dialog.top() + 53.0),
                    "Chats in a project share its instructions, files and memory.",
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                );
                let field = Rect::from_min_size(
                    Pos2::new(left, dialog.top() + 90.0),
                    Vec2::new(462.0, 32.0),
                );
                let edit = crate::ui::action_editor::input(
                    ui,
                    field,
                    "chat-project-name",
                    &mut name,
                    "Project name",
                    false,
                );
                if focus {
                    edit.request_focus();
                }
                use crate::ui::action_editor::{Kind, footer_button, footer_rect};
                let top = dialog.bottom() - 25.0 - 32.0;
                let confirm = footer_rect(ui, dialog.right() - 25.0, top, action);
                let cancel = footer_rect(ui, confirm.left() - 8.0, top, "Cancel");
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if footer_button(ui, confirm, action, Kind::Primary) || enter {
                    decision = Some(true);
                }
                if footer_button(ui, cancel, "Cancel", Kind::Outline)
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    decision = Some(false);
                }
            });
        let trimmed = name.trim().to_owned();
        match decision {
            Some(true) if trimmed.is_empty() => {
                self.status = Some("A project needs a name".into());
                self.chat_projects.dialog = Some((project_id, name, true));
            }
            Some(true) => {
                self.chat_projects.dialog = None;
                match project_id {
                    Some(id) => self.rename_chat_project(&id, &trimmed),
                    None => self.create_chat_project(&trimmed),
                }
            }
            Some(false) => self.chat_projects.dialog = None,
            None => self.chat_projects.dialog = Some((project_id, name, false)),
        }
    }

    /// A project's page: its name, the composer for a new chat in it, then
    /// its instructions, files and chats.
    pub(crate) fn project_page(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        project: &Project,
        left: f32,
        width: f32,
    ) {
        if !self.chat_projects.pages.contains_key(&project.id) {
            self.load_project_page(project);
        }
        let title_top = body.top() + 40.0;
        theme::paint_icon(
            ui,
            "folder",
            Rect::from_min_size(Pos2::new(left, title_top + 2.0), Vec2::splat(24.0)),
            22.0,
            theme::muted_foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(left + 34.0, title_top),
                Vec2::new(width - 34.0, 28.0),
            ),
            &project.title,
            font(24.0, Weight::SemiBold),
            theme::foreground(),
        );
        let mode = crate::ui::composer::Mode::Normal;
        let composer_top = title_top + 56.0;
        let composer_height =
            body.bottom() - 52.0 - self.composer_layout_top(ui, body, width, &mode);
        self.composer(ui, Pos2::new(left, composer_top), width, true, &mode);
        // The checkout strip hangs under the composer of a repository.
        let strip = if self.workspace_is_repo() { 32.0 } else { 0.0 };
        let area = Rect::from_min_max(
            Pos2::new(left - 8.0, composer_top + composer_height + strip + 28.0),
            Pos2::new(left + width + 8.0, body.bottom()),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        egui::ScrollArea::vertical()
            .id_salt(("project-page", &project.id))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let top = ui.max_rect().top();
                let mut y = top;
                y = self.project_instructions_card(ui, project, left, y, width) + 24.0;
                y = self.project_files_card(ui, project, left, y, width) + 24.0;
                y = self.project_memory_card(ui, project, left, y, width) + 24.0;
                y = self.project_chats(ui, project, left, y, width);
                ui.allocate_rect(
                    Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + width, y + 32.0)),
                    Sense::hover(),
                );
            });
    }

    fn card(ui: &Ui, rect: Rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(14),
            crate::ui::settings::card_fill(),
            Stroke::new(1.0, crate::ui::settings::card_border()),
            StrokeKind::Inside,
        );
    }

    fn card_heading(ui: &Ui, left: f32, top: f32, title: &str, description: &str, width: f32) {
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(left + 16.0, top + 14.0),
                Vec2::new(width - 32.0, 20.0),
            ),
            title,
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(left + 16.0, top + 34.0),
                Vec2::new(width - 160.0, 18.0),
            ),
            description,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
        );
    }

    /// The project's instructions, edited in place. Returns the card's bottom.
    fn project_instructions_card(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        let field_height = 5.0 * 20.0 + 16.0;
        let card = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new(width, 62.0 + field_height + 16.0 + 32.0 + 16.0),
        );
        Self::card(ui, card);
        Self::card_heading(
            ui,
            left,
            top,
            "Instructions",
            "How the chats in this project should answer, and what they should know.",
            width,
        );
        let Some(page) = self.chat_projects.pages.get_mut(&project.id) else {
            return card.bottom();
        };
        let Some(saved) = page.instructions.clone() else {
            return card.bottom();
        };
        let field = Rect::from_min_size(
            Pos2::new(left + 16.0, top + 62.0),
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
            .id_salt(("project-instructions", &project.id))
            .max_height(inner.height())
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut page.draft)
                        .id_salt(("project-instructions-edit", &project.id))
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font(14.0, Weight::Regular))
                        .text_color(theme::foreground())
                        .hint_text("For example: answer in French, cite the files you use.")
                        .desired_width(inner.width())
                        .desired_rows(5),
                );
            });
        let changed = page.draft.trim() != saved.trim();
        let draft = page.draft.trim().to_owned();
        use crate::ui::action_editor::{Kind, footer_button, footer_rect};
        let save = footer_rect(ui, card.right() - 16.0, card.bottom() - 16.0 - 32.0, "Save");
        if footer_button(
            ui,
            save,
            "Save",
            if changed {
                Kind::Primary
            } else {
                Kind::Outline
            },
        ) && changed
        {
            if let Some(page) = self.chat_projects.pages.get_mut(&project.id) {
                page.instructions = Some(draft.clone());
            }
            self.write_project_json(project, &project.title, &draft);
        }
        card.bottom()
    }

    /// The project's reference files, with Add files and a remove button
    /// per file. Returns the card's bottom.
    fn project_files_card(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        let page = self.chat_projects.pages.get(&project.id);
        let files = page.and_then(|p| p.files.clone()).unwrap_or_default();
        let uploading: Vec<String> = page
            .map(|p| p.uploading.iter().cloned().collect())
            .unwrap_or_default();
        let rows = files.len() + uploading.len();
        let card = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new(width, 62.0 + (rows.max(1) as f32) * 36.0 + 12.0),
        );
        Self::card(ui, card);
        Self::card_heading(
            ui,
            left,
            top,
            "Files",
            "Documents the chats in this project read when they bear on the question.",
            width,
        );
        use crate::ui::action_editor::{Kind, footer_button, footer_rect};
        let add = footer_rect(ui, card.right() - 16.0, top + 14.0, "Add files");
        if footer_button(ui, add, "Add files", Kind::Outline) {
            let ctx = ui.ctx().clone();
            self.pick_project_files(&project.id, &ctx);
        }
        let mut y = top + 62.0;
        if rows == 0 {
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(width - 32.0, 36.0)),
                "No files yet.",
                font(13.0, Weight::Regular),
                theme::muted_foreground(),
            );
        }
        let mut remove = None;
        for (name, size) in &files {
            let row = Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(width - 32.0, 36.0));
            theme::paint_icon(
                ui,
                "file-text",
                Rect::from_min_size(
                    Pos2::new(row.left(), row.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                15.0,
                theme::muted_foreground(),
            );
            let detail = size
                .map(crate::ui::attachments::format_size)
                .unwrap_or_default();
            let detail_width = widgets::text_width(ui, &detail, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.left() + 26.0, row.top()),
                    Pos2::new(row.right() - 48.0 - detail_width, row.bottom()),
                ),
                name,
                font(14.0, Weight::Regular),
                theme::foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.right() - 36.0 - detail_width, row.top()),
                    Pos2::new(row.right() - 32.0, row.bottom()),
                ),
                &detail,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            if widgets::icon_button(
                ui,
                Rect::from_min_size(
                    Pos2::new(row.right() - 26.0, row.center().y - 12.0),
                    Vec2::splat(24.0),
                ),
                ("project-file-remove", &project.id, name),
                "x",
                14.0,
                theme::muted_foreground(),
                &format!("Remove {name}"),
            )
            .clicked()
            {
                remove = Some(name.clone());
            }
            y += 36.0;
        }
        for name in &uploading {
            let row = Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(width - 32.0, 36.0));
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(row.left() + 26.0, row.top()), row.max),
                &format!("{name} · Uploading…"),
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
            );
            y += 36.0;
        }
        if let Some(name) = remove {
            self.delete_project_file(&project.id, &name);
        }
        card.bottom()
    }

    /// The project's memory, newest first, each with a remove button, and
    /// Delete all. Returns the card's bottom.
    fn project_memory_card(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        let memory = self
            .chat_projects
            .pages
            .get(&project.id)
            .and_then(|page| page.memory.clone());
        let entries: Vec<Value> = match &memory {
            Some(Ok(memory)) => memory["memories"].as_array().cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        let card = Rect::from_min_size(
            Pos2::new(left, top),
            Vec2::new(width, 62.0 + (entries.len().max(1) as f32) * 36.0 + 12.0),
        );
        Self::card(ui, card);
        Self::card_heading(
            ui,
            left,
            top,
            "Memory",
            "What the chats in this project remember, apart from your other chats.",
            width,
        );
        use crate::ui::action_editor::{Kind, footer_button, footer_rect};
        let mut right = card.right() - 16.0;
        if !entries.is_empty() {
            let clear = footer_rect(ui, right, top + 14.0, "Delete all");
            right = clear.left() - 8.0;
            if footer_button(ui, clear, "Delete all", Kind::Destructive) {
                self.confirm = Some(crate::ui::thread_menu::Confirm {
                    title: format!("Delete the memory of \"{}\"?", project.title),
                    message: "The chats in this project will forget everything they saved. \
                              This cannot be undone."
                        .into(),
                    confirm_label: "Delete all",
                    command: json!({"type": CLEAR_MEMORY_COMMAND, "projectId": project.id}),
                    more: Vec::new(),
                });
            }
        }
        if matches!(memory, Some(Ok(_))) {
            let add = footer_rect(ui, right, top + 14.0, "Add");
            if footer_button(ui, add, "Add", Kind::Outline) {
                self.edit_memory(
                    crate::ui::memory_settings::MemoryScope::Project(project.id.clone()),
                    "",
                    "",
                );
            }
        }
        let mut y = top + 62.0;
        let empty = match &memory {
            None => Some("Loading…".to_owned()),
            Some(Err(error)) => Some(error.clone()),
            Some(Ok(_)) if entries.is_empty() => {
                Some("Nothing remembered yet. Ask a chat here to remember something.".to_owned())
            }
            Some(Ok(_)) => None,
        };
        if let Some(text) = empty {
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(width - 32.0, 36.0)),
                &text,
                font(13.0, Weight::Regular),
                theme::muted_foreground(),
            );
        }
        let mut forget = None;
        let mut edit = None;
        for entry in entries.iter().rev() {
            let id = entry["id"].as_str().unwrap_or_default().to_owned();
            let text = entry["text"].as_str().unwrap_or_default();
            let row = Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(width - 32.0, 36.0));
            theme::paint_icon(
                ui,
                "brain",
                Rect::from_min_size(
                    Pos2::new(row.left(), row.center().y - 8.0),
                    Vec2::splat(16.0),
                ),
                15.0,
                theme::muted_foreground(),
            );
            let text_rect = Rect::from_min_max(
                Pos2::new(row.left() + 26.0, row.top()),
                Pos2::new(row.right() - 60.0, row.bottom()),
            );
            if widgets::icon_button(
                ui,
                Rect::from_min_size(
                    Pos2::new(row.right() - 54.0, row.center().y - 12.0),
                    Vec2::splat(24.0),
                ),
                ("project-memory-edit", &project.id, &id),
                "pencil",
                14.0,
                theme::muted_foreground(),
                "Edit",
            )
            .clicked()
            {
                edit = Some((id.clone(), text.to_owned()));
            }
            widgets::text_in(
                ui,
                text_rect,
                text,
                font(14.0, Weight::Regular),
                theme::foreground(),
            );
            // The whole memory on hover, when the row cuts it.
            ui.interact(
                text_rect,
                ui.id().with(("project-memory", &id)),
                Sense::hover(),
            )
            .on_hover_text(text);
            if widgets::icon_button(
                ui,
                Rect::from_min_size(
                    Pos2::new(row.right() - 26.0, row.center().y - 12.0),
                    Vec2::splat(24.0),
                ),
                ("project-memory-forget", &project.id, &id),
                "x",
                14.0,
                theme::muted_foreground(),
                "Forget",
            )
            .clicked()
            {
                forget = Some(id);
            }
            y += 36.0;
        }
        if let Some(id) = forget {
            self.forget_project_memory(&project.id, Some(&id));
        }
        if let Some((id, text)) = edit {
            self.edit_memory(
                crate::ui::memory_settings::MemoryScope::Project(project.id.clone()),
                &id,
                &text,
            );
        }
        card.bottom()
    }

    /// The project's chats, newest first. Returns where the list ends.
    fn project_chats(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        left: f32,
        top: f32,
        width: f32,
    ) -> f32 {
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(left + 16.0, top), Vec2::new(width, 28.0)),
            "Chats",
            font(14.0, Weight::Regular),
            theme::alpha(theme::foreground(), 0.7),
        );
        let chats: Vec<Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| t.project_id == project.id && t.archived_at.is_none() && !t.is_subagent())
            .cloned()
            .collect();
        let mut chats: Vec<Arc<Thread>> = self
            .chat_conversations(chats)
            .into_iter()
            .map(|(thread, _)| thread)
            .collect();
        let at = |t: &Thread| {
            t.latest_user_message_at
                .clone()
                .unwrap_or_else(|| t.updated_at.clone())
        };
        chats.sort_by_key(|t| std::cmp::Reverse(at(t)));
        let mut y = top + 34.0;
        if chats.is_empty() {
            widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(left + 16.0, y), Vec2::new(width - 32.0, 20.0)),
                "The chats you start here will appear here.",
                font(13.0, Weight::Regular),
                theme::muted_foreground(),
            );
            return y + 20.0;
        }
        let mut open = None;
        for thread in &chats {
            let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 44.0));
            let response = ui.interact(
                row,
                ui.id().with(("project-chat", &thread.id)),
                Sense::click(),
            );
            if response.hovered() {
                ui.painter().rect_filled(
                    row,
                    CornerRadius::same(10),
                    theme::alpha(theme::foreground(), 0.04),
                );
            }
            let when = crate::timeline::parse_time(&at(thread))
                .map(|time| {
                    time.with_timezone(&chrono::Local)
                        .format("%-d %b")
                        .to_string()
                })
                .unwrap_or_default();
            let when_width = widgets::text_width(ui, &when, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.left() + 16.0, row.top()),
                    Pos2::new(row.right() - 28.0 - when_width, row.bottom()),
                ),
                &thread.title,
                font(14.0, Weight::Regular),
                theme::foreground(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.right() - 16.0 - when_width, row.top()),
                    Pos2::new(row.right() - 14.0, row.bottom()),
                ),
                &when,
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
            );
            if response.clicked() {
                open = Some(thread.id.clone());
            }
            y += 44.0;
        }
        if let Some(id) = open {
            self.select_thread(Some(id));
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_files_keep_safe_names() {
        assert_eq!(file_name("bail 2026.pdf"), "bail 2026.pdf");
        assert_eq!(file_name("../../etc/passwd"), "-..-etc-passwd");
        assert_eq!(file_name(".env"), "env");
        assert_eq!(file_name("..."), "file");
    }
}
