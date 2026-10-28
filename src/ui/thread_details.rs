//! The thread details card (`ThreadDetailsPanel.tsx`, `ThreadDetailsCard.tsx`
//! as its popover, `ThreadRelationshipsControl.tsx`,
//! `client-runtime/state/threadRelationships.ts`): the header's "Toggle
//! thread details" button opens a 280px card holding the thread's workspace
//! (its checkout and branch) and its lineage: the parent thread, forks,
//! subagents and context transfers, each opening its thread, settled
//! subagents folded under "Previous agents", six rows at a time. A fork
//! pairs its parent's row with "Merge back" (`thread.merge_back` from its
//! latest finished run).
//!
//! Not ported: the workspace section's pickers, scripts, editor and git
//! controls (the header keeps them), the inline card beside a wide canvas,
//! automations, the subagent hover cards, stopping a subagent or workflow
//! from its row, and "Disconnect agent session".

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Pos2, Rect, Sense, Ui, Vec2};
#[cfg(test)]
use serde_json::Value;
use serde_json::json;

use crate::app::App;
use crate::model::{Detail, Thread};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

pub(crate) const MENU: &str = "thread-details";
/// `THREAD_DETAILS_CARD_WIDTH`.
const WIDTH: f32 = 280.0;
/// `THREAD_DETAILS_CARD_GAP`.
const GAP: f32 = 12.0;
const ROW: f32 = 32.0;
/// `THREAD_LINEAGE_INITIAL_COUNT` and `THREAD_LINEAGE_PAGE_COUNT`.
const INITIAL: usize = 6;
const PAGE: usize = 12;
/// The lineage list's bound (`max-h-[13.5rem]`).
const LIST_MAX: f32 = 216.0;

/// `ThreadRelationshipKind`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Fork,
    Subagent,
    Transfer,
}

/// `ThreadRelationshipEdge`.
#[derive(Clone, Debug, PartialEq)]
struct Edge {
    source: String,
    target: String,
    kind: Kind,
    status: Option<String>,
}

/// A related thread as its row shows it.
#[derive(Clone, Debug, PartialEq)]
struct Related {
    thread_id: String,
    edge: Edge,
    /// The thread's shell, when this client has it.
    shell: Option<(String, String)>,
}

fn shell_status(thread: &Thread) -> String {
    thread
        .activity_run_status
        .clone()
        .flatten()
        .unwrap_or_else(|| thread.status.clone())
}

/// `deriveThreadRelationshipGraph`'s edges touching `current`, one per
/// related thread (`immediateThreadRelationships`).
fn relationships(
    threads: &[std::sync::Arc<Thread>],
    detail: &Detail,
    current: &str,
) -> Vec<Related> {
    let by_id: HashMap<&str, &Thread> = threads
        .iter()
        .map(|t| (t.id.as_str(), t.as_ref()))
        .collect();
    let mut edges: Vec<Edge> = Vec::new();
    let mut add = |edge: Edge| {
        let status = match (&edge.kind, edge.status.as_deref()) {
            (Kind::Subagent, Some("rolled_back")) => Some("interrupted".to_owned()),
            _ => edge.status.clone(),
        };
        let edge = Edge { status, ..edge };
        match edges
            .iter_mut()
            .find(|e| e.source == edge.source && e.target == edge.target && e.kind == edge.kind)
        {
            Some(existing) => *existing = edge,
            None => edges.push(edge),
        }
    };
    for thread in threads {
        let parent = match &thread.forked_from {
            Some(from) if from["type"] == "run" => from["threadId"].as_str().map(str::to_owned),
            _ => thread.lineage.parent_thread_id.clone(),
        };
        let Some(parent) = parent else {
            continue;
        };
        add(Edge {
            source: parent,
            target: thread.id.clone(),
            kind: if thread.lineage.relationship_to_parent.as_deref() == Some("subagent") {
                Kind::Subagent
            } else {
                Kind::Fork
            },
            status: Some(shell_status(thread)),
        });
    }
    if detail.id() == current {
        for agent in &detail.subagents {
            let Some(child) = &agent.child_thread_id else {
                continue;
            };
            let status = match by_id.get(child.as_str()) {
                Some(t) if t.activity_run_status.clone().flatten().is_some() => shell_status(t),
                Some(t) if t.latest_run_id.is_some() => t.status.clone(),
                _ => agent.status.clone(),
            };
            add(Edge {
                source: current.to_owned(),
                target: child.clone(),
                kind: Kind::Subagent,
                status: Some(status),
            });
        }
        for transfer in &detail.context_transfers {
            let (Some(source), Some(target)) = (
                transfer["sourceThreadId"].as_str(),
                transfer["targetThreadId"].as_str(),
            ) else {
                continue;
            };
            if source == target {
                continue;
            }
            add(Edge {
                source: source.to_owned(),
                target: target.to_owned(),
                kind: Kind::Transfer,
                status: transfer["status"].as_str().map(str::to_owned),
            });
        }
    }
    let mut seen = HashSet::new();
    edges
        .into_iter()
        .filter_map(|edge| {
            let other = if edge.source == current {
                edge.target.clone()
            } else if edge.target == current {
                edge.source.clone()
            } else {
                return None;
            };
            if !seen.insert(other.clone()) {
                return None;
            }
            let shell = by_id
                .get(other.as_str())
                .map(|t| (t.title.clone(), t.created_at.clone()));
            Some(Related {
                thread_id: other,
                edge,
                shell,
            })
        })
        .collect()
}

/// `isParentThreadRelationship`.
fn is_parent(edge: &Edge, current: &str) -> bool {
    edge.kind != Kind::Transfer && edge.target == current
}

/// `threadRelationshipRowStatus`: a parent row shows its own activity.
fn row_status(threads: &[std::sync::Arc<Thread>], row: &Related) -> Option<String> {
    if row.edge.kind == Kind::Transfer || row.thread_id == row.edge.target {
        return row.edge.status.clone();
    }
    threads
        .iter()
        .find(|t| t.id == row.thread_id)
        .map(|t| shell_status(t))
}

/// `threadRelationshipStatusLabel`.
fn status_label(status: Option<&str>) -> &'static str {
    match status {
        Some("preparing" | "starting") => "Starting",
        Some("running" | "in_progress") => "Running",
        Some("pending" | "queued") => "Queued",
        Some("waiting" | "blocked") => "Waiting",
        Some("completed") => "Done",
        Some("failed" | "error") => "Failed",
        Some("cancelled" | "interrupted") => "Stopped",
        Some("rolled_back") => "Reverted",
        Some("resolved_native") => "Resolved (native)",
        Some("resolved_portable") => "Resolved (portable)",
        Some("consumed") => "Consumed",
        Some("superseded") => "Superseded",
        Some("idle") => "Idle",
        _ => "Unknown",
    }
}

fn status_dot(status: Option<&str>) -> Color32 {
    match status {
        Some("running" | "in_progress" | "pending" | "waiting") => theme::BLUE_500,
        Some("failed" | "error") => Color32::from_rgb(251, 65, 74),
        Some("completed") => Color32::from_rgb(0, 188, 125),
        _ => theme::alpha(theme::muted_foreground(), 0.45),
    }
}

/// `resolveMergeBackTargetThreadId`.
fn merge_target(detail: &Detail) -> Option<String> {
    let thread = &detail.thread;
    if thread["lineage"]["relationshipToParent"] != "fork" {
        return None;
    }
    if thread["forkedFrom"]["type"] == "run" {
        return thread["forkedFrom"]["threadId"].as_str().map(str::to_owned);
    }
    thread["lineage"]["parentThreadId"]
        .as_str()
        .map(str::to_owned)
}

/// `resolveLatestMergeBackRun`: the latest run that finished, unless a newer
/// one still works.
fn merge_back_run(detail: &Detail) -> Option<&crate::model::Run> {
    let latest = detail
        .runs
        .iter()
        .filter(|r| matches!(r.status.as_str(), "waiting" | "completed"))
        .max_by_key(|r| r.ordinal)?;
    let newer_active = detail.runs.iter().any(|r| {
        r.ordinal > latest.ordinal
            && matches!(r.status.as_str(), "preparing" | "starting" | "running")
    });
    (!newer_active).then_some(latest)
}

/// `orderWebThreadLineageRows`: the parent first, the merge target next,
/// then the newest; threads without a shell sink.
fn order(rows: &mut [Related], current: &str, merge: Option<&str>) {
    let rank = |row: &Related| {
        if is_parent(&row.edge, current) {
            0
        } else if Some(row.thread_id.as_str()) == merge {
            1
        } else {
            2
        }
    };
    rows.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| {
                let at = |r: &Related| r.shell.as_ref().map(|(_, at)| at.clone());
                match (at(a), at(b)) {
                    (Some(x), Some(y)) => y.cmp(&x),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            })
            .then_with(|| a.thread_id.cmp(&b.thread_id))
    });
}

/// The card's state across frames.
#[derive(Default)]
pub(crate) struct DetailsState {
    /// Rows shown per lineage group, by group.
    visible: HashMap<&'static str, usize>,
    previous_open: bool,
}

/// One drawn lineage row's action.
enum Pick {
    Open(String),
    Merge(String),
    More(&'static str),
    TogglePrevious,
}

impl App {
    /// Whether the header offers the card: an open, non-chat thread.
    pub(crate) fn thread_details_available(&self) -> bool {
        self.current_thread()
            .is_some_and(|thread| !self.is_chat_thread(&thread))
    }

    /// The card under the header's toggle at `anchor`.
    pub(crate) fn thread_details_card(&mut self, ui: &mut Ui, anchor: Rect) {
        if self.open_menu.as_deref() != Some(MENU) {
            return;
        }
        let (Some(thread), Some(detail)) = (self.current_thread(), self.current_detail()) else {
            self.open_menu = None;
            return;
        };
        let mut rows = relationships(&self.state.threads, &detail, &thread.id);
        let merge = merge_target(&detail);
        let merge_run = merge_back_run(&detail).map(|r| r.id.clone());
        order(&mut rows, &thread.id, merge.as_deref());
        // `groupBy`: related threads, working subagents, settled ones.
        let settled = |row: &Related| {
            matches!(
                row.edge.status.as_deref(),
                Some("completed" | "failed" | "error" | "cancelled" | "interrupted" | "idle")
            )
        };
        let child_agent =
            |row: &Related| row.edge.kind == Kind::Subagent && !is_parent(&row.edge, &thread.id);
        let related: Vec<&Related> = rows.iter().filter(|r| !child_agent(r)).collect();
        let active: Vec<&Related> = rows
            .iter()
            .filter(|r| child_agent(r) && !settled(r))
            .collect();
        let previous: Vec<&Related> = rows
            .iter()
            .filter(|r| child_agent(r) && settled(r))
            .collect();
        let running = detail
            .subagents
            .iter()
            .filter(|a| a.child_thread_id.is_none() && a.status == "running")
            .count()
            + active
                .iter()
                .filter(|r| r.edge.status.as_deref() == Some("running"))
                .count();
        let shown = |state: &DetailsState, group: &'static str, len: usize| {
            state
                .visible
                .get(group)
                .copied()
                .unwrap_or(INITIAL)
                .min(len)
        };
        let lineage = !rows.is_empty() || running > 0;
        // Heights: the workspace rows, then the lineage heading, its groups
        // and their "Show more" rows.
        let group_height = |len: usize, visible: usize| {
            (visible as f32 * ROW).min(LIST_MAX) + if len > visible { ROW } else { 0.0 }
        };
        let workspace = 8.0 + 2.0 * ROW + 10.0;
        let mut height = workspace;
        if lineage {
            height += 1.0 + 8.0 + 32.0 + 4.0;
            for (group, list) in [("related", &related), ("active", &active)] {
                height += group_height(list.len(), shown(&self.thread_details, group, list.len()));
            }
            if !previous.is_empty() {
                height += ROW;
                if self.thread_details.previous_open {
                    height += group_height(
                        previous.len(),
                        shown(&self.thread_details, "previous", previous.len()),
                    );
                }
            }
            height += 10.0;
        }
        let screen = ui.ctx().content_rect();
        // The 1px outline around the scrolled content.
        let height = (height + 2.0).min(screen.height() - 104.0).max(ROW);
        let rect = Rect::from_min_size(
            Pos2::new(
                (anchor.right() + GAP - 8.0 - WIDTH).max(screen.left() + 8.0),
                anchor.bottom() + 8.0,
            ),
            Vec2::new(WIDTH, height),
        );
        let mut open = self.open_menu.take();
        let mut pick: Option<Pick> = None;
        let current = thread.id.clone();
        let threads = self.state.threads.clone();
        let branch = thread.branch.clone();
        let worktree = thread.worktree_path.clone();
        let mark_of = |instance: &str| {
            self.provider_driver(instance)
                .and_then(|d| crate::ui::pickers::provider_icon(&d))
        };
        let marks: HashMap<String, Option<(&'static str, Color32)>> = rows
            .iter()
            .map(|r| {
                let instance = threads
                    .iter()
                    .find(|t| t.id == r.thread_id)
                    .map(|t| t.model_selection.instance_id.clone())
                    .unwrap_or_default();
                (r.thread_id.clone(), mark_of(&instance))
            })
            .collect();
        let state = &self.thread_details;
        // `PopoverPopup variant="panel"`: the card is its own rounded surface.
        let area = egui::Area::new(ui.id().with(("popup", MENU)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                ui.painter().rect(
                    rect,
                    CornerRadius::same(24),
                    widgets::menu_surface(),
                    egui::Stroke::new(1.0, widgets::menu_outline()),
                    egui::StrokeKind::Inside,
                );
                let clip = rect.shrink(1.0);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(clip));
                child.set_clip_rect(clip.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .id_salt("thread-details-card")
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        let left = clip.left() + 8.0;
                        let width = clip.width() - 16.0;
                        let mut y = ui.max_rect().top() + 8.0;
                        // The workspace: where the thread runs, and its branch.
                        let (icon, label) = match &worktree {
                            Some(path) => (
                                "folder-git-2",
                                path.rsplit('/')
                                    .find(|s| !s.is_empty())
                                    .unwrap_or(path)
                                    .to_owned(),
                            ),
                            None => ("folder", "Local checkout".to_owned()),
                        };
                        info_row(
                            ui,
                            Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, ROW)),
                            icon,
                            &label,
                        );
                        y += ROW;
                        if let Some(branch) = &branch {
                            info_row(
                                ui,
                                Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, ROW)),
                                "git-branch",
                                branch,
                            );
                        }
                        y += ROW + 10.0;
                        if lineage {
                            // `ThreadDetailsSection`: `border-t`, then the heading.
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    Pos2::new(clip.left(), y),
                                    Vec2::new(clip.width(), 1.0),
                                ),
                                0.0,
                                theme::alpha(theme::border(), 0.65),
                            );
                            y += 1.0 + 8.0;
                            let title = if running > 0 {
                                format!("Lineage · {running} running")
                            } else {
                                "Lineage".to_owned()
                            };
                            widgets::text_in(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(left + 6.0, y),
                                    Vec2::new(width - 12.0, 32.0),
                                ),
                                &title,
                                font(11.0, Weight::Medium),
                                theme::muted_foreground(),
                            );
                            y += 32.0 + 4.0;
                            let draw_group =
                                |ui: &mut Ui,
                                 y: &mut f32,
                                 group: &'static str,
                                 list: &[&Related]| {
                                    let mut pick = None;
                                    let count = shown(state, group, list.len());
                                    for row in list.iter().take(count) {
                                        let rect = Rect::from_min_size(
                                            Pos2::new(left, *y),
                                            Vec2::new(width, ROW),
                                        );
                                        let status = row_status(&threads, row);
                                        let merging =
                                            merge.as_deref() == Some(row.thread_id.as_str());
                                        if let Some(p) = lineage_row(
                                            ui,
                                            rect,
                                            row,
                                            &current,
                                            status.as_deref(),
                                            marks.get(&row.thread_id).copied().flatten(),
                                            merging.then_some(merge_run.is_some()),
                                        ) {
                                            pick = Some(p);
                                        }
                                        *y += ROW;
                                    }
                                    if list.len() > count {
                                        let rect = Rect::from_min_size(
                                            Pos2::new(left, *y),
                                            Vec2::new(width, ROW),
                                        );
                                        if show_more(
                                            ui,
                                            rect,
                                            group,
                                            (list.len() - count).min(PAGE),
                                        ) {
                                            pick = Some(Pick::More(group));
                                        }
                                        *y += ROW;
                                    }
                                    pick
                                };
                            pick = draw_group(ui, &mut y, "related", &related).or(pick.take());
                            pick = draw_group(ui, &mut y, "active", &active).or(pick.take());
                            if !previous.is_empty() {
                                let rect =
                                    Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, ROW));
                                let label = if state.previous_open {
                                    "Previous agents".to_owned()
                                } else {
                                    format!("Previous agents ({})", previous.len())
                                };
                                if section_toggle(ui, rect, &label, state.previous_open) {
                                    pick = Some(Pick::TogglePrevious);
                                }
                                y += ROW;
                                if state.previous_open {
                                    pick = draw_group(ui, &mut y, "previous", &previous)
                                        .or(pick.take());
                                }
                            }
                            y += 10.0;
                        }
                        ui.allocate_rect(
                            Rect::from_min_max(ui.max_rect().min, Pos2::new(clip.right(), y)),
                            Sense::hover(),
                        );
                    });
            });
        let pressed_outside = ui.input(|i| i.pointer.any_pressed())
            && !area.response.contains_pointer()
            && !ui.rect_contains_pointer(anchor);
        if pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = None;
        }
        self.open_menu = open;
        match pick {
            Some(Pick::Open(id)) => {
                self.open_menu = None;
                self.select_thread(Some(id));
            }
            Some(Pick::Merge(target)) => {
                if let Some(run) = merge_run {
                    self.dispatch(json!({
                        "type": "thread.merge_back",
                        "commandId": uuid::Uuid::new_v4().to_string(),
                        "createdBy": "user",
                        "creationSource": "web",
                        "sourceThreadId": current,
                        "targetThreadId": target,
                        "sourcePoint": {"type": "run", "runId": run},
                    }));
                    self.open_menu = None;
                    self.select_thread(Some(target));
                }
            }
            Some(Pick::More(group)) => {
                let visible = self.thread_details.visible.entry(group).or_insert(INITIAL);
                *visible += PAGE;
            }
            Some(Pick::TogglePrevious) => {
                self.thread_details.previous_open = !self.thread_details.previous_open;
            }
            None => {}
        }
    }
}

/// A workspace row: the icon, then the label (`THREAD_DETAILS_PANEL_LOCKED_ROW_CLASS`).
fn info_row(ui: &Ui, rect: Rect, icon: &str, label: &str) {
    let muted = theme::muted_foreground();
    theme::paint_icon(
        ui,
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 10.0 + 8.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        16.0,
        muted,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.left() + 10.0 + 16.0 + 10.0, rect.top()),
            Pos2::new(rect.right() - 10.0, rect.bottom()),
        ),
        label,
        font(13.0, Weight::Medium),
        theme::alpha(theme::foreground(), 0.8),
    );
}

/// A lineage row (`ThreadDetailsControl` with `ThreadRelationshipIcon`); a
/// merge target pairs it with the merge button (`Some(can_merge)`).
fn lineage_row(
    ui: &Ui,
    rect: Rect,
    row: &Related,
    current: &str,
    status: Option<&str>,
    mark: Option<(&'static str, Color32)>,
    merge: Option<bool>,
) -> Option<Pick> {
    let parent = is_parent(&row.edge, current);
    let subagent = row.edge.kind == Kind::Subagent;
    let missing = row.shell.is_none();
    let open_rect = match merge {
        Some(_) => Rect::from_min_max(rect.min, Pos2::new(rect.right() - 32.0, rect.bottom())),
        None => rect,
    };
    let response = ui.interact(
        open_rect,
        ui.id().with(("lineage-row", &row.thread_id)),
        if missing {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    let hover_fill = theme::overlay(0.08);
    if response.hovered() && !missing {
        ui.painter()
            .rect_filled(open_rect, CornerRadius::same(8), hover_fill);
    }
    let muted = theme::muted_foreground();
    let icon_center = Pos2::new(rect.left() + 10.0 + 8.0, rect.center().y);
    match (subagent && !parent, mark) {
        (true, Some((icon, tint))) => theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(icon_center, Vec2::splat(16.0)),
            16.0,
            tint,
        ),
        _ => theme::paint_icon(
            ui,
            if parent {
                "corner-left-up"
            } else if subagent {
                "bot"
            } else {
                "git-fork"
            },
            Rect::from_center_size(icon_center, Vec2::splat(16.0)),
            16.0,
            muted,
        ),
    }
    let dot = icon_center + Vec2::new(6.0, 6.0);
    ui.painter()
        .circle_filled(dot, 5.0, widgets::menu_surface());
    ui.painter().circle_filled(dot, 3.0, status_dot(status));
    let failed = matches!(status, Some("failed" | "error"));
    let label = status_label(status);
    let label_font = font(11.0, Weight::Regular);
    let mut text_right = open_rect.right() - 10.0;
    if merge.is_none() {
        let width = widgets::text_width(ui, label, label_font.clone());
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(text_right - width, rect.top()),
                Pos2::new(text_right + 1.0, rect.bottom()),
            ),
            label,
            label_font,
            if failed {
                Color32::from_rgb(251, 65, 74)
            } else {
                muted
            },
        );
        text_right -= width + 10.0;
    }
    let title = match &row.shell {
        Some((title, _)) if subagent => crate::model::subagent_display_title(title),
        Some((title, _)) => title.clone(),
        None => row.thread_id.clone(),
    };
    widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(rect.left() + 10.0 + 16.0 + 10.0, rect.top()),
            Pos2::new(text_right, rect.bottom()),
        ),
        &title,
        font(13.0, Weight::Medium),
        theme::alpha(theme::foreground(), 0.85),
    );
    let relationship = match (row.edge.kind, parent) {
        (Kind::Transfer, _) => "context transfer",
        (Kind::Subagent, true) => "parent agent",
        (Kind::Subagent, false) => "subagent",
        (Kind::Fork, true) => "parent thread",
        (Kind::Fork, false) => "fork",
    };
    let hint = if missing {
        "This related thread is unavailable".to_owned()
    } else {
        format!("Open {relationship} in this chat")
    };
    let clicked = response
        .on_hover_cursor(if missing {
            CursorIcon::Default
        } else {
            CursorIcon::PointingHand
        })
        .on_hover_text(hint)
        .clicked();
    if clicked && !missing {
        return Some(Pick::Open(row.thread_id.clone()));
    }
    if let Some(can_merge) = merge {
        // `THREAD_DETAILS_PANEL_SPLIT_SEPARATOR_CLASS`, then the merge button.
        ui.painter().rect_filled(
            Rect::from_center_size(
                Pos2::new(open_rect.right(), rect.center().y),
                Vec2::new(1.0, 16.0),
            ),
            0.0,
            theme::alpha(theme::border(), 0.65),
        );
        let button = Rect::from_min_max(Pos2::new(rect.right() - 32.0, rect.top()), rect.max);
        let response = ui.interact(
            button,
            ui.id().with(("merge-back", &row.thread_id)),
            if can_merge {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if response.hovered() && can_merge {
            ui.painter()
                .rect_filled(button, CornerRadius::same(8), hover_fill);
        }
        theme::paint_icon(
            ui,
            "git-merge",
            Rect::from_center_size(button.center(), Vec2::splat(12.0)),
            12.0,
            theme::alpha(muted, if can_merge { 1.0 } else { 0.64 }),
        );
        let tip = if !can_merge {
            "Complete a run in this fork before merging it back".to_owned()
        } else if missing {
            "Merge this conversation back into its source".to_owned()
        } else {
            format!("Merge this conversation back into {title}")
        };
        if response.on_hover_text(tip).clicked() && can_merge {
            return Some(Pick::Merge(row.thread_id.clone()));
        }
    }
    None
}

/// "Show N more" (`ThreadLineageRowList`).
fn show_more(ui: &Ui, rect: Rect, group: &str, count: usize) -> bool {
    let response = ui
        .interact(rect, ui.id().with(("lineage-more", group)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    let color = if response.hovered() {
        theme::alpha(theme::foreground(), 0.8)
    } else {
        theme::alpha(theme::muted_foreground(), 0.7)
    };
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::overlay(0.075));
    }
    theme::paint_icon(
        ui,
        "plus",
        Rect::from_center_size(
            Pos2::new(rect.left() + 10.0 + 8.0, rect.center().y),
            Vec2::splat(16.0),
        ),
        16.0,
        color,
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 36.0, rect.top()), rect.max),
        &format!("Show {count} more"),
        font(14.0, Weight::Medium),
        color,
    );
    response.clicked()
}

/// `CollapsibleSectionHeader variant="panel"`.
fn section_toggle(ui: &Ui, rect: Rect, label: &str, open: bool) -> bool {
    let response = ui
        .interact(
            rect,
            ui.id().with(("lineage-previous", label)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    let muted = theme::muted_foreground();
    let width = widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 6.0, rect.top()), rect.max),
        label,
        font(11.0, Weight::Medium),
        if response.hovered() {
            theme::foreground()
        } else {
            muted
        },
    );
    theme::paint_icon(
        ui,
        if open {
            "chevron-down"
        } else {
            "chevron-right"
        },
        Rect::from_center_size(
            Pos2::new(rect.left() + 6.0 + width + 10.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        12.0,
        muted,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, parent: Option<(&str, &str)>, at: &str) -> std::sync::Arc<Thread> {
        let mut raw = json!({"id": id, "projectId": "p", "title": id, "createdAt": at,
            "updatedAt": at, "status": "completed", "modelSelection": {"instanceId": "codex", "model": "m"},
            "lineage": {"parentThreadId": null, "relationshipToParent": null, "rootThreadId": id}});
        if let Some((parent, kind)) = parent {
            raw["lineage"]["parentThreadId"] = json!(parent);
            raw["lineage"]["relationshipToParent"] = json!(kind);
        }
        std::sync::Arc::new(Thread::from_shell(&raw).unwrap())
    }

    #[test]
    fn lineage_lists_the_parent_first_then_the_newest() {
        let threads = vec![
            thread("root", None, "2026-01-01T00:00:00.000Z"),
            thread("me", Some(("root", "fork")), "2026-01-02T00:00:00.000Z"),
            thread("old", Some(("me", "fork")), "2026-01-03T00:00:00.000Z"),
            thread("new", Some(("me", "subagent")), "2026-01-04T00:00:00.000Z"),
        ];
        let detail = Detail::default();
        let mut rows = relationships(&threads, &detail, "me");
        order(&mut rows, "me", Some("root"));
        let ids: Vec<&str> = rows.iter().map(|r| r.thread_id.as_str()).collect();
        assert_eq!(ids, ["root", "new", "old"]);
        assert!(is_parent(&rows[0].edge, "me"));
        assert_eq!(rows[1].edge.kind, Kind::Subagent);
    }

    #[test]
    fn merge_back_waits_for_a_finished_run() {
        let run = |ordinal: u64, status: &str| {
            json!({"id": format!("r{ordinal}"), "threadId": "me", "ordinal": ordinal,
                "status": status, "userMessageId": "m", "requestedAt": "1"})
        };
        let detail = |runs: Value| -> Detail {
            serde_json::from_value(json!({"thread": {"id": "me"}, "runs": runs})).unwrap()
        };
        assert_eq!(
            merge_back_run(&detail(json!([run(1, "completed"), run(2, "failed")])))
                .map(|r| r.id.as_str()),
            Some("r1")
        );
        assert!(merge_back_run(&detail(json!([run(1, "completed"), run(2, "running")]))).is_none());
    }
}
