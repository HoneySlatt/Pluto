//! Sidebar (`Sidebar.tsx`): wordmark, search, thread cards grouped into the
//! pinned/active list and the settled shelf, footer shortcuts.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::App;
use crate::model::Thread;
use crate::theme::{self, Weight, font};
use crate::timeline::parse_time;
use crate::ui::{project_icon, project_scope, widgets};

const CARD_HEIGHT: f32 = 78.0; // h-[4.875rem]
const CARD_SLOT: f32 = 83.0; // card + py-0.5, measured
const SLIM_HEIGHT: f32 = 36.0; // h-9
const LIST_GAP: f32 = 1.0; // gap-px between list items
const LIST_TOP: f32 = 100.0; // header (52) + search block (48)
const FOOTER_HEIGHT: f32 = 48.0;

fn timestamp_ms(iso: Option<&str>) -> i64 {
    iso.and_then(parse_time)
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// `sortActiveThreadsByOrderKey`: unkeyed threads first by anchor time, then keyed ones.
pub fn sort_active(threads: &mut [&Thread]) {
    threads.sort_by(|a, b| {
        let anchor = |t: &Thread| {
            timestamp_ms(Some(&t.created_at)).max(timestamp_ms(t.unsettled_at.as_deref()))
        };
        match (&a.active_order_key, &b.active_order_key) {
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => x.cmp(y),
            (None, None) => anchor(b).cmp(&anchor(a)),
        }
        .then_with(|| a.id.cmp(&b.id))
    });
}

/// `sortPinnedThreadsByOrderKey`: keyed threads by key, then keyless by creation time.
pub fn sort_pinned(threads: &mut [&Thread]) {
    threads.sort_by(|a, b| match (&a.pin_order_key, &b.pin_order_key) {
        (Some(x), Some(y)) => x.cmp(y).then_with(|| a.id.cmp(&b.id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => timestamp_ms(Some(&b.created_at))
            .cmp(&timestamp_ms(Some(&a.created_at)))
            .then_with(|| a.id.cmp(&b.id)),
    });
}

fn settled_timestamp(thread: &Thread) -> Option<&str> {
    thread
        .settled_at
        .as_deref()
        .or(thread
            .latest_turn
            .as_ref()
            .and_then(|t| t.completed_at.as_deref()))
        .or(Some(thread.updated_at.as_str()))
}

pub struct Sections<'a> {
    pub pinned: Vec<&'a Thread>,
    pub active: Vec<&'a Thread>,
    pub snoozed: Vec<&'a Thread>,
    pub settled: Vec<&'a Thread>,
}

pub fn sections(threads: &[Thread], now: chrono::DateTime<chrono::Utc>) -> Sections<'_> {
    let mut sections = Sections {
        pinned: Vec::new(),
        active: Vec::new(),
        snoozed: Vec::new(),
        settled: Vec::new(),
    };
    for thread in threads.iter().filter(|t| t.archived_at.is_none()) {
        // Snooze outranks settlement and pinning until the thread wakes.
        if crate::snooze::effective_snoozed(thread, now) {
            sections.snoozed.push(thread);
        } else if thread.settled_override.as_deref() == Some("settled") {
            sections.settled.push(thread);
        } else if thread.pinned_at.is_some() {
            sections.pinned.push(thread);
        } else {
            sections.active.push(thread);
        }
    }
    sort_pinned(&mut sections.pinned);
    sort_active(&mut sections.active);
    // Soonest wake first.
    sections
        .snoozed
        .sort_by_key(|t| timestamp_ms(t.snoozed_until.as_deref()));
    sections.settled.sort_by(|a, b| {
        timestamp_ms(settled_timestamp(b))
            .cmp(&timestamp_ms(settled_timestamp(a)))
            .then_with(|| a.id.cmp(&b.id))
    });
    sections
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Approval,
    Input,
    Working,
    Failed,
    Ready,
}

fn status(thread: &Thread) -> Status {
    if thread.has_pending_approvals {
        Status::Approval
    } else if thread.has_pending_user_input {
        Status::Input
    } else if thread.is_running() || thread.background_liveness.is_some() {
        Status::Working
    } else if thread.session.as_ref().is_some_and(|s| s.status == "error") {
        Status::Failed
    } else {
        Status::Ready
    }
}

impl App {
    /// The sidebar surface, its right border and the toggle + wordmark header,
    /// shared by the app and settings sidebars.
    pub(crate) fn sidebar_chrome(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, theme::SIDEBAR);
        // Right border: 1px of white 7.8% over the app background.
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.right() - 1.0, rect.top()),
                Vec2::new(1.0, rect.height()),
            ),
            0.0,
            Color32::from_rgb(29, 29, 29),
        );
        let left = rect.left();

        // Header: sidebar toggle + wordmark.
        widgets::icon_button(
            ui,
            Rect::from_min_size(Pos2::new(left + 13.0, 12.0), Vec2::splat(28.0)),
            "toggle-sidebar",
            "panel-left-close",
            16.0,
            theme::MUTED_FOREGROUND,
            "Toggle Sidebar",
        );
        let mark = Rect::from_min_size(Pos2::new(left + 52.0, 21.0), Vec2::new(16.5, 9.9));
        egui::Image::new(theme::icon("t3-wordmark"))
            .tint(theme::SIDEBAR_FOREGROUND)
            .paint_at(ui, mark);
        // "Code" is cap-trimmed: its baseline sits on the wordmark's bottom edge.
        let code = widgets::galley(
            ui,
            "Code",
            font(14.0, Weight::Medium),
            theme::SIDEBAR_MUTED,
            20.0,
            f32::INFINITY,
        );
        // Glyph positions are baselines relative to their row.
        let baseline =
            code.rows[0].pos.y + code.rows[0].row.glyphs.first().map_or(15.0, |g| g.pos.y);
        painter.galley(
            Pos2::new(left + 72.5, mark.bottom() - baseline),
            code,
            theme::SIDEBAR_MUTED,
        );
    }

    pub(crate) fn sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        self.sidebar_chrome(ui, rect);
        let painter = ui.painter().clone();
        let left = rect.left();

        // Search field and project actions.
        let search_rect = Rect::from_min_size(Pos2::new(left + 8.0, 60.0), Vec2::new(151.0, 32.0));
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 16.0, 68.0), Vec2::splat(16.0)),
            16.0,
            theme::SIDEBAR_ICON,
        );
        let input_rect = Rect::from_min_size(Pos2::new(left + 40.0, 65.5), Vec2::new(111.0, 21.0));
        let mut search_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        search_ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::SIDEBAR_FOREGROUND)
                .hint_text(
                    egui::RichText::new("Search")
                        .font(font(14.0, Weight::Medium))
                        .color(theme::SIDEBAR_MUTED),
                )
                .desired_width(111.0),
        );
        // Project scope trigger: the scoped project's icon replaces the folder.
        let scope_button = Rect::from_min_size(Pos2::new(left + 163.0, 62.0), Vec2::splat(28.0));
        let scope_open = self.open_menu.as_deref() == Some(project_scope::MENU_ID);
        if scope_open {
            painter.rect_filled(
                scope_button,
                widgets::radius(theme::CONTROL_RADIUS as f32),
                theme::ACCENT,
            );
        }
        let scoped = self.scoped_project().cloned();
        let tooltip = match &scoped {
            Some(project) => format!("Filter threads by project: {}", project.title),
            None => "Filter threads by project".to_owned(),
        };
        let scope_response = match &scoped {
            Some(project) => {
                let response = ui
                    .interact(scope_button, ui.id().with("sidebar-scope"), Sense::click())
                    .on_hover_text(&tooltip);
                if response.hovered() && !scope_open {
                    painter.rect_filled(
                        scope_button,
                        widgets::radius(theme::CONTROL_RADIUS as f32),
                        theme::ACCENT,
                    );
                }
                project_icon::paint(
                    ui,
                    Rect::from_center_size(scope_button.center(), Vec2::splat(16.0)),
                    project,
                );
                response
            }
            None => widgets::icon_button(
                ui,
                scope_button,
                "sidebar-scope",
                "folder",
                16.0,
                if scope_open {
                    theme::FOREGROUND
                } else {
                    theme::SIDEBAR_ICON
                },
                &tooltip,
            ),
        };
        if scope_response.clicked() {
            self.toggle_project_scope_menu();
        }
        let actions = [("folder-plus", "Add project"), ("square-pen", "New thread")];
        for (i, (icon, label)) in actions.iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(left + 191.0 + 28.0 * i as f32, 62.0),
                Vec2::splat(28.0),
            );
            if widgets::icon_button(
                ui,
                button,
                ("sidebar-action", i),
                icon,
                16.0,
                theme::SIDEBAR_ICON,
                label,
            )
            .clicked()
            {
                match *icon {
                    "square-pen" => self.start_draft(None),
                    _ => self.toggle_palette(crate::ui::palette::View::AddProject),
                }
            }
        }
        // The scope popup opens under the whole search row, buttons included.
        self.project_scope_menu(
            ui,
            Rect::from_min_size(search_rect.min, Vec2::new(246.0, 32.0)),
        );

        // Thread list.
        let list_rect = Rect::from_min_max(
            Pos2::new(left, LIST_TOP),
            Pos2::new(rect.right() - 1.0, rect.bottom() - FOOTER_HEIGHT),
        );
        self.thread_list(ui, list_rect);

        // Footer shortcuts.
        let footer = [
            ("settings", "Settings"),
            ("git-pull-request-arrow", "Pull requests"),
            ("chart-no-axes-column", "Usage"),
        ];
        for (i, (icon, label)) in footer.iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(left + 10.0 + 36.0 * i as f32, rect.bottom() - 34.0),
                Vec2::splat(28.0),
            );
            if widgets::icon_button(
                ui,
                button,
                ("sidebar-footer", i),
                icon,
                16.0,
                theme::SIDEBAR_ICON,
                label,
            )
            .clicked()
            {
                if *icon == "settings" {
                    self.open_settings();
                } else {
                    self.status = Some(format!("{label} is not implemented yet"));
                }
            }
        }
    }

    fn thread_list(&mut self, ui: &mut Ui, rect: Rect) {
        let query = self.search.trim().to_lowercase();
        let scope = self.scoped_project().map(|p| p.id.clone());
        let threads: Vec<Thread> = self
            .state
            .threads
            .iter()
            .filter(|t| query.is_empty() || t.title.to_lowercase().contains(&query))
            .filter(|t| scope.as_ref().is_none_or(|id| &t.project_id == id))
            .cloned()
            .collect();
        let sections = sections(&threads, self.now);
        let cards: Vec<&Thread> = sections
            .pinned
            .iter()
            .chain(sections.active.iter())
            .copied()
            .collect();
        let settled = sections.settled.clone();
        let snoozed_count = sections.snoozed.len();
        // A collapsed shelf still shows the open thread's row.
        let snoozed: Vec<&Thread> = sections
            .snoozed
            .iter()
            .filter(|t| self.snoozed_expanded || self.selected.as_deref() == Some(t.id.as_str()))
            .copied()
            .collect();
        let settled_rows = if self.settled_expanded {
            settled.len()
        } else {
            0
        };
        // Shelf headers and rows, 1px apart (`gap-px`).
        let snoozed_block = if snoozed_count > 0 {
            32.0 + snoozed.len() as f32 * (SLIM_HEIGHT + LIST_GAP) + LIST_GAP
        } else {
            0.0
        };
        let tail_height = snoozed_block + 32.0 + settled_rows as f32 * (LIST_GAP + SLIM_HEIGHT);
        let content_height = cards.len() as f32 * CARD_SLOT + tail_height + 4.0;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        egui::ScrollArea::vertical()
            .id_salt("sidebar-threads")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let height = content_height.max(rect.height());
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(rect.width(), height), Sense::hover());
                let mut y = area.top();
                if cards.is_empty() && settled.is_empty() && snoozed_count == 0 {
                    self.empty_list(ui, area);
                    return;
                }
                for thread in &cards {
                    let card = Rect::from_min_size(
                        Pos2::new(area.left() + 8.0, y + 2.0),
                        Vec2::new(area.width() - 16.0, CARD_HEIGHT),
                    );
                    if ui.is_rect_visible(card) {
                        self.thread_card(ui, card, thread);
                    }
                    y += CARD_SLOT;
                }
                // The shelves sink to the bottom (`mt-auto`).
                let mut header_top = y.max(area.bottom() - tail_height);
                if snoozed_count > 0 {
                    let label = if self.snoozed_expanded {
                        "Snoozed".to_owned()
                    } else {
                        format!("Snoozed ({snoozed_count})")
                    };
                    let header = Rect::from_min_size(
                        Pos2::new(area.left() + 10.0, header_top),
                        Vec2::new(area.width() - 20.0, 32.0),
                    );
                    if self
                        .section_header(ui, header, &label, self.snoozed_expanded, true)
                        .clicked()
                    {
                        self.snoozed_expanded = !self.snoozed_expanded;
                    }
                    header_top = header.bottom() + LIST_GAP;
                    for thread in &snoozed {
                        let row = Rect::from_min_size(
                            Pos2::new(area.left() + 8.0, header_top),
                            Vec2::new(area.width() - 16.0, SLIM_HEIGHT),
                        );
                        self.slim_row(ui, row, thread, true);
                        header_top += SLIM_HEIGHT + LIST_GAP;
                    }
                }
                let label = if self.settled_expanded {
                    "Settled".to_owned()
                } else {
                    format!("Settled ({})", settled.len())
                };
                let header = Rect::from_min_size(
                    Pos2::new(area.left() + 10.0, header_top),
                    Vec2::new(area.width() - 20.0, 32.0),
                );
                if self
                    .section_header(ui, header, &label, self.settled_expanded, false)
                    .clicked()
                {
                    self.settled_expanded = !self.settled_expanded;
                }
                let mut y = header.bottom() + LIST_GAP;
                if self.settled_expanded {
                    for thread in &settled {
                        let row = Rect::from_min_size(
                            Pos2::new(area.left() + 8.0, y),
                            Vec2::new(area.width() - 16.0, SLIM_HEIGHT),
                        );
                        self.slim_row(ui, row, thread, false);
                        y += SLIM_HEIGHT + LIST_GAP;
                    }
                }
            });
    }

    fn empty_list(&mut self, ui: &mut Ui, area: Rect) {
        let muted = theme::alpha(theme::MUTED_FOREGROUND, 0.6);
        let center = area.center().x;
        let small = font(12.0, Weight::Regular);
        if self.state.projects.is_empty() {
            let width = widgets::text_width(ui, "No projects yet", small.clone());
            widgets::text(
                ui,
                Pos2::new(center - width / 2.0, area.top() + 24.0),
                "No projects yet",
                small,
                muted,
                16.0,
            );
            let label_font = font(11.0, Weight::Medium);
            let w = widgets::text_width(ui, "Add project", label_font.clone()) + 10.0 + 6.0 + 20.0;
            let button = Rect::from_min_size(
                Pos2::new(center - w / 2.0, area.top() + 48.0),
                Vec2::new(w, 24.0),
            );
            let response = ui.interact(button, ui.id().with("empty-add-project"), Sense::click());
            ui.painter().rect(
                button,
                widgets::radius(6.0),
                if response.hovered() {
                    theme::ACCENT
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::new(1.0, theme::BORDER),
                egui::StrokeKind::Inside,
            );
            theme::paint_icon(
                ui,
                "plus",
                Rect::from_min_size(button.min + Vec2::new(8.0, 6.0), Vec2::splat(12.0)),
                12.0,
                theme::SIDEBAR_MUTED,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(button.min + Vec2::new(24.0, 0.0), button.max),
                "Add project",
                label_font,
                theme::SIDEBAR_MUTED,
            );
            if response.clicked() {
                self.toggle_palette(crate::ui::palette::View::AddProject);
            }
        } else {
            let label = match self.scoped_project() {
                Some(project) => format!("No threads in {} yet", project.title),
                None => "No threads yet".to_owned(),
            };
            let label = label.as_str();
            let width = widgets::text_width(ui, label, small.clone());
            widgets::text(
                ui,
                Pos2::new(center - width / 2.0, area.top() + 24.0),
                label,
                small,
                muted,
                16.0,
            );
        }
    }

    fn section_header(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        label: &str,
        expanded: bool,
        snoozed: bool,
    ) -> egui::Response {
        let response = ui.interact(
            rect,
            ui.id().with(("section", label.split(' ').next())),
            Sense::click(),
        );
        // The snoozed shelf wears the info tone.
        let (color, line) = if snoozed {
            (theme::BLUE_400, theme::alpha(theme::BLUE_500, 0.2))
        } else {
            (theme::alpha(theme::SIDEBAR_MUTED, 0.6), theme::white(0.047))
        };
        let x = rect.left() + 8.0;
        let width = widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
            label,
            font(12.0, Weight::Medium),
            color,
        );
        let line_left = x + width + 8.0;
        let chevron = Rect::from_center_size(
            Pos2::new(rect.right() - 14.0, rect.center().y),
            Vec2::splat(12.0),
        );
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(line_left, rect.center().y.round()),
                Pos2::new(chevron.left() - 8.0, rect.center().y.round() + 1.0),
            ),
            0.0,
            line,
        );
        theme::paint_icon(
            ui,
            if expanded {
                "chevron-up"
            } else {
                "chevron-down"
            },
            chevron,
            12.0,
            color,
        );
        response
    }

    fn thread_card(&mut self, ui: &mut Ui, rect: Rect, thread: &Thread) {
        let response = ui.interact(
            rect,
            ui.id().with(("thread-card", &thread.id)),
            Sense::click(),
        );
        let active = self.selected.as_deref() == Some(thread.id.as_str());
        let status = status(thread);
        let recede = self.recedes(thread, active);
        let unread = self.is_unread(thread);
        let woke = self.woke_at(thread);
        // `group-hover`: the row stays hovered while the pointer is over its
        // own buttons (egui's `hovered()` only reports the topmost widget).
        let hovered = response.contains_pointer();
        let painter = ui.painter().clone();
        if active {
            painter.rect_filled(rect, widgets::radius(8.0), theme::ROW_ACTIVE);
        } else if hovered {
            painter.rect_filled(rect, widgets::radius(8.0), theme::ACCENT);
        }
        let x = rect.left() + 10.0;
        let right = rect.right() - 10.0;
        let top = rect.top() + 8.0;

        // Row 1: project icon, project name, status or time.
        let project = self.state.project(&thread.project_id).cloned();
        if let Some(project) = &project {
            project_icon::paint(
                ui,
                Rect::from_min_size(Pos2::new(x, top + 2.0), Vec2::splat(16.0)),
                project,
            );
        }
        let status_label = match status {
            Status::Approval => Some((
                "Approval",
                "shield-question-mark",
                theme::WARNING_FOREGROUND,
            )),
            Status::Input => Some(("Input", "message-circle-question-mark", theme::INDIGO_300)),
            Status::Working => Some(("Working", "circle-dashed", theme::SKY_400)),
            Status::Failed => Some(("Failed", "circle-alert", theme::RED_300)),
            Status::Ready if woke.is_some() => {
                Some(("Woke", "alarm-clock", theme::WARNING_FOREGROUND))
            }
            Status::Ready if unread => Some(("Done", "circle-check", theme::EMERALD_300)),
            Status::Ready => None,
        };
        let is_woke_status = status_label.is_some_and(|(label, _, _)| label == "Woke");
        let row1 = Rect::from_min_max(Pos2::new(x, top), Pos2::new(right, top + 20.0));
        let snooze_menu = format!("card-snooze:{}", thread.id);
        let snooze_open = self.open_menu.as_deref() == Some(snooze_menu.as_str());
        let show_snooze =
            self.capability("threadSnooze") && crate::snooze::can_snooze(thread, self.now);
        // The visible state owns the slot: actions on hover (or while the
        // snooze menu is open), the status at rest. Woke stays beside them.
        let mut slot_right = right;
        let mut status_width = 0.0;
        if hovered || snooze_open {
            let settle = Rect::from_min_size(Pos2::new(right - 58.0, top), Vec2::new(62.0, 20.0));
            let settle_response =
                ui.interact(settle, ui.id().with(("settle", &thread.id)), Sense::click());
            let color = if settle_response.hovered() {
                theme::FOREGROUND
            } else {
                theme::SIDEBAR_MUTED
            };
            theme::paint_icon(
                ui,
                "check",
                Rect::from_min_size(Pos2::new(settle.left() + 6.0, top + 3.0), Vec2::splat(14.0)),
                14.0,
                color,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(settle.left() + 24.0, top), settle.max),
                "Settle",
                font(12.0, Weight::Regular),
                color,
            );
            if settle_response.clicked() {
                self.dispatch(crate::model::command("thread.settle", &thread.id));
            }
            slot_right = settle.left();
            if show_snooze {
                let clock = Rect::from_min_size(
                    Pos2::new(settle.left() - 24.0, top),
                    Vec2::new(24.0, 20.0),
                );
                let clock_response = ui
                    .interact(clock, ui.id().with(("snooze", &thread.id)), Sense::click())
                    .on_hover_text("Snooze thread");
                theme::paint_icon(
                    ui,
                    "clock",
                    clock,
                    12.0,
                    if clock_response.hovered() || snooze_open {
                        theme::FOREGROUND
                    } else {
                        theme::SIDEBAR_MUTED
                    },
                );
                if clock_response.clicked() {
                    self.open_menu = if snooze_open {
                        None
                    } else {
                        Some(snooze_menu.clone())
                    };
                }
                self.card_snooze_menu(ui, clock, thread, &snooze_menu);
                slot_right = clock.left();
            }
            status_width = right - slot_right + 4.0;
        }
        if let Some((label, icon, color)) = status_label
            && (!(hovered || snooze_open) || is_woke_status)
        {
            let width = widgets::text_width(ui, label, font(12.0, Weight::Medium));
            let start = slot_right - width - 20.0;
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(start, top + 2.0), Vec2::splat(16.0)),
                16.0,
                color,
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(start + 20.0, top),
                    Pos2::new(slot_right, row1.bottom()),
                ),
                label,
                font(12.0, Weight::Medium),
                color,
            );
            status_width = right - start;
            if is_woke_status {
                let badge =
                    Rect::from_min_max(Pos2::new(start, top), Pos2::new(slot_right, row1.bottom()));
                let badge_response = ui
                    .interact(badge, ui.id().with(("woke", &thread.id)), Sense::click())
                    .on_hover_text("Dismiss Woke notification");
                if badge_response.clicked()
                    && let Some(at) = &woke
                {
                    self.mark_visited(&thread.id, at);
                }
            }
        } else if !(hovered || snooze_open) {
            let time = widgets::relative_time(
                thread
                    .latest_user_message_at
                    .as_deref()
                    .unwrap_or(&thread.updated_at),
                self.now,
            );
            let width = widgets::text_width(ui, &time, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(right - width, top), row1.max),
                &time,
                font(12.0, Weight::Regular),
                theme::SECONDARY_LABEL,
            );
            status_width = width;
        }
        if let Some(project) = &project {
            let name_rect = Rect::from_min_max(
                Pos2::new(x + 22.0, top),
                Pos2::new(right - status_width - 6.0, top + 20.0),
            );
            widgets::text_in(
                ui,
                name_rect,
                &project.title,
                font(
                    12.0,
                    if recede {
                        Weight::Regular
                    } else {
                        Weight::Medium
                    },
                ),
                theme::SECONDARY_LABEL,
            );
        }

        // Row 2: title.
        let title_color = if recede {
            theme::SECONDARY_LABEL
        } else if unread || woke.is_some() || status == Status::Input {
            theme::SIDEBAR_FOREGROUND
        } else if status == Status::Failed {
            theme::alpha(theme::SIDEBAR_FOREGROUND, 0.95)
        } else {
            theme::alpha(theme::SIDEBAR_FOREGROUND, 0.9)
        };
        let title_rect = Rect::from_min_max(Pos2::new(x, top + 24.0), Pos2::new(right, top + 44.0));
        if !self.rename_editor(ui, &thread.id, title_rect) {
            widgets::text_in(
                ui,
                title_rect,
                &thread.title,
                font(
                    14.0,
                    if recede {
                        Weight::Regular
                    } else {
                        Weight::Medium
                    },
                ),
                title_color,
            );
        }

        // Row 3: branch, diff stat, provider glyph.
        let row3 = Rect::from_min_max(Pos2::new(x, top + 46.0), Pos2::new(right, top + 62.0));
        // Upstream `latestTurnDiff` is a seam that always returns null for shells.
        let branch_right = right - 20.0;
        if let Some(branch) = &thread.branch {
            widgets::text_in(
                ui,
                Rect::from_min_max(row3.min, Pos2::new(branch_right, row3.bottom())),
                branch,
                font(12.0, Weight::Regular),
                theme::alpha(theme::SIDEBAR_MUTED, 0.4),
            );
        }
        if self
            .provider_driver(&thread.model_selection.instance_id)
            .as_deref()
            == Some("codex")
        {
            theme::paint_icon(
                ui,
                "openai",
                Rect::from_min_size(Pos2::new(right - 14.0, row3.top() + 1.0), Vec2::splat(14.0)),
                14.0,
                theme::alpha(theme::SECONDARY_LABEL, 0.6),
            );
        }

        if response.clicked() {
            self.thread_row_clicked(ui, thread);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.open_thread_menu(&thread.id, at);
        }
    }

    /// `SnoozeMenuButton`'s popup: presets resolved at open time, then Custom….
    fn card_snooze_menu(&mut self, ui: &mut Ui, anchor: Rect, thread: &Thread, id: &str) {
        if self.open_menu.as_deref() != Some(id) {
            return;
        }
        let presets = crate::snooze::presets(self.now.with_timezone(&chrono::Local));
        let mut entries: Vec<widgets::MenuEntry> = presets
            .iter()
            .map(|preset| {
                widgets::MenuEntry::Item(widgets::MenuItem {
                    shortcut: Some(&preset.when),
                    ..widgets::MenuItem::new(preset.label)
                })
            })
            .collect();
        entries.push(widgets::MenuEntry::Separator);
        entries.push(widgets::MenuEntry::Item(widgets::MenuItem::new("Custom…")));
        let chosen =
            widgets::menu_aligned(ui, &mut self.open_menu, id, anchor, false, true, &entries);
        match chosen {
            Some(index) if index < presets.len() => {
                let mut cmd = crate::model::command("thread.snooze", &thread.id);
                cmd["snoozedUntil"] = serde_json::json!(crate::snooze::until_iso(&presets[index]));
                self.dispatch(cmd);
            }
            Some(_) => self.status = Some("Custom snooze is not implemented yet".into()),
            None => {}
        }
    }

    /// `shouldRecedeSidebarThread`: quiet rows drop to regular weight.
    fn recedes(&self, thread: &Thread, active: bool) -> bool {
        if active {
            return false;
        }
        match status(thread) {
            Status::Working => true,
            Status::Ready | Status::Approval => {
                !self.is_unread(thread) && self.woke_at(thread).is_none()
            }
            Status::Input | Status::Failed => false,
        }
    }

    /// Settled and snoozed rows (`variant="slim"`): favicon, title and the
    /// settled age or the wake countdown; hover offers un-settle or wake.
    fn slim_row(&mut self, ui: &mut Ui, rect: Rect, thread: &Thread, snoozed: bool) {
        let response = ui.interact(
            rect,
            ui.id().with(("thread-slim", &thread.id)),
            Sense::click(),
        );
        let active = self.selected.as_deref() == Some(thread.id.as_str());
        // `group-hover`, kept while the pointer is over the row's buttons.
        let hovered = response.contains_pointer();
        if active {
            ui.painter()
                .rect_filled(rect, widgets::radius(8.0), theme::ROW_ACTIVE);
        } else if hovered {
            ui.painter()
                .rect_filled(rect, widgets::radius(8.0), theme::ACCENT);
        }
        let x = rect.left() + 10.0;
        if let Some(project) = self.state.project(&thread.project_id).cloned() {
            let icon_rect =
                Rect::from_min_size(Pos2::new(x, rect.center().y - 8.0), Vec2::splat(16.0));
            // Parked rows recede: dimmed, grayscale favicon at rest.
            if (!active || !snoozed) && !hovered {
                project_icon::paint_dimmed(ui, icon_rect, &project);
            } else {
                project_icon::paint(ui, icon_rect, &project);
            }
        }
        let woke = self.woke_at(thread);
        let recede = self.recedes(thread, active);
        let small = font(12.0, Weight::Regular);
        // Right slot: wake countdown, Woke badge or settled age.
        let (label, label_font, label_color) =
            if let (true, Some(until)) = (snoozed, thread.snoozed_until.as_deref()) {
                (
                    crate::snooze::wake_label(until, self.now),
                    small.clone(),
                    theme::BLUE_400,
                )
            } else if woke.is_some() {
                (
                    "Woke".to_owned(),
                    font(12.0, Weight::Medium),
                    theme::WARNING_FOREGROUND,
                )
            } else {
                (
                    widgets::relative_time(settled_timestamp(thread).unwrap_or_default(), self.now),
                    small.clone(),
                    theme::alpha(theme::SECONDARY_LABEL, 0.7),
                )
            };
        let woke_badge = woke.is_some() && !snoozed;
        let label_width = widgets::text_width(ui, &label, label_font.clone())
            + if woke_badge { 16.0 } else { 0.0 };
        // The action button sits in the slot's right edge (`-mr-1`); a Woke
        // badge stays visible and the button moves in beside it.
        let action_width = if snoozed { 24.0 } else { 26.0 };
        let action_right = rect.right() - 6.0;
        let label_right = if hovered && woke_badge {
            action_right - action_width
        } else {
            rect.right() - 10.0
        };
        // Settled rows dim everything at rest; snoozed rows keep the wake,
        // input and unread emphasis.
        let emphasized = snoozed && !recede && (woke.is_some() || status(thread) == Status::Input);
        let title_color = if hovered || active || emphasized {
            theme::FOREGROUND
        } else if snoozed && !recede && self.is_unread(thread) {
            theme::MUTED_FOREGROUND
        } else {
            theme::alpha(theme::SECONDARY_LABEL, 0.7)
        };
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(x + 26.0, rect.top()),
                Pos2::new(label_right - label_width.max(32.0) - 8.0, rect.bottom()),
            ),
            &thread.title,
            font(
                14.0,
                if recede {
                    Weight::Regular
                } else {
                    Weight::Medium
                },
            ),
            title_color,
        );
        if !hovered || woke_badge {
            let left = label_right - label_width;
            if woke_badge {
                theme::paint_icon(
                    ui,
                    "alarm-clock",
                    Rect::from_min_size(Pos2::new(left, rect.center().y - 6.0), Vec2::splat(12.0)),
                    12.0,
                    label_color,
                );
            }
            let text_left = if woke_badge { left + 16.0 } else { left };
            let badge = Rect::from_min_max(
                Pos2::new(left, rect.top() + 6.0),
                Pos2::new(label_right, rect.bottom() - 6.0),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(text_left, rect.top()),
                    Pos2::new(label_right, rect.bottom()),
                ),
                &label,
                label_font,
                label_color,
            );
            if woke_badge {
                let badge_response = ui
                    .interact(badge, ui.id().with(("woke", &thread.id)), Sense::click())
                    .on_hover_text("Dismiss Woke notification");
                if badge_response.clicked()
                    && let Some(at) = &woke
                {
                    self.mark_visited(&thread.id, at);
                }
            }
        }
        if hovered {
            let button = Rect::from_min_max(
                Pos2::new(action_right - action_width, rect.center().y - 12.0),
                Pos2::new(action_right, rect.center().y + 12.0),
            );
            let (id, icon, size, tooltip) = if snoozed {
                ("wake", "alarm-clock-off", 12.0, "Wake thread now")
            } else {
                ("unsettle", "undo-2", 14.0, "Un-settle thread")
            };
            let button_response = ui
                .interact(button, ui.id().with((id, &thread.id)), Sense::click())
                .on_hover_text(tooltip);
            theme::paint_icon(
                ui,
                icon,
                button.translate(Vec2::new(0.0, -0.5)),
                size,
                if button_response.hovered() {
                    theme::FOREGROUND
                } else {
                    theme::MUTED_FOREGROUND
                },
            );
            if button_response.clicked() {
                let kind = if snoozed {
                    "thread.unsnooze"
                } else {
                    "thread.unsettle"
                };
                let mut cmd = crate::model::command(kind, &thread.id);
                cmd["reason"] = serde_json::json!("user");
                self.dispatch(cmd);
            }
        }
        if response.clicked() {
            self.thread_row_clicked(ui, thread);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.open_thread_menu(&thread.id, at);
        }
    }
}

impl App {
    /// Select on click; a second click on the same row within 400ms renames
    /// it (upstream renames on double click).
    fn thread_row_clicked(&mut self, ui: &Ui, thread: &Thread) {
        let now = ui.input(|i| i.time);
        let double = self
            .last_row_click
            .as_ref()
            .is_some_and(|(id, at)| *id == thread.id && now - at < 0.4);
        if double {
            self.last_row_click = None;
            self.start_rename(thread);
        } else {
            self.last_row_click = Some((thread.id.clone(), now));
            self.select_thread(Some(thread.id.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, created: &str) -> Thread {
        Thread {
            id: id.into(),
            created_at: created.into(),
            updated_at: created.into(),
            ..Default::default()
        }
    }

    #[test]
    fn active_threads_lead_with_newest_unkeyed() {
        let a = thread("a", "2026-01-01T00:00:00.000Z");
        let b = thread("b", "2026-01-02T00:00:00.000Z");
        let mut c = thread("c", "2026-01-03T00:00:00.000Z");
        c.active_order_key = Some("a0".into());
        let mut list = vec![&a, &c, &b];
        sort_active(&mut list);
        let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["b", "a", "c"]);
    }

    #[test]
    fn snoozed_threads_leave_every_other_section() {
        let now = chrono::Utc::now();
        let iso = |minutes: i64| {
            (now + chrono::Duration::minutes(minutes))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        };
        let mut later = thread("later", "2026-01-01T00:00:00.000Z");
        later.snoozed_until = Some(iso(120));
        later.pinned_at = Some("2026-01-01T00:00:00.000Z".into());
        let mut sooner = thread("sooner", "2026-01-02T00:00:00.000Z");
        sooner.snoozed_until = Some(iso(30));
        sooner.settled_override = Some("settled".into());
        let mut elapsed = thread("elapsed", "2026-01-03T00:00:00.000Z");
        elapsed.snoozed_until = Some(iso(-1));
        let threads = [later, sooner, elapsed];
        let sections = sections(&threads, now);
        let ids = |list: &[&Thread]| list.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&sections.snoozed), ["sooner", "later"]);
        assert_eq!(ids(&sections.active), ["elapsed"]);
        assert!(sections.pinned.is_empty() && sections.settled.is_empty());
    }
}
