//! The legacy sidebar (`LegacySidebar.tsx`, on with Settings → General →
//! "Sidebar (legacy)"): the Search row, then per-project thread trees sorted
//! by `sortProjectsForSidebar` and `sortThreads` (`Sidebar.logic.ts`,
//! `threadSort.ts`), with the status pills of `resolveThreadStatusPill` and
//! the "Sidebar options" sort menu.
//!
//! Not ported: the preview panel the port globe opens upstream.
use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Project, Thread};
use crate::theme::{self, Weight, font};
use crate::timeline::parse_time;
use crate::ui::project_grouping::{self, ProjectGroup};
use crate::ui::settings::Store;
use crate::ui::{project_icon, widgets};

const ROW: f32 = 32.0; // h-8
const GAP: f32 = 4.0; // gap-1
const MORE_ROW: f32 = 28.0; // SidebarMenuSubButton h-7
const SORT_MENU: &str = "legacy-sidebar-sort";
const AMBER_300: Color32 = Color32::from_rgb(0xff, 0xd2, 0x30);
const SKY_300: Color32 = Color32::from_rgb(0x74, 0xd4, 0xff);
const VIOLET_300: Color32 = Color32::from_rgb(0xc4, 0xb4, 0xff);
const TEAL_300: Color32 = Color32::from_rgb(0x46, 0xec, 0xd5);
const EMERALD_400: Color32 = Color32::from_rgb(0x00, 0xd4, 0x92); // --success-foreground
const ICON_MUTED: Color32 = Color32::from_rgb(84, 84, 84); // --icon-muted

/// `SIDEBAR_SORT_LABELS` / `SIDEBAR_THREAD_SORT_LABELS`.
const PROJECT_SORTS: [(&str, &str); 3] = [
    ("updated_at", "Last user message"),
    ("created_at", "Created at"),
    ("manual", "Manual"),
];
const THREAD_SORTS: [(&str, &str); 2] = [
    ("updated_at", "Last user message"),
    ("created_at", "Created at"),
];

/// A `ThreadStatusPill`: label, dark-theme color and pulse.
#[derive(Clone, Copy)]
struct Pill {
    label: &'static str,
    color: Color32,
    pulse: bool,
    priority: u8,
}

fn millis(iso: Option<&str>) -> Option<i64> {
    iso.and_then(parse_time).map(|d| d.timestamp_millis())
}

/// `getThreadSortTimestamp`.
fn thread_timestamp(thread: &Thread, order: &str) -> i64 {
    if order == "created_at" {
        return millis(Some(&thread.created_at))
            .or(millis(Some(&thread.updated_at)))
            .unwrap_or(i64::MIN);
    }
    millis(thread.latest_user_message_at.as_deref())
        .or(millis(Some(&thread.updated_at)))
        .or(millis(Some(&thread.created_at)))
        .unwrap_or(i64::MIN)
}

/// `sortThreads`: newest first, then the larger id.
fn sort_threads(threads: &mut [&Thread], order: &str) {
    // Each stamp is parsed once, not in every comparison.
    threads.sort_by_cached_key(|&t| {
        (
            std::cmp::Reverse(thread_timestamp(t, order)),
            std::cmp::Reverse(t.id.as_str()),
        )
    });
}

/// `sortProjectsForSidebar`: groups by their newest thread (or their
/// representative's stamp), ties by title then key; "manual" keeps the
/// given order.
fn sort_groups(groups: &mut [ProjectGroup], threads: &[&Thread], order: &str) {
    if order == "manual" {
        return;
    }
    let thread_stamps: Vec<(&str, i64)> = threads
        .iter()
        .map(|t| (t.project_id.as_str(), thread_timestamp(t, order)))
        .collect();
    let stamp = |group: &ProjectGroup| -> i64 {
        let latest = thread_stamps
            .iter()
            .filter(|(project, _)| group.members.iter().any(|(_, m)| m.id == *project))
            .map(|(_, stamp)| *stamp)
            .max();
        if let Some(latest) = latest {
            return latest;
        }
        let project = &group.representative;
        if order == "created_at" {
            millis(Some(&project.created_at)).unwrap_or(i64::MIN)
        } else {
            millis(Some(&project.updated_at))
                .or(millis(Some(&project.created_at)))
                .unwrap_or(i64::MIN)
        }
    };
    let stamps: HashMap<String, i64> = groups.iter().map(|g| (g.key.clone(), stamp(g))).collect();
    groups.sort_by(|a, b| {
        stamps[&b.key]
            .cmp(&stamps[&a.key])
            .then_with(|| a.representative.title.cmp(&b.representative.title))
            .then_with(|| a.key.cmp(&b.key))
    });
}

/// `projectExpansionPreferenceKeys`: the group, its members' physical keys
/// and their legacy cwd keys.
fn expansion_keys(group: &ProjectGroup) -> Vec<String> {
    std::iter::once(group.key.clone())
        .chain(group.members.iter().map(|(key, _)| key.clone()))
        .chain(
            group
                .members
                .iter()
                .map(|(_, p)| project_grouping::legacy_cwd_key(&p.workspace_root)),
        )
        .collect()
}

/// `formatProjectMemberActionLabel`: the title alone, or the environment
/// and path when several projects share the entry.
fn member_label(project: &Project, members: usize, environment: Option<&str>) -> String {
    match environment {
        _ if members <= 1 => project.title.clone(),
        Some(label) => format!("{label} — {}", project.workspace_root),
        None => project.workspace_root.clone(),
    }
}

/// `PROJECT_GROUPING_MODE_LABELS`.
const GROUPING_MODES: [(&str, &str); 3] = [
    ("repository", "Group by repository"),
    ("repository_path", "Group by repository path"),
    ("separate", "Keep separate"),
];

/// `projectGroupingModeDescription`.
fn grouping_description(mode: &str) -> &'static str {
    match mode {
        "repository_path" => {
            "Projects group only when both the repository and repo-relative path match."
        }
        "separate" => "Every project path gets its own sidebar row.",
        _ => "Projects from the same repository share one sidebar row.",
    }
}

/// The status pulse (`animate-status-pulse`): 2s, full then half opacity,
/// each change in six steps.
fn pulse_alpha(time: f64) -> f32 {
    let t = (time % 2.0) / 2.0;
    let step = |from: f64| ((t - from) / 0.1 * 6.0).floor() / 6.0;
    (match t {
        t if t < 0.4 => 1.0,
        t if t < 0.5 => 1.0 - 0.5 * step(0.4),
        t if t < 0.9 => 0.5,
        _ => 0.5 + 0.5 * step(0.9),
    }) as f32
}

impl App {
    /// `resolveThreadStatusPill`.
    fn legacy_pill(&self, thread: &Thread) -> Option<Pill> {
        let pill = |label, color: Color32, alpha, pulse, priority| Pill {
            label,
            color: theme::alpha(color, alpha),
            pulse,
            priority,
        };
        let runtime = thread.runtime.as_ref().map(|r| r.status.as_str());
        let subagent = thread.pending_subagent_request.as_deref();
        // `THREAD_STATUS_PRIORITY`, doubled to stay whole.
        if thread.has_pending_approvals || subagent == Some("approval") {
            Some(pill("Pending Approval", AMBER_300, 0.9, false, 10))
        } else if thread.has_pending_user_input || subagent == Some("user_input") {
            Some(pill("Awaiting Input", theme::INDIGO_300, 0.9, false, 8))
        } else if matches!(runtime, Some("running" | "waiting")) {
            Some(pill("Working", SKY_300, 0.8, true, 6))
        } else if matches!(runtime, Some("preparing" | "starting" | "queued")) {
            Some(pill("Connecting", SKY_300, 0.8, true, 6))
        } else if thread
            .pending_background_tasks
            .iter()
            .any(|task| task["kind"] != "command")
        {
            Some(pill("Waiting", theme::sidebar_muted(), 1.0, false, 5))
        } else if !thread.has_pending_user_input
            && thread.interaction_mode == "plan"
            && thread.latest_run_settled()
            && thread.has_actionable_proposed_plan
        {
            Some(pill("Plan Ready", VIOLET_300, 0.9, false, 4))
        } else if self.is_unread(thread) {
            Some(pill("Completed", theme::EMERALD_300, 0.9, false, 2))
        } else {
            None
        }
    }

    /// `resolveProjectStatusIndicator`: the highest-priority pill.
    fn legacy_project_pill<'a>(&self, threads: impl Iterator<Item = &'a Thread>) -> Option<Pill> {
        threads.filter_map(|t| self.legacy_pill(t)).fold(
            None,
            |best: Option<Pill>, pill| match best {
                Some(b) if b.priority >= pill.priority => Some(b),
                _ => Some(pill),
            },
        )
    }

    /// `resolveProjectExpanded`: the first remembered key, else expanded.
    fn legacy_expanded(&self, keys: &[String]) -> bool {
        keys.iter()
            .find_map(|key| self.legacy_project_expanded.get(key).copied())
            .unwrap_or(true)
    }

    /// `setProjectExpanded`: every key of the entry takes the state.
    fn set_legacy_expanded(&mut self, keys: &[String], expanded: bool) {
        for key in keys {
            self.legacy_project_expanded.insert(key.clone(), expanded);
        }
        crate::app::save_ui_state(self);
    }

    fn environment_id(&self) -> String {
        self.state.config["environment"]["environmentId"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// Projects in the saved manual order (`orderItemsByPreferredIds`).
    fn legacy_ordered_projects(&self) -> Vec<&Project> {
        let projects: Vec<&Project> = self
            .state
            .projects
            .iter()
            .filter(|p| !self.is_chats(p))
            .collect();
        project_grouping::order_projects(
            &self.environment_id(),
            &projects,
            &self.legacy_project_order,
        )
    }

    /// `buildSidebarProjectSnapshots` over the ordered projects.
    /// The logical projects in sidebar order (`sortLogicalProjectsForSidebar`).
    pub(crate) fn sorted_project_groups(&self) -> Vec<ProjectGroup> {
        let order = self.client_setting("sidebarProjectSortOrder");
        let threads: Vec<&Thread> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none() && !t.is_subagent() && !self.is_chat_thread(t))
            .map(std::sync::Arc::as_ref)
            .collect();
        let mut groups = self.legacy_groups();
        sort_groups(
            &mut groups,
            &threads,
            order.as_str().unwrap_or("updated_at"),
        );
        groups
    }

    fn legacy_groups(&self) -> Vec<ProjectGroup> {
        let mode = self.client_setting("sidebarProjectGroupingMode");
        project_grouping::build_groups(
            &self.environment_id(),
            &self.legacy_ordered_projects(),
            mode.as_str().unwrap_or("repository"),
            &self.client_setting("sidebarProjectGroupingOverrides"),
        )
    }

    /// The legacy Search row and project trees, between the header and the
    /// footer.
    pub(crate) fn legacy_sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let left = rect.left();
        // The rows' terminal and port indicators (`useThreadRunningTerminalIds`,
        // `useThreadDiscoveredPorts`).
        if !self.legacy_streams && self.connected() {
            self.legacy_streams = true;
            self.send(crate::protocol::Action::Stream {
                key: "sidebar-terminal-metadata".into(),
                method: "subscribeTerminalMetadata",
                payload: json!({}),
            });
        }
        self.watch_discovered_servers();
        // Search: opens the command palette.
        let search = Rect::from_min_size(Pos2::new(left + 8.0, 96.0), Vec2::new(239.0, 32.0));
        let response = ui.interact(search, ui.id().with("legacy-search"), Sense::click());
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(search, CornerRadius::same(8), sidebar_row_hover());
        }
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 18.0, 104.0), Vec2::splat(16.0)),
            16.0,
            if hovered {
                theme::sidebar_foreground()
            } else {
                theme::sidebar_icon()
            },
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(left + 42.0, 96.0), Pos2::new(left + 185.0, 128.0)),
            "Search",
            font(14.0, Weight::Medium),
            if hovered {
                theme::sidebar_foreground()
            } else {
                theme::alpha(theme::sidebar_muted(), 0.8)
            },
        );
        let bindings = self.state.config["keybindings"].clone();
        let context = [("isDesktop", true)];
        if let Some(label) = crate::ui::shortcuts::effective_shortcut_label(
            &bindings,
            "commandPalette.toggle",
            &context,
        ) {
            kbd(ui, &label, left + 237.0, 112.0);
        }
        if response.clicked() {
            self.toggle_palette(crate::ui::palette::View::Root);
        }

        // Pluto: under the Code | Chat switch.
        let list = Rect::from_min_max(
            Pos2::new(left, 136.0),
            Pos2::new(rect.right() - 1.0, rect.bottom() - 48.0),
        );
        self.legacy_project_list(ui, list);
        self.legacy_project_menu(ui);
        self.legacy_new_thread_menu(ui);
        self.legacy_multi_menu(ui);
    }

    /// `useLinkedThreadPullRequest`'s fallback: the legacy linked pull request
    /// (servers without thread links) or the branch's, read with
    /// `pullRequests.summary` every minute and shaped like a synced link.
    fn legacy_fallback_pull_request(&mut self, thread: &Thread) -> Option<Value> {
        let links = self.capability("threadPullRequests");
        let reference = (!links)
            .then(|| thread.linked_pull_request.clone())
            .flatten()
            .or_else(|| thread.branch_pull_request.clone())?;
        let url = reference["url"].as_str()?.to_owned();
        let stale = self
            .pr_summaries
            .get(&url)
            .is_none_or(|(_, asked)| asked.elapsed() > std::time::Duration::from_secs(60));
        if stale {
            let mut input = json!({
                "projectId": reference["projectId"],
                "repository": reference["repository"],
                "number": reference["number"],
            });
            // `parseChangeRequestUrl`'s host.
            if let Some(host) = url
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .filter(|h| !h.is_empty())
            {
                input["host"] = json!(host);
            }
            let previous = self.pr_summaries.get(&url).and_then(|(s, _)| s.clone());
            self.pr_summaries
                .insert(url.clone(), (previous, std::time::Instant::now()));
            self.send_request(format!("pr-summary:{url}"), "pullRequests.summary", input);
        }
        let summary = self.pr_summaries.get(&url)?.0.clone()?;
        Some(json!({
            "number": summary["number"],
            "url": summary["url"],
            "snapshot": {
                "state": summary["state"],
                "isDraft": summary["isDraft"] == true,
                "title": summary["title"],
            },
        }))
    }

    /// `terminalMetadata` stream events: the snapshot, then upserts and removals.
    pub(crate) fn terminal_metadata_event(&mut self, event: &Value) {
        let key = |t: &Value| {
            (
                t["threadId"].as_str().unwrap_or_default().to_owned(),
                t["terminalId"].as_str().unwrap_or_default().to_owned(),
            )
        };
        match event["type"].as_str() {
            Some("snapshot") => {
                self.terminal_metadata = event["terminals"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| (key(t), t.clone()))
                    .collect();
            }
            Some("upsert") => {
                let terminal = &event["terminal"];
                self.terminal_metadata
                    .insert(key(terminal), terminal.clone());
            }
            Some("remove") => {
                self.terminal_metadata.remove(&key(event));
            }
            _ => {}
        }
    }

    /// `selectRunningSubprocessTerminalIds` is not empty for the thread.
    fn legacy_terminal_running(&self, thread_id: &str) -> bool {
        self.terminal_metadata
            .iter()
            .any(|((thread, _), t)| thread == thread_id && t["hasRunningSubprocess"] == true)
    }

    /// `rangeSelectTo`: from the anchor to `thread_id` within the entry,
    /// or a plain add when the anchor lies elsewhere.
    fn legacy_range_select(&mut self, thread_id: &str, ordered: &[String]) {
        let anchor_index = self
            .legacy_anchor
            .as_ref()
            .and_then(|a| ordered.iter().position(|id| id == a));
        let target_index = ordered.iter().position(|id| id == thread_id);
        match (anchor_index, target_index) {
            (Some(a), Some(t)) => {
                for id in &ordered[a.min(t)..=a.max(t)] {
                    self.legacy_selection.insert(id.clone());
                }
            }
            _ => {
                self.legacy_selection.insert(thread_id.to_owned());
                self.legacy_anchor = Some(thread_id.to_owned());
            }
        }
    }

    /// The selection's menu (`buildMultiSelectThreadContextMenuItems`).
    fn legacy_multi_menu(&mut self, ui: &mut Ui) {
        use crate::ui::context_menu::{self, Item};
        let Some(state) = self.legacy_multi_menu.take() else {
            return;
        };
        let threads: Vec<std::sync::Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| self.legacy_selection.contains(&t.id))
            .cloned()
            .collect();
        let count = threads.len();
        if count == 0 {
            return;
        }
        let items = [
            Item::new("mark-unread", format!("Mark unread ({count})")),
            Item {
                disabled: threads.iter().any(|t| t.is_running()),
                ..Item::new("archive", format!("Archive ({count})"))
            },
            Item {
                destructive: true,
                ..Item::new("delete", format!("Delete ({count})"))
            },
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        self.legacy_multi_menu = open;
        let commands = |kind: &str| -> Vec<Value> {
            threads
                .iter()
                .map(|t| crate::model::command(kind, &t.id))
                .collect()
        };
        let plural = if count == 1 { "" } else { "s" };
        match clicked.as_deref() {
            Some("mark-unread") => {
                for thread in &threads {
                    self.mark_unread(thread);
                }
                self.legacy_selection.clear();
                self.legacy_anchor = None;
            }
            Some(kind @ ("archive" | "delete")) => {
                let command_kind = if kind == "archive" {
                    "thread.archive"
                } else {
                    "thread.delete"
                };
                let mut list = commands(command_kind);
                let ask = if kind == "archive" {
                    self.client_setting("confirmThreadArchive") == true
                } else {
                    self.client_setting("confirmThreadDelete") != false
                };
                if ask {
                    let first = list.remove(0);
                    self.confirm = Some(crate::ui::thread_menu::Confirm {
                        title: if kind == "archive" {
                            format!("Archive {count} thread{plural}?")
                        } else {
                            format!("Delete {count} thread{plural}?")
                        },
                        message: if kind == "archive" {
                            String::new()
                        } else {
                            "This permanently clears conversation history for these threads.".into()
                        },
                        confirm_label: if kind == "archive" {
                            "Archive"
                        } else {
                            "Delete"
                        },
                        command: first,
                        more: list,
                    });
                } else {
                    let deleted_selected = kind == "delete"
                        && threads
                            .iter()
                            .any(|t| self.selected.as_deref() == Some(t.id.as_str()));
                    for command in list {
                        self.dispatch(command);
                    }
                    if deleted_selected {
                        self.start_draft(None);
                    }
                }
                self.legacy_selection.clear();
                self.legacy_anchor = None;
            }
            _ => {}
        }
    }

    /// The project header's context menu: Rename, Group into..., Copy Path,
    /// Project settings, Remove; with several projects in the entry, each
    /// targeted item opens a submenu of the members.
    fn legacy_project_menu(&mut self, ui: &mut Ui) {
        use crate::ui::context_menu::{self, Item};
        let Some((group_key, state)) = self.legacy_project_menu.take() else {
            return;
        };
        let Some(group) = self
            .legacy_groups()
            .into_iter()
            .find(|g| g.key == group_key)
        else {
            return;
        };
        let count = group.members.len();
        let environment = self.state.config["environment"]["label"]
            .as_str()
            .map(str::to_owned);
        let targeted =
            |action: &str, label: &str, icon: Option<&'static str>, destructive: bool| {
                let leaf = |project: &Project, label: String| Item {
                    destructive,
                    ..Item::new(format!("{action}:{}", project.id), label)
                };
                let item = if count == 1 {
                    leaf(&group.members[0].1, label.to_owned())
                } else {
                    Item {
                        children: group
                            .members
                            .iter()
                            .map(|(_, p)| leaf(p, member_label(p, count, environment.as_deref())))
                            .collect(),
                        ..Item::new(format!("{action}:submenu"), label)
                    }
                };
                Item {
                    icon,
                    destructive: destructive && count == 1,
                    ..item
                }
            };
        let items = [
            targeted("rename", "Rename", None, false),
            targeted("grouping", "Group into...", None, false),
            targeted("copy-path", "Copy Path", None, false),
            Item::new("project-settings", "Project settings").icon("settings"),
            targeted("delete", "Remove", Some("trash"), true),
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.legacy_project_menu = Some((group_key, state));
        }
        let Some(clicked) = clicked else {
            return;
        };
        if clicked == "project-settings" {
            self.open_project_settings(&group.representative.id);
            return;
        }
        let Some((action, project_id)) = clicked.split_once(':') else {
            return;
        };
        let Some(project) = group
            .members
            .iter()
            .map(|(_, p)| p)
            .find(|p| p.id == project_id)
            .cloned()
        else {
            return;
        };
        match action {
            "rename" => {
                self.legacy_project_rename = Some((project.id.clone(), project.title.clone(), true))
            }
            "grouping" => {
                let key = project_grouping::physical_key(&self.environment_id(), &project);
                let current = self.client_setting("sidebarProjectGroupingOverrides")[&key]
                    .as_str()
                    .unwrap_or("inherit")
                    .to_owned();
                self.legacy_grouping = Some((project.id.clone(), current));
            }
            "copy-path" => {
                ui.ctx().copy_text(project.workspace_root.clone());
                self.status = Some(format!("Path copied: {}", project.workspace_root));
            }
            "delete" => self.confirm_legacy_remove(&project),
            _ => {}
        }
    }

    /// A grouped project's New thread: pick the member to start in.
    fn legacy_new_thread_menu(&mut self, ui: &mut Ui) {
        use crate::ui::context_menu::{self, Item};
        let Some((group_key, state)) = self.legacy_new_thread_menu.take() else {
            return;
        };
        let Some(group) = self
            .legacy_groups()
            .into_iter()
            .find(|g| g.key == group_key)
        else {
            return;
        };
        let count = group.members.len();
        let environment = self.state.config["environment"]["label"].as_str();
        let items: Vec<Item> = group
            .members
            .iter()
            .map(|(key, p)| Item::new(key.clone(), member_label(p, count, environment)))
            .collect();
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.legacy_new_thread_menu = Some((group_key, state));
        }
        if let Some(key) = clicked
            && let Some((_, project)) = group.members.iter().find(|(k, _)| *k == key)
        {
            self.start_draft(Some(project.id.clone()));
        }
    }

    /// `handleRemoveProject`'s confirmation. Upstream first warns that a
    /// project with threads is not empty, with "Delete anyway" leading here.
    fn confirm_legacy_remove(&mut self, project: &Project) {
        let threads = self
            .state
            .threads
            .iter()
            .filter(|t| t.project_id == project.id)
            .count();
        let title = if threads > 0 {
            format!(
                "Remove project \"{}\" and delete its {threads} thread{}?",
                project.title,
                if threads == 1 { "" } else { "s" }
            )
        } else {
            format!("Remove project \"{}\"?", project.title)
        };
        let mut lines = vec![format!("Path: {}", project.workspace_root)];
        if let Some(label) = self.state.config["environment"]["label"].as_str() {
            lines.push(format!("Environment: {label}"));
        }
        if threads > 0 {
            lines.push("This permanently clears conversation history for those threads and any archived threads.".into());
            lines.push("This removes only this project entry.".into());
            lines.push("This action cannot be undone.".into());
        } else {
            lines.push("This permanently clears any archived conversation history.".into());
            lines.push("This removes only this project entry.".into());
        }
        self.confirm = Some(crate::ui::thread_menu::Confirm {
            title,
            message: lines.join("\n"),
            confirm_label: "Remove",
            command: json!({
                "type": "project.delete",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": project.id,
                "force": threads > 0,
            }),
            more: Vec::new(),
        });
    }

    /// Dropping a dragged entry moves its projects to the target's slot in
    /// `projectOrder` (`reorderProjects`).
    fn legacy_drop(&mut self, trees: &[Tree], from: Option<usize>, target: Option<usize>) {
        self.legacy_drag = None;
        let (Some(from), Some(target)) = (from, target) else {
            return;
        };
        let order: Vec<String> = self
            .legacy_ordered_projects()
            .iter()
            .map(|p| project_grouping::physical_key(&self.environment_id(), p))
            .collect();
        let keys = |tree: &Tree| -> Vec<String> {
            tree.group.members.iter().map(|(k, _)| k.clone()).collect()
        };
        if let Some(next) =
            project_grouping::reorder(&order, &keys(&trees[from]), &keys(&trees[target]))
        {
            self.legacy_project_order = next;
            crate::app::save_ui_state(self);
            // The entries already sit in their new slots.
            if let Some(ctx) = &self.egui_ctx {
                ctx.clear_animations();
            }
        }
    }

    /// The "Project grouping" dialog: the project's rule, or the global
    /// default (`saveProjectGroupingPreference`).
    pub(crate) fn legacy_grouping_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        use crate::ui::context_menu::{self, Item};
        let Some((project_id, mut selection)) = self.legacy_grouping.clone() else {
            return;
        };
        let Some(project) = self.state.project(&project_id).cloned() else {
            self.legacy_grouping = None;
            return;
        };
        let default_mode = self.client_setting("sidebarProjectGroupingMode");
        let default_mode = default_mode.as_str().unwrap_or("repository").to_owned();
        let mode_label = |mode: &str| {
            GROUPING_MODES
                .iter()
                .find(|(m, _)| *m == mode)
                .map_or("", |(_, label)| *label)
                .to_owned()
        };
        let trigger_label = if selection == "inherit" {
            format!("Use global default ({})", mode_label(&default_mode))
        } else {
            mode_label(&selection)
        };
        let description = format!(
            "Choose how {} should be grouped in the sidebar.",
            project.workspace_root
        );
        let description_height = widgets::galley(
            ui,
            &description,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
            462.0,
        )
        .size()
        .y;
        // Measured on the reference: the header, then label, select, hint.
        let panel_top = 53.0 + description_height + 12.0;
        let footer_top = panel_top + 4.0 + 16.0 + 6.0 + 32.0 + 16.0 + 16.0 + 24.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(512.0, footer_top + 65.0));
        let mut decision = None;
        let mut open_select: Option<Rect> = None;
        let select_open = self.legacy_grouping_select.is_some();
        egui::Area::new(ui.id().with("legacy-grouping-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, backdrop_response) =
                    ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if backdrop_response.clicked() && !ui.rect_contains_pointer(d) && !select_open {
                    decision = Some(false);
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close,
                    "legacy-grouping-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                {
                    decision = Some(false);
                }
                let left = d.left() + 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Project grouping",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                let galley = widgets::galley(
                    ui,
                    &description,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    462.0,
                );
                ui.painter().galley(
                    Pos2::new(left, d.top() + 53.0),
                    galley,
                    theme::muted_foreground(),
                );
                let mut y = d.top() + panel_top + 4.0;
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    "Grouping rule",
                    font(12.0, Weight::Medium),
                    theme::foreground(),
                    16.0,
                );
                y += 16.0 + 6.0;
                // `SelectTrigger`: the value, a chevron at the end.
                let trigger = Rect::from_min_size(Pos2::new(left, y), Vec2::new(462.0, 32.0));
                let response = ui.interact(
                    trigger,
                    ui.id().with("legacy-grouping-trigger"),
                    Sense::click(),
                );
                ui.painter().rect(
                    trigger,
                    CornerRadius::same(10),
                    if response.hovered() {
                        theme::overlay(0.05)
                    } else {
                        theme::alpha(Color32::WHITE, 0.0256)
                    },
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(trigger.left() + 12.0, trigger.top()),
                        Pos2::new(trigger.right() - 28.0, trigger.bottom()),
                    ),
                    &trigger_label,
                    font(14.0, Weight::Regular),
                    theme::foreground(),
                );
                theme::paint_icon(
                    ui,
                    "chevron-down",
                    Rect::from_center_size(
                        Pos2::new(trigger.right() - 14.0, trigger.center().y),
                        Vec2::splat(12.0),
                    ),
                    12.0,
                    theme::muted_foreground(),
                );
                if response.clicked() {
                    open_select = Some(trigger);
                }
                y += 32.0 + 16.0;
                let mode = if selection == "inherit" {
                    default_mode.as_str()
                } else {
                    selection.as_str()
                };
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    grouping_description(mode),
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                );
                let band = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.top() + footer_top),
                    d.max - Vec2::splat(1.0),
                );
                ui.painter().rect_filled(
                    band,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.016),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(band.min, Vec2::new(band.width(), 1.0)),
                    0.0,
                    theme::overlay(0.07),
                );
                use crate::ui::action_editor::{Kind, footer_button, footer_rect};
                let top = band.top() + 16.0;
                let save = footer_rect(ui, d.right() - 25.0, top, "Save");
                let cancel = footer_rect(ui, save.left() - 8.0, top, "Cancel");
                if footer_button(ui, save, "Save", Kind::Primary) {
                    decision = Some(true);
                }
                if footer_button(ui, cancel, "Cancel", Kind::Outline)
                    || (!select_open && ui.input(|i| i.key_pressed(egui::Key::Escape)))
                {
                    decision = Some(false);
                }
            });
        if let Some(trigger) = open_select {
            self.legacy_grouping_select = Some(
                context_menu::State::dropdown(Pos2::new(trigger.left(), trigger.bottom() + 4.0))
                    .with_min_width(trigger.width()),
            );
        }
        // `SelectPopup`: the global default, then each rule.
        let items: Vec<Item> = std::iter::once(("inherit", "Use global default".to_owned()))
            .chain(GROUPING_MODES.iter().map(|(m, l)| (*m, (*l).to_owned())))
            .map(|(value, label)| Item {
                selected: selection == value,
                ..Item::new(value, label)
            })
            .collect();
        if let Some(chosen) = context_menu::show(ui, &mut self.legacy_grouping_select, &items) {
            selection = chosen;
            self.legacy_grouping_select = None;
        }
        match decision {
            Some(true) => {
                let key = project_grouping::physical_key(&self.environment_id(), &project);
                let mut overrides = self.client_setting("sidebarProjectGroupingOverrides");
                if !overrides.is_object() {
                    overrides = json!({});
                }
                if selection == "inherit" {
                    if let Some(map) = overrides.as_object_mut() {
                        map.remove(&key);
                    }
                } else {
                    overrides[&key] = json!(selection);
                }
                self.update_setting(Store::Client, "sidebarProjectGroupingOverrides", overrides);
                self.legacy_grouping = None;
                self.legacy_grouping_select = None;
            }
            Some(false) => {
                self.legacy_grouping = None;
                self.legacy_grouping_select = None;
            }
            None => self.legacy_grouping = Some((project_id, selection)),
        }
    }

    /// The "Rename project" dialog.
    pub(crate) fn legacy_rename_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some((project_id, mut title, focus)) = self.legacy_project_rename.clone() else {
            return;
        };
        let Some(project) = self.state.project(&project_id).cloned() else {
            self.legacy_project_rename = None;
            return;
        };
        let environment = self.state.config["environment"]["label"]
            .as_str()
            .map(str::to_owned);
        let mut decision = None;
        // Measured on the reference: a 512px `DialogPopup`, the header to 84px,
        // the panel, then the footer band.
        let description = format!("Update the title for {}.", project.workspace_root);
        let description_height = widgets::galley(
            ui,
            &description,
            font(14.0, Weight::Regular),
            theme::muted_foreground(),
            20.0,
            462.0,
        )
        .size()
        .y;
        let panel_top = 53.0 + description_height + 12.0;
        let footer_top = panel_top
            + 4.0
            + 16.0
            + 6.0
            + 32.0
            + if environment.is_some() {
                16.0 + 16.0
            } else {
                0.0
            }
            + 24.0;
        let d = Rect::from_center_size(screen.center(), Vec2::new(512.0, footer_top + 65.0));
        egui::Area::new(ui.id().with("legacy-rename-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, backdrop_response) =
                    ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if backdrop_response.clicked() && !ui.rect_contains_pointer(d) {
                    decision = Some(false);
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    StrokeKind::Inside,
                );
                let close = Rect::from_min_size(
                    Pos2::new(d.right() - 41.0, d.top() + 9.0),
                    Vec2::splat(32.0),
                );
                if widgets::icon_button(
                    ui,
                    close,
                    "legacy-rename-close",
                    "x",
                    16.0,
                    theme::muted_foreground(),
                    "",
                )
                .clicked()
                {
                    decision = Some(false);
                }
                let left = d.left() + 25.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Rename project",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                let galley = widgets::galley(
                    ui,
                    &description,
                    font(14.0, Weight::Regular),
                    theme::muted_foreground(),
                    20.0,
                    462.0,
                );
                ui.painter().galley(
                    Pos2::new(left, d.top() + 53.0),
                    galley,
                    theme::muted_foreground(),
                );
                let mut y = d.top() + panel_top + 4.0;
                widgets::text(
                    ui,
                    Pos2::new(left, y),
                    "Project title",
                    font(12.0, Weight::Medium),
                    theme::foreground(),
                    16.0,
                );
                y += 16.0 + 6.0;
                let field = Rect::from_min_size(Pos2::new(left, y), Vec2::new(462.0, 32.0));
                let edit = crate::ui::action_editor::input(
                    ui,
                    field,
                    "legacy-rename-title",
                    &mut title,
                    "",
                    false,
                );
                if focus {
                    edit.request_focus();
                }
                y += 32.0 + 16.0;
                if let Some(label) = &environment {
                    widgets::text(
                        ui,
                        Pos2::new(left, y),
                        &format!("Environment: {label}"),
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                        16.0,
                    );
                }
                // `DialogFooter`: a lighter band under a hairline.
                let band = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.top() + footer_top),
                    d.max - Vec2::splat(1.0),
                );
                ui.painter().rect_filled(
                    band,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.016),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(band.min, Vec2::new(band.width(), 1.0)),
                    0.0,
                    theme::overlay(0.07),
                );
                use crate::ui::action_editor::{Kind, footer_button, footer_rect};
                let top = band.top() + 16.0;
                let save = footer_rect(ui, d.right() - 25.0, top, "Save");
                let cancel = footer_rect(ui, save.left() - 8.0, top, "Cancel");
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if footer_button(ui, save, "Save", Kind::Primary) || enter {
                    decision = Some(true);
                }
                if footer_button(ui, cancel, "Cancel", Kind::Outline)
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    decision = Some(false);
                }
            });
        match decision {
            Some(true) => {
                // `submitProjectRename`: a trimmed, non-empty, changed title.
                let trimmed = title.trim().to_owned();
                if trimmed.is_empty() {
                    self.status = Some("Project title cannot be empty".into());
                    self.legacy_project_rename = Some((project_id, title, true));
                    return;
                }
                if trimmed != project.title {
                    self.dispatch(json!({
                        "type": "project.update",
                        "commandId": uuid::Uuid::new_v4().to_string(),
                        "projectId": project.id,
                        "title": trimmed,
                    }));
                }
                self.legacy_project_rename = None;
            }
            Some(false) => self.legacy_project_rename = None,
            None => self.legacy_project_rename = Some((project_id, title, false)),
        }
    }

    /// Each project's tree, in sidebar order (`sortedProjects` with their
    /// rendered threads), decided before painting.
    fn legacy_trees(&self) -> Vec<Tree> {
        let project_order = self.client_setting("sidebarProjectSortOrder");
        let project_order = project_order.as_str().unwrap_or("updated_at").to_owned();
        let thread_order = self.client_setting("sidebarThreadSortOrder");
        let thread_order = thread_order.as_str().unwrap_or("updated_at").to_owned();
        let preview = self
            .client_setting("sidebarThreadPreviewCount")
            .as_u64()
            .unwrap_or(6)
            .clamp(1, 15) as usize;
        let thread_refs: Vec<&Thread> = self
            .state
            .threads
            .iter()
            .filter(|t| t.archived_at.is_none() && !t.is_subagent() && !self.is_chat_thread(t))
            .map(std::sync::Arc::as_ref)
            .collect();
        let mut groups = self.legacy_groups();
        sort_groups(&mut groups, &thread_refs, &project_order);
        let selected = self.selected.as_deref();
        groups
            .into_iter()
            .map(|group| {
                let mut own: Vec<&Thread> = thread_refs
                    .iter()
                    .copied()
                    .filter(|t| group.members.iter().any(|(_, m)| m.id == t.project_id))
                    .collect();
                sort_threads(&mut own, &thread_order);
                let keys = expansion_keys(&group);
                let expanded = self.legacy_expanded(&keys);
                let pinned = (!expanded)
                    .then(|| {
                        own.iter()
                            .copied()
                            .find(|t| selected == Some(t.id.as_str()))
                    })
                    .flatten();
                let list_open = self.legacy_thread_lists.contains(&group.key);
                let overflowing = own.len() > preview;
                let shown: Vec<&Thread> = if list_open || !overflowing {
                    own.clone()
                } else {
                    own[..preview].to_vec()
                };
                let rows: Vec<Thread> = match pinned {
                    Some(thread) => vec![thread.clone()],
                    None if expanded => shown.iter().map(|t| (*t).clone()).collect(),
                    None => Vec::new(),
                };
                let visible: HashSet<&str> = shown
                    .iter()
                    .chain(pinned.iter())
                    .map(|t| t.id.as_str())
                    .collect();
                let hidden = own
                    .iter()
                    .copied()
                    .filter(|t| !visible.contains(t.id.as_str()));
                Tree {
                    ordered: own.iter().map(|t| t.id.clone()).collect(),
                    group,
                    keys,
                    expanded,
                    empty: expanded && own.is_empty(),
                    more: (expanded && overflowing)
                        .then(|| (list_open, self.legacy_project_pill(hidden))),
                    pill: self.legacy_project_pill(own.iter().copied()),
                    rows,
                }
            })
            .collect()
    }

    /// The rendered rows in order, which `thread.jump.N` follows.
    pub(crate) fn legacy_visible_thread_ids(&self) -> Vec<String> {
        self.legacy_trees()
            .into_iter()
            .flat_map(|tree| tree.rows.into_iter().map(|t| t.id))
            .collect()
    }

    /// `buildThreadJumpLabelMap`, while held modifiers match a jump
    /// shortcut's (`shouldShowThreadJumpHintsForModifiers`); never while the
    /// terminal has focus.
    fn legacy_jump_labels(&self, ui: &Ui) -> HashMap<String, String> {
        let modifiers = ui.input(|i| i.modifiers);
        if self.terminal_focused() || modifiers.is_none() {
            return HashMap::new();
        }
        let bindings = &self.state.config["keybindings"];
        let context = [("isDesktop", true)];
        let held = (1..=9).any(|n| {
            crate::ui::shortcuts::effective_shortcut(
                bindings,
                &format!("thread.jump.{n}"),
                &context,
            )
            .is_some_and(|shortcut| crate::ui::shortcuts::modifiers_match(&shortcut, modifiers))
        });
        if !held {
            return HashMap::new();
        }
        self.legacy_visible_thread_ids()
            .into_iter()
            .take(9)
            .enumerate()
            .filter_map(|(index, id)| {
                crate::ui::shortcuts::effective_shortcut_label(
                    bindings,
                    &format!("thread.jump.{}", index + 1),
                    &context,
                )
                .map(|label| (id, label))
            })
            .collect()
    }

    fn legacy_project_list(&mut self, ui: &mut Ui, rect: Rect) {
        let trees = self.legacy_trees();
        let jump_labels = self.legacy_jump_labels(ui);
        let panel_height = |tree: &Tree| -> f32 {
            let mut items: Vec<f32> = tree.rows.iter().map(|_| ROW).collect();
            if tree.empty {
                items.push(ROW);
            }
            if tree.more.is_some() {
                items.push(MORE_ROW);
            }
            // The sub list stays mounted, so its `py-0.5` remains when empty.
            if items.is_empty() {
                4.0
            } else {
                items.iter().sum::<f32>() + GAP * (items.len() - 1) as f32 + 4.0
            }
        };
        let content_height = 28.0
            + trees.iter().map(|t| ROW + panel_height(t)).sum::<f32>()
            + GAP * trees.len().saturating_sub(1) as f32
            + 16.0;

        let mut toggle: Option<(Vec<String>, bool)> = None;
        let mut new_thread: Option<(String, Pos2)> = None;
        let manual = self.client_setting("sidebarProjectSortOrder") == "manual";
        // Manual sorting: header slots this frame and the drop target.
        let mut slots: Vec<(String, Rect)> = Vec::new();
        let mut dropped = false;
        // Thread rows and their controls keep the selection on a press
        // (`data-thread-item`, `data-thread-selection-safe`).
        let mut safe: Vec<Rect> = Vec::new();
        let mut more: Option<(String, bool)> = None;
        let mut sort_anchor = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        egui::ScrollArea::vertical()
            .id_salt("legacy-sidebar")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let (area, _) = ui.allocate_exact_size(
                    Vec2::new(rect.width(), content_height.max(rect.height())),
                    Sense::hover(),
                );
                let left = area.left();
                // "Projects", the sort menu and Add project.
                widgets::text(
                    ui,
                    Pos2::new(left + 16.0, area.top() + 4.0),
                    "Projects",
                    font(12.0, Weight::Medium),
                    theme::alpha(theme::sidebar_muted(), 0.8),
                    16.0,
                );
                let sort =
                    Rect::from_min_size(Pos2::new(left + 189.0, area.top()), Vec2::splat(24.0));
                let sort_response = ui
                    .interact(sort, ui.id().with("legacy-sort"), Sense::click())
                    .on_hover_text("Sidebar options");
                if sort_response.hovered() || self.open_menu.as_deref() == Some(SORT_MENU) {
                    ui.painter()
                        .rect_filled(sort, CornerRadius::same(6), theme::accent());
                }
                crate::ui::lucide::paint(
                    ui,
                    Rect::from_center_size(sort.center(), Vec2::splat(14.0)),
                    "arrow-up-down",
                    if sort_response.hovered() {
                        theme::foreground()
                    } else {
                        theme::muted_foreground()
                    },
                );
                if sort_response.clicked() {
                    self.open_menu = if self.open_menu.as_deref() == Some(SORT_MENU) {
                        None
                    } else {
                        Some(SORT_MENU.to_owned())
                    };
                }
                sort_anchor = Some(sort);
                let add =
                    Rect::from_min_size(Pos2::new(left + 217.0, area.top()), Vec2::splat(24.0));
                if widgets::icon_button(
                    ui,
                    add,
                    "legacy-add-project",
                    "folder-plus",
                    14.0,
                    theme::muted_foreground(),
                    "Add project",
                )
                .clicked()
                {
                    self.toggle_palette(crate::ui::palette::View::AddProject);
                }
                if trees.is_empty() {
                    let text = "No projects yet";
                    let width = widgets::text_width(ui, text, font(12.0, Weight::Regular));
                    widgets::text(
                        ui,
                        Pos2::new(area.center().x - width / 2.0, area.top() + 44.0),
                        text,
                        font(12.0, Weight::Regular),
                        theme::secondary_label(),
                        16.0,
                    );
                }

                // Base slots, then the manual drag's layout (`SortableProjectItem`,
                // dnd-kit's vertical list): the lifted entry follows the pointer
                // at 80% opacity, the ones it passes slide aside over 200ms, and
                // the entry under the pointer is ringed.
                let heights: Vec<f32> = trees.iter().map(|t| ROW + panel_height(t)).collect();
                let mut tops = Vec::with_capacity(trees.len());
                let mut cursor = area.top() + 28.0;
                for height in &heights {
                    tops.push(cursor);
                    cursor += height + GAP;
                }
                for (index, tree) in trees.iter().enumerate() {
                    slots.push((
                        tree.group.key.clone(),
                        Rect::from_min_size(
                            Pos2::new(left + 8.0, tops[index]),
                            Vec2::new(239.0, heights[index]),
                        ),
                    ));
                }
                let pointer = ui.input(|i| i.pointer.interact_pos());
                let from = self
                    .legacy_drag
                    .as_ref()
                    .and_then(|key| trees.iter().position(|t| &t.group.key == key));
                let target = from.and(pointer).and_then(|p| {
                    slots
                        .iter()
                        .position(|(_, r)| p.y < r.bottom() + GAP / 2.0)
                        .or(slots.len().checked_sub(1))
                });
                let positions: Vec<f32> = (0..trees.len())
                    .map(|index| {
                        let shift = match (from, target) {
                            (Some(f), Some(_)) if index == f => {
                                let top = pointer.map_or(tops[f], |p| p.y - self.legacy_drag_grab);
                                return top.clamp(area.top() + 28.0, cursor - GAP - heights[f]);
                            }
                            (Some(f), Some(t)) if f < t && index > f && index <= t => {
                                -(heights[f] + GAP)
                            }
                            (Some(f), Some(t)) if t < f && index >= t && index < f => {
                                heights[f] + GAP
                            }
                            _ => 0.0,
                        };
                        let animated = ui.ctx().animate_value_with_time(
                            ui.id().with(("legacy-slide", &trees[index].group.key)),
                            shift,
                            0.2,
                        );
                        tops[index] + animated
                    })
                    .collect();
                let mut order: Vec<usize> = (0..trees.len()).filter(|i| Some(*i) != from).collect();
                order.extend(from);
                for index in order {
                    let tree = &trees[index];
                    let mut y = positions[index];
                    let header =
                        Rect::from_min_size(Pos2::new(left + 8.0, y), Vec2::new(239.0, ROW));
                    let dragging = from == Some(index);
                    let opacity = ui.opacity();
                    if dragging {
                        ui.set_opacity(opacity * 0.8);
                    }
                    if target == Some(index) && !dragging {
                        ui.painter().rect_stroke(
                            Rect::from_min_size(header.min, Vec2::new(239.0, heights[index])),
                            CornerRadius::same(6),
                            Stroke::new(1.0, theme::alpha(theme::primary(), 0.4)),
                            StrokeKind::Outside,
                        );
                    }
                    match self.legacy_project_header(
                        ui,
                        header,
                        &tree.group,
                        tree.expanded,
                        tree.pill,
                        manual,
                    ) {
                        Some(HeaderAction::Toggle) => {
                            toggle = Some((tree.keys.clone(), !tree.expanded))
                        }
                        Some(HeaderAction::NewThread(at)) => {
                            new_thread = Some((tree.group.key.clone(), at))
                        }
                        Some(HeaderAction::DragStart) => {
                            self.legacy_drag = Some(tree.group.key.clone());
                            // From where the press began, like dnd-kit's transform.
                            self.legacy_drag_grab = ui
                                .input(|i| i.pointer.press_origin())
                                .or(pointer)
                                .map_or(0.0, |p| p.y - header.top());
                        }
                        Some(HeaderAction::DragEnd) => dropped = true,
                        None => {}
                    }
                    y += ROW;
                    let panel = panel_height(tree);
                    if panel > 0.0 {
                        // `border-l border-sidebar-border` along the sub list.
                        ui.painter().rect_filled(
                            Rect::from_min_size(Pos2::new(left + 12.0, y), Vec2::new(1.0, panel)),
                            0.0,
                            theme::sidebar_border(),
                        );
                        let mut row_y = y + 2.0;
                        if tree.empty {
                            widgets::text_in(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(left + 31.0, row_y),
                                    Vec2::new(202.0, ROW),
                                ),
                                "No threads yet",
                                font(12.0, Weight::Regular),
                                theme::alpha(theme::sidebar_muted(), 0.75),
                            );
                            row_y += ROW + GAP;
                        }
                        for thread in &tree.rows {
                            let row = Rect::from_min_size(
                                Pos2::new(left + 23.0, row_y),
                                Vec2::new(218.0, ROW),
                            );
                            safe.push(row);
                            if ui.is_rect_visible(row) {
                                let jump = jump_labels.get(&thread.id).map(String::as_str);
                                self.legacy_thread_row(ui, row, thread, jump, &tree.ordered);
                            }
                            row_y += ROW + GAP;
                        }
                        if let Some((open, hidden)) = tree.more {
                            let row = Rect::from_min_size(
                                Pos2::new(left + 22.0, row_y),
                                Vec2::new(218.0, MORE_ROW),
                            );
                            safe.push(row);
                            if legacy_more_row(ui, row, &tree.group.key, open, hidden) {
                                more = Some((tree.group.key.clone(), !open));
                            }
                        }
                    }
                    if dragging {
                        ui.set_opacity(opacity);
                    }
                }
                if dropped {
                    self.legacy_drop(&trees, from, target);
                } else if from.is_some() {
                    ui.ctx().request_repaint();
                }
            });
        // `shouldClearThreadSelectionOnMouseDown` and Escape clear it.
        if !self.legacy_selection.is_empty() {
            let pointer = ui.input(|i| i.pointer.interact_pos());
            let pressed = ui.input(|i| i.pointer.primary_pressed());
            let on_safe = pointer.is_some_and(|p| safe.iter().any(|r| r.contains(p)));
            let menu_open = self.legacy_multi_menu.is_some()
                || self.thread_menu.is_some()
                || self.confirm.is_some();
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) || (pressed && !on_safe && !menu_open)
            {
                self.legacy_selection.clear();
                self.legacy_anchor = None;
            }
        }
        if let Some(anchor) = sort_anchor {
            self.legacy_sort_menu(ui, anchor);
        }
        if let Some((keys, expanded)) = toggle {
            self.set_legacy_expanded(&keys, expanded);
        }
        if let Some((key, at)) = new_thread
            && let Some(tree) = trees.iter().find(|t| t.group.key == key)
        {
            if tree.group.members.len() == 1 {
                self.start_draft(Some(tree.group.members[0].1.id.clone()));
            } else {
                self.open_menu = None;
                self.legacy_new_thread_menu = Some((key, crate::ui::context_menu::State::new(at)));
            }
        }
        if let Some((id, open)) = more {
            if open {
                self.legacy_thread_lists.insert(id);
            } else {
                self.legacy_thread_lists.remove(&id);
            }
        }
    }

    /// A project header: the chevron (or the collapsed project's status
    /// dot, swapped for the chevron on hover), icon, name and, on hover, the
    /// new-thread button.
    fn legacy_project_header(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        group: &ProjectGroup,
        expanded: bool,
        pill: Option<Pill>,
        manual: bool,
    ) -> Option<HeaderAction> {
        let project = &group.representative;
        let response = ui.interact(
            rect,
            ui.id().with(("legacy-project", &group.key)),
            if manual {
                Sense::click_and_drag()
            } else {
                Sense::click()
            },
        );
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.open_menu = None;
            self.legacy_project_menu =
                Some((group.key.clone(), crate::ui::context_menu::State::new(at)));
        }
        if manual {
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(if response.dragged() {
                    egui::CursorIcon::Grabbing
                } else {
                    egui::CursorIcon::Grab
                });
            }
            if response.drag_started() {
                return Some(HeaderAction::DragStart);
            }
            if response.drag_stopped() {
                return Some(HeaderAction::DragEnd);
            }
        }
        let hovered = ui.rect_contains_pointer(rect);
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), sidebar_row_hover());
        }
        let chevron = Rect::from_min_size(
            Pos2::new(rect.left() + 8.0, rect.center().y - 7.0),
            Vec2::splat(14.0),
        );
        match pill {
            Some(pill) if !expanded && !hovered => {
                paint_dot(ui, chevron.center(), 4.5, pill);
                response.clone().on_hover_text(pill.label);
            }
            _ => theme::paint_icon(
                ui,
                if expanded {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                chevron,
                14.0,
                if expanded || pill.is_none() {
                    theme::alpha(theme::muted_foreground(), 0.7)
                } else {
                    ICON_MUTED
                },
            ),
        }
        project_icon::paint(
            ui,
            Rect::from_min_size(
                Pos2::new(rect.left() + 30.0, rect.top() + 8.0),
                Vec2::splat(16.0),
            ),
            project,
        );
        // The name truncates; "N projects" (`shrink-0 text-3xs`) stays whole
        // 8px after it.
        let badge = (group.members.len() > 1).then(|| format!("{} projects", group.members.len()));
        let badge_width = badge.as_ref().map_or(0.0, |b| {
            widgets::text_width(ui, b, font(10.0, Weight::Regular)) + 8.0
        });
        let name_width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(rect.left() + 54.0, rect.top()),
                Pos2::new(rect.left() + 205.0 - badge_width, rect.bottom()),
            ),
            &group.label,
            font(14.0, Weight::Medium),
            theme::alpha(theme::sidebar_foreground(), 0.9),
        );
        if let Some(badge) = badge {
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(rect.left() + 54.0 + name_width + 8.0, rect.top()),
                    Pos2::new(rect.left() + 213.0, rect.bottom()),
                ),
                &badge,
                font(10.0, Weight::Regular),
                theme::secondary_label(),
            );
        }
        if hovered {
            let button = Rect::from_min_size(
                Pos2::new(rect.right() - 26.0, rect.top() + 5.0),
                Vec2::splat(24.0),
            );
            let bindings = self.state.config["keybindings"].clone();
            let context = [("isDesktop", true)];
            let shortcut = crate::ui::shortcuts::effective_shortcut_label(
                &bindings,
                "chat.newLocal",
                &context,
            )
            .or_else(|| {
                crate::ui::shortcuts::effective_shortcut_label(&bindings, "chat.new", &context)
            });
            let tooltip = match shortcut {
                Some(label) => format!("New thread ({label})"),
                None => "New thread".to_owned(),
            };
            if widgets::icon_button(
                ui,
                button,
                ("legacy-new-thread", &group.key),
                "square-pen",
                14.0,
                theme::secondary_label(),
                &tooltip,
            )
            .clicked()
            {
                let at = ui
                    .input(|i| i.pointer.interact_pos())
                    .unwrap_or(button.center());
                return Some(HeaderAction::NewThread(at));
            }
        }
        response.clicked().then_some(HeaderAction::Toggle)
    }

    /// A thread row: status label, title, and the relative time that the
    /// archive button replaces on hover.
    fn legacy_thread_row(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        thread: &Thread,
        jump: Option<&str>,
        ordered: &[String],
    ) {
        let id = ui.id().with(("legacy-thread", &thread.id));
        let response = ui.interact(rect, id, Sense::click());
        let hovered = ui.rect_contains_pointer(rect);
        let active = self.selected.as_deref() == Some(thread.id.as_str());
        let running = thread.is_running();
        let selected = self.legacy_selection.contains(&thread.id);
        if active {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::row_active());
        } else if selected {
            // `--sidebar-row-selected`: the foreground at 7%.
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(6),
                theme::alpha(theme::sidebar_foreground(), 0.07),
            );
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), sidebar_row_hover());
        }
        let mut x = rect.left() + 8.0;
        // The current pull request's state glyph, a link to it; without a
        // thread link, the legacy or branch pull request's summary.
        let fallback = crate::ui::thread_pull_requests::current_link(&thread.pull_requests)
            .is_none()
            .then(|| self.legacy_fallback_pull_request(thread))
            .flatten();
        if let Some(link) = fallback
            .as_ref()
            .or(crate::ui::thread_pull_requests::current_link(
                &thread.pull_requests,
            ))
        {
            let (icon, tone, tooltip) = crate::ui::thread_pull_requests::status(link).unwrap_or((
                "git-pull-request-arrow",
                theme::muted_foreground(),
                format!(
                    "PR #{}, status pending",
                    link["number"].as_u64().unwrap_or(0)
                ),
            ));
            let glyph = Rect::from_min_size(Pos2::new(x, rect.center().y - 6.0), Vec2::splat(12.0));
            let pr = ui
                .interact(glyph.expand(2.0), id.with("pr"), Sense::click())
                .on_hover_text(tooltip);
            theme::paint_icon(ui, icon, glyph, 12.0, tone);
            if pr.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if pr.clicked()
                && let Some(url) = link["url"].as_str()
            {
                widgets::open_external(url);
            }
            x += 12.0 + 6.0;
        }
        if let Some(pill) = self.legacy_pill(thread) {
            paint_dot(ui, Pos2::new(x + 3.0, rect.center().y), 3.0, pill);
            let width = widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x + 10.0, rect.top()), rect.max),
                pill.label,
                font(10.0, Weight::Regular),
                pill.color,
            );
            x += 10.0 + width + 6.0;
        }
        let title_color = if active || selected || response.hovered() {
            theme::sidebar_foreground()
        } else {
            theme::alpha(theme::sidebar_muted(), 0.8)
        };
        // Right of the title: ports, worktree and terminal glyphs, then the
        // 48px meta slot (`ml-auto flex gap-1.5`).
        let mut glyph_right = rect.right() - 8.0 - 48.0 - 6.0;
        let mut glyph = |ui: &Ui, paint: &dyn Fn(&Ui, Rect), tooltip: &str, key: &str| {
            let slot = Rect::from_min_size(
                Pos2::new(glyph_right - 12.0, rect.center().y - 6.0),
                Vec2::splat(12.0),
            );
            paint(ui, slot);
            glyph_right -= 12.0 + 6.0;
            ui.interact(slot, id.with(key), Sense::click())
                .on_hover_text(tooltip)
        };
        if self.legacy_terminal_running(&thread.id) {
            let alpha = pulse_alpha(ui.input(|i| i.time));
            glyph(
                ui,
                &|ui, slot| {
                    theme::paint_icon(
                        ui,
                        "terminal",
                        slot,
                        12.0,
                        theme::alpha(TEAL_300, 0.9 * alpha),
                    )
                },
                "Terminal process running",
                "terminal",
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(path) = thread
            .worktree_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            // `formatWorktreePathForDisplay`: the last path segment.
            let name = path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(path);
            let tooltip = match &thread.branch {
                Some(branch) => format!("Worktree: {name} ({branch})"),
                None => format!("Worktree: {name}"),
            };
            glyph(
                ui,
                &|ui, slot| {
                    crate::ui::lucide::paint(
                        ui,
                        slot,
                        "folder-git-2",
                        theme::alpha(theme::muted_foreground(), 0.4),
                    )
                },
                &tooltip,
                "worktree",
            );
        }
        let ports: Vec<Value> = self
            .discovered_servers
            .iter()
            .filter(|s| s["terminal"]["threadId"] == thread.id.as_str())
            .cloned()
            .collect();
        if let Some(first) = ports.first() {
            let more = if ports.len() > 1 {
                format!(" (+{})", ports.len() - 1)
            } else {
                String::new()
            };
            let tooltip = format!("Open localhost:{}{more}", first["port"]);
            let response = glyph(
                ui,
                &|ui, slot| crate::ui::lucide::paint(ui, slot, "globe-2", EMERALD_400),
                &tooltip,
                "port",
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            // `openDiscoveredPort`: the thread opens with the port in a
            // Browser tab (the system browser when none can run headless).
            if response.clicked()
                && let Some(url) = first["url"].as_str()
            {
                self.select_thread(Some(thread.id.clone()));
                if crate::preview::browser_binary().is_some() {
                    self.open_browser_tab(&ui.ctx().clone(), Some(url.to_owned()));
                } else {
                    widgets::open_external(url);
                }
            }
        }
        let title_rect = Rect::from_min_max(
            Pos2::new(x, rect.top()),
            Pos2::new(glyph_right + 6.0 - 8.0, rect.bottom()),
        );
        // Files dragged over the row (`makeWorkspaceFileDropHandlers`): a
        // ring, then the drop opens the thread with them in its composer.
        let pointer_in = ui
            .input(|i| i.pointer.latest_pos())
            .is_some_and(|p| rect.contains(p));
        if pointer_in && ui.input(|i| !i.raw.hovered_files.is_empty()) {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(6),
                Stroke::new(1.0, theme::alpha(theme::primary(), 0.7)),
                StrokeKind::Inside,
            );
        }
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        if pointer_in && !dropped.is_empty() {
            if self.selected.as_deref() != Some(thread.id.as_str()) {
                self.select_thread(Some(thread.id.clone()));
            }
            crate::ui::attachments::add_dropped_files(&self.incoming_images, ui.ctx(), dropped);
        }
        // Double click renames inline.
        let renaming = self.rename_editor(ui, &thread.id, title_rect);
        if !renaming {
            widgets::text_in(
                ui,
                title_rect,
                &thread.title,
                font(
                    14.0,
                    if active {
                        Weight::Medium
                    } else {
                        Weight::Regular
                    },
                ),
                title_color,
            );
        }
        if !renaming && ui.rect_contains_pointer(title_rect) {
            response.clone().on_hover_text(&thread.title);
        }
        let confirming = self.legacy_confirm_archive.as_deref() == Some(thread.id.as_str());
        if confirming && !hovered {
            self.legacy_confirm_archive = None;
        }
        let mut archive = false;
        if confirming && !running {
            let label_font = font(10.0, Weight::Medium);
            let width = widgets::text_width(ui, "Confirm", label_font.clone()) + 16.0;
            let button = Rect::from_min_size(
                Pos2::new(rect.right() - 4.0 - width, rect.center().y - 10.0),
                Vec2::new(width, 20.0),
            );
            let confirm = ui.interact(button, id.with("confirm"), Sense::click());
            let destructive = theme::error_foreground();
            ui.painter().rect_filled(
                button,
                CornerRadius::same(6),
                theme::alpha(destructive, if confirm.hovered() { 0.18 } else { 0.12 }),
            );
            widgets::text_in(
                ui,
                button.shrink2(Vec2::new(8.0, 0.0)),
                "Confirm",
                label_font,
                destructive,
            );
            archive = confirm.clicked();
        } else if hovered && !running {
            let button = Rect::from_min_size(
                Pos2::new(rect.right() - 26.0, rect.center().y - 12.0),
                Vec2::splat(24.0),
            );
            let confirm_first = self.client_setting("confirmThreadArchive") == true;
            if widgets::icon_button(
                ui,
                button,
                id.with("archive"),
                "archive",
                14.0,
                theme::secondary_label(),
                if confirm_first { "" } else { "Archive" },
            )
            .clicked()
            {
                if confirm_first {
                    self.legacy_confirm_archive = Some(thread.id.clone());
                } else {
                    archive = true;
                }
            }
        } else if let Some(label) = jump {
            jump_chip(ui, label, rect.right() - 8.0, rect.center().y);
        } else {
            let at = thread
                .latest_user_message_at
                .as_deref()
                .unwrap_or(&thread.updated_at);
            let label = crate::ui::palette::relative_ago(at, self.now);
            let label_font = font(10.0, Weight::Regular);
            let width = widgets::text_width(ui, &label, label_font.clone());
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(rect.right() - 8.0 - width, rect.top()),
                    Pos2::new(rect.right() - 8.0 + 1.0, rect.bottom()),
                ),
                &label,
                label_font,
                if active {
                    theme::foreground()
                } else {
                    theme::secondary_label()
                },
            );
        }
        if archive {
            self.legacy_confirm_archive = None;
            self.dispatch(crate::model::command("thread.archive", &thread.id));
            return;
        }
        if response.clicked() {
            // `handleThreadClick`: Ctrl toggles, Shift selects a range, a
            // plain click clears the selection and opens the thread.
            let modifiers = ui.input(|i| i.modifiers);
            if modifiers.command {
                if !self.legacy_selection.remove(&thread.id) {
                    self.legacy_selection.insert(thread.id.clone());
                    self.legacy_anchor = Some(thread.id.clone());
                }
            } else if modifiers.shift {
                self.legacy_range_select(&thread.id, ordered);
            } else {
                self.legacy_selection.clear();
                self.legacy_anchor = Some(thread.id.clone());
                self.thread_row_clicked(ui, thread);
            }
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            if selected && !self.legacy_selection.is_empty() {
                self.open_menu = None;
                self.legacy_multi_menu = Some(crate::ui::context_menu::State::new(at));
            } else {
                self.legacy_selection.clear();
                self.legacy_anchor = None;
                self.open_thread_menu(&thread.id, at);
            }
        }
        if self.legacy_pill(thread).is_some_and(|p| p.pulse) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// "Sidebar options": project and thread sort orders, visible threads.
    fn legacy_sort_menu(&mut self, ui: &mut Ui, anchor: Rect) {
        if self.open_menu.as_deref() != Some(SORT_MENU) {
            return;
        }
        let project_order = self.client_setting("sidebarProjectSortOrder");
        let thread_order = self.client_setting("sidebarThreadSortOrder");
        let preview = self
            .client_setting("sidebarThreadPreviewCount")
            .as_i64()
            .unwrap_or(6);
        // Measured on the reference: 160 wide, 4px padding inside the border.
        let width = 160.0;
        let height = 266.0;
        let rect = Rect::from_min_size(
            Pos2::new(anchor.right() - width, anchor.bottom() + 4.0),
            Vec2::new(width, height),
        );
        let mut open = self.open_menu.take();
        let mut change: Option<(&str, Value)> = None;
        widgets::popup(ui, &mut open, SORT_MENU, rect, anchor, |ui, rect| {
            let mut y = rect.top() + 5.0;
            let header = |ui: &mut Ui, y: f32, text: &str| {
                widgets::text_in(
                    ui,
                    Rect::from_min_size(
                        Pos2::new(rect.left() + 13.0, y),
                        Vec2::new(width - 26.0, 24.0),
                    ),
                    text,
                    font(12.0, Weight::Medium),
                    theme::muted_foreground(),
                );
            };
            let radio = |ui: &mut Ui, y: f32, key: &str, label: &str, checked: bool| -> bool {
                let row = Rect::from_min_size(
                    Pos2::new(rect.left() + 5.0, y),
                    Vec2::new(width - 10.0, 28.0),
                );
                let response = ui.interact(
                    row,
                    ui.id().with(("legacy-sort-item", key, label)),
                    Sense::click(),
                );
                // `MenuRadioItem`: the checked item keeps an 8% fill.
                if response.hovered() {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(6), theme::accent());
                } else if checked {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(6),
                        theme::alpha(theme::foreground(), 0.08),
                    );
                }
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(row.left() + 8.0, row.top()), row.max),
                    label,
                    font(14.0, Weight::Regular),
                    theme::foreground(),
                );
                response.clicked()
            };
            header(ui, y, "Sort projects");
            y += 24.0;
            for (value, label) in PROJECT_SORTS {
                if radio(ui, y, "project", label, project_order == value) {
                    change = Some(("sidebarProjectSortOrder", json!(value)));
                }
                y += 28.0;
            }
            header(ui, y + 4.0, "Sort threads");
            y += 28.0;
            for (value, label) in THREAD_SORTS {
                if radio(ui, y, "thread", label, thread_order == value) {
                    change = Some(("sidebarThreadSortOrder", json!(value)));
                }
                y += 28.0;
            }
            header(ui, y + 4.0, "Visible threads");
            y += 28.0;
            // A number field: decrement, value, increment (1 to 15).
            let field = Rect::from_min_size(
                Pos2::new(rect.left() + 13.0, y + 4.0),
                Vec2::new(112.0, 28.0),
            );
            ui.painter().rect(
                field,
                CornerRadius::same(10),
                theme::alpha(Color32::WHITE, 0.0256),
                Stroke::new(1.0, theme::input()),
                StrokeKind::Inside,
            );
            let minus = Rect::from_min_size(field.min, Vec2::new(34.0, 28.0));
            let plus = Rect::from_min_size(
                Pos2::new(field.right() - 34.0, field.top()),
                Vec2::new(34.0, 28.0),
            );
            if widgets::icon_button(
                ui,
                minus,
                "legacy-preview-minus",
                "minus",
                14.0,
                theme::foreground(),
                "Decrease visible thread count",
            )
            .clicked()
                && preview > 1
            {
                change = Some(("sidebarThreadPreviewCount", json!(preview - 1)));
            }
            if widgets::icon_button(
                ui,
                plus,
                "legacy-preview-plus",
                "plus",
                14.0,
                theme::foreground(),
                "Increase visible thread count",
            )
            .clicked()
                && preview < 15
            {
                change = Some(("sidebarThreadPreviewCount", json!(preview + 1)));
            }
            let value = preview.to_string();
            let value_width = widgets::text_width(ui, &value, font(14.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(field.center().x - value_width / 2.0, field.top()),
                    field.max,
                ),
                &value,
                font(14.0, Weight::Regular),
                theme::foreground(),
            );
        });
        self.open_menu = open;
        if let Some((key, value)) = change {
            self.update_setting(Store::Client, key, value);
        }
    }
}

/// A project's tree in the legacy sidebar.
struct Tree {
    group: ProjectGroup,
    /// The entry's visible threads in order (`orderedProjectThreadKeys`),
    /// for Shift ranges.
    ordered: Vec<String>,
    /// `projectExpansionPreferenceKeys`.
    keys: Vec<String>,
    expanded: bool,
    rows: Vec<Thread>,
    empty: bool,
    /// None, or (list expanded, the hidden threads' pill).
    more: Option<(bool, Option<Pill>)>,
    pill: Option<Pill>,
}

enum HeaderAction {
    Toggle,
    NewThread(Pos2),
    DragStart,
    DragEnd,
}

/// `--sidebar-row-hover` in the dark theme: the foreground at 8%.
fn sidebar_row_hover() -> Color32 {
    theme::alpha(theme::sidebar_foreground(), 0.08)
}

/// A status dot, pulsing when the pill does.
fn paint_dot(ui: &Ui, center: Pos2, radius: f32, pill: Pill) {
    let alpha = if pill.pulse {
        pulse_alpha(ui.input(|i| i.time))
    } else {
        1.0
    };
    ui.painter()
        .circle_filled(center, radius, theme::alpha(pill.color, alpha));
}

/// "Show more" (with the hidden threads' status dot) or "Show less";
/// returns whether it was clicked.
fn legacy_more_row(
    ui: &mut Ui,
    rect: Rect,
    project_id: &str,
    open: bool,
    hidden: Option<Pill>,
) -> bool {
    let response = ui.interact(
        rect,
        ui.id().with(("legacy-more", project_id)),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), sidebar_row_hover());
    }
    let mut x = rect.left() + 8.0;
    if !open && let Some(pill) = hidden {
        paint_dot(ui, Pos2::new(x + 7.0, rect.center().y), 4.5, pill);
        x += 14.0 + 8.0;
    }
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
        if open { "Show less" } else { "Show more" },
        font(12.0, Weight::Regular),
        theme::sidebar_foreground(),
    );
    response.clicked()
}

/// A thread jump hint: a 20px pill (`border-border/80 bg-background/90`,
/// 10px mono) ending at `right`.
fn jump_chip(ui: &Ui, label: &str, right: f32, center_y: f32) {
    let label_font = theme::mono(10.0);
    let text_width = widgets::text_width(ui, label, label_font.clone());
    let rect = Rect::from_min_size(
        Pos2::new(right - text_width - 12.0, center_y - 10.0),
        Vec2::new(text_width + 12.0, 20.0),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        theme::alpha(theme::background(), 0.9),
        Stroke::new(1.0, theme::alpha(theme::border(), 0.8)),
        StrokeKind::Inside,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 6.0, rect.top()), rect.max),
        label,
        label_font,
        theme::foreground(),
    );
}

/// A `Kbd` chip (h-5, px-1, `bg-background`, 12px medium) ending at `right`.
fn kbd(ui: &Ui, label: &str, right: f32, center_y: f32) {
    let label_font = font(12.0, Weight::Medium);
    let text_width = widgets::text_width(ui, label, label_font.clone());
    let width = (text_width + 8.0).max(20.0);
    let rect = Rect::from_min_size(
        Pos2::new(right - width, center_y - 10.0),
        Vec2::new(width, 20.0),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), theme::background());
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.center().x - text_width / 2.0, rect.top()),
            rect.max,
        ),
        label,
        label_font,
        theme::muted_foreground(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, project: &str, created: &str, message: Option<&str>) -> Thread {
        Thread {
            id: id.into(),
            project_id: project.into(),
            created_at: created.into(),
            updated_at: created.into(),
            latest_user_message_at: message.map(str::to_owned),
            ..Thread::default()
        }
    }

    #[test]
    fn threads_sort_by_latest_user_message_then_creation() {
        let a = thread(
            "a",
            "p",
            "2026-01-01T00:00:00Z",
            Some("2026-01-05T00:00:00Z"),
        );
        let b = thread("b", "p", "2026-01-03T00:00:00Z", None);
        let mut list = vec![&b, &a];
        sort_threads(&mut list, "updated_at");
        assert_eq!(list[0].id, "a");
        sort_threads(&mut list, "created_at");
        assert_eq!(list[0].id, "b");
    }

    #[test]
    fn projects_follow_their_newest_thread() {
        let p1 = Project {
            id: "p1".into(),
            title: "Alpha".into(),
            workspace_root: "/w/alpha".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            ..Project::default()
        };
        let p2 = Project {
            id: "p2".into(),
            title: "Beta".into(),
            workspace_root: "/w/beta".into(),
            created_at: "2026-01-02T00:00:00Z".into(),
            ..Project::default()
        };
        let t = thread("t", "p1", "2026-01-09T00:00:00Z", None);
        let mut groups =
            project_grouping::build_groups("env", &[&p2, &p1], "separate", &serde_json::json!({}));
        sort_groups(&mut groups, &[&t], "updated_at");
        assert_eq!(groups[0].representative.id, "p1");
        sort_groups(&mut groups, &[], "manual");
        assert_eq!(groups[0].representative.id, "p1");
    }
}
