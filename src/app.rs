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
    /// Images attached to the composer, sent with the next message.
    pub(crate) composer_images: Vec<crate::ui::attachments::ComposerImage>,
    /// Clipboard and dropped images read off the UI thread.
    pub(crate) incoming_images: crate::ui::attachments::Incoming,
    /// Settings → Appearance typography.
    pub(crate) typography: crate::ui::typography::Typography,
    /// The expanded image preview.
    pub(crate) lightbox: Option<crate::ui::lightbox::Lightbox>,
    /// Messages queued while a turn runs.
    pub(crate) queues: crate::ui::queue::Queues,
    block_turn_starts: bool,
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
    /// A `modelPicker.jump.N` press (1-based) for the open model picker.
    pub(crate) picker_jump: Option<usize>,
    /// A `modelPicker.previousProvider` (-1) or `nextProvider` (1) press.
    pub(crate) picker_cycle: Option<i32>,
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
    /// Legacy sidebar: collapsed or expanded projects (`projectExpandedById`,
    /// persisted), projects showing all their threads, a pending archive.
    pub(crate) legacy_project_expanded: std::collections::HashMap<String, bool>,
    pub(crate) legacy_thread_lists: std::collections::HashSet<String>,
    pub(crate) legacy_confirm_archive: Option<String>,
    pub(crate) legacy_project_menu: Option<(String, crate::ui::context_menu::State)>,
    /// The "Rename project" dialog: project, edited title, focus request.
    pub(crate) legacy_project_rename: Option<(String, String, bool)>,
    /// Manual project order (`projectOrder`, physical keys, persisted).
    pub(crate) legacy_project_order: Vec<String>,
    /// The project group being dragged in manual sorting.
    pub(crate) legacy_drag: Option<String>,
    /// Where the dragged header was grabbed, below its top.
    pub(crate) legacy_drag_grab: f32,
    /// The member picker of a grouped project's New thread button.
    pub(crate) legacy_new_thread_menu: Option<(String, crate::ui::context_menu::State)>,
    /// The "Project grouping" dialog: member project id and the selection.
    pub(crate) legacy_grouping: Option<(String, String)>,
    pub(crate) legacy_grouping_select: Option<crate::ui::context_menu::State>,
    /// Selected legacy sidebar threads and the range anchor
    /// (`threadSelectionStore`).
    pub(crate) legacy_selection: std::collections::HashSet<String>,
    pub(crate) legacy_anchor: Option<String>,
    pub(crate) legacy_multi_menu: Option<crate::ui::context_menu::State>,
    /// `subscribeTerminalMetadata`: terminal summaries by (thread, terminal).
    pub(crate) terminal_metadata: std::collections::HashMap<(String, String), Value>,
    /// `subscribeDiscoveredLocalServers`: the latest server list.
    pub(crate) discovered_servers: Vec<Value>,
    /// Whether the legacy sidebar's metadata streams are open.
    pub(crate) legacy_streams: bool,
    /// `pullRequests.summary` by PR URL, with when it was asked for.
    pub(crate) pr_summaries: std::collections::HashMap<String, (Option<Value>, std::time::Instant)>,
    pub(crate) confirm: Option<crate::ui::thread_menu::Confirm>,
    pub(crate) snooze_dialog: Option<crate::ui::snooze_dialog::SnoozeDialog>,
    pub(crate) prices_dialog: Option<crate::ui::usage_prices::PricesDialog>,
    /// The custom model being added: instance, typed slug and its error.
    pub(crate) custom_model_input: Option<(String, String, Option<String>)>,
    /// The open provider accent color picker.
    pub(crate) accent_picker: Option<crate::ui::accent_picker::AccentPicker>,
    /// The open custom model editor.
    pub(crate) custom_model_editor: Option<crate::ui::custom_model_editor::CustomModelEditor>,
    /// Environment variable drafts per provider instance.
    pub(crate) env_drafts: std::collections::HashMap<String, crate::ui::providers::EnvDraft>,
    /// The open "Add provider instance" wizard.
    pub(crate) add_instance: Option<crate::ui::providers::AddInstance>,
    /// Source control rows with their details open (by kind).
    pub(crate) source_control_expanded: std::collections::HashSet<String>,
    /// The Bitbucket credentials drafts (tokens are cleared once sent).
    pub(crate) bitbucket: crate::ui::bitbucket::BitbucketDraft,
    /// The settings scope ("Applying settings for <project> across …").
    pub(crate) settings_scope: crate::ui::settings_scope::SettingsScope,
    /// The open Add/Edit Action dialog.
    pub(crate) action_editor: Option<crate::ui::action_editor::ActionEditor>,
    /// The last action run per project (`lastInvokedScriptByProjectId`).
    pub(crate) last_script: std::collections::HashMap<String, String>,
    /// The open "Choose project icon" dialog.
    pub(crate) icon_picker: Option<crate::ui::icon_picker::IconPicker>,
    /// The UI context, for work finishing off the UI thread (favicon fetches).
    pub(crate) egui_ctx: Option<egui::Context>,
    /// The bottom terminal drawer per thread.
    pub(crate) drawers: std::collections::HashMap<String, crate::ui::terminal_drawer::Drawer>,
    pub(crate) agents: crate::ui::agents_panel::AgentsState,
    pub(crate) keybinding_edit: Option<crate::ui::keybindings::KeybindingEdit>,
    pub(crate) new_keybinding: Option<crate::ui::keybindings::KeybindingEdit>,
    /// The open keybinding search and whether it still needs focus.
    pub(crate) keybinding_search: Option<String>,
    pub(crate) keybinding_search_focus: bool,
    pub(crate) panel_width: f32,
    /// Diff scope per thread (`diffPanelStore.ts`), Working tree by default.
    pub(crate) diff_scopes: std::collections::HashMap<String, crate::ui::diff_panel::DiffScope>,
    pub(crate) diff_scope_menu: Option<crate::ui::context_menu::State>,
    /// Head and base refs of loaded branch diffs, by request key.
    pub(crate) diff_branch_refs: std::collections::HashMap<String, (String, String)>,
    /// Full new-side files of git previews, by `diff-contents:` key.
    pub(crate) diff_contents: std::collections::HashMap<String, Result<Vec<String>, String>>,
    /// Revealed context per (file key, gap): lines after the previous hunk
    /// and before the next.
    pub(crate) diff_expanded: std::collections::HashMap<(String, usize), (u32, u32)>,
    /// The branch scope's chosen base ref per thread (absent: Automatic).
    pub(crate) diff_base_refs: std::collections::HashMap<String, String>,
    pub(crate) base_ref_query: String,
    /// `vcs.listRefs` results by `refs:` key.
    pub(crate) diff_refs: std::collections::HashMap<String, Result<Value, String>>,
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
    /// The settings model row whose picker is open, and its button.
    pub(crate) settings_picker: Option<(String, eframe::egui::Rect)>,
    /// The Usage page replaces the workspace (`/usage`).
    pub(crate) usage_open: bool,
    /// `server.discoverSourceControl` for the Source Control settings.
    pub(crate) discovery: crate::ui::source_control::Discovery,
    /// The latest `subscribeDeviceState` value (Integrations settings).
    pub(crate) device_state: Option<Value>,
    /// The instance open in the Providers settings editor.
    pub(crate) providers_selected: Option<String>,
    /// A `server.refreshProviders` from the Providers page is in flight.
    pub(crate) providers_refreshing: bool,
    pub(crate) usage: crate::ui::usage::UsageState,
    /// The Pull requests page replaces the workspace (`/pull-requests`).
    pub(crate) pull_requests_open: bool,
    pub(crate) prs: crate::ui::pull_requests::PullRequestsState,
    /// The Files surface's reads and view state.
    pub(crate) files: crate::ui::files::FilesState,
    pub(crate) content_search: crate::ui::content_search::ContentSearch,
    /// Terminal sessions of the Terminal surface.
    pub(crate) terminals: crate::ui::terminal_surface::Terminals,
    /// Client settings (`ClientSettings`), in memory.
    pub(crate) client_settings: Value,
    pub(crate) client_settings_path: Option<std::path::PathBuf>,
    pub(crate) settings_search: String,
    pub(crate) settings_search_active: usize,
    pub(crate) settings_target: Option<crate::ui::settings_search::Target>,
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

/// `ThreadRightPanelState`, reduced to the surfaces this client renders.
#[derive(Clone, Default)]
pub(crate) struct Panel {
    pub open: bool,
    /// Open surfaces, as tabs, and the active one.
    pub tabs: Vec<PanelTab>,
    pub active: usize,
}

/// One right-panel surface tab.
#[derive(Clone, PartialEq)]
pub(crate) enum PanelTab {
    Diff,
    /// The file browser, with the file it previews.
    Files {
        file: Option<String>,
    },
    /// The thread's subagents.
    Agents,
    /// A terminal group (`terminalIds`), shown as "Terminal N": its first
    /// session, the sessions split beside it, the active one and whether
    /// the split stacks them (`splitDirection: "vertical"`).
    Terminal {
        id: String,
        number: usize,
        split: Vec<String>,
        active: usize,
        vertical: bool,
    },
}

impl Panel {
    pub(crate) fn active_tab(&self) -> Option<&PanelTab> {
        self.tabs.get(self.active).or(self.tabs.last())
    }

    /// Activates the first tab matching `matches`, or opens `tab`.
    pub(crate) fn show(&mut self, tab: PanelTab, matches: impl Fn(&PanelTab) -> bool) {
        match self.tabs.iter().position(matches) {
            Some(index) => self.active = index,
            None => {
                self.tabs.push(tab);
                self.active = self.tabs.len() - 1;
            }
        }
    }

    pub(crate) fn close(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
        }
        if self.active >= index && self.active > 0 {
            self.active -= 1;
        }
    }
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
    /// Where client settings persist; `None` keeps them in memory (tests).
    pub client_settings_path: Option<std::path::PathBuf>,
    /// Drops every `thread.turn.start`, so automated runs never start a
    /// provider turn.
    pub block_turn_starts: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        theme::install(&cc.egui_ctx);
        cc.egui_ctx.add_plugin(crate::ui::widgets::CopyCleanup);
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
            composer_images: Vec::new(),
            incoming_images: Default::default(),
            queues: Default::default(),
            lightbox: None,
            typography: Default::default(),
            block_turn_starts: options.block_turn_starts,
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
            picker_jump: None,
            picker_cycle: None,
            favorite_models: Default::default(),
            panels: Default::default(),
            launcher_highlight: None,
            launcher_focused: false,
            palette: Default::default(),
            browse: Default::default(),
            thread_menu: None,
            last_visited: load_visits(options.client_settings_path.as_deref()),
            visited_turn: None,
            project_scope: None,
            scope_menu: Default::default(),
            renaming: None,
            rename_focus: false,
            last_row_click: None,
            legacy_project_expanded: load_ui_state(options.client_settings_path.as_deref()).0,
            legacy_project_order: load_ui_state(options.client_settings_path.as_deref()).1,
            legacy_drag: None,
            legacy_drag_grab: 0.0,
            legacy_new_thread_menu: None,
            legacy_grouping: None,
            legacy_grouping_select: None,
            legacy_selection: Default::default(),
            legacy_anchor: None,
            legacy_multi_menu: None,
            terminal_metadata: Default::default(),
            discovered_servers: Vec::new(),
            legacy_streams: false,
            pr_summaries: Default::default(),
            legacy_thread_lists: Default::default(),
            legacy_confirm_archive: None,
            legacy_project_menu: None,
            legacy_project_rename: None,
            confirm: None,
            snooze_dialog: None,
            prices_dialog: None,
            custom_model_input: None,
            accent_picker: None,
            custom_model_editor: None,
            env_drafts: Default::default(),
            add_instance: None,
            source_control_expanded: Default::default(),
            bitbucket: Default::default(),
            settings_scope: Default::default(),
            action_editor: None,
            last_script: Default::default(),
            icon_picker: None,
            egui_ctx: None,
            drawers: Default::default(),
            agents: Default::default(),
            keybinding_edit: None,
            new_keybinding: None,
            keybinding_search: None,
            keybinding_search_focus: false,
            panel_width: 540.0,
            diff_scopes: Default::default(),
            diff_scope_menu: None,
            diff_branch_refs: Default::default(),
            diff_contents: Default::default(),
            diff_expanded: Default::default(),
            diff_base_refs: Default::default(),
            base_ref_query: String::new(),
            diff_refs: Default::default(),
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
            settings_picker: None,
            usage_open: false,
            discovery: Default::default(),
            device_state: None,
            providers_selected: None,
            providers_refreshing: false,
            usage: Default::default(),
            pull_requests_open: false,
            prs: Default::default(),
            files: Default::default(),
            content_search: Default::default(),
            terminals: Default::default(),
            client_settings: crate::ui::settings::load_client_settings(
                options.client_settings_path.as_deref(),
            ),
            client_settings_path: options.client_settings_path.clone(),
            settings_search: String::new(),
            settings_search_active: 0,
            settings_target: None,
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
        // The diff panel starts from the saved client defaults.
        app.diff_split = app.client_settings["diffLayout"] == "split";
        app.diff_ignore_whitespace = app.client_settings["diffIgnoreWhitespace"] != false;
        app.diff_wrap = app.client_settings["wordWrap"] != false;
        app.files.wrap = app.diff_wrap;
        app.load_favorites();
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
        self.usage_open = false;
        self.pull_requests_open = false;
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
            self.save_visits();
        }
    }

    /// Visits persist beside the client settings (upstream keeps them in
    /// its UI state store), so a reply that lands while the app is closed
    /// still reads as Done.
    fn save_visits(&self) {
        let Some(path) = visits_path(self.client_settings_path.as_deref()) else {
            return;
        };
        if let Ok(text) = serde_json::to_string(&self.last_visited) {
            let temp = path.with_extension("json.tmp");
            if std::fs::write(&temp, text).is_ok() {
                let _ = std::fs::rename(&temp, &path);
            }
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
        self.save_visits();
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
        if self.block_turn_starts && command["type"] == "thread.turn.start" {
            self.status = Some("Turn starts are blocked in this run".into());
            return;
        }
        self.send(Action::Dispatch(command));
    }

    pub(crate) fn send(&mut self, action: Action) {
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
            theme::card(),
            egui::Stroke::new(1.0, theme::border()),
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
            theme::foreground(),
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
            theme::muted_foreground(),
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
        self.egui_ctx = Some(ui.ctx().clone());
        self.apply_typography(ui.ctx());
        if self.connected() {
            self.request_favicons();
            self.send_due_queued();
        }
        self.sync_theme(ui.ctx());
        let rect = ui.max_rect();
        match self.screen {
            Screen::Connect => self.connect_screen(ui, rect),
            Screen::Main if self.settings_page.is_some() => {
                self.settings_screen(ui, rect);
                self.status_toast(ui, rect);
                self.confirm_dialog(ui, rect);
                self.snooze_dialog(ui, rect);
                self.add_instance_dialog(ui, rect);
                self.action_editor_dialog(ui, rect);
                self.icon_picker_dialog(ui, rect);
                self.command_palette(ui, rect);
            }
            Screen::Main => {
                self.sync_vcs_watch();
                self.mark_open_thread_visited();
                // The image preview takes its keys before the rest of the
                // window; its layer stays on top.
                self.lightbox_dialog(ui, rect);
                self.palette_shortcuts(ui);
                let sidebar =
                    Rect::from_min_size(rect.min, Vec2::new(theme::SIDEBAR_WIDTH, rect.height()));
                let workspace =
                    Rect::from_min_max(Pos2::new(sidebar.right(), rect.top()), rect.max);
                // The right panel keeps 360px for the chat column (`PreviewPanelShell`).
                let panel_open = !self.usage_open
                    && !self.pull_requests_open
                    && self.panel_open()
                    && self.current_thread().is_some();
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
                if self.usage_open {
                    self.usage_page(ui, main);
                } else if self.pull_requests_open {
                    self.pull_requests_page(ui, main);
                } else if let Some(height) = self
                    .drawer_height(main.height())
                    .filter(|_| self.current_thread().is_some())
                {
                    let split = main.bottom() - height;
                    self.main_area(
                        ui,
                        Rect::from_min_max(main.min, Pos2::new(main.right(), split)),
                    );
                    self.terminal_drawer(
                        ui,
                        Rect::from_min_max(Pos2::new(main.left(), split), main.max),
                    );
                } else {
                    self.main_area(ui, main);
                }
                if panel_open {
                    let panel = Rect::from_min_max(Pos2::new(main.right(), rect.top()), rect.max);
                    self.diff_panel(ui, panel);
                }
                self.sidebar(ui, sidebar);
                self.thread_menu(ui);
                self.status_toast(ui, workspace);
                self.command_palette(ui, rect);
                self.legacy_rename_dialog(ui, rect);
                self.legacy_grouping_dialog(ui, rect);
                self.confirm_dialog(ui, rect);
                self.snooze_dialog(ui, rect);
                self.prices_dialog(ui, rect);
                self.action_editor_dialog(ui, rect);
            }
        }
        // Relative times tick; repaint at least once a minute.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(30));
    }
}

impl App {
    /// Follows the environment's theme (`useDefaultThemeAdoption`): the
    /// published theme named by the `defaultTheme` server setting replaces
    /// the stock dark palette; built-in theme ids, which this client does not
    /// render, keep the stock palette.
    fn sync_theme(&self, ctx: &egui::Context) {
        let id = self.state.config["settings"]["defaultTheme"]
            .as_str()
            .unwrap_or_default();
        let palette = (!id.is_empty())
            .then(|| self.state.config["environmentThemes"].as_array())
            .flatten()
            .and_then(|themes| themes.iter().find(|t| t["id"] == id))
            .and_then(crate::theme::Palette::from_theme)
            .unwrap_or(crate::theme::DARK);
        if crate::theme::set_palette(palette) {
            crate::theme::apply_palette(ctx);
            ctx.request_repaint();
        }
    }

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

    /// Shows the open thread as running a turn, locally only (dev snapshots:
    /// the fixture cannot run one without a provider).
    pub fn simulate_running_turn(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let running = |thread: &mut crate::model::Thread| {
            let session = thread.session.get_or_insert_with(Default::default);
            session.status = "running".into();
            session.active_turn_id = Some("snapshot-turn".into());
        };
        if let Some(detail) = self.state.detail.as_mut().filter(|t| t.id == id) {
            running(detail);
        }
        if let Some(thread) = self.state.threads.iter_mut().find(|t| t.id == id) {
            running(thread);
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
            panel.show(PanelTab::Diff, |t| *t == PanelTab::Diff);
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
            .cloned()
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
        if let Some(project) = key.strip_prefix("favicon:") {
            if let Some(ctx) = self.egui_ctx.clone() {
                self.favicon_url(project, result, &ctx);
            }
        } else if let Some(id) = key.strip_prefix("attachment:") {
            if let Some(ctx) = self.egui_ctx.clone() {
                self.attachment_url(id, result, &ctx);
            }
        } else if key.starts_with("diff:") {
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
        } else if key.starts_with("usage-summary:") {
            self.usage.summaries.insert(key, result);
        } else if let Some(summary) = key.strip_prefix("usage-rates:") {
            // Pricing refreshed (or not): rescan the window either way.
            let window = self.usage.window.clone().unwrap_or_default();
            if summary == crate::ui::usage_summary::summary_key(&window) {
                self.send_request(summary.to_owned(), "server.getUsageSummary", window);
            }
        } else if key == "usage-limits" {
            self.usage.now = chrono::Utc::now();
        } else if key.starts_with("term") {
            self.terminals.receive(&key, result);
        } else if key.starts_with("files-") {
            self.files.receive(&key, result);
        } else if key.starts_with("refs:") {
            self.diff_refs.insert(key, result);
        } else if key.starts_with("diff-contents:") {
            let lines = result.map(|value| {
                let text = value["newContents"].as_str().unwrap_or_default();
                let text = text.strip_suffix('\n').unwrap_or(text);
                text.split('\n').map(str::to_owned).collect()
            });
            self.diff_contents.insert(key, lines);
        } else if key.starts_with("keybinding-") {
            if let Err(error) = result {
                self.status = Some(format!("Could not save the keybinding: {error}"));
            }
        } else if key.starts_with("content-search:") {
            self.content_search.receive(&key, result);
        } else if key.starts_with("prs:") {
            self.prs.refreshing = false;
            self.prs.results.insert(key, result);
        } else if key == "providers-refresh" {
            self.providers_refreshing = false;
        } else if let Some(url) = key.strip_prefix("pr-summary:") {
            if let Some(entry) = self.pr_summaries.get_mut(url) {
                // A failed refresh keeps the last summary.
                if let Ok(summary) = result {
                    entry.0 = Some(summary);
                }
            }
        } else if key == "sidebar-terminal-metadata" {
            if let Ok(event) = result {
                self.terminal_metadata_event(&event);
            }
        } else if key == "discovered-servers" {
            if let Ok(list) = result {
                self.discovered_servers = list["servers"].as_array().cloned().unwrap_or_default();
            }
        } else if key == "device-state" {
            // A stream: keep the last good state if it later fails or expires.
            if let Ok(state) = result {
                self.device_state = Some(state);
            }
        } else if key == "source-control-discovery" {
            self.discovery.pending = false;
            self.discovery.result = Some(result);
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

/// `visited.json` beside the client settings file.
fn visits_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("visited.json"))
}

fn ui_state_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("ui-state.json"))
}

/// The persisted UI state (upstream's `uiStateStore`): project expansion
/// and the manual project order.
fn load_ui_state(
    settings: Option<&std::path::Path>,
) -> (std::collections::HashMap<String, bool>, Vec<String>) {
    let value = ui_state_path(settings)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_default();
    (
        serde_json::from_value(value["projectExpandedById"].clone()).unwrap_or_default(),
        serde_json::from_value(value["projectOrder"].clone()).unwrap_or_default(),
    )
}

pub(crate) fn save_ui_state(app: &App) {
    let Some(path) = ui_state_path(app.client_settings_path.as_deref()) else {
        return;
    };
    let value = serde_json::json!({
        "projectExpandedById": app.legacy_project_expanded,
        "projectOrder": app.legacy_project_order,
    });
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, value.to_string()).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

/// The persisted thread visits (empty without a settings path).
fn load_visits(settings: Option<&std::path::Path>) -> std::collections::HashMap<String, String> {
    visits_path(settings)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}
