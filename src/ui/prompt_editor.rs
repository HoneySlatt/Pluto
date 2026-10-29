//! Native prompt editing rules from `ComposerPromptEditorTiptap.tsx`,
//! `composer-code-block.ts` and `composer-undo-grouping.ts`. Keep Markdown
//! as the stored value: editing and undo must never serialize chip placeholders.

use std::ops::Range;

use eframe::egui::{self, Key, Modifiers, Ui};
use serde_json::Value;

use crate::app::App;
use crate::ui::cite;

#[derive(Clone, Default, PartialEq)]
struct Snapshot {
    text: String,
    contexts: Vec<Value>,
}

#[derive(Clone, Default)]
struct History {
    undo: egui::util::undoer::Undoer<Snapshot>,
}

fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn selection(ctx: &egui::Context, id: egui::Id, prompt: &str) -> Range<usize> {
    let collapsed = cite::collapse(prompt);
    let range = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range());
    let Some(range) = range else {
        return prompt.len()..prompt.len();
    };
    let [start, end] = range.sorted_cursors();
    let offset = |index| {
        cite::expand(
            &collapsed.display[..byte_at(&collapsed.display, index)],
            &collapsed.tokens,
        )
        .len()
    };
    offset(start.index.into())..offset(end.index.into())
}

fn caret(ctx: &egui::Context, id: egui::Id, prompt: &str, range: Range<usize>) {
    let offset = |byte| cite::collapse(&prompt[..byte]).display.chars().count();
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(offset(range.start)),
            egui::text::CCursor::new(offset(range.end)),
        )));
    state.clear_undoer();
    state.store(ctx, id);
}

/// Opening fences are top-level, matching the upstream editor's conversion
/// rule. A selection cannot indent across two separate fences.
fn fence(text: &str, selection: &Range<usize>) -> Option<(usize, String)> {
    let mut open: Option<(usize, String)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if offset > selection.start {
            break;
        }
        let bare = line.trim_end_matches('\n');
        if let Some((_, marker)) = &open {
            if bare.trim_end() == marker && offset + bare.len() < selection.start {
                open = None;
            }
        } else {
            let count = bare.chars().take_while(|c| *c == '`' || *c == '~').count();
            if count >= 3 {
                let marker = &bare[..count];
                if marker.chars().all(|c| marker.starts_with(c)) {
                    open = Some((offset, marker.to_owned()));
                }
            }
        }
        offset += line.len();
    }
    let (start, marker) = open?;
    let after_start = text[start..]
        .find('\n')
        .map_or(text.len(), |n| start + n + 1);
    let mut offset = after_start;
    for line in text[after_start..].split_inclusive('\n') {
        if line.trim_end() == marker && offset >= selection.start {
            return (selection.end <= offset + line.trim_end_matches('\n').len())
                .then_some((start, marker));
        }
        offset += line.len();
    }
    Some((start, marker))
}

fn list_prefix(line: &str) -> Option<(usize, String)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let content = &line[indent..];
    let (length, mut next) =
        if content.starts_with("- ") || content.starts_with("* ") || content.starts_with("+ ") {
            (2, content[..2].to_owned())
        } else {
            let digits = content.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || !content.get(digits..)?.starts_with(". ") {
                return None;
            }
            let number: u64 = content[..digits].parse().ok()?;
            (digits + 2, format!("{}. ", number.checked_add(1)?))
        };
    let task = content
        .get(length..)
        .is_some_and(|s| s.starts_with("[ ] ") || s.starts_with("[x] ") || s.starts_with("[X] "));
    if task {
        next.push_str("[ ] ");
    }
    Some((
        indent + length + if task { 4 } else { 0 },
        format!("{}{next}", &line[..indent]),
    ))
}

/// Enter inside code, or in a list, belongs to the document; Ctrl+Enter
/// remains the explicit send gesture. None leaves ordinary Enter alone.
fn newline(text: &mut String, range: Range<usize>) -> Option<Range<usize>> {
    let line_start = text[..range.start].rfind('\n').map_or(0, |n| n + 1);
    let before = &text[line_start..range.start];
    let insert = if let Some((opening, marker)) = fence(text, &range) {
        if line_start == opening {
            // Turn the typed opening fence into a block without sending it.
            "\n".to_owned()
        } else if before.trim_end() == marker {
            "\n".to_owned()
        } else if range.is_empty()
            && range.end == text.len()
            && before.trim().is_empty()
            && text[..line_start].trim_end_matches([' ', '\t', '\n']).len()
                < line_start.saturating_sub(1)
        {
            // Two blank lines at the end leave an otherwise unclosed fence.
            let end = text.trim_end_matches([' ', '\t', '\n']).len();
            text.truncate(end);
            text.push_str(&format!("\n{marker}\n"));
            return Some(text.len()..text.len());
        } else {
            let indent: String = before
                .chars()
                .take_while(|c| matches!(c, ' ' | '\t'))
                .collect();
            format!("\n{indent}")
        }
    } else if let Some((prefix, next)) = list_prefix(before) {
        if before[prefix..].trim().is_empty()
            && range.is_empty()
            && text[range.end..]
                .split('\n')
                .next()
                .is_none_or(|tail| tail.trim().is_empty())
        {
            text.replace_range(line_start..range.end, "");
            return Some(line_start..line_start);
        }
        format!("\n{next}")
    } else {
        return None;
    };
    let end = range.start + insert.len();
    text.replace_range(range, &insert);
    Some(end..end)
}

fn indent(text: &mut String, range: Range<usize>, out: bool) -> Option<Range<usize>> {
    let (opening, _) = fence(text, &range)?;
    let start = text[..range.start].rfind('\n').map_or(0, |n| n + 1);
    if start == opening {
        return None;
    }
    let end = text[range.end..]
        .find('\n')
        .map_or(text.len(), |n| range.end + n);
    if range.is_empty() && !out {
        text.insert_str(range.start, "  ");
        return Some(range.start + 2..range.start + 2);
    }
    let old = &text[start..end];
    let mut lines: Vec<&str> = old.split('\n').collect();
    // An endpoint at column zero does not select the following line.
    if range.end > range.start && text[..range.end].ends_with('\n') {
        lines.pop();
    }
    let mut changes = Vec::new();
    let mut offset = start;
    for line in lines {
        let removed = if out {
            if line.starts_with("  ") {
                2
            } else if line.starts_with([' ', '\t']) {
                1
            } else {
                0
            }
        } else {
            0
        };
        let insert = if !out && (!line.is_empty() || range.is_empty()) {
            "  "
        } else {
            ""
        };
        changes.push((offset, removed, insert));
        offset += line.len() + 1;
    }
    let map = |pos: usize| {
        let mut mapped = pos as isize;
        for (at, removed, inserted) in &changes {
            if *at <= pos {
                mapped += inserted.len() as isize - (*removed).min(pos - at) as isize;
            }
        }
        mapped as usize
    };
    let selection = map(range.start)..map(range.end);
    for (at, removed, inserted) in changes.into_iter().rev() {
        text.replace_range(at..at + removed, inserted);
    }
    Some(selection)
}

fn surround(text: &mut String, range: Range<usize>, marker: &str) -> Range<usize> {
    let len = marker.len();
    if range.start >= len
        && text[..range.start].ends_with(marker)
        && text[range.end..].starts_with(marker)
    {
        text.replace_range(range.end..range.end + len, "");
        text.replace_range(range.start - len..range.start, "");
        range.start - len..range.end - len
    } else {
        text.insert_str(range.end, marker);
        text.insert_str(range.start, marker);
        range.start + len..range.end + len
    }
}

impl App {
    /// Full-source undo also restores deleted mention/citation chips when
    /// their former placeholder index now belongs to a different chip.
    pub(crate) fn prompt_editor_keys(&mut self, ui: &Ui, id: egui::Id, rich: bool, focused: bool) {
        let snapshot = Snapshot {
            text: self.composer.clone(),
            contexts: self.composer_contexts.clone(),
        };
        let history_id = id.with("source-history");
        let time = ui.input(|i| i.time);
        ui.data_mut(|d| {
            d.get_temp_mut_or_default::<History>(history_id)
                .undo
                .feed_state(time, &snapshot)
        });
        if !focused {
            return;
        }
        let redo = ui.input_mut(|i| {
            i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || i.consume_key(Modifiers::COMMAND, Key::Y)
        });
        let undo = !redo && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
        if redo || undo {
            let restored = ui.data_mut(|d| {
                let history = &mut d.get_temp_mut_or_default::<History>(history_id).undo;
                if redo {
                    history.redo(&snapshot).cloned()
                } else {
                    history.undo(&snapshot).cloned()
                }
            });
            if let Some(restored) = restored {
                self.composer = restored.text;
                self.composer_contexts = restored.contexts;
                caret(
                    ui.ctx(),
                    id,
                    &self.composer,
                    self.composer.len()..self.composer.len(),
                );
            }
            return;
        }
        if !rich {
            return;
        }
        let range = selection(ui.ctx(), id, &self.composer);
        let mods = ui.input(|i| i.modifiers);
        let new_selection = if !mods.command
            && !mods.ctrl
            && !mods.alt
            && ui.input(|i| i.key_pressed(Key::Enter))
        {
            newline(&mut self.composer, range).inspect(|_| {
                ui.input_mut(|i| {
                    i.consume_key(mods, Key::Enter);
                });
            })
        } else if !mods.command && !mods.ctrl && !mods.alt && ui.input(|i| i.key_pressed(Key::Tab))
        {
            indent(&mut self.composer, range, mods.shift).inspect(|_| {
                ui.input_mut(|i| {
                    i.consume_key(mods, Key::Tab);
                });
            })
        } else {
            let marker = ui.input_mut(|i| {
                if i.consume_key(Modifiers::COMMAND, Key::B) {
                    Some("**")
                } else if i.consume_key(Modifiers::COMMAND, Key::I) {
                    Some("*")
                } else if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::X) {
                    Some("~~")
                } else if i.consume_key(Modifiers::COMMAND, Key::E) {
                    Some("`")
                } else {
                    None
                }
            });
            marker.map(|marker| surround(&mut self.composer, range, marker))
        };
        if let Some(range) = new_selection {
            ui.data_mut(|d| {
                d.get_temp_mut_or_default::<History>(history_id)
                    .undo
                    .add_undo(&snapshot)
            });
            caret(ui.ctx(), id, &self.composer, range);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_newlines_indent_and_leave_the_fence() {
        let mut text = "```rust\n  let x = 1;".to_owned();
        let end = text.len();
        newline(&mut text, end..end).unwrap();
        assert!(text.ends_with(";\n  "));
        let end = text.len();
        newline(&mut text, end..end).unwrap();
        let end = text.len();
        newline(&mut text, end..end).unwrap();
        assert_eq!(text, "```rust\n  let x = 1;\n```\n");
    }

    #[test]
    fn lists_continue_and_empty_items_exit() {
        for (source, result) in [
            ("- item", "- item\n- "),
            ("12. item", "12. item\n13. "),
            ("- [x] done", "- [x] done\n- [ ] "),
            ("- ", ""),
        ] {
            let mut text = source.to_owned();
            newline(&mut text, source.len()..source.len()).unwrap();
            assert_eq!(text, result);
        }
    }

    #[test]
    fn indentation_and_styles_keep_unicode_and_chip_sources() {
        let mut text = "```rust\nété\n  next\n```".to_owned();
        let start = text.find("été").unwrap();
        let end = text.find("\n```").unwrap();
        let range = indent(&mut text, start..end, false).unwrap();
        assert_eq!(&text[range.clone()], "été\n    next");
        indent(&mut text, range, true).unwrap();
        assert_eq!(text, "```rust\nété\n  next\n```");
        let mut text = "été".to_owned();
        let end = text.len();
        let range = surround(&mut text, 0..end, "**");
        assert_eq!(text, "**été**");
        surround(&mut text, range, "**");
        assert_eq!(text, "été");
    }

    #[test]
    fn undo_restores_source_not_reassigned_placeholders() {
        let before = Snapshot {
            text: "@alpha @beta".into(),
            contexts: vec![serde_json::json!({"contextId":"test"})],
        };
        let after = Snapshot {
            text: "@beta".into(),
            contexts: vec![],
        };
        let mut history = History::default();
        history.undo.add_undo(&before);
        assert_eq!(history.undo.undo(&after).unwrap().text, "@alpha @beta");
        assert_eq!(history.undo.redo(&before).unwrap().text, "@beta");
    }
    #[test]
    fn closing_fence_and_midline_tab_do_not_send_or_reindent() {
        let mut text = "```rust\nfoo\n```".to_owned();
        let end = text.len();
        newline(&mut text, end..end).unwrap();
        assert!(text.ends_with("```\n"));
        let mut text = "```rust\nété".to_owned();
        let at = text.len();
        indent(&mut text, at..at, false).unwrap();
        assert!(text.ends_with("été  "));
        let mut text = "- item".to_owned();
        newline(&mut text, 2..2).unwrap();
        assert_eq!(text, "- \n- item");
    }

    #[test]
    fn native_editor_undo_restores_deleted_chip_and_redoes_it() {
        use crate::app::Options;
        use egui_kittest::Harness;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .build_eframe(|cc| {
                App::new(
                    cc,
                    Options {
                        server: None,
                        token: None,
                        client_settings_path: None,
                        block_turn_starts: true,
                    },
                )
            });
        harness.state_mut().load_offline(
            &serde_json::json!({"type":"snapshot", "config":{"providers":[], "settings":{}}}),
            &serde_json::json!({"kind":"snapshot", "snapshot":{"snapshotSequence":1,
                "projects":[{"id":"p", "title":"Demo", "workspaceRoot":"/fixture/demo"}], "threads":[]}}),
        ).unwrap();
        harness.state_mut().composer =
            "[one](t3-context://v1/thread/one) [two](t3-context://v1/thread/two) ".into();
        harness.state_mut().composer_contexts = vec![
            serde_json::json!({"contextId":"one"}),
            serde_json::json!({"contextId":"two"}),
        ];
        harness.run_steps(3);
        let expected = harness.state().composer.clone();
        let id = harness.state().cite.editor_id.unwrap();
        harness.ctx.memory_mut(|m| m.request_focus(id));
        let mut state = egui::TextEdit::load_state(&harness.ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(2),
            )));
        state.store(&harness.ctx, id);
        let key = |h: &mut Harness<'_, App>, key, modifiers| {
            h.event(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            });
            h.run_steps(2);
            h.event(egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            });
            h.run_steps(1);
        };
        key(&mut harness, Key::Backspace, Modifiers::NONE);
        assert!(!harness.state().composer.contains("/one"));
        assert!(harness.state().composer.contains("/two"));
        key(&mut harness, Key::Z, Modifiers::COMMAND);
        assert_eq!(harness.state().composer, expected);
        key(&mut harness, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        assert!(!harness.state().composer.contains("/one"));
        assert!(harness.state().composer.contains("/two"));
    }
}
