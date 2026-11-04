//! The thread's "Linked pull requests" surface (`ThreadPullRequestsPanel.tsx`,
//! `pullRequestListLines.ts`, `PullRequestListRow.tsx`) and the Link pull
//! request dialog (`LinkPullRequestDialog.tsx`, `usePullRequestLinking.ts`).
//!
//! The links come from the thread shell; linking, unlinking and watching
//! are the `thread.pull-request.link`, `.unlink` and `.watch` commands. Only
//! environments with the `threadPullRequests` capability link (the
//! single-link `thread.meta.update` mode of older servers is not ported).
//!
//! Not ported: the speed actions (Shift held over a row), the checks and
//! author tooltips, avatars (initials are shown). A row opens its pull
//! request on the host until the detail panel is ported.
use eframe::egui::{self, CornerRadius, Key, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::change_request::{ProjectRepository, ResolvedLink, resolve_link_input};
use crate::theme::{self, Weight, font};
use crate::ui::context_menu::{self, Item};
use crate::ui::snooze_dialog::{dialog_button, field};
use crate::ui::thread_pull_requests::{ListLine, list_lines, visible_links};
use crate::ui::widgets;

const ROW: f32 = 44.0; // py-1, a 20px line and a 16px line
const DIALOG_WIDTH: f32 = 576.0; // max-w-xl
const DESTRUCTIVE: egui::Color32 = egui::Color32::from_rgb(251, 65, 74);

/// The open row menu and the link dialog.
#[derive(Default)]
pub(crate) struct LinkedPullRequests {
    menu: Option<(String, context_menu::State)>,
    pub(crate) dialog: Option<LinkDialog>,
}

/// `LinkPullRequestDialog`'s state, for the thread that asked.
pub(crate) struct LinkDialog {
    thread: String,
    reference: String,
    dirty: bool,
    error: Option<String>,
    /// The `thread.pull-request.link` command on its way.
    pending: Option<String>,
    focus: bool,
}

/// `SOURCE_LABELS`.
fn source_label(source: &str) -> &'static str {
    match source {
        "created" => "Created from this thread",
        "agent" => "Linked by the agent",
        "stack" => "Found in the stack",
        "stack-dismissed" => "Dismissed",
        _ => "Linked by you",
    }
}

fn key_of(link: &Value) -> String {
    format!(
        "{}/{}#{}",
        link["host"].as_str().unwrap_or_default(),
        link["repository"].as_str().unwrap_or_default(),
        link["number"]
    )
}

fn is_open(link: &Value) -> bool {
    link["snapshot"].is_null() || link["snapshot"]["state"] == "open"
}

fn ago(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let time = widgets::relative_time(iso, now);
    if time == "now" || time.is_empty() {
        time
    } else {
        format!("{time} ago")
    }
}

/// Middle-cut `text` to `width` (`MiddleTruncate`).
fn middle_truncate(ui: &Ui, text: &str, font: egui::FontId, width: f32) -> String {
    if widgets::text_width(ui, text, font.clone()) <= width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut keep = chars.len();
    while keep > 2 {
        keep -= 1;
        let head: String = chars[..keep.div_ceil(2)].iter().collect();
        let tail: String = chars[chars.len() - keep / 2..].iter().collect();
        let candidate = format!("{head}…{tail}");
        if widgets::text_width(ui, &candidate, font.clone()) <= width {
            return candidate;
        }
    }
    "…".into()
}

impl App {
    /// Whether this environment links pull requests to threads.
    pub(crate) fn links_pull_requests(&self) -> bool {
        self.capability("threadPullRequests")
    }

    /// The open thread's visible links (`visibleThreadPullRequests`).
    pub(crate) fn linked_pull_request_count(&self) -> usize {
        self.current_thread()
            .map_or(0, |t| visible_links(&t.pull_requests).len())
    }

    /// `openLinkPullRequestDialog` for the open thread.
    pub(crate) fn open_link_pull_request_dialog(&mut self) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        self.linked_prs.dialog = Some(LinkDialog {
            thread,
            reference: String::new(),
            dirty: false,
            error: None,
            pending: None,
            focus: true,
        });
    }

    /// Opens the Linked pull requests surface.
    pub(crate) fn show_linked_pull_requests(&mut self) {
        if let Some(panel) = self.panel_mut() {
            panel.show(crate::app::PanelTab::PullRequests, |t| {
                *t == crate::app::PanelTab::PullRequests
            });
            panel.open = true;
        }
    }

    fn pull_request_command(&mut self, kind: &str, thread: &str, link: &Value, extra: Value) {
        let mut command = crate::model::command(kind, thread);
        command["host"] = link["host"].clone();
        command["repository"] = link["repository"].clone();
        command["number"] = link["number"].clone();
        for (key, value) in extra.as_object().into_iter().flatten() {
            command[key] = value.clone();
        }
        self.dispatch(command);
    }

    /// The surface's body: the rows and the footer, or the empty state.
    pub(crate) fn linked_pull_requests_panel(&mut self, ui: &mut Ui, body: Rect) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        if !self.links_pull_requests() {
            centered_message(
                ui,
                body,
                "Linked pull requests unavailable",
                "This environment does not support multiple linked pull requests.",
            );
            return;
        }
        let links = visible_links(&thread.pull_requests);
        if links.is_empty() {
            if empty_state(ui, body) {
                self.open_link_pull_request_dialog();
            }
            return;
        }
        let lines = list_lines(&links);
        let watch = self.capability("threadPullRequestWatch");
        let footer_height = 33.0;
        let list = Rect::from_min_max(
            body.min,
            Pos2::new(body.right(), body.bottom() - footer_height),
        );
        let mut chosen: Option<(String, Value)> = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt("linked-pull-requests")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let width = list.width() - 12.0;
                let mut y = origin.y + 6.0;
                for line in &lines {
                    let rect =
                        Rect::from_min_size(Pos2::new(origin.x + 6.0, y), Vec2::new(width, ROW));
                    if let Some(action) = self.linked_pull_request_row(ui, rect, line) {
                        chosen = Some((action, line.link.clone()));
                    }
                    y += ROW;
                }
                ui.allocate_rect(
                    Rect::from_min_max(origin, Pos2::new(list.right(), y + 6.0)),
                    Sense::hover(),
                );
            });
        // The row menu.
        if let Some((key, state)) = self.linked_prs.menu.take()
            && let Some(link) = links.iter().find(|l| key_of(l) == key)
        {
            let watching = link["watch"].is_object();
            let mut items = vec![
                Item::new("copy", "Copy link").icon("link"),
                Item::new("open", "Open").icon("arrow-up-right"),
            ];
            if watch && is_open(link) {
                items.push(if watching {
                    Item::new("unwatch", "Stop watching").icon("eye-off")
                } else {
                    Item::new("watch", "Watch for changes").icon("eye")
                });
            }
            items.push(
                Item::new(
                    "unlink",
                    if link["source"] == "stack" {
                        "Dismiss from thread"
                    } else {
                        "Unlink from thread"
                    },
                )
                .icon("unlink-2"),
            );
            let mut open = Some(state);
            let picked = context_menu::show(ui, &mut open, &items);
            if let Some(state) = open {
                self.linked_prs.menu = Some((key, state));
            }
            if let Some(picked) = picked {
                self.linked_prs.menu = None;
                chosen = Some((picked, (*link).clone()));
            }
        }
        if let Some((action, link)) = chosen {
            let url = link["url"].as_str().unwrap_or_default().to_owned();
            match action.as_str() {
                "copy" => ui.ctx().copy_text(url),
                "open" => widgets::open_external(&url),
                "watch" | "unwatch" => self.pull_request_command(
                    "thread.pull-request.watch",
                    &thread.id,
                    &link,
                    json!({"watching": action == "watch"}),
                ),
                "unlink" => self.pull_request_command(
                    "thread.pull-request.unlink",
                    &thread.id,
                    &link,
                    json!({}),
                ),
                _ => {}
            }
        }
        // Footer: counts, the last sync, "Link".
        let footer = Rect::from_min_max(Pos2::new(body.left(), list.bottom()), body.max);
        ui.painter().rect_filled(
            Rect::from_min_size(footer.min, Vec2::new(footer.width(), 1.0)),
            0.0,
            theme::alpha(theme::border(), 0.6),
        );
        let open = links.iter().filter(|l| is_open(l)).count();
        let synced = links
            .iter()
            .filter_map(|l| l["snapshot"]["syncedAt"].as_str())
            .max();
        let mut summary = format!("{open} open · {} linked", links.len());
        if let Some(synced) = synced {
            summary.push_str(&format!(" · synced {}", ago(synced, self.now)));
        }
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(footer.left() + 8.0, footer.top() + 1.0),
                footer.max,
            ),
            &summary,
            font(11.0, Weight::Regular),
            theme::muted_foreground(),
        );
        let label = "Link";
        let width =
            6.0 + 14.0 + 4.0 + widgets::text_width(ui, label, font(12.0, Weight::Medium)) + 6.0;
        let button = Rect::from_min_size(
            Pos2::new(footer.right() - 8.0 - width, footer.center().y - 12.0 + 0.5),
            Vec2::new(width, 24.0),
        );
        if ghost_button(ui, button, "linked-prs-link", "plus", label) {
            self.open_link_pull_request_dialog();
        }
    }

    /// One `LinkRow`; returns the action its "…" button or menu chose.
    fn linked_pull_request_row(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        line: &ListLine,
    ) -> Option<String> {
        let link = line.link;
        let snapshot = &link["snapshot"];
        let key = key_of(link);
        let response = ui.interact(rect, ui.id().with(("linked-pr", &key)), Sense::click());
        let menu_open = self
            .linked_prs
            .menu
            .as_ref()
            .is_some_and(|(open, _)| *open == key);
        let hovered = response.hovered() || menu_open;
        if hovered {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(6),
                theme::alpha(theme::accent(), 0.6),
            );
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.linked_prs.menu = Some((key.clone(), context_menu::State::new(at)));
        }
        let mut left = rect.left() + 8.0 + line.depth.min(3) as f32 * 20.0;
        if line.depth > 0 {
            ui.painter().rect_filled(
                Rect::from_center_size(
                    Pos2::new(left - 8.0 + 0.5, rect.center().y),
                    Vec2::new(1.0, 24.0),
                ),
                0.0,
                theme::alpha(theme::border(), 0.7),
            );
            left += 1.0;
        }
        let line1 = rect.top() + 4.0;
        let line2 = line1 + 20.0;
        // The lifecycle glyph, with the conflict badge on its corner.
        let glyph = Rect::from_min_size(Pos2::new(left, rect.center().y - 8.0), Vec2::splat(16.0));
        if snapshot.is_null() {
            theme::paint_icon(
                ui,
                "git-pull-request-arrow",
                glyph,
                16.0,
                theme::muted_foreground(),
            );
        } else {
            let (icon, tone) = crate::ui::pull_requests::state_glyph(snapshot);
            theme::paint_icon(ui, icon, glyph, 16.0, tone);
            if snapshot["state"] == "open"
                && snapshot["isDraft"] != true
                && snapshot["mergeability"] == "conflicting"
            {
                let badge = Rect::from_min_size(glyph.min + Vec2::new(8.0, 8.0), Vec2::splat(12.0));
                ui.painter()
                    .circle_filled(badge.center(), 6.0, theme::background());
                theme::paint_icon(ui, "triangle-alert", badge, 12.0, DESTRUCTIVE);
            }
        }
        let text_left = glyph.right() + 8.0;
        let right = rect.right() - 4.0;
        // First line: number, title, signals; the diff counts on the right.
        let number = format!("#{}", link["number"]);
        let mono = theme::mono(12.0);
        let number_width = widgets::tabular_width(ui, &number, mono.clone());
        let number_rect = Rect::from_min_size(
            Pos2::new(text_left, line1 + 2.0),
            Vec2::new(number_width, 16.0),
        );
        widgets::tabular_text(
            ui,
            number_rect.min,
            &number,
            mono,
            theme::muted_foreground(),
            16.0,
        );
        let number_hover = ui.interact(
            number_rect,
            ui.id().with(("linked-pr-number", &key)),
            Sense::hover(),
        );
        number_hover.on_hover_text(format!(
            "{} · {}",
            source_label(link["source"].as_str().unwrap_or_default()),
            ago(link["linkedAt"].as_str().unwrap_or_default(), self.now)
        ));
        let mut status_left = right;
        let (additions, deletions) = (
            snapshot["additions"].as_i64().unwrap_or(0),
            snapshot["deletions"].as_i64().unwrap_or(0),
        );
        if additions + deletions > 0 {
            let mono = theme::mono(11.0);
            let removed = format!("-{deletions}");
            let added = format!("+{additions}");
            let removed_width = widgets::text_width(ui, &removed, mono.clone());
            let added_width = widgets::text_width(ui, &added, mono.clone());
            widgets::text(
                ui,
                Pos2::new(right - removed_width, line1 + 3.0),
                &removed,
                mono.clone(),
                theme::diff_deletion_foreground(),
                16.0,
            );
            widgets::text(
                ui,
                Pos2::new(right - removed_width - 4.0 - added_width, line1 + 3.0),
                &added,
                mono,
                theme::diff_addition_foreground(),
                16.0,
            );
            status_left = right - removed_width - 4.0 - added_width - 6.0;
        }
        let mut signals: Vec<(&str, egui::Color32, &str)> = Vec::new();
        if is_open(link) {
            if link["watch"].is_object() {
                signals.push((
                    "eye",
                    theme::foreground(),
                    "Watching: the agent wakes when checks finish, someone comments, or the branch conflicts",
                ));
            }
            match snapshot["checksState"].as_str() {
                Some("passing") => signals.push((
                    "circle-check",
                    theme::alpha(theme::EMERALD_300, 0.9),
                    "Checks passing",
                )),
                Some("failing") => signals.push(("circle-x", DESTRUCTIVE, "Checks failing")),
                Some("pending") => signals.push((
                    "circle-dot",
                    theme::alpha(egui::Color32::from_rgb(0xff, 0xb9, 0x00), 0.9),
                    "Checks pending",
                )),
                _ => {}
            }
            match snapshot["reviewDecision"].as_str() {
                Some("approved") => signals.push((
                    "user-check",
                    theme::alpha(theme::EMERALD_300, 0.9),
                    "Approved",
                )),
                Some("changes-requested") => signals.push((
                    "user-round-x",
                    theme::alpha(egui::Color32::from_rgb(0xff, 0xb9, 0x00), 0.8),
                    "Changes requested",
                )),
                Some("review-required") => signals.push((
                    "user-round",
                    theme::alpha(theme::muted_foreground(), 0.6),
                    "Review required",
                )),
                _ => {}
            }
        }
        let title_left = number_rect.right() + 6.0;
        let signals_width = signals.len() as f32 * 18.0;
        let title_max = (status_left
            - title_left
            - if signals.is_empty() {
                0.0
            } else {
                signals_width + 6.0
            })
        .max(0.0);
        let title = snapshot["title"]
            .as_str()
            .or(link["repository"].as_str())
            .unwrap_or_default();
        let title_width = widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(title_left, line1), Vec2::new(title_max, 20.0)),
            title,
            font(14.0, Weight::Regular),
            theme::foreground(),
        );
        let mut x = title_left + title_width + 6.0;
        for (index, (icon, color, tip)) in signals.into_iter().enumerate() {
            let signal = Rect::from_min_size(Pos2::new(x, line1 + 3.0), Vec2::splat(14.0));
            theme::paint_icon(ui, icon, signal, 14.0, color);
            ui.interact(
                signal,
                ui.id().with(("linked-pr-signal", &key, index)),
                Sense::hover(),
            )
            .on_hover_text(tip);
            x += 18.0;
        }
        // Second line: stack, author, repository and branches; the time.
        let small = font(11.0, Weight::Regular);
        let time = snapshot["updatedAt"]
            .as_str()
            .map(|t| ago(t, self.now))
            .unwrap_or_default();
        let time_width = widgets::tabular_width(ui, &time, small.clone());
        widgets::tabular_text(
            ui,
            Pos2::new(right - time_width, line2),
            &time,
            small.clone(),
            theme::muted_foreground(),
            16.0,
        );
        let meta_right = right - time_width - 6.0;
        let mut x = text_left;
        if let Some((native, size)) = line.stack {
            let stack = Rect::from_min_size(Pos2::new(x, line2), Vec2::new(12.0 + 2.0 + 8.0, 16.0));
            theme::paint_icon(
                ui,
                "layers",
                Rect::from_min_size(Pos2::new(x, line2 + 2.0), Vec2::splat(12.0)),
                12.0,
                theme::alpha(theme::foreground(), 0.7),
            );
            let count = size.to_string();
            let w = widgets::tabular_width(ui, &count, small.clone());
            widgets::tabular_text(
                ui,
                Pos2::new(x + 14.0, line2),
                &count,
                small.clone(),
                theme::alpha(theme::foreground(), 0.7),
                16.0,
            );
            let tip = if native {
                let host = if link["url"]
                    .as_str()
                    .is_some_and(|u| u.contains("/-/merge_requests/"))
                {
                    "GitLab"
                } else {
                    "GitHub"
                };
                format!("{host} stack of {size}: merging a layer lands the ones below it.")
            } else {
                format!("{size} pull requests chained by base branch.")
            };
            ui.interact(
                stack,
                ui.id().with(("linked-pr-stack", &key)),
                Sense::hover(),
            )
            .on_hover_text(tip);
            x += 14.0 + w + 6.0;
        }
        if let Some(login) = snapshot["author"]["login"].as_str() {
            let avatar = Rect::from_min_size(Pos2::new(x, line2 + 1.0), Vec2::splat(14.0));
            ui.painter()
                .circle_filled(avatar.center(), 7.0, theme::muted());
            let initial = login
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            let initial_width = widgets::text_width(ui, &initial, font(10.0, Weight::Medium));
            widgets::text(
                ui,
                Pos2::new(avatar.center().x - initial_width / 2.0, avatar.top() + 1.0),
                &initial,
                font(10.0, Weight::Medium),
                theme::muted_foreground(),
                12.0,
            );
            x = avatar.right() + 4.0;
            let w = widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(x, line2),
                    Vec2::new((meta_right - x).clamp(0.0, 112.0), 16.0),
                ),
                login,
                small.clone(),
                theme::muted_foreground(),
            );
            x += w + 6.0;
        }
        let mono_small = theme::mono(11.0);
        let repository = link["repository"].as_str().unwrap_or_default();
        if snapshot.is_null() {
            let text = format!("{}/{repository}", link["host"].as_str().unwrap_or_default());
            widgets::text_in(
                ui,
                Rect::from_min_size(
                    Pos2::new(x, line2),
                    Vec2::new((meta_right - x).max(0.0), 16.0),
                ),
                &text,
                mono_small,
                theme::muted_foreground(),
            );
        } else if x < meta_right {
            let shown = middle_truncate(
                ui,
                repository,
                mono_small.clone(),
                (meta_right - x).min(128.0),
            );
            let w = widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(x, line2), Vec2::new(meta_right - x, 16.0)),
                &shown,
                mono_small.clone(),
                theme::muted_foreground(),
            );
            if shown != repository {
                ui.interact(
                    Rect::from_min_size(Pos2::new(x, line2), Vec2::new(w, 16.0)),
                    ui.id().with(("linked-pr-repo", &key)),
                    Sense::hover(),
                )
                .on_hover_text(repository);
            }
            x += w + 6.0;
            // `head → base`, the base keeping up to 45% of the room.
            let (head, base) = (
                snapshot["headBranch"].as_str().unwrap_or_default(),
                snapshot["baseBranch"].as_str().unwrap_or_default(),
            );
            let room = meta_right - x;
            let arrow = widgets::text_width(ui, "→", mono_small.clone());
            if room > arrow + 24.0 {
                let base_shown = middle_truncate(ui, base, mono_small.clone(), room * 0.45);
                let base_width = widgets::text_width(ui, &base_shown, mono_small.clone());
                let head_shown = middle_truncate(
                    ui,
                    head,
                    mono_small.clone(),
                    (room - base_width - arrow - 8.0).max(0.0),
                );
                widgets::text(
                    ui,
                    Pos2::new(x, line2),
                    &format!("{head_shown} → {base_shown}"),
                    mono_small,
                    theme::muted_foreground(),
                    16.0,
                );
            }
        }
        // The "…" button over the right end of the second line.
        if hovered {
            let button = Rect::from_min_size(
                Pos2::new(rect.right() - 4.0 - 20.0, rect.bottom() - 2.0 - 20.0),
                Vec2::splat(20.0),
            );
            ui.painter().rect_filled(
                button.expand2(Vec2::new(6.0, 0.0)),
                0.0,
                theme::mix(theme::background(), theme::accent(), 0.6),
            );
            let more = widgets::icon_button(
                ui,
                button,
                ("linked-pr-more", &key),
                "ellipsis",
                14.0,
                theme::muted_foreground(),
                "",
            );
            if more.clicked() {
                self.linked_prs.menu = Some((
                    key.clone(),
                    context_menu::State::dropdown(Pos2::new(button.right(), button.bottom() + 4.0))
                        .aligned_end(),
                ));
            }
        }
        response.clicked().then(|| "open".to_owned())
    }

    /// The Link pull request dialog, over everything.
    pub(crate) fn link_pull_request_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some(mut dialog) = self.linked_prs.dialog.take() else {
            return;
        };
        let project = self
            .state
            .threads
            .iter()
            .find(|t| t.id == dialog.thread)
            .and_then(|t| self.state.projects.iter().find(|p| p.id == t.project_id))
            .and_then(|p| p.repository_identity.as_ref())
            .and_then(ProjectRepository::of);
        let resolved = resolve_link_input(&dialog.reference, project.as_ref());
        let validation: Option<String> = if !dialog.dirty {
            None
        } else if dialog.reference.trim().is_empty() {
            Some("Paste a pull request URL or enter 123 / #123.".into())
        } else {
            match &resolved {
                None => Some("Use a pull request URL, 123, or #123.".into()),
                Some(Err(error)) => Some(error.clone()),
                Some(Ok(_)) => None,
            }
        };
        let preview = match &resolved {
            Some(Ok(link)) => Some(format!(
                "{}/{} #{}",
                link.host, link.repository, link.number
            )),
            _ => None,
        };
        let message = validation.clone().or(dialog.error.clone());
        let pending = dialog.pending.is_some();
        let description = "Attach a pull request to this thread by its URL, or by its number for this thread's repository.";
        let mut job = egui::text::LayoutJob::single_section(
            description.to_owned(),
            widgets::format(font(14.0, Weight::Regular), theme::muted_foreground(), 20.0),
        );
        job.wrap.max_width = DIALOG_WIDTH - 48.0;
        let description = ui.fonts_mut(|f| f.layout_job(job));
        let panel_top = 25.0 + 20.0 + 8.0 + description.size().y + 16.0;
        let extra = preview.as_ref().map_or(0.0, |_| 24.0) + message.as_ref().map_or(0.0, |_| 24.0);
        let height = panel_top + 32.0 + extra + 24.0 + 66.0;
        let d = Rect::from_min_size(
            Pos2::new(
                (screen.center().x - DIALOG_WIDTH / 2.0).round(),
                (screen.center().y - height / 2.0).round(),
            ),
            Vec2::new(DIALOG_WIDTH, height),
        );
        let mut close = false;
        let mut submit = false;
        egui::Area::new(ui.id().with("link-pr-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                let (backdrop, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(
                    backdrop,
                    0.0,
                    theme::alpha(theme::background(), 200.0 / 255.0),
                );
                if response.clicked() && !ui.rect_contains_pointer(d) && !pending {
                    close = true;
                }
                ui.painter().rect(
                    d,
                    CornerRadius::same(18),
                    theme::shade(12),
                    Stroke::new(1.0, theme::input()),
                    egui::StrokeKind::Inside,
                );
                ui.interact(d, ui.id().with("link-pr-body"), Sense::click());
                let left = d.left() + 24.0;
                widgets::text(
                    ui,
                    Pos2::new(left, d.top() + 25.0),
                    "Link pull request",
                    font(20.0, Weight::SemiBold),
                    theme::foreground(),
                    20.0,
                );
                ui.painter().galley(
                    Pos2::new(left, d.top() + 53.0),
                    description.clone(),
                    theme::muted_foreground(),
                );
                let input = Rect::from_min_size(
                    Pos2::new(left, d.top() + panel_top),
                    Vec2::new(DIALOG_WIDTH - 48.0, 32.0),
                );
                field(ui, input, 8);
                let mut field_ui = ui.new_child(
                    egui::UiBuilder::new().max_rect(input.shrink2(Vec2::new(11.0, 6.0))),
                );
                let before = dialog.reference.clone();
                let edit = field_ui.add(
                    egui::TextEdit::singleline(&mut dialog.reference)
                        .frame(egui::Frame::NONE)
                        .hint_text("Pull request URL or #42")
                        .interactive(!pending)
                        .desired_width(f32::INFINITY)
                        .font(font(14.0, Weight::Regular)),
                );
                if dialog.focus {
                    edit.request_focus();
                    dialog.focus = false;
                }
                if dialog.reference != before {
                    dialog.dirty = true;
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
                let mut y = input.bottom() + 8.0;
                if let Some(preview) = &preview {
                    widgets::text_in(
                        ui,
                        Rect::from_min_size(Pos2::new(left, y), Vec2::new(input.width(), 16.0)),
                        preview,
                        font(12.0, Weight::Regular),
                        theme::muted_foreground(),
                    );
                    y += 24.0;
                }
                if let Some(message) = &message {
                    widgets::text_in(
                        ui,
                        Rect::from_min_size(Pos2::new(left, y), Vec2::new(input.width(), 16.0)),
                        message,
                        font(12.0, Weight::Regular),
                        theme::error(),
                    );
                }
                // Footer.
                let footer = Rect::from_min_max(
                    Pos2::new(d.left() + 1.0, d.bottom() - 66.0),
                    Pos2::new(d.right() - 1.0, d.bottom() - 1.0),
                );
                ui.painter().rect_filled(
                    footer,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 17,
                        se: 17,
                    },
                    theme::overlay(0.0216),
                );
                ui.painter().rect_filled(
                    Rect::from_min_size(footer.min, Vec2::new(footer.width(), 1.0)),
                    0.0,
                    theme::border(),
                );
                let label = if pending { "Linking..." } else { "Link" };
                let link_width = widgets::text_width(ui, label, font(14.0, Weight::Medium)) + 22.0;
                let link = Rect::from_min_size(
                    Pos2::new(d.right() - 25.0 - link_width, footer.top() + 17.0),
                    Vec2::new(link_width, 32.0),
                );
                let cancel = Rect::from_min_size(
                    Pos2::new(link.left() - 7.0 - 70.0, link.top()),
                    Vec2::new(70.0, 32.0),
                );
                if dialog_button(ui, cancel, "link-pr-cancel", "Cancel", false) && !pending {
                    close = true;
                }
                let ready = matches!(resolved, Some(Ok(_))) && !pending;
                if ready {
                    if dialog_button(ui, link, "link-pr-submit", label, true) {
                        submit = true;
                    }
                } else {
                    widgets::primary_button(ui, link, "link-pr-submit-off", label, false);
                }
            });
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) && !pending {
            close = true;
        }
        if close {
            return;
        }
        if submit && !pending {
            dialog.dirty = true;
            if let Some(Ok(link)) = resolved {
                dialog.error = None;
                dialog.pending = Some(self.link_pull_request(&dialog.thread, &link));
            }
        }
        self.linked_prs.dialog = Some(dialog);
    }

    /// Sends `thread.pull-request.link`; returns its command id.
    fn link_pull_request(&mut self, thread: &str, link: &ResolvedLink) -> String {
        let mut command = crate::model::command("thread.pull-request.link", thread);
        command["host"] = json!(link.host);
        command["repository"] = json!(link.repository);
        command["number"] = json!(link.number);
        command["url"] = json!(link.url);
        command["source"] = json!("manual");
        let id = command["commandId"].as_str().unwrap_or_default().to_owned();
        self.dispatch(command);
        id
    }

    /// The link command's receipt: the dialog closes, or shows why not.
    pub(crate) fn link_pull_request_settled(&mut self, id: &str, failure: Option<&str>) -> bool {
        let Some(dialog) = self
            .linked_prs
            .dialog
            .as_mut()
            .filter(|d| d.pending.as_deref() == Some(id))
        else {
            return false;
        };
        match failure {
            Some(message) => {
                dialog.pending = None;
                dialog.error = Some(if message.trim().is_empty() {
                    "Could not link the pull request.".into()
                } else {
                    message.to_owned()
                });
            }
            None => self.linked_prs.dialog = None,
        }
        true
    }
}

/// A `size="xs" variant="ghost"` button with an icon.
fn ghost_button(ui: &mut Ui, rect: Rect, id: &str, icon: &str, label: &str) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::accent());
    }
    theme::paint_icon(
        ui,
        icon,
        Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.top()),
            Vec2::new(14.0, rect.height()),
        ),
        14.0,
        theme::foreground(),
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(rect.left() + 24.0, rect.top()), rect.max),
        label,
        font(12.0, Weight::Medium),
        theme::foreground(),
    );
    response.clicked()
}

/// `PullRequestsUnavailableState`'s title and message, centered.
fn centered_message(ui: &Ui, body: Rect, title: &str, message: &str) {
    let center = body.center();
    let width = widgets::text_width(ui, title, font(14.0, Weight::Medium));
    widgets::text(
        ui,
        Pos2::new(center.x - width / 2.0, center.y - 20.0),
        title,
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
    );
    let mut job = egui::text::LayoutJob::single_section(
        message.to_owned(),
        widgets::format(font(12.0, Weight::Regular), theme::muted_foreground(), 16.0),
    );
    job.wrap.max_width = 240.0;
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        Pos2::new(center.x, center.y + 4.0),
        galley,
        theme::muted_foreground(),
    );
}

/// "No linked pull requests" with its "Link pull request" button; true
/// when the button was clicked.
fn empty_state(ui: &mut Ui, body: Rect) -> bool {
    let center = body.center();
    let mut job = egui::text::LayoutJob::single_section(
        "Pull requests the agent opens from this thread land here. Link one yourself from a URL or a number.".to_owned(),
        widgets::format(font(12.0, Weight::Regular), theme::muted_foreground(), 16.0),
    );
    job.wrap.max_width = 240.0;
    job.halign = egui::Align::Center;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    // icon 24, gap 8, title 20, gap 8, text, gap 8, button 32.
    let height = 24.0 + 8.0 + 20.0 + 8.0 + galley.size().y + 8.0 + 32.0;
    let top = center.y - height / 2.0;
    theme::paint_icon(
        ui,
        "link-2",
        Rect::from_center_size(Pos2::new(center.x, top + 12.0), Vec2::splat(24.0)),
        24.0,
        theme::alpha(theme::muted_foreground(), 0.6),
    );
    let title = "No linked pull requests";
    let width = widgets::text_width(ui, title, font(14.0, Weight::Medium));
    widgets::text(
        ui,
        Pos2::new(center.x - width / 2.0, top + 32.0),
        title,
        font(14.0, Weight::Medium),
        theme::foreground(),
        20.0,
    );
    let text_height = galley.size().y;
    ui.painter().galley(
        Pos2::new(center.x, top + 60.0),
        galley,
        theme::muted_foreground(),
    );
    let label = "Link pull request";
    let button_width = widgets::outline_button_width(ui, true, label);
    let button = Rect::from_min_size(
        Pos2::new(
            center.x - button_width / 2.0,
            top + 60.0 + text_height + 8.0,
        ),
        Vec2::new(button_width, 32.0),
    );
    widgets::outline_button(
        ui,
        button,
        "linked-prs-empty-link",
        Some(("plus", theme::foreground())),
        label,
        CornerRadius::same(8),
    )
    .clicked()
}
