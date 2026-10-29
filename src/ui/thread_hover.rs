//! A sidebar row's hover card (`SidebarThreadTooltip`, `ThreadHoverCard`):
//! after 150 ms over a row, a glass card to its right with the title, then
//! the project, the branch (and, for the open thread, a checkout on another
//! branch), the model, the providers it was handed off from, its running
//! terminals, an error, and its pull requests.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `TooltipProvider delay={150}`.
const DELAY: f64 = 0.15;
const WIDTH: f32 = 320.0;

enum Line {
    Icon(&'static str, Color32, String, Color32),
    Project(Box<crate::model::Project>, String),
    Pr(&'static str, Color32, String, String),
}

impl App {
    /// Shows `thread`'s card beside `row` while the pointer rests on it.
    pub(crate) fn thread_hover_card(
        &mut self,
        ui: &mut Ui,
        row: Rect,
        hovered: bool,
        thread: &Thread,
    ) {
        let id = egui::Id::new(("thread-hover", &thread.id));
        let now = ui.input(|i| i.time);
        if !hovered || ui.ctx().dragged_id().is_some() || self.open_menu.is_some() {
            ui.data_mut(|d| d.remove::<f64>(id));
            return;
        }
        let since = ui.data_mut(|d| *d.get_temp_mut_or(id, now));
        if now - since < DELAY {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(DELAY - (now - since)));
            return;
        }
        let muted = theme::muted_foreground();
        let mut lines = Vec::new();
        if let Some(project) = self.state.project(&thread.project_id).cloned() {
            let name = project.title.clone();
            lines.push(Line::Project(Box::new(project), name));
        }
        if let Some(branch) = &thread.branch {
            lines.push(Line::Icon("git-branch", muted, branch.clone(), muted));
            let open = self.selected.as_deref() == Some(thread.id.as_str());
            if open
                && thread.worktree_path.is_none()
                && self
                    .workspace_branch()
                    .is_some_and(|current| &current != branch)
            {
                lines.push(Line::Icon(
                    "circle-alert",
                    theme::warning(),
                    "You're currently checked out on another branch.".into(),
                    theme::warning(),
                ));
            }
        }
        if !thread.model_selection.instance_id.is_empty() {
            let (model, _) = self.model_label(&thread.model_selection);
            let icon = self
                .providers()
                .iter()
                .find(|p| p["instanceId"] == thread.model_selection.instance_id.as_str())
                .and_then(|p| {
                    crate::ui::pickers::provider_icon(p["driver"].as_str().unwrap_or_default())
                })
                .map_or("bot", |(icon, _)| icon);
            lines.push(Line::Icon(icon, theme::alpha(muted, 0.6), model, muted));
        }
        let handed: Vec<String> = thread
            .provider_instance_history
            .iter()
            .filter(|id| **id != thread.model_selection.instance_id)
            .map(|id| {
                self.providers()
                    .iter()
                    .find(|p| p["instanceId"] == id.as_str())
                    .and_then(|p| p["displayName"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| id.clone())
            })
            .collect();
        if !handed.is_empty() {
            lines.push(Line::Icon(
                "arrow-right-left",
                muted,
                format!("Handed off from {}", handed.join(", ")),
                muted,
            ));
        }
        let terminals = self
            .terminal_metadata
            .iter()
            .filter(|((t, _), m)| *t == thread.id && m["hasRunningSubprocess"] == true)
            .count();
        if terminals > 0 {
            lines.push(Line::Icon(
                "terminal",
                Color32::from_rgb(0x46, 0xec, 0xd5),
                if terminals == 1 {
                    "1 terminal process running".into()
                } else {
                    format!("{terminals} terminal processes running")
                },
                muted,
            ));
        }
        if let Some(runtime) = thread.runtime.as_ref().filter(|r| r.last_error.is_some()) {
            let (text, color) = if runtime.last_error_class.as_deref() == Some("usage_limit") {
                ("Usage limit reached", theme::warning())
            } else {
                ("Error occurred", theme::error())
            };
            lines.push(Line::Icon("circle-alert", color, text.into(), color));
        }
        let prs: Vec<Line> = if self.capability("threadPullRequests") {
            thread
                .pull_requests
                .iter()
                .filter_map(|link| {
                    let (icon, tone, _) = crate::ui::thread_pull_requests::status(link)?;
                    let number = link["number"].as_u64().unwrap_or(0);
                    let title = link["snapshot"]["title"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    Some(Line::Pr(icon, tone, format!("#{number}"), title))
                })
                .collect()
        } else {
            Vec::new()
        };
        // Measure: 12px padding, 20px title, 18px lines.
        let inner = WIDTH - 24.0;
        let line = 18.0;
        let height = 12.0
            + 16.0
            + 8.0
            + lines.len() as f32 * line
            + if prs.is_empty() {
                0.0
            } else {
                9.0 + prs.len() as f32 * line
            }
            + 12.0;
        let rect = Rect::from_min_size(
            Pos2::new(row.right() + 4.0, row.top()),
            Vec2::new(WIDTH, height),
        );
        let painter = ui
            .ctx()
            .layer_painter(egui::LayerId::new(egui::Order::Tooltip, id));
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 10],
                blur: 25,
                spread: 0,
                color: Color32::from_black_alpha(60),
            }
            .as_shape(rect, CornerRadius::same(6)),
        );
        painter.rect(
            rect,
            CornerRadius::same(6),
            widgets::menu_surface(),
            Stroke::new(1.0, widgets::menu_outline()),
            StrokeKind::Inside,
        );
        let child = ui.new_child(
            egui::UiBuilder::new()
                .layer_id(painter.layer_id())
                .max_rect(rect),
        );
        let left = rect.left() + 12.0;
        let mut y = rect.top() + 12.0;
        widgets::text_in(
            &child,
            Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, 16.0)),
            &thread.title,
            font(12.0, Weight::Medium),
            theme::foreground(),
        );
        y += 16.0 + 8.0;
        let text_rect = |y: f32| {
            Rect::from_min_size(
                Pos2::new(left + 2.0 + 20.0, y),
                Vec2::new(inner - 22.0, line),
            )
        };
        let icon_rect = |y: f32| {
            Rect::from_center_size(
                Pos2::new(left + 2.0 + 6.0, y + line / 2.0),
                Vec2::splat(12.0),
            )
        };
        for entry in &lines {
            match entry {
                Line::Project(project, name) => {
                    crate::ui::project_icon::paint(&child, icon_rect(y), project);
                    widgets::text_in(
                        &child,
                        text_rect(y),
                        name,
                        font(12.0, Weight::Regular),
                        theme::alpha(theme::foreground(), 0.75),
                    );
                }
                Line::Icon(icon, tint, text, color) => {
                    theme::paint_icon(&child, icon, icon_rect(y), 12.0, *tint);
                    widgets::text_in(
                        &child,
                        text_rect(y),
                        text,
                        font(12.0, Weight::Regular),
                        *color,
                    );
                }
                Line::Pr(..) => {}
            }
            y += line;
        }
        if !prs.is_empty() {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.left() + 12.0, y + 4.0),
                    Vec2::new(inner, 1.0),
                ),
                0.0,
                theme::alpha(theme::border(), 0.6),
            );
            y += 9.0;
            for entry in &prs {
                if let Line::Pr(icon, tone, number, title) = entry {
                    theme::paint_icon(&child, icon, icon_rect(y), 12.0, *tone);
                    let number_width = widgets::text_width(&child, number, theme::mono(12.0));
                    widgets::text(
                        &child,
                        Pos2::new(left + 22.0, y + 1.0),
                        number,
                        theme::mono(12.0),
                        muted,
                        16.0,
                    );
                    widgets::text_in(
                        &child,
                        Rect::from_min_max(
                            Pos2::new(left + 22.0 + number_width + 6.0, y),
                            Pos2::new(left + inner, y + line),
                        ),
                        title,
                        font(12.0, Weight::Regular),
                        theme::alpha(theme::foreground(), 0.75),
                    );
                }
                y += line;
            }
        }
    }
}
