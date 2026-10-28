//! Sidebar (`Sidebar.tsx`): wordmark, search, thread cards grouped into the
//! pinned/active list and the settled shelf, footer shortcuts.
//!
//! Collapsing follows `ui/sidebar.tsx` and `AppSidebarLayout.tsx`: the toggle
//! (or `sidebar.toggle`) hides the docked sidebar and a floating trigger takes
//! its place; below `md` (768px) the sidebar opens as a sheet over a dimmed
//! workspace and closes once a destination is picked. The sheet keeps the
//! docked width rather than upstream's near full width, since this sidebar's
//! layout is fixed to 16rem.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

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

/// Pluto: when `thread` last worked, the latest of its creation, its
/// return from the settled shelf, the user's last message, the last reply
/// and the end of its last run. The card shows it.
pub(crate) fn activity_at(thread: &Thread) -> &str {
    [
        thread.unsettled_at.as_deref(),
        thread.latest_user_message_at.as_deref(),
        thread
            .latest_visible_message
            .as_ref()
            .map(|message| message.updated_at.as_str()),
        thread
            .latest_run
            .as_ref()
            .and_then(|run| run.completed_at.as_deref()),
    ]
    .into_iter()
    .flatten()
    .fold(thread.created_at.as_str(), |latest, at| {
        if timestamp_ms(Some(at)) > timestamp_ms(Some(latest)) {
            at
        } else {
            latest
        }
    })
}

/// Pluto, after Claude Code rather than `sortActiveThreadsByOrderKey`: the
/// threads working now first, then the latest to have worked. Keys are
/// computed once per thread: parsing timestamps in every comparison added
/// up every frame.
pub fn sort_active(threads: &mut [&Thread]) {
    threads.sort_by_cached_key(|&t| {
        (
            t.active_run_id.is_none(),
            std::cmp::Reverse(timestamp_ms(Some(activity_at(t)))),
            t.id.as_str(),
        )
    });
}

/// `sortPinnedThreadsByOrderKey`: keyed threads by key, then keyless by creation time.
pub fn sort_pinned(threads: &mut [&Thread]) {
    threads.sort_by_cached_key(|&t| {
        let created = match t.pin_order_key {
            Some(_) => 0,
            None => timestamp_ms(Some(&t.created_at)),
        };
        (
            t.pin_order_key.is_none(),
            t.pin_order_key.as_deref(),
            std::cmp::Reverse(created),
            t.id.as_str(),
        )
    });
}

fn settled_timestamp(thread: &Thread) -> Option<&str> {
    thread
        .settled_at
        .as_deref()
        .or(thread
            .latest_run
            .as_ref()
            .and_then(|r| r.completed_at.as_deref()))
        .or(Some(thread.updated_at.as_str()))
}

pub struct Sections<'a> {
    pub pinned: Vec<&'a Thread>,
    pub active: Vec<&'a Thread>,
    pub snoozed: Vec<&'a Thread>,
    pub settled: Vec<&'a Thread>,
}

pub fn sections<'a>(
    threads: impl IntoIterator<Item = &'a Thread>,
    now: chrono::DateTime<chrono::Utc>,
) -> Sections<'a> {
    let mut sections = Sections {
        pinned: Vec::new(),
        active: Vec::new(),
        snoozed: Vec::new(),
        settled: Vec::new(),
    };
    // `filterSidebarV2VisibleThreads`: subagent rows stay hidden; their
    // requests show on the threads above them.
    for thread in threads
        .into_iter()
        .filter(|t| t.archived_at.is_none() && !t.is_subagent())
    {
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
        .sort_by_cached_key(|t| timestamp_ms(t.snoozed_until.as_deref()));
    sections.settled.sort_by_cached_key(|&t| {
        (
            std::cmp::Reverse(timestamp_ms(settled_timestamp(t))),
            t.id.as_str(),
        )
    });
    sections
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Status {
    Approval,
    Input,
    Working,
    /// Stopped, with background work that will wake it (runtime `idle`).
    Waiting,
    Failed,
    /// Failed on a usage limit.
    Limited,
    Ready,
}

/// `resolveSidebarThreadStatus`.
pub(crate) fn status(thread: &Thread) -> Status {
    let subagent = thread.pending_subagent_request.as_deref();
    let runtime = thread.runtime.as_ref();
    if thread.has_pending_approvals || subagent == Some("approval") {
        Status::Approval
    } else if thread.has_pending_user_input || subagent == Some("user_input") {
        Status::Input
    } else if runtime.is_some_and(|r| r.is_active()) {
        Status::Working
    } else if runtime.is_some_and(|r| r.status == "idle") {
        Status::Waiting
    } else if let Some(runtime) = runtime.filter(|r| r.status == "failed") {
        if runtime.last_error_class.as_deref() == Some("usage_limit") {
            Status::Limited
        } else {
            Status::Failed
        }
    } else {
        Status::Ready
    }
}

impl App {
    /// The sidebar surface, its right border and the toggle + wordmark header,
    /// shared by the app and settings sidebars.
    pub(crate) fn sidebar_chrome(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, theme::sidebar());
        // Right border: 1px of white 7.8% over the app background.
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.right() - 1.0, rect.top()),
                Vec2::new(1.0, rect.height()),
            ),
            0.0,
            theme::sidebar_border(),
        );
        let left = rect.left();

        // Header: sidebar toggle + wordmark.
        let toggle = widgets::icon_button(
            ui,
            Rect::from_min_size(Pos2::new(left + 13.0, 12.0), Vec2::splat(28.0)),
            "toggle-sidebar",
            "panel-left-close",
            16.0,
            theme::muted_foreground(),
            "Toggle Sidebar",
        );
        // The settings sidebar does not collapse.
        if toggle.clicked() && self.settings_page.is_none() {
            self.toggle_sidebar();
        }
        // Pluto's wordmark (in place of T3's mark and "Code"), its baseline
        // where T3's wordmark sat.
        let wordmark = widgets::galley(
            ui,
            "Pluto",
            font(14.0, Weight::SemiBold),
            theme::sidebar_foreground(),
            20.0,
            f32::INFINITY,
        );
        // Glyph positions are baselines relative to their row.
        let baseline = wordmark.rows[0].pos.y
            + wordmark.rows[0]
                .row
                .glyphs
                .first()
                .map_or(15.0, |g| g.pos.y);
        painter.galley(
            Pos2::new(left + 52.0, 30.9 - baseline),
            wordmark,
            theme::sidebar_foreground(),
        );
    }

    /// The footer's Back row of the settings and usage sidebars; returns
    /// whether it was clicked.
    pub(crate) fn sidebar_back(&mut self, ui: &mut Ui, rect: Rect, id: &str) -> bool {
        let left = rect.left();
        let back = Rect::from_min_size(
            Pos2::new(left + 8.0, rect.bottom() - 36.0),
            Vec2::new(239.0, 32.0),
        );
        let response = ui.interact(back, ui.id().with(id), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(
                back,
                CornerRadius::same(8),
                theme::alpha(theme::sidebar_foreground(), 0.08),
            );
        }
        // Back rests like an inactive item and lights up on hover.
        let (back_icon, back_text) = if response.hovered() {
            (theme::sidebar_foreground(), theme::sidebar_foreground())
        } else {
            (
                theme::sidebar_icon(),
                theme::alpha(theme::sidebar_muted(), 0.8),
            )
        };
        theme::paint_icon(
            ui,
            "arrow-left",
            Rect::from_min_size(
                Pos2::new(back.left() + 10.0, back.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            back_icon,
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(back.left() + 34.0, back.top()), back.max),
            "Back",
            font(14.0, Weight::Medium),
            back_text,
        );
        response.clicked()
    }

    /// Whether the sidebar takes its column in the layout.
    pub(crate) fn sidebar_docked(&self) -> bool {
        !self.mobile && self.sidebar_open
    }

    /// `toggleSidebar`: the sheet below `md`, the docked sidebar above.
    pub(crate) fn toggle_sidebar(&mut self) {
        if self.mobile {
            self.sidebar_sheet_open = !self.sidebar_sheet_open;
        } else {
            self.sidebar_open = !self.sidebar_open;
        }
    }

    /// The hidden sidebar: the floating trigger, or the open sheet over a
    /// dimmed window (`bg-background/60`).
    pub(crate) fn sidebar_sheet(&mut self, ui: &mut Ui, window: Rect, rect: Rect) {
        if !self.sidebar_sheet_open {
            let trigger = widgets::icon_button(
                ui,
                Rect::from_min_size(Pos2::new(window.left() + 13.0, 12.0), Vec2::splat(28.0)),
                "open-sidebar",
                "panel-left-open",
                16.0,
                theme::muted_foreground(),
                "Toggle main sidebar",
            );
            if trigger.clicked() {
                self.toggle_sidebar();
            }
            return;
        }
        let backdrop = ui.interact(window, ui.id().with("sidebar-backdrop"), Sense::click());
        ui.painter()
            .rect_filled(window, 0.0, theme::alpha(theme::background(), 0.6));
        let destination = |app: &App| {
            (
                app.selected.clone(),
                app.draft_project().map(|p| p.id),
                app.usage_open,
                app.pull_requests_open,
                app.settings_page.is_some(),
            )
        };
        let before = destination(self);
        // The sheet sits over the backdrop, so it takes the pointer first.
        ui.interact(rect, ui.id().with("sidebar-sheet"), Sense::click());
        self.sidebar(ui, rect);
        let escape = self.open_menu.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if backdrop.clicked() || escape || destination(self) != before {
            self.sidebar_sheet_open = false;
        }
    }

    pub(crate) fn sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        self.sidebar_chrome(ui, rect);
        // Pluto: Code | Chat (`crate::ui::chat_mode`).
        self.mode_switch(ui, rect);
        if self.chat_mode() {
            self.chat_sidebar(ui, rect);
        } else if self.client_setting("legacySidebarEnabled") == true {
            self.legacy_sidebar(ui, rect);
        } else {
            self.inbox_sidebar(ui, rect);
        }
        self.sidebar_footer(ui, rect);
    }

    /// The default sidebar's search row and thread inbox.
    fn inbox_sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter().clone();
        let left = rect.left();

        // Search field and project actions.
        let search_rect = Rect::from_min_size(Pos2::new(left + 8.0, 60.0), Vec2::new(151.0, 32.0));
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 16.0, 68.0), Vec2::splat(16.0)),
            16.0,
            theme::sidebar_icon(),
        );
        let input_rect = Rect::from_min_size(Pos2::new(left + 40.0, 65.5), Vec2::new(111.0, 21.0));
        let mut search_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        search_ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::sidebar_foreground())
                .hint_text(
                    egui::RichText::new("Search")
                        .font(font(14.0, Weight::Medium))
                        .color(theme::sidebar_muted()),
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
                theme::accent(),
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
                        theme::accent(),
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
                    theme::foreground()
                } else {
                    theme::sidebar_icon()
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
                theme::sidebar_icon(),
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
    }

    /// Footer shortcuts; the usage page swaps them for Back.
    fn sidebar_footer(&mut self, ui: &mut Ui, rect: Rect) {
        let left = rect.left();
        if self.usage_open || self.pull_requests_open || self.design_home.open {
            if self.sidebar_back(ui, rect, "page-back") {
                self.usage_open = false;
                self.pull_requests_open = false;
                self.design_home.open = false;
            }
            return;
        }
        let footer = [
            ("settings", "Settings"),
            ("git-pull-request-arrow", "Pull requests"),
            ("chart-no-axes-column", "Usage"),
            // Pluto: a design session (`crate::ui::design_home`).
            ("palette", "New design"),
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
                theme::sidebar_icon(),
                label,
            )
            .clicked()
            {
                if *icon == "settings" {
                    self.open_settings();
                } else if *icon == "chart-no-axes-column" {
                    self.open_usage();
                } else if *icon == "git-pull-request-arrow" {
                    self.open_pull_requests();
                } else if *icon == "palette" {
                    self.open_design_home();
                } else {
                    self.status = Some(format!("{label} is not implemented yet"));
                }
            }
        }
    }

    fn thread_list(&mut self, ui: &mut Ui, rect: Rect) {
        let query = self.search.trim().to_lowercase();
        let scope = self.scoped_project().map(|p| p.id.clone());
        let threads: Vec<std::sync::Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| query.is_empty() || t.title.to_lowercase().contains(&query))
            .filter(|t| scope.as_ref().is_none_or(|id| &t.project_id == id))
            .filter(|t| !self.is_chat_thread(t))
            .cloned()
            .collect();
        let sections = sections(threads.iter().map(std::sync::Arc::as_ref), self.now);
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
        let muted = theme::alpha(theme::muted_foreground(), 0.6);
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
                    theme::accent()
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::new(1.0, theme::border()),
                egui::StrokeKind::Inside,
            );
            theme::paint_icon(
                ui,
                "plus",
                Rect::from_min_size(button.min + Vec2::new(8.0, 6.0), Vec2::splat(12.0)),
                12.0,
                theme::sidebar_muted(),
            );
            widgets::text_in(
                ui,
                Rect::from_min_max(button.min + Vec2::new(24.0, 0.0), button.max),
                "Add project",
                label_font,
                theme::sidebar_muted(),
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
            (
                theme::alpha(theme::sidebar_muted(), 0.6),
                theme::overlay(0.047),
            )
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
            painter.rect_filled(rect, widgets::radius(8.0), theme::row_active());
        } else if hovered {
            painter.rect_filled(rect, widgets::radius(8.0), theme::accent());
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
                theme::warning_foreground(),
            )),
            Status::Input => Some(("Input", "message-circle-question-mark", theme::INDIGO_300)),
            // A native /goal keeps the agent going across turns.
            Status::Working
                if thread
                    .goal
                    .as_ref()
                    .is_some_and(|g| g["status"] == "active") =>
            {
                Some(("Goal", "circle-dashed", theme::SKY_400))
            }
            Status::Working => Some(("Working", "circle-dashed", theme::SKY_400)),
            Status::Waiting => Some(("Waiting", "circle-dot-dashed", theme::muted_foreground())),
            Status::Limited => Some(("Limited", "circle-alert", theme::warning())),
            Status::Failed => Some(("Failed", "circle-alert", theme::RED_300)),
            Status::Ready if woke.is_some() => {
                Some(("Woke", "alarm-clock", theme::warning_foreground()))
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
                theme::foreground()
            } else {
                theme::sidebar_muted()
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
                        theme::foreground()
                    } else {
                        theme::sidebar_muted()
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
            let time = widgets::relative_time(activity_at(thread), self.now);
            let width = widgets::text_width(ui, &time, font(12.0, Weight::Regular));
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(right - width, top), row1.max),
                &time,
                font(12.0, Weight::Regular),
                theme::secondary_label(),
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
                theme::secondary_label(),
            );
        }

        // Row 2: title.
        let title_color = if recede {
            theme::secondary_label()
        } else if unread || woke.is_some() || status == Status::Input {
            theme::sidebar_foreground()
        } else if matches!(status, Status::Failed | Status::Limited) {
            theme::alpha(theme::sidebar_foreground(), 0.95)
        } else {
            theme::alpha(theme::sidebar_foreground(), 0.9)
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
                theme::alpha(theme::sidebar_muted(), 0.4),
            );
        }
        if let Some((icon, tint)) = self
            .provider_driver(&thread.model_selection.instance_id)
            .and_then(|driver| crate::ui::pickers::provider_icon(&driver))
        {
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(right - 14.0, row3.top() + 1.0), Vec2::splat(14.0)),
                14.0,
                theme::alpha(tint, 0.6),
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
            Some(_) => self.request_custom_snooze(vec![thread.id.clone()]),
            None => {}
        }
    }

    /// `shouldRecedeSidebarThread`: quiet rows drop to regular weight.
    fn recedes(&self, thread: &Thread, active: bool) -> bool {
        if active {
            return false;
        }
        match status(thread) {
            Status::Working | Status::Waiting => true,
            Status::Ready | Status::Approval => {
                !self.is_unread(thread) && self.woke_at(thread).is_none()
            }
            Status::Input | Status::Failed | Status::Limited => false,
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
                .rect_filled(rect, widgets::radius(8.0), theme::row_active());
        } else if hovered {
            ui.painter()
                .rect_filled(rect, widgets::radius(8.0), theme::accent());
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
                    theme::warning_foreground(),
                )
            } else {
                (
                    widgets::relative_time(settled_timestamp(thread).unwrap_or_default(), self.now),
                    small.clone(),
                    theme::alpha(theme::secondary_label(), 0.7),
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
            theme::foreground()
        } else if snoozed && !recede && self.is_unread(thread) {
            theme::muted_foreground()
        } else {
            theme::alpha(theme::secondary_label(), 0.7)
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
                    theme::foreground()
                } else {
                    theme::muted_foreground()
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
    pub(crate) fn thread_row_clicked(&mut self, ui: &Ui, thread: &Thread) {
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
    fn active_threads_lead_with_the_working_then_the_latest_to_work() {
        let old = thread("old", "2026-01-01T00:00:00.000Z");
        let new = thread("new", "2026-01-05T00:00:00.000Z");
        // Created first, but the user wrote in it last.
        let mut talked = thread("talked", "2026-01-02T00:00:00.000Z");
        talked.latest_user_message_at = Some("2026-01-06T00:00:00.000Z".into());
        // Its run finished after that message.
        let mut ran = thread("ran", "2026-01-03T00:00:00.000Z");
        ran.latest_run = Some(crate::model::RunSummary {
            completed_at: Some("2026-01-07T00:00:00.000Z".into()),
            ..Default::default()
        });
        let mut working = thread("working", "2026-01-01T12:00:00.000Z");
        working.active_run_id = Some("run".into());
        let mut list = vec![&old, &new, &talked, &ran, &working];
        sort_active(&mut list);
        let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["working", "ran", "talked", "new", "old"]);
        assert_eq!(activity_at(&ran), "2026-01-07T00:00:00.000Z");
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
