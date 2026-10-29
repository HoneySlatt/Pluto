//! The worktree setup card (`WorktreeSetupCard.tsx`, ChatView's
//! `subscribeWorktreeSetup`): while a new worktree is made, its stages
//! (fetch, checkout, submodules, setup script, agent) with their times, the
//! script's last lines, Details (branch, base, path, script), Open terminal
//! and Cancel; failed or cancelled, it stays under the send. The working row
//! reads "Setting up worktree…" meanwhile, and a send waits.
//!
//! Not ported: Work locally (resending the draft without the worktree) and
//! the background chip with its popover once the agent took over.

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;

use crate::app::App;
use crate::model::{Detail, Thread};
use crate::theme::{self, Weight, font};
use crate::timeline::{self, Row};
use crate::ui::widgets;
use crate::worktree_setup::{WorktreeSetup, stage_label, visible};

const STAGE: f32 = 28.0;
const TAIL_LINE: f32 = 17.875;
const TAIL: f32 = 4.0 * TAIL_LINE + 12.0 + 2.0;
const DESTRUCTIVE_FOREGROUND: Color32 = Color32::from_rgb(255, 100, 103);

fn tail_shown(setup: &WorktreeSetup, stage: &crate::worktree_setup::Stage) -> bool {
    stage.id == "setup-script"
        && ((stage.status == "running" && !stage.tail.is_empty()) || stage.status == "failed")
        && setup.phase != "cancelled"
}

/// The card's height, for the timeline's estimate.
pub(crate) fn card_height(setup: &WorktreeSetup, embedded: bool, details: bool) -> f32 {
    let running = setup.phase == "running";
    if embedded && !running {
        return STAGE + 8.0;
    }
    let header = if !running && setup.phase != "done" {
        37.0 + 6.0
    } else {
        0.0
    };
    let stages: f32 = setup
        .stages
        .iter()
        .map(|s| {
            STAGE
                + if tail_shown(setup, s) {
                    TAIL + 4.0
                } else {
                    0.0
                }
        })
        .sum();
    let error = if setup.phase == "failed" && setup.error.is_some() {
        4.0 + 16.0
    } else {
        0.0
    };
    let rows = [
        setup.branch.is_some(),
        setup.base_ref.is_some(),
        setup.worktree_path.is_some(),
        setup.setup_script.is_some(),
    ]
    .iter()
    .filter(|v| **v)
    .count() as f32;
    let details = if details {
        4.0 + rows * 18.0 + 6.0
    } else {
        0.0
    };
    header + stages + error + details + 2.0 + 24.0 + 8.0
}

fn elapsed(
    start: Option<&str>,
    end: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    let start = timeline::parse_time(start?)?;
    let end = end.and_then(timeline::parse_time).unwrap_or(now);
    let ms = (end - start).num_milliseconds().max(0);
    Some(timeline::format_duration((ms / 1000) * 1000))
}

impl App {
    /// `subscribeWorktreeSetup` for the open thread when it works in a
    /// worktree or prepares one.
    pub(crate) fn watch_worktree_setup(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        if !self.connected() || self.worktree_streams.contains(&thread.id) {
            return;
        }
        let preparing = thread
            .latest_run
            .as_ref()
            .is_some_and(|r| r.status == "preparing");
        if thread.worktree_path.is_none() && !preparing {
            return;
        }
        self.worktree_streams.insert(thread.id.clone());
        self.send(crate::protocol::Action::Stream {
            key: format!("worktree-setup:{}", thread.id),
            method: "subscribeWorktreeSetup",
            payload: json!({"threadId": thread.id}),
        });
    }

    /// A snapshot (held through a later `null`, the newer sequence wins).
    pub(crate) fn worktree_setup_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        if key == "worktree-cancel" {
            return true;
        }
        let Some(thread) = key.strip_prefix("worktree-setup:") else {
            return false;
        };
        match result {
            Ok(value) if !value.is_null() => {
                if let Ok(snapshot) = serde_json::from_value::<WorktreeSetup>(value.clone())
                    && self
                        .worktree_setups
                        .get(thread)
                        .is_none_or(|held| snapshot.sequence >= held.sequence)
                {
                    self.worktree_setups.insert(thread.to_owned(), snapshot);
                }
            }
            Ok(_) => {}
            Err(_) => {
                self.worktree_streams.remove(thread);
            }
        }
        true
    }

    /// `resolveVisibleWorktreeSetup` for `thread`.
    fn visible_worktree_setup(&self, thread: &Thread, detail: &Detail) -> Option<&WorktreeSetup> {
        let setup = self.worktree_setups.get(&thread.id)?;
        let turn_started = thread
            .latest_run
            .as_ref()
            .is_some_and(|r| r.started_at.is_some());
        let users = detail
            .visible
            .iter()
            .filter(|v| v.item.kind == "user_message")
            .count();
        visible(setup, turn_started, users > 1).then_some(setup)
    }

    /// `isPreparingWorktree`: the run prepares, or the setup runs before
    /// the agent started.
    pub(crate) fn preparing_worktree(&self) -> bool {
        let Some(thread) = self.current_thread() else {
            return false;
        };
        thread
            .latest_run
            .as_ref()
            .is_some_and(|r| r.status == "preparing")
            || self
                .worktree_setups
                .get(&thread.id)
                .is_some_and(|s| s.phase == "running" && !s.agent_started())
    }

    /// The send waits while the setup runs and the agent has not started.
    pub(crate) fn worktree_send_block(&self) -> Option<&'static str> {
        let thread = self.current_thread()?;
        self.worktree_setups
            .get(&thread.id)
            .filter(|s| s.phase == "running" && !s.agent_started())
            .map(|_| "Preparing worktree")
    }

    /// The rows with the setup card in its place.
    pub(crate) fn with_worktree_setup(
        &self,
        rows: Arc<Vec<Row>>,
        thread: &Thread,
        detail: &Detail,
    ) -> Arc<Vec<Row>> {
        let Some(setup) = self.visible_worktree_setup(thread, detail) else {
            return rows;
        };
        let mut rows = (*rows).clone();
        let run_started = thread
            .latest_run
            .as_ref()
            .is_some_and(|r| r.started_at.is_some());
        crate::worktree_setup::insert_row(&mut rows, setup, run_started);
        Arc::new(rows)
    }

    /// Opens `thread`'s drawer on the terminal `id` (`storeEnsureTerminal`).
    fn open_terminal_by_id(&mut self, thread: &str, id: &str) {
        self.open_terminal_drawer(thread);
        if let Some(drawer) = self.drawers.get_mut(thread) {
            if !drawer.groups.iter().any(|g| g.ids.iter().any(|t| t == id)) {
                drawer.groups.push(crate::ui::terminal_drawer::Group {
                    ids: vec![id.to_owned()],
                    vertical: false,
                });
            }
            drawer.active = id.to_owned();
        }
    }

    /// The card.
    pub(crate) fn worktree_setup_row(
        &mut self,
        ui: &mut Ui,
        setup: &WorktreeSetup,
        embedded: bool,
        width: f32,
    ) {
        let running = setup.phase == "running";
        let failed_stage = setup.any_failed();
        let header = match setup.phase.as_str() {
            "running" => "Setting up worktree…".to_owned(),
            "done" if failed_stage => "Worktree ready, setup script failed".into(),
            "done" => "Worktree ready".into(),
            "failed" => "Worktree setup failed".into(),
            _ => "Worktree setup cancelled".into(),
        };
        let total = elapsed(Some(&setup.started_at), setup.ended_at.as_deref(), self.now);
        if running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
        let muted = theme::muted_foreground();
        let small = font(12.0, Weight::Regular);
        // Collapsed once the agent took over: one row.
        if embedded && !running {
            let failed = matches!(setup.phase.as_str(), "failed" | "cancelled")
                || setup
                    .stages
                    .iter()
                    .any(|s| s.id == "setup-script" && s.status == "failed");
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, STAGE), Sense::hover());
            self.stage_line(
                ui,
                row,
                if failed { "x" } else { "check" },
                &header,
                theme::secondary_label(),
                total.as_deref(),
                false,
            );
            ui.add_space(8.0);
            return;
        }
        if !running && setup.phase != "done" {
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 37.0), Sense::hover());
            ui.painter().rect_filled(
                Rect::from_min_max(Pos2::new(row.left(), row.bottom() - 1.0), row.max),
                0.0,
                theme::alpha(theme::border(), 0.6),
            );
            let tone = if setup.phase == "failed" {
                DESTRUCTIVE_FOREGROUND
            } else {
                muted
            };
            let line =
                Rect::from_min_size(row.min + Vec2::new(4.0, 4.0), Vec2::new(width - 8.0, 24.0));
            let mut right = line.right();
            if let Some(total) = &total {
                let w = widgets::tabular_width(ui, total, small.clone());
                widgets::tabular_text(
                    ui,
                    Pos2::new(right - w, line.center().y - 8.0),
                    total,
                    small.clone(),
                    muted,
                    16.0,
                );
                right -= w + 8.0;
            }
            widgets::text_in(
                ui,
                Rect::from_min_max(line.min, Pos2::new(right, line.bottom())),
                &header,
                font(14.0, Weight::Regular),
                tone,
            );
            ui.add_space(6.0);
        }
        for stage in &setup.stages {
            let label = match (&setup.setup_script, stage.id.as_str()) {
                (Some(script), "setup-script") if !script.name.trim().is_empty() => {
                    script.name.clone()
                }
                _ => stage_label(&stage.id).to_owned(),
            };
            let (icon, tone) = match stage.status.as_str() {
                "done" => ("check", theme::secondary_label()),
                "running" => ("spinner", theme::secondary_label()),
                "failed" => ("x", DESTRUCTIVE_FOREGROUND),
                "warning" => ("circle-alert", theme::warning_foreground()),
                "skipped" => ("minus", theme::secondary_label()),
                _ => ("circle", theme::alpha(theme::secondary_label(), 0.4)),
            };
            let mut trailing: Vec<String> = Vec::new();
            match stage.status.as_str() {
                "pending" => {}
                "skipped" => {
                    trailing.push(stage.detail.clone().unwrap_or_else(|| "skipped".into()))
                }
                _ => {
                    if stage.id == "checkout"
                        && stage.status == "running"
                        && let Some(percent) = stage.percent
                    {
                        trailing.push(format!("{}%", percent.round()));
                    } else if let Some(detail) = stage.detail.clone().filter(|d| !d.is_empty()) {
                        trailing.push(detail);
                    }
                    if let Some(time) = elapsed(
                        stage.started_at.as_deref(),
                        stage.ended_at.as_deref(),
                        self.now,
                    ) {
                        trailing.push(time);
                    }
                }
            }
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, STAGE), Sense::hover());
            let trailing = (!trailing.is_empty()).then(|| trailing.join(" · "));
            self.stage_line(
                ui,
                row,
                icon,
                &label,
                tone,
                trailing.as_deref(),
                stage.status == "running",
            );
            if tail_shown(setup, stage) {
                let failed = stage.status == "failed";
                let (box_rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, TAIL + 4.0), Sense::hover());
                let frame = Rect::from_min_size(
                    Pos2::new(box_rect.left() + 32.0, box_rect.top()),
                    Vec2::new(width - 32.0, TAIL),
                );
                let (fill, stroke, text) = if failed {
                    (
                        theme::alpha(theme::error(), 0.08),
                        theme::alpha(theme::error(), 0.2),
                        DESTRUCTIVE_FOREGROUND,
                    )
                } else {
                    (theme::overlay(0.012), theme::border(), muted)
                };
                ui.painter().rect(
                    frame,
                    CornerRadius::same(8),
                    fill,
                    Stroke::new(1.0, stroke),
                    StrokeKind::Inside,
                );
                // The last four lines, newest last, never wrapped.
                let lines: Vec<&String> = stage.tail.iter().rev().take(4).rev().collect();
                for (index, line) in lines.iter().enumerate() {
                    widgets::text_in(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(
                                frame.left() + 10.0,
                                frame.top() + 7.0 + index as f32 * TAIL_LINE,
                            ),
                            Vec2::new(frame.width() - 20.0, TAIL_LINE),
                        ),
                        line,
                        theme::mono(11.0),
                        text,
                    );
                }
            }
        }
        if setup.phase == "failed"
            && let Some(error) = setup.error.as_deref().filter(|e| !e.is_empty())
        {
            ui.add_space(4.0);
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(row.left() + 32.0, row.top()), row.max),
                error,
                small.clone(),
                muted,
            );
        }
        let details_key = egui::Id::new(("worktree-setup-details", &setup.thread_id));
        let details_open = ui
            .data(|d| d.get_temp::<bool>(details_key))
            .unwrap_or(false);
        if details_open {
            ui.add_space(4.0);
            let terms: Vec<(&str, String)> = [
                ("Branch", setup.branch.clone()),
                ("Base", setup.base_ref.clone()),
                ("Path", setup.worktree_path.clone()),
                (
                    "Setup",
                    setup.setup_script.as_ref().map(|s| s.command.clone()),
                ),
            ]
            .into_iter()
            .filter_map(|(term, value)| value.map(|v| (term, v)))
            .collect();
            let term_width = terms
                .iter()
                .map(|(t, _)| widgets::text_width(ui, t, small.clone()))
                .fold(0.0, f32::max);
            for (term, value) in &terms {
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
                let x = row.left() + 32.0;
                widgets::text_in(
                    ui,
                    Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(term_width, 16.0)),
                    term,
                    small.clone(),
                    theme::alpha(theme::foreground(), 0.8),
                );
                widgets::text_in(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(x + term_width + 12.0, row.top()),
                        Pos2::new(row.right(), row.top() + 16.0),
                    ),
                    value,
                    theme::mono(12.0),
                    muted,
                );
            }
            ui.add_space(6.0);
        }
        // Details, Open terminal, Cancel (`xs ghost-muted`).
        ui.add_space(2.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
        let mut x = bar.left() + 19.0;
        let mut button = |ui: &mut Ui, id: &str, icon: &str, label: &str| {
            let w = 7.0 + 14.0 + 4.0 + widgets::text_width(ui, label, small.clone()) + 7.0;
            let rect = Rect::from_min_size(Pos2::new(x, bar.top()), Vec2::new(w, 24.0));
            x += w + 2.0;
            let response = ui.interact(rect, ui.id().with(("worktree-setup", id)), Sense::click());
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            let hovered = response.hovered();
            if hovered {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), theme::accent());
            }
            let color = if hovered { theme::foreground() } else { muted };
            theme::paint_icon(
                ui,
                icon,
                Rect::from_center_size(
                    Pos2::new(rect.left() + 7.0 + 7.0, rect.center().y),
                    Vec2::splat(14.0),
                ),
                14.0,
                color,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(rect.left() + 25.0, rect.top()), rect.max),
                label,
                small.clone(),
                color,
            );
            response.clicked()
        };
        if button(
            ui,
            "details",
            if details_open {
                "chevron-down"
            } else {
                "chevron-right"
            },
            "Details",
        ) {
            ui.data_mut(|d| d.insert_temp(details_key, !details_open));
        }
        let terminal: Option<String> = setup
            .setup_script
            .as_ref()
            .and_then(|s| s.terminal_id.clone());
        let script_started = setup
            .stages
            .iter()
            .any(|s| s.id == "setup-script" && s.status != "pending");
        if let Some(terminal) = terminal.filter(|_| script_started)
            && self.selected.as_deref() == Some(setup.thread_id.as_str())
            && button(ui, "terminal", "terminal", "Open terminal")
        {
            let thread = setup.thread_id.clone();
            self.open_terminal_by_id(&thread, &terminal);
        }
        if running && !embedded && button(ui, "cancel", "x", "Cancel") {
            self.send_request(
                "worktree-cancel".into(),
                "worktreeSetup.cancel",
                json!({"threadId": setup.thread_id}),
            );
        }
        ui.add_space(8.0);
    }

    /// A `WorkLogRow`: the icon column, the label, and trailing text.
    #[allow(clippy::too_many_arguments)]
    fn stage_line(
        &self,
        ui: &Ui,
        row: Rect,
        icon: &str,
        label: &str,
        tone: Color32,
        trailing: Option<&str>,
        shimmer: bool,
    ) {
        let line = row.shrink(2.0);
        let icon_rect = Rect::from_min_size(line.min, Vec2::new(24.0, line.height()));
        if icon == "spinner" {
            crate::ui::content_search::paint_spinner(ui, icon_rect.center(), 16.0);
        } else {
            theme::paint_icon(
                ui,
                icon,
                icon_rect,
                16.0,
                theme::alpha(theme::muted_foreground(), 0.8),
            );
        }
        let mut right = line.right();
        let small = font(12.0, Weight::Regular);
        if let Some(text) = trailing {
            let w = widgets::tabular_width(ui, text, small.clone()).min(line.width() * 0.5);
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(right - w, line.top()),
                    Vec2::new(w, line.height()),
                ),
                text,
                small,
                theme::muted_foreground(),
            );
            right -= w + 8.0;
        }
        let text_rect = Rect::from_min_max(
            Pos2::new(icon_rect.right() + 6.0, line.top()),
            Pos2::new(right, line.bottom()),
        );
        crate::ui::live_activity::sweep_label(
            ui,
            text_rect,
            label,
            font(14.0, Weight::Regular),
            20.0,
            tone,
            theme::foreground(),
            shimmer,
        );
    }
}
