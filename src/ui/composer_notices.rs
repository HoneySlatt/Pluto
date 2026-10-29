//! The composer's notices (`ComposerBannerStack.tsx`, `ComposerBanner.tsx`,
//! `ChatView.tsx`'s `composerBannerItems`): the first notice attached above
//! the composer, the others behind it as a 12px peek that opens them on
//! hover. Ordered like upstream's `bannerPriority`: activity first, then
//! errors and warnings, then the other notices.
//!
//! Items: the server reconnecting (with Change server, as the single
//! environment's Disconnect), subagents waiting on the user (Open approval
//! or Open question), a usage limit reached with its reset time, and a
//! local checkout whose branch changed since the thread ran there (Restore
//! branch). Not ported yet: the server update, project clone, goal,
//! background work, usage-limits command and feedback notices.

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `--chat-composer-drawer-inset`.
const INSET: f32 = 22.0;
/// A notice's row (`density="comfortable"`: 28px and 5px above and below).
const ROW: f32 = 28.0;
const PAD: f32 = 5.0;
/// The peek of the notices behind the front one.
const PEEK: f32 = 12.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Variant {
    Info,
    Warning,
}

#[derive(Clone, Copy, PartialEq)]
enum Priority {
    Activity,
    Notice,
}

#[derive(Clone)]
pub(crate) enum NoticeAction {
    ChangeServer,
    OpenThread(String),
    RestoreBranch(String),
}

#[derive(Clone)]
pub(crate) struct Notice {
    id: String,
    variant: Variant,
    priority: Priority,
    icon: &'static str,
    title: String,
    description: Option<String>,
    actions: Vec<(String, NoticeAction)>,
    dismissable: bool,
}

impl Notice {
    /// `bannerPriority`.
    fn rank(&self) -> u8 {
        if self.priority == Priority::Activity {
            0
        } else if self.variant == Variant::Warning {
            1
        } else {
            2
        }
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
        Variant::Info => (theme::shade(16), theme::shade(28)),
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
                notices.push(Notice {
                    id: format!("child-input:{first}"),
                    variant: Variant::Info,
                    priority: Priority::Activity,
                    icon: if approval {
                        "shield-question"
                    } else {
                        "message-circle"
                    },
                    title: if children.len() == 1 {
                        format!("Subagent needs {need}")
                    } else {
                        format!("{} subagents need {need}", children.len())
                    },
                    description: Some(
                        children
                            .iter()
                            .map(|c| c.1.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    actions: vec![(
                        if approval {
                            "Open approval"
                        } else {
                            "Open question"
                        }
                        .into(),
                        NoticeAction::OpenThread(first),
                    )],
                    dismissable: false,
                });
            }
            // A usage limit: the reset time it waits for.
            if let Some(runtime) = &thread.runtime
                && runtime.status == "failed"
                && runtime.last_error_class.as_deref() == Some("usage_limit")
            {
                let reset = runtime
                    .usage_limit_reset_at
                    .as_deref()
                    .map(|at| crate::ui::usage::upcoming_time(at, self.now))
                    .filter(|t| !t.is_empty());
                notices.push(Notice {
                    id: format!("usage-limit:{}", thread.id),
                    variant: Variant::Warning,
                    priority: Priority::Notice,
                    icon: "gauge",
                    title: "Usage limit reached".into(),
                    description: reset.map(|time| format!("Resets {time}")),
                    actions: Vec::new(),
                    dismissable: true,
                });
            }
        }
        if let Some((host, error)) = self.reconnecting_host() {
            notices.push(Notice {
                id: "environment-unavailable".into(),
                variant: Variant::Warning,
                priority: Priority::Notice,
                icon: "wifi-off",
                title: format!("{host} is reconnecting"),
                description: Some(error).filter(|e| !e.is_empty()),
                actions: vec![("Change server".into(), NoticeAction::ChangeServer)],
                dismissable: false,
            });
        }
        if let Some(thread) = &thread
            && let Some(was) = self.branch_mismatch(thread)
        {
            notices.push(Notice {
                id: format!("branch-mismatch:{}:{was}", thread.id),
                variant: Variant::Info,
                priority: Priority::Notice,
                icon: "git-branch",
                title: format!("Branch changed — was {was}"),
                description: None,
                actions: vec![("Restore branch".into(), NoticeAction::RestoreBranch(was))],
                dismissable: true,
            });
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
        let tone = match notice.variant {
            Variant::Warning => theme::warning(),
            Variant::Info => theme::muted_foreground(),
        };
        theme::paint_icon(
            ui,
            notice.icon,
            Rect::from_center_size(
                Pos2::new(row.left() + 12.0, row.center().y),
                Vec2::splat(14.0),
            ),
            14.0,
            tone,
        );
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
                .on_hover_text("Dismiss");
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
        painter.galley(
            Pos2::new(text_left, row.center().y - 8.0),
            title,
            theme::foreground(),
        );
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
        }
        match clicked {
            Some(NoticeAction::ChangeServer) => self.change_server(),
            Some(NoticeAction::OpenThread(id)) => self.select_thread(Some(id)),
            Some(NoticeAction::RestoreBranch(branch)) => {
                if let Some(cwd) = self.workspace_cwd() {
                    let key = format!("restore-branch:{}", uuid::Uuid::new_v4());
                    self.send_request(key, "vcs.switchRef", json!({"cwd": cwd, "refName": branch}));
                }
            }
            None => {}
        }
    }
}
