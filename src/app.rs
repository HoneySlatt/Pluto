use eframe::egui::{self, Pos2, Rect, Vec2};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::highlight::Highlighter;
use crate::model::{Project, State, Thread};
use crate::protocol::{Action, Connection, Event, Route};
use crate::theme;
use crate::timeline::Expansion;

pub struct App {
    pub(crate) screen: Screen,
    pub(crate) connect: ConnectForm,
    connection: Option<Connection>,
    pub(crate) state: State,
    pub(crate) selected: Option<String>,
    /// Project of the open new-thread draft; `None` follows the latest project.
    pub(crate) draft_project_id: Option<String>,
    pub(crate) composer: String,
    pub(crate) status: Option<String>,
    pub(crate) expansion: Expansion,
    pub(crate) settled_expanded: bool,
    pub(crate) snoozed_expanded: bool,
    pub(crate) keep_scroll: bool,
    pub(crate) open_menu: Option<String>,
    pub(crate) composer_selection: std::collections::HashMap<String, crate::model::ModelSelection>,
    pub(crate) composer_runtime: std::collections::HashMap<String, String>,
    pub(crate) picker_provider: Option<String>,
    pub(crate) picker_query: String,
    pub(crate) picker_legacy_open: bool,
    pub(crate) favorite_models: std::collections::BTreeSet<String>,
    /// Right panel per thread (`rightPanelStore.ts`, in memory).
    pub(crate) panels: std::collections::HashMap<String, Panel>,
    /// Surface launcher keyboard state: highlighted row, focus taken once.
    pub(crate) launcher_highlight: Option<usize>,
    pub(crate) launcher_focused: bool,
    pub(crate) palette: crate::ui::palette::PaletteState,
    pub(crate) browse: std::collections::HashMap<String, crate::ui::palette::Browse>,
    pub(crate) thread_menu: Option<(String, crate::ui::context_menu::State)>,
    /// `threadLastVisitedAtById` (in memory; upstream persists it).
    pub(crate) last_visited: std::collections::HashMap<String, String>,
    /// The (thread, completion) last stamped as visited.
    visited_turn: Option<(String, String)>,
    /// Sidebar project filter (`sidebarProjectScopeKey`); `None` shows all projects.
    pub(crate) project_scope: Option<String>,
    pub(crate) scope_menu: crate::ui::project_scope::ScopeMenu,
    pub(crate) renaming: Option<(String, String)>,
    pub(crate) rename_focus: bool,
    pub(crate) last_row_click: Option<(String, f64)>,
    pub(crate) confirm: Option<crate::ui::thread_menu::Confirm>,
    pub(crate) panel_width: f32,
    /// Diff scope per thread (`diffPanelStore.ts`), Working tree by default.
    pub(crate) diff_scopes: std::collections::HashMap<String, crate::ui::diff_panel::DiffScope>,
    pub(crate) diff_scope_menu: Option<crate::ui::context_menu::State>,
    /// Head and base refs of loaded branch diffs, by request key.
    pub(crate) diff_branch_refs: std::collections::HashMap<String, (String, String)>,
    pub(crate) diff_tree_open: bool,
    /// Whether the diff file list overflowed (and showed a scrollbar) last frame.
    pub(crate) diff_list_overflows: bool,
    /// The file selected in the diff file tree.
    pub(crate) diff_tree_selected: Option<String>,
    /// Folders collapsed in the diff file tree (all start open).
    pub(crate) diff_tree_collapsed: std::collections::BTreeSet<String>,
    pub(crate) diff_scope_menu_suppress: bool,
    pub(crate) diffs: std::collections::HashMap<String, crate::ui::diff_panel::DiffLoad>,
    pub(crate) diff_open_files: std::collections::BTreeSet<String>,
    pub(crate) diff_wrap: bool,
    pub(crate) diff_split: bool,
    pub(crate) diff_ignore_whitespace: bool,
    pub(crate) diff_reveal: Option<String>,
    pub(crate) question_request: Option<String>,
    pub(crate) question_drafts: std::collections::BTreeMap<String, crate::requests::DraftAnswer>,
    pub(crate) question_index: usize,
    pub(crate) question_collapsed: Option<String>,
    pub(crate) responding: std::collections::HashSet<String>,
    reply_commands: std::collections::HashMap<String, String>,
    pub(crate) search: String,
    /// The open settings page (index into `settings::PAGES`), if any.
    pub(crate) settings_page: Option<usize>,
    /// Client settings (`ClientSettings`), in memory.
    pub(crate) client_settings: Value,
    pub(crate) settings_search: String,
    pub(crate) settings_legacy_open: bool,
    /// The archived shell snapshot for the Archive page; `None` while loading.
    pub(crate) archived: Option<Result<Value, String>>,
    pub(crate) archive_menu: Option<(String, crate::ui::context_menu::State)>,
    /// Settings text fields being edited, by row id.
    pub(crate) settings_edits: std::collections::HashMap<String, String>,
    pub(crate) highlighter: Highlighter,
    pub(crate) now: chrono::DateTime<chrono::Utc>,
    vcs: Option<(String, Value)>,
    watched_vcs: Option<String>,
    /// Workspace whose `vcs.init` is in flight.
    pub(crate) vcs_init: Option<String>,
}

/// `ThreadRightPanelState`, reduced to the one surface this client renders.
#[derive(Clone, Copy, Default)]
pub(crate) struct Panel {
    pub open: bool,
    /// The Diff surface is open (as a tab).
    pub diff: bool,
}

#[derive(PartialEq)]
pub(crate) enum Screen {
    Connect,
    Main,
}

#[derive(Default)]
pub(crate) struct ConnectForm {
    pub server: String,
    pub token: Zeroizing<String>,
    pub error: Option<String>,
    pub busy: bool,
}

pub struct Options {
    pub server: Option<String>,
    pub token: Option<Zeroizing<String>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            screen: Screen::Connect,
            connect: ConnectForm {
                server: options
                    .server
                    .clone()
                    .unwrap_or_else(|| "http://127.0.0.1:3773".into()),
                ..Default::default()
            },
            connection: None,
            state: State::default(),
            selected: None,
            draft_project_id: None,
            composer: String::new(),
            status: None,
            expansion: Expansion::default(),
            settled_expanded: false,
            snoozed_expanded: false,
            keep_scroll: false,
            open_menu: None,
            composer_selection: Default::default(),
            composer_runtime: Default::default(),
            picker_provider: None,
            picker_query: String::new(),
            picker_legacy_open: false,
            favorite_models: Default::default(),
            panels: Default::default(),
            launcher_highlight: None,
            launcher_focused: false,
            palette: Default::default(),
            browse: Default::default(),
            thread_menu: None,
            last_visited: Default::default(),
            visited_turn: None,
            project_scope: None,
            scope_menu: Default::default(),
            renaming: None,
            rename_focus: false,
            last_row_click: None,
            confirm: None,
            panel_width: 540.0,
            diff_scopes: Default::default(),
            diff_scope_menu: None,
            diff_branch_refs: Default::default(),
            diff_tree_open: false,
            diff_list_overflows: false,
            diff_tree_selected: None,
            diff_tree_collapsed: Default::default(),
            diff_scope_menu_suppress: false,
            diffs: Default::default(),
            diff_open_files: Default::default(),
            diff_wrap: true,
            diff_split: false,
            diff_ignore_whitespace: true,
            diff_reveal: None,
            question_request: None,
            question_drafts: Default::default(),
            question_index: 0,
            question_collapsed: None,
            responding: Default::default(),
            settings_page: None,
            client_settings: crate::ui::settings::default_client_settings(),
            settings_search: String::new(),
            settings_legacy_open: false,
            archived: None,
            archive_menu: None,
            settings_edits: Default::default(),
            vcs_init: None,
            reply_commands: Default::default(),
            search: String::new(),
            highlighter: Highlighter::default(),
            now: chrono::Utc::now(),
            vcs: None,
            watched_vcs: None,
        };
        if let (Some(_), Some(token)) = (options.server, options.token) {
            app.connect.token = token;
            app.connect_now(&cc.egui_ctx);
        }
        app
    }

    pub(crate) fn connected(&self) -> bool {
        self.connection.is_some() && self.screen == Screen::Main
    }

    fn poll_events(&mut self) {
        let Some(connection) = &self.connection else {
            return;
        };
        let mut disconnected = None;
        let mut events = Vec::new();
        while let Ok(event) = connection.events.try_recv() {
            events.push(event);
        }
        for event in events {
            match event {
                Event::Data(route, value) => match route {
                    Route::Config => {
                        self.state.config_event(&value);
                        if self.connect.busy {
                            self.connect.busy = false;
                            self.screen = Screen::Main;
                        }
                    }
                    Route::Shell => {
                        if let Err(error) = self.state.shell(&value) {
                            self.status = Some(error);
                        }
                    }
                    Route::Thread(id) => {
                        if let Err(error) = self.state.thread(&id, &value) {
                            self.status = Some(error);
                        }
                    }
                    Route::Vcs(cwd) => self.vcs_event(cwd, &value),
                    Route::Command(id) => self.finish_reply(&id),
                    Route::Rpc(key) => self.rpc_result(key, Ok(value)),
                },
                Event::Failed(Route::Vcs(_), _) => self.vcs = None,
                Event::Failed(Route::Rpc(key), message) => self.rpc_result(key, Err(message)),
                Event::Failed(Route::Command(id), message) => {
                    self.finish_reply(&id);
                    self.status = Some(message);
                }
                Event::Failed(_, message) => self.status = Some(message),
                Event::Disconnected(message) => disconnected = Some(message),
            }
        }
        if let Some(message) = disconnected {
            self.connection = None;
            self.connect.error = Some(message);
            self.connect.busy = false;
            self.screen = Screen::Connect;
        }
    }

    fn vcs_event(&mut self, cwd: String, value: &Value) {
        let mut status = match self.vcs.take() {
            Some((old, status)) if old == cwd => status,
            _ => Value::Object(Default::default()),
        };
        let mut merge = |part: &Value| {
            if let (Some(target), Some(source)) = (status.as_object_mut(), part.as_object()) {
                for (key, value) in source {
                    target.insert(key.clone(), value.clone());
                }
            }
        };
        match value["_tag"].as_str() {
            Some("snapshot") => {
                merge(&value["local"]);
                merge(&value["remote"]);
                status["remoteKnown"] = Value::Bool(!value["remote"].is_null());
            }
            Some("localUpdated") => merge(&value["local"]),
            Some("remoteUpdated") => {
                merge(&value["remote"]);
                status["remoteKnown"] = Value::Bool(!value["remote"].is_null());
            }
            _ => {}
        }
        self.vcs = Some((cwd, status));
    }

    pub(crate) fn select_thread(&mut self, id: Option<String>) {
        if self.selected == id {
            return;
        }
        self.selected = id.clone();
        self.state.clear_detail();
        self.expansion = Expansion::default();
        if id.is_some() {
            self.draft_project_id = None;
        }
        self.send(Action::Select(id));
    }

    /// Reading a finished thread clears its Done badge: the visit is stamped
    /// at the turn's completion time, once per completion (`ChatView.tsx`).
    fn mark_open_thread_visited(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let Some(completed_at) = thread.latest_turn.and_then(|t| t.completed_at) else {
            return;
        };
        let key = (thread.id.clone(), completed_at.clone());
        if self.visited_turn.as_ref() == Some(&key) {
            return;
        }
        self.visited_turn = Some(key);
        self.mark_visited(&thread.id, &completed_at);
    }

    /// `markThreadVisited`: never moves a visit backwards.
    pub(crate) fn mark_visited(&mut self, thread_id: &str, at: &str) {
        let Some(visited) = crate::timeline::parse_time(at) else {
            return;
        };
        let newer = self
            .last_visited
            .get(thread_id)
            .and_then(|previous| crate::timeline::parse_time(previous))
            .is_none_or(|previous| visited > previous);
        if newer {
            self.last_visited
                .insert(thread_id.to_owned(), at.to_owned());
        }
    }

    /// The Woke signal: woke after the last visit and not settled.
    pub(crate) fn woke_at(&self, thread: &crate::model::Thread) -> Option<String> {
        let woke = crate::snooze::woke_at(thread, self.now)?;
        let woke_time = crate::timeline::parse_time(&woke)?;
        let seen = self
            .last_visited
            .get(&thread.id)
            .and_then(|at| crate::timeline::parse_time(at))
            .is_some_and(|visited| visited >= woke_time);
        (!seen && thread.settled_override.as_deref() != Some("settled")).then_some(woke)
    }

    /// `markThreadUnread`: the visit moves to just before the latest completion.
    pub(crate) fn mark_unread(&mut self, thread: &crate::model::Thread) {
        let Some(completed) = thread
            .latest_turn
            .as_ref()
            .and_then(|t| t.completed_at.as_deref())
            .and_then(crate::timeline::parse_time)
        else {
            return;
        };
        let visited = (completed - chrono::Duration::milliseconds(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.last_visited.insert(thread.id.clone(), visited);
    }

    /// `hasUnseenCompletion`: never-visited threads count as read.
    pub(crate) fn is_unread(&self, thread: &crate::model::Thread) -> bool {
        let Some(completed) = thread
            .latest_turn
            .as_ref()
            .and_then(|t| t.completed_at.as_deref())
            .and_then(crate::timeline::parse_time)
        else {
            return false;
        };
        match self.last_visited.get(&thread.id) {
            None => false,
            Some(at) => crate::timeline::parse_time(at).is_none_or(|visited| completed > visited),
        }
    }

    pub(crate) fn start_draft(&mut self, project_id: Option<String>) {
        let project_id = project_id
            .or_else(|| self.current_thread().map(|t| t.project_id))
            .or_else(|| self.draft_project().map(|p| p.id));
        self.select_thread(None);
        self.draft_project_id = project_id;
    }

    /// Dispatches a request reply; the request stays "responding" until the
    /// server receipt (or failure) for this command arrives.
    pub(crate) fn dispatch_reply(&mut self, command: Value, request_id: &str) {
        if let Some(id) = command["commandId"].as_str() {
            self.reply_commands
                .insert(id.to_owned(), request_id.to_owned());
        }
        self.responding.insert(request_id.to_owned());
        self.dispatch(command);
    }

    pub(crate) fn dispatch(&mut self, command: Value) {
        self.send(Action::Dispatch(command));
    }

    fn send(&mut self, action: Action) {
        if let Some(connection) = &self.connection {
            let _ = connection.actions.send(action);
        }
    }

    pub(crate) fn current_thread(&self) -> Option<Thread> {
        let selected = self.selected.as_deref()?;
        self.state
            .detail
            .clone()
            .filter(|t| t.id == selected)
            .or_else(|| {
                self.state
                    .threads
                    .iter()
                    .find(|t| t.id == selected)
                    .cloned()
            })
    }

    /// The project a new thread starts in: the explicit draft project, else
    /// the project of the most recently created active thread.
    pub(crate) fn draft_project(&self) -> Option<Project> {
        if self.selected.is_some() {
            return None;
        }
        if let Some(id) = &self.draft_project_id
            && let Some(project) = self.state.project(id)
        {
            return Some(project.clone());
        }
        let latest = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none())
            .max_by(|a, b| a.created_at.cmp(&b.created_at));
        latest
            .and_then(|t| self.state.project(&t.project_id))
            .or_else(|| self.state.projects.first())
            .cloned()
    }

    pub(crate) fn workspace_cwd(&self) -> Option<String> {
        if let Some(thread) = self.current_thread() {
            if let Some(path) = thread.worktree_path {
                return Some(path);
            }
            return self
                .state
                .project(&thread.project_id)
                .map(|p| p.workspace_root.clone());
        }
        self.draft_project().map(|p| p.workspace_root)
    }

    /// Header git action (`resolveQuickAction`): label and icon.
    pub(crate) fn git_action_label(&self) -> Option<(String, &'static str)> {
        let cwd = self.workspace_cwd()?;
        let status = self.vcs.as_ref().filter(|(c, _)| *c == cwd).map(|(_, s)| s);
        let Some(status) = status.filter(|s| s["isRepo"] == true) else {
            return Some(("Commit".into(), "git-commit-horizontal"));
        };
        let short = "PR";
        let has_branch = !status["refName"].is_null();
        let has_changes = status["hasWorkingTreeChanges"] == true;
        let has_upstream = status["hasUpstream"] == true;
        let has_remote = status["hasPrimaryRemote"] == true;
        let open_pr = status["pr"]["state"] == "open";
        let is_default = status["isDefaultRef"] == true;
        let ahead = status["aheadCount"].as_u64().unwrap_or(0) > 0;
        let behind = status["behindCount"].as_u64().unwrap_or(0) > 0;
        let default_delta = status["aheadOfDefaultCount"]
            .as_u64()
            .or(status["aheadCount"].as_u64())
            .unwrap_or(0)
            > 0;
        let action = |label: &str, icon: &'static str| Some((label.to_owned(), icon));
        if !has_branch {
            return action("Commit", "git-commit-horizontal");
        }
        if has_changes {
            if !has_upstream && !has_remote {
                return action("Commit", "git-commit-horizontal");
            }
            if open_pr || is_default {
                return action("Commit & push", "cloud-upload");
            }
            return action(&format!("Commit, push & {short}"), "git-pull-request-arrow");
        }
        if !has_upstream {
            if !has_remote {
                if open_pr && !ahead {
                    return action(&format!("View {short}"), "git-pull-request-arrow");
                }
                return action("Publish repository", "cloud-upload");
            }
            if !ahead {
                if open_pr {
                    return action(&format!("View {short}"), "git-pull-request-arrow");
                }
                return action("Push", "cloud-upload");
            }
            if open_pr || is_default {
                return action("Push", "cloud-upload");
            }
            return action(&format!("Push & create {short}"), "git-pull-request-arrow");
        }
        if ahead && behind {
            return action("Sync ref", "info");
        }
        if behind {
            return action("Pull", "cloud-download");
        }
        if ahead {
            if open_pr || is_default {
                return action("Push", "cloud-upload");
            }
            return action(&format!("Push & create {short}"), "git-pull-request-arrow");
        }
        if open_pr && has_upstream {
            return action(&format!("View {short}"), "git-pull-request-arrow");
        }
        if default_delta && !is_default {
            return action(&format!("Create {short}"), "git-pull-request-arrow");
        }
        action("Commit", "git-commit-horizontal")
    }

    pub(crate) fn connect_now(&mut self, ctx: &egui::Context) {
        self.connect.error = None;
        self.connect.busy = true;
        let token = self.connect.token.clone();
        match Connection::open(&self.connect.server, token, ctx.clone()) {
            Ok(connection) => self.connection = Some(connection),
            Err(error) => {
                self.connect.error = Some(error);
                self.connect.busy = false;
            }
        }
    }

    fn sync_vcs_watch(&mut self) {
        let cwd = self.workspace_cwd();
        if cwd != self.watched_vcs {
            self.watched_vcs = cwd.clone();
            self.send(Action::WatchVcs(cwd));
        }
    }

    fn status_toast(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(message) = self.status.clone() else {
            return;
        };
        let font = theme::font(13.0, theme::Weight::Regular);
        let width = crate::ui::widgets::text_width(ui, &message, font.clone()).min(420.0) + 48.0;
        let toast = Rect::from_min_size(
            Pos2::new(rect.right() - width - 16.0, 64.0),
            Vec2::new(width, 40.0),
        );
        ui.painter().rect(
            toast,
            egui::CornerRadius::same(10),
            theme::CARD,
            egui::Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );
        crate::ui::widgets::text_in(
            ui,
            Rect::from_min_max(
                toast.min + Vec2::new(14.0, 0.0),
                toast.max - Vec2::new(34.0, 0.0),
            ),
            &message,
            font,
            theme::FOREGROUND,
        );
        let close = Rect::from_min_size(
            Pos2::new(toast.right() - 30.0, toast.top() + 8.0),
            Vec2::splat(24.0),
        );
        if crate::ui::widgets::icon_button(
            ui,
            close,
            "toast-close",
            "x",
            14.0,
            theme::MUTED_FOREGROUND,
            "Dismiss",
        )
        .clicked()
        {
            self.status = None;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.now = chrono::Utc::now();
        self.poll_events();
        let rect = ui.max_rect();
        match self.screen {
            Screen::Connect => self.connect_screen(ui, rect),
            Screen::Main if self.settings_page.is_some() => {
                self.settings_screen(ui, rect);
                self.status_toast(ui, rect);
                self.confirm_dialog(ui, rect);
            }
            Screen::Main => {
                self.sync_vcs_watch();
                self.mark_open_thread_visited();
                self.palette_shortcuts(ui);
                let sidebar =
                    Rect::from_min_size(rect.min, Vec2::new(theme::SIDEBAR_WIDTH, rect.height()));
                let workspace =
                    Rect::from_min_max(Pos2::new(sidebar.right(), rect.top()), rect.max);
                // The right panel keeps 360px for the chat column (`PreviewPanelShell`).
                let panel_open = self.panel_open() && self.current_thread().is_some();
                let main = if panel_open {
                    let max = (rect.width() * 0.7)
                        .min(workspace.width() - 360.0)
                        .max(360.0);
                    self.panel_width = self.panel_width.clamp(360.0, max);
                    Rect::from_min_max(
                        workspace.min,
                        Pos2::new(workspace.right() - self.panel_width, workspace.bottom()),
                    )
                } else {
                    workspace
                };
                self.main_area(ui, main);
                if panel_open {
                    let panel = Rect::from_min_max(Pos2::new(main.right(), rect.top()), rect.max);
                    self.diff_panel(ui, panel);
                }
                self.sidebar(ui, sidebar);
                self.thread_menu(ui);
                self.status_toast(ui, workspace);
                self.command_palette(ui, rect);
                self.confirm_dialog(ui, rect);
            }
        }
        // Relative times tick; repaint at least once a minute.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(30));
    }
}

impl App {
    /// Opens the first thread whose title starts with `prefix` (dev snapshots).
    pub fn open_thread_titled(&mut self, prefix: &str) -> bool {
        let id = self
            .state
            .threads
            .iter()
            .find(|t| t.title.starts_with(prefix))
            .map(|t| t.id.clone());
        let found = id.is_some();
        if found {
            self.select_thread(id);
        }
        found
    }

    /// Whether the shell (and the selected thread, if any) has loaded.
    pub fn is_loaded(&self) -> bool {
        self.connected()
            && !self.state.projects.is_empty()
            && self
                .selected
                .as_ref()
                .is_none_or(|id| self.state.detail.as_ref().is_some_and(|d| &d.id == id))
    }

    /// Expands every settled-turn fold (dev snapshots).
    pub fn expand_turns(&mut self) {
        self.keep_scroll = true;
        if let Some(thread) = self.current_thread() {
            for message in &thread.messages {
                if let Some(turn) = &message.turn_id {
                    self.expansion.turns.insert(turn.clone());
                }
            }
        }
    }
}

impl App {
    /// Shows `config`/`shell` stream items without a server (tests, previews).
    pub fn load_offline(&mut self, config: &Value, shell: &Value) -> Result<(), String> {
        self.state.config_event(config);
        self.state.shell(shell)?;
        self.screen = Screen::Main;
        Ok(())
    }

    /// Applies a thread stream item for the selected thread without a server.
    pub fn load_offline_thread(&mut self, item: &Value) -> Result<(), String> {
        let selected = self.selected.clone().unwrap_or_default();
        self.state.thread(&selected, item)
    }

    pub fn selected_thread_id(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn expanded_turns(&self) -> usize {
        self.expansion.turns.len()
    }
}

impl App {
    fn finish_reply(&mut self, command_id: &str) {
        if let Some(request_id) = self.reply_commands.remove(command_id) {
            self.responding.remove(&request_id);
        }
    }
}

impl App {
    /// Opens the diff panel on a turn, revealing (expanding) `file`.
    pub(crate) fn open_diff(&mut self, turn_id: &str, file: Option<String>) {
        let count = self.current_thread().and_then(|t| {
            t.checkpoints
                .iter()
                .find(|c| c.turn_id == turn_id)
                .map(|c| c.checkpoint_turn_count)
        });
        if let (Some(count), Some(thread)) = (count, self.selected.clone()) {
            self.diff_scopes
                .insert(thread, crate::ui::diff_panel::DiffScope::Turn(count));
        }
        if let Some(panel) = self.panel_mut() {
            panel.open = true;
            panel.diff = true;
        }
        if let Some(file) = file {
            self.diff_tree_selected = Some(file.clone());
            self.diff_reveal = Some(file);
        }
    }

    /// The open thread's right panel.
    pub(crate) fn panel(&self) -> Panel {
        self.selected
            .as_ref()
            .and_then(|id| self.panels.get(id))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn panel_mut(&mut self) -> Option<&mut Panel> {
        let id = self.selected.clone()?;
        Some(self.panels.entry(id).or_default())
    }

    pub(crate) fn panel_open(&self) -> bool {
        self.panel().open
    }

    pub(crate) fn send_request(&mut self, key: String, method: &'static str, payload: Value) {
        self.send(Action::Request {
            key,
            method,
            payload,
        });
    }

    fn rpc_result(&mut self, key: String, result: Result<Value, String>) {
        if key.starts_with("diff:") {
            self.diffs
                .insert(key, crate::ui::diff_panel::DiffLoad::from_result(result));
        } else if let Some(rest) = key.strip_prefix("preview:") {
            // "preview:<kind>:..." — one source of `review.getDiffPreview`.
            let kind = rest.split(':').next().unwrap_or_default().to_owned();
            if let Ok(value) = &result
                && let Some(source) = value["sources"]
                    .as_array()
                    .and_then(|s| s.iter().find(|s| s["kind"] == kind.as_str()))
                && let (Some(head), Some(base)) =
                    (source["headRef"].as_str(), source["baseRef"].as_str())
            {
                self.diff_branch_refs
                    .insert(key.clone(), (head.to_owned(), base.to_owned()));
            }
            self.diffs.insert(
                key,
                crate::ui::diff_panel::DiffLoad::from_preview(result, &kind),
            );
        } else if key.starts_with("browse:") {
            self.browse
                .insert(key, crate::ui::palette::Browse::from_result(result));
        } else if key == "archived" {
            self.archived = Some(result);
        } else if key.starts_with("vcs-init:") {
            self.vcs_init = None;
            match result {
                // Re-subscribe so the header picks up the new repository.
                Ok(_) => self.watched_vcs = None,
                Err(error) => self.status = Some(format!("Could not initialize Git: {error}")),
            }
        }
    }

    /// False once the workspace status says it is not a repository
    /// (`gitStatus?.isRepo ?? true`).
    pub(crate) fn workspace_is_repo(&self) -> bool {
        let Some(cwd) = self.workspace_cwd() else {
            return true;
        };
        self.vcs
            .as_ref()
            .filter(|(c, _)| *c == cwd)
            .is_none_or(|(_, status)| status["isRepo"] != false)
    }

    pub(crate) fn initialize_git(&mut self) {
        let Some(cwd) = self.workspace_cwd() else {
            return;
        };
        if self.vcs_init.is_some() {
            return;
        }
        self.vcs_init = Some(cwd.clone());
        self.send_request(
            format!("vcs-init:{cwd}"),
            "vcs.init",
            serde_json::json!({ "cwd": cwd }),
        );
    }
}

impl App {
    /// Thread whose title is being edited inline, if any.
    pub fn renaming_thread(&self) -> Option<&str> {
        self.renaming.as_ref().map(|(id, _)| id.as_str())
    }
}
