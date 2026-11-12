//! The rivals' bench's keyboard and pointer
//! ([ADR-0028](../../../docs/adr/0028-the-open-rivals-and-the-web-chats-measured.md)).
//!
//! `pluto-input` holds one virtual keyboard and one virtual pointer for a whole run, created
//! before the subject starts, and plays the commands it reads from a pipe. A tool that makes a new
//! virtual device for each call, as `wtype` and `wlrctl` do, makes the application load a new
//! keymap or see a new pointer each time: Zed lost the first keys of some prompts, and clicks
//! were lost.
//!
//! The keyboard is a US keyboard (evdev keycodes, the `us` layout): each character is typed with
//! the key that makes it there, Shift held where the layout needs it, so that an application sees
//! the key codes a real keyboard sends. Text outside printable ASCII is refused.
//!
//! This module reads the commands; the binary speaks Wayland.
//!
//! # Commands
//!
//! One a line; a blank line or one that starts with `#` is skipped.
//!
//! | Command | Does |
//! |---|---|
//! | `type TEXT` | types the rest of the line, as is |
//! | `key COMBO...` | presses each combination in turn: `ctrl+shift+o`, `Return`, `alt+F4` |
//! | `delay MS` | the pause after each key from now on (10 ms at start) |
//! | `sleep MS` | waits |
//! | `move X Y` | moves the pointer to a point of the output, in pixels |
//! | `click [left\|right\|middle]` | presses and releases a button (left by default) |
//! | `mark PATH` | once the compositor has every event before it, creates the file `PATH` |
//!
//! A combination's modifiers are `ctrl`, `shift`, `alt` and `super`; its key is a printable
//! character or a key's name ([`named_key`]). A command that cannot be read is reported and
//! skipped: the commands after it still run.

use std::path::PathBuf;
use std::time::Duration;

/// The keymap the keyboard sends once: a US keyboard on evdev keycodes, which the compositor
/// compiles from its own XKB data.
pub const KEYMAP: &str = r#"xkb_keymap {
  xkb_keycodes { include "evdev+aliases(qwerty)" };
  xkb_types { include "complete" };
  xkb_compat { include "complete" };
  xkb_symbols { include "pc+us+inet(evdev)" };
  xkb_geometry { include "pc(pc105)" };
};
"#;

/// The pause after each key at start: enough for an application to see each key apart.
pub const DEFAULT_DELAY: Duration = Duration::from_millis(10);

/// A modifier, held while a key is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Super,
}

impl Modifier {
    /// Its key on the left of a US keyboard, as an evdev keycode.
    pub fn keycode(self) -> u32 {
        match self {
            Modifier::Shift => 42,
            Modifier::Ctrl => 29,
            Modifier::Alt => 56,
            Modifier::Super => 125,
        }
    }

    /// Its bit in the keymap's modifier mask (XKB's Shift, Control, Mod1 and Mod4).
    pub fn mask(self) -> u32 {
        match self {
            Modifier::Shift => 1,
            Modifier::Ctrl => 4,
            Modifier::Alt => 8,
            Modifier::Super => 64,
        }
    }

    fn from_name(name: &str) -> Option<Modifier> {
        match name.to_ascii_lowercase().as_str() {
            "shift" => Some(Modifier::Shift),
            "ctrl" | "control" => Some(Modifier::Ctrl),
            "alt" => Some(Modifier::Alt),
            "super" | "logo" | "meta" => Some(Modifier::Super),
            _ => None,
        }
    }
}

/// One key pressed with its modifiers held, in the order they are pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stroke {
    pub modifiers: Vec<Modifier>,
    /// The key, as an evdev keycode.
    pub keycode: u32,
}

/// A pointer's button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

impl Button {
    /// Its code in Linux's input events (`BTN_LEFT` and the next two).
    pub fn code(self) -> u32 {
        match self {
            Button::Left => 0x110,
            Button::Right => 0x111,
            Button::Middle => 0x112,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Type(Vec<Stroke>),
    Keys(Vec<Stroke>),
    Delay(Duration),
    Sleep(Duration),
    Move { x: u32, y: u32 },
    Click(Button),
    Mark(PathBuf),
}

/// Reads one line of commands: `Ok(None)` for a blank line or a comment, an error that says what is
/// wrong otherwise.
pub fn parse_line(line: &str) -> Result<Option<Command>, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let (name, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
    let command = match name {
        // The text is the rest of the line as written, its spaces kept.
        "type" => Command::Type(strokes_for_text(rest)?),
        "key" => Command::Keys(
            words(rest, "key needs at least one key")?
                .into_iter()
                .map(parse_combo)
                .collect::<Result<_, _>>()?,
        ),
        "delay" => Command::Delay(parse_ms(rest)?),
        "sleep" => Command::Sleep(parse_ms(rest)?),
        "move" => match words(rest, "move needs X and Y")?.as_slice() {
            [x, y] => Command::Move {
                x: parse_number(x)?,
                y: parse_number(y)?,
            },
            _ => return Err("move needs X and Y".to_owned()),
        },
        "click" => Command::Click(match rest.trim() {
            "" | "left" => Button::Left,
            "right" => Button::Right,
            "middle" => Button::Middle,
            other => return Err(format!("{other:?} is not a button: left, right or middle")),
        }),
        "mark" => match rest.trim() {
            "" => return Err("mark needs a path".to_owned()),
            path => Command::Mark(PathBuf::from(path)),
        },
        _ => return Err(format!("unknown command {name:?}")),
    };
    Ok(Some(command))
}

fn words<'a>(text: &'a str, if_none: &str) -> Result<Vec<&'a str>, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        Err(if_none.to_owned())
    } else {
        Ok(words)
    }
}

fn parse_number(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{text:?} is not a whole number"))
}

fn parse_ms(text: &str) -> Result<Duration, String> {
    parse_number(text).map(|ms| Duration::from_millis(ms.into()))
}

/// A combination such as `ctrl+shift+o`: modifiers, then one key. A character that needs Shift on
/// a US keyboard (`A`, `?`) adds it.
pub fn parse_combo(combo: &str) -> Result<Stroke, String> {
    // `ctrl++` is Ctrl with the plus key: the key is what follows the last separator, even `+`.
    let (modifiers, key) = if combo == "+" {
        ("", "+")
    } else if let Some(modifiers) = combo.strip_suffix("++") {
        (modifiers, "+")
    } else {
        combo.rsplit_once('+').unwrap_or(("", combo))
    };
    if key.is_empty() {
        return Err(format!("{combo:?} has no key"));
    }
    let mut held = Vec::new();
    for name in modifiers.split('+').filter(|name| !name.is_empty()) {
        let modifier =
            Modifier::from_name(name).ok_or_else(|| format!("{name:?} is not a modifier"))?;
        if !held.contains(&modifier) {
            held.push(modifier);
        }
    }
    let (keycode, shifted) = match named_key(key) {
        Some(keycode) => (keycode, false),
        None => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => {
                    key_for_char(c).ok_or_else(|| format!("{c:?} has no key on a US keyboard"))?
                }
                _ => return Err(format!("{key:?} is not a key")),
            }
        }
    };
    if shifted && !held.contains(&Modifier::Shift) {
        held.push(Modifier::Shift);
    }
    Ok(Stroke {
        modifiers: held,
        keycode,
    })
}

/// The strokes that type a text on a US keyboard; an error names the first character that has no
/// key there.
pub fn strokes_for_text(text: &str) -> Result<Vec<Stroke>, String> {
    text.chars()
        .map(|c| {
            let (keycode, shifted) =
                key_for_char(c).ok_or_else(|| format!("{c:?} has no key on a US keyboard"))?;
            Ok(Stroke {
                modifiers: if shifted {
                    vec![Modifier::Shift]
                } else {
                    vec![]
                },
                keycode,
            })
        })
        .collect()
}

/// The keys that have a name rather than a character, by the name a driver writes; case does not
/// matter. Their evdev keycodes.
pub fn named_key(name: &str) -> Option<u32> {
    let keycode = match name.to_ascii_lowercase().as_str() {
        "escape" | "esc" => 1,
        "backspace" => 14,
        "tab" => 15,
        "return" | "enter" => 28,
        "space" => 57,
        "home" => 102,
        "up" => 103,
        "pageup" => 104,
        "left" => 105,
        "right" => 106,
        "end" => 107,
        "down" => 108,
        "pagedown" => 109,
        "insert" => 110,
        "delete" => 111,
        "f11" => 87,
        "f12" => 88,
        other => {
            // F1 to F10 follow each other.
            let number: u32 = other.strip_prefix('f')?.parse().ok()?;
            return (1..=10).contains(&number).then_some(58 + number);
        }
    };
    Some(keycode)
}

/// The key that types a printable ASCII character on a US keyboard, as an evdev keycode, and
/// whether Shift is held for it.
pub fn key_for_char(c: char) -> Option<(u32, bool)> {
    const LETTERS: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
    const LETTER_KEYS: [u32; 26] = [
        30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17,
        45, 21, 44,
    ];
    // The keys that are not letters, each with its character alone and with Shift.
    const OTHERS: [(u32, char, char); 21] = [
        (2, '1', '!'),
        (3, '2', '@'),
        (4, '3', '#'),
        (5, '4', '$'),
        (6, '5', '%'),
        (7, '6', '^'),
        (8, '7', '&'),
        (9, '8', '*'),
        (10, '9', '('),
        (11, '0', ')'),
        (12, '-', '_'),
        (13, '=', '+'),
        (26, '[', '{'),
        (27, ']', '}'),
        (39, ';', ':'),
        (40, '\'', '"'),
        (41, '`', '~'),
        (43, '\\', '|'),
        (51, ',', '<'),
        (52, '.', '>'),
        (53, '/', '?'),
    ];
    if c == ' ' {
        return Some((57, false));
    }
    if c.is_ascii_alphabetic() {
        let lower = c.to_ascii_lowercase() as u8;
        let index = LETTERS.iter().position(|&letter| letter == lower)?;
        return Some((LETTER_KEYS[index], c.is_ascii_uppercase()));
    }
    OTHERS.iter().find_map(|&(keycode, plain, shifted)| {
        if c == plain {
            Some((keycode, false))
        } else if c == shifted {
            Some((keycode, true))
        } else {
            None
        }
    })
}

/// Reads `WIDTHxHEIGHT`, the output's size the pointer's positions are given in.
pub fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let (width, height) = text
        .split_once('x')
        .ok_or_else(|| format!("{text:?} is not a size like 3840x2160"))?;
    let (width, height) = (parse_number(width)?, parse_number(height)?);
    if width == 0 || height == 0 {
        return Err(format!("{text:?} is empty"));
    }
    Ok((width, height))
}
