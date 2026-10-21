//! Right panel with the diff (`RightPanelTabs.tsx`, `DiffPanel.tsx`), rendered
//! like `@pierre/diffs` with the app's diff surface colors. The branch scope's
//! base-ref picker is not ported: the base is shown, not chosen.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2, text::LayoutJob};
use serde_json::{Value, json};

use crate::app::App;
use crate::diff::{self, Block, FileDiff, FileStatus, LineKind};
use crate::theme::{self, Weight, font};
use crate::ui::context_menu::{self, Item};
use crate::ui::widgets;

pub enum DiffLoad {
    Loading,
    Loaded(Vec<FileDiff>),
    Failed(String),
}

impl DiffLoad {
    pub fn from_result(result: Result<Value, String>) -> Self {
        match result {
            Ok(value) => DiffLoad::Loaded(diff::parse(value["diff"].as_str().unwrap_or_default())),
            Err(message) => DiffLoad::Failed(message),
        }
    }

    /// The `kind` source ("working-tree" or "branch-range") of a
    /// `review.getDiffPreview` result.
    pub fn from_preview(result: Result<Value, String>, kind: &str) -> Self {
        match result {
            Ok(value) => {
                let patch = value["sources"]
                    .as_array()
                    .and_then(|sources| sources.iter().find(|s| s["kind"] == kind))
                    .and_then(|source| source["diff"].as_str())
                    .unwrap_or_default();
                DiffLoad::Loaded(diff::parse(patch))
            }
            Err(message) => DiffLoad::Failed(message),
        }
    }
}

/// What the diff panel shows (`DiffPanelSelection`): the uncommitted working
/// tree, the branch against its base, or one turn's checkpoint diff.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiffScope {
    WorkingTree,
    Branch,
    /// A checkpoint turn count.
    Turn(u64),
}

const HEADER: f32 = 52.0;
const TOOLBAR: f32 = 28.0;
const FILE_HEADER: f32 = 32.0;
const LINE: f32 = 20.0;
const SEPARATOR: f32 = 24.0;
const MODIFIED: Color32 = Color32::from_rgb(0, 159, 255); // --diffs-modified-base
const ADDED: Color32 = Color32::from_rgb(7, 196, 128);
const DELETED: Color32 = Color32::from_rgb(255, 45, 62);
fn deletion_row() -> Color32 {
    theme::stock_or(Color32::from_rgb(28, 16, 15), |p| {
        theme::mix(p.background, DELETED, 0.08)
    })
}
fn addition_row() -> Color32 {
    theme::stock_or(Color32::from_rgb(12, 23, 19), |p| {
        theme::mix(p.background, ADDED, 0.08)
    })
}
fn deletion_emphasis() -> Color32 {
    theme::stock_or(Color32::from_rgb(73, 22, 25), |p| {
        theme::mix(p.background, DELETED, 0.25)
    })
}
fn addition_emphasis() -> Color32 {
    theme::stock_or(Color32::from_rgb(9, 56, 40), |p| {
        theme::mix(p.background, ADDED, 0.25)
    })
}
fn context_number() -> Color32 {
    theme::stock_or(Color32::from_rgb(122, 122, 122), |p| p.muted_foreground)
}
const STAT_ADDITION: Color32 = Color32::from_rgb(0, 188, 125);
const STAT_DELETION: Color32 = Color32::from_rgb(251, 65, 74);

impl App {
    /// Turn counts with checkpoints, newest first.
    fn diff_turns(&self) -> Vec<(u64, String)> {
        let mut turns: Vec<(u64, String)> = self
            .current_thread()
            .map(|t| {
                t.checkpoints
                    .iter()
                    .filter(|c| !c.files.is_empty() || c.checkpoint_turn_count > 0)
                    .map(|c| (c.checkpoint_turn_count, c.completed_at.clone()))
                    .collect()
            })
            .unwrap_or_default();
        turns.sort_by_key(|t| std::cmp::Reverse(t.0));
        turns
    }

    fn diff_key(&self, thread_id: &str, turn: u64) -> String {
        format!(
            "diff:{thread_id}:{}:{turn}:{}",
            turn.saturating_sub(1),
            self.diff_ignore_whitespace
        )
    }

    pub(crate) fn diff_panel(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::background());
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(1.0, rect.height())),
            0.0,
            theme::border(),
        );
        self.resize_handle(ui, rect);
        let inner = Rect::from_min_max(Pos2::new(rect.left() + 1.0, rect.top()), rect.max);

        // No surface open: the launcher fills the panel.
        let panel = self.panel();
        let Some(active) = panel.active_tab().cloned() else {
            self.panel_controls(ui, rect.right());
            let body = Rect::from_min_max(Pos2::new(inner.left(), HEADER), inner.max);
            self.surface_launcher(ui, body, HEADER);
            return;
        };
        self.launcher_focused = false;
        self.panel_tab_bar(ui, inner);
        self.panel_controls(ui, rect.right());
        if let crate::app::PanelTab::Terminal { id, .. } = &active {
            let body = Rect::from_min_max(Pos2::new(inner.left(), HEADER), inner.max);
            self.terminal_surface(ui, body, id);
            return;
        }
        if let crate::app::PanelTab::Files { file } = active {
            let body = Rect::from_min_max(Pos2::new(inner.left(), HEADER), inner.max);
            self.files_surface(ui, body, file);
            return;
        }

        let Some(thread) = self.current_thread() else {
            return;
        };
        let turns = self.diff_turns();
        let latest = turns.first().map(|t| t.0);
        let scope = self
            .diff_scopes
            .get(&thread.id)
            .copied()
            .unwrap_or(DiffScope::WorkingTree);
        // A selected turn that no longer exists falls back to the latest.
        let selected_turn = match scope {
            DiffScope::Turn(count) => turns
                .iter()
                .find(|(c, _)| *c == count)
                .or(turns.first())
                .map(|t| t.0),
            _ => None,
        };
        let turn_scope = matches!(scope, DiffScope::Turn(_));
        let scope_label = match scope {
            DiffScope::WorkingTree => "Working tree".to_owned(),
            DiffScope::Branch => "Branch changes".to_owned(),
            DiffScope::Turn(_) if selected_turn == latest => "Latest turn".to_owned(),
            DiffScope::Turn(_) => format!("Turn {}", selected_turn.unwrap_or_default()),
        };

        // Toolbar.
        let bar = Rect::from_min_size(
            Pos2::new(inner.left(), HEADER),
            Vec2::new(inner.width(), TOOLBAR),
        );
        // Inline panels keep the toolbar's bottom border transparent.
        let label_width = widgets::text_width(ui, &scope_label, font(12.0, Weight::Medium));
        let scope_button = Rect::from_min_size(
            Pos2::new(bar.left() + 8.0, bar.top() + 2.0),
            Vec2::new((label_width + 30.0).ceil(), 24.0),
        );
        let response = ui.interact(scope_button, ui.id().with("diff-scope"), Sense::click());
        ui.painter().rect_filled(
            scope_button,
            CornerRadius::same(8),
            if response.hovered() {
                theme::overlay(0.05)
            } else {
                theme::muted()
            },
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(scope_button.min + Vec2::new(8.0, 0.0), scope_button.max),
            &scope_label,
            font(12.0, Weight::Medium),
            theme::foreground(),
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(scope_button.right() - 20.0, scope_button.top()),
                Vec2::new(14.0, 24.0),
            ),
            14.0,
            theme::alpha(theme::foreground(), 0.7),
        );
        // The press that closes an open menu must not reopen it on release.
        if self.diff_scope_menu.is_some()
            && ui.input(|i| i.pointer.primary_pressed())
            && response.contains_pointer()
        {
            self.diff_scope_menu_suppress = true;
        }
        if response.clicked() {
            if std::mem::take(&mut self.diff_scope_menu_suppress) {
                self.diff_scope_menu = None;
            } else {
                self.diff_scope_menu = Some(context_menu::State::dropdown(Pos2::new(
                    scope_button.left(),
                    scope_button.bottom() + 4.0,
                )));
            }
        }
        self.diff_scope_dropdown(ui, &thread.id, scope, selected_turn, latest, &turns);

        // Load the selection: a turn's checkpoint diff or a git preview source.
        let cwd = self.workspace_cwd();
        let key = match (scope, selected_turn) {
            (DiffScope::Turn(_), Some(turn)) => Some(self.diff_key(&thread.id, turn)),
            (DiffScope::Turn(_), None) => None,
            (DiffScope::WorkingTree | DiffScope::Branch, _) => cwd.as_ref().map(|cwd| {
                format!(
                    "preview:{}:{cwd}:{}",
                    if scope == DiffScope::Branch {
                        "branch-range"
                    } else {
                        "working-tree"
                    },
                    self.diff_ignore_whitespace
                )
            }),
        };
        if let Some(key) = &key
            && !self.diffs.contains_key(key)
        {
            self.diffs.insert(key.clone(), DiffLoad::Loading);
            if let Some(turn) = selected_turn.filter(|_| turn_scope) {
                let mut payload = json!({"threadId": thread.id, "fromTurnCount": turn.saturating_sub(1), "toTurnCount": turn});
                if self.diff_ignore_whitespace {
                    payload["ignoreWhitespace"] = json!(true);
                }
                self.send_request(key.clone(), "orchestration.getTurnDiff", payload);
            } else if let Some(cwd) = &cwd {
                self.send_request(
                    key.clone(),
                    "review.getDiffPreview",
                    json!({"cwd": cwd, "ignoreWhitespace": self.diff_ignore_whitespace}),
                );
            }
        }
        let files: Vec<FileDiff> = match key.as_ref().and_then(|k| self.diffs.get(k)) {
            Some(DiffLoad::Loaded(files)) => files.clone(),
            _ => Vec::new(),
        };
        let has_files = !files.is_empty();

        // Branch scope: "head → base" beside the scope button.
        if scope == DiffScope::Branch
            && let Some((head, base)) = key
                .as_ref()
                .and_then(|k| self.diff_branch_refs.get(k))
                .cloned()
        {
            let refs_font = font(12.0, Weight::Regular);
            let mut x = scope_button.right() + 12.0;
            let head_width = widgets::text_width(ui, &head, refs_font.clone()).min(192.0);
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(x, bar.top()),
                    Pos2::new(x + head_width, bar.bottom()),
                ),
                &head,
                refs_font.clone(),
                theme::muted_foreground(),
            );
            x += head_width + 8.0;
            theme::paint_icon(
                ui,
                "arrow-right",
                Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(14.0, bar.height())),
                14.0,
                theme::alpha(theme::muted_foreground(), 0.7),
            );
            x += 22.0;
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, bar.top()), bar.max),
                &base,
                refs_font,
                theme::muted_foreground(),
            );
        }

        // Right-hand controls, drawn right to left: file tree, whitespace,
        // wrap, layout, collapse-all, refresh, stats.
        let mut x = bar.right() - 8.0;
        let mut toggle = |ui: &mut Ui, icon: &str, active: bool, tip: &str, id: &str| -> bool {
            let button = Rect::from_min_size(Pos2::new(x - 28.0, bar.top()), Vec2::splat(28.0));
            x = button.left() - 4.0;
            let response = ui.interact(button, ui.id().with(id), Sense::click());
            if active || response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(10), theme::accent());
            }
            theme::paint_icon(ui, icon, button, 14.0, theme::foreground());
            response.on_hover_text(tip).clicked()
        };
        if has_files {
            let tip = if self.diff_tree_open {
                "Hide file tree"
            } else {
                "Show file tree"
            };
            if toggle(ui, "folder-tree", self.diff_tree_open, tip, "diff-tree") {
                self.diff_tree_open = !self.diff_tree_open;
            }
        }
        let tip = if self.diff_ignore_whitespace {
            "Show whitespace changes"
        } else {
            "Hide whitespace changes"
        };
        if toggle(ui, "pilcrow", self.diff_ignore_whitespace, tip, "diff-ws") {
            self.diff_ignore_whitespace = !self.diff_ignore_whitespace;
        }
        let tip = if self.diff_wrap {
            "Disable line wrapping"
        } else {
            "Enable line wrapping"
        };
        if toggle(ui, "text-wrap", self.diff_wrap, tip, "diff-wrap") {
            self.diff_wrap = !self.diff_wrap;
        }
        let segmented = Rect::from_min_size(Pos2::new(x - 70.0, bar.top()), Vec2::new(70.0, 28.0));
        ui.painter()
            .rect_filled(segmented, CornerRadius::same(10), theme::overlay(0.032));
        let unified =
            Rect::from_min_size(segmented.min + Vec2::new(2.0, 2.0), Vec2::new(32.0, 24.0));
        let split = unified.translate(Vec2::new(34.0, 0.0));
        for (button, icon, tip, is_split) in [
            (unified, "rows-3", "Stacked diff view", false),
            (split, "columns-2", "Split diff view", true),
        ] {
            let active = self.diff_split == is_split;
            let response = ui
                .interact(
                    button,
                    ui.id().with(("diff-style", is_split)),
                    Sense::click(),
                )
                .on_hover_text(tip);
            if active {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(8), theme::overlay(0.0576));
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(8), theme::accent());
            }
            theme::paint_icon(
                ui,
                icon,
                button,
                14.0,
                if active || response.hovered() {
                    theme::foreground()
                } else {
                    theme::muted_foreground()
                },
            );
            if response.clicked() {
                // The toolbar toggle changes the `diffLayout` setting too.
                self.update_setting(
                    crate::ui::settings::Store::Client,
                    "diffLayout",
                    json!(if is_split { "split" } else { "stacked" }),
                );
            }
        }
        x = segmented.left() - 4.0;
        if has_files && let Some(key) = &key {
            // `allDiffFilesCollapsed` picks the icon and the action.
            let all_collapsed = files
                .iter()
                .all(|f| !self.diff_open_files.contains(&file_key(key, &f.path)));
            let all_open = !all_collapsed;
            let collapse = Rect::from_min_size(Pos2::new(x - 28.0, bar.top()), Vec2::splat(28.0));
            if widgets::icon_button(
                ui,
                collapse,
                "diff-collapse",
                if all_open {
                    "chevrons-down-up"
                } else {
                    "chevrons-up-down"
                },
                14.0,
                theme::muted_foreground(),
                if all_open {
                    "Collapse all files"
                } else {
                    "Expand all files"
                },
            )
            .clicked()
            {
                for file in &files {
                    let file_key = file_key(key, &file.path);
                    if all_open {
                        self.diff_open_files.remove(&file_key);
                    } else {
                        self.diff_open_files.insert(file_key);
                    }
                }
            }
            x = collapse.left() - 4.0;
        }
        if !turn_scope {
            let refresh = Rect::from_min_size(Pos2::new(x - 28.0, bar.top()), Vec2::splat(28.0));
            if widgets::icon_button(
                ui,
                refresh,
                "diff-refresh",
                "refresh-cw",
                14.0,
                theme::muted_foreground(),
                "Refresh diff",
            )
            .clicked()
                && let Some(key) = &key
            {
                self.diffs.remove(key);
            }
            x = refresh.left() - 4.0;
        }
        if has_files {
            let additions: u32 = files.iter().map(|f| f.additions).sum();
            let deletions: u32 = files.iter().map(|f| f.deletions).sum();
            let mono = egui::FontId::monospace(11.0);
            let deletions_text = format!("-{deletions}");
            let del_width = widgets::text_width(ui, &deletions_text, mono.clone());
            // `mr-1` on the stat label.
            let del_x = x - 4.0 - del_width;
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(del_x, bar.top()), Pos2::new(x, bar.bottom())),
                &deletions_text,
                mono.clone(),
                STAT_DELETION,
            );
            let additions_text = format!("+{additions}");
            let add_width = widgets::text_width(ui, &additions_text, mono.clone());
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(del_x - 4.0 - add_width, bar.top()),
                    Pos2::new(del_x, bar.bottom()),
                ),
                &additions_text,
                mono,
                STAT_ADDITION,
            );
        }

        // Body: a message, or the files beside an optional file tree.
        let body = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom() + 12.0), inner.max);
        if turn_scope && turns.is_empty() {
            centered_note(ui, body, "No completed turns yet.");
            return;
        }
        let Some(key) = key else {
            centered_note(ui, body, "No patch available for this selection.");
            return;
        };
        match self.diffs.get(&key) {
            Some(DiffLoad::Loaded(_)) if has_files => {}
            Some(DiffLoad::Loaded(_)) => {
                centered_note(ui, body, "No net changes in this selection.");
                return;
            }
            Some(DiffLoad::Failed(message)) => {
                let message = if message.is_empty() {
                    "Failed to load diff".to_owned()
                } else {
                    message.clone()
                };
                widgets::text(
                    ui,
                    Pos2::new(inner.left() + 12.0, bar.bottom() + 12.0),
                    &message,
                    font(11.0, Weight::Regular),
                    STAT_DELETION,
                    16.0,
                );
                centered_note(ui, body, "No patch available for this selection.");
                return;
            }
            _ => {
                let label = match scope {
                    DiffScope::Turn(_) => "Loading checkpoint diff...",
                    DiffScope::WorkingTree => "Loading working tree diff...",
                    DiffScope::Branch => "Loading branch diff...",
                };
                centered_note(ui, body, label);
                return;
            }
        }

        if let Some(path) = self.diff_reveal.take() {
            self.diff_open_files.insert(file_key(&key, &path));
        }

        // The file tree takes `min(16rem, 40%)` (at least 10rem) on the right.
        let tree_width = if self.diff_tree_open {
            (inner.width() * 0.4).clamp(160.0, 256.0)
        } else {
            0.0
        };
        let list = Rect::from_min_max(
            body.min,
            Pos2::new(inner.right() - tree_width, inner.bottom()),
        );
        if self.diff_tree_open {
            let tree = Rect::from_min_max(Pos2::new(list.right(), body.top()), inner.max);
            self.diff_file_tree(ui, tree, &files, &key);
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        let output = egui::ScrollArea::vertical()
            .id_salt(("diff-files", &key))
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.set_width(list.width());
                for file in &files {
                    let open_key = file_key(&key, &file.path);
                    let open = self.diff_open_files.contains(&open_key);
                    if self.diff_file_header(ui, file, open, list.width()) {
                        if open {
                            self.diff_open_files.remove(&open_key);
                        } else {
                            self.diff_open_files.insert(open_key);
                        }
                    }
                    if open {
                        self.diff_file_body(ui, file, list.width(), !turn_scope);
                        ui.add_space(8.0);
                    }
                }
                ui.add_space(12.0);
            });
        // Rows leave the scrollbar's strip clear only while the list scrolls.
        self.diff_list_overflows = output.content_size.y > output.inner_rect.height() + 0.5;
    }

    /// The scope dropdown: Working tree, Branch changes, Latest turn, and a
    /// Turn submenu listing every turn with its completion time.
    fn diff_scope_dropdown(
        &mut self,
        ui: &mut Ui,
        thread_id: &str,
        scope: DiffScope,
        selected_turn: Option<u64>,
        latest: Option<u64>,
        turns: &[(u64, String)],
    ) {
        if self.diff_scope_menu.is_none() {
            return;
        }
        let now = self.now;
        let turn_items: Vec<Item> = turns
            .iter()
            .map(|(count, completed_at)| Item {
                selected: matches!(scope, DiffScope::Turn(_)) && selected_turn == Some(*count),
                detail: Some(widgets::clock_time(completed_at, now)),
                ..Item::new(format!("turn:{count}"), format!("Turn {count}"))
            })
            .collect();
        let items = vec![
            Item {
                selected: scope == DiffScope::WorkingTree,
                ..Item::new("unstaged", "Working tree")
            },
            Item {
                selected: scope == DiffScope::Branch,
                ..Item::new("branch", "Branch changes")
            },
            Item {
                selected: matches!(scope, DiffScope::Turn(_)) && selected_turn == latest,
                ..Item::new("latest", "Latest turn")
            },
            Item {
                children: turn_items,
                ..Item::new("turn", "Turn")
            },
        ];
        let Some(chosen) = context_menu::show(ui, &mut self.diff_scope_menu, &items) else {
            return;
        };
        let next = match chosen.as_str() {
            "unstaged" => Some(DiffScope::WorkingTree),
            "branch" => Some(DiffScope::Branch),
            "latest" => latest.map(DiffScope::Turn),
            other => other
                .strip_prefix("turn:")
                .and_then(|c| c.parse().ok())
                .map(DiffScope::Turn),
        };
        if let Some(next) = next {
            self.diff_scopes.insert(thread_id.to_owned(), next);
        }
    }

    /// `DiffFileTree`: the changed files as a directory tree beside the diff
    /// (`@pierre/trees`, compact density). Folders start open, single-child
    /// folder chains are flattened, and rows keep the diff's order.
    fn diff_file_tree(&mut self, ui: &mut Ui, rect: Rect, files: &[FileDiff], key: &str) {
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, theme::background());
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(1.0, rect.height())),
            0.0,
            theme::overlay(0.036),
        );
        let aside = Rect::from_min_max(Pos2::new(rect.left() + 1.0, rect.top()), rect.max);
        let rows = tree_rows(files, &self.diff_tree_collapsed);
        let directories: Vec<String> = tree_rows(files, &Default::default())
            .into_iter()
            .filter(|r| r.directory)
            .map(|r| r.path)
            .collect();

        // Header: "Files", the file count and expand/collapse-all folders.
        let header = Rect::from_min_size(aside.min, Vec2::new(aside.width(), 28.0));
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(header.left() + 12.0, header.top()), header.max),
            "Files",
            font(12.0, Weight::Medium),
            theme::foreground(),
        );
        let mut count_right = header.right() - 8.0;
        if !directories.is_empty() {
            let all_open = self.diff_tree_collapsed.is_empty();
            let button = Rect::from_min_size(
                Pos2::new(header.right() - 32.0, header.top() + 2.0),
                Vec2::splat(24.0),
            );
            if widgets::icon_button(
                ui,
                button,
                "diff-tree-folders",
                if all_open {
                    "chevrons-down-up"
                } else {
                    "chevrons-up-down"
                },
                14.0,
                theme::muted_foreground(),
                if all_open {
                    "Collapse all folders"
                } else {
                    "Expand all folders"
                },
            )
            .clicked()
            {
                if all_open {
                    self.diff_tree_collapsed = directories.iter().cloned().collect();
                } else {
                    self.diff_tree_collapsed.clear();
                }
            }
            count_right = button.left() - 4.0;
        }
        let count = files.len().to_string();
        let count_width = widgets::text_width(ui, &count, font(12.0, Weight::Regular));
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(count_right - count_width, header.top()),
                Pos2::new(count_right, header.bottom()),
            ),
            &count,
            font(12.0, Weight::Regular),
            theme::muted_foreground(),
        );

        // Rows.
        let item_left = aside.left() + 17.0;
        let item_right = aside.right() - 16.0;
        let mut y = header.bottom() + 12.0;
        // Indent guides (`fg-muted` at 25%) show while the tree is hovered, at 75%.
        let guide = theme::alpha(Color32::from_rgb(0x84, 0x84, 0x8a), 0.25 * 0.75);
        let show_guides = ui.rect_contains_pointer(aside);
        let mut select: Option<String> = None;
        for (index, row) in rows.iter().enumerate() {
            let item = Rect::from_min_max(Pos2::new(item_left, y), Pos2::new(item_right, y + 24.0));
            y += 24.0;
            if item.top() > aside.bottom() {
                break;
            }
            let response =
                ui.interact(item, ui.id().with(("diff-tree-row", index)), Sense::click());
            let selected = !row.directory && self.diff_tree_selected.as_deref() == Some(&row.path);
            if selected {
                painter.rect_filled(
                    item,
                    CornerRadius::same(5),
                    theme::alpha(theme::foreground(), 0.12),
                );
            } else if response.hovered() {
                painter.rect_filled(
                    item,
                    CornerRadius::same(5),
                    theme::alpha(theme::foreground(), 0.07),
                );
            }
            // One indent guide per ancestor level, centered on its icon slot.
            for level in 0..row.depth * usize::from(show_guides) {
                let x = item.left() + 6.0 + level as f32 * 19.0 + 7.5;
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(x, item.top()), Pos2::new(x + 1.0, item.bottom())),
                    0.0,
                    guide,
                );
            }
            let slot = Rect::from_min_size(
                Pos2::new(item.left() + 6.0 + row.depth as f32 * 19.0, item.top()),
                Vec2::new(16.0, 24.0),
            );
            let status_color = row.status.map(status_color);
            if row.directory {
                theme::paint_icon(
                    ui,
                    if row.open {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    },
                    // Pierre's chevron is wider than a 16px Lucide one.
                    slot.translate(Vec2::new(0.0, 1.0)),
                    20.0,
                    Color32::from_rgb(0x84, 0x84, 0x8a),
                );
            } else {
                let extension = row.name.rsplit('.').next().unwrap_or_default();
                let (icon, color) = crate::ui::markdown::language_icon(extension)
                    .unwrap_or(("file-text", Color32::from_rgb(0xad, 0xad, 0xb1)));
                theme::paint_icon(ui, icon, slot, 16.0, color);
            }
            let name_color = if row.directory {
                theme::foreground()
            } else {
                status_color.unwrap_or(theme::foreground())
            };
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    // Folder names sit 5px after the chevron, file names 4px after the icon.
                    Pos2::new(
                        slot.right() + if row.directory { 5.0 } else { 4.0 },
                        item.top(),
                    ),
                    Pos2::new(item.right() - 22.0, item.bottom()),
                ),
                &row.name,
                font(12.0, Weight::Regular),
                name_color,
            );
            // Git lane: a dot for folders with changes, the letter for files.
            let lane = Pos2::new(item.right() - 11.5, item.center().y + 1.0);
            if row.directory {
                // A folder holding changes always shows the modified color.
                painter.circle_filled(lane, 3.0, theme::alpha(TREE_MODIFIED, 0.5));
            } else if let (Some(status), Some(color)) = (row.status, status_color) {
                let letter = match status {
                    FileStatus::Added => "A",
                    FileStatus::Deleted => "D",
                    FileStatus::Renamed => "R",
                    FileStatus::Modified => "M",
                };
                let width = widgets::text_width(ui, letter, font(12.0, Weight::SemiBold));
                widgets::text_in(
                    ui,
                    Rect::from_min_max(Pos2::new(item.right() - 7.0 - width, item.top()), item.max),
                    letter,
                    font(12.0, Weight::SemiBold),
                    color,
                );
            }
            if response.clicked() {
                if row.directory {
                    if !self.diff_tree_collapsed.remove(&row.path) {
                        self.diff_tree_collapsed.insert(row.path.clone());
                    }
                } else {
                    select = Some(row.path.clone());
                }
            }
        }
        // `revealDiffFile`: picking a file selects it and expands it.
        if let Some(path) = select {
            self.diff_open_files.insert(file_key(key, &path));
            self.diff_tree_selected = Some(path);
        }
    }

    /// Maximize / terminal / right panel toggles shared by both headers.
    pub(crate) fn panel_controls(&mut self, ui: &mut Ui, right: f32) {
        let mut x = right - 13.0;
        let buttons = [
            ("panel-right", "Toggle right panel", self.panel_open()),
            ("panel-bottom", "Toggle terminal", false),
            ("maximize-2", "Maximize panel", false),
        ];
        for (icon, tip, active) in buttons {
            if icon == "maximize-2" && !self.panel_open() {
                continue;
            }
            let button = Rect::from_min_size(Pos2::new(x - 28.0, 12.0), Vec2::splat(28.0));
            x -= 32.0;
            if active {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(10), theme::accent());
            }
            if widgets::icon_button(
                ui,
                button,
                ("panel-control", icon),
                icon,
                16.0,
                theme::foreground(),
                tip,
            )
            .clicked()
            {
                match icon {
                    "panel-right" => {
                        if let Some(panel) = self.panel_mut() {
                            panel.open = !panel.open;
                        }
                    }
                    _ => self.status = Some(format!("{tip} is not implemented yet")),
                }
            }
        }
    }

    fn resize_handle(&mut self, ui: &mut Ui, rect: Rect) {
        let handle = Rect::from_min_size(
            Pos2::new(rect.left() - 4.0, rect.top()),
            Vec2::new(8.0, rect.height()),
        );
        let response = ui.interact(handle, ui.id().with("panel-resize"), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if response.dragged() {
            self.panel_width -= response.drag_delta().x;
        }
    }

    /// File row; returns true when clicked (toggle).
    fn diff_file_header(&mut self, ui: &mut Ui, file: &FileDiff, open: bool, width: f32) -> bool {
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, FILE_HEADER), Sense::hover());
        let response = ui.interact(row, ui.id().with(("diff-file", &file.path)), Sense::click());
        if open {
            ui.painter().rect_filled(
                Rect::from_min_size(row.min, Vec2::new(3.0, row.height())),
                0.0,
                theme::overlay(0.25),
            );
        }
        let chevron = Rect::from_min_size(
            Pos2::new(row.left() + 6.0, row.top() + 6.0),
            Vec2::splat(20.0),
        );
        if open {
            ui.painter()
                .rect_filled(chevron, CornerRadius::same(6), theme::accent());
        }
        theme::paint_icon(
            ui,
            if open {
                "chevron-down"
            } else {
                "chevron-right"
            },
            chevron,
            16.0,
            MODIFIED,
        );
        let (status_icon, status_color) = match file.status {
            FileStatus::Added => ("diffs-symbol-added", ADDED),
            FileStatus::Deleted => ("diffs-symbol-deleted", DELETED),
            FileStatus::Renamed => ("diffs-symbol-moved", MODIFIED),
            FileStatus::Modified => ("diffs-symbol-modified", MODIFIED),
        };
        theme::paint_icon(
            ui,
            status_icon,
            Rect::from_min_size(
                Pos2::new(row.left() + 34.0, row.top()),
                Vec2::new(16.0, row.height()),
            ),
            16.0,
            status_color,
        );
        let path_width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(row.left() + 58.0, row.top()),
                Pos2::new(row.right() - 110.0, row.bottom()),
            ),
            &file.path,
            font(13.0, Weight::Regular),
            theme::foreground(),
        );
        let copy = Rect::from_min_size(
            Pos2::new(row.left() + 58.0 + path_width + 8.0, row.top() + 6.0),
            Vec2::splat(20.0),
        );
        if widgets::icon_button(
            ui,
            copy,
            ("copy-path", &file.path),
            "copy",
            12.0,
            theme::muted_foreground(),
            "Copy path",
        )
        .clicked()
        {
            ui.ctx().copy_text(file.path.clone());
            return false;
        }
        // Counts show only when non-zero, additions rightmost.
        let mono = egui::FontId::monospace(12.0);
        let mut right = row.right() - 12.0;
        for (count, sign, color) in [
            (file.additions, "+", STAT_ADDITION),
            (file.deletions, "-", STAT_DELETION),
        ] {
            if count == 0 {
                continue;
            }
            let text = format!("{sign}{count}");
            let width = widgets::text_width(ui, &text, mono.clone());
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(right - width, row.top()), row.max),
                &text,
                mono.clone(),
                color,
            );
            right -= width + 6.0;
        }
        response.clicked()
    }

    /// A file's hunks, unified or side by side (`diffStyle`). Rows mirror
    /// @pierre/diffs: a 4px change bar, the number gutter, then the code.
    /// `partial`: a git preview patch, whose files end with "More unchanged
    /// context may be available" (upstream can load the rest on demand).
    fn diff_file_body(&mut self, ui: &mut Ui, file: &FileDiff, width: f32, partial: bool) {
        if file.binary {
            separator_row(ui, width, width, "Binary file not shown", false);
            return;
        }
        let language = file.path.rsplit('.').next().unwrap_or_default().to_owned();
        // A scrolling list keeps the 6px scrollbar strip clear on the right.
        let content = width - if self.diff_list_overflows { 6.0 } else { 0.0 };
        // Split columns: 2px apart, the left one rounded down.
        let left_width = ((content - 2.0) / 2.0).floor();
        // The number gutter fits the file's widest line number; the offsets
        // below are measured for three digits.
        let widest = file
            .hunks
            .iter()
            .flat_map(|h| h.lines())
            .map(|l| l.old.max(l.new))
            .max()
            .unwrap_or(1);
        let digit = widgets::text_width(ui, "0", egui::FontId::monospace(13.0));
        let shift = (widest.to_string().len() as f32 - 3.0) * digit;
        let mut previous_end: u32 = 1;
        for hunk in &file.hunks {
            let skipped = hunk.new_start.saturating_sub(previous_end);
            if skipped > 0 {
                separator_row(
                    ui,
                    width,
                    content,
                    &format!(
                        "{skipped} unmodified line{}",
                        if skipped == 1 { "" } else { "s" }
                    ),
                    self.diff_split,
                );
            }
            if let Some(last) = hunk.lines().filter(|l| l.kind != LineKind::Deletion).last() {
                previous_end = last.new + 1;
            }
            if !self.diff_split {
                for line in hunk.lines() {
                    let galley = self.code_galley(ui, line, &language, content, shift);
                    let height = galley.rows.len().max(1) as f32 * LINE;
                    let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                    let column =
                        Rect::from_min_max(row.min, Pos2::new(row.left() + content, row.bottom()));
                    paint_diff_cell(ui, column, line, line.number(), galley, shift);
                }
                continue;
            }
            let mut rows: Vec<(Option<&diff::Line>, Option<&diff::Line>)> = Vec::new();
            for block in &hunk.blocks {
                match block {
                    Block::Context(lines) => rows.extend(lines.iter().map(|l| (Some(l), Some(l)))),
                    Block::Change {
                        deletions,
                        additions,
                    } => {
                        for index in 0..deletions.len().max(additions.len()) {
                            rows.push((deletions.get(index), additions.get(index)));
                        }
                    }
                }
            }
            let right_width = content - left_width - 2.0;
            for (old, new) in rows {
                let old_galley = old.map(|l| self.code_galley(ui, l, &language, left_width, shift));
                let new_galley =
                    new.map(|l| self.code_galley(ui, l, &language, right_width, shift));
                let lines = |g: &Option<std::sync::Arc<egui::Galley>>| {
                    g.as_ref().map_or(1, |g| g.rows.len().max(1))
                };
                let height = lines(&old_galley).max(lines(&new_galley)) as f32 * LINE;
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                let left = Rect::from_min_size(row.min, Vec2::new(left_width, height));
                let right = Rect::from_min_size(
                    Pos2::new(left.right() + 2.0, row.top()),
                    Vec2::new(right_width, height),
                );
                match (old, old_galley) {
                    (Some(line), Some(galley)) => {
                        paint_diff_cell(ui, left, line, line.old, galley, shift)
                    }
                    _ => paint_filler(ui, left, 2.0, shift),
                }
                match (new, new_galley) {
                    (Some(line), Some(galley)) => {
                        paint_diff_cell(ui, right, line, line.new, galley, shift)
                    }
                    _ => paint_filler(ui, right, 0.0, shift),
                }
            }
        }
        // Partial patches can't tell how much unchanged code follows.
        if partial && matches!(file.status, FileStatus::Modified | FileStatus::Renamed) {
            separator_row(
                ui,
                width,
                content,
                "More unchanged context may be available",
                self.diff_split,
            );
        }
    }

    /// Highlighted code for one cell, wrapped to the column when wrapping.
    fn code_galley(
        &mut self,
        ui: &Ui,
        line: &diff::Line,
        language: &str,
        column: f32,
        shift: f32,
    ) -> std::sync::Arc<egui::Galley> {
        let wrap = if self.diff_wrap {
            column - CODE_TEXT - shift - 8.0
        } else {
            f32::INFINITY
        };
        self.diff_line_galley(ui, line, language, wrap)
    }

    fn diff_line_galley(
        &mut self,
        ui: &Ui,
        line: &diff::Line,
        language: &str,
        wrap: f32,
    ) -> std::sync::Arc<egui::Galley> {
        let mut job = LayoutJob::default();
        for (text, color) in self.highlighter.spans(&line.text, language).to_vec() {
            job.append(
                &text,
                0.0,
                widgets::format(egui::FontId::monospace(13.0), color, LINE),
            );
        }
        if job.text.is_empty() {
            job.append(
                " ",
                0.0,
                widgets::format(egui::FontId::monospace(13.0), theme::foreground(), LINE),
            );
        }
        job.wrap.max_width = wrap;
        widgets::center_rows(ui.fonts_mut(|f| f.layout_job(job)))
    }
}

fn file_key(diff_key: &str, path: &str) -> String {
    format!("{diff_key}:{path}")
}

fn centered_note(ui: &Ui, rect: Rect, text: &str) {
    let width = widgets::text_width(ui, text, font(12.0, Weight::Regular));
    widgets::text(
        ui,
        Pos2::new(rect.center().x - width / 2.0, rect.center().y - 8.0),
        text,
        font(12.0, Weight::Regular),
        theme::muted_foreground(),
        16.0,
    );
}

/// "N unmodified lines": centered between rules in the unified view,
/// left-aligned without rules in the split view.
/// `content` is the width the rows use (the list minus a scrollbar strip).
fn separator_row(ui: &mut Ui, width: f32, content: f32, text: &str, split: bool) {
    let (allocated, _) = ui.allocate_exact_size(Vec2::new(width, SEPARATOR), Sense::hover());
    let label_width = widgets::text_width(ui, text, font(11.0, Weight::Regular));
    if split {
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(allocated.left() + 16.0, allocated.top()),
                allocated.max,
            ),
            text,
            font(11.0, Weight::Regular),
            theme::muted_foreground(),
        );
        return;
    }
    let row = Rect::from_min_size(allocated.min, Vec2::new(content - 4.0, SEPARATOR));
    let center = row.center();
    let line_y = center.y.round();
    let left = Rect::from_min_max(
        Pos2::new(row.left() + 8.0, line_y),
        Pos2::new(center.x - label_width / 2.0 - 8.0, line_y + 1.0),
    );
    let right = Rect::from_min_max(
        Pos2::new(center.x + label_width / 2.0 + 8.0, line_y),
        Pos2::new(row.right() - 8.0, line_y + 1.0),
    );
    ui.painter().rect_filled(left, 0.0, theme::shade(29));
    ui.painter().rect_filled(right, 0.0, theme::shade(29));
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(center.x - label_width / 2.0, row.top()), row.max),
        text,
        font(11.0, Weight::Regular),
        theme::muted_foreground(),
    );
}

/// Rectangles covering the glyphs of `text[start..end]` in `galley`.
fn glyph_spans(galley: &egui::Galley, text: &str, start: usize, end: usize) -> Vec<Rect> {
    let first = text[..start].chars().count();
    let last = text[..end].chars().count();
    let mut out = Vec::new();
    let mut index = 0usize;
    for placed in &galley.rows {
        let mut span: Option<(f32, f32)> = None;
        for glyph in &placed.row.glyphs {
            if index >= first && index < last {
                let right = glyph.pos.x + glyph.advance_width;
                span = Some(span.map_or((glyph.pos.x, right), |(l, _)| (l, right)));
            }
            index += 1;
        }
        if let Some((l, r)) = span {
            let top = placed.pos.y + 1.0;
            out.push(Rect::from_min_max(
                Pos2::new(placed.pos.x + l, top),
                Pos2::new(placed.pos.x + r, top + LINE - 2.0),
            ));
        }
        if placed.ends_with_newline {
            index += 1;
        }
    }
    out
}

/// Offsets inside a diff column (unified, or each split side).
const BAR: f32 = 4.0;
const GUTTER: f32 = 47.0;
const NUMBER_RIGHT: f32 = 40.0;
const CODE_BG: f32 = 49.0;
const CODE_TEXT: f32 = 57.0;

/// One line in a column: change bar and tints, gutter number, emphasis and
/// highlighted code.
fn paint_diff_cell(
    ui: &Ui,
    column: Rect,
    line: &diff::Line,
    number: u32,
    galley: std::sync::Arc<egui::Galley>,
    shift: f32,
) {
    let painter = ui.painter();
    let (gutter, number_right, code_bg, code_text) = (
        GUTTER + shift,
        NUMBER_RIGHT + shift,
        CODE_BG + shift,
        CODE_TEXT + shift,
    );
    let (tint, bar, number_color, emphasis) = match line.kind {
        LineKind::Deletion => (
            Some(deletion_row()),
            Some(DELETED),
            DELETED,
            deletion_emphasis(),
        ),
        LineKind::Addition => (
            Some(addition_row()),
            Some(ADDED),
            ADDED,
            addition_emphasis(),
        ),
        LineKind::Context => (None, None, context_number(), Color32::TRANSPARENT),
    };
    if let Some(tint) = tint {
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(column.left() + BAR, column.top()),
                Pos2::new(column.left() + gutter, column.bottom()),
            ),
            0.0,
            tint,
        );
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(column.left() + code_bg, column.top()), column.max),
            0.0,
            tint,
        );
    }
    if let Some(bar) = bar {
        if line.kind == LineKind::Deletion {
            // Dashed deletion bar.
            let mut y = column.top();
            while y < column.bottom() {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(column.left(), y),
                        Vec2::new(BAR, 2.0f32.min(column.bottom() - y)),
                    ),
                    0.0,
                    bar,
                );
                y += 4.0;
            }
        } else {
            painter.rect_filled(
                Rect::from_min_size(column.min, Vec2::new(BAR, column.height())),
                0.0,
                bar,
            );
        }
    }
    // Text sits 1px above egui's centered line box.
    let top = column.top() - 1.0;
    let number = number.to_string();
    let number_font = egui::FontId::monospace(13.0);
    let number_width = widgets::text_width(ui, &number, number_font.clone());
    widgets::text(
        ui,
        Pos2::new(column.left() + number_right - number_width, top),
        &number,
        number_font,
        number_color,
        LINE,
    );
    let origin = Pos2::new(column.left() + code_text, top);
    for (start, end) in &line.emphasis {
        for range in glyph_spans(&galley, &line.text, *start, *end) {
            painter.rect_filled(
                range.translate(origin.to_vec2()),
                CornerRadius::same(2),
                emphasis,
            );
        }
    }
    painter
        .with_clip_rect(Rect::from_min_max(
            Pos2::new(origin.x, column.top()),
            column.max,
        ))
        .galley(origin, galley, theme::foreground());
}

/// The empty side of a split row: a tinted gutter and diagonal hatching that
/// runs `gap` past the column into the middle gap.
fn paint_filler(ui: &Ui, column: Rect, gap: f32, shift: f32) {
    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_max(
            column.min,
            Pos2::new(column.left() + GUTTER + shift, column.bottom()),
        ),
        0.0,
        theme::overlay(0.037),
    );
    let hatch = Rect::from_min_max(
        Pos2::new(column.left() + GUTTER + shift, column.top()),
        Pos2::new(column.right() + gap, column.bottom()),
    );
    let clipped = painter.with_clip_rect(hatch.intersect(painter.clip_rect()));
    let stroke = egui::Stroke::new(std::f32::consts::SQRT_2, theme::overlay(0.075));
    // "/" stripes 2px wide every 8px, phased on screen coordinates.
    let height = hatch.height();
    let mut c = ((hatch.left() + hatch.top()) / 8.0).floor() * 8.0 + 4.5;
    while c - hatch.top() - height < hatch.right() + 8.0 {
        clipped.line_segment(
            [
                Pos2::new(c - hatch.top(), hatch.top()),
                Pos2::new(c - hatch.top() - height, hatch.bottom()),
            ],
            stroke,
        );
        c += 8.0;
    }
}

/// `--trees-git-*-color` for a file status.
fn status_color(status: FileStatus) -> Color32 {
    match status {
        FileStatus::Added => ADDED,
        FileStatus::Deleted => DELETED,
        FileStatus::Renamed | FileStatus::Modified => TREE_MODIFIED,
    }
}

const TREE_MODIFIED: Color32 = Color32::from_rgb(0x08, 0xc0, 0xef); // --trees-status-modified

/// One visible row of the diff file tree.
#[derive(Debug, PartialEq)]
struct TreeRow {
    /// File path, or folder path ending in "/".
    path: String,
    name: String,
    depth: usize,
    directory: bool,
    open: bool,
    /// A file's status; folders take any status to show their dot.
    status: Option<FileStatus>,
}

/// Rows of the tree (`compareDiffFileTreeEntries`): siblings follow the diff's
/// order, a folder taking its first file's position; single-child folder
/// chains read as one "a/b" folder and collapsed folders hide their contents.
fn tree_rows(files: &[FileDiff], collapsed: &std::collections::BTreeSet<String>) -> Vec<TreeRow> {
    #[derive(Default)]
    struct Node {
        position: usize,
        folders: std::collections::BTreeMap<String, Node>,
        files: Vec<(usize, usize)>, // (position, file index)
    }
    let mut root = Node::default();
    for (index, file) in files.iter().enumerate() {
        let segments: Vec<&str> = file.path.split('/').collect();
        let mut node = &mut root;
        for segment in &segments[..segments.len() - 1] {
            node = node
                .folders
                .entry((*segment).to_owned())
                .or_insert_with(|| Node {
                    position: index,
                    ..Default::default()
                });
        }
        node.files.push((index, index));
    }
    fn walk(
        node: &Node,
        prefix: &str,
        depth: usize,
        files: &[FileDiff],
        collapsed: &std::collections::BTreeSet<String>,
        rows: &mut Vec<TreeRow>,
    ) {
        enum Entry<'a> {
            Folder(&'a str, &'a Node),
            File(usize),
        }
        let mut entries: Vec<(usize, Entry)> = node
            .folders
            .iter()
            .map(|(name, child)| (child.position, Entry::Folder(name, child)))
            .chain(node.files.iter().map(|(pos, i)| (*pos, Entry::File(*i))))
            .collect();
        entries.sort_by_key(|(position, _)| *position);
        for (_, entry) in entries {
            match entry {
                Entry::Folder(name, child) => {
                    // Flatten a chain of folders that each hold a single folder.
                    let mut name = name.to_owned();
                    let mut child = child;
                    while child.files.is_empty() && child.folders.len() == 1 {
                        let (next_name, next) = child.folders.iter().next().expect("one folder");
                        name = format!("{name}/{next_name}");
                        child = next;
                    }
                    let path = format!("{prefix}{name}/");
                    let open = !collapsed.contains(&path);
                    rows.push(TreeRow {
                        path: path.clone(),
                        name,
                        depth,
                        directory: true,
                        open,
                        status: Some(files[child.position].status),
                    });
                    if open {
                        walk(child, &path, depth + 1, files, collapsed, rows);
                    }
                }
                Entry::File(index) => {
                    let file = &files[index];
                    rows.push(TreeRow {
                        path: file.path.clone(),
                        name: file.path.rsplit('/').next().unwrap_or_default().to_owned(),
                        depth,
                        directory: false,
                        open: false,
                        status: Some(file.status),
                    });
                }
            }
        }
    }
    let mut rows = Vec::new();
    walk(&root, "", 0, files, collapsed, &mut rows);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> FileDiff {
        FileDiff {
            path: path.into(),
            old_path: None,
            status: FileStatus::Modified,
            hunks: Vec::new(),
            additions: 0,
            deletions: 0,
            binary: false,
        }
    }

    #[test]
    fn tree_rows_flatten_chains_and_keep_diff_order() {
        let files = [file("src/ui/a.rs"), file("README.md"), file("src/ui/b.rs")];
        let names = |collapsed: &std::collections::BTreeSet<String>| {
            tree_rows(&files, collapsed)
                .into_iter()
                .map(|r| (r.name, r.depth))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&Default::default()),
            [
                ("src/ui".to_owned(), 0),
                ("a.rs".to_owned(), 1),
                ("b.rs".to_owned(), 1),
                ("README.md".to_owned(), 0),
            ]
        );
        let collapsed = ["src/ui/".to_owned()].into_iter().collect();
        assert_eq!(
            names(&collapsed),
            [("src/ui".to_owned(), 0), ("README.md".to_owned(), 0)]
        );
    }
}
