//! The Pull requests page (`routes/_chat.pull-requests.tsx`,
//! `PullRequestRow.tsx`, `PullRequestListRow.tsx`, `PullRequestListEmptyState.tsx`,
//! `PullRequestGhosts.tsx`, `PullRequestListFilters.tsx`,
//! `pullRequestList.logic.ts`, `pullRequestPresentation.tsx`).
//!
//! Ported: the toolbar (search, sort, filters, provider, refresh), the
//! `pullRequests.list` listing grouped by involvement and sorted like
//! upstream, the rows (state and conflict glyphs, number, title, checks and
//! review verdict, line counts, author, repository, labels, update time), the
//! loading ghost and the empty and error states.
//!
//! Not ported: the detail panel and right panel (selecting a row shows a
//! notice), searching on the hosts (the search filters loaded rows), the
//! author and label filters, line-count backfill (`pullRequests.listStats`),
//! paging past the first 99 rows per repository, stacks, avatars (initials
//! are shown) and the condensed topbar.
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::context_menu::{self, Item};
use crate::ui::widgets;

const EMERALD: Color32 = Color32::from_rgb(0x5e, 0xe9, 0xb5); // emerald-300
const RED: Color32 = Color32::from_rgb(0xff, 0xa2, 0xa2); // red-300
const VIOLET: Color32 = Color32::from_rgb(0xc4, 0xb4, 0xff); // violet-300
const ZINC: Color32 = Color32::from_rgb(0x9f, 0x9f, 0xa9); // zinc-400
const AMBER_400: Color32 = Color32::from_rgb(0xff, 0xb9, 0x00);
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74);
const PAGE_SIZE: u32 = 99;

/// `SORT_OPTIONS`.
const SORTS: [(&str, &str, &str); 7] = [
    ("ready", "Merge readiness", "list-checks"),
    ("blocked", "Blocked on me", "user-lock"),
    ("updated", "Recently updated", "clock"),
    ("newest", "Newest shown", "calendar-arrow-down"),
    ("oldest", "Oldest shown", "calendar-arrow-up"),
    ("largest", "Largest shown", "maximize-2"),
    ("smallest", "Smallest shown", "minimize-2"),
];
const STATES: [(&str, &str, &str); 4] = [
    ("all", "All", "layers"),
    ("open", "Open", "git-pull-request-arrow"),
    ("closed", "Closed", "git-pull-request-closed"),
    ("merged", "Merged", "git-merge"),
];
const INVOLVEMENTS: [(&str, &str, &str); 3] = [
    ("all", "All", "layers"),
    ("reviewing", "Reviewing", "eye"),
    ("authored", "Authored", "pen-line"),
];
const DRAFTS: [(&str, &str, &str); 3] = [
    ("", "All", "layers"),
    ("only", "Drafts only", "git-pull-request-draft"),
    ("hide", "Hide drafts", "eye-off"),
];
const REVIEWS: [(&str, &str, &str); 5] = [
    ("", "All", "layers"),
    ("approved", "Approved", "circle-check"),
    ("changes-requested", "Changes requested", "circle-x"),
    ("review-required", "Review required", "circle-dashed"),
    ("none", "No reviews", "circle-slash"),
];
const CHECKS: [(&str, &str, &str); 3] = [
    ("", "All", "layers"),
    ("passing", "Passing", "circle-check"),
    ("failing", "Failing", "circle-x"),
];

/// List preferences (`pullRequestListPreferences.ts`: open, all, merge readiness).
pub struct PullRequestsState {
    pub query: String,
    pub sort: &'static str,
    pub state: &'static str,
    pub involvement: &'static str,
    pub draft: &'static str,
    pub review: &'static str,
    pub checks: &'static str,
    pub project: Option<String>,
    pub host: Option<String>,
    /// Listing results by request key, and the key last asked for.
    pub results: std::collections::HashMap<String, Result<Value, String>>,
    pub requested: std::collections::HashSet<String>,
    pub refreshing: bool,
    pub menu: Option<(&'static str, context_menu::State)>,
}

impl Default for PullRequestsState {
    fn default() -> Self {
        Self {
            query: String::new(),
            sort: "ready",
            state: "open",
            involvement: "all",
            draft: "",
            review: "",
            checks: "",
            project: None,
            host: None,
            results: Default::default(),
            requested: Default::default(),
            refreshing: false,
            menu: None,
        }
    }
}

impl PullRequestsState {
    /// The `pullRequests.list` input: involvement and search are narrowed
    /// here, so switching them never waits on the hosts.
    fn input(&self) -> Value {
        let mut input = json!({"state": self.state, "limit": PAGE_SIZE});
        let mut filters = serde_json::Map::new();
        for (key, value) in [
            ("draft", self.draft),
            ("review", self.review),
            ("checks", self.checks),
        ] {
            if !value.is_empty() {
                filters.insert(key.into(), json!(value));
            }
        }
        if !filters.is_empty() {
            input["filters"] = Value::Object(filters);
        }
        if let Some(project) = &self.project {
            input["projectId"] = json!(project);
        }
        if let Some(host) = &self.host {
            input["host"] = json!(host);
        }
        input
    }

    fn filtered(&self) -> bool {
        self.state != "open"
            || self.involvement != "all"
            || self.project.is_some()
            || self.host.is_some()
            || !self.draft.is_empty()
            || !self.review.is_empty()
            || !self.checks.is_empty()
    }
}

fn str_field<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry[key].as_str().unwrap_or_default()
}

fn lines(entry: &Value) -> i64 {
    entry["additions"].as_i64().unwrap_or(0) + entry["deletions"].as_i64().unwrap_or(0)
}

fn viewer_of<'a>(entry: &Value, viewers: &'a Value) -> Option<&'a str> {
    viewers[str_field(entry, "host")].as_str()
}

fn authored(entry: &Value, viewers: &Value) -> bool {
    viewer_of(entry, viewers).is_some_and(|viewer| {
        entry["author"]["login"]
            .as_str()
            .is_some_and(|login| login.eq_ignore_ascii_case(viewer.trim()))
    })
}

/// `matchesPullRequestQuery`.
fn matches_query(entry: &Value, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || format!(
            "#{} {} {} {} {}",
            entry["number"],
            str_field(entry, "title"),
            str_field(entry, "repository"),
            str_field(entry, "headBranch"),
            entry["author"]["login"].as_str().unwrap_or_default()
        )
        .to_lowercase()
        .contains(&query)
}

/// One involvement group (`groupPullRequestsByInvolvement`).
struct Group {
    key: &'static str,
    label: &'static str,
    entries: Vec<Value>,
}

fn group_entries(entries: Vec<Value>, viewers: &Value) -> Vec<Group> {
    let mut authored_entries = Vec::new();
    let mut requested = Vec::new();
    let mut others = Vec::new();
    for entry in entries {
        if authored(&entry, viewers) {
            authored_entries.push(entry);
        } else if entry["viewerReviewRequested"] == true {
            requested.push(entry);
        } else {
            others.push(entry);
        }
    }
    [
        ("authored", "Authored", authored_entries),
        ("reviewRequested", "Review requested", requested),
        ("others", "Others", others),
    ]
    .into_iter()
    .filter(|(_, _, entries)| !entries.is_empty())
    .map(|(key, label, entries)| Group {
        key,
        label,
        entries,
    })
    .collect()
}

/// `sortPullRequestGroups` (the listing arrives newest update first).
fn sort_groups(groups: &mut [Group], sort: &str, searching: bool, involvement: &str) {
    let updated = |e: &Value| str_field(e, "updatedAt").to_owned();
    let created = |e: &Value| str_field(e, "createdAt").to_owned();
    let measured = |e: &Value| lines(e) > 0;
    for group in groups.iter_mut() {
        let entries = &mut group.entries;
        match sort {
            "ready" if !searching => {
                let tier = |e: &Value| {
                    if e["mergeability"] == "conflicting" {
                        4
                    } else if e["state"] != "open" {
                        3
                    } else if e["isDraft"] == true {
                        2
                    } else if e["checksState"] == "passing" && e["reviewDecision"] == "approved" {
                        0
                    } else if e["checksState"] == "passing" {
                        1
                    } else {
                        2
                    }
                };
                entries.sort_by(|a, b| {
                    tier(a)
                        .cmp(&tier(b))
                        .then(measured(b).cmp(&measured(a)))
                        .then(lines(a).cmp(&lines(b)))
                        .then(updated(b).cmp(&updated(a)))
                });
            }
            "blocked" if !searching => {
                let role = match group.key {
                    "others" => involvement,
                    "authored" => "authored",
                    _ => "reviewing",
                };
                let tier = |e: &Value| -> i32 {
                    if role == "authored" {
                        if e["state"] != "open" {
                            6
                        } else if e["mergeability"] == "conflicting" {
                            0
                        } else if e["reviewDecision"] == "changes-requested" {
                            1
                        } else if e["checksState"] == "failing" {
                            2
                        } else if e["isDraft"] == true {
                            3
                        } else if e["checksState"] == "passing" && e["reviewDecision"] == "approved"
                        {
                            5
                        } else {
                            4
                        }
                    } else {
                        i32::from(e["state"] != "open")
                    }
                };
                if role != "all" {
                    entries.sort_by(|a, b| tier(a).cmp(&tier(b)).then(updated(b).cmp(&updated(a))));
                }
            }
            "newest" | "oldest" => entries.sort_by(|a, b| {
                let order = created(a).cmp(&created(b));
                let order = if sort == "newest" {
                    order.reverse()
                } else {
                    order
                };
                order.then(updated(b).cmp(&updated(a)))
            }),
            "largest" | "smallest" => entries.sort_by(|a, b| {
                let order = lines(a).cmp(&lines(b));
                measured(b)
                    .cmp(&measured(a))
                    .then(if sort == "largest" {
                        order.reverse()
                    } else {
                        order
                    })
                    .then(updated(b).cmp(&updated(a)))
            }),
            _ => {}
        }
    }
}

/// The lifecycle glyph: icon and tone (`PULL_REQUEST_STATE_PRESENTATION`).
fn state_glyph(entry: &Value) -> (&'static str, Color32) {
    match (str_field(entry, "state"), entry["isDraft"] == true) {
        ("merged", _) => ("git-merge", theme::alpha(VIOLET, 0.9)),
        ("closed", _) => ("git-pull-request-closed", theme::alpha(RED, 0.9)),
        (_, true) => ("git-pull-request-draft", theme::alpha(ZINC, 0.8)),
        _ => ("git-pull-request-arrow", theme::alpha(EMERALD, 0.9)),
    }
}

/// Text balanced over as few lines as `max_width` allows (`text-balance`).
fn balanced_lines(ui: &Ui, text: &str, font: egui::FontId, max_width: f32) -> Vec<String> {
    let wrap = |width: f32| -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for word in text.split(' ') {
            match lines.last_mut() {
                Some(line)
                    if widgets::text_width(ui, &format!("{line} {word}"), font.clone())
                        <= width =>
                {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        lines
    };
    let lines = wrap(max_width);
    if lines.len() < 2 {
        return lines;
    }
    let (mut low, mut high) = (0.0f32, max_width);
    while high - low > 1.0 {
        let middle = (low + high) / 2.0;
        if wrap(middle).len() > lines.len() {
            low = middle;
        } else {
            high = middle;
        }
    }
    wrap(high)
}

/// An outline button (`size="sm"` 28 tall, or the default 32) with an
/// optional leading icon; returns its response.
fn outline_button(
    ui: &Ui,
    rect: Rect,
    id: &str,
    icon: Option<(&str, f32)>,
    label: &str,
    large: bool,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(("pr-button", id)), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        if response.hovered() {
            theme::overlay(0.05)
        } else {
            theme::control_surface()
        },
        Stroke::new(1.0, theme::input()),
        StrokeKind::Inside,
    );
    let pad = if large { 10.0 } else { 8.0 };
    let mut x = rect.left() + pad;
    if let Some((icon, size)) = icon {
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(
                Pos2::new(x, rect.center().y - size / 2.0),
                Vec2::splat(size),
            ),
            size,
            theme::muted_foreground(),
        );
        x += size + 6.0;
    }
    if !label.is_empty() {
        widgets::text_in(
            ui,
            Rect::from_min_max(Pos2::new(x, rect.top()), rect.max),
            label,
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
    }
    response
}

/// Width of `outline_button`: padding, icon and gap, label.
fn outline_width(ui: &Ui, icon: Option<f32>, label: &str, large: bool) -> f32 {
    let pad = if large { 10.0 } else { 8.0 };
    let label_width = if label.is_empty() {
        0.0
    } else {
        widgets::text_width(ui, label, font(14.0, Weight::Medium))
    };
    let icon_width = icon.map_or(0.0, |size| size + if label.is_empty() { 0.0 } else { 6.0 });
    (pad + icon_width + label_width + pad + 2.0).round()
}

impl App {
    pub(crate) fn open_pull_requests(&mut self) {
        self.pull_requests_open = true;
        self.usage_open = false;
        self.open_menu = None;
    }

    fn request_pull_requests(&mut self, force: bool) {
        let input = self.prs.input();
        let key = format!("prs:{input}");
        if force || !self.prs.requested.contains(&key) {
            self.prs.requested.insert(key.clone());
            self.send_request(key, "pullRequests.list", input);
        }
    }

    /// The page in the workspace area.
    pub(crate) fn pull_requests_page(&mut self, ui: &mut Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::background());
        widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(rect.left() + 20.0, 16.0), Vec2::new(300.0, 20.0)),
            "Pull Requests",
            font(14.0, Weight::Medium),
            theme::foreground(),
        );
        let panel = Rect::from_min_size(Pos2::new(rect.right() - 41.0, 12.0), Vec2::splat(28.0));
        if widgets::icon_button(
            ui,
            panel,
            "pr-right-panel",
            "panel-right",
            16.0,
            theme::muted_foreground(),
            "Select a pull request first",
        )
        .clicked()
        {
            self.status = Some("The pull request panel is not implemented yet".into());
        }
        self.request_pull_requests(false);

        let body = Rect::from_min_max(Pos2::new(rect.left(), theme::TOPBAR_HEIGHT), rect.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical()
            .id_salt("pull-requests")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                // `scrollbar-gutter-both`, then the expanded container (`max-w-6xl px-6`).
                let width = (body.width() - 32.0).min(1152.0) - 48.0;
                let left = body.center().x - width / 2.0;
                let top = ui.max_rect().top() + 24.0;
                self.pull_requests_toolbar(ui, left, top, width);
                let list_top = top + 34.0 + 17.0;
                let min_bottom = body.bottom() - 48.0 + (ui.max_rect().top() - body.top());
                let bottom = self.pull_requests_list(ui, left, list_top, width, min_bottom);
                ui.allocate_rect(
                    Rect::from_min_max(
                        Pos2::new(left, ui.max_rect().top()),
                        Pos2::new(left + width, bottom + 48.0),
                    ),
                    Sense::hover(),
                );
            });
        self.pull_requests_menu(ui);
    }

    fn pull_requests_toolbar(&mut self, ui: &mut Ui, left: f32, top: f32, width: f32) {
        let right = left + width;
        let refresh = Rect::from_min_size(Pos2::new(right - 32.0, top + 1.0), Vec2::splat(32.0));
        if outline_button(ui, refresh, "refresh", Some(("refresh-cw", 16.0)), "", true)
            .on_hover_text("Refresh pull requests")
            .clicked()
            && !self.prs.refreshing
        {
            self.prs.refreshing = true;
            self.request_pull_requests(true);
        }
        let host_label = self.prs.host.clone().unwrap_or_else(|| "All".into());
        let mut x = refresh.left() - 8.0;
        let mut buttons: Vec<(&'static str, &'static str, String)> = vec![
            ("host", "plug-2", host_label),
            ("filters", "list-filter", "Filters".into()),
            ("sort", "arrow-down-up", "Sort".into()),
        ];
        let filter_count = [self.prs.draft, self.prs.review, self.prs.checks]
            .iter()
            .filter(|v| !v.is_empty())
            .count()
            + usize::from(self.prs.project.is_some());
        if filter_count > 0 {
            buttons[1].2 = format!("Filters {filter_count}");
        }
        for (id, icon, label) in buttons {
            let w = outline_width(ui, Some(16.0), &label, true);
            let rect = Rect::from_min_size(Pos2::new(x - w, top + 1.0), Vec2::new(w, 32.0));
            x = rect.left() - 8.0;
            if outline_button(ui, rect, id, Some((icon, 16.0)), &label, true).clicked() {
                let open = self.prs.menu.as_ref().is_some_and(|(menu, _)| *menu == id);
                self.prs.menu = if open {
                    None
                } else {
                    Some((
                        id,
                        context_menu::State::dropdown(Pos2::new(rect.left(), rect.bottom() + 4.0)),
                    ))
                };
            }
        }
        // Search (`Input` with a leading search glyph).
        let search = Rect::from_min_max(Pos2::new(left, top), Pos2::new(x, top + 34.0));
        ui.painter().rect(
            search,
            CornerRadius::same(8),
            theme::control_surface(),
            Stroke::new(1.0, theme::input()),
            StrokeKind::Inside,
        );
        theme::paint_icon(
            ui,
            "search",
            Rect::from_min_size(
                Pos2::new(search.left() + 10.0, search.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            16.0,
            theme::foreground(),
        );
        let inner = Rect::from_min_max(
            Pos2::new(search.left() + 32.0, search.top() + 7.0),
            Pos2::new(search.right() - 10.0, search.bottom() - 7.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        child.add(
            egui::TextEdit::singleline(&mut self.prs.query)
                .id_salt("pr-search")
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font(14.0, Weight::Regular))
                .text_color(theme::foreground())
                .desired_width(inner.width())
                .vertical_align(egui::Align::Center),
        );
        if self.prs.query.is_empty() {
            widgets::text(
                ui,
                Pos2::new(inner.left(), search.top() + 7.0),
                "Search pull requests, or label:bug",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
        }
    }

    /// The listing, or its loading, empty or error state; returns its bottom.
    fn pull_requests_list(
        &mut self,
        ui: &mut Ui,
        left: f32,
        top: f32,
        width: f32,
        min_bottom: f32,
    ) -> f32 {
        let key = format!("prs:{}", self.prs.input());
        let result = self.prs.results.get(&key).cloned();
        let Some(result) = result else {
            return pull_request_ghost(ui, left, top, width);
        };
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                return self.pull_requests_empty(
                    ui,
                    left,
                    top,
                    width,
                    min_bottom,
                    "Could not load pull requests",
                    &error,
                    Some("Retry"),
                );
            }
        };
        let viewers = value["viewers"].clone();
        let entries: Vec<Value> = value["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| self.prs.state == "all" || e["state"] == self.prs.state)
            .filter(|e| match self.prs.involvement {
                "reviewing" => e["viewerReviewRequested"] == true,
                "authored" => authored(e, &viewers),
                _ => true,
            })
            .filter(|e| match self.prs.draft {
                "only" => e["isDraft"] == true,
                "hide" => e["isDraft"] != true,
                _ => true,
            })
            .filter(|e| match self.prs.review {
                "" => true,
                "none" => e["reviewDecision"].is_null(),
                review => e["reviewDecision"] == review,
            })
            .filter(|e| matches_query(e, &self.prs.query))
            .cloned()
            .collect();
        if entries.is_empty() {
            let query = self.prs.query.trim().to_owned();
            if self.state.projects.is_empty() {
                return self.pull_requests_empty(
                    ui,
                    left,
                    top,
                    width,
                    min_bottom,
                    "No projects in this workspace",
                    "Add a project, and the pull requests from its repository appear here.",
                    None,
                );
            }
            if !query.is_empty() {
                let shown = if query.chars().count() > 48 {
                    format!("{}…", query.chars().take(48).collect::<String>())
                } else {
                    query
                };
                return self.pull_requests_empty(
                    ui,
                    left,
                    top,
                    width,
                    min_bottom,
                    &format!("Nothing matches \u{201c}{shown}\u{201d}"),
                    "The hosts were searched for it. Try fewer words, or search by number, author or branch.",
                    Some("Check again"),
                );
            }
            let (title, description) = if self.prs.filtered() {
                (
                    "Nothing under these filters",
                    "Widen the state, involvement or project filter to see more.",
                )
            } else {
                (
                    "No pull requests",
                    "Pull requests from every project in this workspace appear here.",
                )
            };
            return self.pull_requests_empty(
                ui,
                left,
                top,
                width,
                min_bottom,
                title,
                description,
                Some("Check again"),
            );
        }
        let mut groups = group_entries(entries, &viewers);
        sort_groups(
            &mut groups,
            self.prs.sort,
            !self.prs.query.trim().is_empty(),
            self.prs.involvement,
        );
        let multiple_hosts = value["providers"].as_array().is_some_and(|p| p.len() > 1);
        let mut y = top;
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                y += 12.0;
            }
            y = pull_request_group_header(ui, group, left, y, width);
            for entry in &group.entries {
                if self.pull_request_row(ui, entry, left, y, width, multiple_hosts) {
                    self.status =
                        Some("The pull request detail panel is not implemented yet".into());
                }
                y += 56.0 + 2.0;
            }
        }
        if value["truncated"] == true {
            widgets::text(
                ui,
                Pos2::new(left + width / 2.0 - 120.0, y + 12.0),
                "Narrow your search to find more pull requests.",
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
            y += 40.0;
        }
        y
    }

    /// One row (`PullRequestRow`); returns whether it was clicked.
    fn pull_request_row(
        &self,
        ui: &Ui,
        entry: &Value,
        left: f32,
        top: f32,
        width: f32,
        show_provider: bool,
    ) -> bool {
        let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, 56.0));
        let response = ui.interact(
            rect,
            ui.id().with(("pr-row", str_field(entry, "url"))),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(6),
                theme::alpha(theme::accent(), 0.6),
            );
        }
        let inner_left = rect.left() + 12.0;
        let inner_right = rect.right() - 12.0;
        let line1 = rect.top() + 10.0;
        let line2 = line1 + 20.0;
        // Lifecycle glyph (`mt-0.75`), with the conflict badge on its corner.
        let (icon, tone) = state_glyph(entry);
        theme::paint_icon(
            ui,
            icon,
            Rect::from_min_size(Pos2::new(inner_left, line1 + 3.0), Vec2::splat(16.0)),
            16.0,
            tone,
        );
        if entry["state"] == "open"
            && entry["isDraft"] != true
            && entry["mergeability"] == "conflicting"
        {
            let badge =
                Rect::from_min_size(Pos2::new(inner_left + 8.0, line1 + 11.0), Vec2::splat(12.0));
            ui.painter()
                .circle_filled(badge.center(), 6.0, theme::background());
            theme::paint_icon(ui, "triangle-alert", badge, 12.0, DESTRUCTIVE);
        }
        let text_left = inner_left + 16.0 + 8.0;
        // First line: number, title, signals; line counts on the right.
        let number = format!("#{}", entry["number"]);
        let number_width = widgets::tabular_width(ui, &number, theme::mono(12.0));
        widgets::text(
            ui,
            Pos2::new(text_left, line1 + 2.0),
            &number,
            theme::mono(12.0),
            theme::muted_foreground(),
            16.0,
        );
        let mut status_left = inner_right;
        let (additions, deletions) = (
            entry["additions"].as_i64().unwrap_or(0),
            entry["deletions"].as_i64().unwrap_or(0),
        );
        if additions + deletions > 0 {
            let mono = theme::mono(11.0);
            let removed = format!("-{deletions}");
            let added = format!("+{additions}");
            let removed_width = widgets::text_width(ui, &removed, mono.clone());
            let added_width = widgets::text_width(ui, &added, mono.clone());
            widgets::text(
                ui,
                Pos2::new(inner_right - removed_width, line1 + 3.0),
                &removed,
                mono.clone(),
                theme::diff_deletion_foreground(),
                16.0,
            );
            widgets::text(
                ui,
                Pos2::new(inner_right - removed_width - 4.0 - added_width, line1 + 3.0),
                &added,
                mono,
                theme::diff_addition_foreground(),
                16.0,
            );
            status_left = inner_right - removed_width - 4.0 - added_width - 6.0;
        }
        let mut signals: Vec<(&str, Color32)> = Vec::new();
        match entry["checksState"].as_str() {
            Some("passing") => signals.push(("circle-check", theme::alpha(EMERALD, 0.9))),
            Some("failing") => signals.push(("circle-x", DESTRUCTIVE)),
            Some("pending") => signals.push(("circle-dot", theme::alpha(AMBER_400, 0.9))),
            _ => {}
        }
        match entry["reviewDecision"].as_str() {
            Some("approved") => signals.push(("user-check", theme::alpha(EMERALD, 0.9))),
            Some("changes-requested") => {
                signals.push(("user-round-x", theme::alpha(AMBER_400, 0.8)))
            }
            Some("review-required") => {
                signals.push(("user-round", theme::alpha(theme::muted_foreground(), 0.6)))
            }
            _ => {}
        }
        let signals_width = signals.len() as f32 * 18.0;
        let title_left = text_left + number_width + 6.0;
        let title_max = (status_left
            - title_left
            - if signals.is_empty() {
                0.0
            } else {
                signals_width + 6.0
            })
        .max(0.0);
        let title_width = widgets::text_in(
            ui,
            Rect::from_min_size(Pos2::new(title_left, line1), Vec2::new(title_max, 20.0)),
            str_field(entry, "title"),
            font(14.0, Weight::Regular),
            theme::foreground(),
        );
        let mut x = title_left + title_width + 6.0;
        for (icon, color) in signals {
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(x, line1 + 3.0), Vec2::splat(14.0)),
                14.0,
                color,
            );
            x += 18.0;
        }
        // Second line: provider, author, repository, labels; the time on the right.
        let small = font(11.0, Weight::Regular);
        let time = widgets::relative_time(str_field(entry, "updatedAt"), self.now);
        let time_label = if time == "now" || time.is_empty() {
            time
        } else {
            format!("{time} ago")
        };
        let time_width = widgets::tabular_width(ui, &time_label, small.clone());
        widgets::tabular_text(
            ui,
            Pos2::new(inner_right - time_width, line2),
            &time_label,
            small.clone(),
            theme::muted_foreground(),
            16.0,
        );
        let meta_right = inner_right - time_width - 6.0;
        let mut x = text_left;
        if show_provider {
            let icon = match str_field(entry, "provider") {
                "gitlab" => "sc-gitlab",
                "azure-devops" => "sc-azure-devops",
                "bitbucket" => "sc-bitbucket",
                "forgejo" => "sc-forgejo",
                _ => "sc-github",
            };
            theme::paint_icon(
                ui,
                icon,
                Rect::from_min_size(Pos2::new(x, line2 + 2.0), Vec2::splat(12.0)),
                12.0,
                theme::muted_foreground(),
            );
            x += 18.0;
        }
        // Avatar initial (`size-3.5`) and login.
        let login = entry["author"]["login"].as_str().unwrap_or("ghost");
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
        let login_width = widgets::text_in(
            ui,
            Rect::from_min_size(
                Pos2::new(x, line2),
                Vec2::new((meta_right - x).clamp(0.0, 160.0), 16.0),
            ),
            login,
            small.clone(),
            theme::muted_foreground(),
        );
        x += login_width + 6.0;
        let repository = str_field(entry, "repository");
        if x < meta_right {
            let w = widgets::text_in(
                ui,
                Rect::from_min_size(Pos2::new(x, line2), Vec2::new(meta_right - x, 16.0)),
                repository,
                small.clone(),
                theme::muted_foreground(),
            );
            x += w + 6.0;
        }
        // The first label, with "+N" for the rest.
        if let Some(labels) = entry["labels"].as_array().filter(|l| !l.is_empty())
            && x + 40.0 < meta_right
        {
            let name = labels[0]["name"].as_str().unwrap_or_default();
            let extra = if labels.len() > 1 {
                format!(" +{}", labels.len() - 1)
            } else {
                String::new()
            };
            let text = format!("{name}{extra}");
            let text_width = widgets::text_width(ui, &text, font(10.0, Weight::Medium)).min(160.0);
            let chip =
                Rect::from_min_size(Pos2::new(x, line2 + 1.0), Vec2::new(text_width + 8.0, 14.0));
            let color = labels[0]["color"].as_str().and_then(|hex| {
                let hex = hex.trim_start_matches('#');
                let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
                Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
            });
            let (fill, text_color) = match color {
                Some(c) => (
                    theme::alpha(c, 0.12),
                    theme::mix(c, theme::foreground(), 0.55),
                ),
                None => (theme::muted(), theme::foreground()),
            };
            ui.painter().rect_filled(chip, CornerRadius::same(4), fill);
            widgets::text_in(
                ui,
                Rect::from_min_max(Pos2::new(chip.left() + 4.0, chip.top()), chip.max),
                &text,
                font(10.0, Weight::Medium),
                text_color,
            );
        }
        response.clicked()
    }

    /// `PullRequestListEmptyState` / `PullRequestsUnavailableState`,
    /// centered in the rest of the page; returns its bottom.
    #[allow(clippy::too_many_arguments)]
    fn pull_requests_empty(
        &mut self,
        ui: &mut Ui,
        left: f32,
        top: f32,
        width: f32,
        min_bottom: f32,
        title: &str,
        description: &str,
        button: Option<&str>,
    ) -> f32 {
        let description_font = font(14.0, Weight::Regular);
        let lines = balanced_lines(ui, description, description_font.clone(), 384.0);
        let content = 80.0
            + 24.0
            + 28.0
            + 4.0
            + lines.len() as f32 * 20.0
            + if button.is_some() { 24.0 + 28.0 } else { 0.0 };
        let region_bottom = min_bottom.max(top + content + 48.0);
        let center_x = left + width / 2.0;
        let mut y = ((top + region_bottom) / 2.0 - content / 2.0).round();
        paint_branch_mark(
            ui,
            Rect::from_min_size(Pos2::new(center_x - 64.0, y), Vec2::new(128.0, 80.0)),
        );
        y += 80.0 + 24.0;
        let title_font = font(20.0, Weight::SemiBold);
        let title_width = widgets::text_width(ui, title, title_font.clone());
        widgets::text(
            ui,
            Pos2::new(center_x - title_width / 2.0, y),
            title,
            title_font,
            theme::foreground(),
            28.0,
        );
        y += 28.0 + 4.0;
        for line in &lines {
            let w = widgets::text_width(ui, line, description_font.clone());
            widgets::text(
                ui,
                Pos2::new(center_x - w / 2.0, y),
                line,
                description_font.clone(),
                theme::muted_foreground(),
                20.0,
            );
            y += 20.0;
        }
        if let Some(label) = button {
            y += 24.0;
            let label = if self.prs.refreshing {
                "Checking..."
            } else {
                label
            };
            let w = outline_width(ui, Some(14.0), label, false);
            let rect = Rect::from_min_size(
                Pos2::new((center_x - w / 2.0).round(), y),
                Vec2::new(w, 28.0),
            );
            if outline_button(
                ui,
                rect,
                "check-again",
                Some(("refresh-cw", 14.0)),
                label,
                false,
            )
            .clicked()
                && !self.prs.refreshing
            {
                self.prs.refreshing = true;
                self.request_pull_requests(true);
            }
            y += 28.0;
        }
        region_bottom.max(y)
    }

    /// The open toolbar menu: sort, filters (submenus) or provider.
    fn pull_requests_menu(&mut self, ui: &mut Ui) {
        let Some((which, mut state)) = self.prs.menu.take() else {
            return;
        };
        let radio =
            |prefix: &str, options: &[(&str, &str, &'static str)], current: &str| -> Vec<Item> {
                options
                    .iter()
                    .map(|(value, label, icon)| Item {
                        selected: *value == current,
                        ..Item::new(format!("{prefix}:{value}"), *label).icon(icon)
                    })
                    .collect()
            };
        let items: Vec<Item> = match which {
            "sort" => radio("sort", &SORTS, self.prs.sort),
            "host" => {
                let mut hosts: Vec<String> = self
                    .prs
                    .results
                    .values()
                    .filter_map(|r| r.as_ref().ok())
                    .flat_map(|v| v["providers"].as_array().cloned().unwrap_or_default())
                    .filter_map(|p| p["host"].as_str().map(str::to_owned))
                    .collect();
                hosts.sort();
                hosts.dedup();
                let current = self.prs.host.clone().unwrap_or_default();
                let mut items = vec![Item {
                    selected: current.is_empty(),
                    ..Item::new("host:", "All").icon("plug-2")
                }];
                items.extend(hosts.into_iter().map(|host| Item {
                    selected: host == current,
                    ..Item::new(format!("host:{host}"), host.clone()).icon("sc-github")
                }));
                items
            }
            _ => {
                let mut projects = vec![Item {
                    selected: self.prs.project.is_none(),
                    ..Item::new("project:", "All projects").icon("layers")
                }];
                projects.extend(self.state.projects.iter().map(|p| Item {
                    selected: self.prs.project.as_deref() == Some(p.id.as_str()),
                    ..Item::new(format!("project:{}", p.id), p.title.clone())
                }));
                let submenu = |id: &str, label: &str, children: Vec<Item>| Item {
                    children,
                    ..Item::new(id, label)
                };
                vec![
                    submenu("state", "State", radio("state", &STATES, self.prs.state)),
                    submenu(
                        "involvement",
                        "Involvement",
                        radio("involvement", &INVOLVEMENTS, self.prs.involvement),
                    ),
                    Item {
                        separator_before: true,
                        ..submenu("draft", "Draft", radio("draft", &DRAFTS, self.prs.draft))
                    },
                    submenu(
                        "review",
                        "Review",
                        radio("review", &REVIEWS, self.prs.review),
                    ),
                    submenu(
                        "checks",
                        "Checks",
                        radio("checks", &CHECKS, self.prs.checks),
                    ),
                    Item {
                        separator_before: true,
                        ..submenu("project", "Project", projects)
                    },
                ]
            }
        };
        let mut open = Some(state);
        let chosen = context_menu::show(ui, &mut open, &items);
        if let Some(next) = open.take() {
            state = next;
            self.prs.menu = Some((which, state));
        }
        let Some(chosen) = chosen else {
            return;
        };
        self.prs.menu = None;
        let (group, value) = chosen.split_once(':').unwrap_or((chosen.as_str(), ""));
        let pick = |options: &[(&'static str, &str, &str)]| {
            options.iter().find(|o| o.0 == value).map(|o| o.0)
        };
        match group {
            "sort" => self.prs.sort = pick(&SORTS).unwrap_or("ready"),
            "state" => self.prs.state = pick(&STATES).unwrap_or("open"),
            "involvement" => self.prs.involvement = pick(&INVOLVEMENTS).unwrap_or("all"),
            "draft" => self.prs.draft = pick(&DRAFTS).unwrap_or(""),
            "review" => self.prs.review = pick(&REVIEWS).unwrap_or(""),
            "checks" => self.prs.checks = pick(&CHECKS).unwrap_or(""),
            "project" => self.prs.project = (!value.is_empty()).then(|| value.to_owned()),
            "host" => self.prs.host = (!value.is_empty()).then(|| value.to_owned()),
            _ => {}
        }
    }
}

/// `PullRequestGroupHeader`: glyph, name, count, a rule to the edge;
/// returns the top of the first row.
fn pull_request_group_header(ui: &Ui, group: &Group, left: f32, top: f32, width: f32) -> f32 {
    let icon = match group.key {
        "authored" => "pen-line",
        "reviewRequested" => "eye",
        "others" => "users",
        _ => "layers",
    };
    let color = theme::alpha(theme::muted_foreground(), 0.7);
    let mut x = left + 12.0;
    theme::paint_icon(
        ui,
        icon,
        Rect::from_min_size(Pos2::new(x, top + 1.0), Vec2::splat(14.0)),
        14.0,
        color,
    );
    x += 14.0 + 8.0;
    let label_font = font(12.0, Weight::Medium);
    widgets::text(
        ui,
        Pos2::new(x, top),
        group.label,
        label_font.clone(),
        color,
        16.0,
    );
    x += widgets::text_width(ui, group.label, label_font.clone()) + 8.0;
    let count = group.entries.len().to_string();
    widgets::tabular_text(
        ui,
        Pos2::new(x, top),
        &count,
        label_font.clone(),
        theme::alpha(theme::muted_foreground(), 0.5),
        16.0,
    );
    x += widgets::tabular_width(ui, &count, label_font) + 8.0;
    ui.painter().hline(
        x..=left + width - 12.0,
        top + 8.5,
        Stroke::new(1.0, theme::border()),
    );
    top + 16.0 + 4.0 + 2.0
}

/// `PullRequestListGhost` (seven rows); returns its bottom.
fn pull_request_ghost(ui: &Ui, left: f32, top: f32, width: f32) -> f32 {
    const TITLES: [f32; 7] = [0.6, 0.4, 0.5, 0.667, 0.4, 0.6, 0.5];
    const METAS: [f32; 7] = [0.4, 0.333, 0.4, 0.25, 0.333, 0.4, 0.333];
    let bar = theme::alpha(theme::muted_foreground(), 0.15);
    let mut y = top;
    for index in 0..7 {
        let row = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, 56.0));
        let inner_left = row.left() + 12.0;
        let inner_right = row.right() - 12.0;
        ui.painter()
            .circle_filled(Pos2::new(inner_left + 8.0, row.center().y), 8.0, bar);
        let text_left = inner_left + 24.0;
        let column = inner_right - 64.0 - 8.0 - text_left;
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(text_left, row.center().y - 14.0),
                Vec2::new(column * TITLES[index], 16.0),
            ),
            CornerRadius::same(4),
            bar,
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(text_left, row.center().y + 4.0),
                Vec2::new(column * METAS[index], 14.0),
            ),
            CornerRadius::same(4),
            bar,
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(inner_right - 48.0, row.center().y - 10.5),
                Vec2::new(48.0, 12.0),
            ),
            CornerRadius::same(4),
            bar,
        );
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(inner_right - 64.0, row.center().y + 3.5),
                Vec2::new(64.0, 12.0),
            ),
            CornerRadius::same(4),
            bar,
        );
        y += 58.0;
    }
    y
}

/// `BranchMark` (not joined), the empty state's drawing: viewBox 120x72
/// fitted into `rect` (128x80), stroked at 2.
fn paint_branch_mark(ui: &Ui, rect: Rect) {
    let scale = (rect.width() / 120.0).min(rect.height() / 72.0);
    let origin = rect.center() - Vec2::new(60.0, 36.0) * scale;
    let at = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
    let base = theme::muted_foreground();
    let current = theme::alpha(base, 0.6);
    let stroke = |color: Color32| Stroke::new(2.0 * scale, color);
    let painter = ui.painter();
    painter.line_segment(
        [at(10.0, 58.0), at(110.0, 58.0)],
        stroke(theme::alpha(base, 0.3)),
    );
    for x in [10.0, 110.0] {
        painter.circle(
            at(x, 58.0),
            5.0 * scale,
            theme::alpha(current, 0.25),
            stroke(current),
        );
    }
    let cubic = |p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| -> Vec<Pos2> {
        (0..=24)
            .map(|i| {
                let t = i as f32 / 24.0;
                let u = 1.0 - t;
                let x = u * u * u * p0.0
                    + 3.0 * u * u * t * p1.0
                    + 3.0 * u * t * t * p2.0
                    + t * t * t * p3.0;
                let y = u * u * u * p0.1
                    + 3.0 * u * u * t * p1.1
                    + 3.0 * u * t * t * p2.1
                    + t * t * t * p3.1;
                at(x, y)
            })
            .collect()
    };
    // The branch leaving the base, stopped short of the node.
    let mut left_branch = cubic((30.0, 58.0), (30.0, 40.0), (38.0, 32.0), (54.0, 32.0));
    left_branch.push(at(58.0, 32.0));
    painter.add(egui::Shape::line(left_branch, stroke(current)));
    // The returning half, dashed (`2 7`).
    let mut right_branch = cubic((90.0, 58.0), (90.0, 40.0), (82.0, 32.0), (66.0, 32.0));
    right_branch.push(at(62.0, 32.0));
    let dashed = theme::alpha(base, 0.5);
    painter.extend(egui::Shape::dashed_line(
        &right_branch,
        stroke(dashed),
        2.0 * scale,
        7.0 * scale,
    ));
    painter.circle_stroke(
        at(60.0, 32.0),
        4.0 * scale,
        stroke(theme::alpha(base, 0.45)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(number: u64, author: &str, review: bool) -> Value {
        json!({"number": number, "host": "github.com", "author": {"login": author},
               "viewerReviewRequested": review, "state": "open", "title": format!("PR {number}"),
               "repository": "o/r", "headBranch": "b", "updatedAt": format!("2026-10-0{number}T00:00:00Z"),
               "additions": 0, "deletions": 0})
    }

    #[test]
    fn groups_follow_involvement() {
        let viewers = json!({"github.com": "me"});
        let groups = group_entries(
            vec![
                entry(1, "other", true),
                entry(2, "me", false),
                entry(3, "x", false),
            ],
            &viewers,
        );
        let keys: Vec<&str> = groups.iter().map(|g| g.key).collect();
        assert_eq!(keys, ["authored", "reviewRequested", "others"]);
    }

    #[test]
    fn merge_readiness_puts_approved_passing_first() {
        let mut a = entry(1, "x", false);
        let mut b = entry(2, "x", false);
        b["checksState"] = json!("passing");
        b["reviewDecision"] = json!("approved");
        a["mergeability"] = json!("conflicting");
        let mut groups = vec![Group {
            key: "others",
            label: "Others",
            entries: vec![a, b],
        }];
        sort_groups(&mut groups, "ready", false, "all");
        assert_eq!(groups[0].entries[0]["number"], 2);
    }

    #[test]
    fn queries_match_number_title_and_author() {
        let e = entry(42, "octocat", false);
        assert!(matches_query(&e, "#42"));
        assert!(matches_query(&e, "octo"));
        assert!(!matches_query(&e, "nope"));
    }
}
