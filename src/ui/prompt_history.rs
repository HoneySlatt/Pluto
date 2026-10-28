//! Terminal-style prompt recall for the composer (`composerPromptHistory.ts`):
//! ArrowUp on an empty composer walks back through the open thread's sent
//! prompts, ArrowDown walks forward and empties the composer past the newest.
//! History is per thread and text only, derived from the thread's user
//! messages on each keypress.

const CLAUDE_ULTRATHINK_PREFIX: &str = "Ultrathink:\n";
/// Sent in place of an empty prompt when a message is attachments only.
const ATTACHMENT_ONLY_BOOTSTRAP_PROMPT: &str = "[User attached one or more files without additional text. Respond using the conversation context and the attached files.]";
const PLAN_IMPLEMENTATION_PROMPT_PREFIX: &str = "PLEASE IMPLEMENT THIS PLAN:\n";
const LEGACY_CONTEXT_TAGS: [&str; 3] =
    ["terminal_context", "element_context", "preview_annotation"];

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub prompt: String,
}

/// Active recall: the entry, resolved by id on every step, and the text put
/// in the composer; once the composer no longer matches it, browsing is over.
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub entry_id: String,
    pub recalled: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Direction {
    Backward,
    Forward,
}

#[derive(Debug, PartialEq)]
pub struct Step {
    pub position: Option<Position>,
    pub prompt: String,
}

/// JavaScript's `\w`.
fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `<review_comment\b[^>]*>[\s\S]*?</review_comment>` matches, in order.
fn review_comment_blocks(text: &str) -> Vec<(usize, usize)> {
    const OPEN: &str = "<review_comment";
    const CLOSE: &str = "</review_comment>";
    let mut blocks = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(OPEN) {
        let start = from + found;
        let after = start + OPEN.len();
        let bounded = text[after..].chars().next().is_none_or(|c| !word(c));
        let tag_end = text[after..].find('>').map(|i| after + i + 1);
        let block = bounded.then_some(tag_end).flatten().and_then(|tag_end| {
            text[tag_end..]
                .find(CLOSE)
                .map(|i| tag_end + i + CLOSE.len())
        });
        match block {
            Some(end) => {
                blocks.push((start, end));
                from = end;
            }
            None => from = start + 1,
        }
    }
    blocks
}

/// Drops only the review comments appended at send time, at the end.
fn strip_trailing_review_comments(prompt: &str) -> String {
    let mut cut = prompt.len();
    for (start, end) in review_comment_blocks(prompt).into_iter().rev() {
        if !prompt[end..cut].trim().is_empty() {
            break;
        }
        cut = start;
    }
    if cut == prompt.len() {
        prompt.to_owned()
    } else {
        prompt[..cut].trim_end().to_owned()
    }
}

/// `\n*<(tag)>\n([\s\S]*?)\n</\1>\s*$`: the trailing context block's start
/// (its leading newlines included), tag and body.
fn trailing_legacy_context(prompt: &str) -> Option<(usize, &'static str, &str)> {
    let text = prompt.trim_end();
    LEGACY_CONTEXT_TAGS
        .iter()
        .filter_map(|&tag| {
            let close = format!("\n</{tag}>");
            let close_start = text.len().checked_sub(close.len())?;
            if !text.ends_with(&close) {
                return None;
            }
            let open = format!("<{tag}>\n");
            let body_start = text.find(&open)? + open.len();
            if body_start > close_start {
                return None;
            }
            let start = text[..body_start - open.len()].trim_end_matches('\n').len();
            Some((start, tag, &text[body_start..close_start]))
        })
        .min_by_key(|(start, ..)| *start)
}

/// `^(.+?) lines? (\d+(?:-\d+)?)$`: a terminal block header's name and range.
fn terminal_header(header: &str) -> Option<(&str, &str)> {
    let range_ok = |range: &str| {
        let mut parts = range.splitn(2, '-');
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        parts.next().is_some_and(digits) && parts.next().is_none_or(digits)
    };
    let mut from = 1;
    while let Some(found) = header.get(from..)?.find(" line") {
        let at = from + found;
        let rest = &header[at + " line".len()..];
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        if let Some(range) = rest.strip_prefix(' ')
            && range_ok(range)
        {
            return Some((&header[..at], range));
        }
        from = at + 1;
    }
    None
}

/// Removes each block entry's inline `@terminal-1:12-13` label (the first
/// whole match) and the single space beside it.
fn strip_inline_terminal_labels(prompt: &str, headers: &[&str]) -> String {
    let mut result = prompt.to_owned();
    for header in headers {
        let Some((name, range)) = terminal_header(header) else {
            continue;
        };
        let name = name
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");
        let label = format!("@{name}:{range}");
        let found = result.match_indices(&label).map(|(i, _)| i).find(|&i| {
            let before = result[..i].chars().next_back();
            let after = result[i + label.len()..].chars().next();
            before.is_none_or(|c| !(word(c) || matches!(c, '@' | '.' | '-')))
                && after.is_none_or(|c| !(c.is_ascii_digit() || c == '-'))
        });
        let Some(mut start) = found else {
            continue;
        };
        let mut end = start + label.len();
        if result[end..].starts_with(' ') {
            end += 1;
        } else if result[..start].ends_with(' ') {
            start -= 1;
        }
        result.replace_range(start..end, "");
    }
    result
}

/// `collectComposerContextReferences`: `[label](t3-context://v1/kind/id)` or
/// its `![...]` image form, valid hrefs only.
fn context_references(text: &str) -> Vec<(usize, usize)> {
    const HREF: &str = "t3-context://v1/";
    let valid = |href: &str| {
        let mut parts = href.split('/');
        let (Some(kind), Some(id), None) = (parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        let kind_ok = kind.len() <= 40
            && kind.starts_with(|c: char| c.is_ascii_lowercase())
            && kind
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let id_ok = (1..=128).contains(&id.len())
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        kind_ok && id_ok
    };
    let mut references = Vec::new();
    if !text.contains("](t3-context:") {
        return references;
    }
    let mut from = 0;
    while let Some(found) = text[from..].find('[') {
        let open = from + found;
        from = open + 1;
        let Some(close) = text[open + 1..].find([']', '\n']).map(|i| open + 1 + i) else {
            continue;
        };
        if !text[close..].starts_with(']') || text[open + 1..close].chars().count() > 512 {
            continue;
        }
        let Some(rest) = text[close + 1..].strip_prefix('(') else {
            continue;
        };
        let Some(href) = rest.strip_prefix(HREF) else {
            continue;
        };
        let length = href
            .find(|c: char| c.is_whitespace() || c == ')')
            .unwrap_or(href.len());
        if !(1..=200).contains(&href[..length].chars().count())
            || !href[length..].starts_with(')')
            || !valid(&href[..length])
        {
            continue;
        }
        let start = if text[..open].ends_with('!') {
            open - 1
        } else {
            open
        };
        let end = close + 1 + 1 + HREF.len() + length + 1;
        references.push((start, end));
        from = end;
    }
    references
}

/// `replaceComposerContextReferences`: each reference replaced by what
/// `replace` returns for its kind and label.
pub(crate) fn replace_context_references(
    text: &str,
    replace: impl Fn(&str, &str) -> String,
) -> String {
    const HREF: &str = "](t3-context://v1/";
    let mut result = String::new();
    let mut cursor = 0;
    for (start, end) in context_references(text) {
        let reference = &text[start..end];
        let open = reference.find('[').unwrap_or(0);
        let close = reference.find(HREF).unwrap_or(open + 1);
        let kind = reference[close + HREF.len()..]
            .split('/')
            .next()
            .unwrap_or_default();
        result.push_str(&text[cursor..start]);
        result.push_str(&replace(kind, &reference[open + 1..close]));
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
}

/// `recallableComposerPrompt`: a sent message reduced to the text the user
/// typed, without send-time appends or context chips.
pub fn recallable_prompt(message_text: &str) -> String {
    let mut prompt = message_text.trim().to_owned();
    if let Some(rest) = prompt.strip_prefix(CLAUDE_ULTRATHINK_PREFIX) {
        prompt = rest.to_owned();
    }
    while !prompt.is_empty() {
        let without_reviews = strip_trailing_review_comments(&prompt);
        if without_reviews != prompt {
            prompt = without_reviews;
            continue;
        }
        let Some((start, tag, body)) = trailing_legacy_context(&prompt) else {
            break;
        };
        let lines: Vec<&str> = body.split('\n').collect();
        if tag == "preview_annotation"
            && lines.iter().any(|l| {
                l.strip_prefix("<preview_annotation>")
                    .is_some_and(|r| r.trim().is_empty())
            })
            && !lines.iter().any(|l| {
                ["Id: ", "Page: ", "Comment: ", "Targets: "]
                    .iter()
                    .any(|p| l.starts_with(p))
            })
        {
            break;
        }
        let headers: Vec<&str> = lines
            .iter()
            .filter_map(|l| l.strip_prefix("- ")?.strip_suffix(':'))
            .filter(|h| !h.is_empty())
            .collect();
        if tag != "preview_annotation" && headers.is_empty() {
            break;
        }
        let kept = prompt[..start].trim_end().to_owned();
        prompt = if tag == "terminal_context" {
            strip_inline_terminal_labels(&kept, &headers)
        } else {
            kept
        };
    }
    for (mut start, mut end) in context_references(&prompt).into_iter().rev() {
        if prompt[end..].starts_with(' ') {
            end += 1;
        } else if prompt[..start].ends_with(' ') {
            start -= 1;
        }
        prompt.replace_range(start..end, "");
    }
    let trimmed = prompt.trim();
    if trimmed == ATTACHMENT_ONLY_BOOTSTRAP_PROMPT
        || trimmed.starts_with(PLAN_IMPLEMENTATION_PROMPT_PREFIX)
    {
        return String::new();
    }
    trimmed.to_owned()
}

/// `buildComposerPromptHistoryEntries`: oldest first, consecutive duplicates
/// collapsed onto the newest message, text-less sends skipped.
pub fn entries<'a>(messages: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for (id, role, text) in messages {
        if role != "user" {
            continue;
        }
        let prompt = recallable_prompt(text);
        if prompt.is_empty() {
            continue;
        }
        let entry = Entry {
            id: id.to_owned(),
            prompt,
        };
        match entries.last_mut() {
            Some(previous) if previous.prompt == entry.prompt => *previous = entry,
            _ => entries.push(entry),
        }
    }
    entries
}

/// The active entry: by id, else the newest one with the recalled text.
fn find_active(entries: &[Entry], position: &Position) -> Option<usize> {
    entries
        .iter()
        .position(|e| e.id == position.entry_id)
        .or_else(|| entries.iter().rposition(|e| e.prompt == position.recalled))
}

/// `stepComposerPromptHistory`: None when the key should move the caret.
pub fn step(
    direction: Direction,
    entries: &[Entry],
    position: Option<&Position>,
    current: &str,
) -> Option<Step> {
    let active = position
        .filter(|p| p.recalled == current)
        .and_then(|p| find_active(entries, p));
    let recall = |entry: &Entry| Step {
        position: Some(Position {
            entry_id: entry.id.clone(),
            recalled: entry.prompt.clone(),
        }),
        prompt: entry.prompt.clone(),
    };
    match direction {
        Direction::Backward => {
            if active.is_none() && !current.is_empty() {
                return None;
            }
            let index = match active {
                None => entries.len().checked_sub(1)?,
                Some(index) => index.checked_sub(1)?,
            };
            Some(recall(&entries[index]))
        }
        Direction::Forward => {
            let index = active? + 1;
            Some(match entries.get(index) {
                Some(entry) => recall(entry),
                None => Step {
                    position: None,
                    prompt: String::new(),
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Entry> {
        entries([
            ("m1", "user", "first"),
            ("a1", "assistant", "reply"),
            ("m2", "user", "second"),
            ("m3", "user", "third"),
        ])
    }

    fn position(id: &str, recalled: &str) -> Position {
        Position {
            entry_id: id.into(),
            recalled: recalled.into(),
        }
    }

    #[test]
    fn strips_annotations_with_literal_opening_tags() {
        for comment in [
            "Render <preview_annotation>",
            "Render\n<preview_annotation>\nhere",
            "Render &lt;preview_annotation&gt;",
        ] {
            let sent =
                format!("Prompt\n<preview_annotation>\nComment: {comment}\n</preview_annotation>");
            assert_eq!(recallable_prompt(&sent), "Prompt");
        }
        let malformed = "Prompt\n<preview_annotation>\nouter literal\n<preview_annotation>\ninner\n</preview_annotation>";
        assert_eq!(recallable_prompt(malformed), malformed);
    }

    #[test]
    fn strips_legacy_context_and_terminal_labels() {
        assert_eq!(
            recallable_prompt(
                "email@build:7\n<terminal_context>\n- Build line 7:\n  output\n</terminal_context>"
            ),
            "email@build:7"
        );
        let sent = "Ultrathink:\nInvestigate this\n\n<terminal_context>\n- Terminal 1 lines 12-13:\n  12 | git status\n  13 | On branch main\n</terminal_context>\n\n<element_context>\n- <button>:\n  url: https://example.com\n</element_context>";
        assert_eq!(recallable_prompt(sent), "Investigate this");
        for (typed, expected) in [
            ("Look at @terminal-1:4 please", "Look at please"),
            (
                "@terminal-1:4 typed twice: @terminal-1:4\n    indented  code",
                "typed twice: @terminal-1:4\n    indented  code",
            ),
            (
                "see @terminal-1:40 and @terminal-1:4-12 then @terminal-1:4",
                "see @terminal-1:40 and @terminal-1:4-12 then",
            ),
        ] {
            let sent = format!(
                "{typed}\n\n<terminal_context>\n- Terminal 1 line 4:\n  4 | ls\n</terminal_context>"
            );
            assert_eq!(recallable_prompt(&sent), expected);
        }
    }

    #[test]
    fn strips_trailing_review_comments_and_annotations() {
        let block =
            "<review_comment filePath=\"src/app.ts\">Keep this configurable.</review_comment>";
        assert_eq!(
            recallable_prompt(&format!("Please update this.\n\n{block}")),
            "Please update this."
        );
        let mid = format!("Before\n\n{block}\n\nAfter");
        assert_eq!(recallable_prompt(&mid), mid);
        assert_eq!(recallable_prompt(&format!("{mid}\n\n{block}")), mid);
        let sent = "Fix this\n\n<preview_annotation>\nPage: Example\n<element_context>\n- <button>:\n  html: Save\n</element_context>\n</preview_annotation>";
        assert_eq!(recallable_prompt(sent), "Fix this");
    }

    #[test]
    fn removes_context_references_and_app_composed_sends() {
        let sent = "Look at [Terminal](t3-context://v1/terminal/term-1) please\n    indented  code\n![image](t3-context://v1/image/img-1)";
        assert_eq!(
            recallable_prompt(sent),
            "Look at please\n    indented  code"
        );
        assert_eq!(
            recallable_prompt("![image](t3-context://v1/image/img-1)"),
            ""
        );
        assert_eq!(recallable_prompt("   "), "");
        assert_eq!(recallable_prompt(ATTACHMENT_ONLY_BOOTSTRAP_PROMPT), "");
        assert_eq!(
            recallable_prompt("PLEASE IMPLEMENT THIS PLAN:\n# Plan\n1. do it"),
            ""
        );
        // Citations stay: they are not context references.
        let cited = "[Assistant quote](t3-citation://v1/e/t/m?text=x) more";
        assert_eq!(recallable_prompt(cited), cited);
    }

    #[test]
    fn entries_collapse_consecutive_duplicates() {
        let prompts: Vec<_> = sample().into_iter().map(|e| e.prompt).collect();
        assert_eq!(prompts, ["first", "second", "third"]);
        let collapsed = entries([
            ("m1", "user", "same"),
            ("m2", "user", "same"),
            ("m3", "user", "other"),
            ("m4", "user", "same"),
        ]);
        let ids: Vec<_> = collapsed.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["m2", "m3", "m4"]);
    }

    #[test]
    fn steps_like_a_shell_history() {
        let entries = sample();
        let back =
            |p: Option<&Position>, current: &str| step(Direction::Backward, &entries, p, current);
        let fwd =
            |p: Option<&Position>, current: &str| step(Direction::Forward, &entries, p, current);
        assert_eq!(back(None, "typing"), None);
        let first = back(None, "").unwrap();
        assert_eq!(first.position, Some(position("m3", "third")));
        assert_eq!(
            back(first.position.as_ref(), "third").unwrap().prompt,
            "second"
        );
        assert_eq!(back(Some(&position("m1", "first")), "first"), None);
        let newer = fwd(Some(&position("m2", "second")), "second").unwrap();
        assert_eq!(newer.prompt, "third");
        assert_eq!(
            fwd(newer.position.as_ref(), "third"),
            Some(Step {
                position: None,
                prompt: String::new()
            })
        );
        let edited = position("m3", "third");
        assert_eq!(back(Some(&edited), "third edited"), None);
        assert_eq!(fwd(Some(&edited), "third edited"), None);
        assert_eq!(
            back(Some(&edited), "").unwrap().position,
            Some(position("m3", "third"))
        );
        assert_eq!(fwd(None, ""), None);
    }

    #[test]
    fn follows_entries_by_id_then_text() {
        let grown = entries([
            ("m0", "user", "zeroth"),
            ("m1", "user", "A"),
            ("m2", "user", "B"),
            ("m3", "user", "A"),
        ]);
        let older = step(Direction::Backward, &grown, Some(&position("m1", "A")), "A");
        assert_eq!(older.unwrap().prompt, "zeroth");
        let missing = step(
            Direction::Forward,
            &grown,
            Some(&position("gone", "not sent")),
            "not sent",
        );
        assert_eq!(missing, None);
        let collapsed = entries([("m1", "user", "first"), ("m3", "user", "A")]);
        let step = step(
            Direction::Backward,
            &collapsed,
            Some(&position("m2", "A")),
            "A",
        );
        assert_eq!(step.unwrap().prompt, "first");
    }
}
