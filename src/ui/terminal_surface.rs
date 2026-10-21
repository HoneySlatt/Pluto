//! The Terminal surface (`ThreadTerminalDrawer.tsx`, `terminal/ghostty/surface.ts`,
//! `terminal/ghostty/renderer.ts`): a terminal session of the thread's
//! workspace, attached with `terminal.attach` (a stream of the snapshot and
//! its output), drawn from `crate::terminal`'s cell grid at upstream's
//! defaults (12px monospace, 4px padding, Ghostty's Tomorrow Night palette
//! over the app's foreground and background), typed into with
//! `terminal.write` and resized with `terminal.resize`.
//!
//! Not ported: split panes, the bottom drawer, selection and copy, links,
//! "Add to chat", font settings and cursor blinking.
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

const FONT_SIZE: f32 = 12.0;
const CELL_HEIGHT: f32 = 18.0;
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
        let lines: Vec<&Vec<terminal::Cell>> =
            screen.scrollback.iter().chain(screen.grid.iter()).collect();
        let mut out = Vec::new();
        for (line, cells) in lines
            .iter()
            .enumerate()
            .skip(start.0)
            .take(end.0 + 1 - start.0)
        {
            let from = if line == start.0 { start.1 } else { 0 };
            let to = if line == end.0 { end.1 } else { cells.len() };
            let text: String = cells
                .iter()
                .skip(from)
                .take(to.saturating_sub(from))
                .map(|c| c.c)
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
}

impl App {
    /// `terminalContextMenuItems` / `terminalSelectionMenuItems`: Copy
    /// (disabled without a selection) and Paste. "Add to chat" is not ported.
    fn terminal_menu(&mut self, ui: &mut Ui, key: &str, _thread: &str, _id: &str) {
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
        let items = [
            crate::ui::context_menu::Item {
                disabled: selection.is_none(),
                ..crate::ui::context_menu::Item::new("copy", "Copy")
            },
            crate::ui::context_menu::Item::new("paste", "Paste"),
        ];
        let mut open = Some(state);
        let clicked = crate::ui::context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.terminals.menu = Some((key.to_owned(), state));
        }
        match clicked.as_deref() {
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
            _ => {}
        }
    }

    /// Whether a terminal holds the keyboard: the focused one is the open
    /// panel's active surface.
    pub(crate) fn terminal_focused(&self) -> bool {
        let panel = self.panel();
        self.terminals.focused.is_some()
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
                    view.status = event["snapshot"]["status"]
                        .as_str()
                        .unwrap_or("running")
                        .to_owned();
                }
                Some("output") => {
                    view.terminal
                        .feed(event["data"].as_str().unwrap_or_default());
                    view.offset = 0;
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

impl App {
    /// Opens a new terminal tab in the current panel.
    pub(crate) fn open_terminal_tab(&mut self) {
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
        let id = if number == 1 {
            "default".to_owned()
        } else {
            format!("terminal-{number}")
        };
        panel.tabs.push(PanelTab::Terminal { id, number });
        panel.active = panel.tabs.len() - 1;
    }

    fn terminal_write(&mut self, thread: &str, id: &str, data: &str) {
        self.send_request(
            "term-write".into(),
            "terminal.write",
            json!({"threadId": thread, "terminalId": id, "data": data}),
        );
    }

    /// The surface in `body` for the terminal `id`.
    pub(crate) fn terminal_surface(&mut self, ui: &mut Ui, body: Rect, id: &str) {
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
        let mono = theme::mono(FONT_SIZE);
        let cell_width = widgets::text_width(ui, "M", mono.clone()).max(1.0);
        let cols = (((body.width() - PADDING * 2.0) / cell_width).floor() as usize).max(1);
        let rows = (((body.height() - PADDING * 2.0) / CELL_HEIGHT).floor() as usize).max(1);
        if !self.terminals.views.contains_key(&key) {
            self.terminals.views.insert(
                key.clone(),
                TerminalView {
                    terminal: Terminal::new(cols, rows),
                    status: "starting".into(),
                    error: None,
                    offset: 0,
                    selection: None,
                },
            );
            self.terminals.focused = Some(key.clone());
            self.send(crate::protocol::Action::Stream {
                key: key.clone(),
                method: "terminal.attach",
                payload: json!({"threadId": thread, "terminalId": id, "cwd": cwd, "cols": cols, "rows": rows, "restartIfNotRunning": true}),
            });
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
        // Mouse selection on the cell grid; a release opens the selection
        // actions, a right click the terminal menu (Copy, Paste).
        let origin = body.min + Vec2::splat(PADDING);
        let cell_at = |view: &TerminalView, pos: Pos2| -> (usize, usize) {
            let screen = &view.terminal.screen;
            let first = screen.scrollback.len() - view.offset.min(screen.scrollback.len());
            let row = (((pos.y - origin.y) / CELL_HEIGHT).floor().max(0.0) as usize)
                .min(screen.rows.saturating_sub(1));
            let col =
                (((pos.x - origin.x) / cell_width).round().max(0.0) as usize).min(screen.cols);
            (first + row, col)
        };
        if let Some(view) = self.terminals.views.get_mut(&key)
            && let Some(pos) = response.interact_pointer_pos()
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
        if response.clicked()
            && let Some(view) = self.terminals.views.get_mut(&key)
        {
            view.selection = None;
        }
        if response.secondary_clicked()
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
                        v.terminal.screen.app_cursor,
                        v.terminal.screen.bracketed_paste,
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
                self.terminal_write(&thread, id, &out);
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(view) = self.terminals.views.get_mut(&key)
                && !view.terminal.screen.alternate()
            {
                let lines = (scroll / CELL_HEIGHT).round() as i64;
                let max = view.terminal.screen.scrollback.len() as i64;
                view.offset = (view.offset as i64 + lines).clamp(0, max) as usize;
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
        let first_line = screen.scrollback.len() - view.offset.min(screen.scrollback.len());
        for (row, line) in lines.iter().enumerate() {
            let y = origin.y + row as f32 * CELL_HEIGHT;
            // The selection band under the glyphs (`--terminal-selection-background`).
            let mut col = 0;
            while col < line.len() {
                if !view.selected(first_line + row, col) {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < line.len() && view.selected(first_line + row, col) {
                    col += 1;
                }
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(origin.x + start as f32 * cell_width, y),
                        Vec2::new((col - start) as f32 * cell_width, CELL_HEIGHT),
                    ),
                    0.0,
                    SELECTION,
                );
            }
            let mut col = 0;
            while col < line.len() {
                let cell = line[col];
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
                            Vec2::new((end - col) as f32 * cell_width, CELL_HEIGHT),
                        ),
                        0.0,
                        bg,
                    );
                }
                let text: String = line[col..end].iter().map(|c| c.c).collect();
                if !text.trim().is_empty() {
                    let run_font = if cell.flags & terminal::BOLD != 0 {
                        theme::mono_bold(FONT_SIZE)
                    } else {
                        mono.clone()
                    };
                    // Glyphs are placed on the cell grid one by one, so a
                    // fallback glyph cannot push the rest of the row.
                    for (i, c) in text.chars().enumerate() {
                        if c != ' ' {
                            let galley = widgets::galley(
                                ui,
                                &c.to_string(),
                                run_font.clone(),
                                fg,
                                CELL_HEIGHT,
                                f32::INFINITY,
                            );
                            painter.galley(Pos2::new(x + i as f32 * cell_width, y), galley, fg);
                        }
                    }
                    if cell.flags & terminal::UNDERLINE != 0 {
                        painter.hline(
                            x..=x + (end - col) as f32 * cell_width,
                            y + CELL_HEIGHT - 3.0,
                            Stroke::new(1.0, fg),
                        );
                    }
                }
                col = end;
            }
        }
        // The cursor: a block while focused, an outline otherwise.
        if screen.cursor_visible && view.offset == 0 {
            let cursor = Rect::from_min_size(
                Pos2::new(
                    origin.x + screen.col as f32 * cell_width,
                    origin.y + screen.row as f32 * CELL_HEIGHT,
                ),
                Vec2::new(cell_width, CELL_HEIGHT),
            );
            if focused {
                painter.rect_filled(cursor, 0.0, CURSOR);
                let c = screen.grid[screen.row][screen.col].c;
                if c != ' ' {
                    let galley = widgets::galley(
                        ui,
                        &c.to_string(),
                        mono.clone(),
                        background,
                        CELL_HEIGHT,
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

        // The floating toolbar: split right, split down, new terminal, close.
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
            let response = ui
                .interact(
                    button,
                    ui.id().with(("terminal-bar", index)),
                    Sense::click(),
                )
                .on_hover_text(tip);
            if response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(6), theme::accent());
            }
            theme::paint_icon(ui, icon, button, 13.0, theme::alpha(foreground, 0.9));
            if response.clicked() {
                action = Some(index);
            }
        }
        match action {
            Some(0) | Some(1) => {
                self.status = Some("Split terminals are not implemented yet".into())
            }
            Some(2) => self.open_terminal_tab(),
            Some(3) => self.close_terminal(&thread, id),
            _ => {}
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
        if let Some(panel) = self.panel_mut()
            && let Some(index) = panel
                .tabs
                .iter()
                .position(|t| matches!(t, PanelTab::Terminal { id: tid, .. } if tid == id))
        {
            panel.close(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
