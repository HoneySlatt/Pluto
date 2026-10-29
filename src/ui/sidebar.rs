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
// Pluto: header (52), Code | Chat (36 + 12), New and Search (2 × 34), then
// the project scope row (28 + 8).
pub(crate) const NEW_TOP: f32 = 100.0;
const SCOPE_TOP: f32 = 172.0;
const LIST_TOP: f32 = 208.0;
/// The range the sidebar can be dragged to.
const SIDEBAR_MIN_WIDTH: f32 = 220.0;
const SIDEBAR_MAX_WIDTH: f32 = 420.0;
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

// Pinned order keys (`threadSort.ts`): base-26 fractions in (0, 1), compared
// as plain strings; a key never ends with the minimum digit.
const PIN_DIGITS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

fn valid_pin_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| PIN_DIGITS.contains(&b)) && !key.ends_with('a')
}

/// `pinOrderMidpoint`: a key strictly between `a` and `b` ("" is open).
fn pin_midpoint(a: &str, b: &str) -> String {
    let digit = |s: &str, i: usize| s.as_bytes().get(i).copied();
    if !b.is_empty() {
        let mut n = 0;
        while digit(a, n).unwrap_or(b'a') == digit(b, n).unwrap_or(0) {
            n += 1;
        }
        if n > 0 {
            return format!(
                "{}{}",
                &b[..n],
                pin_midpoint(a.get(n..).unwrap_or(""), &b[n..])
            );
        }
    }
    let index = |c: u8| PIN_DIGITS.iter().position(|&d| d == c).unwrap_or(0);
    let low = digit(a, 0).map_or(0, index);
    let high = digit(b, 0).map_or(PIN_DIGITS.len(), index);
    if high - low > 1 {
        return (PIN_DIGITS[(low + high).div_ceil(2).min(25)] as char).to_string();
    }
    if b.len() > 1 {
        return b[..1].to_owned();
    }
    format!(
        "{}{}",
        PIN_DIGITS[low] as char,
        pin_midpoint(a.get(1..).unwrap_or(""), "")
    )
}

/// `topOfPinnedRunOrderKey`: a key before every keyed pinned thread, so a
/// new pin lands at the top; `None` when the keys are corrupt.
pub(crate) fn top_pin_key(threads: &[std::sync::Arc<Thread>]) -> Option<String> {
    let first = threads
        .iter()
        .filter(|t| t.pinned_at.is_some())
        .filter_map(|t| t.pin_order_key.as_deref())
        .min();
    match first {
        Some(first) if !valid_pin_key(first) => None,
        Some(first) => Some(pin_midpoint("", first)),
        None => Some(pin_midpoint("", "")),
    }
}

impl App {
    /// The pen of a thread with an unsent draft (`text-warning-foreground`).
    fn draft_pen(&self, ui: &Ui, at: Pos2, thread_id: &str) {
        let rect = Rect::from_min_size(at, Vec2::splat(12.0));
        theme::paint_icon(ui, "square-pen", rect, 12.0, theme::warning_foreground());
        ui.interact(rect, ui.id().with(("draft-pen", thread_id)), Sense::hover())
            .on_hover_text("Unsent draft");
    }

    /// A pinned thread's pin; it unpins on a click where pinning is offered,
    /// showing pin-off on hover.
    fn pin_glyph(&mut self, ui: &Ui, at: Pos2, thread: &Thread) {
        let rect = Rect::from_min_size(at, Vec2::splat(12.0));
        let muted = theme::alpha(theme::muted_foreground(), 0.65);
        if !self.capability("threadPinning") {
            theme::paint_icon(ui, "pin", rect, 12.0, muted);
            ui.interact(rect, ui.id().with(("pin", &thread.id)), Sense::hover())
                .on_hover_text("Pinned");
            return;
        }
        let response = ui
            .interact(
                rect.expand(2.0),
                ui.id().with(("pin", &thread.id)),
                Sense::click(),
            )
            .on_hover_text("Unpin thread");
        let (icon, color) = if response.hovered() {
            ("pin-off", theme::foreground())
        } else {
            ("pin", muted)
        };
        theme::paint_icon(ui, icon, rect, 12.0, color);
        if response.clicked() {
            self.unpin(thread);
        }
    }
}

/// `formatWorkingDurationLabel`: "42s", "5m", "1h 5m".
fn working_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

/// `resolveSettledThreadTimestamp`: when it was settled, else its latest
/// activity (user message, run requested, started or completed), else its
/// update.
fn settled_timestamp(thread: &Thread) -> Option<&str> {
    fn parsed(at: &Option<String>) -> Option<(chrono::DateTime<chrono::Utc>, &str)> {
        at.as_deref()
            .and_then(|s| crate::timeline::parse_time(s).map(|t| (t, s)))
    }
    if let Some((_, at)) = parsed(&thread.settled_at) {
        return Some(at);
    }
    let run = thread.latest_run.as_ref();
    [
        parsed(&thread.latest_user_message_at),
        run.and_then(|r| parsed(&r.requested_at)),
        run.and_then(|r| parsed(&r.started_at)),
        run.and_then(|r| parsed(&r.completed_at)),
    ]
    .into_iter()
    .flatten()
    .max_by_key(|(time, _)| *time)
    .map(|(_, at)| at)
    .or_else(|| crate::timeline::parse_time(&thread.updated_at).map(|_| thread.updated_at.as_str()))
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

pub(crate) use pluto_core::notifications::{Status, status};

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

        // Header: logo and wordmark, the sidebar toggle at the right.
        let toggle = widgets::icon_button(
            ui,
            Rect::from_min_size(Pos2::new(rect.right() - 39.0, 12.0), Vec2::splat(28.0)),
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
        // Pluto's pixel logo, centered on the toggle's row, then the wordmark.
        crate::ui::pixel_logo::paint(
            ui,
            Rect::from_center_size(Pos2::new(left + 26.0, 26.0), Vec2::new(24.0, 20.0)),
            0.0,
        );
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
            Pos2::new(left + 46.0, 30.9 - baseline),
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
            Vec2::new(rect.width() - 17.0, 32.0),
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

    /// Pluto: the sidebar's width, dragged by its right edge and kept with
    /// the client settings; the legacy sidebar keeps the fixed width its
    /// layout is drawn for.
    pub fn sidebar_width(&self) -> f32 {
        if self.client_setting("legacySidebarEnabled") == true {
            return theme::SIDEBAR_WIDTH;
        }
        self.client_settings["plutoSidebarWidth"]
            .as_f64()
            .map_or(theme::SIDEBAR_WIDTH, |w| {
                (w as f32).clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
            })
    }

    /// The docked sidebar's right edge: dragging it resizes the sidebar,
    /// a double click brings back the default width.
    pub(crate) fn sidebar_resize_handle(&mut self, ui: &mut Ui, sidebar: Rect) {
        if self.client_setting("legacySidebarEnabled") == true {
            return;
        }
        // Mostly outside the sidebar, clear of its list's scroll bar.
        let handle = Rect::from_min_max(
            Pos2::new(sidebar.right() - 2.0, sidebar.top()),
            Pos2::new(sidebar.right() + 4.0, sidebar.bottom()),
        );
        let response = ui.interact(
            handle,
            ui.id().with("sidebar-resize"),
            Sense::click_and_drag(),
        );
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            ui.painter().rect_filled(
                Rect::from_min_max(
                    Pos2::new(sidebar.right() - 1.0, sidebar.top()),
                    Pos2::new(sidebar.right() + 1.0, sidebar.bottom()),
                ),
                0.0,
                theme::alpha(theme::primary(), 0.5),
            );
        }
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let width = (pointer.x - sidebar.left())
                .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
                .round();
            self.client_settings["plutoSidebarWidth"] = serde_json::json!(width);
        }
        if response.drag_stopped() {
            self.save_client_settings();
        }
        if response.double_clicked() {
            if let Some(settings) = self.client_settings.as_object_mut() {
                settings.remove("plutoSidebarWidth");
            }
            self.save_client_settings();
        }
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
            self.sidebar_selection_menu(ui);
            self.draft_row_menu(ui);
        }
        self.sidebar_footer(ui, rect);
    }

    /// The default sidebar's search row and thread inbox.
    fn inbox_sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter().clone();
        let left = rect.left();

        if self.sidebar_new_and_search(ui, rect, "New thread", "Search threads") {
            self.start_draft(None);
        }

        // Project scope: the scoped project (or "All projects") and its menu,
        // with Add project at the right.
        let scoped = self.scoped_project().cloned();
        let label = scoped
            .as_ref()
            .map_or("All projects", |p| p.title.as_str())
            .to_owned();
        let label_font = font(12.5, Weight::Medium);
        let room = rect.width() - 1.0 - 20.0 - 36.0;
        let label_width = widgets::text_width(ui, &label, label_font.clone()).min(room - 50.0);
        let scope_button = Rect::from_min_size(
            Pos2::new(left + 10.0, SCOPE_TOP),
            Vec2::new((label_width + 50.0).min(room), 28.0),
        );
        let scope_open = self.open_menu.as_deref() == Some(project_scope::MENU_ID);
        let scope_response = ui
            .interact(scope_button, ui.id().with("sidebar-scope"), Sense::click())
            .on_hover_text("Filter threads by project");
        let lit = scope_open || scope_response.hovered();
        if lit {
            painter.rect_filled(
                scope_button,
                widgets::radius(theme::CONTROL_RADIUS as f32),
                theme::accent(),
            );
        }
        let icon_rect = Rect::from_min_size(
            Pos2::new(scope_button.left() + 8.0, SCOPE_TOP + 6.0),
            Vec2::splat(16.0),
        );
        match &scoped {
            Some(project) => project_icon::paint(ui, icon_rect, project),
            None => theme::paint_icon(ui, "folder", icon_rect, 16.0, theme::sidebar_icon()),
        }
        let text_color = if lit {
            theme::sidebar_foreground()
        } else {
            theme::sidebar_muted()
        };
        let text_left = scope_button.left() + 30.0;
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(text_left, SCOPE_TOP),
                Pos2::new(text_left + label_width + 1.0, SCOPE_TOP + 28.0),
            ),
            &label,
            label_font,
            text_color,
        );
        theme::paint_icon(
            ui,
            "chevron-down",
            Rect::from_min_size(
                Pos2::new(text_left + label_width + 6.0, SCOPE_TOP + 8.0),
                Vec2::splat(12.0),
            ),
            12.0,
            text_color,
        );
        if scope_response.clicked() {
            self.toggle_project_scope_menu();
        }
        if widgets::icon_button(
            ui,
            Rect::from_min_size(Pos2::new(rect.right() - 39.0, SCOPE_TOP), Vec2::splat(28.0)),
            "sidebar-add-project",
            "folder-plus",
            16.0,
            theme::sidebar_icon(),
            "Add project",
        )
        .clicked()
        {
            self.toggle_palette(crate::ui::palette::View::AddProject);
        }
        self.project_scope_menu(
            ui,
            Rect::from_min_size(scope_button.min, Vec2::new(246.0, 28.0)),
        );

        // Thread list.
        // The undo notice heads the footer.
        let notice = self.undo_notice(
            ui,
            left + 10.0,
            rect.right() - 10.0,
            rect.bottom() - FOOTER_HEIGHT,
        );
        let list_rect = Rect::from_min_max(
            Pos2::new(left, LIST_TOP),
            Pos2::new(rect.right() - 1.0, rect.bottom() - FOOTER_HEIGHT - notice),
        );
        self.thread_list(ui, list_rect);
        self.sidebar_drag_frame(ui, list_rect);
        self.sweep_frame(ui, list_rect);
    }

    /// Pluto: the New and Search rows under Code | Chat, shared by both
    /// modes (`hint` names what the search filters); returns whether New was
    /// clicked.
    pub(crate) fn sidebar_new_and_search(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        new_label: &str,
        hint: &str,
    ) -> bool {
        let left = rect.left();
        let width = rect.width() - 1.0 - 16.0;
        let new_row = Rect::from_min_size(Pos2::new(left + 8.0, NEW_TOP), Vec2::new(width, 34.0));
        let response = ui.interact(new_row, ui.id().with("sidebar-new"), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(new_row, widgets::radius(8.0), theme::accent());
        }
        theme::paint_icon(
            ui,
            "square-pen",
            Rect::from_min_size(Pos2::new(left + 18.0, NEW_TOP + 9.0), Vec2::splat(16.0)),
            16.0,
            theme::sidebar_icon(),
        );
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(left + 44.0, NEW_TOP), new_row.max),
            new_label,
            font(14.0, Weight::Medium),
            theme::sidebar_foreground(),
        );

        let search_top = NEW_TOP + 34.0;
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(Pos2::new(left + 18.0, search_top + 9.0), Vec2::splat(16.0)),
            16.0,
            theme::sidebar_icon(),
        );
        let input_rect = Rect::from_min_size(
            Pos2::new(left + 44.0, search_top + 6.5),
            Vec2::new(width - 44.0, 21.0),
        );
        let mut search_ui = ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
        search_ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Medium))
                .text_color(theme::sidebar_foreground())
                .hint_text(
                    egui::RichText::new(hint)
                        .font(font(14.0, Weight::Medium))
                        .color(theme::sidebar_muted()),
                )
                .desired_width(input_rect.width()),
        );
        response.clicked()
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
                    self.notify(
                        crate::ui::toasts::Kind::Info,
                        format!("{label} is not implemented yet"),
                    );
                }
            }
        }
    }

    fn thread_list(&mut self, ui: &mut Ui, rect: Rect) {
        let query = self.search.trim().to_lowercase();
        let scope = self.scoped_project().map(|p| p.id.clone());
        // `searchSidebarThreads`: title matches, then message matches.
        let search = self.search.clone();
        let found = self.thread_search(ui.ctx(), crate::ui::thread_search::Slot::Sidebar, &search);
        let mut threads: Vec<std::sync::Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| {
                query.is_empty()
                    || t.title.to_lowercase().contains(&query)
                    || found.contains_key(&t.id)
            })
            .filter(|t| scope.as_ref().is_none_or(|id| &t.project_id == id))
            .filter(|t| !self.is_chat_thread(t))
            .cloned()
            .collect();
        threads.sort_by_key(|t| !query.is_empty() && !t.title.to_lowercase().contains(&query));
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
        // Unsent drafts lead the list, outside search.
        let draft_rows = if query.is_empty() {
            self.draft_rows(scope.as_deref())
        } else {
            Vec::new()
        };
        let drafts_height = Self::draft_block_height(&draft_rows);
        let content_height = drafts_height + cards.len() as f32 * CARD_SLOT + tail_height + 4.0;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        egui::ScrollArea::vertical()
            .id_salt("sidebar-threads")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                // dnd-kit's auto-scroll while a row is dragged within the
                // list: 20% of the viewport at either edge, faster deeper
                // in (10px every 5ms at the edge).
                if self.sidebar_drag.is_some()
                    && let Some(pointer) = ui.ctx().pointer_latest_pos()
                    && (rect.left()..=rect.right()).contains(&pointer.x)
                {
                    let threshold = rect.height() * 0.2;
                    let depth = if pointer.y < rect.top() + threshold {
                        (rect.top() + threshold - pointer.y).min(threshold)
                    } else if pointer.y > rect.bottom() - threshold {
                        -(pointer.y - (rect.bottom() - threshold)).min(threshold)
                    } else {
                        0.0
                    };
                    if depth != 0.0 && threshold > 0.0 {
                        let dt = ui.input(|i| i.stable_dt).min(0.1);
                        ui.scroll_with_delta(Vec2::new(0.0, 2000.0 * depth / threshold * dt));
                        ui.ctx().request_repaint();
                    }
                }
                let height = content_height.max(rect.height());
                let (area, _) =
                    ui.allocate_exact_size(Vec2::new(rect.width(), height), Sense::hover());
                let mut y = area.top();
                if draft_rows.is_empty()
                    && cards.is_empty()
                    && settled.is_empty()
                    && snoozed_count == 0
                {
                    self.empty_list(ui, area);
                    return;
                }
                y = self.draft_block(ui, area, y, &draft_rows);
                // The sections a dragged row can land on.
                use crate::ui::sidebar_drag::Zone;
                let pinned_count = cards.iter().filter(|t| t.pinned_at.is_some()).count();
                let pinned_top = y;
                let pinned_bottom = y
                    + (pinned_count.max(1) as f32)
                        * CARD_SLOT
                        * if pinned_count == 0 { 0.5 } else { 1.0 };
                self.sidebar_zones.clear();
                self.sidebar_rows.clear();
                self.sidebar_zones.push((
                    Zone::Pinned,
                    Rect::from_min_max(
                        Pos2::new(area.left() + 4.0, pinned_top),
                        Pos2::new(area.right() - 4.0, pinned_bottom),
                    ),
                ));
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
                let active_top = if pinned_count > 0 {
                    pinned_bottom
                } else {
                    pinned_top + CARD_SLOT * 0.5
                };
                self.sidebar_zones.push((
                    Zone::Active,
                    Rect::from_min_max(
                        Pos2::new(area.left() + 4.0, active_top),
                        Pos2::new(
                            area.right() - 4.0,
                            (y.max(active_top + CARD_SLOT)).min(area.bottom() - tail_height),
                        ),
                    ),
                ));
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
                self.sidebar_zones.push((
                    Zone::Settled,
                    Rect::from_min_max(
                        Pos2::new(area.left() + 4.0, header.top()),
                        Pos2::new(area.right() - 4.0, area.bottom()),
                    ),
                ));
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
        let color = if snoozed {
            theme::BLUE_400
        } else {
            theme::sidebar_muted()
        };
        // "Settled (3)": the name, then its count in the icon tone.
        let (name, count) = label
            .rsplit_once(" (")
            .map_or((label, ""), |(name, count)| {
                (name, count.trim_end_matches(')'))
            });
        let heading = font(11.5, Weight::Medium);
        let x = rect.left() + 10.0;
        let width = widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
            name,
            heading.clone(),
            color,
        );
        if !count.is_empty() {
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(x + width + 6.0, rect.top()), rect.max),
                count,
                heading,
                theme::sidebar_icon(),
            );
        }
        let chevron = Rect::from_center_size(
            Pos2::new(rect.right() - 14.0, rect.center().y),
            Vec2::splat(12.0),
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
            if response.hovered() {
                theme::sidebar_foreground()
            } else {
                theme::sidebar_icon()
            },
        );
        response
    }

    fn thread_card(&mut self, ui: &mut Ui, rect: Rect, thread: &Thread) {
        let response = ui.interact(
            rect,
            ui.id().with(("thread-card", &thread.id)),
            Sense::click_and_drag(),
        );
        self.sidebar_row_drag(&response, thread);
        self.sidebar_rows.push((
            thread.id.clone(),
            crate::ui::sidebar_drag::zone_of(thread, self.now),
            rect,
        ));
        let active = self.selected.as_deref() == Some(thread.id.as_str());
        let status = status(thread);
        let recede = self.recedes(thread, active);
        let unread = self.is_unread(thread);
        let woke = self.woke_at(thread);
        let swept = self.swept(&thread.id);
        // `group-hover`: the row stays hovered while the pointer is over its
        // own buttons (egui's `hovered()` only reports the topmost widget).
        // A sweep takes the hovers.
        let hovered = response.contains_pointer() && self.sidebar_sweep.is_none();
        let painter = ui.painter().clone();
        if active {
            painter.rect_filled(rect, widgets::radius(8.0), theme::row_active());
        } else if self.legacy_selection.contains(&thread.id) || swept {
            // `bg-sidebar-row-selected`.
            painter.rect_filled(rect, widgets::radius(8.0), theme::muted());
        } else if hovered {
            painter.rect_filled(rect, widgets::radius(8.0), theme::accent());
        }
        let x = rect.left() + 10.0;
        let right = rect.right() - 10.0;
        let top = rect.top() + 8.0;

        // Row 1: unsent draft, project icon, project name, pin, status or time.
        let draft = !active && self.has_draft(&thread.id);
        if draft {
            self.draft_pen(ui, Pos2::new(x, top + 4.0), &thread.id);
        }
        let lead = if draft { 18.0 } else { 0.0 };
        let project = self.state.project(&thread.project_id).cloned();
        if let Some(project) = &project {
            project_icon::paint(
                ui,
                Rect::from_min_size(Pos2::new(x + lead, top + 2.0), Vec2::splat(16.0)),
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
                Some(("Goal", "circle-dashed", theme::BLUE_500))
            }
            Status::Working => Some(("Working", "circle-dashed", theme::BLUE_500)),
            Status::Waiting => Some(("Waiting", "circle-dot-dashed", theme::muted_foreground())),
            Status::Limited => Some(("Limited", "circle-alert", theme::warning())),
            Status::Failed => Some(("Failed", "circle-alert", theme::error())),
            Status::Ready if woke.is_some() => Some(("Woke", "alarm-clock", theme::warning())),
            Status::Ready if unread => Some(("Done", "circle-check", theme::EMERALD_500)),
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
            let settle_response = ui.interact(
                settle,
                ui.id().with(("settle", &thread.id)),
                Sense::click_and_drag(),
            );
            if self.capability("threadSettlement") {
                self.sweep_button(&settle_response, thread, "settle");
            }
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
                let settle = crate::model::command("thread.settle", &thread.id);
                self.park(&thread.id, settle, crate::ui::park::Park::Settle, &[]);
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
            // Working and Waiting count the time (`WorkingDuration`).
            let since = match status {
                Status::Working => thread.working_started_at(),
                Status::Waiting => thread
                    .latest_run
                    .as_ref()
                    .and_then(|r| r.completed_at.clone()),
                _ => None,
            };
            let duration = since
                .as_deref()
                .and_then(crate::timeline::parse_time)
                .map(|start| working_duration((self.now - start).num_seconds()));
            let duration_width = duration.as_ref().map_or(0.0, |d| {
                widgets::tabular_width(ui, d, font(12.0, Weight::Medium)) + 4.0
            });
            if duration.is_some() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(1));
            }
            let width = widgets::text_width(ui, label, font(12.0, Weight::Medium)) + duration_width;
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
            if let Some(duration) = &duration {
                widgets::tabular_text(
                    ui,
                    Pos2::new(slot_right - duration_width + 4.0, top + 2.0),
                    duration,
                    font(12.0, Weight::Medium),
                    color,
                    16.0,
                );
            }
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
                    self.acknowledge_woke(&thread.id, at);
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
        let pinned = thread.pinned_at.is_some();
        if let Some(project) = &project {
            let name_rect = Rect::from_min_max(
                Pos2::new(x + lead + 22.0, top),
                Pos2::new(
                    right - status_width - 6.0 - if pinned { 16.0 } else { 0.0 },
                    top + 20.0,
                ),
            );
            if pinned {
                let name_width =
                    widgets::text_width(ui, &project.title, font(12.0, Weight::Medium))
                        .min(name_rect.width());
                self.pin_glyph(
                    ui,
                    Pos2::new(name_rect.left() + name_width + 4.0, top + 4.0),
                    thread,
                );
            }
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
        // A title the server rewrites dims meanwhile.
        let title_color = if thread.title_regeneration.is_some() {
            theme::alpha(title_color, 0.55)
        } else {
            title_color
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

        // Row 3: worktree, branch, running terminals, pull request; at the
        // right the providers it ran on (`SidebarProviderStack`).
        let row3 = Rect::from_min_max(Pos2::new(x, top + 46.0), Pos2::new(right, top + 62.0));
        let center = row3.center().y;
        // From the right: the current provider, then up to two before it.
        let mut stack_left = right;
        let current = thread.model_selection.instance_id.clone();
        if let Some((icon, tint)) = self
            .provider_driver(&current)
            .and_then(|driver| crate::ui::pickers::provider_icon(&driver))
        {
            stack_left -= 14.0;
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(stack_left, center - 7.0), Vec2::splat(14.0)),
                14.0,
                theme::alpha(tint, 0.6),
            );
            let previous: Vec<&String> = thread
                .provider_instance_history
                .iter()
                .filter(|id| **id != current)
                .collect();
            for id in previous.iter().rev().take(2) {
                if let Some((icon, _)) = self
                    .provider_driver(id)
                    .and_then(|driver| crate::ui::pickers::provider_icon(&driver))
                {
                    stack_left -= 6.0 + 12.0;
                    theme::paint_icon(
                        ui,
                        icon,
                        Rect::from_min_size(Pos2::new(stack_left, center - 6.0), Vec2::splat(12.0)),
                        12.0,
                        theme::alpha(theme::muted_foreground(), 0.35),
                    );
                }
            }
            stack_left -= 6.0;
        }
        let mut lx = row3.left();
        // The worktree it runs in.
        if let Some(path) = thread
            .worktree_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let name = path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or(path);
            let slot = Rect::from_min_size(Pos2::new(lx, center - 6.0), Vec2::splat(12.0));
            crate::ui::lucide::paint(
                ui,
                slot,
                "folder-git-2",
                theme::alpha(theme::muted_foreground(), 0.4),
            );
            ui.interact(slot, ui.id().with(("worktree", &thread.id)), Sense::hover())
                .on_hover_text(match &thread.branch {
                    Some(branch) => format!("Worktree: {name} ({branch})"),
                    None => format!("Worktree: {name}"),
                });
            lx += 12.0 + 6.0;
        }
        // Running terminals and the pull request sit after the branch.
        let terminals = self
            .terminal_metadata
            .iter()
            .filter(|((t, _), m)| *t == thread.id && m["hasRunningSubprocess"] == true)
            .count();
        let pull_request =
            crate::ui::thread_pull_requests::current_link(&thread.pull_requests).cloned();
        let pr_label = pull_request.as_ref().map(|link| {
            let others = thread.pull_requests.len().saturating_sub(1);
            let number = link["number"].as_u64().unwrap_or(0);
            if others > 0 {
                format!("#{number} +{others}")
            } else {
                format!("#{number}")
            }
        });
        let pr_width = pr_label.as_ref().map_or(0.0, |l| {
            12.0 + 4.0 + widgets::tabular_width(ui, l, font(12.0, Weight::Regular)) + 6.0
        });
        let terminal_width = if terminals > 0 { 14.0 + 6.0 } else { 0.0 };
        let branch_right = stack_left - pr_width - terminal_width;
        // A search found by its messages shows the matching one instead.
        let mut after_branch = lx;
        if let Some(found) = self.sidebar_excerpt(thread) {
            crate::ui::thread_search::paint_excerpt(
                ui,
                Rect::from_min_max(
                    Pos2::new(lx, row3.top()),
                    Pos2::new(branch_right, row3.bottom()),
                ),
                found,
            );
            after_branch = branch_right;
        } else if let Some(branch) = &thread.branch {
            let width = widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(lx, row3.top()),
                    Pos2::new(branch_right, row3.bottom()),
                ),
                branch,
                font(12.0, Weight::Regular),
                theme::alpha(theme::sidebar_muted(), 0.4),
            );
            after_branch = lx + width + 6.0;
        }
        let mut gx = after_branch.min(branch_right);
        if terminals > 0 {
            let alpha = crate::ui::legacy_sidebar::pulse_alpha(ui.input(|i| i.time));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            let slot = Rect::from_min_size(Pos2::new(gx, center - 7.0), Vec2::splat(14.0));
            theme::paint_icon(
                ui,
                "terminal",
                slot,
                14.0,
                theme::alpha(crate::ui::legacy_sidebar::TEAL_300, 0.9 * alpha),
            );
            let label = if terminals == 1 {
                "1 terminal process running. Open terminal".to_owned()
            } else {
                format!("{terminals} terminal processes running. Open terminal")
            };
            if ui
                .interact(
                    slot.expand(2.0),
                    ui.id().with(("terminal", &thread.id)),
                    Sense::click(),
                )
                .on_hover_text(label)
                .clicked()
            {
                self.select_thread(Some(thread.id.clone()));
                self.open_terminal_drawer(&thread.id);
            }
            gx += terminal_width;
        }
        if let (Some(link), Some(label)) = (&pull_request, &pr_label) {
            let (icon, tone, tooltip) = crate::ui::thread_pull_requests::status(link).unwrap_or((
                "git-pull-request-arrow",
                theme::muted_foreground(),
                format!(
                    "PR #{}, status pending",
                    link["number"].as_u64().unwrap_or(0)
                ),
            ));
            let badge =
                Rect::from_min_size(Pos2::new(gx, row3.top()), Vec2::new(pr_width - 6.0, 16.0));
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(gx, center - 6.0), Vec2::splat(12.0)),
                12.0,
                tone,
            );
            widgets::tabular_text(
                ui,
                Pos2::new(gx + 16.0, row3.top()),
                label,
                font(12.0, Weight::Regular),
                tone,
                16.0,
            );
            if ui
                .interact(
                    badge,
                    ui.id().with(("pr-badge", &thread.id)),
                    Sense::click(),
                )
                .on_hover_text(tooltip)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                && let Some(url) = link["url"].as_str()
            {
                widgets::open_external(url);
            }
        }

        if swept {
            self.sweep_badge(ui, rect);
        }
        self.thread_hover_card(
            ui,
            rect,
            response.contains_pointer() && self.sidebar_sweep.is_none(),
            thread,
        );
        self.row_file_drop(ui, rect, thread);
        if response.clicked() {
            self.sidebar_row_clicked(ui, thread);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            // A selected row opens the selection's menu, another its own.
            if self.legacy_selection.contains(&thread.id) {
                self.open_menu = None;
                self.legacy_multi_menu = Some(crate::ui::context_menu::State::new(at));
            } else {
                self.open_thread_menu(&thread.id, at);
            }
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
                self.park(&thread.id, cmd, crate::ui::park::Park::Snooze, &[]);
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
            Sense::click_and_drag(),
        );
        self.sidebar_row_drag(&response, thread);
        self.sidebar_rows.push((
            thread.id.clone(),
            crate::ui::sidebar_drag::zone_of(thread, self.now),
            rect,
        ));
        let active = self.selected.as_deref() == Some(thread.id.as_str());
        let swept = self.swept(&thread.id);
        // `group-hover`, kept while the pointer is over the row's buttons; a
        // sweep takes the hovers.
        let hovered = response.contains_pointer() && self.sidebar_sweep.is_none();
        if active {
            ui.painter()
                .rect_filled(rect, widgets::radius(8.0), theme::row_active());
        } else if self.legacy_selection.contains(&thread.id) || swept {
            ui.painter()
                .rect_filled(rect, widgets::radius(8.0), theme::muted());
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
        let title_color = if thread.title_regeneration.is_some() {
            theme::alpha(title_color, 0.55)
        } else {
            title_color
        };
        // After the favicon: an unsent draft; after the title: the pin.
        let draft = !active && self.has_draft(&thread.id);
        if draft {
            self.draft_pen(ui, Pos2::new(x + 24.0, rect.center().y - 6.0), &thread.id);
        }
        let title_left = x + 26.0 + if draft { 18.0 } else { 0.0 };
        let pinned = thread.pinned_at.is_some();
        let title_rect = Rect::from_min_max(
            Pos2::new(title_left, rect.top()),
            Pos2::new(
                label_right - label_width.max(32.0) - 8.0 - if pinned { 16.0 } else { 0.0 },
                rect.bottom(),
            ),
        );
        if pinned {
            let title_font = font(
                14.0,
                if recede {
                    Weight::Regular
                } else {
                    Weight::Medium
                },
            );
            let width = widgets::text_width(ui, &thread.title, title_font).min(title_rect.width());
            self.pin_glyph(
                ui,
                Pos2::new(title_left + width + 4.0, rect.center().y - 6.0),
                thread,
            );
        }
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
                    self.acknowledge_woke(&thread.id, at);
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
                .interact(
                    button,
                    ui.id().with((id, &thread.id)),
                    Sense::click_and_drag(),
                )
                .on_hover_text(tooltip);
            let capable = self.capability(if snoozed {
                "threadSnooze"
            } else {
                "threadSettlement"
            });
            if capable {
                self.sweep_button(
                    &button_response,
                    thread,
                    if snoozed { "unsnooze" } else { "unsettle" },
                );
            }
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
                let failure = if snoozed {
                    "Failed to wake thread"
                } else {
                    "Failed to un-settle thread"
                };
                self.dispatch_titled(cmd, failure);
            }
        }
        if swept {
            self.sweep_badge(ui, rect);
        }
        self.thread_hover_card(
            ui,
            rect,
            response.contains_pointer() && self.sidebar_sweep.is_none(),
            thread,
        );
        self.row_file_drop(ui, rect, thread);
        if response.clicked() {
            self.sidebar_row_clicked(ui, thread);
        }
        if response.secondary_clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            // A selected row opens the selection's menu, another its own.
            if self.legacy_selection.contains(&thread.id) {
                self.open_menu = None;
                self.legacy_multi_menu = Some(crate::ui::context_menu::State::new(at));
            } else {
                self.open_thread_menu(&thread.id, at);
            }
        }
    }
}

impl App {
    /// Select on click; a second click on the same row within 400ms renames
    /// it (upstream renames on double click).
    /// Files dragged over a row ring it; dropped, they open its thread and
    /// join its composer.
    fn row_file_drop(&mut self, ui: &Ui, rect: Rect, thread: &Thread) {
        let pointer_in = ui
            .input(|i| i.pointer.latest_pos())
            .is_some_and(|p| rect.contains(p));
        if !pointer_in {
            return;
        }
        if ui.input(|i| !i.raw.hovered_files.is_empty()) {
            ui.painter().rect_stroke(
                rect,
                widgets::radius(8.0),
                Stroke::new(1.0, theme::alpha(theme::primary(), 0.7)),
                egui::StrokeKind::Inside,
            );
        }
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            if self.selected.as_deref() != Some(thread.id.as_str()) {
                self.select_thread(Some(thread.id.clone()));
            }
            crate::ui::attachments::add_dropped_files(&self.incoming_images, ui.ctx(), dropped);
        }
    }

    /// `handleThreadClick`: Ctrl toggles the row in the selection, Shift
    /// selects from the anchor, a plain click clears it and opens the thread.
    fn sidebar_row_clicked(&mut self, ui: &Ui, thread: &Thread) {
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.command {
            if !self.legacy_selection.remove(&thread.id) {
                self.legacy_selection.insert(thread.id.clone());
                self.legacy_anchor = Some(thread.id.clone());
            }
        } else if modifiers.shift {
            let ordered = self.ordered_sidebar_threads();
            let anchor = self
                .legacy_anchor
                .as_ref()
                .and_then(|a| ordered.iter().position(|id| id == a));
            match (anchor, ordered.iter().position(|id| *id == thread.id)) {
                (Some(a), Some(t)) => {
                    for id in &ordered[a.min(t)..=a.max(t)] {
                        self.legacy_selection.insert(id.clone());
                    }
                }
                _ => {
                    self.legacy_selection.insert(thread.id.clone());
                    self.legacy_anchor = Some(thread.id.clone());
                }
            }
        } else {
            self.legacy_selection.clear();
            self.legacy_anchor = Some(thread.id.clone());
            self.thread_row_clicked(ui, thread);
        }
    }

    /// The selection's menu (`Sidebar.tsx`'s multi-select menu): Unpin,
    /// Settle, Snooze, Regenerate titles, Mark unread, Delete.
    pub(crate) fn sidebar_selection_menu(&mut self, ui: &mut Ui) {
        use crate::ui::context_menu::{self, Item};
        // Escape clears the selection.
        if !self.legacy_selection.is_empty()
            && self.legacy_multi_menu.is_none()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.legacy_selection.clear();
            self.legacy_anchor = None;
            return;
        }
        let Some(state) = self.legacy_multi_menu.take() else {
            return;
        };
        let threads: Vec<std::sync::Arc<Thread>> = self
            .state
            .threads
            .iter()
            .filter(|t| self.legacy_selection.contains(&t.id))
            .cloned()
            .collect();
        let count = threads.len();
        if count == 0 {
            return;
        }
        let pinned = threads.iter().filter(|t| t.pinned_at.is_some()).count();
        let unsettled = threads
            .iter()
            .filter(|t| t.settled_override.as_deref() != Some("settled"))
            .count();
        let snoozable = self.capability("threadSnooze")
            && threads
                .iter()
                .all(|t| crate::snooze::can_snooze(t, self.now));
        let regenerating = threads
            .iter()
            .filter(|t| t.title_regeneration.is_some())
            .count();
        let now = self.now.with_timezone(&chrono::Local);
        let presets = crate::snooze::presets(now);
        let mut items = Vec::new();
        if pinned > 0 && self.capability("threadPinning") {
            items.push(Item::new("unpin", format!("Unpin ({pinned})")));
        }
        items.push(Item {
            disabled: unsettled == 0,
            ..Item::new("settle", format!("Settle ({count})"))
        });
        if snoozable {
            let mut children: Vec<Item> = presets
                .iter()
                .map(|p| {
                    Item::new(
                        format!("snooze:{}", p.id),
                        format!("{} ({})", p.label, p.when),
                    )
                })
                .collect();
            children.push(Item {
                separator_before: true,
                ..Item::new("snooze:custom", "Custom…")
            });
            items.push(Item {
                children,
                ..Item::new("snooze", format!("Snooze ({count})"))
            });
        }
        if self.capability("threadTitleRegeneration") {
            items.push(if regenerating == count {
                Item {
                    disabled: true,
                    ..Item::new("regenerate", format!("Regenerating… ({regenerating})"))
                }
            } else {
                Item::new(
                    "regenerate",
                    format!("Regenerate titles ({})", count - regenerating),
                )
            });
        }
        items.push(Item::new("mark-unread", format!("Mark unread ({count})")));
        items.push(Item {
            destructive: true,
            ..Item::new("delete", format!("Delete ({count})"))
        });
        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        self.legacy_multi_menu = open;
        let Some(clicked) = clicked else {
            return;
        };
        let ids: Vec<String> = threads.iter().map(|t| t.id.clone()).collect();
        let clear = |app: &mut App| {
            app.legacy_selection.clear();
            app.legacy_anchor = None;
        };
        match clicked.as_str() {
            "unpin" => {
                for thread in threads.iter().filter(|t| t.pinned_at.is_some()) {
                    self.unpin(thread);
                }
                clear(self);
            }
            "settle" => {
                for thread in threads
                    .iter()
                    .filter(|t| t.settled_override.as_deref() != Some("settled"))
                {
                    let settle = crate::model::command("thread.settle", &thread.id);
                    self.park(&thread.id, settle, crate::ui::park::Park::Settle, &ids);
                }
                clear(self);
            }
            "snooze:custom" => {
                clear(self);
                self.request_custom_snooze(ids);
            }
            id if id.starts_with("snooze:") => {
                if let Some(preset) = presets.iter().find(|p| format!("snooze:{}", p.id) == id) {
                    let until = crate::snooze::until_iso(preset);
                    for thread in &ids {
                        let mut command = crate::model::command("thread.snooze", thread);
                        command["snoozedUntil"] = serde_json::json!(until);
                        self.park(thread, command, crate::ui::park::Park::Snooze, &ids);
                    }
                }
                clear(self);
            }
            "regenerate" => {
                for thread in threads.iter().filter(|t| t.title_regeneration.is_none()) {
                    let mut command = crate::model::command("thread.metadata.update", &thread.id);
                    command["regenerateTitle"] = serde_json::json!(true);
                    self.dispatch_titled(command, "Failed to regenerate thread titles");
                }
                clear(self);
            }
            "mark-unread" => {
                for thread in &threads {
                    self.mark_unread(thread);
                }
                clear(self);
            }
            "delete" => {
                let mut list: Vec<serde_json::Value> = ids
                    .iter()
                    .map(|id| crate::model::command("thread.delete", id))
                    .collect();
                let plural = if count == 1 { "" } else { "s" };
                if self.client_setting("confirmThreadDelete") != false {
                    let first = list.remove(0);
                    self.confirm = Some(crate::ui::thread_menu::Confirm {
                        title: format!("Delete {count} thread{plural}?"),
                        message: "This permanently clears conversation history for these threads."
                            .into(),
                        confirm_label: "Delete",
                        command: first,
                        more: list,
                    });
                } else {
                    let open_deleted = self.selected.as_ref().is_some_and(|s| ids.contains(s));
                    for command in list {
                        self.dispatch_titled(command, "Failed to delete threads");
                    }
                    if open_deleted {
                        self.start_draft(None);
                    }
                }
                clear(self);
            }
            _ => {}
        }
    }

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

    #[test]
    fn new_pins_go_first_and_durations_compact() {
        assert_eq!(pin_midpoint("", ""), "n");
        assert_eq!(pin_midpoint("", "n"), "h");
        assert_eq!(pin_midpoint("", "b"), "an");
        assert!(pin_midpoint("", "ab").as_str() < "ab");
        assert!(!valid_pin_key("ba"));
        assert_eq!(working_duration(-3), "0s");
        assert_eq!(working_duration(42), "42s");
        assert_eq!(working_duration(300), "5m");
        assert_eq!(working_duration(3900), "1h 5m");
    }
}
