//! The Agents surface (`AgentsPanel.tsx`): the thread's subagents folded
//! from its activities (`subagents`), workflows with their phases, direct
//! spawns and the totals footer.
//!
//! Not ported: the workflow script viewer.
use std::collections::HashMap;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::App;
use crate::subagents::{self, Agent, PanelModel, Phase, PhaseState, Status, WorkflowGroup};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const ROW: f32 = 62.0;
const EMERALD_400: Color32 = Color32::from_rgb(0, 212, 146);
const EMERALD_500: Color32 = Color32::from_rgb(0, 188, 125);

/// Workflow and phase open states (presentation state, keyed by ids).
#[derive(Default)]
pub struct AgentsState {
    workflows: HashMap<String, bool>,
    phases: HashMap<String, bool>,
}

/// `STATUS_VISUALS`: in-flight states all read Working.
fn status_visual(status: Status) -> (Color32, &'static str) {
    match status {
        Status::Pending | Status::Running | Status::Waiting => (theme::BLUE_500, "Working"),
        Status::Idle => (
            theme::alpha(theme::muted_foreground(), 0.5),
            "Idle · resumable",
        ),
        Status::Completed => (EMERALD_500, "Completed"),
        Status::Failed => (theme::error_foreground(), "Failed"),
        Status::Cancelled | Status::Interrupted => {
            (theme::alpha(theme::muted_foreground(), 0.6), "Stopped")
        }
    }
}

fn dot(ui: &Ui, center: Pos2, status: Status) {
    ui.painter()
        .circle_filled(center, 3.0, status_visual(status).0);
}

/// `formatElapsedSeconds`.
fn elapsed(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let minutes = seconds / 60;
    if minutes == 0 {
        return format!("{seconds}s");
    }
    let hours = minutes / 60;
    if hours == 0 {
        format!("{minutes}m {:02}s", seconds % 60)
    } else {
        format!("{hours}h {:02}m", minutes % 60)
    }
}

fn elapsed_between(start: &str, end: Option<&str>) -> Option<String> {
    let start = crate::timeline::parse_time(start)?;
    let end = match end {
        Some(end) => crate::timeline::parse_time(end)?,
        None => chrono::Utc::now(),
    };
    Some(elapsed((end - start).num_seconds()))
}

/// `agentActivityText`: live rows lead with the work, settled with the outcome.
fn activity_text(agent: &Agent) -> Option<String> {
    let tool = agent.last_tool_name.as_ref().map(|t| format!("▸ {t}"));
    if agent.status.active() {
        agent
            .progress
            .clone()
            .or(tool)
            .or_else(|| agent.result.clone())
            .or_else(|| agent.error.clone())
    } else {
        agent
            .error
            .clone()
            .or_else(|| agent.result.clone())
            .or_else(|| agent.progress.clone())
            .or(tool)
    }
}

/// `AgentRow`: identity, activity and metrics on three fixed lines.
fn agent_row(ui: &Ui, rect: Rect, agent: &Agent) {
    let left = rect.left() + 6.0;
    let right = rect.right() - 6.0;
    let first = rect.top() + 4.0;
    let (_, label) = status_visual(agent.status);
    let label = if agent.kind == subagents::Kind::Batch && agent.status == Status::Idle {
        "Idle"
    } else {
        label
    };
    dot(ui, Pos2::new(left + 3.0, first + 10.0), agent.status);
    // The elapsed column (`min-w-14`, right aligned).
    let live = matches!(agent.status, Status::Running | Status::Waiting);
    let mono_small = theme::mono(11.0);
    let mut time_right = right;
    if agent.status == Status::Completed {
        theme::paint_icon(
            ui,
            "check",
            Rect::from_min_size(Pos2::new(right - 12.0, first + 4.0), Vec2::splat(12.0)),
            12.0,
            EMERALD_500,
        );
        time_right -= 16.0;
    }
    let time = agent
        .started_at
        .as_deref()
        .and_then(|start| {
            elapsed_between(
                start,
                if live {
                    None
                } else {
                    agent.completed_at.as_deref()
                },
            )
        })
        .unwrap_or_default();
    let time_width = widgets::text_width(ui, &time, mono_small.clone());
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(time_right - time_width, first),
            Pos2::new(time_right + 1.0, first + 20.0),
        ),
        &time,
        mono_small.clone(),
        theme::alpha(theme::muted_foreground(), 0.8),
    );
    let column_left = (time_right - time_width).min(right - 56.0) - 8.0;
    let text_left = left + 14.0;
    let title_width = widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(text_left, first),
            Pos2::new(column_left, first + 20.0),
        ),
        &agent.title,
        font(14.0, Weight::Medium),
        theme::foreground(),
    );
    let role = agent
        .role
        .as_ref()
        .filter(|role| role.trim().to_lowercase() != agent.title.trim().to_lowercase());
    if let Some(role) = role {
        let chip_left = text_left + title_width + 8.0;
        let width = (widgets::text_width(ui, role, theme::mono(10.0)) + 8.0).min(112.0);
        if chip_left + width < column_left {
            let chip =
                Rect::from_min_size(Pos2::new(chip_left, first + 3.0), Vec2::new(width, 14.0));
            ui.painter().rect(
                chip,
                CornerRadius::same(4),
                Color32::TRANSPARENT,
                Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
                StrokeKind::Inside,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(chip.min + Vec2::new(4.0, 0.0), chip.max),
                role,
                theme::mono(10.0),
                theme::muted_foreground(),
            );
        }
    }
    let activity = activity_text(agent).unwrap_or_else(|| label.to_owned());
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(text_left, first + 20.0),
            Pos2::new(right, first + 38.0),
        ),
        &activity,
        font(12.0, Weight::Regular),
        if agent.status == Status::Failed {
            theme::error_foreground()
        } else {
            theme::muted_foreground()
        },
    );
    let metadata: Vec<String> = [
        subagents::model_label(agent.model.as_deref(), agent.effort.as_deref()),
        Some(match &agent.usage {
            Some(usage) => format!("{} tok", subagents::token_count(usage.total_tokens)),
            None => "— tok".to_owned(),
        }),
        agent
            .usage
            .as_ref()
            .and_then(|u| u.tool_uses)
            .map(|tools| format!("{tools} tools")),
        (agent.activation_count > 1).then(|| format!("run {}", agent.activation_count)),
    ]
    .into_iter()
    .flatten()
    .collect();
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(text_left, first + 38.0),
            Pos2::new(right, first + 54.0),
        ),
        &metadata.join(" · "),
        mono_small,
        theme::alpha(theme::muted_foreground(), 0.7),
    );
}

fn workflow_live(group: &WorkflowGroup) -> bool {
    !group.workflow.status.terminal()
}

fn members(group: &WorkflowGroup) -> Vec<&Agent> {
    group
        .phases
        .iter()
        .flat_map(|p| p.members.iter())
        .chain(group.unphased.iter())
        .collect()
}

fn phase_color(state: PhaseState) -> Color32 {
    match state {
        PhaseState::Running => theme::BLUE_400,
        PhaseState::Done => EMERALD_400,
        PhaseState::Pending => theme::alpha(theme::muted_foreground(), 0.7),
    }
}

impl App {
    /// The surface in `body`.
    pub(crate) fn agents_panel(&mut self, ui: &mut Ui, body: Rect) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        let live = thread
            .session
            .as_ref()
            .is_some_and(|s| !matches!(s.status.as_str(), "stopped" | "interrupted" | "error"));
        let model = subagents::panel_model(subagents::fold(&thread.activities, live));
        if !model.has_agents {
            empty_state(ui, body);
            return;
        }
        if model.running + model.waiting > 0 {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
        let footer_height = 27.0;
        let list = Rect::from_min_max(
            body.min,
            Pos2::new(body.right(), body.bottom() - footer_height),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt("agents-panel")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let width = list.width() - 16.0;
                let mut y = origin.y + 8.0;
                for group in &model.workflows {
                    y = self.workflow_section(ui, group, Pos2::new(origin.x + 8.0, y), width) + 8.0;
                }
                if !model.direct.is_empty() {
                    let header = Rect::from_min_size(
                        Pos2::new(origin.x + 14.0, y + 4.0),
                        Vec2::new(width, 14.0),
                    );
                    tracked_caps(ui, header, "Direct spawns", theme::muted_foreground());
                    y = header.bottom();
                    for agent in &model.direct {
                        agent_row(
                            ui,
                            Rect::from_min_size(
                                Pos2::new(origin.x + 8.0, y),
                                Vec2::new(width, ROW),
                            ),
                            agent,
                        );
                        y += ROW;
                    }
                    y += 8.0;
                }
                ui.allocate_rect(
                    Rect::from_min_max(origin, Pos2::new(list.right(), y)),
                    Sense::hover(),
                );
            });
        footer(
            ui,
            Rect::from_min_max(Pos2::new(body.left(), list.bottom()), body.max),
            &model,
        );
    }

    /// A workflow, collapsed to one line or expanded with its phases;
    /// returns its bottom.
    fn workflow_section(
        &mut self,
        ui: &mut Ui,
        group: &WorkflowGroup,
        origin: Pos2,
        width: f32,
    ) -> f32 {
        let id = group.workflow.id.clone();
        let open = *self
            .agents
            .workflows
            .entry(id.clone())
            .or_insert_with(|| workflow_live(group));
        let all = members(group);
        let name = group
            .workflow
            .workflow_name
            .clone()
            .unwrap_or_else(|| group.workflow.title.clone());
        if !open {
            // `CollapsedWorkflowSection`.
            let row = Rect::from_min_size(origin, Vec2::new(width, 28.0));
            let response = ui.interact(row, ui.id().with(("agents-workflow", &id)), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(
                    row,
                    CornerRadius::same(6),
                    theme::alpha(theme::accent(), 0.4),
                );
            }
            let failed = all.iter().filter(|m| m.status == Status::Failed).count();
            dot(
                ui,
                Pos2::new(row.left() + 9.0, row.center().y),
                if failed > 0 {
                    Status::Failed
                } else {
                    group.workflow.status
                },
            );
            let tokens: f64 = all
                .iter()
                .map(|m| m.usage.as_ref().map_or(0.0, |u| u.total_tokens))
                .sum::<f64>()
                + if all.is_empty() {
                    group
                        .workflow
                        .usage
                        .as_ref()
                        .map_or(0.0, |u| u.total_tokens)
                } else {
                    0.0
                };
            let mut parts = Vec::new();
            if failed > 0 {
                parts.push(format!("{failed} failed"));
            }
            parts.push(format!("{} agents", all.len()));
            parts.push(format!("· {} tok", subagents::token_count(tokens)));
            if let (Some(start), Some(end)) =
                (&group.workflow.started_at, &group.workflow.completed_at)
                && let Some(time) = elapsed_between(start, Some(end))
            {
                parts.push(format!("· {time}"));
            }
            let meta = parts.join("  ");
            theme::paint_icon(
                ui,
                "chevron-right",
                Rect::from_min_size(
                    Pos2::new(row.right() - 18.0, row.center().y - 6.0),
                    Vec2::splat(12.0),
                ),
                12.0,
                theme::alpha(theme::muted_foreground(), 0.8),
            );
            let meta_width = widgets::text_width(ui, &meta, theme::mono(11.0));
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.right() - 24.0 - meta_width, row.top()),
                    row.max,
                ),
                &meta,
                theme::mono(11.0),
                theme::alpha(theme::muted_foreground(), 0.8),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(row.left() + 20.0, row.top()),
                    Pos2::new(row.right() - 32.0 - meta_width, row.bottom()),
                ),
                &name,
                font(14.0, Weight::Regular),
                theme::foreground(),
            );
            if response.clicked() {
                self.agents.workflows.insert(id, true);
            }
            return row.bottom();
        }

        // `ExpandedWorkflowSection`: measure first, then paint the frame under the rows.
        let height = self.expanded_height(ui, group, width);
        let frame = Rect::from_min_size(origin, Vec2::new(width, height));
        ui.painter().rect(
            frame,
            CornerRadius::same(10),
            theme::alpha(theme::card(), 0.3),
            Stroke::new(1.0, theme::alpha(theme::border(), 0.5)),
            StrokeKind::Inside,
        );
        let inner_left = frame.left() + 6.0;
        let inner_right = frame.right() - 6.0;
        let header = Rect::from_min_max(
            Pos2::new(inner_left + 6.0, frame.top() + 8.0),
            Pos2::new(inner_right - 6.0, frame.top() + 24.0),
        );
        dot(
            ui,
            Pos2::new(header.left() + 3.0, header.center().y),
            group.workflow.status,
        );
        let collapse = Rect::from_min_size(
            Pos2::new(header.right() - 16.0, header.top()),
            Vec2::splat(16.0),
        );
        if widgets::icon_button(
            ui,
            collapse,
            ("agents-collapse", &id),
            "chevron-down",
            12.0,
            theme::muted_foreground(),
            "",
        )
        .clicked()
        {
            self.agents.workflows.insert(id.clone(), false);
        }
        let settled = all.iter().filter(|m| m.status.terminal()).count();
        let count = format!("{settled}/{} settled", all.len());
        let count_width = widgets::text_width(ui, &count, theme::mono(10.0));
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(collapse.left() - 8.0 - count_width, header.top()),
                header.max,
            ),
            &count,
            theme::mono(10.0),
            theme::alpha(theme::muted_foreground(), 0.8),
        );
        tracked_caps(
            ui,
            Rect::from_min_max(
                Pos2::new(header.left() + 14.0, header.top()),
                Pos2::new(collapse.left() - 16.0 - count_width, header.bottom()),
            ),
            &name,
            theme::muted_foreground(),
        );
        let mut y = header.bottom();
        if !group.phases.is_empty() {
            y = phase_rail(
                ui,
                &group.phases,
                Pos2::new(inner_left + 6.0, y + 6.0),
                inner_right - inner_left - 12.0,
            ) + 4.0;
        }
        let row_width = inner_right - inner_left;
        for phase in &group.phases {
            y = self.phase_section(ui, group, phase, Pos2::new(inner_left, y), row_width);
        }
        for member in &group.unphased {
            agent_row(
                ui,
                Rect::from_min_size(Pos2::new(inner_left, y), Vec2::new(row_width, ROW)),
                member,
            );
            y += ROW;
        }
        if group.phases.is_empty() && group.unphased.is_empty() {
            agent_row(
                ui,
                Rect::from_min_size(Pos2::new(inner_left, y), Vec2::new(row_width, ROW)),
                &group.workflow,
            );
        }
        frame.bottom()
    }

    fn phase_open(&mut self, group: &WorkflowGroup, phase: &Phase) -> bool {
        let key = format!("{}\0{}", group.workflow.id, phase.index);
        *self
            .agents
            .phases
            .entry(key)
            .or_insert(!workflow_live(group) || phase.state == PhaseState::Running)
    }

    fn expanded_height(&mut self, ui: &Ui, group: &WorkflowGroup, width: f32) -> f32 {
        let mut height = 8.0 + 16.0;
        if !group.phases.is_empty() {
            height = phase_rail_height(ui, &group.phases, width - 24.0) + height + 10.0;
        }
        for phase in &group.phases {
            height += 8.0 + 14.0;
            if self.phase_open(group, phase) {
                height += phase.members.len() as f32 * ROW;
            }
        }
        height += group.unphased.len() as f32 * ROW;
        if group.phases.is_empty() && group.unphased.is_empty() {
            height += ROW;
        }
        height + 6.0
    }

    /// `PhaseSection`: a toggle line, then its members while open.
    fn phase_section(
        &mut self,
        ui: &mut Ui,
        group: &WorkflowGroup,
        phase: &Phase,
        origin: Pos2,
        width: f32,
    ) -> f32 {
        let open = self.phase_open(group, phase);
        let line = Rect::from_min_size(Pos2::new(origin.x, origin.y + 8.0), Vec2::new(width, 14.0));
        let response = ui.interact(
            line,
            ui.id()
                .with(("agents-phase", &group.workflow.id, phase.index.to_bits())),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter().rect_filled(
                line,
                CornerRadius::same(4),
                theme::alpha(theme::accent(), 0.4),
            );
        }
        let color = phase_color(phase.state);
        let mut x = line.left() + 6.0;
        theme::paint_icon(
            ui,
            if open {
                "chevron-down"
            } else {
                "chevron-right"
            },
            Rect::from_min_size(Pos2::new(x, line.top() + 1.0), Vec2::splat(12.0)),
            12.0,
            color,
        );
        x += 18.0;
        if phase.state == PhaseState::Done {
            theme::paint_icon(
                ui,
                "check",
                Rect::from_min_size(Pos2::new(x, line.top() + 1.0), Vec2::splat(12.0)),
                12.0,
                color,
            );
            x += 18.0;
        }
        x = tracked_caps(
            ui,
            Rect::from_min_max(Pos2::new(x, line.top()), line.max),
            &phase.title,
            color,
        ) + 6.0;
        let detail = if phase.state == PhaseState::Pending && phase.members.is_empty() {
            "pending".to_owned()
        } else if phase.state == PhaseState::Done {
            format!("{} done", phase.settled)
        } else {
            format!("{} active · {} done", phase.active, phase.settled)
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, line.top()), line.max),
            &detail,
            font(10.0, Weight::Regular),
            theme::alpha(theme::muted_foreground(), 0.7),
        );
        if !open && !phase.members.is_empty() {
            let mut dot_x = line.right() - 6.0 - phase.members.len() as f32 * 8.0;
            for member in &phase.members {
                dot(ui, Pos2::new(dot_x + 3.0, line.center().y), member.status);
                dot_x += 8.0;
            }
        }
        if response.clicked() {
            let key = format!("{}\0{}", group.workflow.id, phase.index);
            self.agents.phases.insert(key, !open);
        }
        let mut y = line.bottom();
        if open {
            for member in &phase.members {
                agent_row(
                    ui,
                    Rect::from_min_size(Pos2::new(origin.x, y), Vec2::new(width, ROW)),
                    member,
                );
                y += ROW;
            }
        }
        y
    }
}

/// Uppercase, `tracking-wider` 10px label; returns its right edge.
fn tracked_caps(ui: &Ui, rect: Rect, text: &str, color: Color32) -> f32 {
    let mut job = egui::text::LayoutJob::default();
    let mut format = widgets::format(font(10.0, Weight::Medium), color, 14.0);
    format.extra_letter_spacing = 0.5;
    job.append(&text.to_uppercase(), 0.0, format);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width().max(1.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let width = galley.size().x;
    ui.painter().galley(
        Pos2::new(rect.left(), (rect.center().y - 7.0).round()),
        galley,
        color,
    );
    rect.left() + width
}

/// The phase rail's chips laid out in rows; returns them with their rects.
fn rail_layout(ui: &Ui, phases: &[Phase], origin: Pos2, width: f32) -> Vec<(Rect, f32)> {
    let mut out = Vec::new();
    let (mut x, mut y) = (origin.x, origin.y);
    for (index, phase) in phases.iter().enumerate() {
        let check = if phase.state == PhaseState::Done {
            12.0
        } else {
            0.0
        };
        let text_width = widgets::text_width(ui, &phase.title, theme::mono(10.0)) + check;
        let dots = if phase.members.is_empty() {
            8.0
        } else {
            phase.members.len() as f32 * 8.0 - 2.0
        };
        let chip_width = 6.0 + text_width + 4.0 + dots + 6.0;
        let lead = if index > 0 { 16.0 } else { 0.0 };
        if x + lead + chip_width > origin.x + width && x > origin.x {
            x = origin.x;
            y += 22.0;
        }
        out.push((
            Rect::from_min_size(Pos2::new(x + lead, y), Vec2::new(chip_width, 18.0)),
            text_width,
        ));
        x += lead + chip_width + 4.0;
    }
    out
}

fn phase_rail_height(ui: &Ui, phases: &[Phase], width: f32) -> f32 {
    rail_layout(ui, phases, Pos2::ZERO, width)
        .last()
        .map_or(0.0, |(rect, _)| rect.bottom())
}

/// `PhaseRail`: one chip per phase, chevrons between; returns its bottom.
fn phase_rail(ui: &Ui, phases: &[Phase], origin: Pos2, width: f32) -> f32 {
    let layout = rail_layout(ui, phases, origin, width);
    for (index, (phase, (chip, text_width))) in phases.iter().zip(&layout).enumerate() {
        if index > 0 {
            theme::paint_icon(
                ui,
                "chevron-right",
                Rect::from_min_size(
                    Pos2::new(chip.left() - 14.0, chip.top() + 3.0),
                    Vec2::splat(12.0),
                ),
                12.0,
                theme::alpha(theme::muted_foreground(), 0.4),
            );
        }
        let border = match phase.state {
            PhaseState::Running => theme::alpha(theme::BLUE_500, 0.4),
            PhaseState::Done => theme::alpha(EMERALD_500, 0.3),
            PhaseState::Pending => theme::alpha(theme::border(), 0.5),
        };
        ui.painter().rect(
            *chip,
            CornerRadius::same(4),
            Color32::TRANSPARENT,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        // "✓ " leads a done phase; the mono font has no check glyph.
        let mut label_left = chip.left() + 6.0;
        if phase.state == PhaseState::Done {
            theme::paint_icon(
                ui,
                "check",
                Rect::from_min_size(
                    Pos2::new(label_left, chip.center().y - 5.0),
                    Vec2::splat(10.0),
                ),
                10.0,
                phase_color(phase.state),
            );
            label_left += 12.0;
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(label_left, chip.top()), chip.max),
            &phase.title,
            theme::mono(10.0),
            phase_color(phase.state),
        );
        let mut x = chip.left() + 6.0 + text_width + 4.0;
        if phase.members.is_empty() {
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x, chip.top()), chip.max),
                "–",
                theme::mono(10.0),
                theme::alpha(theme::muted_foreground(), 0.5),
            );
        }
        for member in &phase.members {
            dot(ui, Pos2::new(x + 3.0, chip.center().y), member.status);
            x += 8.0;
        }
    }
    layout.last().map_or(origin.y, |(rect, _)| rect.bottom())
}

/// The totals footer: working, idle and settled counts and the token sum.
fn footer(ui: &Ui, rect: Rect, model: &PanelModel) {
    ui.painter().rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), 1.0)),
        0.0,
        theme::alpha(theme::border(), 0.6),
    );
    let mono = theme::mono(11.0);
    let mut x = rect.left() + 12.0;
    let mut part = |ui: &Ui, text: String, color: Color32| {
        let width = widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, rect.top() + 1.0), rect.max),
            &text,
            mono.clone(),
            color,
        );
        x += width + 8.0;
    };
    if model.running + model.waiting > 0 {
        part(
            ui,
            format!("● {} working", model.running + model.waiting),
            theme::BLUE_400,
        );
    }
    if model.idle > 0 {
        part(
            ui,
            format!("{} idle", model.idle),
            theme::muted_foreground(),
        );
    }
    if model.settled > 0 {
        part(
            ui,
            format!("{} settled", model.settled),
            theme::muted_foreground(),
        );
    }
    let total = format!("Σ {} tok", subagents::token_count(model.total_tokens));
    let width = widgets::text_width(ui, &total, theme::mono(11.0));
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.right() - 12.0 - width, rect.top() + 1.0),
            rect.max,
        ),
        &total,
        theme::mono(11.0),
        theme::muted_foreground(),
    );
}

/// "No agents yet", centered (`gap-2 p-6`).
fn empty_state(ui: &Ui, body: Rect) {
    let center = body.center();
    let icon = Rect::from_center_size(Pos2::new(center.x, center.y - 42.0), Vec2::splat(24.0));
    theme::paint_icon(
        ui,
        "bot",
        icon,
        24.0,
        theme::alpha(theme::muted_foreground(), 0.6),
    );
    let title = "No agents yet";
    let width = widgets::text_width(ui, title, font(14.0, Weight::Medium));
    widgets::text_in(
        ui,
        Rect::from_min_size(
            Pos2::new(center.x - width / 2.0, icon.bottom() + 8.0),
            Vec2::new(width + 1.0, 20.0),
        ),
        title,
        font(14.0, Weight::Medium),
        theme::foreground(),
    );
    // `max-w-56`, each line centered.
    let mut job = egui::text::LayoutJob::single_section(
        "When this thread spawns subagents or runs a workflow, they show up here with live status, activity, and token usage.".to_owned(),
        widgets::format(font(12.0, Weight::Regular), theme::muted_foreground(), 16.0),
    );
    job.wrap.max_width = 224.0;
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        Pos2::new(center.x, icon.bottom() + 36.0),
        galley,
        theme::muted_foreground(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_reads_like_upstream() {
        assert_eq!(elapsed(42), "42s");
        assert_eq!(elapsed(125), "2m 05s");
        assert_eq!(elapsed(3 * 3600 + 7 * 60), "3h 07m");
    }
}
