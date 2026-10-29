use std::sync::Arc;

use eframe::egui::{self, Pos2, Rect, Vec2};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::highlight::Highlighter;
use crate::model::{Project, State, Thread};
use crate::protocol::{Action, Connection, Event, Route};
use crate::theme;
use crate::timeline::Expansion;

/// A lost connection's way back (`WsTransport`'s reconnection): attempts
/// one after another, from 1.5 s apart up to 15 s, until the server answers
/// or the user changes server.
pub(crate) struct Reconnect {
    attempt: u32,
    /// When the connection was lost: the notice waits 2 s, "Change server"
    /// 20 s.
    pub(crate) since: std::time::Instant,
    next: std::time::Instant,
    /// Why the last attempt failed.
    pub(crate) error: String,
}

/// "192.168.1.20:3774" from "http://192.168.1.20:3774/".
fn server_host(server: &str) -> String {
    let server = server.trim();
    server
        .split_once("://")
        .map_or(server, |(_, rest)| rest)
        .trim_end_matches('/')
        .to_owned()
}

const NOT_CONNECTED: &str = "Not connected: message not sent. Reconnecting to the environment; try again once it is connected.";

pub struct App {
    pub(crate) screen: Screen,
    pub(crate) connect: ConnectForm,
    connection: Option<Connection>,
    /// The connection's server synchronized (its config arrived): until
    /// then commands fail at once and subscriptions wait (`send`).
    synced: bool,
    /// A sidebar row being dragged, this frame's sections and the
    /// composer's rect (`ui/sidebar_drag.rs`).
    pub(crate) sidebar_drag: Option<crate::ui::sidebar_drag::Drag>,
    pub(crate) sidebar_zones: Vec<(crate::ui::sidebar_drag::Zone, Rect)>,
    pub(crate) composer_rect: Option<Rect>,
    /// Panel tabs closed, newest first, with their thread.
    pub(crate) closed_views: Vec<(String, PanelTab)>,
    /// Ctrl+Q's hold (`ui/quit_hold.rs`).
    pub(crate) quit_hold: crate::ui::quit_hold::QuitHold,
    /// What the last parks can undo (`ui/undo.rs`).
    pub(crate) undo: crate::ui::undo::Undo,
    /// Commands whose receipt something waits on (`ui/park.rs`).
    pub(crate) pending_commands: std::collections::HashMap<String, crate::ui::park::Continuation>,
    /// The open thread's move once its park shows (`ui/park.rs`).
    pub(crate) advance: Option<crate::ui::park::Advance>,
    /// "Some requests are slow" while any is.
    slow_toast: Option<u64>,
    /// "Restoring your threads…" while the server migrates them.
    migration_toast: Option<u64>,
    /// When "Keybindings updated" last showed (a 2 s cooldown).
    keybindings_toast_at: Option<std::time::Instant>,
    /// The server the shell came from: connecting to another one clears it.
    shell_server: Option<String>,
    /// Whether the window had the focus last frame: coming back checks the
    /// connection (`Action::Ping`).
    focused: bool,
    /// The window's size when not maximized, and whether it is: written at
    /// exit for the next start (`saved_window`).
    window: Option<([f32; 2], bool)>,
    /// A lost connection coming back: the attempt, when the next one goes,
    /// and its toast.
    pub(crate) reconnect: Option<Reconnect>,
    /// The streams open, by key, opened again after a reconnection.
    streams: Vec<(String, &'static str, Value)>,
    /// Failures of what was sent without a synchronized connection, handled
    /// with the server's events.
    offline_failures: Vec<Event>,
    pub(crate) state: State,
    pub(crate) selected: Option<String>,
    /// The open thread, when its subscription failed before the server had
    /// created it (a launch and its subscription race), with the attempts
    /// made and whether a new one is due once the shell lists the thread.
    thread_retry: Option<(String, u8, bool)>,
    /// Project of the open new-thread draft; `None` follows the latest project.
    pub(crate) draft_project_id: Option<String>,
    /// Where the open draft starts, when picked (`DraftThreadEnvMode`):
    /// `local` or `worktree`, by project.
    pub(crate) draft_env_mode: Option<(String, String)>,
    pub(crate) draft_workspace: Option<crate::ui::branch_picker::DraftWorkspace>,
    pub(crate) branch_picker: crate::ui::branch_picker::BranchPicker,
    pub(crate) composer: String,
    /// Images attached to the composer, sent with the next message.
    pub(crate) composer_images: Vec<crate::ui::attachments::ComposerImage>,
    /// Files attached to the composer, uploading or uploaded.
    pub(crate) composer_files: Vec<crate::ui::attachments::ComposerFile>,
    pub(crate) file_uploads: crate::ui::attachments::Uploads,
    /// The prompt recalled with the arrows, and the thread it came from.
    pub(crate) prompt_recall: Option<(String, crate::ui::prompt_history::Position)>,
    /// Clipboard and dropped images read off the UI thread.
    pub(crate) incoming_images: crate::ui::attachments::Incoming,
    /// Settings → Appearance typography.
    pub(crate) typography: crate::ui::typography::Typography,
    /// The expanded image preview.
    pub(crate) lightbox: Option<crate::ui::lightbox::Lightbox>,
    /// Messages on their way: waiting for their images to persist
    /// (`persist:<message id>`) or for a command's receipt (`after:<command id>`).
    pub(crate) turn_sends: std::collections::HashMap<String, crate::ui::composer::TurnSend>,
    /// The drafts of the composers not open, and of the messages on their way.
    pub(crate) drafts: crate::ui::drafts::Drafts,
    block_turn_starts: bool,
    /// The next toast: its kind and text (`notify`).
    pub(crate) status: Option<(crate::ui::toasts::Kind, String)>,
    pub(crate) expansion: Expansion,
    pub(crate) settled_expanded: bool,
    pub(crate) snoozed_expanded: bool,
    pub(crate) keep_scroll: bool,
    pub(crate) timeline_view: crate::ui::timeline_view::TimelineView,
    pub(crate) open_menu: Option<String>,
    /// The docked sidebar is shown (`SidebarProvider`'s `open`).
    pub(crate) sidebar_open: bool,
    /// Below `md` the sidebar is a sheet over the workspace (`openMobile`).
    pub(crate) sidebar_sheet_open: bool,
    /// The window is narrower than `md` (768px, `useIsMobile`).
    pub(crate) mobile: bool,
    pub(crate) composer_selection: std::collections::HashMap<String, crate::model::ModelSelection>,
    /// The last models picked, for new drafts (`stickyModelSelectionByProvider`
    /// and `stickyActiveProvider`), by space: `code`, or Pluto's `chat`.
    pub(crate) sticky_models: std::collections::HashMap<String, crate::ui::pickers::StickyModel>,
    pub(crate) composer_runtime: std::collections::HashMap<String, String>,
    /// The composer's interaction mode (plan or default) by composer key.
    pub(crate) composer_interaction: std::collections::HashMap<String, String>,
    /// The running turn's task drawer is open, for this thread.
    /// The composer's "/", "@" and "$" menu.
    pub(crate) command_menu: crate::ui::command_menu::MenuState,
    /// The composer's context records (terminal excerpts), by context id.
    pub(crate) composer_contexts: Vec<serde_json::Value>,
    /// A sent terminal excerpt's popover: its chip's rect and record.
    pub(crate) bubble_popover: Option<(eframe::egui::Rect, serde_json::Value)>,
    /// User messages opened past their fold, and their measured text sizes
    /// by (message, text length, width).
    pub(crate) expanded_user_messages: std::collections::HashSet<String>,
    pub(crate) user_text_sizes: std::collections::HashMap<(String, usize, u32), eframe::egui::Vec2>,
    pub(crate) tasks_open: bool,
    /// The queued messages' banner above the composer.
    pub(crate) queue: crate::ui::queue::QueueUi,
    /// MCP apps hosted in the chat.
    pub(crate) mcp_apps: crate::ui::mcp_apps::McpApps,
    /// The thread details card's paging and folds.
    pub(crate) thread_details: crate::ui::thread_details::DetailsState,
    /// Every thread's messages searched (`orchestration.searchThreads`).
    pub(crate) thread_search: [crate::ui::thread_search::ThreadSearch; 2],
    /// Storage cleanup's last report (`server.getStorageCleanupReport`), the
    /// stream asked, and a manual run in flight.
    pub(crate) storage_report: Option<serde_json::Value>,
    /// Settings → Scheduled Tasks.
    pub(crate) scheduled: crate::ui::scheduled_tasks::ScheduledTasks,
    pub(crate) storage_stream: bool,
    pub(crate) storage_running: bool,
    pub(crate) tasks_thread: Option<String>,
    pub(crate) picker_provider: Option<String>,
    pub(crate) picker_query: String,
    pub(crate) picker_legacy_open: bool,
    /// A `modelPicker.jump.N` press (1-based) for the open model picker.
    pub(crate) picker_jump: Option<usize>,
    /// A `modelPicker.previousProvider` (-1) or `nextProvider` (1) press.
    pub(crate) picker_cycle: Option<i32>,
    /// The composer's mode and effort controls as last drawn (their menu
    /// ids), for `composer.mode` and `composer.effort`.
    pub(crate) composer_controls: (Option<&'static str>, Option<&'static str>),
    /// The threads opened before and after the open one (`navigation.back`
    /// and `navigation.forward`, the browser history upstream).
    pub(crate) nav_back: Vec<Option<String>>,
    pub(crate) nav_forward: Vec<Option<String>>,
    pub(crate) navigating: bool,
    /// Edit from here: its dialog, then the rollback on its way.
    pub(crate) rewind: Option<crate::ui::rewind::Rewind>,
    /// Composer notices dismissed for the run, by id.
    pub(crate) dismissed_notices: std::collections::HashSet<String>,
    /// Branch notices shown once (they stay while the mismatch lasts), and
    /// the thread whose branch is being restored.
    pub(crate) revealed_notices: std::cell::RefCell<std::collections::HashSet<String>>,
    /// The way the next send goes, from its keys.
    pub(crate) send_intent: Option<crate::ui::composer::SendIntent>,
    /// Threads started in the background, until the server takes them.
    pub(crate) background_launches: std::collections::HashSet<String>,
    /// Secret request cards' fields (`ui/secret_request.rs`).
    pub(crate) secrets: crate::ui::secret_request::Secrets,
    /// Prompts put aside (`ui/stash.rs`).
    pub(crate) stash: crate::ui::stash::Stash,
    /// Holds a request back while the user types (`ui/typing_guard.rs`).
    pub(crate) typing_guard: crate::ui::typing_guard::TypingGuard,
    /// The composer's prompt-length alert, once a send was refused.
    pub(crate) prompt_length_error: Option<String>,
    /// The provider status banner dismissed, by its key.
    pub(crate) dismissed_provider_status: Option<String>,
    pub(crate) restoring_branch: Option<String>,
    /// Notices whose command is on its way ("Stopping...", "Saving...").
    pub(crate) notice_busy: std::collections::HashSet<String>,
    /// Thread errors dismissed for the run, by thread and message.
    pub(crate) dismissed_thread_errors: std::collections::HashSet<String>,
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
    /// The draft hero's project picker, while open.
    pub(crate) hero_project_menu: Option<crate::ui::context_menu::State>,
    /// Toasts, newest last.
    pub(crate) toasts: crate::ui::toasts::Toasts,
    pub(crate) thread_notifications: crate::ui::thread_notifications::ThreadNotifications,
    /// The header's git actions: what runs, its menu and dialogs.
    pub(crate) git: crate::ui::git_actions::GitActions,
    /// Proposed plan cards: folds, menu and save dialog.
    pub(crate) plans: crate::ui::plan_card::PlanState,
    /// The "Publish repository" wizard, while open.
    pub(crate) publish: Option<crate::ui::publish::PublishDialog>,
    /// The editor last opened with (`t3code:last-editor`), persisted.
    pub(crate) last_editor: Option<String>,
    /// The "Open in" editor list and the path it opens.
    pub(crate) open_in_menu: Option<(String, crate::ui::context_menu::State)>,
    /// An image a file link opens once it has loaded.
    pub(crate) media_pending: Option<crate::ui::media::Source>,
    /// An image's right-click menu, while open.
    pub(crate) media_menu: Option<(crate::ui::media::Source, crate::ui::context_menu::State)>,
    /// A Scratch project asked for, until it is in the shell.
    pub(crate) scratch: Option<crate::ui::scratch::Pending>,
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
    /// Whether `subscribeDiscoveredLocalServers` runs.
    discovery_stream: bool,
    /// The text menu (Electron's context menu) and the link under it.
    /// The text menu (Electron's context menu): its state, the link under it
    /// and whether Copy applies, both read when it opened.
    pub(crate) text_menu: Option<(crate::ui::context_menu::State, Option<String>, bool)>,
    pub(crate) pending_copy: bool,
    /// Citing assistant text (`cite`).
    pub(crate) cite: crate::ui::cite::CiteState,
    /// Find in thread, this frame's marks, and the passes spent bringing
    /// the active match into view.
    pub(crate) find: crate::ui::thread_find::ThreadFind,
    pub(crate) find_paint: Option<crate::ui::thread_find::FindPaint>,
    pub(crate) find_attempts: u32,
    /// Opened tool calls' fetched images, by item and revision.
    pub(crate) tool_outputs: std::collections::HashMap<String, crate::ui::tool_images::ToolOutput>,
    /// The Browser surface's preview browser (`ui::preview`).
    pub(crate) preview: crate::ui::preview::PreviewUi,
    pub(crate) html_renders: crate::ui::html_render::HtmlRenders,
    pub(crate) design: crate::ui::design::DesignState,
    pub(crate) design_home: crate::ui::design_home::DesignHome,
    pub(crate) chat_mode: crate::ui::chat_mode::ChatMode,
    pub(crate) chat_versions: crate::ui::chat_versions::ChatVersions,
    pub(crate) chat_projects: crate::ui::chat_projects::ChatProjects,
    /// A chat to export as Markdown once its history is loaded.
    pub(crate) pending_chat_export: Option<String>,
    pub(crate) memory: crate::ui::memory_settings::MemorySettings,
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
    pub(crate) diff_contents: std::collections::HashMap<String, Result<Arc<Vec<String>>, String>>,
    /// Rows each wrapped diff cell takes, by its text and wrap width.
    pub(crate) diff_rows: std::collections::HashMap<u64, usize>,
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
    pub(crate) vcs: Option<(String, Value)>,
    watched_vcs: Option<String>,
    /// Workspace whose `vcs.init` is in flight.
    pub(crate) vcs_init: Option<String>,
}

/// `ThreadRightPanelState`, reduced to the surfaces this client renders.
#[derive(Clone, Default)]
pub(crate) struct Panel {
    pub open: bool,
    /// The panel takes the chat column's width too (`maximized`).
    pub maximized: bool,
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
    /// A Browser tab and its page in the preview browser.
    Browser {
        id: u64,
    },
    /// The thread's published pages (`crate::ui::design`).
    Design,
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
        cc.egui_ctx
            .add_plugin(crate::ui::widgets::CopyCleanup::default());
        let mut app = Self {
            screen: Screen::Connect,
            connect: ConnectForm {
                server: options
                    .server
                    .clone()
                    .unwrap_or_else(|| "http://127.0.0.1:3774".into()),
                ..Default::default()
            },
            connection: None,
            synced: false,
            pending_commands: Default::default(),
            undo: Default::default(),
            quit_hold: Default::default(),
            closed_views: Vec::new(),
            sidebar_drag: None,
            sidebar_zones: Vec::new(),
            composer_rect: None,
            advance: None,
            migration_toast: None,
            slow_toast: None,
            keybindings_toast_at: None,
            shell_server: None,
            focused: true,
            window: None,
            reconnect: None,
            streams: Vec::new(),
            offline_failures: Vec::new(),
            state: State::default(),
            selected: None,
            thread_retry: None,
            draft_project_id: None,
            draft_env_mode: None,
            draft_workspace: None,
            branch_picker: Default::default(),
            composer: String::new(),
            composer_images: Vec::new(),
            composer_files: Vec::new(),
            file_uploads: Default::default(),
            prompt_recall: None,
            incoming_images: Default::default(),
            turn_sends: Default::default(),
            drafts: crate::ui::drafts::load(options.client_settings_path.as_deref()),
            lightbox: None,
            typography: Default::default(),
            block_turn_starts: options.block_turn_starts,
            status: None,
            expansion: Expansion::default(),
            settled_expanded: false,
            snoozed_expanded: false,
            keep_scroll: false,
            timeline_view: Default::default(),
            open_menu: None,
            sidebar_open: true,
            sidebar_sheet_open: false,
            mobile: false,
            composer_selection: Default::default(),
            sticky_models: serde_json::from_value(
                read_ui_state(options.client_settings_path.as_deref())["stickyModels"].clone(),
            )
            .unwrap_or_default(),
            composer_runtime: Default::default(),
            composer_interaction: Default::default(),
            command_menu: Default::default(),
            composer_contexts: Vec::new(),
            bubble_popover: None,
            expanded_user_messages: Default::default(),
            user_text_sizes: Default::default(),
            tasks_open: false,
            queue: Default::default(),
            mcp_apps: Default::default(),
            thread_details: Default::default(),
            thread_search: Default::default(),
            storage_report: None,
            scheduled: Default::default(),
            storage_stream: false,
            storage_running: false,
            tasks_thread: None,
            picker_provider: None,
            picker_query: String::new(),
            picker_legacy_open: false,
            picker_jump: None,
            picker_cycle: None,
            composer_controls: (None, None),
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            navigating: false,
            rewind: None,
            dismissed_notices:
                read_ui_state(options.client_settings_path.as_deref())["dismissedNotices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            revealed_notices: Default::default(),
            dismissed_provider_status: None,
            prompt_length_error: None,
            typing_guard: Default::default(),
            stash: crate::ui::stash::load(options.client_settings_path.as_deref()),
            send_intent: None,
            secrets: Default::default(),
            background_launches: Default::default(),
            restoring_branch: None,
            notice_busy: Default::default(),
            dismissed_thread_errors: Default::default(),
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
            hero_project_menu: None,
            scratch: None,
            media_menu: None,
            media_pending: None,
            toasts: Default::default(),
            thread_notifications: Default::default(),
            git: Default::default(),
            plans: Default::default(),
            publish: None,
            last_editor: load_ui_state(options.client_settings_path.as_deref()).2,
            open_in_menu: None,
            legacy_grouping: None,
            legacy_grouping_select: None,
            legacy_selection: Default::default(),
            legacy_anchor: None,
            legacy_multi_menu: None,
            terminal_metadata: Default::default(),
            discovered_servers: Vec::new(),
            legacy_streams: false,
            discovery_stream: false,
            text_menu: None,
            pending_copy: false,
            cite: Default::default(),
            find: Default::default(),
            find_paint: None,
            find_attempts: 0,
            tool_outputs: Default::default(),
            preview: Default::default(),
            html_renders: Default::default(),
            design: Default::default(),
            design_home: Default::default(),
            chat_mode: Default::default(),
            chat_versions: Default::default(),
            chat_projects: Default::default(),
            pending_chat_export: None,
            memory: Default::default(),
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
            diff_rows: Default::default(),
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
            files: {
                let saved = read_ui_state(options.client_settings_path.as_deref());
                let mut files = crate::ui::files::FilesState::default();
                files.render_markdown = saved["renderMarkdown"].as_bool().unwrap_or(false);
                files.render_table = saved["renderTable"].as_bool().unwrap_or(true);
                files
            },
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
            highlighter: Highlighter::new(cc.egui_ctx.clone()),
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

    /// Shows `text` as the next toast, of `kind` (success, warning, error…).
    pub(crate) fn notify(&mut self, kind: crate::ui::toasts::Kind, text: impl Into<String>) {
        self.status = Some((kind, text.into()));
    }

    pub(crate) fn connected(&self) -> bool {
        self.connection.is_some() && self.synced && self.screen == Screen::Main
    }

    pub(crate) fn poll_events(&mut self) {
        let mut events = std::mem::take(&mut self.offline_failures);
        let mut disconnected = None;
        if let Some(connection) = &self.connection {
            while let Ok(event) = connection.events.try_recv() {
                events.push(event);
            }
        }
        if self.connection.is_none() && events.is_empty() {
            return;
        }
        for event in events {
            match event {
                Event::Data(route, value) => match route {
                    Route::Config => {
                        // Only the snapshot completes the handshake: the
                        // worker refuses commands until it came.
                        let snapshot = value["type"] == "snapshot";
                        if self.synced && value["type"] == "keybindingsUpdated" {
                            self.keybindings_reloaded(&value["payload"]);
                        }
                        self.state.config_event(&value);
                        if !self.synced && snapshot {
                            self.synced = true;
                            self.shell_server = Some(self.connect.server.trim().to_owned());
                            self.reconnected();
                        }
                        if self.connect.busy && snapshot {
                            self.connect.busy = false;
                            self.screen = Screen::Main;
                            self.restore_open_draft();
                        }
                    }
                    Route::Shell => {
                        // The open thread deleted elsewhere: its draft takes
                        // the screen, in the same project.
                        let removed = value["kind"] == "thread.removed"
                            && self
                                .selected
                                .as_deref()
                                .is_some_and(|id| value["threadId"] == id);
                        let project = self.current_thread().map(|t| t.project_id.clone());
                        if let Err(error) = self.state.shell(&value) {
                            self.notify(crate::ui::toasts::Kind::Error, error);
                        }
                        if removed {
                            self.start_draft(project);
                            self.notify(
                                crate::ui::toasts::Kind::Warning,
                                "This thread was deleted elsewhere.",
                            );
                        }
                    }
                    Route::Thread(id) => {
                        if self
                            .thread_retry
                            .as_ref()
                            .is_some_and(|(retry, ..)| *retry == id)
                        {
                            self.thread_retry = None;
                        }
                        if let Err(error) = self.state.thread(&id, &value) {
                            self.notify(crate::ui::toasts::Kind::Error, error);
                        }
                    }
                    Route::Vcs(cwd) => self.vcs_event(cwd, &value),
                    Route::Lifecycle => self.lifecycle_event(&value),
                    Route::Command(id) => {
                        self.command_settled(&id, None);
                        self.finish_reply(&id);
                        self.rewind_done(&id, None);
                        self.sent_done(&format!("command:{id}"));
                        self.turn_command_done(&id, true);
                    }
                    Route::Rpc(key) => self.rpc_result(key, Ok(value)),
                    Route::History {
                        thread_id,
                        cursor,
                        complete,
                    } => {
                        if let Err(error) = self
                            .state
                            .history_page(&thread_id, &cursor, &value, complete)
                        {
                            self.state.history_failed(&thread_id, &cursor, error);
                        }
                    }
                },
                Event::Failed(
                    Route::History {
                        thread_id, cursor, ..
                    },
                    message,
                ) => {
                    self.state.history_failed(&thread_id, &cursor, message);
                }
                Event::Failed(Route::Vcs(_), _) => self.vcs = None,
                // A server without the lifecycle stream: nothing to show.
                Event::Failed(Route::Lifecycle, _) => {}
                Event::Failed(Route::Rpc(key), message) => {
                    // A stream that failed is not opened again.
                    self.streams.retain(|(open, ..)| *open != key);
                    self.rpc_result(key, Err(message))
                }
                Event::Failed(Route::Command(id), message)
                    if self.command_settled(&id, Some(&message)) => {}
                Event::Failed(Route::Command(id), message) => {
                    self.finish_reply(&id);
                    self.rewind_done(&id, Some(&message));
                    let waiting = self.turn_command_done(&id, false);
                    let text = if waiting || self.give_back(&format!("command:{id}")) {
                        format!("Message not sent: {message} Your text is back in the composer.")
                    } else {
                        message
                    };
                    self.notify(crate::ui::toasts::Kind::Error, text);
                }
                Event::Failed(Route::Thread(id), message) => {
                    let attempts = match &self.thread_retry {
                        Some((retry, attempts, _)) if *retry == id => *attempts,
                        _ => 0,
                    };
                    // `orchestration.launchThread` and the new thread's
                    // subscription go out together, and the server can load
                    // the thread before it has created it: subscribe again
                    // once the shell lists it.
                    if self.selected.as_deref() == Some(id.as_str()) && attempts < 3 {
                        self.thread_retry = Some((id, attempts + 1, true));
                    } else {
                        self.notify(crate::ui::toasts::Kind::Error, message);
                    }
                }
                Event::Failed(_, message) => self.notify(crate::ui::toasts::Kind::Error, message),
                Event::Disconnected(message) => disconnected = Some(message),
                Event::Slow(requests) => self.slow_requests(&requests),
            }
        }
        if let Some((id, _, due)) = &mut self.thread_retry {
            if self.selected.as_deref() != Some(id.as_str()) {
                self.thread_retry = None;
            } else if *due && self.state.threads.iter().any(|thread| thread.id == *id) {
                *due = false;
                let id = id.clone();
                self.send(Action::Select(Some(id)));
            }
        }
        self.poll_scratch();
        self.poll_chat_mode();
        self.poll_advance();
        if let Some(message) = disconnected {
            // What was on its way may not have arrived: it comes back.
            if self.give_back_all() > 0 {
                self.notify(crate::ui::toasts::Kind::Warning,
                    "The connection dropped while sending: your message is back in the composer. Check the thread before sending it again.",
                );
            }
            self.connection = None;
            self.synced = false;
            if self.screen == Screen::Main && !self.connect.busy {
                // A session lost after it was established comes back by itself.
                self.lost(message);
            } else {
                self.connect.error = Some(message);
                self.connect.busy = false;
                self.screen = Screen::Connect;
            }
        }
    }

    /// The connection dropped: the next attempt is due shortly; the
    /// composer's notice says so until the server is back.
    fn lost(&mut self, error: String) {
        let attempt = self.reconnect.as_ref().map_or(0, |r| r.attempt) + 1;
        let delay = (1500u64 << (attempt - 1).min(4)).min(15_000);
        let since = self
            .reconnect
            .as_ref()
            .map_or_else(std::time::Instant::now, |r| r.since);
        self.reconnect = Some(Reconnect {
            attempt,
            since,
            next: std::time::Instant::now() + std::time::Duration::from_millis(delay),
            error,
        });
    }

    /// The server's address while the connection comes back.
    pub(crate) fn reconnecting_host(&self) -> Option<(String, String)> {
        self.reconnect
            .as_ref()
            .map(|r| (server_host(&self.connect.server), r.error.clone()))
    }

    /// Opens the next attempt once it is due (every frame).
    fn poll_reconnect(&mut self, ctx: &egui::Context) {
        let Some(reconnect) = &self.reconnect else {
            return;
        };
        if self.connection.is_some() {
            return;
        }
        let wait = reconnect
            .next
            .saturating_duration_since(std::time::Instant::now());
        if !wait.is_zero() {
            ctx.request_repaint_after(wait);
            return;
        }
        let token = self.connect.token.clone();
        match Connection::open(&self.connect.server, token, crate::repaint(ctx)) {
            Ok(connection) => self.connection = Some(connection),
            Err(error) => self.lost(error),
        }
    }

    /// The server synchronized again: the streams open again, the open
    /// thread and the VCS status are followed again.
    fn reconnected(&mut self) {
        let Some(reconnect) = self.reconnect.take() else {
            return;
        };
        let _ = reconnect;
        let mut toast =
            crate::ui::toasts::Toast::new(crate::ui::toasts::Kind::Success, "Reconnected");
        toast.timeout = Some(3.0);
        self.toasts.add(toast);
        for (key, method, payload) in self.streams.clone() {
            self.send(Action::Stream {
                key,
                method,
                payload,
            });
        }
        self.watched_vcs = None;
        if let Some(id) = self.selected.clone() {
            self.thread_retry = Some((id, 0, true));
        }
    }

    /// A synchronized fake connection, for the tests of other modules.
    #[cfg(test)]
    pub(crate) fn use_connection_for_tests(&mut self, connection: Connection) {
        self.connection = Some(connection);
        self.synced = true;
    }

    /// `KeybindingsUpdateToast`: the server reloaded `keybindings.json`.
    fn keybindings_reloaded(&mut self, payload: &Value) {
        use crate::ui::toasts::{Action, Kind, Toast};
        let issue = payload["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|issue| {
                issue["kind"]
                    .as_str()
                    .is_some_and(|kind| kind.starts_with("keybindings."))
            });
        if let Some(issue) = issue {
            let mut toast = Toast::new(Kind::Warning, "Invalid keybindings configuration")
                .description(issue["message"].as_str().unwrap_or_default());
            if let Some(path) = self.state.config["keybindingsConfigPath"].as_str()
                && self.preferred_editor().is_some()
            {
                toast.secondary.push((
                    "Open keybindings.json".into(),
                    Action::OpenInEditor(path.to_owned()),
                ));
            }
            self.toasts.add(toast);
            return;
        }
        if self
            .keybindings_toast_at
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2))
        {
            return;
        }
        self.keybindings_toast_at = Some(std::time::Instant::now());
        self.toasts.add(
            Toast::new(Kind::Success, "Keybindings updated")
                .description("Keybindings configuration reloaded successfully."),
        );
    }

    /// `SlowRpcRequestToastCoordinator`: one toast while requests are slow.
    fn slow_requests(&mut self, requests: &[crate::protocol::SlowRequest]) {
        use crate::ui::toasts::{Kind, Toast};
        let Some(threshold) = requests.iter().map(|r| r.threshold).min() else {
            if let Some(id) = self.slow_toast.take() {
                self.toasts.close(id);
            }
            return;
        };
        let count = requests.len();
        let mut toast = Toast::new(Kind::Warning, "Some requests are slow").description(format!(
            "{count} request{} waiting longer than {}s.",
            if count == 1 { "" } else { "s" },
            threshold.as_secs()
        ));
        toast.timeout = None;
        toast.details = Some(
            requests
                .iter()
                .map(|r| {
                    let started: chrono::DateTime<chrono::Local> = r.started.into();
                    format!("{}\nStarted {}", r.method, started.format("%X"))
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
        toast.details_labels = ("Show requests", "Hide requests");
        match self.slow_toast {
            Some(id) => self.toasts.update(id, toast),
            None => self.slow_toast = Some(self.toasts.add(toast)),
        }
    }

    /// `LegacyThreadMigrationToast`: the server imports the previous
    /// version's threads; a loading toast says so until it is done.
    fn lifecycle_event(&mut self, value: &Value) {
        use crate::ui::toasts::{Kind, Toast};
        if value["type"] != "legacyThreadMigration" {
            return;
        }
        let payload = &value["payload"];
        if payload["status"] != "running" {
            if let Some(id) = self.migration_toast.take() {
                self.toasts.close(id);
            }
            return;
        }
        if self.migration_toast.is_some() {
            return;
        }
        let count = payload["totalThreadCount"].as_u64().unwrap_or(0);
        let description = format!(
            "Migrating {} {} from the previous version. You can keep working while this finishes.",
            group_digits(count),
            if count == 1 { "thread" } else { "threads" }
        );
        self.migration_toast = Some(
            self.toasts
                .add(Toast::new(Kind::Loading, "Restoring your threads…").description(description)),
        );
    }

    /// Follows the workspace's VCS status again, for a fresh snapshot.
    pub(crate) fn refresh_vcs(&mut self) {
        self.watched_vcs = None;
    }

    /// Tries the connection again now (the notice's "Reconnect").
    pub(crate) fn reconnect_now(&mut self) {
        if let Some(reconnect) = &mut self.reconnect {
            reconnect.next = std::time::Instant::now();
        }
    }

    /// Gives up on a server that does not answer, for the connection screen.
    pub(crate) fn change_server(&mut self) {
        self.reconnect = None;
        self.connection = None;
        self.synced = false;
        self.screen = Screen::Connect;
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
        self.design_home.open = false;
        if self.selected == id {
            return;
        }
        // Opening another thread is a step the history can go back over.
        if !self.navigating {
            self.nav_back.push(self.selected.clone());
            if self.nav_back.len() > 100 {
                self.nav_back.remove(0);
            }
            self.nav_forward.clear();
        }
        // The composer keeps each thread's draft.
        let from = self.composer_key(false);
        self.swap_drafts(&from, id.as_deref().unwrap_or("draft"));
        self.selected = id.clone();
        self.state.switch_detail(id.as_deref());
        self.expansion = Expansion::default();
        if id.is_some() {
            self.draft_project_id = None;
        }
        self.send(Action::Select(id));
    }

    /// Reading a thread clears its Done badge. A server that tracks visits
    /// takes the thread's time as the watermark (`thread.visit`, once per
    /// time, not while a turn streams); otherwise the visit is stamped
    /// locally at the run's completion time (`ChatView.tsx`).
    fn mark_open_thread_visited(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        if thread.last_visited_at.is_some() {
            let key = (thread.id.clone(), thread.updated_at.clone());
            if thread.is_running() || self.visited_turn.as_ref() == Some(&key) {
                return;
            }
            self.visited_turn = Some(key);
            let mut command = crate::model::command("thread.visit", &thread.id);
            command["visitedAt"] = json!(thread.updated_at);
            self.dispatch(command);
            return;
        }
        let Some(completed_at) = thread
            .latest_run
            .as_ref()
            .and_then(|r| r.completed_at.clone())
        else {
            return;
        };
        let key = (thread.id.clone(), completed_at.clone());
        if self.visited_turn.as_ref() == Some(&key) {
            return;
        }
        self.visited_turn = Some(key);
        self.mark_visited(&thread.id, &completed_at);
    }

    /// `resolveThreadLastVisitedAt`: the server's watermark when it tracks
    /// visits, else the local one.
    pub(crate) fn last_visited(&self, thread: &crate::model::Thread) -> Option<String> {
        match &thread.last_visited_at {
            Some(server) => server.clone(),
            None => self.last_visited.get(&thread.id).cloned(),
        }
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
            .last_visited(thread)
            .and_then(|at| crate::timeline::parse_time(&at))
            .is_some_and(|visited| visited >= woke_time);
        (!seen && thread.settled_override.as_deref() != Some("settled")).then_some(woke)
    }

    /// `thread.pin`, at the top of the pinned block on servers that order
    /// pins (`threadPinReorder`).
    pub(crate) fn pin_command(&self, thread_id: &str) -> Value {
        let mut command = crate::model::command("thread.pin", thread_id);
        if self.capability("threadPinReorder")
            && let Some(key) = crate::ui::sidebar::top_pin_key(&self.state.threads)
        {
            command["orderKey"] = json!(key);
        }
        command
    }

    /// `useAcknowledgeThreadWoke`: the wake at `at` was seen, on the server
    /// when it tracks visits (it keeps the later time), else here.
    pub(crate) fn acknowledge_woke(&mut self, thread_id: &str, at: &str) {
        let tracked = self
            .state
            .threads
            .iter()
            .find(|t| t.id == thread_id)
            .is_some_and(|t| t.last_visited_at.is_some());
        if tracked {
            let mut command = crate::model::command("thread.visit", thread_id);
            command["visitedAt"] = json!(at);
            self.dispatch(command);
        } else {
            self.mark_visited(thread_id, at);
        }
    }

    /// `markThreadUnread`: the visit moves to just before the latest
    /// completion, on the server when it tracks visits.
    pub(crate) fn mark_unread(&mut self, thread: &crate::model::Thread) {
        if thread.last_visited_at.is_some() {
            self.dispatch(crate::model::command("thread.mark-unread", &thread.id));
            return;
        }
        let Some(completed) = thread
            .latest_run
            .as_ref()
            .and_then(|r| r.completed_at.as_deref())
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
            .latest_run
            .as_ref()
            .and_then(|r| r.completed_at.as_deref())
            .and_then(crate::timeline::parse_time)
        else {
            return false;
        };
        match self.last_visited(thread) {
            None => false,
            Some(at) => crate::timeline::parse_time(&at).is_none_or(|visited| completed > visited),
        }
    }

    pub(crate) fn start_draft(&mut self, project_id: Option<String>) {
        let project_id = project_id
            .or_else(|| self.current_thread().map(|t| t.project_id.clone()))
            .or_else(|| self.draft_project().map(|p| p.id));
        self.select_thread(None);
        // A draft clears the sidebar's selection.
        self.legacy_selection.clear();
        self.legacy_anchor = None;
        self.draft_project_id = project_id;
        // A new draft starts from its project's default or the last model
        // picked (`applyStickyState`), not from the last draft's pick.
        self.composer_selection.remove("draft");
        self.draft_env_mode = None;
        self.draft_workspace = None;
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

    /// The server runs on this machine (a loopback URL), so its paths are
    /// this machine's paths.
    pub(crate) fn server_is_local(&self) -> bool {
        url::Url::parse(self.connect.server.trim())
            .ok()
            .and_then(|url| match url.host()? {
                url::Host::Domain(host) => Some(host == "localhost"),
                url::Host::Ipv4(ip) => Some(ip.is_loopback()),
                url::Host::Ipv6(ip) => Some(ip.is_loopback()),
            })
            .unwrap_or(false)
    }

    /// Sends a command: project mutations go to `projects.mutate`, thread
    /// commands to `orchestration.dispatchCommand`.
    pub(crate) fn dispatch(&mut self, command: Value) {
        // An opposite action expires what could be undone.
        if let (Some(kind), Some(thread)) = (command["type"].as_str(), command["threadId"].as_str())
        {
            self.undo.invalidate(kind, thread);
        }
        if self.block_turn_starts && command["type"] == "message.dispatch" {
            self.notify(
                crate::ui::toasts::Kind::Warning,
                "Turn starts are blocked in this run",
            );
            return;
        }
        if command["type"]
            .as_str()
            .is_some_and(|t| t.starts_with("project."))
        {
            let key = format!(
                "project-mutation:{}",
                command["commandId"].as_str().unwrap_or_default()
            );
            self.send_request(key, "projects.mutate", command);
            return;
        }
        self.send(Action::Dispatch(command));
    }

    /// Loads the open thread's turns before its loaded window.
    pub(crate) fn load_earlier_history(&mut self) {
        self.load_earlier_history_through(None);
    }

    /// Loads older turns, through `entry` (a message or plan) when it is
    /// further back than one page.
    pub(crate) fn load_earlier_history_through(&mut self, entry: Option<&str>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(cursor) = self.state.begin_history_load(&id) else {
            return;
        };
        match &self.connection {
            Some(connection) => connection.load_history(&id, &cursor, entry),
            None => self
                .state
                .history_failed(&id, &cursor, "Environment is not connected.".into()),
        }
    }

    pub(crate) fn send(&mut self, action: Action) {
        match &action {
            Action::Stream {
                key,
                method,
                payload,
            } => {
                self.streams.retain(|(open, ..)| open != key);
                self.streams.push((key.clone(), method, payload.clone()));
            }
            Action::StopStream(key) => self.streams.retain(|(open, ..)| open != key),
            _ => {}
        }
        // Without a synchronized server a command fails at once (sent
        // early it would end the connection); subscriptions open once it is.
        // With no session at all (the connection screen), nothing goes.
        if !self.synced {
            let session = self.connection.is_some() || self.reconnect.is_some();
            match action {
                _ if !session => {}
                Action::Dispatch(command) => {
                    if let Some(id) = command["commandId"].as_str() {
                        self.offline_failures.push(Event::Failed(
                            Route::Command(id.to_owned()),
                            NOT_CONNECTED.into(),
                        ));
                    }
                }
                Action::Request { key, .. } => {
                    self.offline_failures
                        .push(Event::Failed(Route::Rpc(key), NOT_CONNECTED.into()));
                }
                _ => {}
            }
            return;
        }
        if let Some(connection) = &self.connection {
            connection.send(action);
        }
    }

    /// The open thread's shell row, or for a thread the shell does not
    /// list (an archived one) the row its projection gives.
    pub(crate) fn current_thread(&self) -> Option<Arc<Thread>> {
        let selected = self.selected.as_deref()?;
        self.state
            .threads
            .iter()
            .find(|t| t.id == selected)
            .cloned()
            .or_else(|| {
                self.current_detail()
                    .and_then(|d| d.shell_thread())
                    .map(Arc::new)
            })
    }

    /// The open thread's projection, once its subscription has delivered it.
    pub(crate) fn current_detail(&self) -> Option<Arc<crate::model::Detail>> {
        let selected = self.selected.as_deref()?;
        self.state.detail.clone().filter(|d| d.id() == selected)
    }

    pub(crate) fn blocks_turn_starts(&self) -> bool {
        self.block_turn_starts
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
        // Pluto: a draft follows the sidebar's mode. Chat opens a new chat;
        // Code takes the latest code thread's project, never a chat space,
        // which would show "How can I help?" in Code.
        if self.chat_mode() {
            return self.chats_project();
        }
        let code_project = |p: &&Project| {
            !self.is_chats(p) && !self.is_temporary_chats(p) && !self.is_chat_project(p)
        };
        let latest = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none())
            .filter(|t| {
                self.state
                    .project(&t.project_id)
                    .is_some_and(|p| code_project(&p))
            })
            .max_by(|a, b| a.created_at.cmp(&b.created_at));
        latest
            .and_then(|t| self.state.project(&t.project_id))
            .or_else(|| self.state.projects.iter().find(code_project))
            .cloned()
    }

    pub(crate) fn workspace_cwd(&self) -> Option<String> {
        if let Some(thread) = self.current_thread() {
            if let Some(path) = &thread.worktree_path {
                return Some(path.clone());
            }
            return self
                .state
                .project(&thread.project_id)
                .map(|p| p.workspace_root.clone());
        }
        if let Some(path) = self.draft_workspace().and_then(|w| w.worktree.clone()) {
            return Some(path);
        }
        self.draft_project().map(|p| p.workspace_root)
    }

    pub(crate) fn connect_now(&mut self, ctx: &egui::Context) {
        // Another server: nothing of the last one's shell stays.
        if self
            .shell_server
            .as_deref()
            .is_some_and(|server| server != self.connect.server.trim())
        {
            self.shell_server = None;
            self.state = State::default();
            self.selected = None;
            self.thread_retry = None;
            self.streams.clear();
            self.vcs = None;
            self.watched_vcs = None;
        }
        self.connect.error = None;
        self.connect.busy = true;
        self.synced = false;
        self.reconnect = None;
        let token = self.connect.token.clone();
        match Connection::open(&self.connect.server, token, crate::repaint(ctx)) {
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

    /// Status messages join the toasts, which draw at the top right.
    fn status_toast(&mut self, ui: &mut egui::Ui, rect: Rect) {
        if let Some((kind, message)) = self.status.take() {
            self.toasts.notice(kind, message);
        }
        self.tick_git_toast(ui.ctx());
        let open = self.selected.clone();
        match self.toasts.show(ui, rect, open.as_deref()) {
            Some(crate::ui::toasts::Action::GitAction(action)) => {
                self.run_git_action(crate::ui::git_actions::Run {
                    action,
                    ..Default::default()
                });
            }
            Some(crate::ui::toasts::Action::OpenUrl(url)) => {
                crate::ui::widgets::open_external(&url)
            }
            Some(crate::ui::toasts::Action::OpenThread(thread_id)) => {
                self.settings_page = None;
                self.select_thread(Some(thread_id));
            }
            Some(crate::ui::toasts::Action::OpenInEditor(path)) => {
                if let Some(editor) = self.preferred_editor() {
                    self.open_in_editor(&path, editor);
                }
            }
            Some(crate::ui::toasts::Action::Dismiss) | None => {}
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self) {
        self.save_drafts_now();
        if let (Some(path), Some(([width, height], maximized))) = (
            window_path(self.client_settings_path.as_deref()),
            self.window,
        ) {
            let value =
                serde_json::json!({"width": width, "height": height, "maximized": maximized});
            let temp = path.with_extension("json.tmp");
            if std::fs::write(&temp, value.to_string()).is_ok() {
                let _ = std::fs::rename(&temp, &path);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        theme::set_blue_orange_diffs(self.client_setting("diffColorScheme") == "blue-orange");
        crate::ui::widgets::set_time_format(
            self.client_setting("timestampFormat")
                .as_str()
                .unwrap_or("locale"),
        );
        // The text menu's Copy lands as a copy event for this pass.
        if std::mem::take(&mut self.pending_copy) {
            ui.ctx().input_mut(|i| i.events.push(egui::Event::Copy));
        }
        self.now = chrono::Utc::now();
        // Back in the window: a connection that died meanwhile (a suspend,
        // a network change) shows within seconds rather than 45.
        let focused = ui.ctx().input(|i| i.focused);
        let (inner, maximized, fullscreen) = ui.ctx().input(|i| {
            let viewport = i.viewport();
            (viewport.inner_rect, viewport.maximized, viewport.fullscreen)
        });
        if let Some(maximized) = maximized {
            let size = match (maximized || fullscreen == Some(true), inner) {
                (false, Some(inner)) => Some([inner.width(), inner.height()]),
                _ => self.window.map(|(size, _)| size),
            };
            self.window = size.map(|size| (size, maximized));
        }
        let back = focused && !self.focused;
        self.focused = focused;
        if back && let Some(connection) = &self.connection {
            connection.send(Action::Ping);
        }
        self.poll_events();
        self.poll_thread_find(ui.ctx());
        self.thread_notifications(ui.ctx());
        self.egui_ctx = Some(ui.ctx().clone());
        self.apply_typography(ui.ctx());
        if self.connected() {
            self.request_favicons();
        }
        self.sync_theme(ui.ctx());
        self.save_drafts_when_idle(ui.ctx());
        self.poll_reconnect(ui.ctx());
        let rect = ui.max_rect();
        self.quit_hold(ui, rect);
        match self.screen {
            Screen::Connect => self.connect_screen(ui, rect),
            Screen::Main if self.settings_page.is_some() => {
                self.settings_screen(ui, rect);
                self.status_toast(ui, rect);
                self.memory_edit_dialog(ui, rect);
                self.confirm_dialog(ui, rect);
                self.rewind_dialog(ui, rect);
                self.snooze_dialog(ui, rect);
                self.add_instance_dialog(ui, rect);
                self.action_editor_dialog(ui, rect);
                self.scheduled_task_editor(ui, rect);
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
                // Below `md` the sidebar leaves the layout and opens as a sheet.
                self.mobile = rect.width() < 768.0;
                if !self.mobile {
                    self.sidebar_sheet_open = false;
                }
                let docked = self.sidebar_docked();
                let sidebar =
                    Rect::from_min_size(rect.min, Vec2::new(self.sidebar_width(), rect.height()));
                let workspace = if docked {
                    Rect::from_min_max(Pos2::new(sidebar.right(), rect.top()), rect.max)
                } else {
                    rect
                };
                // The right panel keeps 360px for the chat column (`PreviewPanelShell`).
                if !self.design_home.open {
                    self.apply_studio();
                }
                let panel_open = !self.usage_open
                    && !self.pull_requests_open
                    && !self.design_home.open
                    && self.panel_open()
                    && self.current_thread().is_some();
                // A design project's thread keeps only a narrow chat.
                let studio = panel_open && self.studio_active();
                // A maximized panel leaves the chat column no width.
                let maximized = panel_open && self.panel().maximized;
                // Too narrow for a 360px chat beside a 360px panel: the
                // panel covers the chat's right side, like a sheet.
                let covering = panel_open && !maximized && !studio && workspace.width() < 720.0;
                let main = if maximized {
                    Rect::from_min_max(
                        workspace.min,
                        Pos2::new(workspace.left(), workspace.bottom()),
                    )
                } else if studio {
                    self.panel_width = self.studio_panel_width(workspace.width());
                    Rect::from_min_max(
                        workspace.min,
                        Pos2::new(workspace.right() - self.panel_width, workspace.bottom()),
                    )
                } else if covering {
                    self.panel_width = self
                        .panel_width
                        .clamp(360.0_f32.min(workspace.width()), workspace.width());
                    workspace
                } else if panel_open {
                    // The chat column keeps 360px.
                    let max = (rect.width() * 0.7).min(workspace.width() - 360.0);
                    self.panel_width = self.panel_width.clamp(360.0, max.max(360.0));
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
                } else if self.design_home.open {
                    self.design_home_page(ui, main);
                } else if maximized {
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
                    let left = if covering {
                        rect.right() - self.panel_width
                    } else {
                        main.right()
                    };
                    let panel = Rect::from_min_max(Pos2::new(left, rect.top()), rect.max);
                    if covering {
                        // Over the chat: a shadow on its edge.
                        let shadow = egui::epaint::Shadow {
                            offset: [0, 0],
                            blur: 24,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(90),
                        };
                        ui.painter()
                            .add(shadow.as_shape(panel, egui::CornerRadius::ZERO));
                        ui.painter().rect_filled(panel, 0.0, theme::background());
                    }
                    self.diff_panel(ui, panel);
                }
                if docked {
                    self.sidebar(ui, sidebar);
                    self.sidebar_resize_handle(ui, sidebar);
                } else {
                    self.sidebar_sheet(ui, rect, sidebar);
                }
                self.thread_menu(ui);
                self.text_menu(ui);
                self.cite_overlays(ui);
                self.poll_media_open();
                self.media_menu(ui);
                self.open_in_menu(ui);
                self.git_menu(ui);
                self.git_dialogs(ui, rect);
                self.plan_menu(ui);
                self.plan_save_dialog(ui, rect);
                self.publish_dialog(ui, rect);
                self.bubble_context_popover(ui);
                self.preview_end_frame();
                self.html_renders_end_frame(ui.input(|i| i.time));
                self.mcp_apps_end_frame(ui.input(|i| i.time));
                self.design_end_frame();
                self.status_toast(ui, workspace);
                self.command_palette(ui, rect);
                self.legacy_rename_dialog(ui, rect);
                self.legacy_grouping_dialog(ui, rect);
                self.chat_project_dialog(ui, rect);
                self.memory_edit_dialog(ui, rect);
                self.confirm_dialog(ui, rect);
                self.rewind_dialog(ui, rect);
                self.snooze_dialog(ui, rect);
                self.prices_dialog(ui, rect);
                self.action_editor_dialog(ui, rect);
                self.scheduled_task_editor(ui, rect);
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
    /// the stock dark palette; a built-in theme id takes that theme's dark
    /// variant (this client renders dark appearances only).
    fn sync_theme(&self, ctx: &egui::Context) {
        let palette = self
            .active_theme()
            .as_ref()
            .and_then(crate::theme::Palette::from_theme)
            .unwrap_or_else(crate::theme::pluto_stock);
        if crate::theme::set_palette(palette) {
            crate::theme::apply_palette(ctx);
            ctx.request_repaint();
        }
    }

    /// The theme the `defaultTheme` server setting names
    /// (`getThemeDefinition`: a built-in first, then the published themes);
    /// none on the stock theme.
    pub(crate) fn active_theme(&self) -> Option<Value> {
        let id = self.state.config["settings"]["defaultTheme"]
            .as_str()
            .unwrap_or_default();
        crate::theme::built_in_theme(id).or_else(|| {
            (!id.is_empty())
                .then(|| self.state.config["environmentThemes"].as_array())
                .flatten()
                .and_then(|themes| themes.iter().find(|t| t["id"] == id).cloned())
        })
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

    /// Opens the diff panel on the open thread's last run (dev benches).
    pub fn open_last_diff(&mut self) {
        let run = self
            .current_detail()
            .and_then(|d| d.checkpoints.iter().rev().find_map(|c| c.run_id.clone()));
        if let Some(run) = run {
            self.open_diff(&run, None);
        }
    }

    /// Starts a draft in a project and returns its composer model as
    /// "instance/model" (tests).
    pub fn draft_model_in(&mut self, project_id: &str) -> String {
        self.start_draft(Some(project_id.to_owned()));
        let selection = self.composer_model_selection(true);
        format!("{}/{}", selection.instance_id, selection.model)
    }

    /// Opens a workspace file in the right panel (dev benches).
    pub fn show_file(&mut self, path: &str) {
        self.open_file(path.to_owned());
    }

    /// Overrides a server setting in this client only (snapshots).
    pub fn override_server_setting(&mut self, key: &str, value: serde_json::Value) {
        self.state.config["settings"][key] = value;
    }

    /// Overrides a client setting in memory (snapshots).
    pub fn override_setting(&mut self, key: &str, value: serde_json::Value) {
        self.client_settings[key] = value;
    }

    /// Opens a workspace file with Markdown rendered (dev benches).
    pub fn show_rendered_file(&mut self, path: &str) {
        self.files.render_markdown = true;
        self.open_file(path.to_owned());
    }

    /// Shows a settings page, or closes settings (dev benches).
    pub fn show_settings(&mut self, page: Option<usize>) {
        match page {
            Some(page) => self.open_settings_page(page),
            None => self.settings_page = None,
        }
    }

    /// Whether the shell (and the selected thread, if any) has loaded.
    pub fn is_loaded(&self) -> bool {
        self.connected()
            && !self.state.projects.is_empty()
            && self
                .selected
                .as_ref()
                .is_none_or(|id| self.state.detail.as_ref().is_some_and(|d| d.id() == id))
    }

    /// Expands every settled-run fold (dev snapshots).
    pub fn expand_turns(&mut self) {
        self.keep_scroll = true;
        if let Some(detail) = self.current_detail() {
            for run in &detail.runs {
                self.expansion.turns.insert(run.id.clone());
            }
            // Responses imported without a run fold under their prompt.
            for row in &detail.visible {
                if row.item.kind == "user_message" && row.item.run_id.is_none() {
                    self.expansion.turns.insert(format!(
                        "runless:{}",
                        row.item.str("messageId").unwrap_or_default()
                    ));
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
        self.state.revision += 1;
        if let Some(thread) = self.state.threads.iter_mut().find(|t| t.id == id) {
            let thread = Arc::make_mut(thread);
            let runtime = thread.runtime.get_or_insert_with(Default::default);
            runtime.status = "running".into();
            runtime.active_run_id = Some("snapshot-run".into());
            thread.latest_run = Some(crate::model::RunSummary {
                run_id: "snapshot-run".into(),
                status: "running".into(),
                ..Default::default()
            });
        }
    }

    /// Gives the open thread a context usage report and its provider quota
    /// windows, for snapshots of the context window meter.
    pub fn simulate_context_usage(&mut self) {
        let instance = self.composer_model_selection(false).instance_id;
        let resets =
            |hours: i64| (chrono::Utc::now() + chrono::Duration::hours(hours)).to_rfc3339();
        if let Some(providers) = self.state.config["providers"].as_array_mut() {
            for provider in providers
                .iter_mut()
                .filter(|p| p["instanceId"] == instance.as_str())
            {
                provider["usageLimits"] = serde_json::json!({
                    "checkedAt": resets(0),
                    "windows": [
                        {"id": "five_hour", "kind": "session", "label": "Session", "usedPercent": 3, "resetsAt": resets(3)},
                        {"id": "seven_day", "kind": "weekly", "label": "Weekly", "usedPercent": 97, "resetsAt": resets(20)},
                        {"id": "seven_day_fable", "kind": "weekly", "label": "Weekly · Fable", "usedPercent": 0, "resetsAt": resets(20)},
                    ],
                });
            }
        }
        if let Some(detail) = self.state.detail.as_mut() {
            let detail = Arc::make_mut(detail);
            detail.thread["activeProviderThreadId"] = "snapshot-provider-thread".into();
            detail.provider_threads.push(serde_json::json!({
                "id": "snapshot-provider-thread",
                "contextUsage": {"usedTokens": 712_500, "maxTokens": 1_000_000, "autoCompactThreshold": 962_500},
            }));
        }
        self.state.revision += 1;
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

    /// Answers an RPC request without a server (`key` as the client asked).
    pub fn load_offline_reply(&mut self, key: &str, value: Value) {
        self.rpc_result(key.to_owned(), Ok(value));
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
    /// Opens the diff panel on a run's changes, revealing (expanding) `file`.
    pub(crate) fn open_diff(&mut self, run_id: &str, file: Option<String>) {
        let count = self.current_detail().and_then(|d| {
            d.checkpoints
                .iter()
                .find(|c| c.run_id.as_deref() == Some(run_id))
                .and_then(|c| c.app_run_ordinal)
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

    /// Starts `subscribeDiscoveredLocalServers` once (the sidebar's port
    /// indicators and the preview's local servers share it).
    pub(crate) fn watch_discovered_servers(&mut self) {
        if !self.discovery_stream && self.connected() {
            self.discovery_stream = true;
            self.send(crate::protocol::Action::Stream {
                key: "discovered-servers".into(),
                method: "subscribeDiscoveredLocalServers",
                payload: serde_json::json!({}),
            });
        }
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
        if self.thread_find_result(&key, &result)
            || self.turn_item_result(&key, &result)
            || self.turn_send_result(&key, &result)
            || self.scratch_result(&key, &result)
            || self.open_in_result(&key, &result)
            || self.git_result(&key, &result)
            || self.plan_result(&key, &result)
            || self.publish_result(&key, &result)
            || self.command_menu_result(&key, &result)
            || self.html_render_url_result(&key, &result)
            || self.mcp_app_result(&key, &result)
            || self.design_home_result(&key, &result)
            || self.design_result(&key, &result)
            || self.chat_project_result(&key, &result)
            || self.chat_mode_result(&key, &result)
            || self.thread_search_result(&key, &result)
            || self.branch_result(&key, &result)
            || self.scheduled_result(&key, &result)
            || self.memory_result(&key, &result)
            || self.chat_versions_result(&key, &result)
            || self.file_upload_result(&key, &result)
            || self.notice_result(&key, &result)
            || self.secret_result(&key, &result)
        {
            return;
        }
        if let Some(ctx) = self.egui_ctx.clone()
            && self.preview_url_result(&key, &result, &ctx)
        {
            return;
        }
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
        } else if key.starts_with("project-mutation:") {
            if let Err(error) = result {
                self.notify(crate::ui::toasts::Kind::Error, error);
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
                Arc::new(text.split('\n').map(str::to_owned).collect())
            });
            self.diff_contents.insert(key, lines);
        } else if key.starts_with("keybinding-") {
            if let Err(error) = result {
                self.notify(
                    crate::ui::toasts::Kind::Error,
                    format!("Could not save the keybinding: {error}"),
                );
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
        } else if key.starts_with("stop-workflow:") {
            if result.is_err() {
                self.toasts.add(crate::ui::toasts::Toast::new(
                    crate::ui::toasts::Kind::Error,
                    "Could not stop workflow",
                ));
            }
        } else if key == "storage-cleanup-report" {
            if let Ok(report) = result {
                self.storage_report = (!report.is_null()).then_some(report);
            }
        } else if key == "storage-cleanup-run" {
            self.storage_running = false;
            match result {
                Ok(report) => {
                    if let Some(failure) = report["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|e| e["outcome"] == "failed")
                    {
                        self.toasts.add(
                            crate::ui::toasts::Toast::new(
                                crate::ui::toasts::Kind::Error,
                                "Cleanup failed",
                            )
                            .description(failure["reason"].as_str().unwrap_or_default()),
                        );
                    }
                    self.storage_report = Some(report);
                }
                Err(error) => {
                    self.toasts.add(
                        crate::ui::toasts::Toast::new(
                            crate::ui::toasts::Kind::Error,
                            "Could not run cleanup",
                        )
                        .description(error),
                    );
                }
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
                Err(error) => self.notify(
                    crate::ui::toasts::Kind::Error,
                    format!("Could not initialize Git: {error}"),
                ),
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

/// `toLocaleString()`'s digit groups: 1234 → "1,234".
fn group_digits(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `window.json` beside the client settings file.
fn window_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("window.json"))
}

/// The window's size and maximized state at the last exit.
pub fn saved_window(settings: Option<&std::path::Path>) -> Option<([f32; 2], bool)> {
    let text = std::fs::read_to_string(window_path(settings)?).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let size = [
        value["width"].as_f64()? as f32,
        value["height"].as_f64()? as f32,
    ];
    let sane = size.iter().all(|side| (100.0..=16384.0).contains(side));
    sane.then_some((size, value["maximized"] == true))
}

/// `visited.json` beside the client settings file.
fn visits_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("visited.json"))
}

fn ui_state_path(settings: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    Some(settings?.with_file_name("ui-state.json"))
}

fn read_ui_state(settings: Option<&std::path::Path>) -> serde_json::Value {
    ui_state_path(settings)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_default()
}

/// The persisted UI state (upstream's `uiStateStore`): project expansion
/// and the manual project order.
fn load_ui_state(
    settings: Option<&std::path::Path>,
) -> (
    std::collections::HashMap<String, bool>,
    Vec<String>,
    Option<String>,
) {
    let value = read_ui_state(settings);
    (
        serde_json::from_value(value["projectExpandedById"].clone()).unwrap_or_default(),
        serde_json::from_value(value["projectOrder"].clone()).unwrap_or_default(),
        value["lastEditor"].as_str().map(str::to_owned),
    )
}

pub(crate) fn save_ui_state(app: &App) {
    let Some(path) = ui_state_path(app.client_settings_path.as_deref()) else {
        return;
    };
    let value = serde_json::json!({
        "projectExpandedById": app.legacy_project_expanded,
        "projectOrder": app.legacy_project_order,
        "lastEditor": app.last_editor,
        "renderMarkdown": app.files.render_markdown,
        "renderTable": app.files.render_table,
        "stickyModels": app.sticky_models,
        // Only the server version notices are kept across runs.
        "dismissedNotices": app
            .dismissed_notices
            .iter()
            .filter(|id| id.starts_with("server-version:"))
            .collect::<Vec<_>>(),
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

#[cfg(test)]
mod thread_retry_tests {
    use super::*;
    use crate::protocol::{Action, Connection, Event, Route};

    #[test]
    fn a_launched_thread_is_subscribed_again_once_the_server_lists_it() {
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
        let ctx = harness.ctx.clone();
        let (connection, actions, events) = Connection::fake(crate::repaint(&ctx));
        let app = harness.state_mut();
        app.connection = Some(connection);
        app.synced = true;
        app.select_thread(Some("new".into()));
        assert!(matches!(actions.try_recv(), Ok(Action::Select(Some(id))) if id == "new"));

        // The subscription failed: the server had not created the thread yet.
        events
            .send(Event::Failed(
                Route::Thread("new".into()),
                "Failed to load orchestration V2 thread new".into(),
            ))
            .unwrap();
        app.poll_events();
        assert!(app.status.is_none());
        assert!(actions.try_recv().is_err());

        // The shell lists it: one new subscription.
        app.state.threads.push(Arc::new(crate::model::Thread {
            id: "new".into(),
            ..Default::default()
        }));
        app.poll_events();
        assert!(matches!(actions.try_recv(), Ok(Action::Select(Some(id))) if id == "new"));
        app.poll_events();
        assert!(actions.try_recv().is_err());
    }

    fn app() -> egui_kittest::Harness<'static, App> {
        egui_kittest::Harness::builder().build_eframe(|cc| {
            App::new(
                cc,
                Options {
                    server: None,
                    token: None,
                    client_settings_path: None,
                    block_turn_starts: false,
                },
            )
        })
    }

    #[test]
    fn each_thread_keeps_its_draft() {
        let mut harness = app();
        let app = harness.state_mut();
        app.select_thread(Some("a".into()));
        app.composer = "for a".into();
        app.select_thread(Some("b".into()));
        assert!(app.composer.is_empty());
        app.composer = "for b".into();
        app.select_thread(None);
        app.composer = "a new thread".into();
        app.select_thread(Some("a".into()));
        assert_eq!(app.composer, "for a");
        app.select_thread(Some("b".into()));
        assert_eq!(app.composer, "for b");
        app.select_thread(None);
        assert_eq!(app.composer, "a new thread");
    }

    #[test]
    fn a_refused_message_comes_back_and_a_lost_session_reconnects() {
        let mut harness = app();
        let ctx = harness.ctx.clone();
        let (connection, actions, events) = Connection::fake(crate::repaint(&ctx));
        let app = harness.state_mut();
        app.connection = Some(connection);
        app.synced = true;
        app.screen = Screen::Main;
        app.select_thread(Some("t".into()));
        while actions.try_recv().is_ok() {}
        app.composer = "hello".into();
        let draft = app.take_composer();
        app.hold_sent("command:c1".into(), "t".into(), draft);
        events
            .send(Event::Failed(
                Route::Command("c1".into()),
                "Refused.".into(),
            ))
            .unwrap();
        app.poll_events();
        assert_eq!(app.composer, "hello");
        assert!(
            app.status
                .as_ref()
                .is_some_and(|(_, s)| s.contains("back in the composer"))
        );

        // A stream is open, then the connection drops.
        app.send(Action::Stream {
            key: "scheduled-tasks".into(),
            method: "scheduledTasks.subscribe",
            payload: serde_json::json!({}),
        });
        events.send(Event::Disconnected("gone".into())).unwrap();
        app.poll_events();
        assert!(app.screen == Screen::Main);
        assert!(app.reconnect.is_some() && !app.connected());
        // The notice waits 2 s before it shows.
        assert_eq!(app.composer_notices(true).len(), 0);
        if let Some(reconnect) = &mut app.reconnect {
            reconnect.since -= std::time::Duration::from_secs(3);
        }
        assert_eq!(app.composer_notices(true).len(), 1);
        // Meanwhile a command fails at once instead of ending the next connection.
        app.send(Action::Request {
            key: "mutation:x".into(),
            method: "server.getSettings",
            payload: serde_json::json!({}),
        });
        assert_eq!(app.offline_failures.len(), 1);

        // The next connection synchronizes: the stream and the thread come back.
        let (connection, actions, events) = Connection::fake(crate::repaint(&ctx));
        app.connection = Some(connection);
        events
            .send(Event::Data(
                Route::Config,
                serde_json::json!({"type": "snapshot", "config": {}}),
            ))
            .unwrap();
        app.poll_events();
        assert!(app.reconnect.is_none() && app.synced);
        assert!(
            matches!(actions.try_recv(), Ok(Action::Stream { key, .. }) if key == "scheduled-tasks")
        );
        app.state.threads.push(Arc::new(crate::model::Thread {
            id: "t".into(),
            ..Default::default()
        }));
        app.poll_events();
        let mut selected = false;
        while let Ok(action) = actions.try_recv() {
            selected |= matches!(action, Action::Select(Some(ref id)) if id == "t");
        }
        assert!(selected);
    }
}
