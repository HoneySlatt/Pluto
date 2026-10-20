//! Right panel with the turn diff (`RightPanelTabs.tsx`, `DiffPanel.tsx`,
//! rendered like `@pierre/diffs` with the app's diff surface colors).
//! Unified view only; the split view and the working-tree/branch scopes are
//! not ported yet.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2, text::LayoutJob};
use serde_json::{Value, json};

use crate::app::App;
use crate::diff::{self, Block, FileDiff, FileStatus, LineKind};
use crate::theme::{self, Weight, font};
use crate::ui::widgets::{self, MenuEntry, MenuItem};

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
}

const HEADER: f32 = 52.0;
const TOOLBAR: f32 = 28.0;
const FILE_HEADER: f32 = 32.0;
const LINE: f32 = 20.0;
const SEPARATOR: f32 = 24.0;
const MODIFIED: Color32 = Color32::from_rgb(0, 159, 255); // --diffs-modified-base
const ADDED: Color32 = Color32::from_rgb(7, 196, 128);
const DELETED: Color32 = Color32::from_rgb(255, 45, 62);
const DELETION_ROW: Color32 = Color32::from_rgb(28, 16, 15);
const ADDITION_ROW: Color32 = Color32::from_rgb(12, 23, 19);
const DELETION_EMPHASIS: Color32 = Color32::from_rgb(73, 22, 25);
const ADDITION_EMPHASIS: Color32 = Color32::from_rgb(9, 56, 40);
const CONTEXT_NUMBER: Color32 = Color32::from_rgb(122, 122, 122);
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
        ui.painter().rect_filled(rect, 0.0, theme::BACKGROUND);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(1.0, rect.height())),
            0.0,
            theme::BORDER,
        );
        self.resize_handle(ui, rect);
        let inner = Rect::from_min_max(Pos2::new(rect.left() + 1.0, rect.top()), rect.max);

        // No surface open: the launcher fills the panel.
        if !self.panel().diff {
            self.panel_controls(ui, rect.right());
            let body = Rect::from_min_max(Pos2::new(inner.left(), HEADER), inner.max);
            self.surface_launcher(ui, body, HEADER);
            return;
        }
        self.launcher_focused = false;

        // Tab bar: the Diff tab (its icon turns into a close button while the
        // tab is hovered) and the "+" surface menu.
        let tab = Rect::from_min_size(Pos2::new(inner.left() + 8.0, 14.0), Vec2::new(51.0, 24.0));
        ui.painter()
            .rect_filled(tab, CornerRadius::same(8), theme::ACCENT);
        let tab_hovered = ui.rect_contains_pointer(tab);
        let close = Rect::from_min_size(
            Pos2::new(tab.left() + 6.0, tab.center().y - 8.0),
            Vec2::splat(16.0),
        );
        let close_response = ui
            .interact(close, ui.id().with("close-diff-tab"), Sense::click())
            .on_hover_text("Close Diff");
        if close_response.hovered() {
            ui.painter()
                .rect_filled(close, CornerRadius::same(6), theme::MUTED);
        }
        theme::paint_icon(
            ui,
            if tab_hovered { "x" } else { "file-diff" },
            close,
            12.0,
            theme::FOREGROUND,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(tab.min + Vec2::new(24.0, 0.0), tab.max),
            "Diff",
            font(12.0, Weight::Regular),
            theme::FOREGROUND,
        );
        if close_response.clicked()
            && let Some(panel) = self.panel_mut()
        {
            panel.diff = false;
        }
        let add = Rect::from_min_size(Pos2::new(tab.right() + 4.0, 14.0), Vec2::splat(24.0));
        self.add_surface_button(ui, add);
        self.panel_controls(ui, rect.right());

        let Some(thread) = self.current_thread() else {
            return;
        };
        let turns = self.diff_turns();
        let latest = turns.first().map(|t| t.0);
        let turn = self
            .diff_turn
            .filter(|t| turns.iter().any(|(c, _)| c == t))
            .or(latest);

        // Toolbar.
        let bar = Rect::from_min_size(
            Pos2::new(inner.left(), HEADER),
            Vec2::new(inner.width(), TOOLBAR),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(bar.left(), bar.bottom() - 1.0),
                Vec2::new(bar.width(), 1.0),
            ),
            0.0,
            theme::white(0.036),
        );
        let scope_label = match turn {
            Some(t) if Some(t) == latest => "Latest turn".to_owned(),
            Some(t) => format!("Turn {t}"),
            None => "Latest turn".to_owned(),
        };
        let label_width = widgets::text_width(ui, &scope_label, font(12.0, Weight::Medium));
        let scope = Rect::from_min_size(
            Pos2::new(bar.left() + 8.0, bar.top() + 2.0),
            Vec2::new((label_width + 34.0).ceil(), 24.0),
        );
        let response = ui.interact(scope, ui.id().with("diff-scope"), Sense::click());
        ui.painter().rect_filled(
            scope,
            CornerRadius::same(8),
            if response.hovered() {
                theme::white(0.05)
            } else {
                theme::MUTED
            },
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(scope.min + Vec2::new(8.0, 0.0), scope.max),
            &scope_label,
            font(12.0, Weight::Medium),
            theme::FOREGROUND,
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(scope.right() - 20.0, scope.top()),
                Vec2::new(14.0, 24.0),
            ),
            14.0,
            theme::alpha(theme::FOREGROUND, 0.7),
        );
        if response.clicked() {
            self.open_menu = Some("diff-scope".into());
        }
        let labels: Vec<String> = turns
            .iter()
            .enumerate()
            .map(|(i, (count, _))| {
                if i == 0 {
                    "Latest turn".to_owned()
                } else {
                    format!("Turn {count}")
                }
            })
            .collect();
        let entries: Vec<MenuEntry> = labels
            .iter()
            .zip(&turns)
            .map(|(label, (count, _))| {
                MenuEntry::Item(MenuItem {
                    selected: Some(*count) == turn,
                    ..MenuItem::new(label)
                })
            })
            .collect();
        if let Some(index) = widgets::menu(
            ui,
            &mut self.open_menu,
            "diff-scope",
            scope,
            false,
            &entries,
        ) {
            self.diff_turn = Some(turns[index].0);
        }

        // Right-hand toggles.
        let mut x = bar.right() - 8.0;
        let mut toggle = |ui: &mut Ui, icon: &str, active: bool, tip: &str, id: &str| -> bool {
            let button = Rect::from_min_size(Pos2::new(x - 28.0, bar.top()), Vec2::splat(28.0));
            x = button.left() - 4.0;
            let response = ui.interact(button, ui.id().with(id), Sense::click());
            if active || response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(10), theme::ACCENT);
            }
            theme::paint_icon(ui, icon, button, 14.0, theme::FOREGROUND);
            response.on_hover_text(tip).clicked()
        };
        if toggle(ui, "folder-tree", false, "Show file tree", "diff-tree") {
            self.status = Some("The diff file tree is not implemented yet".into());
        }
        if toggle(
            ui,
            "pilcrow",
            self.diff_ignore_whitespace,
            "Ignore whitespace",
            "diff-ws",
        ) {
            self.diff_ignore_whitespace = !self.diff_ignore_whitespace;
        }
        if toggle(ui, "text-wrap", self.diff_wrap, "Wrap lines", "diff-wrap") {
            self.diff_wrap = !self.diff_wrap;
        }
        let segmented = Rect::from_min_size(Pos2::new(x - 70.0, bar.top()), Vec2::new(70.0, 28.0));
        ui.painter()
            .rect_filled(segmented, CornerRadius::same(10), theme::white(0.032));
        let unified =
            Rect::from_min_size(segmented.min + Vec2::new(2.0, 2.0), Vec2::new(32.0, 24.0));
        let split = unified.translate(Vec2::new(34.0, 0.0));
        for (button, icon, tip, is_split) in [
            (unified, "rows-3", "Unified view", false),
            (split, "columns-2", "Split view", true),
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
                    .rect_filled(button, CornerRadius::same(8), theme::white(0.0576));
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(8), theme::ACCENT);
            }
            theme::paint_icon(
                ui,
                icon,
                button,
                14.0,
                if active || response.hovered() {
                    theme::FOREGROUND
                } else {
                    theme::MUTED_FOREGROUND
                },
            );
            if response.clicked() {
                self.diff_split = is_split;
            }
        }
        x = segmented.left() - 4.0;

        let Some(turn) = turn else {
            centered_note(ui, rect, "No changes in this thread yet.");
            return;
        };
        let key = self.diff_key(&thread.id, turn);
        if !self.diffs.contains_key(&key) {
            self.diffs.insert(key.clone(), DiffLoad::Loading);
            let mut payload = json!({"threadId": thread.id, "fromTurnCount": turn.saturating_sub(1), "toTurnCount": turn});
            if self.diff_ignore_whitespace {
                payload["ignoreWhitespace"] = json!(true);
            }
            self.send_request(key.clone(), "orchestration.getTurnDiff", payload);
        }
        let files = match self.diffs.get(&key) {
            Some(DiffLoad::Loaded(files)) => files.clone(),
            Some(DiffLoad::Failed(message)) => {
                let message = if message.is_empty() {
                    "Failed to load turn diff".to_owned()
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
                centered_note(ui, rect, "No patch available for this selection.");
                return;
            }
            _ => {
                centered_note(ui, rect, "Loading diff…");
                return;
            }
        };
        if files.is_empty() {
            centered_note(ui, rect, "No patch available for this selection.");
            return;
        }

        // Stats and collapse-all, left of the toggles.
        let additions: u32 = files.iter().map(|f| f.additions).sum();
        let deletions: u32 = files.iter().map(|f| f.deletions).sum();
        let all_open = files
            .iter()
            .all(|f| self.diff_open_files.contains(&file_key(&key, &f.path)));
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
            theme::MUTED_FOREGROUND,
            if all_open {
                "Collapse all files"
            } else {
                "Expand all files"
            },
        )
        .clicked()
        {
            for file in &files {
                let file_key = file_key(&key, &file.path);
                if all_open {
                    self.diff_open_files.remove(&file_key);
                } else {
                    self.diff_open_files.insert(file_key);
                }
            }
        }
        let mono = egui::FontId::monospace(11.0);
        let deletions_text = format!("-{deletions}");
        let del_width = widgets::text_width(ui, &deletions_text, mono.clone());
        let del_x = collapse.left() - 8.0 - del_width;
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(del_x, bar.top()),
                Pos2::new(collapse.left(), bar.bottom()),
            ),
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

        if let Some(path) = self.diff_reveal.take() {
            self.diff_open_files.insert(file_key(&key, &path));
        }

        // Files.
        let list = Rect::from_min_max(Pos2::new(inner.left(), bar.bottom() + 12.0), inner.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        egui::ScrollArea::vertical()
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
                        self.diff_file_body(ui, file, list.width());
                    }
                }
                ui.add_space(12.0);
            });
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
                    .rect_filled(button, CornerRadius::same(10), theme::ACCENT);
            }
            if widgets::icon_button(
                ui,
                button,
                ("panel-control", icon),
                icon,
                16.0,
                theme::FOREGROUND,
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
                theme::white(0.25),
            );
        }
        let chevron = Rect::from_min_size(
            Pos2::new(row.left() + 6.0, row.top() + 6.0),
            Vec2::splat(20.0),
        );
        if open {
            ui.painter()
                .rect_filled(chevron, CornerRadius::same(6), theme::ACCENT);
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
            theme::FOREGROUND,
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
            theme::MUTED_FOREGROUND,
            "Copy path",
        )
        .clicked()
        {
            ui.ctx().copy_text(file.path.clone());
            return false;
        }
        let mono = egui::FontId::monospace(12.0);
        let additions = format!("+{}", file.additions);
        let add_width = widgets::text_width(ui, &additions, mono.clone());
        let add_x = row.right() - 12.0 - add_width;
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(add_x, row.top()), row.max),
            &additions,
            mono.clone(),
            STAT_ADDITION,
        );
        let deletions = format!("-{}", file.deletions);
        let del_width = widgets::text_width(ui, &deletions, mono.clone());
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(add_x - 6.0 - del_width, row.top()), row.max),
            &deletions,
            mono,
            STAT_DELETION,
        );
        response.clicked()
    }

    /// A file's hunks, unified or side by side (`diffStyle`). Rows mirror
    /// @pierre/diffs: a 4px change bar, the number gutter, then the code.
    fn diff_file_body(&mut self, ui: &mut Ui, file: &FileDiff, width: f32) {
        if file.binary {
            separator_row(ui, width, "Binary file not shown", false);
            return;
        }
        let language = file.path.rsplit('.').next().unwrap_or_default().to_owned();
        // The overlay scrollbar keeps a 6px strip clear on the right.
        let content = width - 6.0;
        // Split columns: 2px apart, the left one rounded down.
        let left_width = ((content - 2.0) / 2.0).floor();
        let mut previous_end: u32 = 1;
        for hunk in &file.hunks {
            let skipped = hunk.new_start.saturating_sub(previous_end);
            if skipped > 0 {
                separator_row(
                    ui,
                    width,
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
                    let galley = self.code_galley(ui, line, &language, content);
                    let height = galley.rows.len().max(1) as f32 * LINE;
                    let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
                    let column =
                        Rect::from_min_max(row.min, Pos2::new(row.left() + content, row.bottom()));
                    paint_diff_cell(ui, column, line, line.number(), galley);
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
                let old_galley = old.map(|l| self.code_galley(ui, l, &language, left_width));
                let new_galley = new.map(|l| self.code_galley(ui, l, &language, right_width));
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
                    (Some(line), Some(galley)) => paint_diff_cell(ui, left, line, line.old, galley),
                    _ => paint_filler(ui, left, 2.0),
                }
                match (new, new_galley) {
                    (Some(line), Some(galley)) => {
                        paint_diff_cell(ui, right, line, line.new, galley)
                    }
                    _ => paint_filler(ui, right, 0.0),
                }
            }
        }
    }

    /// Highlighted code for one cell, wrapped to the column when wrapping.
    fn code_galley(
        &mut self,
        ui: &Ui,
        line: &diff::Line,
        language: &str,
        column: f32,
    ) -> std::sync::Arc<egui::Galley> {
        let wrap = if self.diff_wrap {
            column - CODE_TEXT - 8.0
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
                widgets::format(egui::FontId::monospace(13.0), theme::FOREGROUND, LINE),
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
        theme::MUTED_FOREGROUND,
        16.0,
    );
}

/// "N unmodified lines": centered between rules in the unified view,
/// left-aligned without rules in the split view.
fn separator_row(ui: &mut Ui, width: f32, text: &str, split: bool) {
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
            theme::MUTED_FOREGROUND,
        );
        return;
    }
    // The rules stop short of the scrollbar strip.
    let row = Rect::from_min_size(allocated.min, Vec2::new(width - 10.0, SEPARATOR));
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
    ui.painter()
        .rect_filled(left, 0.0, Color32::from_rgb(29, 29, 29));
    ui.painter()
        .rect_filled(right, 0.0, Color32::from_rgb(29, 29, 29));
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(center.x - label_width / 2.0, row.top()), row.max),
        text,
        font(11.0, Weight::Regular),
        theme::MUTED_FOREGROUND,
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
) {
    let painter = ui.painter();
    let (tint, bar, number_color, emphasis) = match line.kind {
        LineKind::Deletion => (
            Some(DELETION_ROW),
            Some(DELETED),
            DELETED,
            DELETION_EMPHASIS,
        ),
        LineKind::Addition => (Some(ADDITION_ROW), Some(ADDED), ADDED, ADDITION_EMPHASIS),
        LineKind::Context => (None, None, CONTEXT_NUMBER, Color32::TRANSPARENT),
    };
    if let Some(tint) = tint {
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(column.left() + BAR, column.top()),
                Pos2::new(column.left() + GUTTER, column.bottom()),
            ),
            0.0,
            tint,
        );
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(column.left() + CODE_BG, column.top()), column.max),
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
        Pos2::new(column.left() + NUMBER_RIGHT - number_width, top),
        &number,
        number_font,
        number_color,
        LINE,
    );
    let origin = Pos2::new(column.left() + CODE_TEXT, top);
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
        .galley(origin, galley, theme::FOREGROUND);
}

/// The empty side of a split row: a tinted gutter and diagonal hatching that
/// runs `gap` past the column into the middle gap.
fn paint_filler(ui: &Ui, column: Rect, gap: f32) {
    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_max(
            column.min,
            Pos2::new(column.left() + GUTTER, column.bottom()),
        ),
        0.0,
        theme::white(0.037),
    );
    let hatch = Rect::from_min_max(
        Pos2::new(column.left() + GUTTER, column.top()),
        Pos2::new(column.right() + gap, column.bottom()),
    );
    let clipped = painter.with_clip_rect(hatch.intersect(painter.clip_rect()));
    let stroke = egui::Stroke::new(std::f32::consts::SQRT_2, theme::white(0.075));
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
