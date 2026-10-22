//! Project content search (`search/ProjectContentSearchDialog.tsx`,
//! `search/HighlightedSearchLine.tsx`): `projects.searchContents` matches
//! grouped by file, with syntax-highlighted lines and marked matches, inside
//! the command palette's frame. Opening a match reveals its line in the
//! file's right-panel tab.
//!
//! Not ported: the option tooltips and the 100-row rendering windows (every
//! match is laid out).
use std::collections::HashMap;

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const DEBOUNCE: f64 = 0.12;
const LIMIT: usize = 500;
const STATUS: f32 = 36.0;
const HEADER: f32 = 32.0;
const ROW: f32 = 28.0;
/// The option group's width (`Aa`, `ab`, `.*`), right of the input.
pub(crate) const OPTIONS_WIDTH: f32 = 126.0;

/// The dialog's options, typing state and results.
#[derive(Default)]
pub struct ContentSearch {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    selected: usize,
    /// The query as last typed, and when (the 120ms debounce).
    typed: String,
    typed_at: f64,
    /// The request whose matches are shown; older ones stay up while a newer
    /// query is pending.
    shown: Option<String>,
    results: HashMap<String, Result<Value, String>>,
}

impl ContentSearch {
    /// A fresh dialog keeps the cached results only.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            results: std::mem::take(&mut self.results),
            ..Self::default()
        };
    }

    pub(crate) fn receive(&mut self, key: &str, result: Result<Value, String>) -> bool {
        if !key.starts_with("content-search:") {
            return false;
        }
        self.results.insert(key.to_owned(), result);
        true
    }
}

struct Match {
    path: String,
    line: usize,
    content: String,
    /// Merged byte ranges into `content`.
    ranges: Vec<(usize, usize)>,
}

fn parse_matches(value: &Value) -> Vec<Match> {
    value["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let content = m["lineContent"].as_str()?.to_owned();
            let ranges = m["matchRanges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| Some((r["start"].as_u64()? as usize, r["end"].as_u64()? as usize)))
                .collect::<Vec<_>>();
            Some(Match {
                path: m["path"].as_str()?.to_owned(),
                line: m["lineNumber"].as_u64()? as usize,
                ranges: normalize_ranges(&content, &ranges),
                content,
            })
        })
        .collect()
}

/// `normalizeRanges`: clamps the UTF-16 ranges to the line, drops empty
/// ones, sorts and merges overlaps; returned as byte ranges.
fn normalize_ranges(line: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let length = line.encode_utf16().count();
    let mut ranges: Vec<(usize, usize)> = ranges
        .iter()
        .map(|&(start, end)| (start.min(length), end.min(length)))
        .filter(|(start, end)| end > start)
        .collect();
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(previous) if start <= previous.1 => previous.1 = previous.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let byte = |unit: usize| {
        let mut units = 0;
        for (index, c) in line.char_indices() {
            if units >= unit {
                return index;
            }
            units += c.len_utf16();
        }
        line.len()
    };
    merged
        .into_iter()
        .map(|(start, end)| (byte(start), byte(end)))
        .collect()
}

/// `toLocaleString` for counts (en-US grouping).
fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl App {
    /// The workspace and project name behind the open thread.
    pub(crate) fn content_search_target(&self) -> Option<(String, String)> {
        let thread = self.current_thread()?;
        let project = self.state.project(&thread.project_id)?.title.clone();
        Some((self.workspace_cwd()?, project))
    }

    /// The option toggles right of the search input (`SearchOptionButton`).
    pub(crate) fn content_search_options(&mut self, ui: &mut Ui, search: Rect) {
        let group = Rect::from_min_size(
            Pos2::new(
                search.right() + 8.0 - 10.0 - OPTIONS_WIDTH,
                search.center().y - 15.0,
            ),
            Vec2::new(OPTIONS_WIDTH, 30.0),
        );
        ui.painter().rect(
            group,
            CornerRadius::same(8),
            theme::alpha(theme::muted(), 0.3),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        let width = (OPTIONS_WIDTH - 2.0 - 4.0 - 4.0) / 3.0;
        let options = [
            ("Aa", self.content_search.case_sensitive),
            ("ab", self.content_search.whole_word),
            (".*", self.content_search.use_regex),
        ];
        for (index, (label, pressed)) in options.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(
                    group.left() + 3.0 + index as f32 * (width + 2.0),
                    group.top() + 3.0,
                ),
                Vec2::new(width, 24.0),
            );
            let response = ui.interact(
                button,
                ui.id().with(("content-option", index)),
                Sense::click(),
            );
            let fill = if pressed {
                theme::alpha(theme::input(), 0.72)
            } else if response.hovered() {
                theme::alpha(theme::input(), 0.32)
            } else {
                Color32::TRANSPARENT
            };
            ui.painter()
                .rect_filled(button, CornerRadius::same(8), fill);
            let color = if pressed || response.hovered() {
                theme::foreground()
            } else {
                theme::muted_foreground()
            };
            let mono = theme::mono(14.0);
            let text_width = widgets::text_width(ui, label, mono.clone());
            let left = (button.center().x - text_width / 2.0).round();
            widgets::text_in(
                ui,
                Rect::from_min_max(
                    Pos2::new(left, button.top()),
                    Pos2::new(button.right(), button.bottom()),
                ),
                label,
                mono,
                color,
            );
            if label == "ab" {
                // `underline decoration-2 underline-offset-2`.
                let y = button.center().y + 6.0;
                ui.painter().line_segment(
                    [Pos2::new(left, y), Pos2::new(left + text_width, y)],
                    Stroke::new(2.0, color),
                );
            }
            if response.clicked() {
                let flag = match index {
                    0 => &mut self.content_search.case_sensitive,
                    1 => &mut self.content_search.whole_word,
                    _ => &mut self.content_search.use_regex,
                };
                *flag = !*flag;
            }
        }
    }

    /// Everything between the search row and the footer.
    pub(crate) fn content_search_body(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        query: &str,
        keys: (bool, bool, bool),
    ) -> bool {
        let Some((cwd, _)) = self.content_search_target() else {
            paint_centered(ui, body, "Open a project to search its files.");
            return false;
        };
        let now = ui.input(|i| i.time);
        let state = &mut self.content_search;
        if query != state.typed {
            state.typed = query.to_owned();
            state.typed_at = now;
        }
        let has_query = !query.trim().is_empty();
        let settled = now - state.typed_at >= DEBOUNCE;
        if !settled {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(DEBOUNCE));
        }
        let key = format!(
            "content-search:{cwd}\0{}{}{}\0{query}",
            state.case_sensitive as u8, state.whole_word as u8, state.use_regex as u8
        );
        let mut pending = false;
        if has_query && settled {
            if !state.results.contains_key(&key) {
                pending = true;
                if state.shown.as_deref() != Some(key.as_str()) {
                    state.shown = Some(key.clone());
                    state.selected = 0;
                    let payload = json!({
                        "cwd": cwd,
                        "query": query,
                        "limit": LIMIT,
                        "caseSensitive": state.case_sensitive,
                        "wholeWord": state.whole_word,
                        "useRegex": state.use_regex,
                    });
                    self.send_request(key.clone(), "projects.searchContents", payload);
                }
            } else if self.content_search.shown.as_deref() != Some(key.as_str()) {
                self.content_search.shown = Some(key.clone());
                self.content_search.selected = 0;
            }
        } else if has_query {
            pending = true;
        }
        let state = &mut self.content_search;
        let result = if has_query {
            state.shown.as_ref().and_then(|k| state.results.get(k))
        } else {
            None
        };
        let (matches, truncated, error, invalid_regex) = match result {
            Some(Ok(value)) => (
                parse_matches(value),
                value["truncated"] == true,
                None,
                state.use_regex && value["regexFallbackError"].is_string(),
            ),
            Some(Err(error)) => (Vec::new(), false, Some(error.clone()), false),
            None => (Vec::new(), false, None, false),
        };
        let pending = pending || (has_query && result.is_none());
        let error = error.filter(|_| !pending);

        // Keyboard: arrows move through the matches, Enter opens one.
        let (up, down, enter) = keys;
        let count = matches.len();
        if count > 0 {
            if down {
                state.selected = (state.selected + 1) % count;
            }
            if up {
                state.selected = (state.selected + count - 1) % count;
            }
        }
        state.selected = state.selected.min(count.saturating_sub(1));
        let selected = state.selected;

        let mut top = body.top();
        let show_status = has_query || pending || error.is_some() || invalid_regex;
        if show_status {
            let status = Rect::from_min_size(body.min, Vec2::new(body.width(), STATUS));
            ui.painter().rect_filled(
                Rect::from_min_max(Pos2::new(status.left(), status.bottom() - 1.0), status.max),
                0.0,
                theme::border(),
            );
            let text = Rect::from_min_max(status.min + Vec2::new(12.0, 0.0), status.max);
            let small = font(12.0, Weight::Regular);
            if pending {
                paint_spinner(ui, Pos2::new(text.left() + 7.0, text.center().y), 14.0);
                widgets::text_in(
                    ui,
                    Rect::from_min_max(text.min + Vec2::new(22.0, 0.0), text.max),
                    "Searching…",
                    small,
                    theme::muted_foreground(),
                );
            } else if let Some(error) = &error {
                widgets::text_in(ui, text, error, small, theme::error_foreground());
            } else if invalid_regex {
                widgets::text_in(
                    ui,
                    text,
                    "Invalid regular expression",
                    small,
                    theme::error_foreground(),
                );
            } else {
                let files = matches
                    .iter()
                    .map(|m| m.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                let label = format!(
                    "{}{} results in {} files",
                    grouped(count),
                    if truncated { "+" } else { "" },
                    grouped(files)
                );
                widgets::text_in(ui, text, &label, small, theme::muted_foreground());
            }
            top = status.bottom();
        }
        let list = Rect::from_min_max(Pos2::new(body.left(), top), body.max);
        if matches.is_empty() {
            let message = if has_query && !pending && error.is_none() {
                "No results found."
            } else {
                "Type to search across your project."
            };
            paint_centered(ui, list, message);
            return false;
        }

        // Matches grouped by file, in server order.
        let mut groups: Vec<(String, Vec<(usize, &Match)>)> = Vec::new();
        for (index, m) in matches.iter().enumerate() {
            match groups.iter_mut().find(|(path, _)| *path == m.path) {
                Some((_, rows)) => rows.push((index, m)),
                None => groups.push((m.path.clone(), vec![(index, m)])),
            }
        }
        let mut hovered = None;
        let mut open = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        let mut headers: Vec<(f32, f32, &str, usize)> = Vec::new();
        let output = egui::ScrollArea::vertical()
            .id_salt("content-search-results")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let origin = ui.max_rect().min;
                let mut y = origin.y + 8.0;
                for (path, rows) in &groups {
                    let group_top = y;
                    y += HEADER;
                    for (index, m) in rows {
                        let row = Rect::from_min_size(
                            Pos2::new(list.left(), y),
                            Vec2::new(list.width(), ROW),
                        );
                        let response = ui.interact(
                            row,
                            ui.id().with(("content-match", *index)),
                            Sense::click(),
                        );
                        if response.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                            hovered = Some(*index);
                        }
                        if response.clicked() {
                            open = Some(*index);
                        }
                        if *index == selected {
                            ui.painter().rect_filled(row, 0.0, theme::accent());
                            if up || down {
                                ui.scroll_to_rect(row, None);
                            }
                        } else if response.hovered() {
                            ui.painter()
                                .rect_filled(row, 0.0, theme::alpha(theme::accent(), 0.6));
                        }
                        self.paint_match(ui, row, path, m);
                        y += ROW;
                    }
                    y += 8.0;
                    headers.push((group_top, y - 8.0, path.as_str(), rows.len()));
                }
                ui.allocate_rect(
                    Rect::from_min_max(origin, Pos2::new(list.right(), y + 8.0)),
                    Sense::hover(),
                );
            });
        // Sticky file headers: each stays at the top while its rows scroll.
        let mut sticky = ui.new_child(egui::UiBuilder::new().max_rect(list));
        sticky.set_clip_rect(list.intersect(ui.clip_rect()));
        for (group_top, group_bottom, path, count) in headers {
            let top = group_top.max(list.top()).min(group_bottom - HEADER);
            let header =
                Rect::from_min_size(Pos2::new(list.left(), top), Vec2::new(list.width(), HEADER));
            if header.bottom() > list.top() && header.top() < list.bottom() {
                paint_header(&sticky, header, path, count);
            }
        }
        // `scrollFade`: 1.5rem edges fade where more content lies beyond.
        let below = output.content_size.y - output.state.offset.y - output.inner_rect.height();
        paint_fade(&sticky, list, output.state.offset.y.min(24.0), true);
        paint_fade(&sticky, list, below.min(24.0), false);
        if let Some(index) = hovered {
            self.content_search.selected = index;
        }
        let open = open.or(enter.then_some(selected));
        if !pending && let Some(m) = open.and_then(|index| matches.get(index)) {
            self.open_file_at(m.path.clone(), m.line);
            return true;
        }
        false
    }

    /// One match row: the line number and the highlighted line.
    fn paint_match(&mut self, ui: &Ui, row: Rect, path: &str, m: &Match) {
        let small = theme::mono(12.0);
        let number = m.line.to_string();
        let number_width = widgets::text_width(ui, &number, small.clone());
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(row.left() + 12.0 + 40.0 - number_width, row.top()),
                Pos2::new(row.left() + 52.0, row.bottom()),
            ),
            &number,
            small.clone(),
            theme::alpha(theme::muted_foreground(), 0.7),
        );
        let extension = path.rsplit('.').next().unwrap_or_default();
        let spans = self.highlighter.spans_or_plain(&m.content, extension);
        let mut job = egui::text::LayoutJob::default();
        let mut offset = 0;
        let mut marks = Vec::new();
        for (text, color) in spans {
            let text = text.trim_end_matches('\n');
            let end = offset + text.len();
            let mut cursor = offset;
            for &(start, stop) in &m.ranges {
                if stop <= cursor || start >= end {
                    continue;
                }
                let from = start.max(cursor);
                if from > cursor {
                    job.append(
                        &m.content[cursor..from],
                        0.0,
                        widgets::format(small.clone(), color, 16.0),
                    );
                }
                let to = stop.min(end);
                marks.push((job.text.len(), job.text.len() + (to - from)));
                job.append(
                    &m.content[from..to],
                    0.0,
                    widgets::format(small.clone(), color, 16.0),
                );
                cursor = to;
            }
            if cursor < end {
                job.append(
                    &m.content[cursor..end],
                    0.0,
                    widgets::format(small.clone(), color, 16.0),
                );
            }
            offset = end;
        }
        let code = Rect::from_min_max(
            Pos2::new(row.left() + 64.0, row.top()),
            Pos2::new(row.right() - 12.0, row.bottom()),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(code.width());
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let pos = Pos2::new(code.left(), (row.center().y - 8.0).round());
        let mark = theme::alpha(theme::primary(), 0.25);
        for (start, end) in marks {
            let x = |byte: usize| {
                let ccursor = egui::text::CCursor::new(galley.text()[..byte].chars().count());
                galley.pos_from_cursor(ccursor).min.x
            };
            let rect = Rect::from_min_max(
                Pos2::new(pos.x + x(start), pos.y),
                Pos2::new(pos.x + x(end), pos.y + 16.0),
            );
            ui.painter().rect_filled(rect, CornerRadius::same(2), mark);
        }
        ui.painter().galley(pos, galley, theme::foreground());
    }
}

fn paint_header(ui: &Ui, header: Rect, path: &str, count: usize) {
    ui.painter()
        .rect_filled(header, 0.0, theme::alpha(theme::popover(), 0.95));
    let (name, directory) = match path.rfind('/') {
        Some(index) => (&path[index + 1..], &path[..index]),
        None => (path, ""),
    };
    let (icon, color) = crate::ui::files::file_icon(path);
    theme::paint_icon(
        ui,
        icon,
        Rect::from_min_size(
            Pos2::new(header.left() + 12.0, header.center().y - 7.0),
            Vec2::splat(14.0),
        ),
        14.0,
        color,
    );
    let small = font(12.0, Weight::Regular);
    let chip_text = count.to_string();
    let chip_width = widgets::text_width(ui, &chip_text, font(10.0, Weight::Regular)) + 12.0;
    let chip = Rect::from_min_size(
        Pos2::new(header.right() - 12.0 - chip_width, header.center().y - 9.0),
        Vec2::new(chip_width, 18.0),
    );
    let left = header.left() + 34.0;
    let name_width = widgets::text_in(
        ui,
        Rect::from_min_max(
            Pos2::new(left, header.top()),
            Pos2::new(chip.left() - 8.0, header.bottom()),
        ),
        name,
        font(12.0, Weight::Medium),
        theme::foreground(),
    );
    if !directory.is_empty() {
        widgets::text_in(
            ui,
            Rect::from_min_max(
                Pos2::new(left + name_width + 8.0, header.top()),
                Pos2::new(chip.left() - 8.0, header.bottom()),
            ),
            directory,
            small,
            theme::muted_foreground(),
        );
    }
    ui.painter()
        .rect_filled(chip, CornerRadius::same(9), theme::muted());
    widgets::text_in(
        ui,
        Rect::from_min_max(chip.min + Vec2::new(6.0, 0.0), chip.max),
        &chip_text,
        font(10.0, Weight::Regular),
        theme::muted_foreground(),
    );
}

/// Fades `size` points at the top or bottom edge of `rect` into the
/// palette surface (the web client masks the scroll content instead).
fn paint_fade(ui: &Ui, rect: Rect, size: f32, top: bool) {
    if size <= 0.0 {
        return;
    }
    let solid = theme::shade(12);
    let (edge, inner) = if top {
        (rect.top(), rect.top() + size)
    } else {
        (rect.bottom(), rect.bottom() - size)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(Pos2::new(rect.left(), edge), solid);
    mesh.colored_vertex(Pos2::new(rect.right(), edge), solid);
    mesh.colored_vertex(Pos2::new(rect.right(), inner), Color32::TRANSPARENT);
    mesh.colored_vertex(Pos2::new(rect.left(), inner), Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn paint_centered(ui: &Ui, rect: Rect, message: &str) {
    let width = widgets::text_width(ui, message, font(14.0, Weight::Regular));
    widgets::text_in(
        ui,
        Rect::from_center_size(rect.center(), Vec2::new(width + 1.0, 20.0)),
        message,
        font(14.0, Weight::Regular),
        theme::muted_foreground(),
    );
}

/// `Spinner`: lucide's loader-circle (a 3/4 arc), turning once a second.
fn paint_spinner(ui: &Ui, center: Pos2, size: f32) {
    let time = ui.input(|i| i.time);
    ui.ctx().request_repaint();
    let radius = size * 9.0 / 24.0;
    let start = (time % 1.0) as f32 * std::f32::consts::TAU;
    let points: Vec<Pos2> = (0..=24)
        .map(|step| {
            let angle = start + step as f32 / 24.0 * std::f32::consts::TAU * 0.75;
            center + Vec2::angled(angle) * radius
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        Stroke::new(size * 2.0 / 24.0, theme::muted_foreground()),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_merge_and_map_to_bytes() {
        assert_eq!(
            normalize_ranges("let x", &[(0, 3), (2, 4), (9, 12)]),
            vec![(0, 4)]
        );
        // "é" is one UTF-16 unit but two bytes.
        assert_eq!(normalize_ranges("é let", &[(2, 5)]), vec![(3, 6)]);
        assert!(normalize_ranges("abc", &[(2, 2)]).is_empty());
    }

    #[test]
    fn counts_group_thousands() {
        assert_eq!(grouped(30), "30");
        assert_eq!(grouped(1234), "1,234");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
