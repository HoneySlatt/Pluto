//! A terminal screen for the Terminal surface: the cell grid upstream's
//! ghostty-web keeps, emulated by `alacritty_terminal` (cursor motion,
//! scroll regions, SGR colors, the alternate screen, wide characters and
//! combining marks, mouse modes, OSC 8 hyperlinks, the window title) and
//! read here as plain cells. Replies the emulator owes the program (DA,
//! DSR, color queries) are collected for `terminal.write`.
//!
//! The Kitty keyboard protocol stays off: programs fall back to the legacy
//! keys the surface sends.
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{self, TermMode};
use alacritty_terminal::vte::ansi::{self, NamedColor, Processor};

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
/// The first cell of a double-width character.
pub const WIDE: u8 = 64;
/// The cell a wide character's second half covers: nothing to draw.
pub const SPACER: u8 = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    /// Combining marks drawn with `c`.
    pub marks: Option<Box<[char]>>,
    pub fg: Color,
    pub bg: Color,
    pub flags: u8,
    /// An OSC 8 hyperlink's URI.
    pub link: Option<Arc<str>>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            marks: None,
            fg: Color::Default,
            bg: Color::Default,
            flags: 0,
            link: None,
        }
    }
}

impl Cell {
    /// The cell's grapheme: its character and combining marks.
    pub fn text(&self) -> String {
        let mut text = String::from(self.c);
        text.extend(self.marks.iter().flat_map(|m| m.iter()));
        text
    }
}

const SCROLLBACK: usize = 5000;

fn color(color: ansi::Color) -> Color {
    match color {
        ansi::Color::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        ansi::Color::Indexed(i) => Color::Indexed(i),
        ansi::Color::Named(named) => {
            let index = named as usize;
            if index < 16 {
                Color::Indexed(index as u8)
            } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize)
                .contains(&index)
            {
                Color::Indexed((index - NamedColor::DimBlack as usize) as u8)
            } else {
                Color::Default
            }
        }
    }
}

fn cell(source: &term::cell::Cell) -> Cell {
    let mut flags = 0;
    for (from, to) in [
        (Flags::BOLD, BOLD),
        (Flags::ITALIC, ITALIC),
        (Flags::ALL_UNDERLINES, UNDERLINE),
        (Flags::INVERSE, INVERSE),
        (Flags::DIM, DIM),
        (Flags::STRIKEOUT, STRIKE),
        (Flags::WIDE_CHAR, WIDE),
        (
            Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
            SPACER,
        ),
    ] {
        if source.flags.intersects(from) {
            flags |= to;
        }
    }
    let hidden = source.flags.contains(Flags::HIDDEN);
    Cell {
        c: if hidden { ' ' } else { source.c },
        marks: source.zerowidth().map(Into::into),
        fg: color(source.fg),
        bg: color(source.bg),
        flags,
        link: source.hyperlink().map(|h| Arc::from(h.uri())),
    }
}

/// What the emulator tells its host: replies for the program, the title.
#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Outbox>>);

#[derive(Default)]
struct Outbox {
    replies: Vec<String>,
    title: Option<String>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let Ok(mut outbox) = self.0.lock() else {
            return;
        };
        match event {
            Event::PtyWrite(text) => outbox.replies.push(text),
            Event::Title(title) => outbox.title = Some(title),
            Event::ResetTitle => outbox.title = Some(String::new()),
            _ => {}
        }
    }
}

struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// How the program asked for mouse events (modes 1000/1002/1003, 1006, 1005).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseMode {
    /// Motion with a button held (1002) or any motion (1003).
    pub drag: bool,
    pub motion: bool,
    pub sgr: bool,
    pub utf8: bool,
}

/// The screen state, read by lines counted from the oldest history line.
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    term: term::Term<Listener>,
    outbox: Listener,
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let outbox = Listener::default();
        // Ghostty's default cursor blinks until a program asks otherwise.
        let config = term::Config {
            scrolling_history: SCROLLBACK,
            default_cursor_style: ansi::CursorStyle {
                shape: ansi::CursorShape::Block,
                blinking: true,
            },
            ..Default::default()
        };
        Self {
            cols,
            rows,
            term: term::Term::new(config, &Size { cols, rows }, outbox.clone()),
            outbox,
        }
    }

    fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    pub fn alternate(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    pub fn app_cursor(&self) -> bool {
        self.mode().contains(TermMode::APP_CURSOR)
    }

    pub fn bracketed_paste(&self) -> bool {
        self.mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub fn cursor_visible(&self) -> bool {
        self.mode().contains(TermMode::SHOW_CURSOR)
    }

    pub fn cursor_blinking(&self) -> bool {
        self.term.cursor_style().blinking
    }

    /// Wheel turns into arrow keys on the alternate screen (mode 1007).
    pub fn alternate_scroll(&self) -> bool {
        self.mode().contains(TermMode::ALTERNATE_SCROLL) && self.alternate()
    }

    pub fn mouse(&self) -> Option<MouseMode> {
        let mode = self.mode();
        mode.intersects(TermMode::MOUSE_MODE).then(|| MouseMode {
            drag: mode.contains(TermMode::MOUSE_DRAG),
            motion: mode.contains(TermMode::MOUSE_MOTION),
            sgr: mode.contains(TermMode::SGR_MOUSE),
            utf8: mode.contains(TermMode::UTF8_MOUSE),
        })
    }

    /// The cursor's row and column on the screen.
    pub fn cursor(&self) -> (usize, usize) {
        let point = self.term.grid().cursor.point;
        (
            point.line.0.max(0) as usize,
            point.column.0.min(self.cols - 1),
        )
    }

    /// Lines in the history above the screen.
    pub fn history_len(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Every line: the history, then the screen.
    pub fn len(&self) -> usize {
        self.history_len() + self.rows
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// Line `index`, 0 being the oldest history line.
    pub fn line(&self, index: usize) -> Vec<Cell> {
        if index >= self.len() {
            return Vec::new();
        }
        let line = Line(index as i32 - self.history_len() as i32);
        let row = &self.term.grid()[line];
        (0..self.cols).map(|col| cell(&row[Column(col)])).collect()
    }

    /// Whether line `index` continues on the next one (soft wrap).
    pub fn wrapped(&self, index: usize) -> bool {
        if index >= self.len() {
            return false;
        }
        let line = Line(index as i32 - self.history_len() as i32);
        self.term.grid()[line][Column(self.cols - 1)]
            .flags
            .contains(Flags::WRAPLINE)
    }

    /// The visible lines `offset` lines back into the history.
    pub fn view(&self, offset: usize) -> Vec<Vec<Cell>> {
        let first = self.history_len() - offset.min(self.history_len());
        (first..first + self.rows).map(|i| self.line(i)).collect()
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        self.cols = cols;
        self.rows = rows;
        self.term.resize(Size { cols, rows });
    }

    /// The window title the program last set.
    pub fn title(&self) -> Option<String> {
        self.outbox.0.lock().ok()?.title.clone()
    }
}

/// A screen with its parser.
pub struct Terminal {
    pub screen: Screen,
    parser: Processor,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            screen: Screen::new(cols, rows),
            parser: Processor::new(),
        }
    }

    pub fn feed(&mut self, data: &str) {
        self.parser.advance(&mut self.screen.term, data.as_bytes());
    }

    /// The replies the program asked for since the last call (`CSI c`,
    /// `CSI 6 n`, ...), to write back to it.
    pub fn take_replies(&mut self) -> String {
        self.screen
            .outbox
            .0
            .lock()
            .map(|mut outbox| std::mem::take(&mut outbox.replies).concat())
            .unwrap_or_default()
    }

    /// Starts over (a `cleared` or `restarted` event).
    pub fn reset(&mut self) {
        self.screen = Screen::new(self.screen.cols, self.screen.rows);
        self.parser = Processor::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(terminal: &Terminal, row: usize) -> String {
        let history = terminal.screen.history_len();
        terminal
            .screen
            .line(history + row)
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
        let first: String = terminal.screen.line(0).iter().map(|c| c.c).collect();
        assert_eq!(first, "abcde");
    }

    #[test]
    fn colors_and_erase() {
        let mut terminal = Terminal::new(10, 2);
        terminal.feed("\x1b[1;31mred\x1b[0m \x1b[38;2;1;2;3mx\x1b[38;5;200my");
        let line = terminal.screen.line(terminal.screen.history_len());
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
        assert!(terminal.screen.alternate());
        assert_eq!(text(&terminal, 0), "vim");
        terminal.feed("\x1b[?1049l");
        assert_eq!(text(&terminal, 0), "shell");
    }

    #[test]
    fn wide_characters_links_modes_and_replies() {
        let mut terminal = Terminal::new(8, 2);
        terminal.feed("中a\x1b]8;;https://example.com\x1b\\L\x1b]8;;\x1b\\");
        let line = terminal.screen.line(0);
        assert!(line[0].flags & WIDE != 0 && line[1].flags & SPACER != 0);
        assert_eq!(line[2].c, 'a');
        assert_eq!(line[3].link.as_deref(), Some("https://example.com"));
        terminal.feed("\x1b[?1000h\x1b[?1006h");
        assert!(terminal.screen.mouse().is_some_and(|m| m.sgr));
        terminal.feed("\x1b[6n");
        assert_eq!(terminal.take_replies(), "\x1b[1;5R");
    }
}
