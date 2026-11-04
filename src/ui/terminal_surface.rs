//! The Terminal surface (`ThreadTerminalDrawer.tsx`, `terminal/ghostty/surface.ts`,
//! `terminal/ghostty/renderer.ts`): a terminal session of the thread's
//! workspace, attached with `terminal.attach` (a stream of the snapshot and
//! its output), drawn from `crate::terminal`'s cell grid at upstream's
//! defaults (12px monospace, 4px padding, Ghostty's Tomorrow Night palette
//! over the app's foreground and background), typed into with
//! `terminal.write` and resized with `terminal.resize`.
//!
//! Selection offers "Add to chat" (a terminal excerpt for the composer),
//! Copy, Paste, Select all and Jump to latest. Programs that ask for the
//! mouse get xterm reports (Shift keeps it for selecting); OSC 8
//! hyperlinks, URLs and paths (`terminal-links.ts`, across soft-wrapped
//! lines) open on click, paths in the preferred editor. The cursor blinks
//! while focused. Closing asks first (`terminalCloseConfirm.ts`).
//!
//! Not ported: the Kitty keyboard protocol, links opening in the preview
//! browser, server session labels and the read-only mode.
use std::collections::HashMap;

use eframe::egui::{
    self, Color32, CornerRadius, Event, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};
use serde_json::{Value, json};

use crate::app::{App, PanelTab};
use crate::terminal::{self, Color, Terminal};
use crate::theme::{self, Weight, font};
use crate::ui::widgets;

const PADDING: f32 = 4.0;
const CURSOR: Color32 = Color32::from_rgb(180, 203, 255);
/// `rgba(180, 203, 255, 0.25)`.
const SELECTION: Color32 = Color32::from_rgba_unmultiplied_const(180, 203, 255, 64);

/// Ghostty's default palette (Tomorrow Night) for the 16 ANSI colors.
const PALETTE: [Color32; 16] = [
    Color32::from_rgb(0x1d, 0x1f, 0x21),
    Color32::from_rgb(0xcc, 0x66, 0x66),
    Color32::from_rgb(0xb5, 0xbd, 0x68),
    Color32::from_rgb(0xf0, 0xc6, 0x74),
    Color32::from_rgb(0x81, 0xa2, 0xbe),
    Color32::from_rgb(0xb2, 0x94, 0xbb),
    Color32::from_rgb(0x8a, 0xbe, 0xb7),
    Color32::from_rgb(0xc5, 0xc8, 0xc6),
    Color32::from_rgb(0x66, 0x66, 0x66),
    Color32::from_rgb(0xd5, 0x4e, 0x53),
    Color32::from_rgb(0xb9, 0xca, 0x4a),
    Color32::from_rgb(0xe7, 0xc5, 0x47),
    Color32::from_rgb(0x7a, 0xa6, 0xda),
    Color32::from_rgb(0xc3, 0x97, 0xd8),
    Color32::from_rgb(0x70, 0xc0, 0xb1),
    Color32::from_rgb(0xea, 0xea, 0xea),
];

fn indexed(index: u8) -> Color32 {
    match index {
        0..=15 => PALETTE[index as usize],
        16..=231 => {
            let i = index - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
        }
        _ => {
            let gray = 8 + (index - 232) * 10;
            Color32::from_rgb(gray, gray, gray)
        }
    }
}

fn resolve(color: Color, default: Color32) -> Color32 {
    match color {
        Color::Default => default,
        Color::Indexed(i) => indexed(i),
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

/// One terminal session on this client.
pub struct TerminalView {
    terminal: Terminal,
    status: String,
    error: Option<String>,
    /// Lines scrolled back into the history.
    offset: usize,
    /// The selection's anchor and head as (buffer line, column), lines
    /// counted from the oldest scrollback line.
    selection: Option<((usize, usize), (usize, usize))>,
    /// Replies the emulator owes the program, written once attached.
    replies: String,
    /// The mouse button a reporting program saw pressed, and the cell it
    /// was last told about.
    mouse_button: Option<u8>,
    mouse_cell: Option<(usize, usize)>,
    /// When the blink cycle last restarted on its visible phase.
    blink_since: std::time::Instant,
}

/// The link under the pointer: its (line, start, end) row spans, whether
/// it is a URL, and its text.
type HoveredLink = (Vec<(usize, usize, usize)>, bool, String);

/// A link in a terminal line (`TerminalLinkMatch`): a URL or a path.
#[derive(Debug, PartialEq, Eq)]
struct LinkMatch {
    url: bool,
    text: String,
    start: usize,
    end: usize,
}

/// `trimClosingDelimiters`: trailing punctuation (a path's colon too) and
/// unbalanced closers.
fn trim_link(mut text: String, url: bool) -> String {
    let punctuation: &[char] = if url {
        &['.', ',', ';', '!', '?']
    } else {
        &['.', ',', ';', ':', '!', '?']
    };
    while text.ends_with(punctuation) {
        text.pop();
    }
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        while text.ends_with(close) && text.matches(open).count() < text.matches(close).count() {
            text.pop();
        }
    }
    text
}

fn link_char(c: char) -> bool {
    !c.is_whitespace() && !"\"'`<>".contains(c)
}

fn path_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || "._-".contains(c)
}

/// `FILE_PATH_PATTERN` at `at`: a rooted path (`~/`, `./`, `../`, `/`,
/// `C:\`, `\\`) to the end of the word, else `a/b[/c][:line[:col]]`.
fn path_match(line: &[char], at: usize) -> Option<usize> {
    let rest = &line[at..];
    let starts = |prefix: &str| {
        rest.len() >= prefix.chars().count()
            && rest.iter().zip(prefix.chars()).all(|(a, b)| *a == b)
    };
    let drive = rest.len() >= 3
        && rest[0].is_ascii_alphabetic()
        && rest[1] == ':'
        && matches!(rest[2], '\\' | '/');
    let prefix = if drive {
        Some(3)
    } else {
        ["~/", "./", "../", "/", "\\\\"]
            .iter()
            .find(|p| starts(p))
            .map(|p| p.chars().count())
    };
    if let Some(prefix) = prefix {
        // At least one path character after the prefix.
        let len = prefix + rest[prefix..].iter().take_while(|c| link_char(**c)).count();
        return (len > prefix).then_some(at + len);
    }
    let mut end = at;
    let word = |from: usize| line[from..].iter().take_while(|c| path_word(**c)).count();
    let first = word(end);
    if first == 0 {
        return None;
    }
    end += first;
    let mut segments = 0;
    while end < line.len() && line[end] == '/' {
        let next = word(end + 1);
        if next == 0 {
            break;
        }
        end += 1 + next;
        segments += 1;
    }
    if segments == 0 {
        return None;
    }
    for _ in 0..2 {
        if end < line.len() && line[end] == ':' {
            let digits = line[end + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if digits == 0 {
                break;
            }
            end += 1 + digits;
        }
    }
    Some(end)
}

/// `extractTerminalLinks`: URLs, then paths that overlap none of them.
fn extract_links(line: &[char]) -> Vec<LinkMatch> {
    let text: String = line.iter().collect();
    let lower: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let mut links = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let rest: String = lower[i..lower.len().min(i + 8)].iter().collect();
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = i + line[i..].iter().take_while(|c| link_char(**c)).count();
            let url = trim_link(line[i..end].iter().collect(), true);
            if !url.is_empty() {
                links.push(LinkMatch {
                    url: true,
                    start: i,
                    end: i + url.chars().count(),
                    text: url,
                });
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    let mut paths = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let Some(end) = path_match(line, i) else {
            i += 1;
            continue;
        };
        let path = trim_link(line[i..end].iter().collect(), false);
        let candidate_end = i + path.chars().count();
        let lowered = path.to_ascii_lowercase();
        let overlaps = links
            .iter()
            .any(|l: &LinkMatch| i < l.end && l.start < candidate_end);
        if !path.is_empty()
            && !lowered.starts_with("http://")
            && !lowered.starts_with("https://")
            && !overlaps
        {
            paths.push(LinkMatch {
                url: false,
                start: i,
                end: candidate_end,
                text: path,
            });
        }
        i = end.max(i + 1);
    }
    links.extend(paths);
    links.sort_by_key(|l| l.start);
    links
}

/// `resolvePathLinkTarget`: `~/` from the home in `cwd`, relative paths
/// under `cwd`; a `:line:col` suffix rides along.
fn resolve_path_link(path: &str, cwd: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = ["/Users/", "/home/"].iter().find_map(|root| {
            let user = cwd
                .strip_prefix(root)?
                .split('/')
                .next()
                .filter(|u| !u.is_empty())?;
            Some(format!("{root}{user}"))
        });
        return match home {
            Some(home) => format!("{home}/{rest}"),
            None => path.to_owned(),
        };
    }
    let absolute =
        path.starts_with('/') || path.starts_with("\\\\") || path.chars().nth(1) == Some(':');
    if absolute {
        path.to_owned()
    } else {
        format!(
            "{}/{}",
            cwd.trim_end_matches('/'),
            path.trim_start_matches("./")
        )
    }
}

impl TerminalView {
    /// The selected text: each line's cells trimmed at the end, joined by
    /// newlines (`getSelection`).
    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection?;
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        if start == end {
            return None;
        }
        let screen = &self.terminal.screen;
        let mut out = Vec::new();
        for line in start.0..=end.0.min(screen.len().saturating_sub(1)) {
            let cells = screen.line(line);
            let from = if line == start.0 { start.1 } else { 0 };
            let to = if line == end.0 { end.1 } else { cells.len() };
            let text: String = cells
                .iter()
                .skip(from)
                .take(to.saturating_sub(from))
                .filter(|c| c.flags & terminal::SPACER == 0)
                .map(terminal::Cell::text)
                .collect();
            out.push(text.trim_end().to_owned());
        }
        let text = out.join("\n");
        (!text.trim().is_empty()).then_some(text)
    }

    /// Whether the cell at (line, col) lies in the selection.
    fn selected(&self, line: usize, col: usize) -> bool {
        let Some((a, b)) = self.selection else {
            return false;
        };
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        (line, col) >= start && (line, col) < end
    }
}

/// Terminal sessions by stream key.
#[derive(Default)]
pub struct Terminals {
    views: HashMap<String, TerminalView>,
    /// The terminal that has keyboard focus.
    focused: Option<String>,
    /// The open selection or right-click menu: its terminal and state.
    menu: Option<(String, crate::ui::context_menu::State)>,
    /// Input typed once a terminal has attached (a project action's command).
    pending: HashMap<String, String>,
}

impl App {
    /// `terminalContextMenuItems` / `terminalSelectionMenuItems`: Copy
    /// (disabled without a selection) and Paste. "Add to chat" is not ported.
    fn terminal_menu(&mut self, ui: &mut Ui, key: &str, thread: &str, id: &str) {
        let Some((menu_key, state)) = self.terminals.menu.take() else {
            return;
        };
        if menu_key != key {
            self.terminals.menu = Some((menu_key, state));
            return;
        }
        let selection = self
            .terminals
            .views
            .get(key)
            .and_then(TerminalView::selected_text);
        // `terminalSelectionLineRange`: buffer lines, 1-based.
        let lines = self
            .terminals
            .views
            .get(key)
            .and_then(|view| view.selection)
            .map(|(a, b)| {
                let (start, end) = if a <= b { (a, b) } else { (b, a) };
                (
                    start.0 as u64 + 1,
                    (end.0 as u64 + 1).max(start.0 as u64 + 1),
                )
            });
        // A thread's composer takes the excerpt.
        let can_add = self.selected.as_deref() == Some(thread);
        let mut items = Vec::new();
        if can_add {
            items.push(crate::ui::context_menu::Item {
                disabled: selection.is_none(),
                ..crate::ui::context_menu::Item::new("add-to-chat", "Add to chat")
            });
        }
        items.extend([
            crate::ui::context_menu::Item {
                disabled: selection.is_none(),
                ..crate::ui::context_menu::Item::new("copy", "Copy")
            },
            crate::ui::context_menu::Item::new("paste", "Paste"),
            crate::ui::context_menu::Item::new("select-all", "Select all"),
            crate::ui::context_menu::Item::new("scroll-to-bottom", "Jump to latest"),
        ]);
        let mut open = Some(state);
        let clicked = crate::ui::context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.terminals.menu = Some((key.to_owned(), state));
        }
        match clicked.as_deref() {
            Some("add-to-chat") => {
                if let (Some(text), Some((start, end))) = (selection, lines)
                    && let Some(record) = crate::ui::composer_context::terminal_record(
                        id,
                        &terminal_label(id),
                        start,
                        end,
                        &text,
                    )
                {
                    self.add_composer_context(ui.ctx(), record);
                }
                if let Some(view) = self.terminals.views.get_mut(key) {
                    view.selection = None;
                }
            }
            Some("copy") => {
                if let Some(text) = selection {
                    ui.ctx().copy_text(text);
                }
                if let Some(view) = self.terminals.views.get_mut(key) {
                    view.selection = None;
                }
                self.terminals.focused = Some(key.to_owned());
            }
            Some("paste") => {
                self.terminals.focused = Some(key.to_owned());
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            Some("select-all") => {
                if let Some(view) = self.terminals.views.get_mut(key) {
                    let screen = &view.terminal.screen;
                    let last = screen.len().saturating_sub(1);
                    view.selection = Some(((0, 0), (last, screen.cols)));
                }
                self.terminals.focused = Some(key.to_owned());
            }
            Some("scroll-to-bottom") => {
                if let Some(view) = self.terminals.views.get_mut(key) {
                    view.offset = 0;
                }
                self.terminals.focused = Some(key.to_owned());
            }
            _ => {}
        }
    }

    /// Whether a terminal holds the keyboard: the focused one is the open
    /// panel's active surface.
    pub(crate) fn terminal_focused(&self) -> bool {
        let panel = self.panel();
        let in_drawer = self.selected.as_ref().is_some_and(|thread| {
            self.drawers.get(thread).is_some_and(|drawer| {
                drawer.open
                    && drawer.groups.iter().flat_map(|g| g.ids.iter()).any(|id| {
                        self.terminals.focused.as_deref() == Some(&format!("term:{thread}\0{id}"))
                    })
            })
        });
        in_drawer
            || self.terminals.focused.is_some()
                && panel.open
                && matches!(panel.active_tab(), Some(PanelTab::Terminal { .. }))
    }
}

impl Terminals {
    /// Applies a stream event (`TerminalAttachStreamEvent`); returns whether
    /// the key was a terminal stream.
    pub(crate) fn receive(&mut self, key: &str, result: Result<Value, String>) -> bool {
        let Some(view) = self.views.get_mut(key) else {
            return key.starts_with("term:");
        };
        match result {
            Err(error) => view.error = Some(error),
            Ok(event) => match event["type"].as_str() {
                Some("snapshot") | Some("restarted") => {
                    view.terminal.reset();
                    view.terminal
                        .feed(event["snapshot"]["history"].as_str().unwrap_or_default());
                    // Questions in the history were answered long ago.
                    view.terminal.take_replies();
                    view.status = event["snapshot"]["status"]
                        .as_str()
                        .unwrap_or("running")
                        .to_owned();
                }
                Some("output") => {
                    view.terminal
                        .feed(event["data"].as_str().unwrap_or_default());
                    view.offset = 0;
                    view.blink_since = std::time::Instant::now();
                    let replies = view.terminal.take_replies();
                    view.replies.push_str(&replies);
                }
                Some("cleared") => view.terminal.reset(),
                Some("exited") => view.status = "exited".into(),
                Some("closed") => view.status = "closed".into(),
                Some("error") => view.error = event["message"].as_str().map(str::to_owned),
                _ => {}
            },
        }
        true
    }
}

/// The screen cell (row, column) under `pos`.
fn grid_point(
    pos: Pos2,
    origin: Pos2,
    cell_width: f32,
    rows: usize,
    cols: usize,
) -> (usize, usize) {
    let row = ((pos.y - origin.y) / theme::terminal_cell_height())
        .floor()
        .max(0.0) as usize;
    let col = ((pos.x - origin.x) / cell_width).floor().max(0.0) as usize;
    (
        row.min(rows.saturating_sub(1)),
        col.min(cols.saturating_sub(1)),
    )
}

/// An xterm mouse report: SGR (1006) when asked, else the legacy bytes
/// (UTF-8 coordinates under 1005); `code` is the button with its motion
/// and wheel bits.
fn mouse_report(
    mode: terminal::MouseMode,
    code: u8,
    pressed: bool,
    row: usize,
    col: usize,
) -> String {
    let (x, y) = (col + 1, row + 1);
    if mode.sgr {
        return format!("\x1b[<{code};{x};{y}{}", if pressed { 'M' } else { 'm' });
    }
    let code = if pressed { code } else { 3 };
    let encode = |v: usize| -> Option<char> {
        let v = 32 + v;
        if mode.utf8 {
            char::from_u32(v as u32)
        } else {
            (v <= 255).then_some(v as u8 as char)
        }
    };
    match (encode(code as usize), encode(x), encode(y)) {
        (Some(b), Some(x), Some(y)) => format!("\x1b[M{b}{x}{y}"),
        _ => String::new(),
    }
}

/// The bytes a key press sends (xterm conventions).
fn key_bytes(key: Key, modifiers: Modifiers, app_cursor: bool) -> Option<String> {
    let arrow = |c: char| {
        if app_cursor {
            format!("\x1bO{c}")
        } else {
            format!("\x1b[{c}")
        }
    };
    if modifiers.ctrl && !modifiers.alt {
        let name = key.name();
        if name.len() == 1 {
            let c = name.chars().next()?.to_ascii_lowercase();
            if c.is_ascii_lowercase() {
                return Some(((c as u8 - b'a' + 1) as char).to_string());
            }
        }
    }
    Some(match key {
        Key::Enter => "\r".into(),
        Key::Backspace => "\x7f".into(),
        Key::Tab if modifiers.shift => "\x1b[Z".into(),
        Key::Tab => "\t".into(),
        Key::Escape => "\x1b".into(),
        Key::ArrowUp => arrow('A'),
        Key::ArrowDown => arrow('B'),
        Key::ArrowRight => arrow('C'),
        Key::ArrowLeft => arrow('D'),
        Key::Home => arrow('H'),
        Key::End => arrow('F'),
        Key::PageUp => "\x1b[5~".into(),
        Key::PageDown => "\x1b[6~".into(),
        Key::Delete => "\x1b[3~".into(),
        Key::Insert => "\x1b[2~".into(),
        _ => return None,
    })
}

/// `MAX_TERMINALS_PER_GROUP`.
const MAX_TERMINALS_PER_GROUP: usize = 4;

/// The next free session id (`nextTerminalId`): "default" while unused,
/// then the lowest unused "term-N".
pub(crate) fn next_terminal_id<'a>(used: impl IntoIterator<Item = &'a String>) -> String {
    let used: Vec<&String> = used.into_iter().collect();
    if !used.iter().any(|id| *id == "default") {
        return "default".to_owned();
    }
    (1..)
        .map(|n| format!("term-{n}"))
        .find(|id| !used.contains(&id))
        .unwrap_or_default()
}

/// `getTerminalLabel`: "term-3" reads "Terminal 3".
pub(crate) fn terminal_label(id: &str) -> String {
    let suffix = id
        .strip_prefix("term-")
        .or_else(|| id.strip_prefix("terminal-"))
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    match suffix {
        Some(n) => format!("Terminal {n}"),
        None => id.to_owned(),
    }
}

impl App {
    /// Every terminal session id the thread's panel tabs and drawer use.
    pub(crate) fn used_terminal_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .panel()
            .tabs
            .iter()
            .filter_map(|t| match t {
                PanelTab::Terminal { id, split, .. } => {
                    Some(std::iter::once(id.clone()).chain(split.iter().cloned()))
                }
                _ => None,
            })
            .flatten()
            .collect();
        if let Some(drawer) = self.selected.as_ref().and_then(|id| self.drawers.get(id)) {
            ids.extend(drawer.groups.iter().flat_map(|g| g.ids.iter().cloned()));
        }
        ids
    }
}

impl App {
    /// Opens a new terminal tab in the current panel.
    pub(crate) fn open_terminal_tab(&mut self) {
        let id = next_terminal_id(&self.used_terminal_ids());
        let Some(panel) = self.panel_mut() else {
            return;
        };
        let number = panel
            .tabs
            .iter()
            .filter_map(|t| match t {
                PanelTab::Terminal { number, .. } => Some(*number),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            + 1;
        panel.tabs.push(PanelTab::Terminal {
            id,
            number,
            split: Vec::new(),
            active: 0,
            vertical: false,
        });
        panel.active = panel.tabs.len() - 1;
    }

    /// `splitPanelTerminal`: a new session beside the group's (at most four),
    /// stacked when `vertical`; it becomes the active one.
    fn split_terminal(&mut self, vertical: bool) {
        let next = next_terminal_id(&self.used_terminal_ids());
        let Some(panel) = self.panel_mut() else {
            return;
        };
        let index = panel.active.min(panel.tabs.len().saturating_sub(1));
        if let Some(PanelTab::Terminal {
            split,
            active,
            vertical: stacked,
            ..
        }) = panel.tabs.get_mut(index)
            && split.len() + 1 < MAX_TERMINALS_PER_GROUP
        {
            split.push(next.clone());
            *active = split.len();
            *stacked = vertical;
            if let Some(thread) = self.selected.clone() {
                self.terminals.focused = Some(format!("term:{thread}\0{next}"));
            }
        }
    }

    /// A terminal group: its sessions side by side (or stacked), each with a
    /// border (`border-l` / `border-t`, full on the active one), and the
    /// floating toolbar acting on the active session.
    pub(crate) fn terminal_surface(&mut self, ui: &mut Ui, body: Rect, tab: &PanelTab) {
        let PanelTab::Terminal {
            id,
            split,
            active,
            vertical,
            ..
        } = tab
        else {
            return;
        };
        let ids: Vec<String> = std::iter::once(id.clone())
            .chain(split.iter().cloned())
            .collect();
        let active = (*active).min(ids.len() - 1);
        let count = ids.len() as f32;
        for (index, terminal) in ids.iter().enumerate() {
            let cell = if *vertical {
                let height = body.height() / count;
                Rect::from_min_size(
                    Pos2::new(body.left(), body.top() + index as f32 * height),
                    Vec2::new(body.width(), height),
                )
            } else {
                let width = body.width() / count;
                Rect::from_min_size(
                    Pos2::new(body.left() + index as f32 * width, body.top()),
                    Vec2::new(width, body.height()),
                )
            };
            self.terminal_view(ui, cell, terminal);
            if index > 0 {
                let border = if index == active {
                    theme::border()
                } else {
                    theme::alpha(theme::border(), 0.7)
                };
                let edge = if *vertical {
                    Rect::from_min_size(cell.min, Vec2::new(cell.width(), 1.0))
                } else {
                    Rect::from_min_size(cell.min, Vec2::new(1.0, cell.height()))
                };
                ui.painter().rect_filled(edge, 0.0, border);
            }
            // A press in a session makes it the group's active one.
            if ids.len() > 1
                && ui.input(|i| i.pointer.any_pressed())
                && ui
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|pos| cell.contains(pos))
                && index != active
                && let Some(panel) = self.panel_mut()
            {
                let tab_index = panel.active.min(panel.tabs.len().saturating_sub(1));
                if let Some(PanelTab::Terminal { active, .. }) = panel.tabs.get_mut(tab_index) {
                    *active = index;
                }
            }
        }
        let Some(thread) = self.selected.clone() else {
            return;
        };
        match terminal_toolbar(ui, body, ids.len()) {
            Some(0) => self.split_terminal(false),
            Some(1) => self.split_terminal(true),
            Some(2) => self.open_terminal_tab(),
            Some(3) => self.confirm_close_terminals(&thread, vec![ids[active].clone()], false),
            _ => {}
        }
    }

    /// Types `data` into a terminal now, or once it has attached.
    pub(crate) fn queue_terminal_input(&mut self, thread: &str, id: &str, data: String) {
        let key = format!("term:{thread}\0{id}");
        let ready = self
            .terminals
            .views
            .get(&key)
            .is_some_and(|view| view.status != "starting");
        if ready {
            self.terminal_write(thread, id, &data);
        } else {
            self.terminals.pending.insert(key, data);
        }
    }

    fn terminal_write(&mut self, thread: &str, id: &str, data: &str) {
        self.send_request(
            "term-write".into(),
            "terminal.write",
            json!({"threadId": thread, "terminalId": id, "data": data}),
        );
    }

    /// One session's grid in `body` for the terminal `id`.
    pub(crate) fn terminal_view(&mut self, ui: &mut Ui, body: Rect, id: &str) {
        ui.painter().rect_filled(body, 0.0, theme::background());
        let (Some(thread), Some(cwd)) = (self.selected.clone(), self.workspace_cwd()) else {
            widgets::text(
                ui,
                body.min + Vec2::splat(16.0),
                "Available when a project is open.",
                font(14.0, Weight::Regular),
                theme::muted_foreground(),
                20.0,
            );
            return;
        };
        let key = format!("term:{thread}\0{id}");
        let mono = theme::terminal_font(false);
        let cell_width = widgets::text_width(ui, "M", mono.clone()).max(1.0);
        let cols = (((body.width() - PADDING * 2.0) / cell_width).floor() as usize).max(1);
        let rows = (((body.height() - PADDING * 2.0) / theme::terminal_cell_height()).floor()
            as usize)
            .max(1);
        if !self.terminals.views.contains_key(&key) {
            self.terminals.views.insert(
                key.clone(),
                TerminalView {
                    terminal: Terminal::new(cols, rows),
                    status: "starting".into(),
                    error: None,
                    offset: 0,
                    selection: None,
                    replies: String::new(),
                    mouse_button: None,
                    mouse_cell: None,
                    blink_since: std::time::Instant::now(),
                },
            );
            self.terminals.focused = Some(key.clone());
            self.send(crate::protocol::Action::Stream {
                key: key.clone(),
                method: "terminal.attach",
                payload: json!({"threadId": thread, "terminalId": id, "cwd": cwd, "cols": cols, "rows": rows, "restartIfNotRunning": true}),
            });
        }
        // Queued input goes out once the session has attached.
        let attached = self
            .terminals
            .views
            .get(&key)
            .is_some_and(|view| view.status != "starting");
        if attached && let Some(data) = self.terminals.pending.remove(&key) {
            self.terminal_write(&thread, id, &data);
        }
        let replies = self
            .terminals
            .views
            .get_mut(&key)
            .filter(|view| view.status != "starting")
            .map(|view| std::mem::take(&mut view.replies))
            .unwrap_or_default();
        if !replies.is_empty() {
            self.terminal_write(&thread, id, &replies);
        }
        // Follow the panel's size.
        let resized = self
            .terminals
            .views
            .get(&key)
            .is_some_and(|v| v.terminal.screen.cols != cols || v.terminal.screen.rows != rows);
        if resized {
            if let Some(view) = self.terminals.views.get_mut(&key) {
                view.terminal.screen.resize(cols, rows);
            }
            self.send_request(
                "term-resize".into(),
                "terminal.resize",
                json!({"threadId": thread, "terminalId": id, "cols": cols, "rows": rows}),
            );
        }

        // Focus, keyboard and paste, wheel scrolling.
        let response = ui.interact(
            body,
            ui.id().with(("terminal", &key)),
            Sense::click_and_drag(),
        );
        if response.clicked() {
            self.terminals.focused = Some(key.clone());
        }
        // A program that asked for the mouse gets it; Shift keeps it for
        // selecting.
        let shift = ui.input(|i| i.modifiers.shift);
        let origin = body.min + Vec2::splat(PADDING);
        let mouse = self
            .terminals
            .views
            .get(&key)
            .and_then(|v| v.terminal.screen.mouse())
            .filter(|_| !shift);
        if let Some(mode) = mouse {
            let mut reports = String::new();
            let events = ui.input(|i| i.events.clone());
            if let Some(view) = self.terminals.views.get_mut(&key) {
                let screen = &view.terminal.screen;
                let (rows, cols) = (screen.rows, screen.cols);
                for event in events {
                    match event {
                        Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            ..
                        } if (pressed && body.contains(pos))
                            || (!pressed && view.mouse_button.is_some()) =>
                        {
                            let code = match button {
                                egui::PointerButton::Primary => 0,
                                egui::PointerButton::Middle => 1,
                                egui::PointerButton::Secondary => 2,
                                _ => continue,
                            };
                            let (row, col) = grid_point(pos, origin, cell_width, rows, cols);
                            reports.push_str(&mouse_report(mode, code, pressed, row, col));
                            view.mouse_button = pressed.then_some(code);
                            view.mouse_cell = Some((row, col));
                        }
                        Event::PointerMoved(pos)
                            if body.contains(pos)
                                && (mode.motion || mode.drag && view.mouse_button.is_some()) =>
                        {
                            let cell = grid_point(pos, origin, cell_width, rows, cols);
                            if view.mouse_cell != Some(cell) {
                                view.mouse_cell = Some(cell);
                                let code = view.mouse_button.unwrap_or(3) + 32;
                                reports.push_str(&mouse_report(mode, code, true, cell.0, cell.1));
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !reports.is_empty() {
                self.terminal_write(&thread, id, &reports);
            }
        }
        // Mouse selection on the cell grid; a release opens the selection
        // actions, a right click the terminal menu (Copy, Paste).
        let cell_at = |view: &TerminalView, pos: Pos2| -> (usize, usize) {
            let screen = &view.terminal.screen;
            let first = screen.history_len() - view.offset.min(screen.history_len());
            let row = (((pos.y - origin.y) / theme::terminal_cell_height())
                .floor()
                .max(0.0) as usize)
                .min(screen.rows.saturating_sub(1));
            let col =
                (((pos.x - origin.x) / cell_width).round().max(0.0) as usize).min(screen.cols);
            (first + row, col)
        };
        if let Some(view) = self.terminals.views.get_mut(&key)
            && let Some(pos) = response.interact_pointer_pos()
            && mouse.is_none()
        {
            if response.drag_started_by(egui::PointerButton::Primary) {
                let cell = cell_at(view, pos);
                view.selection = Some((cell, cell));
                self.terminals.focused = Some(key.clone());
            } else if response.dragged_by(egui::PointerButton::Primary)
                && let Some((anchor, _)) = view.selection
            {
                view.selection = Some((anchor, cell_at(view, pos)));
            }
        }
        if response.drag_stopped()
            && self
                .terminals
                .views
                .get(&key)
                .is_some_and(|v| v.selected_text().is_some())
            && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
        {
            self.terminals.menu = Some((key.clone(), crate::ui::context_menu::State::new(pos)));
        }
        // Links: an OSC 8 hyperlink, a URL or a path under the pointer
        // underlines (across wrapped lines) and opens on click: URLs in the
        // browser, paths in the preferred editor.
        let mut hovered_link: Option<HoveredLink> = None;
        if let Some(pos) = response.hover_pos()
            && let Some(view) = self.terminals.views.get(&key)
        {
            let screen = &view.terminal.screen;
            let (line, _) = cell_at(view, pos);
            let cells = screen.line(line);
            let col = (((pos.x - origin.x) / cell_width).floor().max(0.0)) as usize;
            if let Some(uri) = cells.get(col).and_then(|c| c.link.clone()) {
                let same = |c: &terminal::Cell| c.link.as_deref() == Some(&*uri);
                let start = (0..=col).rev().take_while(|&i| same(&cells[i])).last();
                let end = (col..cells.len()).take_while(|&i| same(&cells[i])).last();
                if let (Some(start), Some(end)) = (start, end) {
                    hovered_link = Some((vec![(line, start, end + 1)], true, uri.to_string()));
                }
            } else {
                // `collectWrappedTerminalLinkLine`: the rows one soft-wrapped line spans.
                let mut first = line;
                while first > 0 && screen.wrapped(first - 1) {
                    first -= 1;
                }
                let mut last = line;
                while screen.wrapped(last) && last + 1 < screen.len() {
                    last += 1;
                }
                let cols = screen.cols;
                let chars: Vec<char> = (first..=last)
                    .flat_map(|l| screen.line(l).into_iter().map(|c| c.c))
                    .collect();
                let at = (line - first) * cols + col;
                if let Some(link) = extract_links(&chars)
                    .into_iter()
                    .find(|l| at >= l.start && at < l.end)
                {
                    let segments = (first..=last)
                        .filter_map(|l| {
                            let row_start = (l - first) * cols;
                            let start = link.start.max(row_start);
                            let end = link.end.min(row_start + cols);
                            (start < end).then(|| (l, start - row_start, end - row_start))
                        })
                        .collect();
                    hovered_link = Some((segments, link.url, link.text));
                }
            }
        }
        if let Some((_, url, text)) = &hovered_link {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() {
                if *url {
                    crate::ui::widgets::open_external(text);
                } else {
                    let target = resolve_path_link(text, &cwd);
                    match self.preferred_editor() {
                        Some(editor) => self.open_in_editor(&target, editor),
                        None => self.notify(crate::ui::toasts::Kind::Error, "Unable to open path"),
                    }
                }
            }
        }
        if response.clicked()
            && let Some(view) = self.terminals.views.get_mut(&key)
        {
            view.selection = None;
        }
        if response.secondary_clicked()
            && mouse.is_none()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.terminals.menu = Some((key.clone(), crate::ui::context_menu::State::new(pos)));
        }
        self.terminal_menu(ui, &key, &thread, id);
        let focused =
            self.terminals.focused.as_deref() == Some(key.as_str()) && self.open_menu.is_none();
        if focused {
            let (app_cursor, bracketed) = self
                .terminals
                .views
                .get(&key)
                .map(|v| {
                    (
                        v.terminal.screen.app_cursor(),
                        v.terminal.screen.bracketed_paste(),
                    )
                })
                .unwrap_or((false, false));
            // Ctrl+Shift+C copies the selection (`hasSelection` / `getSelection`).
            if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::C))
                && let Some(text) = self
                    .terminals
                    .views
                    .get(&key)
                    .and_then(TerminalView::selected_text)
            {
                ui.ctx().copy_text(text);
            }
            let mut out = String::new();
            ui.input_mut(|input| {
                input.events.retain(|event| match event {
                    Event::Text(text) => {
                        out.push_str(text);
                        false
                    }
                    Event::Paste(text) => {
                        if bracketed {
                            out.push_str(&format!("\x1b[200~{text}\x1b[201~"));
                        } else {
                            out.push_str(text);
                        }
                        false
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => match key_bytes(*key, *modifiers, app_cursor) {
                        Some(bytes) => {
                            out.push_str(&bytes);
                            false
                        }
                        None => true,
                    },
                    _ => true,
                });
            });
            if !out.is_empty() {
                if let Some(view) = self.terminals.views.get_mut(&key) {
                    view.blink_since = std::time::Instant::now();
                }
                self.terminal_write(&thread, id, &out);
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            let lines = (scroll / theme::terminal_cell_height()).round() as i64;
            let mut wheel = String::new();
            if lines != 0
                && let Some(view) = self.terminals.views.get_mut(&key)
            {
                let screen = &view.terminal.screen;
                let pointer = ui.input(|i| i.pointer.latest_pos()).unwrap_or(origin);
                let (row, col) = grid_point(pointer, origin, cell_width, screen.rows, screen.cols);
                if let Some(mode) = screen.mouse().filter(|_| !shift) {
                    let button = if lines > 0 { 64 } else { 65 };
                    for _ in 0..lines.unsigned_abs().min(5) {
                        wheel.push_str(&mouse_report(mode, button, true, row, col));
                    }
                } else if screen.alternate_scroll() {
                    let arrow = match (lines > 0, screen.app_cursor()) {
                        (true, true) => "\x1bOA",
                        (true, false) => "\x1b[A",
                        (false, true) => "\x1bOB",
                        (false, false) => "\x1b[B",
                    };
                    wheel = arrow.repeat(lines.unsigned_abs().min(5) as usize);
                } else if !screen.alternate() {
                    let max = screen.history_len() as i64;
                    view.offset = (view.offset as i64 + lines).clamp(0, max) as usize;
                }
            }
            if !wheel.is_empty() {
                self.terminal_write(&thread, id, &wheel);
            }
        }

        // The grid.
        let Some(view) = self.terminals.views.get(&key) else {
            return;
        };
        let screen = &view.terminal.screen;
        let foreground = theme::foreground();
        let background = theme::background();
        let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
        let lines = screen.view(view.offset);
        let first_line = screen.history_len() - view.offset.min(screen.history_len());
        paint_terminal_lines(ui, &painter, origin, &lines, cell_width, |row, col| {
            view.selected(first_line + row, col)
        });
        if let Some((segments, ..)) = &hovered_link {
            for (line, start, end) in segments {
                if *line < first_line || *line >= first_line + screen.rows {
                    continue;
                }
                let y = origin.y
                    + (line - first_line) as f32 * theme::terminal_cell_height()
                    + theme::terminal_cell_height()
                    - 3.0;
                painter.hline(
                    origin.x + *start as f32 * cell_width..=origin.x + *end as f32 * cell_width,
                    y,
                    Stroke::new(1.0, foreground),
                );
            }
        }
        // The cursor: a block while focused, an outline otherwise.
        // `shouldBlinkTerminalCursor`: 500ms phases while focused.
        let blink_off = focused
            && screen.cursor_blinking()
            && (view.blink_since.elapsed().as_millis() / 500) % 2 == 1;
        if screen.cursor_visible() && view.offset == 0 && !blink_off {
            let (row, col) = screen.cursor();
            let under = lines.get(row).and_then(|l| l.get(col));
            let wide = under.is_some_and(|c| c.flags & terminal::WIDE != 0);
            let cursor = Rect::from_min_size(
                Pos2::new(
                    origin.x + col as f32 * cell_width,
                    origin.y + row as f32 * theme::terminal_cell_height(),
                ),
                Vec2::new(
                    cell_width * if wide { 2.0 } else { 1.0 },
                    theme::terminal_cell_height(),
                ),
            );
            if focused {
                painter.rect_filled(cursor, 0.0, CURSOR);
                let c = under.map(terminal::Cell::text).unwrap_or_default();
                if !c.trim().is_empty() {
                    let galley = widgets::galley(
                        ui,
                        &c,
                        mono.clone(),
                        background,
                        theme::terminal_cell_height(),
                        f32::INFINITY,
                    );
                    painter.galley(cursor.min, galley, background);
                }
            } else {
                painter.rect_stroke(
                    cursor.shrink(0.5),
                    0.0,
                    Stroke::new(1.0, foreground),
                    StrokeKind::Inside,
                );
            }
        }
        if let Some(error) = &view.error {
            widgets::text(
                ui,
                Pos2::new(origin.x, body.bottom() - 28.0),
                error,
                font(12.0, Weight::Regular),
                theme::error_foreground(),
                16.0,
            );
        } else if view.status == "exited" {
            widgets::text(
                ui,
                Pos2::new(origin.x, body.bottom() - 28.0),
                "Process exited.",
                font(12.0, Weight::Regular),
                theme::muted_foreground(),
                16.0,
            );
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// The floating toolbar (split right, split down, new terminal, close);
/// returns the clicked action's index. Splits stop at four sessions.
/// Paints terminal lines on the cell grid from `origin`: the selection band,
/// cell backgrounds, then each glyph with its colors and attributes.
pub(crate) fn paint_terminal_lines(
    ui: &Ui,
    painter: &egui::Painter,
    origin: Pos2,
    lines: &[Vec<terminal::Cell>],
    cell_width: f32,
    selected: impl Fn(usize, usize) -> bool,
) {
    let foreground = theme::foreground();
    let background = theme::background();
    let mono = theme::terminal_font(false);
    for (row, line) in lines.iter().enumerate() {
        let y = origin.y + row as f32 * theme::terminal_cell_height();
        // The selection band under the glyphs (`--terminal-selection-background`).
        let mut col = 0;
        while col < line.len() {
            if !selected(row, col) {
                col += 1;
                continue;
            }
            let start = col;
            while col < line.len() && selected(row, col) {
                col += 1;
            }
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(origin.x + start as f32 * cell_width, y),
                    Vec2::new(
                        (col - start) as f32 * cell_width,
                        theme::terminal_cell_height(),
                    ),
                ),
                0.0,
                SELECTION,
            );
        }
        let mut col = 0;
        while col < line.len() {
            let cell = &line[col];
            let mut end = col + 1;
            while end < line.len()
                && line[end].fg == cell.fg
                && line[end].bg == cell.bg
                && line[end].flags == cell.flags
            {
                end += 1;
            }
            let (mut fg, mut bg) = (resolve(cell.fg, foreground), resolve(cell.bg, background));
            if cell.flags & terminal::INVERSE != 0 {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags & terminal::DIM != 0 {
                fg = theme::alpha(fg, 0.6);
            }
            let x = origin.x + col as f32 * cell_width;
            if bg != background {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, y),
                        Vec2::new(
                            (end - col) as f32 * cell_width,
                            theme::terminal_cell_height(),
                        ),
                    ),
                    0.0,
                    bg,
                );
            }
            let run = &line[col..end];
            if run.iter().any(|c| c.c != ' ') {
                let run_font = if cell.flags & terminal::BOLD != 0 {
                    theme::terminal_font(true)
                } else {
                    mono.clone()
                };
                // Glyphs are placed on the cell grid one by one, so a
                // fallback glyph cannot push the rest of the row.
                for (i, c) in run.iter().enumerate() {
                    if c.c != ' ' && c.flags & terminal::SPACER == 0 {
                        let galley = widgets::galley(
                            ui,
                            &c.text(),
                            run_font.clone(),
                            fg,
                            theme::terminal_cell_height(),
                            f32::INFINITY,
                        );
                        painter.galley(Pos2::new(x + i as f32 * cell_width, y), galley, fg);
                    }
                }
                if cell.flags & terminal::UNDERLINE != 0 {
                    painter.hline(
                        x..=x + (end - col) as f32 * cell_width,
                        y + theme::terminal_cell_height() - 3.0,
                        Stroke::new(1.0, fg),
                    );
                }
            }
            col = end;
        }
    }
}

pub(crate) fn terminal_toolbar(ui: &mut Ui, body: Rect, sessions: usize) -> Option<usize> {
    {
        let background = theme::background();
        let foreground = theme::foreground();
        let bar = Rect::from_min_size(
            Pos2::new(body.right() - 97.0, body.top() + 8.0),
            Vec2::new(89.0, 23.0),
        );
        ui.painter().rect(
            bar,
            CornerRadius::same(8),
            background,
            Stroke::new(1.0, theme::overlay(0.048)),
            StrokeKind::Inside,
        );
        let mut action = None;
        for (index, (icon, tip)) in [
            ("square-split-horizontal", "Split right"),
            ("square-split-vertical", "Split down"),
            ("plus", "New terminal"),
            ("trash", "Close terminal"),
        ]
        .into_iter()
        .enumerate()
        {
            let button = Rect::from_min_size(
                Pos2::new(bar.left() + 1.0 + index as f32 * 22.0, bar.top() + 1.0),
                Vec2::new(21.0, 21.0),
            );
            if index > 0 {
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(button.left() - 1.0, bar.top() + 4.0),
                        Vec2::new(1.0, 16.0),
                    ),
                    0.0,
                    theme::overlay(0.048),
                );
            }
            let enabled = index > 1 || sessions < MAX_TERMINALS_PER_GROUP;
            let response = ui
                .interact(
                    button,
                    ui.id().with(("terminal-bar", index)),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                )
                .on_hover_text(tip);
            if response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(6), theme::accent());
            }
            theme::paint_icon(
                ui,
                icon,
                button,
                13.0,
                theme::alpha(foreground, if enabled { 0.9 } else { 0.4 }),
            );
            if response.clicked() {
                action = Some(index);
            }
        }
        action
    }
}

/// The confirm dialog's local command for closing terminals.
pub(crate) const CLOSE_TERMINALS_COMMAND: &str = "pluto.terminals.close";

impl App {
    /// `confirmTerminalClose`: asks before stopping the sessions `ids`
    /// (in the bottom drawer when `drawer`).
    pub(crate) fn confirm_close_terminals(&mut self, thread: &str, ids: Vec<String>, drawer: bool) {
        let labels: Vec<String> = ids.iter().map(|id| terminal_label(id)).collect();
        let (title, message) = match labels.as_slice() {
            [one] => (
                format!("Close terminal \"{one}\"?"),
                "This stops the running process and clears its history.".to_owned(),
            ),
            _ => (
                format!("Close {} terminals?", labels.len()),
                format!(
                    "This stops their running processes and clears their histories: {}.",
                    labels
                        .iter()
                        .map(|l| format!("\"{l}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
        };
        self.confirm = Some(crate::ui::thread_menu::Confirm {
            title,
            message,
            confirm_label: "Close",
            command: json!({"type": CLOSE_TERMINALS_COMMAND, "threadId": thread, "terminalIds": ids, "drawer": drawer}),
            more: Vec::new(),
        });
    }

    /// The confirmed close.
    pub(crate) fn close_confirmed_terminals(&mut self, command: &Value) {
        let thread = command["threadId"].as_str().unwrap_or_default().to_owned();
        let ids: Vec<String> = command["terminalIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .collect();
        for id in ids {
            if command["drawer"] == true {
                self.drawer_action(
                    &thread,
                    crate::ui::terminal_drawer::DrawerAction::Closed(id),
                );
            } else {
                self.close_terminal(&thread, &id);
            }
        }
    }

    /// Closes the terminal session and its tab.
    pub(crate) fn close_terminal(&mut self, thread: &str, id: &str) {
        let key = format!("term:{thread}\0{id}");
        self.send(crate::protocol::Action::StopStream(key.clone()));
        self.terminals.views.remove(&key);
        self.send_request(
            "term-close".into(),
            "terminal.close",
            json!({"threadId": thread, "terminalId": id}),
        );
        // `closeTerminal`: the session leaves its group; an empty group's tab closes.
        let Some(panel) = self.panel_mut() else {
            return;
        };
        let Some(index) = panel.tabs.iter().position(|t| {
            matches!(t, PanelTab::Terminal { id: tid, split, .. } if tid == id || split.iter().any(|s| s == id))
        }) else {
            return;
        };
        let mut empty = false;
        if let Some(PanelTab::Terminal {
            id: first,
            split,
            active,
            ..
        }) = panel.tabs.get_mut(index)
        {
            if first == id {
                if split.is_empty() {
                    empty = true;
                } else {
                    *first = split.remove(0);
                }
            } else {
                split.retain(|s| s != id);
            }
            *active = (*active).min(split.len());
        }
        if empty {
            panel.close(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_take_the_lowest_free_slot() {
        assert_eq!(next_terminal_id(&[]), "default");
        let used = [
            "default".to_owned(),
            "term-1".to_owned(),
            "term-3".to_owned(),
        ];
        assert_eq!(next_terminal_id(&used), "term-2");
        assert_eq!(terminal_label("term-2"), "Terminal 2");
        assert_eq!(terminal_label("default"), "default");
    }

    #[test]
    fn urls_trim_like_terminal_links() {
        let line: Vec<char> = "see (https://example.com/a_(b)). next src/main.rs:12:3: x ~/a.txt"
            .chars()
            .collect();
        let links = extract_links(&line);
        assert_eq!(links[0].text, "https://example.com/a_(b)");
        assert_eq!((links[0].start, links[0].end), (5, 30));
        assert_eq!(links[1].text, "src/main.rs:12:3");
        assert!(!links[1].url);
        assert_eq!(links[2].text, "~/a.txt");
        assert_eq!(links.len(), 3);
        assert_eq!(
            resolve_path_link("src/a.rs:3", "/home/me/repo"),
            "/home/me/repo/src/a.rs:3"
        );
        assert_eq!(resolve_path_link("~/x", "/home/me/repo"), "/home/me/x");
    }

    #[test]
    fn selections_read_trimmed_lines() {
        let mut terminal = Terminal::new(10, 3);
        terminal.feed("hello\r\nworld");
        let mut view = TerminalView {
            terminal,
            status: "running".into(),
            error: None,
            offset: 0,
            selection: Some(((1, 3), (0, 0))),
            replies: String::new(),
            mouse_button: None,
            mouse_cell: None,
            blink_since: std::time::Instant::now(),
        };
        assert_eq!(view.selected_text().as_deref(), Some("hello\nwor"));
        assert!(view.selected(0, 9) && !view.selected(1, 3));
        view.selection = Some(((0, 2), (0, 2)));
        assert_eq!(view.selected_text(), None);
    }

    #[test]
    fn keys_follow_xterm() {
        assert_eq!(
            key_bytes(Key::Enter, Modifiers::NONE, false).as_deref(),
            Some("\r")
        );
        assert_eq!(
            key_bytes(Key::ArrowUp, Modifiers::NONE, false).as_deref(),
            Some("\x1b[A")
        );
        assert_eq!(
            key_bytes(Key::ArrowUp, Modifiers::NONE, true).as_deref(),
            Some("\x1bOA")
        );
        assert_eq!(
            key_bytes(Key::C, Modifiers::CTRL, false).as_deref(),
            Some("\x03")
        );
        assert_eq!(key_bytes(Key::A, Modifiers::NONE, false), None);
    }

    #[test]
    fn the_color_cube_and_grays() {
        assert_eq!(indexed(1), PALETTE[1]);
        assert_eq!(indexed(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(indexed(232), Color32::from_rgb(8, 8, 8));
    }
}
