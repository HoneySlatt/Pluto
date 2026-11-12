//! The commands the bench's drivers write, read as the tool plays them.

use pluto_input::{
    Button, Command, KEYMAP, Modifier, Stroke, key_for_char, named_key, parse_combo, parse_line,
    parse_size, strokes_for_text,
};
use std::path::PathBuf;
use std::time::Duration;

fn stroke(modifiers: &[Modifier], keycode: u32) -> Stroke {
    Stroke {
        modifiers: modifiers.to_vec(),
        keycode,
    }
}

#[test]
fn every_printable_ascii_character_has_one_key() {
    let mut seen = std::collections::HashSet::new();
    for c in ' '..='~' {
        let key = key_for_char(c).unwrap_or_else(|| panic!("{c:?} has no key"));
        assert!(
            seen.insert(key),
            "{c:?} shares {key:?} with another character"
        );
    }
    assert_eq!(seen.len(), 95);
    assert_eq!(key_for_char('a'), Some((30, false)));
    assert_eq!(key_for_char('A'), Some((30, true)));
    assert_eq!(key_for_char('q'), Some((16, false)));
    assert_eq!(key_for_char('m'), Some((50, false)));
    assert_eq!(key_for_char('['), Some((26, false)));
    assert_eq!(key_for_char('?'), Some((53, true)));
    assert_eq!(key_for_char('é'), None);
    assert_eq!(key_for_char('\n'), None);
    assert_eq!(key_for_char('\t'), None);
}

#[test]
fn a_prompt_is_typed_with_shift_where_the_layout_needs_it() {
    assert_eq!(
        strokes_for_text("[turn 3] Hi!").unwrap(),
        vec![
            stroke(&[], 26),
            stroke(&[], 20),
            stroke(&[], 22),
            stroke(&[], 19),
            stroke(&[], 49),
            stroke(&[], 57),
            stroke(&[], 4),
            stroke(&[], 27),
            stroke(&[], 57),
            stroke(&[Modifier::Shift], 35),
            stroke(&[], 23),
            stroke(&[Modifier::Shift], 2),
        ]
    );
    assert_eq!(strokes_for_text("").unwrap(), vec![]);
    assert!(strokes_for_text("café").unwrap_err().contains("'é'"));
}

#[test]
fn combinations() {
    use Modifier::{Alt, Ctrl, Shift, Super};
    assert_eq!(
        parse_combo("ctrl+shift+o").unwrap(),
        stroke(&[Ctrl, Shift], 24)
    );
    assert_eq!(parse_combo("Return").unwrap(), stroke(&[], 28));
    assert_eq!(parse_combo("enter").unwrap(), stroke(&[], 28));
    assert_eq!(parse_combo("ctrl+1").unwrap(), stroke(&[Ctrl], 2));
    assert_eq!(parse_combo("alt+F4").unwrap(), stroke(&[Alt], 62));
    assert_eq!(parse_combo("super+Tab").unwrap(), stroke(&[Super], 15));
    // A character that needs Shift adds it once.
    assert_eq!(parse_combo("ctrl+?").unwrap(), stroke(&[Ctrl, Shift], 53));
    assert_eq!(parse_combo("shift+?").unwrap(), stroke(&[Shift], 53));
    assert_eq!(parse_combo("ctrl+O").unwrap(), stroke(&[Ctrl, Shift], 24));
    // The plus key itself.
    assert_eq!(parse_combo("+").unwrap(), stroke(&[Shift], 13));
    assert_eq!(parse_combo("ctrl++").unwrap(), stroke(&[Ctrl, Shift], 13));
    assert!(parse_combo("ctrl+").is_err());
    assert!(parse_combo("hyper+a").is_err());
    assert!(parse_combo("ctrl+nothing").is_err());
    assert!(parse_combo("é").is_err());
}

#[test]
fn named_keys() {
    assert_eq!(named_key("Escape"), Some(1));
    assert_eq!(named_key("BackSpace"), Some(14));
    assert_eq!(named_key("F1"), Some(59));
    assert_eq!(named_key("F10"), Some(68));
    assert_eq!(named_key("F11"), Some(87));
    assert_eq!(named_key("F0"), None);
    assert_eq!(named_key("F13"), None);
    assert_eq!(named_key("Delete"), Some(111));
    assert_eq!(named_key("o"), None);
}

#[test]
fn lines() {
    assert_eq!(parse_line(""), Ok(None));
    assert_eq!(parse_line("   "), Ok(None));
    assert_eq!(parse_line("# a comment"), Ok(None));
    assert_eq!(
        parse_line("type  two spaces\n"),
        Ok(Some(Command::Type(
            strokes_for_text(" two spaces").unwrap()
        )))
    );
    assert_eq!(
        parse_line("key ctrl+n Tab"),
        Ok(Some(Command::Keys(vec![
            stroke(&[Modifier::Ctrl], 49),
            stroke(&[], 15)
        ])))
    );
    assert_eq!(
        parse_line("delay 300"),
        Ok(Some(Command::Delay(Duration::from_millis(300))))
    );
    assert_eq!(
        parse_line("sleep 1000"),
        Ok(Some(Command::Sleep(Duration::from_millis(1000))))
    );
    assert_eq!(
        parse_line("move 1920 1080"),
        Ok(Some(Command::Move { x: 1920, y: 1080 }))
    );
    assert_eq!(parse_line("click"), Ok(Some(Command::Click(Button::Left))));
    assert_eq!(
        parse_line("click right"),
        Ok(Some(Command::Click(Button::Right)))
    );
    assert_eq!(
        parse_line("mark /tmp/run/input.7"),
        Ok(Some(Command::Mark(PathBuf::from("/tmp/run/input.7"))))
    );
    for bad in [
        "key",
        "sleep",
        "sleep soon",
        "sleep -1",
        "move 1",
        "move 1 2 3",
        "click twice",
        "mark",
        "jump",
        "type é",
    ] {
        assert!(parse_line(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn sizes() {
    assert_eq!(parse_size("3840x2160"), Ok((3840, 2160)));
    assert!(parse_size("3840").is_err());
    assert!(parse_size("0x10").is_err());
    assert!(parse_size("ax1").is_err());
}

#[test]
fn the_keymap_is_a_us_keyboard_on_evdev_codes() {
    assert!(KEYMAP.contains("include \"evdev+aliases(qwerty)\""));
    assert!(KEYMAP.contains("include \"pc+us+inet(evdev)\""));
    assert!(!KEYMAP.contains('\0'));
}
