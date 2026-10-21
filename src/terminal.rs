//! A terminal screen for the Terminal surface: the cell grid that upstream's
//! ghostty-web keeps, fed through `vte`'s parser. Covers what shells and
//! common full-screen programs use: printing with autowrap, cursor motion,
//! erase and insert/delete, scroll regions, SGR (16, 256 and truecolor), the
//! alternate screen, cursor visibility, application cursor keys, bracketed
//! paste and the window title. Wide (CJK) characters take one cell.
use std::collections::VecDeque;

use vte::{Params, Parser, Perform};

/// A cell color: the default, a palette index or RGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const UNDERLINE: u8 = 4;
pub const INVERSE: u8 = 8;
pub const DIM: u8 = 16;
pub const STRIKE: u8 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Default,
            bg: Color::Default,
            flags: 0,
        }
    }
}

const SCROLLBACK: usize = 5000;

#[derive(Clone)]
struct Saved {
    row: usize,
    col: usize,
    pen: Cell,
}

/// The screen state the parser drives.
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub grid: Vec<Vec<Cell>>,
    pub scrollback: VecDeque<Vec<Cell>>,
    pub row: usize,
    pub col: usize,
    /// The next print wraps first (the cursor sits past the last column).
    wrap_pending: bool,
    pen: Cell,
    top: usize,
    bottom: usize,
    saved: Option<Saved>,
    /// The primary screen while the alternate one is shown.
    primary: Option<(Vec<Vec<Cell>>, usize, usize)>,
    pub cursor_visible: bool,
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub title: String,
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        Self {
            cols,
            rows,
            grid: vec![vec![Cell::default(); cols]; rows],
            scrollback: VecDeque::new(),
            row: 0,
            col: 0,
            wrap_pending: false,
            pen: Cell::default(),
            top: 0,
            bottom: rows - 1,
            saved: None,
            primary: None,
            cursor_visible: true,
            app_cursor: false,
            bracketed_paste: false,
            title: String::new(),
        }
    }

    pub fn alternate(&self) -> bool {
        self.primary.is_some()
    }

    fn blank(&self) -> Cell {
        Cell {
            bg: self.pen.bg,
            ..Cell::default()
        }
    }

    /// Resizes the grid; rows that no longer fit move into the scrollback.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        for line in &mut self.grid {
            line.resize(cols, Cell::default());
        }
        while self.grid.len() > rows {
            if self.row > 0 {
                let line = self.grid.remove(0);
                self.push_scrollback(line);
                self.row -= 1;
            } else {
                self.grid.pop();
            }
        }
        while self.grid.len() < rows {
            self.grid.push(vec![Cell::default(); cols]);
        }
        self.cols = cols;
        self.rows = rows;
        self.top = 0;
        self.bottom = rows - 1;
        self.row = self.row.min(rows - 1);
        self.col = self.col.min(cols - 1);
        self.wrap_pending = false;
    }

    fn push_scrollback(&mut self, line: Vec<Cell>) {
        if self.primary.is_some() {
            return;
        }
        self.scrollback.push_back(line);
        if self.scrollback.len() > SCROLLBACK {
            self.scrollback.pop_front();
        }
    }

    /// Scrolls the region up by `count` lines.
    fn scroll_up(&mut self, count: usize) {
        for _ in 0..count {
            let line = self.grid.remove(self.top);
            if self.top == 0 {
                self.push_scrollback(line);
            }
            let blank = vec![self.blank(); self.cols];
            self.grid.insert(self.bottom, blank);
        }
    }

    fn scroll_down(&mut self, count: usize) {
        for _ in 0..count {
            self.grid.remove(self.bottom);
            let blank = vec![self.blank(); self.cols];
            self.grid.insert(self.top, blank);
        }
    }

    fn line_feed(&mut self) {
        if self.row == self.bottom {
            self.scroll_up(1);
        } else if self.row + 1 < self.rows {
            self.row += 1;
        }
    }

    fn erase(&mut self, row: usize, from: usize, to: usize) {
        let blank = self.blank();
        if let Some(line) = self.grid.get_mut(row) {
            for cell in line.iter_mut().take(to.min(self.cols)).skip(from) {
                *cell = blank;
            }
        }
    }

    fn set_alternate(&mut self, on: bool) {
        if on && self.primary.is_none() {
            let grid = std::mem::replace(
                &mut self.grid,
                vec![vec![Cell::default(); self.cols]; self.rows],
            );
            self.primary = Some((grid, self.row, self.col));
        } else if !on && let Some((grid, row, col)) = self.primary.take() {
            self.grid = grid;
            self.grid
                .resize(self.rows, vec![Cell::default(); self.cols]);
            for line in &mut self.grid {
                line.resize(self.cols, Cell::default());
            }
            self.row = row.min(self.rows - 1);
            self.col = col.min(self.cols - 1);
        }
        self.wrap_pending = false;
    }

    fn sgr(&mut self, params: &Params) {
        let values: Vec<Vec<u16>> = params.iter().map(|p| p.to_vec()).collect();
        if values.is_empty() {
            self.pen = Cell {
                c: ' ',
                ..Cell::default()
            };
            return;
        }
        let mut index = 0;
        while index < values.len() {
            let param = &values[index];
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => {
                    self.pen.fg = Color::Default;
                    self.pen.bg = Color::Default;
                    self.pen.flags = 0;
                }
                1 => self.pen.flags |= BOLD,
                2 => self.pen.flags |= DIM,
                3 => self.pen.flags |= ITALIC,
                4 => self.pen.flags |= UNDERLINE,
                7 => self.pen.flags |= INVERSE,
                9 => self.pen.flags |= STRIKE,
                21 | 22 => self.pen.flags &= !(BOLD | DIM),
                23 => self.pen.flags &= !ITALIC,
                24 => self.pen.flags &= !UNDERLINE,
                27 => self.pen.flags &= !INVERSE,
                29 => self.pen.flags &= !STRIKE,
                30..=37 => self.pen.fg = Color::Indexed((code - 30) as u8),
                39 => self.pen.fg = Color::Default,
                40..=47 => self.pen.bg = Color::Indexed((code - 40) as u8),
                49 => self.pen.bg = Color::Default,
                90..=97 => self.pen.fg = Color::Indexed((code - 90 + 8) as u8),
                100..=107 => self.pen.bg = Color::Indexed((code - 100 + 8) as u8),
                38 | 48 => {
                    // Either colon subparameters (38:5:n, 38:2::r:g:b) or the
                    // following semicolon parameters (38;5;n, 38;2;r;g;b).
                    let (color, used) = if param.len() > 1 {
                        (extended_color(&param[1..]), 0)
                    } else {
                        let rest: Vec<u16> = values[index + 1..]
                            .iter()
                            .map(|p| p.first().copied().unwrap_or(0))
                            .collect();
                        let color = extended_color(&rest);
                        let used = match rest.first() {
                            Some(5) => 2,
                            Some(2) => 4,
                            _ => 0,
                        };
                        (color, used)
                    };
                    if let Some(color) = color {
                        if code == 38 {
                            self.pen.fg = color;
                        } else {
                            self.pen.bg = color;
                        }
                    }
                    index += used;
                }
                _ => {}
            }
            index += 1;
        }
    }

    /// The visible lines `offset` lines back into the scrollback.
    pub fn view(&self, offset: usize) -> Vec<&Vec<Cell>> {
        let offset = offset.min(self.scrollback.len());
        let start = self.scrollback.len() - offset;
        self.scrollback
            .iter()
            .skip(start)
            .chain(self.grid.iter())
            .take(self.rows)
            .collect()
    }
}

/// `5;n` or `2;r;g;b` (an optional color-space id before r is skipped).
fn extended_color(values: &[u16]) -> Option<Color> {
    match values {
        [5, n, ..] => Some(Color::Indexed(*n as u8)),
        [2, _, r, g, b, ..] if values.len() >= 5 => Some(Color::Rgb(*r as u8, *g as u8, *b as u8)),
        [2, r, g, b, ..] => Some(Color::Rgb(*r as u8, *g as u8, *b as u8)),
        _ => None,
    }
}

fn param(params: &Params, index: usize, default: usize) -> usize {
    params
        .iter()
        .nth(index)
        .and_then(|p| p.first().copied())
        .filter(|&v| v != 0)
        .map_or(default, usize::from)
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        if self.wrap_pending {
            self.col = 0;
            self.line_feed();
            self.wrap_pending = false;
        }
        let mut cell = self.pen;
        cell.c = c;
        if let Some(line) = self.grid.get_mut(self.row)
            && let Some(slot) = line.get_mut(self.col)
        {
            *slot = cell;
        }
        if self.col + 1 >= self.cols {
            self.wrap_pending = true;
        } else {
            self.col += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => {
                self.line_feed();
                self.wrap_pending = false;
            }
            b'\r' => {
                self.col = 0;
                self.wrap_pending = false;
            }
            0x08 => {
                self.col = self.col.saturating_sub(1);
                self.wrap_pending = false;
            }
            b'\t' => {
                self.col = ((self.col / 8 + 1) * 8).min(self.cols - 1);
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let private = intermediates.first() == Some(&b'?');
        let p = |i: usize, d: usize| param(params, i, d);
        match (private, action) {
            (false, 'A') => {
                self.row = self
                    .row
                    .saturating_sub(p(0, 1))
                    .max(if self.row >= self.top { self.top } else { 0 })
            }
            (false, 'B') | (false, 'e') => {
                self.row = (self.row + p(0, 1)).min(if self.row <= self.bottom {
                    self.bottom
                } else {
                    self.rows - 1
                })
            }
            (false, 'C') | (false, 'a') => self.col = (self.col + p(0, 1)).min(self.cols - 1),
            (false, 'D') => self.col = self.col.saturating_sub(p(0, 1)),
            (false, 'E') => {
                self.row = (self.row + p(0, 1)).min(self.rows - 1);
                self.col = 0;
            }
            (false, 'F') => {
                self.row = self.row.saturating_sub(p(0, 1));
                self.col = 0;
            }
            (false, 'G') | (false, '`') => self.col = (p(0, 1) - 1).min(self.cols - 1),
            (false, 'd') => self.row = (p(0, 1) - 1).min(self.rows - 1),
            (false, 'H') | (false, 'f') => {
                self.row = (p(0, 1) - 1).min(self.rows - 1);
                self.col = (p(1, 1) - 1).min(self.cols - 1);
            }
            (false, 'J') => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(0);
                match mode {
                    0 => {
                        self.erase(self.row, self.col, self.cols);
                        for row in self.row + 1..self.rows {
                            self.erase(row, 0, self.cols);
                        }
                    }
                    1 => {
                        for row in 0..self.row {
                            self.erase(row, 0, self.cols);
                        }
                        self.erase(self.row, 0, self.col + 1);
                    }
                    2 | 3 => {
                        for row in 0..self.rows {
                            self.erase(row, 0, self.cols);
                        }
                        if mode == 3 {
                            self.scrollback.clear();
                        }
                    }
                    _ => {}
                }
            }
            (false, 'K') => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(0);
                match mode {
                    0 => self.erase(self.row, self.col, self.cols),
                    1 => self.erase(self.row, 0, self.col + 1),
                    2 => self.erase(self.row, 0, self.cols),
                    _ => {}
                }
            }
            (false, 'X') => self.erase(self.row, self.col, self.col + p(0, 1)),
            (false, '@') => {
                let blank = self.blank();
                let count = p(0, 1);
                if let Some(line) = self.grid.get_mut(self.row) {
                    for _ in 0..count.min(line.len()) {
                        line.insert(self.col, blank);
                        line.pop();
                    }
                }
            }
            (false, 'P') => {
                let blank = self.blank();
                let count = p(0, 1);
                if let Some(line) = self.grid.get_mut(self.row) {
                    for _ in 0..count.min(line.len().saturating_sub(self.col)) {
                        line.remove(self.col);
                        line.push(blank);
                    }
                }
            }
            (false, 'L') => {
                if (self.top..=self.bottom).contains(&self.row) {
                    let saved_top = self.top;
                    self.top = self.row;
                    self.scroll_down(p(0, 1));
                    self.top = saved_top;
                }
            }
            (false, 'M') => {
                if (self.top..=self.bottom).contains(&self.row) {
                    let saved_top = self.top;
                    self.top = self.row;
                    let count = p(0, 1);
                    for _ in 0..count {
                        self.grid.remove(self.top);
                        let blank = vec![self.blank(); self.cols];
                        self.grid.insert(self.bottom, blank);
                    }
                    self.top = saved_top;
                }
            }
            (false, 'S') => self.scroll_up(p(0, 1)),
            (false, 'T') => self.scroll_down(p(0, 1)),
            (false, 'r') => {
                let top = p(0, 1) - 1;
                let bottom = p(1, self.rows) - 1;
                if top < bottom && bottom < self.rows {
                    self.top = top;
                    self.bottom = bottom;
                    self.row = 0;
                    self.col = 0;
                }
            }
            (false, 'm') => self.sgr(params),
            (false, 's') => {
                self.saved = Some(Saved {
                    row: self.row,
                    col: self.col,
                    pen: self.pen,
                })
            }
            (false, 'u') => {
                if let Some(saved) = self.saved.clone() {
                    self.row = saved.row.min(self.rows - 1);
                    self.col = saved.col.min(self.cols - 1);
                }
            }
            (true, 'h') | (true, 'l') => {
                let on = action == 'h';
                for mode in params.iter().filter_map(|p| p.first().copied()) {
                    match mode {
                        1 => self.app_cursor = on,
                        25 => self.cursor_visible = on,
                        47 | 1047 | 1049 => {
                            if mode == 1049 && on {
                                self.saved = Some(Saved {
                                    row: self.row,
                                    col: self.col,
                                    pen: self.pen,
                                });
                            }
                            self.set_alternate(on);
                            if mode == 1049
                                && !on
                                && let Some(saved) = self.saved.clone()
                            {
                                self.row = saved.row.min(self.rows - 1);
                                self.col = saved.col.min(self.cols - 1);
                            }
                        }
                        2004 => self.bracketed_paste = on,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if action != 'm' {
            self.wrap_pending = false;
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => {
                self.saved = Some(Saved {
                    row: self.row,
                    col: self.col,
                    pen: self.pen,
                })
            }
            b'8' => {
                if let Some(saved) = self.saved.clone() {
                    self.row = saved.row.min(self.rows - 1);
                    self.col = saved.col.min(self.cols - 1);
                    self.pen = saved.pen;
                }
            }
            b'D' => self.line_feed(),
            b'E' => {
                self.col = 0;
                self.line_feed();
            }
            b'M' => {
                if self.row == self.top {
                    self.scroll_down(1);
                } else {
                    self.row = self.row.saturating_sub(1);
                }
            }
            b'c' => *self = Screen::new(self.cols, self.rows),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if let [kind, title, ..] = params
            && (*kind == b"0" || *kind == b"2")
        {
            self.title = String::from_utf8_lossy(title).into_owned();
        }
    }
}

/// A screen with its parser.
pub struct Terminal {
    pub screen: Screen,
    parser: Parser,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            screen: Screen::new(cols, rows),
            parser: Parser::new(),
        }
    }

    pub fn feed(&mut self, data: &str) {
        self.parser.advance(&mut self.screen, data.as_bytes());
    }

    /// Starts over (a `cleared` or `restarted` event).
    pub fn reset(&mut self) {
        self.screen = Screen::new(self.screen.cols, self.screen.rows);
        self.parser = Parser::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(terminal: &Terminal, row: usize) -> String {
        terminal.screen.grid[row]
            .iter()
            .map(|c| c.c)
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    #[test]
    fn prints_wraps_and_scrolls() {
        let mut terminal = Terminal::new(5, 2);
        terminal.feed("abcdefg\r\nxy\r\nz");
        assert_eq!(text(&terminal, 0), "xy");
        assert_eq!(text(&terminal, 1), "z");
        let first: String = terminal.screen.scrollback[0].iter().map(|c| c.c).collect();
        assert_eq!(first, "abcde");
    }

    #[test]
    fn colors_and_erase() {
        let mut terminal = Terminal::new(10, 2);
        terminal.feed("\x1b[1;31mred\x1b[0m \x1b[38;2;1;2;3mx\x1b[38;5;200my");
        let line = &terminal.screen.grid[0];
        assert_eq!(line[0].fg, Color::Indexed(1));
        assert!(line[0].flags & BOLD != 0);
        assert_eq!(line[3].fg, Color::Default);
        assert_eq!(line[4].fg, Color::Rgb(1, 2, 3));
        assert_eq!(line[5].fg, Color::Indexed(200));
        terminal.feed("\x1b[1;3H\x1b[K");
        assert_eq!(text(&terminal, 0), "re");
    }

    #[test]
    fn alternate_screen_restores_the_primary() {
        let mut terminal = Terminal::new(6, 2);
        terminal.feed("shell");
        terminal.feed("\x1b[?1049h\x1b[2J\x1b[Hvim");
        assert_eq!(text(&terminal, 0), "vim");
        terminal.feed("\x1b[?1049l");
        assert_eq!(text(&terminal, 0), "shell");
    }
}
