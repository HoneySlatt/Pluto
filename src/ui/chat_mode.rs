//! Pluto's Chat mode (not upstream): the app as a chat client for the
//! user's subscriptions, apart from coding, after LibreChat and the
//! providers' own chat sites.
//!
//! Chats live in one hidden project, the chat space: a `chats` folder beside
//! the server's Scratch folder, holding `.pluto/chat.json`, whose sessions
//! the server gives its chat charter, the saved memories and closed tools.
//! The sidebar's Code | Chat switch splits the app: Code lists every thread
//! but the chat space's, Chat lists only those, by date, and opens new chats
//! on a "How can I help?" draft. Chat threads run in approval-required mode,
//! so an agent that tried to write or run something would ask first.
//!
//! The sidebar's search matches titles and, through the server's
//! `orchestration.searchThreads` kept to the chat space, the messages, whose
//! matching excerpt shows under the chat's title.
//!
//! Temporary chats, after ChatGPT's, live in a second hidden project beside
//! it (`temporary-chats`), whose profile tells the server to leave memory
//! out. They are listed nowhere and deleted as soon as the user leaves them.

use std::collections::HashSet;

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Project, Thread};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const PROFILE_PATH: &str = ".pluto/chat.json";
const ROW: f32 = 32.0;
/// A row with a message excerpt under its title.
const MATCH_ROW: f32 = 50.0;
/// The pause in typing before the messages are searched.
const SEARCH_DELAY: std::time::Duration = std::time::Duration::from_millis(300);
const GROUP: f32 = 28.0;

/// One of the two hidden chat projects.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Space {
    Chats,
    Temporary,
}

impl Space {
    const ALL: [Space; 2] = [Space::Chats, Space::Temporary];

    fn index(self) -> usize {
        self as usize
    }

    /// The folder beside the Scratch folder.
    fn folder(self) -> &'static str {
        match self {
            Space::Chats => "chats",
            Space::Temporary => "temporary-chats",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Space::Chats => "Chats",
            Space::Temporary => "Temporary chats",
        }
    }

    fn profile(self) -> &'static str {
        match self {
            Space::Chats => "{\n  \"version\": 1\n}\n",
            Space::Temporary => "{\n  \"version\": 1,\n  \"temporary\": true\n}\n",
        }
    }
}

/// A chat project's setup this run.
#[derive(Default)]
struct SpaceState {
    /// Being created: its `project.create` command id.
    creating: Option<String>,
    /// Its profile was written this run.
    profile_written: bool,
    /// Open a new chat in it once it exists.
    pending_draft: bool,
}

#[derive(Default)]
pub(crate) struct ChatMode {
    spaces: [SpaceState; 2],
    /// The thread each mode showed last: (code, chat).
    last: (Option<String>, Option<String>),
    /// Temporary chats already sent `thread.delete`.
    deleted: HashSet<String>,
    search: ChatSearch,
}

/// The sidebar's message search.
#[derive(Default)]
struct ChatSearch {
    /// The query being typed, and since when.
    typed: Option<(String, std::time::Instant)>,
    /// The query last sent, and its matches: thread and excerpt.
    asked: String,
    matches: Vec<(String, String)>,
}

/// An excerpt starting shortly before `query`, so the narrow sidebar shows
/// the match.
fn excerpt_around(snippet: &str, query: &str) -> String {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let chars: Vec<char> = snippet.chars().collect();
    let lower: Vec<char> = chars.iter().copied().map(fold).collect();
    let needle: Vec<char> = query.chars().map(fold).collect();
    if needle.is_empty() || needle.len() > lower.len() {
        return snippet.to_owned();
    }
    match lower
        .windows(needle.len())
        .position(|w| w == needle.as_slice())
    {
        Some(at) if at > 16 => {
            // From the first word start within a dozen characters before it.
            let mut start = at - 12;
            if let Some(space) = chars[start..at].iter().position(|c| c.is_whitespace()) {
                start += space + 1;
            }
            let rest: String = chars[start..].iter().collect();
            format!("…{}", rest.trim_start_matches('…'))
        }
        _ => snippet.to_owned(),
    }
}

/// A chat's day group, from the time of its last message.
fn day_group(at: &str, now: chrono::DateTime<chrono::Local>) -> &'static str {
    let Some(at) = crate::timeline::parse_time(at) else {
        return "Older";
    };
    let days = (now.date_naive() - at.with_timezone(&chrono::Local).date_naive()).num_days();
    match days {
        ..=0 => "Today",
        1 => "Yesterday",
        2..=7 => "Previous 7 days",
        8..=30 => "Previous 30 days",
        _ => "Older",
    }
}

impl App {
    /// The sidebar's Chat mode is on.
    pub(crate) fn chat_mode(&self) -> bool {
        self.client_settings["plutoMode"] == "chat"
    }

    /// A chat project's folder, beside the Scratch folder.
    fn space_root(&self, space: Space) -> Option<String> {
        let scratch = self.scratch_root()?.trim_end_matches('/');
        let (parent, _) = scratch.rsplit_once('/')?;
        Some(format!("{parent}/{}", space.folder()))
    }

    /// Which chat project `project` is, if any.
    fn space_of(&self, project: &Project) -> Option<Space> {
        let root = project.workspace_root.trim_end_matches('/');
        Space::ALL
            .into_iter()
            .find(|space| self.space_root(*space).is_some_and(|r| r == root))
    }

    /// Whether `project` holds chats (the chat space or temporary chats).
    pub(crate) fn is_chats(&self, project: &Project) -> bool {
        self.space_of(project).is_some()
    }

    /// Whether `project` holds temporary chats.
    pub(crate) fn is_temporary_chats(&self, project: &Project) -> bool {
        self.space_of(project) == Some(Space::Temporary)
    }

    fn space_project(&self, space: Space) -> Option<Project> {
        self.state
            .projects
            .iter()
            .find(|p| self.space_of(p) == Some(space))
            .cloned()
    }

    /// The chat space, whose chats are kept.
    pub(crate) fn chats_project(&self) -> Option<Project> {
        self.space_project(Space::Chats)
    }

    /// Whether `thread` is a chat, temporary or not.
    pub(crate) fn is_chat_thread(&self, thread: &Thread) -> bool {
        self.state
            .project(&thread.project_id)
            .is_some_and(|p| self.is_chats(p))
    }

    /// Whether `thread` is a temporary chat.
    pub(crate) fn is_temporary_thread(&self, thread: &Thread) -> bool {
        self.state
            .project(&thread.project_id)
            .is_some_and(|p| self.is_temporary_chats(p))
    }

    /// Whether `thread` belongs to the mode the sidebar shows; temporary
    /// chats belong to none.
    pub(crate) fn in_mode(&self, thread: &Thread) -> bool {
        !self.is_temporary_thread(thread) && self.is_chat_thread(thread) == self.chat_mode()
    }

    /// Whether the open thread or draft is a chat.
    pub(crate) fn chat_context(&self, draft: bool) -> bool {
        if draft {
            self.draft_project().is_some_and(|p| self.is_chats(&p))
        } else {
            self.current_thread()
                .is_some_and(|t| self.is_chat_thread(&t))
        }
    }

    /// Whether the open thread or draft is a temporary chat.
    pub(crate) fn temporary_context(&self) -> bool {
        match self.current_thread() {
            Some(thread) => self.is_temporary_thread(&thread),
            None => self
                .draft_project()
                .is_some_and(|p| self.is_temporary_chats(&p)),
        }
    }

    /// Switches between Code and Chat, each mode coming back to the thread it
    /// showed last (a new chat when Chat had none).
    pub(crate) fn set_chat_mode(&mut self, chat: bool) {
        if chat == self.chat_mode() {
            return;
        }
        let current = self.selected.clone();
        if chat {
            self.chat_mode.last.0 = current;
        } else {
            self.chat_mode.last.1 = current;
        }
        self.client_settings["plutoMode"] = json!(if chat { "chat" } else { "code" });
        self.save_client_settings();
        let back = if chat {
            self.chat_mode.last.1.clone()
        } else {
            self.chat_mode.last.0.clone()
        };
        let back = back.filter(|id| self.state.threads.iter().any(|t| &t.id == id));
        match back {
            Some(id) => self.select_thread(Some(id)),
            None if chat => self.new_chat(),
            None => self.select_thread(None),
        }
    }

    /// Opens a new chat, creating the chat space the first time.
    pub(crate) fn new_chat(&mut self) {
        self.open_space(Space::Chats);
    }

    /// Opens a new temporary chat, or a regular one from a temporary draft.
    pub(crate) fn toggle_temporary_chat(&mut self) {
        if self.temporary_context() {
            self.open_space(Space::Chats);
        } else {
            self.open_space(Space::Temporary);
        }
    }

    /// Opens a new chat in `space`, creating it the first time.
    fn open_space(&mut self, space: Space) {
        let Some(root) = self.space_root(space) else {
            self.status = Some("Chats need a connected server with a Scratch folder".into());
            return;
        };
        match self.space_project(space) {
            Some(project) => {
                if !self.chat_mode.spaces[space.index()].profile_written {
                    self.write_chat_profile(space, &root);
                }
                self.start_draft(Some(project.id));
            }
            None => {
                let state = &mut self.chat_mode.spaces[space.index()];
                if state.creating.is_some() {
                    return;
                }
                let command_id = uuid::Uuid::new_v4().to_string();
                state.creating = Some(command_id.clone());
                state.pending_draft = true;
                // Temporary chats start on the chat space's model.
                let model = match space {
                    Space::Chats => Value::Null,
                    Space::Temporary => self
                        .chats_project()
                        .and_then(|p| p.default_model_selection)
                        .unwrap_or(Value::Null),
                };
                self.select_thread(None);
                self.dispatch(json!({
                    "type": "project.create",
                    "commandId": command_id,
                    "projectId": uuid::Uuid::new_v4().to_string(),
                    "title": space.title(),
                    "workspaceRoot": root,
                    "createWorkspaceRootIfMissing": true,
                    "defaultModelSelection": model,
                }));
            }
        }
    }

    fn write_chat_profile(&mut self, space: Space, root: &str) {
        self.chat_mode.spaces[space.index()].profile_written = true;
        self.send_request(
            format!("chat-profile:{}", space.index()),
            "projects.writeFile",
            json!({"cwd": root, "relativePath": PROFILE_PATH, "contents": space.profile()}),
        );
        // Temporary chats follow the custom instructions too.
        if space == Space::Temporary {
            self.share_instructions();
        }
    }

    /// The chats whose messages match the sidebar's search, best match
    /// first, with the excerpt; searched once typing pauses.
    fn search_chat_messages(&mut self, ctx: &egui::Context) -> Vec<(String, String)> {
        let query = self.search.trim().to_owned();
        let search = &mut self.chat_mode.search;
        if query.chars().count() < 2 {
            search.typed = None;
            search.asked.clear();
            search.matches.clear();
            return Default::default();
        }
        if query != search.asked {
            match &search.typed {
                Some((typed, since)) if *typed == query => {
                    if since.elapsed() >= SEARCH_DELAY {
                        search.typed = None;
                        search.asked = query.clone();
                        search.matches.clear();
                        if let Some(chats) = self.chats_project() {
                            self.send_request(
                                format!("chat-search:{query}"),
                                "orchestration.searchThreads",
                                json!({"query": query, "limit": 50, "projectIds": [chats.id]}),
                            );
                        }
                    } else {
                        ctx.request_repaint_after(SEARCH_DELAY);
                    }
                }
                _ => {
                    search.typed = Some((query, std::time::Instant::now()));
                    ctx.request_repaint_after(SEARCH_DELAY);
                }
            }
        }
        self.chat_mode.search.matches.clone()
    }

    /// The chat projects' creation and profile answers.
    pub(crate) fn chat_mode_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if let Some(query) = key.strip_prefix("chat-search:") {
            // Only the search still asked counts.
            if query == self.chat_mode.search.asked {
                self.chat_mode.search.matches = result
                    .as_ref()
                    .ok()
                    .and_then(|value| value["matches"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|m| {
                        Some((
                            m["threadId"].as_str()?.to_owned(),
                            m["snippet"].as_str().unwrap_or_default().to_owned(),
                        ))
                    })
                    .collect();
            }
            return true;
        }
        if let Some(index) = key.strip_prefix("chat-profile:") {
            if let Err(error) = result {
                if let Some(state) = index
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| self.chat_mode.spaces.get_mut(i))
                {
                    state.profile_written = false;
                }
                self.status = Some(format!("Could not prepare the chat space: {error}"));
            }
            return true;
        }
        let Some(space) = Space::ALL.into_iter().find(|space| {
            self.chat_mode.spaces[space.index()]
                .creating
                .as_ref()
                .is_some_and(|command| key == format!("project-mutation:{command}"))
        }) else {
            return false;
        };
        let state = &mut self.chat_mode.spaces[space.index()];
        state.creating = None;
        match result {
            Ok(_) => {
                if let Some(root) = self.space_root(space) {
                    self.write_chat_profile(space, &root);
                }
            }
            Err(error) => {
                state.pending_draft = false;
                self.status = Some(format!("Could not create the chat space: {error}"));
            }
        }
        true
    }

    /// Opens the new chat once a created chat project reaches the shell, and
    /// deletes the temporary chats the user left.
    pub(crate) fn poll_chat_mode(&mut self) {
        self.poll_chat_versions();
        self.poll_chat_export();
        for space in Space::ALL {
            if self.chat_mode.spaces[space.index()].pending_draft
                && let Some(project) = self.space_project(space)
            {
                self.chat_mode.spaces[space.index()].pending_draft = false;
                self.start_draft(Some(project.id));
            }
        }
        if !self.connected() {
            return;
        }
        // The open chat keeps its versions.
        let open = self.selected.as_deref().map(|id| self.version_root(id));
        let left: Vec<String> = self
            .state
            .threads
            .iter()
            .filter(|t| {
                // A subagent goes with its chat.
                !t.is_subagent()
                    && !self.chat_mode.deleted.contains(&t.id)
                    && self.is_temporary_thread(t)
                    && open.as_deref() != Some(self.version_root(&t.id).as_str())
            })
            .map(|t| t.id.clone())
            .collect();
        for id in left {
            self.dispatch(crate::model::command("thread.delete", &id));
            self.chat_mode.deleted.insert(id);
        }
    }

    /// A chat's header: its title, and the panel controls while the panel
    /// is closed.
    pub(crate) fn chat_header(&mut self, ui: &mut Ui, rect: Rect, title: &str) {
        let left = self.titlebar_left(rect);
        let panel_header = self.panel_open() && self.current_thread().is_some();
        let mut right = if panel_header {
            rect.right() - theme::WORKSPACE_GUTTER
        } else {
            self.panel_controls(ui, rect.right());
            rect.right() - 13.0 - 3.0 * 32.0
        };
        let temporary = self.temporary_context();
        // A new chat can be made temporary.
        if self.current_thread().is_none() {
            let button = Rect::from_min_size(
                Pos2::new(right - 32.0, rect.center().y - 14.0),
                Vec2::splat(28.0),
            );
            if temporary {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(8), theme::accent());
            }
            let tip = if temporary {
                "Turn off temporary chat"
            } else {
                "Temporary chat"
            };
            if widgets::icon_button(
                ui,
                button,
                "temporary-chat",
                "message-square-dashed",
                16.0,
                if temporary {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
                tip,
            )
            .clicked()
            {
                self.toggle_temporary_chat();
            }
            right = button.left() - 4.0;
        }
        // An open chat can be exported.
        if let Some(thread) = self.current_thread() {
            let button = Rect::from_min_size(
                Pos2::new(right - 32.0, rect.center().y - 14.0),
                Vec2::splat(28.0),
            );
            if widgets::icon_button(
                ui,
                button,
                "export-chat",
                "download",
                16.0,
                theme::muted_foreground(),
                "Export as Markdown",
            )
            .clicked()
            {
                self.request_chat_export(&thread.id);
            }
            right = button.left() - 4.0;
        }
        let title = if temporary { "Temporary chat" } else { title };
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, rect.top()),
                Pos2::new(right - 8.0, rect.bottom()),
            ),
            title,
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
    }

    /// The header's Code | Chat switch, right of the wordmark.
    pub(crate) fn mode_switch(&mut self, ui: &mut Ui, rect: Rect) {
        let track = Rect::from_min_size(
            Pos2::new(rect.left() + 247.0 - 104.0, 14.0),
            Vec2::new(104.0, 24.0),
        );
        ui.painter().rect_filled(
            track,
            CornerRadius::same(8),
            theme::alpha(theme::sidebar_foreground(), 0.06),
        );
        let chat = self.chat_mode();
        for (index, label) in ["Code", "Chat"].into_iter().enumerate() {
            let segment = Rect::from_min_size(
                track.min + Vec2::new(2.0 + 50.0 * index as f32, 2.0),
                Vec2::new(50.0, 20.0),
            );
            let active = (index == 1) == chat;
            let response = ui.interact(segment, ui.id().with(("mode", index)), Sense::click());
            if active {
                ui.painter()
                    .rect_filled(segment, CornerRadius::same(6), theme::accent());
            }
            let color = if active || response.hovered() {
                theme::sidebar_foreground()
            } else {
                theme::sidebar_muted()
            };
            let width = widgets::text_width(ui, label, font(12.0, Weight::Medium));
            widgets::text_in(
                ui,
                Rect::from_center_size(segment.center(), Vec2::new(width + 1.0, 16.0)),
                label,
                font(12.0, Weight::Medium),
                color,
            );
            if response.clicked() {
                self.set_chat_mode(index == 1);
            }
        }
    }

    /// Chat mode's sidebar: search, a new chat, and the chats by day.
    pub(crate) fn chat_sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let left = rect.left();
        let search_rect = Rect::from_min_size(Pos2::new(left + 8.0, 60.0), Vec2::new(207.0, 32.0));
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 16.0, 68.0), Vec2::splat(16.0)),
            16.0,
            theme::sidebar_icon(),
        );
        let input_rect = Rect::from_min_size(
            Pos2::new(left + 40.0, 65.5),
            Vec2::new(search_rect.width() - 40.0, 21.0),
        );
        let mut search_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        search_ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::sidebar_foreground())
                .hint_text(
                    egui::RichText::new("Search chats")
                        .font(font(14.0, Weight::Medium))
                        .color(theme::sidebar_muted()),
                )
                .desired_width(input_rect.width()),
        );
        if widgets::icon_button(
            ui,
            Rect::from_min_size(Pos2::new(left + 219.0, 62.0), Vec2::splat(28.0)),
            "chat-new",
            "square-pen",
            16.0,
            theme::sidebar_icon(),
            "New chat",
        )
        .clicked()
        {
            self.new_chat();
        }

        // The chats, newest first, by day.
        let query = self.search.trim().to_lowercase();
        let roots = if query.is_empty() {
            Default::default()
        } else {
            self.version_roots()
        };
        let root = |id: &str| roots.get(id).map_or(id, String::as_str).to_owned();
        // A conversation shows its best-ranked excerpt.
        let excerpts: std::collections::HashMap<String, String> = self
            .search_chat_messages(ui.ctx())
            .into_iter()
            .rev()
            .map(|(thread, excerpt)| (root(&thread), excerpt))
            .collect();
        let chats: Vec<std::sync::Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none() && !t.is_subagent() && self.in_mode(t))
            .cloned()
            .collect();
        // One entry per conversation, whatever its versions; a search keeps
        // those whose title or messages match.
        let mut conversations = self.chat_conversations(chats);
        if !query.is_empty() {
            conversations.retain(|(t, _)| {
                t.title.to_lowercase().contains(&query) || excerpts.contains_key(&root(&t.id))
            });
        }
        let excerpt_of = |t: &Thread| {
            excerpts
                .get(&root(&t.id))
                .filter(|_| !t.title.to_lowercase().contains(&query))
                .map(|excerpt| excerpt_around(excerpt, &query))
        };
        let excerpts: std::collections::HashMap<String, String> = conversations
            .iter()
            .filter_map(|(t, _)| Some((t.id.clone(), excerpt_of(t)?)))
            .collect();
        let at = |t: &Thread| {
            t.latest_user_message_at
                .clone()
                .unwrap_or_else(|| t.updated_at.clone())
        };
        // Pinned chats lead, under their own heading.
        let pinned: std::collections::HashSet<String> = conversations
            .iter()
            .filter(|(_, pinned)| *pinned)
            .map(|(t, _)| t.id.clone())
            .collect();
        let mut chats: Vec<std::sync::Arc<Thread>> =
            conversations.into_iter().map(|(t, _)| t).collect();
        chats.sort_by_key(|t| (!pinned.contains(&t.id), std::cmp::Reverse(at(t))));
        let now = chrono::Local::now();
        let list = Rect::from_min_max(
            Pos2::new(left, 100.0),
            Pos2::new(rect.right() - 1.0, rect.bottom() - 48.0),
        );
        if chats.is_empty() {
            let text = if query.is_empty() {
                "No chats yet"
            } else {
                "No matching chats"
            };
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(left + 16.0, list.top() + 8.0),
                    Vec2::new(220.0, 20.0),
                ),
                text,
                font(13.0, Weight::Regular),
                theme::sidebar_muted(),
            );
            return;
        }
        let mut open = None;
        let mut menu = None;
        let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical()
            .id_salt("chat-list")
            .auto_shrink([false, false])
            .show(&mut list_ui, |ui| {
                let mut group = "";
                for thread in &chats {
                    let label = if pinned.contains(&thread.id) {
                        "Pinned"
                    } else {
                        day_group(&at(thread), now)
                    };
                    if label != group {
                        group = label;
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(list.width(), GROUP), Sense::hover());
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(
                                Pos2::new(rect.left() + 16.0, rect.top() + 8.0),
                                rect.max,
                            ),
                            label,
                            font(12.0, Weight::Medium),
                            theme::sidebar_muted(),
                        );
                    }
                    let excerpt = excerpts.get(&thread.id);
                    let height = if excerpt.is_some() { MATCH_ROW } else { ROW };
                    let (row, response) =
                        ui.allocate_exact_size(Vec2::new(list.width(), height), Sense::click());
                    let item = Rect::from_min_max(
                        Pos2::new(row.left() + 8.0, row.top() + 1.0),
                        Pos2::new(row.right() - 8.0, row.bottom() - 1.0),
                    );
                    let selected = self.selected.as_deref() == Some(thread.id.as_str());
                    if selected {
                        ui.painter()
                            .rect_filled(item, CornerRadius::same(8), theme::row_active());
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            item,
                            CornerRadius::same(8),
                            theme::alpha(theme::sidebar_foreground(), 0.06),
                        );
                    }
                    let mut text_right = item.right() - 10.0;
                    if thread.is_running() {
                        ui.painter().circle_filled(
                            Pos2::new(item.right() - 14.0, item.center().y),
                            3.0,
                            theme::primary(),
                        );
                        text_right -= 14.0;
                    }
                    // The title, above the excerpt a message search matched.
                    let band = if excerpt.is_some() {
                        Rect::from_min_max(item.min, Pos2::new(item.right(), item.top() + 28.0))
                    } else {
                        item
                    };
                    if let Some(excerpt) = excerpt {
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(
                                Pos2::new(item.left() + 10.0, band.bottom() - 4.0),
                                Pos2::new(text_right, item.bottom() - 4.0),
                            ),
                            excerpt,
                            font(12.0, Weight::Regular),
                            theme::sidebar_muted(),
                        );
                    }
                    // "Rename thread" edits the title in place.
                    let title = Rect::from_min_max(
                        Pos2::new(band.left() + 6.0, band.top() + 2.0),
                        Pos2::new(text_right, band.bottom() - 2.0),
                    );
                    if !self.rename_editor(ui, &thread.id, title) {
                        widgets::text_in(
                            ui,
                            Rect::from_min_max(
                                Pos2::new(band.left() + 10.0, band.top()),
                                Pos2::new(text_right, band.bottom()),
                            ),
                            &thread.title,
                            font(14.0, Weight::Regular),
                            if selected {
                                theme::sidebar_foreground()
                            } else {
                                theme::alpha(theme::sidebar_foreground(), 0.85)
                            },
                        );
                    }
                    if response.clicked() {
                        open = Some(thread.id.clone());
                    }
                    if response.secondary_clicked()
                        && let Some(at) = response.interact_pointer_pos()
                    {
                        menu = Some((thread.id.clone(), at));
                    }
                }
            });
        if let Some(id) = open {
            self.select_thread(Some(id));
        }
        if let Some((id, at)) = menu {
            self.open_thread_menu(&id, at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_start_near_the_match() {
        let snippet = "…à Osaka, goûte les crêpes japonaises soufflées chez Gram";
        assert_eq!(
            excerpt_around(snippet, "crêpe"),
            "…goûte les crêpes japonaises soufflées chez Gram"
        );
        assert_eq!(excerpt_around("Une crêpe", "CRÊPE"), "Une crêpe");
        assert_eq!(excerpt_around("ab", "abc"), "ab");
    }

    #[test]
    fn only_the_temporary_profile_leaves_memory_out() {
        let profile = |space: Space| serde_json::from_str::<Value>(space.profile()).unwrap();
        assert_eq!(profile(Space::Chats), json!({"version": 1}));
        assert_eq!(
            profile(Space::Temporary),
            json!({"version": 1, "temporary": true})
        );
        assert_ne!(Space::Chats.folder(), Space::Temporary.folder());
    }

    #[test]
    fn chats_group_by_day() {
        let now = chrono::Local::now();
        let ago = |days: i64| (now - chrono::Duration::days(days)).to_rfc3339();
        assert_eq!(day_group(&ago(0), now), "Today");
        assert_eq!(day_group(&ago(1), now), "Yesterday");
        assert_eq!(day_group(&ago(5), now), "Previous 7 days");
        assert_eq!(day_group(&ago(20), now), "Previous 30 days");
        assert_eq!(day_group(&ago(90), now), "Older");
    }
}
