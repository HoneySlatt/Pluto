//! The legacy sidebar (`LegacySidebar.tsx`, on with Settings → General →
//! "Sidebar (legacy)"): the Search row, then per-project thread trees sorted
//! by `sortProjectsForSidebar` and `sortThreads` (`Sidebar.logic.ts`,
//! `threadSort.ts`), with the status pills of `resolveThreadStatusPill` and
//! the "Sidebar options" sort menu.
//!
//! Not ported: project grouping across environments (every project is its
//! own entry), manual drag ordering ("Manual" keeps the server's order),
//! multi-select, file drops, PR, terminal, worktree and port indicators,
//! thread jump hints, the project context menu and the "Plan Ready" pill.
use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::{Project, Thread};
use crate::theme::{self, Weight, font};
use crate::timeline::parse_time;
use crate::ui::settings::Store;
use crate::ui::{project_icon, widgets};

const ROW: f32 = 32.0; // h-8
const GAP: f32 = 4.0; // gap-1
const MORE_ROW: f32 = 28.0; // SidebarMenuSubButton h-7
const SORT_MENU: &str = "legacy-sidebar-sort";
const AMBER_300: Color32 = Color32::from_rgb(0xff, 0xd2, 0x30);
const SKY_300: Color32 = Color32::from_rgb(0x74, 0xd4, 0xff);
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
        .or_else(|| {
            thread
                .messages
                .iter()
                .filter(|m| m.role == "user")
                .filter_map(|m| millis(Some(&m.created_at)))
                .max()
        })
        .or(millis(Some(&thread.updated_at)))
        .or(millis(Some(&thread.created_at)))
        .unwrap_or(i64::MIN)
}

/// `sortThreads`: newest first, then the larger id.
fn sort_threads(threads: &mut [&Thread], order: &str) {
    threads.sort_by(|a, b| {
        thread_timestamp(b, order)
            .cmp(&thread_timestamp(a, order))
            .then_with(|| b.id.cmp(&a.id))
    });
}

/// `sortProjectsForSidebar`: by their newest thread (or their own stamp),
/// ties by title then id; "manual" keeps the given order.
fn sort_projects(projects: &mut [&Project], threads: &[&Thread], order: &str) {
    if order == "manual" {
        return;
    }
    let stamp = |project: &Project| -> i64 {
        let own: Vec<i64> = threads
            .iter()
            .filter(|t| t.project_id == project.id)
            .map(|t| thread_timestamp(t, order))
            .collect();
        if let Some(latest) = own.into_iter().max() {
            return latest;
        }
        if order == "created_at" {
            millis(Some(&project.created_at)).unwrap_or(i64::MIN)
        } else {
            millis(Some(&project.updated_at))
                .or(millis(Some(&project.created_at)))
                .unwrap_or(i64::MIN)
        }
    };
    let stamps: HashMap<String, i64> = projects.iter().map(|p| (p.id.clone(), stamp(p))).collect();
    projects.sort_by(|a, b| {
        stamps[&b.id]
            .cmp(&stamps[&a.id])
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });
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
    /// `resolveThreadStatusPill` (without "Plan Ready").
    fn legacy_pill(&self, thread: &Thread) -> Option<Pill> {
        let pill = |label, color: Color32, alpha, pulse, priority| Pill {
            label,
            color: theme::alpha(color, alpha),
            pulse,
            priority,
        };
        let session = thread.session.as_ref().map(|s| s.status.as_str());
        let liveness = thread.background_liveness.as_deref();
        if thread.has_pending_approvals {
            Some(pill("Pending Approval", AMBER_300, 0.9, false, 6))
        } else if thread.has_pending_user_input {
            Some(pill("Awaiting Input", theme::INDIGO_300, 0.9, false, 5))
        } else if session == Some("running") || liveness == Some("working") {
            Some(pill("Working", SKY_300, 0.8, true, 4))
        } else if session == Some("starting") {
            Some(pill("Connecting", SKY_300, 0.8, true, 4))
        } else if liveness == Some("monitoring") {
            Some(pill("Monitoring", SKY_300, 0.8, false, 2))
        } else if self.is_unread(thread) {
            Some(pill("Completed", theme::EMERALD_300, 0.9, false, 1))
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

    /// `resolveProjectExpanded`: expanded unless collapsed before.
    fn legacy_expanded(&self, project_id: &str) -> bool {
        self.legacy_project_expanded
            .get(project_id)
            .copied()
            .unwrap_or(true)
    }

    fn set_legacy_expanded(&mut self, project_id: &str, expanded: bool) {
        self.legacy_project_expanded
            .insert(project_id.to_owned(), expanded);
        crate::app::save_ui_state(self);
    }

    /// The legacy Search row and project trees, between the header and the
    /// footer.
    pub(crate) fn legacy_sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let left = rect.left();
        // Search: opens the command palette.
        let search = Rect::from_min_size(Pos2::new(left + 8.0, 60.0), Vec2::new(239.0, 32.0));
        let response = ui.interact(search, ui.id().with("legacy-search"), Sense::click());
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(search, CornerRadius::same(8), sidebar_row_hover());
        }
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 18.0, 68.0), Vec2::splat(16.0)),
            16.0,
            if hovered {
                theme::sidebar_foreground()
            } else {
                theme::sidebar_icon()
            },
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(left + 42.0, 60.0), Pos2::new(left + 185.0, 92.0)),
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
            kbd(ui, &label, left + 237.0, 76.0);
        }
        if response.clicked() {
            self.toggle_palette(crate::ui::palette::View::Root);
        }

        let list = Rect::from_min_max(
            Pos2::new(left, 100.0),
            Pos2::new(rect.right() - 1.0, rect.bottom() - 48.0),
        );
        self.legacy_project_list(ui, list);
        self.legacy_project_menu(ui);
    }

    /// The project header's context menu: Rename, Copy Path, Project
    /// settings, Remove ("Group into..." needs project grouping, not ported).
    fn legacy_project_menu(&mut self, ui: &mut Ui) {
        use crate::ui::context_menu::{self, Item};
        let Some((project_id, state)) = self.legacy_project_menu.take() else {
            return;
        };
        let Some(project) = self.state.project(&project_id).cloned() else {
            return;
        };
        let items = [
            Item::new("rename", "Rename"),
            Item::new("copy-path", "Copy Path"),
            Item::new("project-settings", "Project settings").icon("settings"),
            Item {
                destructive: true,
                ..Item::new("delete", "Remove").icon("trash")
            },
        ];
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.legacy_project_menu = Some((project_id, state));
        }
        match clicked.as_deref() {
            Some("rename") => {
                self.legacy_project_rename = Some((project.id.clone(), project.title.clone(), true))
            }
            Some("copy-path") => {
                ui.ctx().copy_text(project.workspace_root.clone());
                self.status = Some(format!("Path copied: {}", project.workspace_root));
            }
            Some("project-settings") => self.open_project_settings(&project.id),
            Some("delete") => self.confirm_legacy_remove(&project),
            _ => {}
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
        });
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
                        "type": "project.meta.update",
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
            .filter(|t| t.archived_at.is_none())
            .collect();
        let mut projects: Vec<&Project> = self.state.projects.iter().collect();
        sort_projects(&mut projects, &thread_refs, &project_order);
        let selected = self.selected.as_deref();
        projects
            .iter()
            .map(|project| {
                let mut own: Vec<&Thread> = thread_refs
                    .iter()
                    .copied()
                    .filter(|t| t.project_id == project.id)
                    .collect();
                sort_threads(&mut own, &thread_order);
                let expanded = self.legacy_expanded(&project.id);
                let pinned = (!expanded)
                    .then(|| {
                        own.iter()
                            .copied()
                            .find(|t| selected == Some(t.id.as_str()))
                    })
                    .flatten();
                let list_open = self.legacy_thread_lists.contains(&project.id);
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
                    project: (*project).clone(),
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

        let mut toggle: Option<(String, bool)> = None;
        let mut new_thread: Option<String> = None;
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

                let mut y = area.top() + 28.0;
                for tree in &trees {
                    let header =
                        Rect::from_min_size(Pos2::new(left + 8.0, y), Vec2::new(239.0, ROW));
                    match self.legacy_project_header(
                        ui,
                        header,
                        &tree.project,
                        tree.expanded,
                        tree.pill,
                    ) {
                        Some(HeaderAction::Toggle) => {
                            toggle = Some((tree.project.id.clone(), !tree.expanded))
                        }
                        Some(HeaderAction::NewThread) => new_thread = Some(tree.project.id.clone()),
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
                            if ui.is_rect_visible(row) {
                                let jump = jump_labels.get(&thread.id).map(String::as_str);
                                self.legacy_thread_row(ui, row, thread, jump);
                            }
                            row_y += ROW + GAP;
                        }
                        if let Some((open, hidden)) = tree.more {
                            let row = Rect::from_min_size(
                                Pos2::new(left + 22.0, row_y),
                                Vec2::new(218.0, MORE_ROW),
                            );
                            if legacy_more_row(ui, row, &tree.project.id, open, hidden) {
                                more = Some((tree.project.id.clone(), !open));
                            }
                        }
                    }
                    y += panel + GAP;
                }
            });
        if let Some(anchor) = sort_anchor {
            self.legacy_sort_menu(ui, anchor);
        }
        if let Some((id, expanded)) = toggle {
            self.set_legacy_expanded(&id, expanded);
        }
        if let Some(id) = new_thread {
            self.start_draft(Some(id));
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
        project: &Project,
        expanded: bool,
        pill: Option<Pill>,
    ) -> Option<HeaderAction> {
        let response = ui.interact(
            rect,
            ui.id().with(("legacy-project", &project.id)),
            Sense::click(),
        );
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.open_menu = None;
            self.legacy_project_menu =
                Some((project.id.clone(), crate::ui::context_menu::State::new(at)));
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
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(rect.left() + 54.0, rect.top()),
                Pos2::new(rect.left() + 205.0, rect.bottom()),
            ),
            &project.title,
            font(14.0, Weight::Medium),
            theme::alpha(theme::sidebar_foreground(), 0.9),
        );
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
                ("legacy-new-thread", &project.id),
                "square-pen",
                14.0,
                theme::secondary_label(),
                &tooltip,
            )
            .clicked()
            {
                return Some(HeaderAction::NewThread);
            }
        }
        response.clicked().then_some(HeaderAction::Toggle)
    }

    /// A thread row: status label, title, and the relative time that the
    /// archive button replaces on hover.
    fn legacy_thread_row(&mut self, ui: &mut Ui, rect: Rect, thread: &Thread, jump: Option<&str>) {
        let id = ui.id().with(("legacy-thread", &thread.id));
        let response = ui.interact(rect, id, Sense::click());
        let hovered = ui.rect_contains_pointer(rect);
        let active = self.selected.as_deref() == Some(thread.id.as_str());
        let running = thread.is_running();
        if active {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::row_active());
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), sidebar_row_hover());
        }
        let mut x = rect.left() + 8.0;
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
        let title_color = if active || response.hovered() {
            theme::sidebar_foreground()
        } else {
            theme::alpha(theme::sidebar_muted(), 0.8)
        };
        let title_rect = Rect::from_min_max(
            Pos2::new(x, rect.top()),
            Pos2::new(rect.left() + 154.0, rect.bottom()),
        );
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
            self.thread_row_clicked(ui, thread);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.open_thread_menu(&thread.id, at);
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
    project: Project,
    expanded: bool,
    rows: Vec<Thread>,
    empty: bool,
    /// None, or (list expanded, the hidden threads' pill).
    more: Option<(bool, Option<Pill>)>,
    pill: Option<Pill>,
}

enum HeaderAction {
    Toggle,
    NewThread,
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
            created_at: "2026-01-01T00:00:00Z".into(),
            ..Project::default()
        };
        let p2 = Project {
            id: "p2".into(),
            title: "Beta".into(),
            created_at: "2026-01-02T00:00:00Z".into(),
            ..Project::default()
        };
        let t = thread("t", "p1", "2026-01-09T00:00:00Z", None);
        let mut projects = vec![&p2, &p1];
        sort_projects(&mut projects, &[&t], "updated_at");
        assert_eq!(projects[0].id, "p1");
        sort_projects(&mut projects, &[], "manual");
        assert_eq!(projects[0].id, "p1");
    }
}
