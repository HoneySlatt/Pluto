//! The composer's notices (`ComposerBannerStack.tsx`, `ComposerBanner.tsx`,
//! `ChatView.tsx`'s `composerBannerItems`): the first notice attached above
//! the composer, the others behind it as a 12px peek that opens them on
//! hover. Ordered like upstream's `bannerPriority`: activity first, then
//! errors and warnings, then the other notices.
//!
//! Items: the server reconnecting after 2 s (Reconnect, and after 20 s
//! Change server, the single environment's Disconnect), a server older than
//! the one this client was built for (`PINNED_SERVER`: Pluto's server comes
//! from Nix, so there is no update button), subagents waiting on the user
//! (Open approval or Open question), a usage limit with its reset time
//! (Resume at reset, Snooze until reset), a provider goal (Resume, Clear),
//! background work left running (Stop), and a local checkout whose branch
//! changed since the thread ran there (Restore branch, once the composer
//! holds a draft), and a project being cloned (Cancel; failed or cancelled,
//! Remove project and Retry). Not ported yet: the usage-limits command and
//! feedback notices.

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `--chat-composer-drawer-inset`.
const INSET: f32 = 22.0;
/// The pluto-server version this client was built and checked against.
pub(crate) const PINNED_SERVER: &str = "0.1.0";

/// A notice's row (`density="comfortable"`: 24px at sm and 5px above and
/// below).
const ROW: f32 = 24.0;
const PAD: f32 = 5.0;
/// The peek of the notices behind the front one.
const PEEK: f32 = 12.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Variant {
    Default,
    Info,
    Warning,
}

#[derive(Clone, Copy, PartialEq)]
enum Priority {
    Activity,
    Urgent,
    Notice,
}

#[derive(Clone)]
pub(crate) enum NoticeAction {
    ChangeServer,
    Reconnect,
    OpenThread(String),
    RestoreBranch(String),
    /// Sends a command as its own turn (`/goal resume`), leaving the draft.
    SendCommand(&'static str),
    StopBackground,
    /// Turns the usage limit's auto-resume (`true`) or snooze on or off.
    LimitRecovery {
        auto_resume: bool,
    },
    /// A project clone's Cancel, Retry or Remove project.
    Clone {
        project: String,
        action: &'static str,
    },
}

#[derive(Clone)]
pub(crate) struct Notice {
    id: String,
    variant: Variant,
    priority: Priority,
    icon: &'static str,
    title: String,
    description: Option<String>,
    /// The title's hover text.
    tooltip: Option<String>,
    actions: Vec<(String, NoticeAction)>,
    dismissable: bool,
    dismiss_label: &'static str,
}

impl Notice {
    fn new(
        id: String,
        variant: Variant,
        priority: Priority,
        icon: &'static str,
        title: String,
    ) -> Self {
        Self {
            id,
            variant,
            priority,
            icon,
            title,
            description: None,
            tooltip: None,
            actions: Vec::new(),
            dismissable: false,
            dismiss_label: "Dismiss warning",
        }
    }

    /// `bannerPriority`.
    fn rank(&self) -> u8 {
        if self.priority == Priority::Activity {
            0
        } else if self.priority == Priority::Urgent || self.variant == Variant::Warning {
            1
        } else {
            2
        }
    }
}

/// `presentProviderGoal`: the title and, as description, the objective.
pub(crate) fn goal_title(goal: &serde_json::Value, working: bool) -> String {
    let status = match goal["status"].as_str().unwrap_or("active") {
        "active" if working => "Pursuing goal",
        "active" => "Goal set",
        "paused" => "Goal paused",
        "blocked" => "Goal blocked",
        "usage_limited" => "Goal hit a usage limit",
        "budget_limited" => "Goal reached its token budget",
        "complete" => "Goal complete",
        _ => "Pursuing goal",
    };
    let tokens = |n: u64| match n {
        0..1000 => n.to_string(),
        1000..1_000_000 => format!("{}k", (n as f64 / 1000.0).round()),
        _ => {
            let m = format!("{:.1}", n as f64 / 1_000_000.0);
            format!("{}m", m.trim_end_matches(".0"))
        }
    };
    let mut usage = Vec::new();
    if let Some(used) = goal["tokensUsed"].as_u64().filter(|n| *n > 0) {
        usage.push(match goal["tokenBudget"].as_u64() {
            Some(budget) => format!("{} / {} tokens", tokens(used), tokens(budget)),
            None => format!("{} tokens", tokens(used)),
        });
    }
    if let Some(seconds) = goal["timeUsedSeconds"].as_u64().filter(|s| *s >= 60) {
        usage.push(if seconds >= 3600 {
            format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
        } else {
            format!("{}m", seconds / 60)
        });
    }
    if let Some(checks) = goal["checks"].as_u64().filter(|n| *n > 0) {
        usage.push(format!(
            "{checks} check{}",
            if checks == 1 { "" } else { "s" }
        ));
    }
    if usage.is_empty() {
        status.to_owned()
    } else {
        format!("{status} · {}", usage.join(" · "))
    }
}

/// `presentPendingBackgroundWork`: the title, the description and whether
/// it waits on something (not only commands).
pub(crate) fn background_work(
    tasks: &[serde_json::Value],
) -> Option<(String, Option<String>, bool)> {
    const KINDS: [(&str, &str); 4] = [
        ("subagent", "subagent"),
        ("command", "command"),
        ("monitor", "monitor"),
        ("background_task", "background task"),
    ];
    let mut tasks: Vec<&serde_json::Value> = tasks.iter().collect();
    if tasks.is_empty() {
        return None;
    }
    let rank = |t: &serde_json::Value| KINDS.iter().position(|(k, _)| t["kind"] == *k).unwrap_or(3);
    tasks.sort_by_key(|t| rank(t));
    let noun = |t: &serde_json::Value| KINDS[rank(t)].1;
    let label = |t: &serde_json::Value| {
        t["description"]
            .as_str()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_owned)
    };
    let waiting = tasks.iter().any(|t| t["kind"] != "command");
    let verb = if waiting { "Waiting on" } else { "Running" };
    let title = if let [task] = tasks.as_slice() {
        match (label(task), waiting) {
            (Some(label), true) if task["kind"] == "monitor" => label,
            (Some(label), true) => format!("Waiting on {} {label}", noun(task)),
            (None, true) => format!("Waiting on a {}", noun(task)),
            (Some(label), false) => format!("Running: {label}"),
            (None, false) => format!("Running a {}", noun(task)),
        }
    } else {
        let counts: Vec<String> = KINDS
            .iter()
            .filter_map(|(kind, noun)| {
                let n = tasks.iter().filter(|t| t["kind"] == *kind).count();
                (n > 0).then(|| format!("{n} {noun}{}", if n == 1 { "" } else { "s" }))
            })
            .collect();
        let list = match counts.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => counts.join(""),
        };
        format!("{verb} {list}")
    };
    let single_plain = tasks.len() == 1 && tasks[0]["childThreadId"].as_str().is_none();
    let description = (!single_plain).then(|| {
        tasks
            .iter()
            .map(|t| label(t).unwrap_or_else(|| noun(t).to_owned()))
            .collect::<Vec<_>>()
            .join(", ")
    });
    Some((title, description, waiting))
}

/// Whether `server` is behind `client` (`resolveVersionMismatch`): their
/// `major.minor.patch` compared, any suffix dropped; versions that do not
/// parse differ when they are not the same text.
pub(crate) fn server_behind(server: &str, client: &str) -> bool {
    let core = |v: &str| -> Option<(u64, u64, u64)> {
        let v = v.trim().trim_start_matches('v');
        let v = v.split(['-', '+']).next()?;
        let mut parts = v.split('.').map(|p| p.parse::<u64>().ok());
        Some((parts.next()??, parts.next()??, parts.next()??))
    };
    let (server, client) = (server.trim(), client.trim());
    if server.is_empty() || client.is_empty() {
        return false;
    }
    match (core(server), core(client)) {
        (Some(s), Some(c)) => s < c,
        _ => server != client,
    }
}

fn colors(variant: Variant) -> (Color32, Color32) {
    match variant {
        Variant::Warning => (
            theme::stock_or(Color32::from_rgb(35, 27, 15), |p| {
                theme::mix(p.background, p.warning, 0.1)
            }),
            theme::stock_or(Color32::from_rgb(96, 63, 10), |p| {
                theme::mix(p.background, p.warning, 0.35)
            }),
        ),
        Variant::Info | Variant::Default => (theme::shade(16), theme::shade(28)),
    }
}

impl App {
    /// Subagents below the open thread waiting on the user
    /// (`childThreadInputsAtom`): their ids, titles and whether each asks
    /// for an approval.
    fn child_inputs(&self, parent: &str) -> Vec<(String, String, bool)> {
        let by_id: std::collections::HashMap<&str, &Arc<Thread>> = self
            .state
            .threads
            .iter()
            .map(|t| (t.id.as_str(), t))
            .collect();
        self.state
            .threads
            .iter()
            .filter(|t| t.is_subagent() && !t.is_provider_native_subagent() && t.child_run_live())
            .filter_map(|t| {
                let kind = t.pending_runtime_request.as_ref()?.kind.as_str();
                if kind == "auth_refresh" {
                    return None;
                }
                // Up the subagent lineage to the open thread.
                let mut current = t.lineage.parent_thread_id.as_deref();
                let mut steps = 0;
                while let Some(id) = current {
                    if id == parent {
                        return Some((t.id.clone(), t.title.clone(), kind != "user_input"));
                    }
                    steps += 1;
                    if steps > 64 {
                        return None;
                    }
                    current = by_id
                        .get(id)
                        .filter(|p| p.is_subagent())
                        .and_then(|p| p.lineage.parent_thread_id.as_deref());
                }
                None
            })
            .collect()
    }

    /// The notices of the open composer, front first.
    pub(crate) fn composer_notices(&self, draft: bool) -> Vec<Notice> {
        let mut notices = Vec::new();
        let thread = if draft { None } else { self.current_thread() };
        let working = thread.as_ref().is_some_and(|t| t.is_running());
        // A usage limit: the reset time it waits for, and what to do then.
        if let Some(thread) = &thread
            && let Some(runtime) = &thread.runtime
            && runtime.status == "failed"
            && runtime.last_error_class.as_deref() == Some("usage_limit")
            && let Some(run) = &thread.latest_run
        {
            let reset = runtime.usage_limit_reset_at.clone();
            let mut notice = Notice::new(
                format!("usage-limit-recovery:{}", run.run_id),
                Variant::Warning,
                Priority::Urgent,
                "gauge",
                "Usage limit reached".into(),
            );
            notice.description = Some(
                match reset.as_deref().and_then(crate::timeline::parse_time) {
                    Some(at) => format!(
                        "Resets {}",
                        at.with_timezone(&chrono::Local).format("%x, %X")
                    ),
                    None => "Reset time unavailable; retry manually".into(),
                },
            );
            let stopped = run
                .completed_at
                .as_deref()
                .unwrap_or(&thread.updated_at)
                .to_owned();
            let later = reset
                .as_deref()
                .and_then(crate::timeline::parse_time)
                .zip(crate::timeline::parse_time(&stopped))
                .is_some_and(|(reset, stopped)| reset > stopped);
            if later && let Some(reset) = &reset {
                let recovery = thread.limit_recovery.as_ref();
                let same = recovery.is_some_and(|r| {
                    r["runId"] == run.run_id.as_str() && r["resetAt"] == reset.as_str()
                });
                let scheduled = same && recovery.is_some_and(|r| r["autoResume"] == true);
                let snoozed = same
                    && recovery.is_some_and(|r| r["snooze"] == true)
                    && thread.snoozed_until.as_deref() == Some(reset.as_str());
                let busy = self.notice_busy.contains(&notice.id);
                notice.actions.push((
                    if busy {
                        "Saving...".into()
                    } else if scheduled {
                        "Cancel auto-resume".into()
                    } else {
                        "Resume at reset".into()
                    },
                    NoticeAction::LimitRecovery { auto_resume: true },
                ));
                let passed = crate::timeline::parse_time(reset).is_some_and(|at| at <= self.now);
                if !snoozed && !passed {
                    notice.actions.push((
                        if busy {
                            "Saving..."
                        } else {
                            "Snooze until reset"
                        }
                        .into(),
                        NoticeAction::LimitRecovery { auto_resume: false },
                    ));
                }
            }
            notices.push(notice);
        }
        if let Some((host, _)) = self.reconnecting_host()
            && let Some(since) = self.reconnect.as_ref().map(|r| r.since)
            && since.elapsed() >= std::time::Duration::from_secs(2)
        {
            let mut notice = Notice::new(
                "environment-unavailable".into(),
                Variant::Warning,
                Priority::Notice,
                "wifi-off",
                format!("{host} is reconnecting"),
            );
            notice
                .actions
                .push(("Reconnect".into(), NoticeAction::Reconnect));
            if since.elapsed() >= std::time::Duration::from_secs(20) {
                notice
                    .actions
                    .push(("Change server".into(), NoticeAction::ChangeServer));
            }
            notices.push(notice);
        }
        // A server older than the one this client expects.
        if let Some(server) = self.state.config["environment"]["serverVersion"].as_str()
            && server_behind(server, PINNED_SERVER)
        {
            let environment = self.state.config["environment"]["environmentId"]
                .as_str()
                .unwrap_or_default();
            let mut notice = Notice::new(
                format!("server-version:{environment}:{PINNED_SERVER}:{server}"),
                Variant::Default,
                Priority::Notice,
                "download",
                "Server update available".into(),
            );
            notice.tooltip = Some(format!("server {server} → {PINNED_SERVER}"));
            notice.description = Some("Update pluto-server to stay in sync".into());
            notice.dismissable = true;
            notice.dismiss_label = "Dismiss update notice";
            notices.push(notice);
        }
        // The open project's clone (`ProjectCloneSnapshot`).
        let project = thread
            .as_ref()
            .map(|t| t.project_id.clone())
            .or_else(|| self.draft_project().map(|p| p.id));
        if let Some(snapshot) = project
            .as_ref()
            .and_then(|id| self.clones.iter().find(|c| c["projectId"] == id.as_str()))
            && let Some(notice) = clone_notice(snapshot)
        {
            notices.push(notice);
        }
        if let Some(thread) = &thread {
            let children = self.child_inputs(&thread.id);
            let parent_waits = thread.has_pending_approvals || thread.has_pending_user_input;
            if let Some((first, _, approval)) = children.first().cloned()
                && !parent_waits
            {
                let approvals = children.iter().filter(|c| c.2).count();
                let need = match approvals {
                    0 => "input",
                    n if n == children.len() => "approval",
                    _ => "you",
                };
                let mut notice = Notice::new(
                    format!("child-input:{first}"),
                    Variant::Info,
                    Priority::Activity,
                    if approval {
                        "shield-question"
                    } else {
                        "message-circle"
                    },
                    if children.len() == 1 {
                        format!("Subagent needs {need}")
                    } else {
                        format!("{} subagents need {need}", children.len())
                    },
                );
                notice.description = Some(
                    children
                        .iter()
                        .map(|c| c.1.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                notice.actions.push((
                    if approval {
                        "Open approval"
                    } else {
                        "Open question"
                    }
                    .into(),
                    NoticeAction::OpenThread(first),
                ));
                notices.push(notice);
            }
            // A provider goal: where it stands, and resuming or clearing it.
            if let Some(goal) = &thread.goal {
                let mut notice = Notice::new(
                    format!("goal:{}", thread.id),
                    Variant::Default,
                    Priority::Activity,
                    "target",
                    goal_title(goal, working),
                );
                notice.description = goal["objective"].as_str().map(str::to_owned);
                if !working {
                    let status = goal["status"].as_str().unwrap_or("active");
                    if status != "active" && status != "complete" {
                        notice
                            .actions
                            .push(("Resume".into(), NoticeAction::SendCommand("/goal resume")));
                    }
                    notice
                        .actions
                        .push(("Clear".into(), NoticeAction::SendCommand("/goal clear")));
                }
                notices.push(notice);
            }
            // Work a run left going: what it waits on, and Stop.
            if !working
                && let Some((title, description, waiting)) =
                    background_work(&thread.pending_background_tasks)
            {
                let id = format!("background-work:{}", thread.id);
                let stopping = self.notice_busy.contains(&id);
                let mut notice = Notice::new(
                    id,
                    Variant::Default,
                    Priority::Activity,
                    if waiting { "__dot-pulse" } else { "__dot" },
                    title,
                );
                notice.description = description;
                notice.actions.push((
                    if stopping { "Stopping..." } else { "Stop" }.into(),
                    NoticeAction::StopBackground,
                ));
                notices.push(notice);
            }
        }
        if let Some(thread) = &thread
            && let Some(was) = self.branch_mismatch(thread)
        {
            let key = format!("branch-mismatch:{}:{was}", thread.id);
            // Shown once the composer holds a draft, then while it lasts.
            let has_draft = !self.composer.trim().is_empty()
                || !self.composer_images.is_empty()
                || !self.composer_files.is_empty();
            let revealed = self.revealed_notices.borrow().contains(&key);
            if has_draft || revealed {
                self.revealed_notices.borrow_mut().insert(key.clone());
                let current = self.workspace_branch().unwrap_or_default();
                let restoring = self.restoring_branch.as_deref() == Some(thread.id.as_str());
                let mut notice = Notice::new(
                    key,
                    Variant::Info,
                    Priority::Notice,
                    "git-branch",
                    format!("Branch changed — was {was}"),
                );
                notice.tooltip = Some(format!(
                    "This thread last ran on {was}. Sending will continue on {current}."
                ));
                notice.actions.push((
                    if restoring {
                        "Restoring..."
                    } else {
                        "Restore branch"
                    }
                    .into(),
                    NoticeAction::RestoreBranch(was),
                ));
                notice.dismissable = true;
                notice.dismiss_label = "Dismiss branch change notice";
                notices.push(notice);
            }
        }
        notices.retain(|n| !self.dismissed_notices.contains(&n.id));
        notices.sort_by_key(Notice::rank);
        notices
    }

    /// `localCheckoutBranchMismatch`: a thread on the project's own
    /// checkout whose branch is not the checkout's current one.
    fn branch_mismatch(&self, thread: &Thread) -> Option<String> {
        if thread.worktree_path.is_some() {
            return None;
        }
        let was = thread.branch.clone()?;
        let current = self.workspace_branch()?;
        (current != was).then_some(was)
    }

    /// The height the notices take above the other banners.
    pub(crate) fn notices_height(notices: &[Notice]) -> f32 {
        match notices.len() {
            0 => 0.0,
            1 => PAD + ROW + PAD,
            _ => PAD + ROW + PAD + PEEK,
        }
    }

    /// Draws the notices with their bottom at `bottom` (the top of what sits
    /// under them), over the composer `surface`'s width.
    pub(crate) fn paint_notices(
        &mut self,
        ui: &mut Ui,
        surface: Rect,
        bottom: f32,
        notices: &[Notice],
    ) {
        let Some(front) = notices.first() else {
            return;
        };
        let left = surface.left() + INSET;
        let right = surface.right() - INSET;
        let front_rect = Rect::from_min_max(
            Pos2::new(left, bottom - (PAD + ROW + PAD)),
            Pos2::new(right, bottom + crate::ui::requests_panel::OVERLAP),
        );
        // The others peek behind it; hovering the peek lays them out above.
        let rest = &notices[1..];
        let mut expanded = false;
        if !rest.is_empty() {
            let peek = Rect::from_min_max(
                Pos2::new(left + front_rect.width() * 0.02, front_rect.top() - PEEK),
                Pos2::new(right - front_rect.width() * 0.02, front_rect.top() + 4.0),
            );
            let (fill, outline) = colors(rest[0].variant);
            ui.painter().rect(
                peek,
                CornerRadius {
                    nw: 16,
                    ne: 16,
                    sw: 0,
                    se: 0,
                },
                fill,
                Stroke::new(1.0, outline),
                StrokeKind::Inside,
            );
            let stack_height = rest.len() as f32 * (PAD + ROW + PAD + 8.0);
            let stack = Rect::from_min_max(
                Pos2::new(left, peek.top() - stack_height),
                Pos2::new(right, peek.bottom()),
            );
            let state = ui.id().with("notice-stack-open");
            let was_open = ui.data(|d| d.get_temp::<bool>(state)).unwrap_or(false);
            let hovered =
                ui.rect_contains_pointer(peek) || (was_open && ui.rect_contains_pointer(stack));
            ui.data_mut(|d| d.insert_temp(state, hovered));
            expanded = hovered;
            if expanded {
                let mut top = stack.top();
                for notice in rest {
                    let rect = Rect::from_min_size(
                        Pos2::new(left, top),
                        Vec2::new(right - left, PAD + ROW + PAD),
                    );
                    let (fill, outline) = colors(notice.variant);
                    ui.painter().rect(
                        rect,
                        CornerRadius::same(16),
                        fill,
                        Stroke::new(1.0, outline),
                        StrokeKind::Inside,
                    );
                    self.notice_row(ui, notice, rect);
                    top = rect.bottom() + 8.0;
                }
            }
        }
        let (fill, outline) = colors(front.variant);
        ui.painter().rect(
            front_rect,
            CornerRadius {
                nw: 16,
                ne: 16,
                sw: 0,
                se: 0,
            },
            fill,
            Stroke::new(1.0, outline),
            StrokeKind::Inside,
        );
        let row = Rect::from_min_size(
            front_rect.min,
            Vec2::new(front_rect.width(), PAD + ROW + PAD),
        );
        let _ = expanded;
        self.notice_row(ui, front, row);
    }

    /// One notice's row: icon, title, description, actions, dismiss.
    fn notice_row(&mut self, ui: &mut Ui, notice: &Notice, rect: Rect) {
        let painter = ui.painter().clone();
        let row = Rect::from_min_size(
            Pos2::new(rect.left() + 4.0, rect.top() + PAD),
            Vec2::new(rect.width() - 8.0, ROW),
        );
        // Icons are muted (`[&>svg]:size-3`); running work is a dot,
        // pulsing while it waits.
        let tone = theme::muted_foreground();
        let icon_center = Pos2::new(row.left() + 12.0, row.center().y);
        match notice.icon {
            "__dot" | "__dot-pulse" => {
                let pulse = notice.icon == "__dot-pulse";
                let alpha = if pulse {
                    let t = ui.input(|i| i.time) % 2.0;
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(330));
                    // `steps(6)` from 1 to 0.5 and back over 2 s.
                    let step = (t / 2.0 * 6.0).floor() / 6.0;
                    1.0 - 0.5 * (1.0 - (step * 2.0 - 1.0).abs()) as f32
                } else {
                    1.0
                };
                painter.circle_filled(icon_center, 3.0, theme::alpha(theme::foreground(), alpha));
            }
            icon => theme::paint_icon(
                ui,
                icon,
                Rect::from_center_size(icon_center, Vec2::splat(12.0)),
                12.0,
                tone,
            ),
        }
        // Actions from the right: dismiss, then the buttons.
        let mut right = row.right();
        let mut clicked: Option<NoticeAction> = None;
        let mut dismissed = false;
        if notice.dismissable {
            let close =
                Rect::from_min_size(Pos2::new(right - 24.0, row.top() + 2.0), Vec2::splat(24.0));
            let response = ui
                .interact(
                    close,
                    ui.id().with(("notice-dismiss", &notice.id)),
                    Sense::click(),
                )
                .on_hover_text(notice.dismiss_label);
            if response.hovered() {
                painter.rect_filled(close, CornerRadius::same(6), theme::accent());
            }
            theme::paint_icon(
                ui,
                "x",
                Rect::from_center_size(close.center(), Vec2::splat(14.0)),
                14.0,
                theme::muted_foreground(),
            );
            dismissed = response.clicked();
            right = close.left() - 2.0;
        }
        let label_font = font(12.0, Weight::Medium);
        for (label, action) in notice.actions.iter().rev() {
            let width = widgets::text_width(ui, label, label_font.clone()) + 16.0;
            let button = Rect::from_min_size(
                Pos2::new(right - width, row.top() + 2.0),
                Vec2::new(width, 24.0),
            );
            let response = ui
                .interact(
                    button,
                    ui.id().with(("notice-action", &notice.id, label)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.hovered() {
                painter.rect_filled(button, CornerRadius::same(6), theme::accent());
            }
            let galley = widgets::galley(
                ui,
                label,
                label_font.clone(),
                theme::foreground(),
                16.0,
                f32::INFINITY,
            );
            painter.galley(
                Pos2::new(button.left() + 8.0, button.top() + 4.0),
                galley,
                theme::foreground(),
            );
            if response.clicked() {
                clicked = Some(action.clone());
            }
            right = button.left() - 2.0;
        }
        // The title, then the muted description, cut to the room left.
        let text_left = row.left() + 28.0;
        let room = (right - 8.0 - text_left).max(0.0);
        let title_font = font(12.0, Weight::Medium);
        let title = widgets::galley_truncated(
            ui,
            &notice.title,
            title_font.clone(),
            theme::foreground(),
            16.0,
            room,
        );
        let title_width = title.size().x;
        let title_rect =
            Rect::from_min_size(Pos2::new(text_left, row.center().y - 8.0), title.size());
        painter.galley(title_rect.min, title, theme::foreground());
        if let Some(tooltip) = &notice.tooltip {
            ui.interact(
                title_rect,
                ui.id().with(("notice-title", &notice.id)),
                Sense::hover(),
            )
            .on_hover_text(tooltip);
        }
        if let Some(description) = &notice.description {
            let left = text_left + title_width + 6.0;
            let room = (right - 8.0 - left).max(0.0);
            if room > 24.0 {
                let galley = widgets::galley_truncated(
                    ui,
                    description,
                    font(12.0, Weight::Regular),
                    theme::muted_foreground(),
                    16.0,
                    room,
                );
                let clipped = galley.size().x >= room - 1.0;
                let rect =
                    Rect::from_min_size(Pos2::new(left, row.center().y - 8.0), galley.size());
                painter.galley(rect.min, galley, theme::muted_foreground());
                if clipped {
                    ui.interact(
                        rect,
                        ui.id().with(("notice-description", &notice.id)),
                        Sense::hover(),
                    )
                    .on_hover_text(description);
                }
            }
        }
        if dismissed {
            self.dismissed_notices.insert(notice.id.clone());
            if notice.id.starts_with("server-version:") {
                crate::app::save_ui_state(self);
            }
        }
        if let Some(action) = clicked {
            self.notice_action(notice, action);
        }
    }

    /// A project clone's Cancel, Retry or Remove project.
    pub(crate) fn clone_action(&mut self, project: &str, action: &str) {
        match action {
            "remove" => {
                let command = json!({
                    "type": "project.delete",
                    "commandId": uuid::Uuid::new_v4().to_string(),
                    "projectId": project,
                });
                self.dispatch_titled(command, "Failed to remove project");
            }
            kind => {
                let method = if kind == "cancel" {
                    "projectClone.cancel"
                } else {
                    "projectClone.retry"
                };
                self.send_request(
                    format!("clone-action:{kind}"),
                    method,
                    json!({"projectId": project}),
                );
            }
        }
    }

    fn notice_action(&mut self, notice: &Notice, action: NoticeAction) {
        let Some(thread) = self.current_thread() else {
            if let NoticeAction::ChangeServer = action {
                self.change_server();
            } else if let NoticeAction::Reconnect = action {
                self.reconnect_now();
            } else if let NoticeAction::Clone { project, action } = action {
                self.clone_action(&project, action);
            }
            return;
        };
        match action {
            NoticeAction::ChangeServer => self.change_server(),
            NoticeAction::Reconnect => self.reconnect_now(),
            NoticeAction::Clone { project, action } => self.clone_action(&project, action),
            NoticeAction::OpenThread(id) => self.select_thread(Some(id)),
            NoticeAction::RestoreBranch(branch) => {
                if self.restoring_branch.is_some() {
                    return;
                }
                if let Some(cwd) = self.workspace_cwd() {
                    self.restoring_branch = Some(thread.id.clone());
                    let key = format!("restore-branch:{}:{branch}", thread.id);
                    self.send_request(key, "vcs.switchRef", json!({"cwd": cwd, "refName": branch}));
                }
            }
            NoticeAction::SendCommand(text) => {
                let model = self.composer_model_json(false);
                let runtime = self.composer_runtime_mode(false);
                let interaction = self.composer_interaction_mode(false);
                let command =
                    crate::ui::composer::message_dispatch(&thread.id, text, &model, "auto");
                let before = self.mode_commands(&thread, &runtime, &interaction);
                self.start_turn(crate::ui::composer::TurnSend {
                    thread_id: thread.id.clone(),
                    before,
                    message: command,
                    launch: false,
                    uploads: Vec::new(),
                });
            }
            NoticeAction::StopBackground => {
                if self.notice_busy.contains(&notice.id) {
                    return;
                }
                let run = thread
                    .runtime
                    .as_ref()
                    .and_then(|r| r.active_run_id.clone())
                    .or_else(|| thread.latest_run.as_ref().map(|r| r.run_id.clone()));
                if let Some(run) = run {
                    let mut command = crate::model::command("run.interrupt", &thread.id);
                    command["runId"] = json!(run);
                    command["holdQueue"] = json!(true);
                    self.dispatch_busy(command, &notice.id, "Failed to stop background work.");
                }
                // Pull request watches stop with it.
                for link in &thread.pull_requests {
                    if link["watch"].is_null() || link["watch"] == false {
                        continue;
                    }
                    let mut command =
                        crate::model::command("thread.pull-request.watch", &thread.id);
                    command["host"] = link["host"].clone();
                    command["repository"] = link["repository"].clone();
                    command["number"] = link["number"].clone();
                    command["watching"] = json!(false);
                    self.dispatch(command);
                }
            }
            NoticeAction::LimitRecovery { auto_resume } => {
                let (Some(run), Some(reset)) = (
                    thread.latest_run.as_ref().map(|r| r.run_id.clone()),
                    thread
                        .runtime
                        .as_ref()
                        .and_then(|r| r.usage_limit_reset_at.clone()),
                ) else {
                    return;
                };
                if self.notice_busy.contains(&notice.id) {
                    return;
                }
                let recovery = thread.limit_recovery.as_ref();
                let same = recovery
                    .is_some_and(|r| r["runId"] == run.as_str() && r["resetAt"] == reset.as_str());
                let mut value = json!({"runId": run, "resetAt": reset});
                if auto_resume {
                    let scheduled = same && recovery.is_some_and(|r| r["autoResume"] == true);
                    value["autoResume"] = json!(!scheduled);
                } else {
                    let snoozed = same && recovery.is_some_and(|r| r["snooze"] == true);
                    value["snooze"] = json!(!snoozed);
                }
                let mut command = crate::model::command("thread.metadata.update", &thread.id);
                command["limitRecovery"] = value;
                self.dispatch_busy(command, &notice.id, "Could not change limit recovery.");
            }
        }
    }

    /// `projectCloneSendBlockReason`: the open project is still cloning, or
    /// its clone did not finish.
    pub(crate) fn clone_send_block(&self) -> Option<&'static str> {
        let project = self
            .current_thread()
            .map(|t| t.project_id.clone())
            .or_else(|| self.draft_project().map(|p| p.id))?;
        let clone = self
            .clones
            .iter()
            .find(|c| c["projectId"] == project.as_str())?;
        match clone["phase"].as_str() {
            Some("running") => Some("Cloning repository"),
            Some("done") => None,
            _ => Some("Repository not cloned"),
        }
    }

    /// `ProjectCloneToastCoordinator`: a clone's progress, success or
    /// failure as a toast, except while its project's draft is open (the
    /// composer notice shows it there).
    fn sync_clone_toasts(&mut self) {
        use crate::ui::toasts::{Action, Kind, Toast};
        let viewing = self
            .selected
            .is_none()
            .then(|| self.draft_project().map(|p| p.id))
            .flatten();
        let mut seen = std::collections::HashSet::new();
        for clone in self.clones.clone() {
            let Some(project) = clone["projectId"].as_str().map(str::to_owned) else {
                continue;
            };
            seen.insert(project.clone());
            let tracked = self.clone_toasts.get(&project).cloned();
            if viewing.as_deref() == Some(project.as_str()) {
                if let Some((id, ..)) = tracked {
                    self.toasts.close(id);
                    self.clone_toasts.remove(&project);
                }
                continue;
            }
            let text = |key: &str| clone[key].as_str().unwrap_or_default().to_owned();
            let phase = text("phase");
            let key = format!(
                "{phase}:{}:{}:{}:{}",
                text("stage"),
                clone["percent"]
                    .as_f64()
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
                text("detail"),
                text("error")
            );
            if tracked.as_ref().is_some_and(|(_, k, _)| *k == key) {
                continue;
            }
            if !matches!(phase.as_str(), "running" | "done" | "failed" | "cancelled") {
                continue;
            }
            let name = clone_name(&clone);
            let toast = match phase.as_str() {
                "running" => {
                    let mut toast = Toast::new(Kind::Loading, format!("Cloning {name}"));
                    toast.description = clone_progress(&clone);
                    toast.action = Some((
                        "Cancel".into(),
                        Action::Clone {
                            project: project.clone(),
                            action: "cancel",
                        },
                    ));
                    toast.hide_copy = true;
                    toast.timeout = None;
                    toast
                }
                "done" => {
                    let mut toast = Toast::new(Kind::Success, format!("Cloned {name}"))
                        .description(text("destinationPath"));
                    toast.timeout = Some(8.0);
                    toast.action =
                        Some(("Open project".into(), Action::OpenProject(project.clone())));
                    toast.hide_copy = true;
                    toast
                }
                _ => {
                    let cancelled = phase == "cancelled";
                    let mut toast = if cancelled {
                        Toast::new(Kind::Info, format!("Cancelled cloning {name}"))
                            .description(text("destinationPath"))
                    } else {
                        Toast::new(Kind::Error, format!("Failed to clone {name}")).description(
                            clone["error"]
                                .as_str()
                                .unwrap_or("The clone failed.")
                                .to_owned(),
                        )
                    };
                    toast.timeout = None;
                    toast.hide_copy = cancelled;
                    toast.action = Some((
                        "Retry".into(),
                        Action::Clone {
                            project: project.clone(),
                            action: "retry",
                        },
                    ));
                    toast.secondary = vec![(
                        "Remove project".into(),
                        Action::Clone {
                            project: project.clone(),
                            action: "remove",
                        },
                    )];
                    toast
                }
            };
            let id = match tracked {
                Some((id, ..)) => {
                    self.toasts.update(id, toast);
                    id
                }
                None => self.toasts.add(toast),
            };
            self.clone_toasts.insert(project, (id, key, phase));
        }
        // A clone the server stopped tracking takes its toast, unless it
        // settled into a timed success toast.
        let gone: Vec<String> = self
            .clone_toasts
            .keys()
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect();
        for project in gone {
            if let Some((id, _, phase)) = self.clone_toasts.remove(&project)
                && phase != "done"
            {
                self.toasts.close(id);
            }
        }
    }

    /// `subscribeProjectClones` once, where the server tracks clones.
    pub(crate) fn watch_project_clones(&mut self) {
        if !self.clones.is_empty() || !self.clone_toasts.is_empty() {
            self.sync_clone_toasts();
        }
        if self.clones_streaming
            || !self.connected()
            || self.state.config["environment"]["capabilities"]["projectCloneTracking"] != true
        {
            return;
        }
        self.clones_streaming = true;
        self.send(crate::protocol::Action::Stream {
            key: "project-clones".into(),
            method: "subscribeProjectClones",
            payload: json!({}),
        });
    }

    /// The clones' snapshots, and their actions' failures.
    pub(crate) fn clone_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        if key == "project-clones" {
            match result {
                Ok(value) => {
                    self.clones = value
                        .as_array()
                        .or(value["clones"].as_array())
                        .cloned()
                        .unwrap_or_default();
                }
                Err(_) => self.clones_streaming = false,
            }
            return true;
        }
        let Some(kind) = key.strip_prefix("clone-action:") else {
            return false;
        };
        if let Err(error) = result {
            let title = if kind == "cancel" {
                "Failed to cancel clone"
            } else {
                "Failed to retry clone"
            };
            self.toasts.add(
                crate::ui::toasts::Toast::new(crate::ui::toasts::Kind::Error, title)
                    .description(error.clone()),
            );
        }
        true
    }

    /// The branch restore's answer: the thread follows the checkout, or
    /// the failure says why.
    pub(crate) fn notice_result(
        &mut self,
        key: &str,
        result: &Result<serde_json::Value, String>,
    ) -> bool {
        let Some(rest) = key.strip_prefix("restore-branch:") else {
            return false;
        };
        self.restoring_branch = None;
        let Some((thread_id, branch)) = rest.split_once(':') else {
            return true;
        };
        match result {
            Ok(value) => {
                let next = value["refName"].as_str().unwrap_or(branch).to_owned();
                let current = self
                    .state
                    .threads
                    .iter()
                    .find(|t| t.id == thread_id)
                    .and_then(|t| t.branch.clone());
                if current.as_deref() != Some(next.as_str()) {
                    let mut command = crate::model::command("thread.metadata.update", thread_id);
                    command["branch"] = json!(next);
                    command["worktreePath"] = serde_json::Value::Null;
                    self.dispatch_titled(
                        command,
                        "Checkout switched, but the thread could not be updated",
                    );
                }
                self.refresh_vcs();
            }
            Err(error) => {
                self.toasts.add(
                    crate::ui::toasts::Toast::new(
                        crate::ui::toasts::Kind::Error,
                        "Failed to switch checkout",
                    )
                    .description(error.clone()),
                );
            }
        }
        true
    }
}

/// `projectCloneDisplayName`: `owner/repo`, else the folder cloned into.
fn clone_name(snapshot: &serde_json::Value) -> String {
    snapshot["repository"]["nameWithOwner"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            snapshot["destinationPath"]
                .as_str()
                .and_then(|p| p.trim_end_matches('/').rsplit('/').next())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "repository".into())
}

/// `projectCloneProgressSummary`: `Receiving objects · 45% · 12.3 MiB`.
fn clone_progress(snapshot: &serde_json::Value) -> Option<String> {
    let stage = match snapshot["stage"].as_str() {
        Some("connecting") => Some("Connecting"),
        Some("counting") => Some("Counting objects"),
        Some("receiving") => Some("Receiving objects"),
        Some("resolving") => Some("Resolving deltas"),
        Some("checkout") => Some("Checking out files"),
        _ => None,
    };
    let parts: Vec<String> = stage
        .map(str::to_owned)
        .into_iter()
        .chain(
            snapshot["percent"]
                .as_f64()
                .map(|p| format!("{}%", p.round())),
        )
        .chain(
            snapshot["detail"]
                .as_str()
                .filter(|d| !d.is_empty())
                .map(str::to_owned),
        )
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The clone's notice; none once done.
fn clone_notice(snapshot: &serde_json::Value) -> Option<Notice> {
    let project = snapshot["projectId"].as_str()?.to_owned();
    let name = clone_name(snapshot);
    let id = format!("project-clone:{project}");
    match snapshot["phase"].as_str()? {
        "running" => {
            let mut notice = Notice::new(
                id,
                Variant::Info,
                Priority::Activity,
                "download",
                format!("Cloning {name}"),
            );
            notice.description = clone_progress(snapshot);
            notice.actions.push((
                "Cancel".into(),
                NoticeAction::Clone {
                    project,
                    action: "cancel",
                },
            ));
            Some(notice)
        }
        phase @ ("failed" | "cancelled") => {
            let failed = phase == "failed";
            let mut notice = Notice::new(
                id,
                Variant::Warning,
                Priority::Activity,
                "download",
                if failed {
                    format!("Failed to clone {name}")
                } else {
                    format!("Cancelled cloning {name}")
                },
            );
            notice.description = Some(if failed {
                snapshot["error"]
                    .as_str()
                    .unwrap_or("The clone failed.")
                    .to_owned()
            } else {
                "Retry to bring in the repository.".into()
            });
            notice.actions.push((
                "Remove project".into(),
                NoticeAction::Clone {
                    project: project.clone(),
                    action: "remove",
                },
            ));
            notice.actions.push((
                "Retry".into(),
                NoticeAction::Clone {
                    project,
                    action: "retry",
                },
            ));
            Some(notice)
        }
        _ => None,
    }
}

#[cfg(test)]
mod presenter_tests {
    use super::*;

    #[test]
    fn goals_background_work_and_versions_read_like_upstream() {
        let goal = json!({"status": "paused", "tokensUsed": 12_400, "tokenBudget": 1_500_000,
            "timeUsedSeconds": 3700, "checks": 1});
        assert_eq!(
            goal_title(&goal, false),
            "Goal paused · 12k / 1.5m tokens · 1h 1m · 1 check"
        );
        assert_eq!(goal_title(&json!({"status": "active"}), false), "Goal set");
        assert_eq!(
            goal_title(&json!({"status": "active"}), true),
            "Pursuing goal"
        );

        let command = json!({"taskId": "a", "kind": "command", "description": "npm run dev"});
        assert_eq!(
            background_work(std::slice::from_ref(&command)),
            Some(("Running: npm run dev".into(), None, false))
        );
        let monitor =
            json!({"taskId": "b", "kind": "monitor", "description": "Watching pull request #4"});
        let agent = json!({"taskId": "c", "kind": "subagent", "childThreadId": "t2"});
        assert_eq!(
            background_work(&[command, monitor, agent]),
            Some((
                "Waiting on 1 subagent, 1 command and 1 monitor".into(),
                Some("subagent, npm run dev, Watching pull request #4".into()),
                true
            ))
        );
        assert!(background_work(&[]).is_none());

        assert!(server_behind("0.0.45", "0.1.0"));
        assert!(!server_behind("0.1.0", "0.1.0"));
        assert!(!server_behind("0.2.0-nightly", "0.1.0"));
        assert!(server_behind("dev", "0.1.0"));
        assert!(!server_behind("", "0.1.0"));
    }

    #[test]
    fn clone_notice_follows_the_phase() {
        let running = serde_json::json!({
            "projectId": "p", "destinationPath": "/src/pluto/", "repository": null,
            "phase": "running", "stage": "receiving", "percent": 45, "detail": "12 MiB",
        });
        let notice = clone_notice(&running).unwrap();
        assert_eq!(notice.title, "Cloning pluto");
        assert_eq!(
            notice.description.as_deref(),
            Some("Receiving objects · 45% · 12 MiB")
        );
        assert_eq!(notice.actions.len(), 1);
        let failed = serde_json::json!({
            "projectId": "p", "destinationPath": "/src/x", "repository": {"nameWithOwner": "a/b"},
            "phase": "failed", "error": "denied",
        });
        let notice = clone_notice(&failed).unwrap();
        assert_eq!(notice.title, "Failed to clone a/b");
        assert_eq!(notice.description.as_deref(), Some("denied"));
        assert_eq!(notice.actions.len(), 2);
        assert!(clone_notice(&serde_json::json!({"projectId": "p", "phase": "done"})).is_none());
    }
}
