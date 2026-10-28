//! Lifecycle cards in the timeline (`V2LifecycleRow.tsx`,
//! `TimelineSystemDivider.tsx`, `MessagesTimeline.tsx`'s `V2SubagentGroup`):
//! an interrupted run and a context handoff as dividers, a fork as a divider
//! whose pill opens the other conversation, and subagents as cards that open
//! their thread, adjacent ones of a provider turn grouped under a
//! disclosure. A card reads the thread's projected subagent for its live
//! status, like upstream.
//!
//! Not ported: the subagent hover card (`SubagentTimelineTooltip`), the
//! middle truncation of path-like details, and the provider's own label
//! colors (a handoff's models take the provider mark's tint).

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};
use serde_json::Value;

use crate::app::App;
use crate::model::{Detail, Subagent, TurnItem};
use crate::theme::{self, Weight, font};
use crate::timeline::WorkEntry;
use crate::ui::widgets;

const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74); // --destructive
const SUCCESS: Color32 = Color32::from_rgb(0, 188, 125); // --success
/// The subagent card: `py-1.5` around a 12px title and an 11px relaxed line.
const CARD: f32 = 46.0;
/// The group's disclosure: `py-2` around a 12px label and a 10px summary.
const GROUP: f32 = 46.0;

/// A piece of a divider's pill.
enum Piece {
    Icon(&'static str, Color32),
    /// Text, its font, color and widest extent.
    Text(String, FontId, Color32, f32),
    Gap(f32),
}

fn piece_width(ui: &Ui, piece: &Piece) -> f32 {
    match piece {
        Piece::Icon(..) => 12.0,
        Piece::Text(text, font, _, max) => widgets::text_width(ui, text, font.clone()).min(*max),
        Piece::Gap(gap) => *gap,
    }
}

fn paint_pieces(ui: &Ui, mut x: f32, center: f32, pieces: &[Piece]) {
    for piece in pieces {
        match piece {
            Piece::Icon(icon, color) => theme::paint_icon(
                ui,
                icon,
                Rect::from_center_size(Pos2::new(x + 6.0, center), Vec2::splat(12.0)),
                12.0,
                *color,
            ),
            Piece::Text(text, font, color, max) => {
                let galley = widgets::galley_truncated(ui, text, font.clone(), *color, 16.0, *max);
                ui.painter()
                    .galley(Pos2::new(x, center - 8.0), galley, *color);
            }
            Piece::Gap(_) => {}
        }
        x += piece_width(ui, piece);
    }
}

/// `TimelineSystemDivider`: a centered pill between two hairlines, a button
/// when it has an action. Returns the pill's response.
fn divider(ui: &mut Ui, width: f32, pieces: &[Piece], action: Option<&str>) -> egui::Response {
    let height = if action.is_some() { 42.0 } else { 40.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let content: f32 = pieces.iter().map(|p| piece_width(ui, p)).sum();
    let pad = if action.is_some() { 11.0 } else { 8.0 };
    let pill_width = (content + pad * 2.0).min(width - 32.0);
    let pill = Rect::from_center_size(
        rect.center(),
        Vec2::new(pill_width, if action.is_some() { 26.0 } else { 24.0 }),
    );
    let response = ui.interact(
        pill,
        ui.id().with(("divider", rect.top() as i32)),
        if action.is_some() {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let line = theme::alpha(theme::border(), 0.7);
    for (from, to) in [
        (rect.left(), pill.left() - 8.0),
        (pill.right() + 8.0, rect.right()),
    ] {
        if to > from {
            ui.painter().rect_filled(
                Rect::from_min_max(
                    Pos2::new(from, rect.center().y - 0.5),
                    Pos2::new(to, rect.center().y + 0.5),
                ),
                0.0,
                line,
            );
        }
    }
    if action.is_some() {
        let fill = if response.hovered() {
            theme::accent()
        } else {
            theme::background()
        };
        ui.painter().rect(
            pill,
            CornerRadius::same(13),
            fill,
            Stroke::new(1.0, line),
            StrokeKind::Inside,
        );
    }
    paint_pieces(ui, pill.left() + pad, pill.center().y, pieces);
    match action {
        Some(label) => response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(label),
        None => response,
    }
}

/// `STATUS_VISUALS`: the dot and the word for a subagent's status.
fn status_visual(status: &str) -> (Color32, &'static str) {
    let muted = theme::muted_foreground();
    match status {
        "pending" => (theme::BLUE_500, "Queued"),
        "running" => (theme::BLUE_500, "Running"),
        "waiting" => (theme::BLUE_500, "Waiting"),
        "idle" => (theme::alpha(muted, 0.5), "Idle · resumable"),
        "completed" => (SUCCESS, "Completed"),
        "failed" => (DESTRUCTIVE, "Failed"),
        _ => (theme::alpha(muted, 0.6), "Stopped"),
    }
}

fn active(status: &str) -> bool {
    matches!(status, "pending" | "running" | "waiting")
}

/// `plainDetail`: one line of a markdown result.
fn plain_detail(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    // `[label](target)` reads as its label.
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    let lines: Vec<String> = out
        .replace('`', "")
        .lines()
        .map(|line| {
            let trimmed = line.trim_start_matches([' ', '\t']);
            match trimmed.strip_prefix(['-', '*']) {
                Some(after) if after.starts_with([' ', '\t']) => after.trim_start().to_owned(),
                _ => line.to_owned(),
            }
        })
        .collect();
    lines
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A subagent as its card shows it: the item, updated by the thread's
/// projected subagent.
struct Agent {
    title: String,
    driver: Option<String>,
    status: String,
    detail: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    child_thread_id: Option<String>,
}

fn agent(item: &TurnItem, detail: &Detail) -> Agent {
    let live: Option<&Subagent> = item
        .str("subagentId")
        .and_then(|id| detail.subagents.iter().find(|a| a.id == id));
    let status = live.map_or(item.status.clone(), |a| a.status.clone());
    let pick = |live: Option<&Option<String>>, key: &str| {
        live.and_then(|v| v.clone())
            .or_else(|| item.str(key).map(str::to_owned))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let result = pick(live.map(|a| &a.result), "result");
    let progress = pick(live.map(|a| &a.progress), "progress");
    let settled = matches!(
        status.as_str(),
        "completed" | "failed" | "cancelled" | "interrupted"
    );
    let raw = if settled {
        result.or(progress)
    } else {
        progress.or(result)
    };
    // The server's placeholder when a child ends without output.
    let detail = raw
        .filter(|d| !d.to_lowercase().starts_with("child task ended with status"))
        .map(|d| plain_detail(&d))
        .filter(|d| !d.is_empty());
    Agent {
        title: crate::model::subagent_display_title(item.title.as_deref().unwrap_or("Subagent")),
        driver: item.str("driver").map(str::to_owned),
        detail,
        started_at: live
            .and_then(|a| a.started_at.clone())
            .or_else(|| item.started_at.clone()),
        completed_at: live
            .and_then(|a| a.completed_at.clone())
            .or_else(|| item.completed_at.clone()),
        child_thread_id: item.str("childThreadId").map(str::to_owned),
        status,
    }
}

/// `formatElapsedSeconds` of `deriveSubagentElapsedMs`.
fn elapsed(status: &str, started_at: Option<&str>, completed_at: Option<&str>) -> Option<String> {
    let start = crate::timeline::parse_time(started_at?)?;
    let end = if active(status) {
        chrono::Utc::now()
    } else {
        crate::timeline::parse_time(completed_at?)?
    };
    let seconds = (end - start).num_seconds().max(0);
    let minutes = seconds / 60;
    Some(if minutes == 0 {
        format!("{seconds}s")
    } else if minutes < 60 {
        format!("{minutes}m {:02}s", seconds % 60)
    } else {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    })
}

/// `summarizeSubagentStatuses`.
fn summarize(statuses: &[&str]) -> String {
    let mut counts = [0usize; 5];
    for status in statuses {
        let slot = match *status {
            s if active(s) => 0,
            "completed" => 1,
            "failed" => 2,
            "idle" => 4,
            _ => 3,
        };
        counts[slot] += 1;
    }
    ["working", "done", "failed", "stopped", "idle"]
        .iter()
        .zip(counts)
        .filter(|(_, count)| *count > 0)
        .map(|(word, count)| format!("{count} {word}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// `SubagentAvatar`: the provider's mark in a round tile, the status dot at
/// its corner.
fn avatar(ui: &Ui, center: Pos2, mark: Option<(&'static str, Color32)>, status: Option<&str>) {
    let painter = ui.painter();
    painter.circle_filled(center, 14.0, theme::background());
    painter.circle(
        center,
        12.0,
        theme::muted(),
        Stroke::new(1.0, theme::alpha(theme::border(), 0.7)),
    );
    let (icon, tint) = mark.unwrap_or(("bot", theme::muted_foreground()));
    theme::paint_icon(
        ui,
        icon,
        Rect::from_center_size(center, Vec2::splat(14.0)),
        14.0,
        tint,
    );
    if let Some(status) = status {
        let dot = center + Vec2::new(12.0 - 3.0, 12.0 - 3.0);
        painter.circle_filled(dot, 6.0, theme::background());
        painter.circle_filled(dot, 4.0, status_visual(status).0);
    }
}

impl App {
    /// A lifecycle row and the space under it.
    pub(crate) fn event_row(&mut self, ui: &mut Ui, id: &str, entries: &[WorkEntry], width: f32) {
        let Some(detail) = self.current_detail() else {
            return;
        };
        let items: Vec<&TurnItem> = entries.iter().filter_map(|e| e.item.as_deref()).collect();
        let Some(first) = items.first() else {
            return;
        };
        if items.len() > 1 {
            self.subagent_group(ui, id, &items, &detail, width);
        } else {
            match first.kind.as_str() {
                "subagent" => self.subagent_card(ui, first, &detail, width, 0.0),
                "fork" => self.fork_divider(ui, first, width),
                "handoff" => self.handoff_divider(ui, first, &detail, width),
                _ => {
                    // `run_interrupt_result`.
                    let mut pieces = vec![
                        Piece::Icon("x", DESTRUCTIVE),
                        Piece::Gap(6.0),
                        Piece::Text(
                            "Run interrupted".into(),
                            font(11.0, Weight::Medium),
                            DESTRUCTIVE,
                            f32::INFINITY,
                        ),
                    ];
                    if let Some(message) = first.str("message").filter(|m| !m.trim().is_empty()) {
                        pieces.push(Piece::Gap(6.0));
                        pieces.push(Piece::Text(
                            format!("·\u{a0}{}", message.trim()),
                            font(11.0, Weight::Regular),
                            theme::alpha(DESTRUCTIVE, 0.7),
                            320.0,
                        ));
                    }
                    divider(ui, width, &pieces, None);
                }
            }
        }
        ui.add_space(8.0);
    }

    fn fork_divider(&mut self, ui: &mut Ui, item: &TurnItem, width: f32) {
        let from_run = item.get("source")["type"] == "run";
        let (label, action) = if from_run {
            ("Forked from conversation", "Open source conversation")
        } else {
            ("Conversation fork", "Open fork")
        };
        let related = if from_run {
            item.get("source")["threadId"].as_str()
        } else {
            item.str("targetThreadId")
        }
        .map(str::to_owned);
        let muted = theme::muted_foreground();
        let pieces = [
            Piece::Icon("git-fork", muted),
            Piece::Gap(6.0),
            Piece::Text(
                label.into(),
                font(11.0, Weight::Medium),
                muted,
                f32::INFINITY,
            ),
        ];
        if divider(ui, width, &pieces, Some(action)).clicked()
            && let Some(thread) = related
        {
            self.select_thread(Some(thread));
        }
    }

    /// `HandoffRow` (`resolveHandoffEndpoints`): the providers the context
    /// came from, an arrow, and the one it went to.
    fn handoff_divider(&self, ui: &mut Ui, item: &TurnItem, detail: &Detail, width: f32) {
        let handoff_run = item
            .run_id
            .as_deref()
            .and_then(|id| detail.runs.iter().find(|r| r.id == id));
        let to_instance = item
            .str("toProviderInstanceId")
            .unwrap_or_default()
            .to_owned();
        let to_model = item.str("toModel").map(str::to_owned).or_else(|| {
            handoff_run
                .filter(|r| r.provider_instance_id == to_instance)
                .map(|r| r.model_selection.model.clone())
        });
        let selections: Vec<(String, Option<String>)> = item
            .get("fromModelSelections")
            .as_array()
            .filter(|list| !list.is_empty())
            .map(|list| {
                list.iter()
                    .map(|s| {
                        (
                            s["instanceId"].as_str().unwrap_or_default().to_owned(),
                            s["model"].as_str().map(str::to_owned),
                        )
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                item.get("fromProviderInstanceIds")
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|instance| {
                        // The newest run of that instance before the handoff's own.
                        let model = detail
                            .runs
                            .iter()
                            .filter(|r| {
                                r.provider_instance_id == instance
                                    && handoff_run.is_none_or(|h| r.ordinal < h.ordinal)
                            })
                            .max_by_key(|r| r.ordinal)
                            .map(|r| r.model_selection.model.clone());
                        (instance.to_owned(), model)
                    })
                    .collect()
            });
        let failed = item.status == "failed";
        let base = if failed {
            DESTRUCTIVE
        } else {
            theme::muted_foreground()
        };
        let mut pieces = vec![
            Piece::Icon("arrow-right-left", base),
            Piece::Gap(6.0),
            Piece::Text(
                "Context handoff".into(),
                font(11.0, Weight::Medium),
                base,
                f32::INFINITY,
            ),
            Piece::Gap(6.0),
        ];
        let endpoint = |pieces: &mut Vec<Piece>, instance: &str, model: Option<&str>| {
            let selection = crate::model::ModelSelection {
                instance_id: instance.to_owned(),
                model: model.unwrap_or_default().to_owned(),
                options: None,
            };
            let mark = self
                .provider_driver(instance)
                .and_then(|d| crate::ui::pickers::provider_icon(&d));
            let label = match model.filter(|m| !m.trim().is_empty()) {
                Some(_) => self.model_label(&selection).0,
                None => self
                    .providers()
                    .iter()
                    .find(|p| p["instanceId"] == instance)
                    .and_then(|p| p["displayName"].as_str())
                    .unwrap_or(instance)
                    .to_owned(),
            };
            if let Some((icon, tint)) = mark {
                pieces.push(Piece::Icon(icon, theme::alpha(tint, 0.7)));
                pieces.push(Piece::Gap(4.0));
            }
            let color = mark.map_or(base, |(_, tint)| tint);
            pieces.push(Piece::Text(
                label,
                font(11.0, Weight::Medium),
                theme::alpha(color, 0.7),
                160.0,
            ));
        };
        for (index, (instance, model)) in selections.iter().enumerate() {
            endpoint(&mut pieces, instance, model.as_deref());
            if index + 1 < selections.len() {
                pieces.push(Piece::Text(
                    ",".into(),
                    font(11.0, Weight::Regular),
                    theme::alpha(base, 0.7),
                    f32::INFINITY,
                ));
            }
            pieces.push(Piece::Gap(6.0));
        }
        if !selections.is_empty() {
            pieces.push(Piece::Icon("arrow-right", theme::alpha(base, 0.7)));
            pieces.push(Piece::Gap(6.0));
        }
        endpoint(&mut pieces, &to_instance, to_model.as_deref());
        divider(ui, width, &pieces, None);
    }

    /// `SubagentTimelineLink`: opens the child's thread when it has one.
    fn subagent_card(
        &mut self,
        ui: &mut Ui,
        item: &TurnItem,
        detail: &Detail,
        width: f32,
        inset: f32,
    ) {
        let agent = agent(item, detail);
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, CARD), Sense::hover());
        let rect = row.shrink2(Vec2::new(inset, 0.0));
        let opens = agent.child_thread_id.is_some();
        let response = ui.interact(
            rect,
            ui.id().with(("subagent", &item.id)),
            if opens {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if opens && response.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(6),
                theme::alpha(theme::accent(), 0.2),
            );
        }
        let mark = agent
            .driver
            .as_deref()
            .and_then(crate::ui::pickers::provider_icon);
        avatar(
            ui,
            Pos2::new(rect.left() + 8.0 + 12.0, rect.center().y),
            mark,
            Some(&agent.status),
        );
        let muted = theme::muted_foreground();
        let mut right = rect.right() - 8.0;
        if opens {
            theme::paint_icon(
                ui,
                "chevron-right",
                Rect::from_center_size(Pos2::new(right - 7.0, rect.center().y), Vec2::splat(14.0)),
                14.0,
                if response.hovered() {
                    theme::foreground()
                } else {
                    theme::alpha(muted, 0.6)
                },
            );
            right -= 14.0 + 10.0;
        }
        if let Some(time) = elapsed(
            &agent.status,
            agent.started_at.as_deref(),
            agent.completed_at.as_deref(),
        ) {
            let time_font = font(12.0, Weight::Regular);
            let time_width = widgets::tabular_width(ui, &time, time_font.clone());
            widgets::tabular_text(
                ui,
                Pos2::new(right - time_width, rect.center().y - 8.0),
                &time,
                time_font,
                muted,
                16.0,
            );
            right -= time_width + 10.0;
            if active(&agent.status) {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        let left = rect.left() + 8.0 + 24.0 + 10.0;
        let failed = agent.status == "failed";
        let (_, status_label) = status_visual(&agent.status);
        let title_width = widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, rect.top() + 6.0),
                Pos2::new(right, rect.top() + 22.0),
            ),
            &agent.title,
            font(12.0, Weight::Medium),
            theme::foreground(),
        );
        if agent.detail.is_some() && agent.status != "completed" {
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(left + title_width + 8.0, rect.top() + 7.0),
                    Pos2::new(right, rect.top() + 22.0),
                ),
                status_label,
                font(10.0, Weight::Regular),
                if failed { DESTRUCTIVE } else { muted },
            );
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, rect.top() + 22.0),
                Pos2::new(right, rect.top() + 40.0),
            ),
            agent.detail.as_deref().unwrap_or(status_label),
            font(11.0, Weight::Regular),
            if failed { DESTRUCTIVE } else { muted },
        );
        let response = if opens {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        };
        if response.clicked()
            && let Some(thread) = agent.child_thread_id
        {
            self.select_thread(Some(thread));
        }
    }

    /// `V2SubagentGroup`: the members' avatars, their count and states, and
    /// the cards once opened.
    fn subagent_group(
        &mut self,
        ui: &mut Ui,
        id: &str,
        items: &[&TurnItem],
        detail: &Detail,
        width: f32,
    ) {
        let key = format!("subagent-group:{id}");
        let expanded = self.expansion.groups.contains(&key);
        let agents: Vec<Agent> = items.iter().map(|item| agent(item, detail)).collect();
        let statuses: Vec<&str> = agents.iter().map(|a| a.status.as_str()).collect();
        let live = statuses.iter().any(|s| active(s));
        let failed = statuses.contains(&"failed");
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, GROUP), Sense::hover());
        let response = ui
            .interact(rect, ui.id().with(&key), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.clicked() {
            if !self.expansion.groups.remove(&key) {
                self.expansion.groups.insert(key.clone());
            }
            self.keep_scroll = true;
        }
        let opacity = if expanded || live || response.hovered() {
            1.0
        } else {
            0.55
        };
        let tint = |color: Color32| theme::alpha(color, opacity);
        let mut x = rect.left() + 12.0;
        for agent in agents.iter().take(3) {
            let mark = agent
                .driver
                .as_deref()
                .and_then(crate::ui::pickers::provider_icon);
            avatar(
                ui,
                Pos2::new(x, rect.center().y),
                mark,
                (agents.len() == 1).then_some(agent.status.as_str()),
            );
            x += 24.0 - 6.0;
        }
        if agents.len() > 3 {
            let center = Pos2::new(x, rect.center().y);
            ui.painter()
                .circle_filled(center, 14.0, theme::background());
            ui.painter().circle_filled(center, 12.0, theme::muted());
            let more = format!("+{}", agents.len() - 3);
            let more_font = font(10.0, Weight::Medium);
            let more_width = widgets::text_width(ui, &more, more_font.clone());
            widgets::text(
                ui,
                Pos2::new(center.x - more_width / 2.0, center.y - 7.0),
                &more,
                more_font,
                theme::muted_foreground(),
                14.0,
            );
            x += 24.0 - 6.0;
        }
        // The stack's right edge, then `gap-3`.
        let left = x - 18.0 + 12.0 + 12.0;
        let muted = theme::muted_foreground();
        let mut right = rect.right();
        theme::paint_icon(
            ui,
            if expanded {
                "chevron-up"
            } else {
                "chevron-down"
            },
            Rect::from_center_size(Pos2::new(right - 7.0, rect.center().y), Vec2::splat(14.0)),
            14.0,
            tint(muted),
        );
        right -= 14.0 + 12.0;
        // `subagentGroupTiming`: the earliest start to the latest end.
        let start = agents.iter().filter_map(|a| a.started_at.clone()).min();
        let end = agents
            .iter()
            .map(|a| a.completed_at.clone())
            .collect::<Option<Vec<_>>>()
            .and_then(|ends| ends.into_iter().max());
        let timing_status = if live { "running" } else { "completed" };
        if let Some(time) = elapsed(timing_status, start.as_deref(), end.as_deref()) {
            let time_font = font(12.0, Weight::Regular);
            let time_width = widgets::tabular_width(ui, &time, time_font.clone());
            widgets::tabular_text(
                ui,
                Pos2::new(right - time_width, rect.center().y - 8.0),
                &time,
                time_font,
                tint(muted),
                16.0,
            );
            right -= time_width + 12.0;
            if live {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        let label = format!("{} subagents", agents.len());
        let text_color = if expanded || live {
            theme::foreground()
        } else {
            muted
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, rect.top() + 8.0),
                Pos2::new(right, rect.top() + 24.0),
            ),
            &label,
            font(12.0, Weight::SemiBold),
            tint(text_color),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left, rect.top() + 24.0),
                Pos2::new(right, rect.top() + 38.0),
            ),
            &summarize(&statuses),
            font(10.0, Weight::Regular),
            tint(if live {
                theme::BLUE_500
            } else if failed {
                DESTRUCTIVE
            } else {
                muted
            }),
        );
        if !expanded {
            return;
        }
        // The members, in a bordered panel (`mt-1 mb-1 p-1`).
        ui.add_space(4.0);
        let top = ui.cursor().top();
        let panel = Rect::from_min_size(
            Pos2::new(rect.left(), top),
            Vec2::new(width, items.len() as f32 * CARD + 8.0),
        );
        ui.painter().rect(
            panel,
            CornerRadius::same(8),
            theme::alpha(theme::card(), 0.3),
            Stroke::new(1.0, theme::alpha(theme::border(), 0.6)),
            StrokeKind::Inside,
        );
        ui.add_space(4.0);
        for item in items {
            self.subagent_card(ui, item, detail, width, 4.0);
        }
        ui.add_space(4.0 + 4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_read_as_one_plain_line() {
        assert_eq!(
            plain_detail("- Fixed [the parser](src/p.rs)\n- Ran `cargo test`"),
            "Fixed the parser Ran cargo test"
        );
    }

    #[test]
    fn group_states_count_in_reading_order() {
        assert_eq!(
            summarize(&["completed", "running", "failed", "completed", "interrupted"]),
            "1 working · 2 done · 1 failed · 1 stopped"
        );
    }
}
