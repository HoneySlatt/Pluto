//! Find in thread (`chat/useThreadFind.ts`, `chat/ThreadFindBar.tsx`,
//! `chat/threadFind.ts`, `chat/useThreadFindNavigation.ts`): Mod+F opens a
//! search field at the top right of the chat. The server counts the query's
//! occurrences in the thread's messages and plans
//! (`orchestration.searchThreadStream`, or `searchThread` on servers that do
//! not stream), Enter, Shift+Enter and the arrows step through them, and the
//! timeline marks the drawn ones (`threadFindHighlights.ts`, see
//! `markdown::FindMarks`), opening the fold, the collapsed message or the
//! older history page that holds the active one and scrolling it into view.
//!
//! Not ported: the skill labels sent with the query (a `$skill` chip is
//! searched by its name), and the reading position a new query starts
//! from is the first entry in view rather than its first occurrence in view.
use std::collections::HashSet;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::protocol::Action;
use crate::theme::{self, Weight, font};
use crate::ui::toasts::{Kind, Toast};
use crate::ui::widgets;

/// `--thread-details-panel-width`.
const BAR_WIDTH: f32 = 280.0;
const BAR_HEIGHT: f32 = 36.0;
/// `useDebouncedValue(normalizedQuery, 100)`.
const DEBOUNCE: f64 = 0.1;
/// Live results refresh at most this often while the thread changes.
const REFRESH: f64 = 0.3;
const DESTRUCTIVE: Color32 = Color32::from_rgb(251, 65, 74); // --destructive
/// `FIND_MATCH_VIEW_MARGIN`.
pub(crate) const VIEW_MARGIN: f32 = 96.0;

/// One occurrence: the entry (a message id or a plan's turn item id), the
/// turn to unfold, and its index within the entry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Match {
    pub entry_id: String,
    pub run_id: Option<String>,
    pub occurrence: usize,
}

/// A search anchor (`ThreadFindStart`).
#[derive(Clone, Debug, PartialEq)]
struct Start {
    entry_id: String,
    occurrence: usize,
}

/// The request in flight.
#[derive(Clone, PartialEq)]
struct Request {
    query: String,
    start: Option<Start>,
    offset: i64,
    request_id: u64,
}

/// A server answer (`OrchestrationV2SearchThreadResult`) and the request
/// it answered.
struct Answer {
    value: Value,
    query: String,
    navigation_id: u64,
    /// Tells answers apart (`selection.base === data`).
    serial: u64,
}

/// A step taken from an answer's navigation window, without a request.
#[derive(Clone)]
struct Local {
    base_serial: u64,
    base_navigation_id: u64,
    active_index: usize,
    matched: Match,
    navigation_id: u64,
}

#[derive(Default)]
pub(crate) struct ThreadFind {
    /// The thread the field is open on.
    thread_id: Option<String>,
    pub(crate) query: String,
    focus: bool,
    start: Option<Start>,
    offset: i64,
    navigation_id: u64,
    request_id: u64,
    /// When the query last changed.
    edited_at: f64,
    /// The request in flight or last answered, and its RPC key.
    sent: Option<(String, Request)>,
    next_key: u64,
    answer: Option<Answer>,
    /// The request has not finished (a streamed one may have answered).
    pending: bool,
    error: bool,
    local: Option<Local>,
    /// The thread content the search last saw (`content`), and when a
    /// change to it refreshes the results.
    revision: u64,
    refresh_at: Option<f64>,
    refresh_after: bool,
    /// The navigation whose match the timeline brought into view.
    pub(crate) revealed: Option<u64>,
    /// History cursors already paged for the navigation in `paged_for`.
    paged: HashSet<String>,
    paged_for: u64,
}

/// The marks of a frame's timeline: the query, the row holding the active
/// match and its occurrence, and where that occurrence was drawn.
pub(crate) struct FindPaint {
    pub query: String,
    /// The row's id: a message id, or a plan's id.
    pub row_id: String,
    pub occurrence: usize,
    pub active_rect: Option<Rect>,
}

impl App {
    /// The marks for the row `row_id` (`findQuery` is set once a match is).
    pub(crate) fn find_marks(&self, row_id: &str) -> Option<crate::ui::markdown::FindMarks> {
        let paint = self.find_paint.as_ref()?;
        let active = (paint.row_id == row_id).then_some(paint.occurrence);
        Some(crate::ui::markdown::FindMarks::new(&paint.query, active))
    }

    /// Keeps where the row's marks drew the active occurrence.
    pub(crate) fn found_mark(
        &mut self,
        marks: Option<crate::ui::markdown::FindMarks>,
    ) -> Option<Rect> {
        let rect = marks?.active_rect?;
        if let Some(paint) = &mut self.find_paint {
            paint.active_rect = Some(rect);
        }
        Some(rect)
    }
}

/// What the timeline draws for an open search with a selected match.
pub(crate) struct Selection {
    pub query: String,
    pub matched: Match,
    pub navigation_id: u64,
}

/// `stepThreadFindIndex`.
fn step_index(index: usize, total: usize, delta: i64) -> usize {
    if total == 0 {
        return 0;
    }
    let total = total as i64;
    ((index.min(total as usize - 1) as i64 + delta).rem_euclid(total)) as usize
}

/// `formatThreadFindCount`.
fn count_label(index: usize, total: usize) -> String {
    if total == 0 {
        "0/0".into()
    } else {
        format!("{}/{total}", index.min(total - 1) + 1)
    }
}

fn read_match(value: &Value) -> Option<Match> {
    Some(Match {
        entry_id: value["entryId"].as_str()?.to_owned(),
        run_id: value["runId"].as_str().map(str::to_owned),
        occurrence: value["occurrence"].as_u64()? as usize,
    })
}

impl ThreadFind {
    pub(crate) fn is_open(&self) -> bool {
        self.thread_id.is_some()
    }

    fn trimmed(&self) -> &str {
        self.query.trim()
    }

    /// The answer for the current query (`data` while the key matches).
    fn current(&self) -> Option<&Answer> {
        self.answer
            .as_ref()
            .filter(|a| !self.error && a.query == self.trimmed())
    }

    fn total(&self) -> usize {
        self.current()
            .and_then(|a| a.value["totalMatches"].as_u64())
            .unwrap_or(0) as usize
    }

    fn counting(&self) -> bool {
        self.current()
            .is_some_and(|a| a.value["complete"] == Value::Bool(false))
    }

    /// `rebaseLocalSelection`: a response for an older anchor can update
    /// counts without undoing newer local steps.
    fn rebased(&self) -> Option<Local> {
        let local = self.local.as_ref()?;
        let answer = self.current()?;
        if local.base_serial == answer.serial && local.base_navigation_id == answer.navigation_id {
            return Some(local.clone());
        }
        if answer.navigation_id >= local.navigation_id {
            return None;
        }
        let entry = answer.value["navigation"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["entryId"] == local.matched.entry_id.as_str());
        let Some(entry) = entry else {
            return Some(local.clone());
        };
        let count = entry["count"].as_u64().unwrap_or(0) as usize;
        if local.matched.occurrence >= count {
            return Some(local.clone());
        }
        let mut local = local.clone();
        local.active_index =
            entry["startIndex"].as_u64().unwrap_or(0) as usize + local.matched.occurrence;
        Some(local)
    }

    fn active_index(&self) -> usize {
        self.rebased().map_or_else(
            || {
                self.current()
                    .and_then(|a| a.value["activeIndex"].as_u64())
                    .unwrap_or(0) as usize
            },
            |l| l.active_index,
        )
    }

    fn selected(&self) -> Option<Match> {
        self.rebased()
            .map(|l| l.matched)
            .or_else(|| self.current().and_then(|a| read_match(&a.value["match"])))
    }

    /// The navigation the shown selection belongs to.
    fn shown_navigation(&self) -> u64 {
        self.rebased().map_or_else(
            || self.answer.as_ref().map_or(0, |a| a.navigation_id),
            |l| l.navigation_id,
        )
    }

    /// The query is still settling, asked, or streaming (`isPending`).
    fn busy(&self, now: f64) -> bool {
        !self.trimmed().is_empty() && (now - self.edited_at < DEBOUNCE || self.pending)
    }

    /// What the timeline marks and scrolls to.
    pub(crate) fn selection(&self) -> Option<Selection> {
        self.is_open().then_some(())?;
        Some(Selection {
            query: self.trimmed().to_owned(),
            matched: self.selected()?,
            navigation_id: self.shown_navigation(),
        })
    }

    /// The query changed (`onQueryChange`), searching from `start`.
    fn edited(&mut self, now: f64, start: Option<Start>) {
        self.edited_at = now;
        self.error = false;
        self.start = start;
        self.offset = 0;
        self.local = None;
        self.navigation_id += 1;
        self.request_id = self.navigation_id;
    }

    /// `step`: within the answer's navigation window when it can, else
    /// asks the server relative to the selection.
    fn step(&mut self, delta: i64, now: f64, content_changed: bool) {
        let total = self.total();
        if total == 0 || self.counting() {
            return;
        }
        let local = self.rebased();
        let pending = self.navigation_id != self.shown_navigation();
        let selected = self.selected();
        let next = step_index(self.active_index(), total, delta);
        let next_navigation = self.navigation_id + 1;
        let entry = self.current().and_then(|a| {
            a.value["navigation"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|e| {
                    let start = e["startIndex"].as_u64().unwrap_or(0) as usize;
                    let count = e["count"].as_u64().unwrap_or(0) as usize;
                    next >= start && next < start + count
                })
                .cloned()
        });
        let answer = self.current();
        let base_ok = match (&local, answer) {
            (None, _) => true,
            (Some(local), Some(answer)) => {
                local.base_serial == answer.serial
                    || answer.value["navigation"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|e| {
                            e["entryId"] == local.matched.entry_id.as_str()
                                && (local.matched.occurrence as u64)
                                    < e["count"].as_u64().unwrap_or(0)
                        })
            }
            (Some(_), None) => false,
        };
        if let (false, false, Some(entry), Some(answer), true, None, false) = (
            pending,
            self.busy(now),
            entry,
            answer,
            base_ok,
            self.refresh_at,
            content_changed,
        ) {
            let start = entry["startIndex"].as_u64().unwrap_or(0) as usize;
            self.local = Some(Local {
                base_serial: answer.serial,
                base_navigation_id: answer.navigation_id,
                active_index: next,
                matched: Match {
                    entry_id: entry["entryId"].as_str().unwrap_or_default().to_owned(),
                    run_id: entry["runId"].as_str().map(str::to_owned),
                    occurrence: next - start,
                },
                navigation_id: next_navigation,
            });
            self.navigation_id = next_navigation;
            return;
        }
        if !pending && let Some(selected) = selected {
            self.start = Some(Start {
                entry_id: selected.entry_id,
                occurrence: selected.occurrence,
            });
        }
        self.offset = if pending { self.offset + delta } else { delta };
        self.navigation_id = next_navigation;
        self.request_id = next_navigation;
    }

    /// Asks again for the same request (Retry, live refresh).
    fn refresh(&mut self) {
        self.sent = None;
        self.error = false;
    }

    /// `refreshLiveResults`: re-anchors on the selection so matches added
    /// before it do not move it.
    fn refresh_live(&mut self) {
        if self.trimmed().is_empty() {
            return;
        }
        if self.pending {
            self.refresh_after = true;
            return;
        }
        let Some(selected) = self.selected() else {
            self.refresh();
            return;
        };
        if self.navigation_id != self.shown_navigation() {
            self.refresh();
            return;
        }
        let start = Start {
            entry_id: selected.entry_id,
            occurrence: selected.occurrence,
        };
        if self.offset == 0
            && self.start.as_ref() == Some(&start)
            && self.request_id == self.navigation_id
        {
            self.refresh();
        } else {
            self.start = Some(start);
            self.offset = 0;
            self.request_id = self.navigation_id;
        }
    }

    /// Whether the timeline should page older turns in for the selection:
    /// once per cursor for each navigation.
    pub(crate) fn page_for(&mut self, navigation_id: u64, cursor: &str) -> bool {
        if self.paged_for != navigation_id {
            self.paged_for = navigation_id;
            self.paged.clear();
        }
        self.paged.insert(cursor.to_owned())
    }
}

impl App {
    /// A fingerprint of the open thread's timeline: an applied event moves
    /// its time, a loaded page its rows.
    fn find_content(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(detail) = self.current_detail() {
            (detail.id(), &detail.updated_at, detail.visible.len()).hash(&mut hasher);
        }
        hasher.finish()
    }

    fn thread_find_enabled(&self) -> bool {
        self.state.config["threadFind"] == true && self.current_thread().is_some()
    }

    /// `chat.find` (Mod+F): opens the field, or focuses it again; an open
    /// query searches again from the reading position.
    pub(crate) fn open_thread_find(&mut self) {
        let Some(thread) = self.current_thread() else {
            return;
        };
        if self.state.config["threadFind"] != true {
            self.toasts.add(
                Toast::new(Kind::Plain, "Thread search is unavailable on this server.")
                    .description("Update the server to enable it."),
            );
            return;
        }
        let now = self.egui_ctx.as_ref().map_or(0.0, |c| c.input(|i| i.time));
        if self.find.thread_id.as_deref() != Some(thread.id.as_str()) {
            self.find = ThreadFind {
                thread_id: Some(thread.id.clone()),
                revision: self.find_content(),
                ..Default::default()
            };
        } else if !self.find.trimmed().is_empty() {
            let start = self.find_reading_position();
            self.find.edited(now, start);
            self.find.edited_at = now - DEBOUNCE;
        }
        self.find.focus = true;
    }

    pub(crate) fn close_thread_find(&mut self) {
        if let Some((key, _)) = self.find.sent.take() {
            self.send(Action::StopStream(key));
        }
        self.find = ThreadFind::default();
    }

    /// The first searchable entry in view (`readThreadFindPosition`).
    fn find_reading_position(&self) -> Option<Start> {
        self.timeline_view
            .first_visible_entry()
            .map(|entry_id| Start {
                entry_id,
                occurrence: 0,
            })
    }

    /// Sends the request the field asks for and refreshes live results;
    /// runs every frame.
    pub(crate) fn poll_thread_find(&mut self, ctx: &egui::Context) {
        let Some(thread_id) = self.find.thread_id.clone() else {
            return;
        };
        if self.selected.as_deref() != Some(thread_id.as_str()) || !self.thread_find_enabled() {
            self.close_thread_find();
            return;
        }
        let now = ctx.input(|i| i.time);
        let query = self.find.trimmed().to_owned();
        if query.is_empty() {
            if let Some((key, _)) = self.find.sent.take() {
                self.send(Action::StopStream(key));
            }
            self.find.pending = false;
            return;
        }
        let settle = DEBOUNCE - (now - self.find.edited_at);
        if settle > 0.0 {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(settle));
            return;
        }
        // The thread changed under the search: refresh at most every 300ms.
        let content = self.find_content();
        if content != self.find.revision {
            self.find.revision = content;
            self.find.refresh_at.get_or_insert(now + REFRESH);
        }
        if let Some(at) = self.find.refresh_at {
            if now >= at {
                self.find.refresh_at = None;
                self.find.refresh_live();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            }
        }
        if self.find.refresh_after && !self.find.pending {
            self.find.refresh_after = false;
            if !self.find.error {
                self.find.refresh_live();
            }
        }
        let request = Request {
            query: query.clone(),
            start: self.find.start.clone(),
            offset: self.find.offset,
            request_id: self.find.request_id,
        };
        if self.find.error
            || self
                .find
                .sent
                .as_ref()
                .is_some_and(|(_, sent)| *sent == request)
        {
            return;
        }
        if let Some((key, _)) = self.find.sent.take() {
            self.send(Action::StopStream(key));
        }
        self.find.next_key += 1;
        let key = format!("thread-find:{}", self.find.next_key);
        let mut payload = json!({"threadId": thread_id, "query": query});
        if let Some(start) = &request.start {
            payload["start"] = json!({"entryId": start.entry_id, "occurrence": start.occurrence});
        }
        if request.offset != 0 {
            payload["offset"] = json!(request.offset);
        }
        if self.state.config["threadFindProgressive"] == true {
            self.send(Action::Stream {
                key: key.clone(),
                method: "orchestration.searchThreadStream",
                payload,
            });
        } else {
            self.send(Action::Request {
                key: key.clone(),
                method: "orchestration.searchThread",
                payload,
            });
        }
        self.find.sent = Some((key, request));
        self.find.pending = true;
    }

    /// A search answer; true when `key` was one.
    pub(crate) fn thread_find_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if !key.starts_with("thread-find:") {
            return false;
        }
        let Some((sent_key, request)) = &self.find.sent else {
            return true;
        };
        if sent_key != key {
            return true;
        }
        match result {
            // A stream's end.
            Ok(Value::Null) => self.find.pending = false,
            Ok(value) => {
                let serial = self.find.answer.as_ref().map_or(1, |a| a.serial + 1);
                self.find.pending = value["complete"] == Value::Bool(false);
                self.find.answer = Some(Answer {
                    value: value.clone(),
                    query: request.query.clone(),
                    navigation_id: request.request_id,
                    serial,
                });
            }
            Err(_) => {
                self.find.pending = false;
                self.find.error = true;
            }
        }
        true
    }

    /// Escape closes an open search and returns to the composer.
    pub(crate) fn dismiss_thread_find(&mut self, ctx: &egui::Context) -> bool {
        if !self.find.is_open() {
            return false;
        }
        self.close_thread_find();
        if let Some(editor) = self.cite.editor_id {
            ctx.memory_mut(|m| m.request_focus(editor));
        }
        true
    }

    /// `ThreadFindBar`: the field, the count, Retry, previous, next and
    /// close, at the top right of the chat (`top-3 right-3`).
    pub(crate) fn thread_find_bar(&mut self, ui: &mut Ui, body: Rect) {
        if !self.find.is_open() {
            return;
        }
        let now = ui.input(|i| i.time);
        // Escape closes it (`dismissFind`), unless the palette, a terminal
        // or the model picker has the key.
        let model_picker = self
            .open_menu
            .as_deref()
            .is_some_and(|menu| menu.contains("model"));
        if !self.palette.open
            && !self.terminal_focused()
            && !model_picker
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.dismiss_thread_find(ui.ctx());
            return;
        }
        let width = BAR_WIDTH.min(body.width() - 24.0);
        let bar = Rect::from_min_size(
            Pos2::new(body.right() - 12.0 - width, body.top() + 12.0),
            Vec2::new(width, BAR_HEIGHT),
        );
        // Over the timeline (`z-40`).
        egui::Area::new(egui::Id::new("thread-find-bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(bar.min)
            .show(ui.ctx(), |ui| self.thread_find_field(ui, bar, now));
    }

    fn thread_find_field(&mut self, ui: &mut Ui, bar: Rect, now: f64) {
        ui.set_clip_rect(bar.expand(1.0));
        let painter = ui.painter();
        let input_id = ui.id().with("thread-find-input");
        let focused = ui.memory(|m| m.has_focus(input_id));
        // `dropdown-glass rounded-2xl`; focus darkens the hairline.
        let outline = if focused {
            theme::stock_or(Color32::from_rgba_premultiplied(46, 46, 46, 46), |_| {
                theme::overlay(0.18)
            })
        } else {
            widgets::menu_outline()
        };
        painter.rect(
            bar,
            CornerRadius::same(16),
            widgets::menu_surface(),
            Stroke::new(1.0, outline),
            StrokeKind::Inside,
        );
        ui.interact(bar, ui.id().with("thread-find-bar"), Sense::click());

        let total = self.find.total();
        let counting = self.find.counting();
        let status_loading = self.find.busy(now) && self.find.current().is_none();
        let has_query = !self.find.trimmed().is_empty();
        let mut label = String::new();
        if has_query {
            label = if counting {
                "1/…".into()
            } else {
                count_label(self.find.active_index(), total)
            };
        }
        if status_loading {
            label = "Searching…".into();
        }
        if self.find.error {
            label = "Search failed".into();
        }
        let no_results = has_query && !status_loading && !self.find.error && total == 0;
        let navigation_disabled = total == 0 || status_loading || counting;

        // The inline-end addon: `pe-1 gap-0.5`, 24px pill buttons.
        let button = 24.0;
        let mut right = bar.right() - 4.0;
        let mut slot = |width: f32| {
            let rect = Rect::from_min_size(
                Pos2::new(right - width, bar.center().y - button / 2.0),
                Vec2::new(width, button),
            );
            right = rect.left() - 2.0;
            rect
        };
        let close = slot(button);
        let next = slot(button);
        let previous = slot(button);
        let retry_font = font(12.0, Weight::Medium);
        let retry = self.find.error.then(|| {
            let width = widgets::text_width(ui, "Retry", retry_font.clone()) + 14.0;
            slot(width)
        });
        let label_font = font(12.0, Weight::Regular);
        let label_width = widgets::text_width(ui, &label, label_font.clone()).max(40.0);
        let label_rect = slot(label_width);
        // `gap-2` between the input and the addon.
        let input_right = label_rect.left() - 8.0 + 2.0;

        let mut clicked_previous = false;
        let mut clicked_next = false;
        let mut clicked_close = false;
        let pill = |ui: &mut Ui, rect: Rect, id: &str, enabled: bool| {
            let sense = if enabled {
                Sense::click()
            } else {
                Sense::hover()
            };
            let response = ui.interact(rect, ui.id().with(id), sense);
            if enabled && response.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(12), theme::accent());
            }
            response
        };
        for (rect, icon, id, enabled, flag, tooltip) in [
            (
                previous,
                "chevron-up",
                "find-previous",
                !navigation_disabled,
                &mut clicked_previous,
                "Previous match",
            ),
            (
                next,
                "chevron-down",
                "find-next",
                !navigation_disabled,
                &mut clicked_next,
                "Next match",
            ),
            (
                close,
                "x",
                "find-close",
                true,
                &mut clicked_close,
                "Close find",
            ),
        ] {
            let response = pill(ui, rect, id, enabled);
            let tint = if !enabled {
                theme::alpha(theme::foreground(), 0.64 * 0.5)
            } else if response.hovered() {
                theme::foreground()
            } else {
                theme::alpha(theme::foreground(), 0.64)
            };
            theme::paint_icon(ui, icon, rect, 16.0, tint);
            *flag = response.on_hover_text(tooltip).clicked();
        }
        if let Some(retry) = retry {
            let response = pill(ui, retry, "find-retry", true);
            widgets::text_in(
                ui,
                retry.shrink2(Vec2::new(7.0, 0.0)),
                "Retry",
                retry_font,
                theme::foreground(),
            );
            if response.clicked() {
                self.find.refresh();
            }
        }
        let label_color: Color32 = if no_results {
            DESTRUCTIVE
        } else {
            theme::muted_foreground()
        };
        let text_width = widgets::text_width(ui, &label, label_font.clone());
        widgets::text_in(
            ui,
            Rect::from_center_size(label_rect.center(), Vec2::new(text_width, button)),
            &label,
            label_font,
            label_color,
        );

        // `InputGroupInput`: borderless, 14px, 11px from the left edge.
        let input = Rect::from_min_max(
            Pos2::new(bar.left() + 11.0, bar.top() + 8.0),
            Pos2::new(input_right, bar.bottom() - 8.0),
        );
        let mut edit_ui = ui.new_child(egui::UiBuilder::new().max_rect(input));
        let mut query = self.find.query.clone();
        let (enter, shift) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift));
        let response = edit_ui.add(
            egui::TextEdit::singleline(&mut query)
                .id(input_id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .char_limit(200)
                .font(font(14.0, Weight::Regular))
                .text_color(theme::foreground())
                .hint_text(
                    egui::RichText::new("Find in thread")
                        .font(font(14.0, Weight::Regular))
                        .color(theme::muted_foreground()),
                )
                .desired_width(input.width()),
        );
        if std::mem::take(&mut self.find.focus) {
            response.request_focus();
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), input_id) {
                let all = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(query.chars().count()),
                );
                state.cursor.set_char_range(Some(all));
                state.store(ui.ctx(), input_id);
            }
        }
        if query != self.find.query {
            self.find.query = query;
            let start = self.find_reading_position();
            self.find.edited(now, start);
        }
        // Enter keeps the field focused (egui drops a single line's focus).
        let submitted = response.lost_focus() && enter;
        if submitted {
            response.request_focus();
        }
        let content_changed = self.find_content() != self.find.revision;
        if (submitted && !shift) || clicked_next {
            self.find.step(1, now, content_changed);
        }
        if (submitted && shift) || clicked_previous {
            self.find.step(-1, now, content_changed);
        }
        if clicked_close {
            self.dismiss_thread_find(ui.ctx());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(find: &mut ThreadFind, value: Value) {
        let serial = find.answer.as_ref().map_or(1, |a| a.serial + 1);
        find.answer = Some(Answer {
            value,
            query: find.trimmed().to_owned(),
            navigation_id: find.request_id,
            serial,
        });
        find.pending = false;
    }

    #[test]
    fn occurrences_fold_case_without_overlaps() {
        use crate::ui::markdown::{find_occurrences, fold_find_case};
        let query = fold_find_case("AA");
        assert_eq!(find_occurrences("aaaa Aa", &query), [0..2, 2..4, 5..7]);
        let sigma = find_occurrences("ΣΑΣ", &fold_find_case("σας"));
        assert_eq!((sigma.len(), sigma[0].clone()), (1, 0..3));
        assert!(find_occurrences("abc", &[]).is_empty());
    }

    #[test]
    fn counts_and_steps_wrap() {
        assert_eq!(count_label(0, 0), "0/0");
        assert_eq!(count_label(2, 5), "3/5");
        assert_eq!(step_index(4, 5, 1), 0);
        assert_eq!(step_index(0, 5, -1), 4);
    }

    #[test]
    fn steps_within_the_navigation_window_then_ask_the_server() {
        let mut find = ThreadFind {
            thread_id: Some("t".into()),
            query: "plan".into(),
            ..Default::default()
        };
        find.edited(0.0, None);
        answer(
            &mut find,
            json!({"snapshotSequence":1,"totalMatches":3,"activeIndex":0,
                "match":{"entryId":"m1","runId":"r1","occurrence":0},
                "navigation":[{"entryId":"m1","runId":"r1","startIndex":0,"count":2},
                    {"entryId":"m2","runId":null,"startIndex":2,"count":1}]}),
        );
        let now = 1.0;
        find.step(1, now, false);
        let selected = find.selected().unwrap();
        assert_eq!((selected.entry_id.as_str(), selected.occurrence), ("m1", 1));
        find.step(1, now, false);
        assert_eq!(find.selected().unwrap().entry_id, "m2");
        assert_eq!(find.active_index(), 2);
        // Past the window's end it wraps locally too.
        find.step(1, now, false);
        assert_eq!(find.active_index(), 0);
        // A step outside the window asks the server relative to the selection.
        find.edited(0.0, None);
        answer(
            &mut find,
            json!({"snapshotSequence":1,"totalMatches":30,"activeIndex":1,
                "match":{"entryId":"m1","runId":"r1","occurrence":1},
                "navigation":[{"entryId":"m1","runId":"r1","startIndex":0,"count":2}]}),
        );
        let before = find.request_id;
        find.step(1, now, false);
        assert!(find.request_id > before);
        assert_eq!(find.offset, 1);
        let start = find.start.clone().unwrap();
        assert_eq!((start.entry_id.as_str(), start.occurrence), ("m1", 1));
    }
}
