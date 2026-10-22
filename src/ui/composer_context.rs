//! Context that rides on a user message (`shared/composerContextReferences.ts`,
//! `shared/composerContextLegacySend.ts`, `web/lib/composerContextReferences.ts`,
//! `web/lib/composerContextRecords.ts`, `web/lib/terminalContext.ts`): the
//! prompt holds an inline link per context, `[label](t3-context://v1/<kind>/<id>)`,
//! and the payload travels as the message's `context` records; a server
//! without `inlineMessageContext` gets the legacy text instead (the links as
//! labels, the payloads in trailing blocks). Ported for terminal excerpts.
use std::ops::Range;

use serde_json::{Value, json};

const HREF_PREFIX: &str = "t3-context://v1/";
const LABEL_MAX: usize = 200;

/// One inline reference in a text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Reference {
    pub kind: String,
    pub context_id: String,
    pub label: String,
    pub range: Range<usize>,
}

fn valid_kind(kind: &str) -> bool {
    kind.len() <= 40
        && kind.starts_with(|c: char| c.is_ascii_lowercase())
        && kind
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `sanitizeComposerContextLabel`: one line, no brackets, bounded, never empty.
pub(crate) fn sanitize_label(label: &str, kind: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | '\\' | '\r' | '\n') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned: String = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(LABEL_MAX)
        .collect();
    if cleaned.is_empty() {
        kind.to_owned()
    } else {
        cleaned
    }
}

/// `formatComposerContextReference`.
pub(crate) fn format_reference(kind: &str, context_id: &str, label: &str) -> String {
    let image = if kind == "image" { "!" } else { "" };
    format!(
        "{image}[{}]({HREF_PREFIX}{kind}/{context_id})",
        sanitize_label(label, kind)
    )
}

/// `collectComposerContextReferences`.
pub(crate) fn collect(text: &str) -> Vec<Reference> {
    let mut out = Vec::new();
    if !text.contains("](t3-context:") {
        return out;
    }
    let mut from = 0;
    while let Some(offset) = text[from..].find("](t3-context:") {
        let close = from + offset;
        from = close + 2;
        // The label: back to its `[`, on one line, at most 512 characters.
        let Some(open) = text[..close].rfind('[') else {
            continue;
        };
        let label = &text[open + 1..close];
        if label.contains(['\n', ']']) || label.chars().count() > 512 {
            continue;
        }
        let href_start = close + 2;
        let Some(length) = text[href_start..].find(|c: char| c == ')' || c.is_whitespace()) else {
            continue;
        };
        if !text[href_start + length..].starts_with(')') || length > 200 + HREF_PREFIX.len() {
            continue;
        }
        let href = &text[href_start..href_start + length];
        let Some(rest) = href.strip_prefix(HREF_PREFIX) else {
            continue;
        };
        let Some((kind, id)) = rest.split_once('/') else {
            continue;
        };
        if !valid_kind(kind) || !valid_id(id) {
            continue;
        }
        let start = if open > 0 && text[..open].ends_with('!') {
            open - 1
        } else {
            open
        };
        let end = href_start + length + 1;
        out.push(Reference {
            kind: kind.to_owned(),
            context_id: id.to_owned(),
            label: sanitize_label(label, kind),
            range: start..end,
        });
        from = end;
    }
    out
}

/// `formatTerminalContextLabel`: "Terminal 1 line 4" / "Terminal 1 lines 4-9".
pub(crate) fn terminal_label(terminal_label: &str, line_start: u64, line_end: u64) -> String {
    if line_start == line_end {
        format!("{terminal_label} line {line_start}")
    } else {
        format!("{terminal_label} lines {line_start}-{line_end}")
    }
}

/// `terminalContextRecord` for a selection (`normalizeTerminalContextSelection`):
/// None when the excerpt is blank.
pub(crate) fn terminal_record(
    terminal_id: &str,
    label: &str,
    line_start: u64,
    line_end: u64,
    text: &str,
) -> Option<Value> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim_matches('\n');
    if text.is_empty() || terminal_id.trim().is_empty() || label.trim().is_empty() {
        return None;
    }
    let line_start = line_start.max(1);
    let line_end = line_end.max(line_start);
    let id = format!("terminal_{}", uuid::Uuid::new_v4());
    Some(json!({
        "version": 1,
        "contextId": id,
        "kind": "terminal",
        "label": sanitize_label(&terminal_label(label.trim(), line_start, line_end), "terminal"),
        "terminalId": terminal_id.trim(),
        "terminalLabel": label.trim(),
        "lineStart": line_start,
        "lineEnd": line_end,
        "text": text,
    }))
}

/// `insertInlineContextReference`: the link at byte `cursor`, a space before
/// it when a word would join, one after it (absorbing a space already there).
pub(crate) fn insert_reference(prompt: &str, cursor: usize, reference: &str) -> (String, usize) {
    let cursor = cursor.min(prompt.len());
    let lead = !prompt[..cursor]
        .chars()
        .next_back()
        .is_none_or(|c| matches!(c, ' ' | '\n' | '\t' | '\r'));
    let end = if prompt[cursor..].starts_with(' ') {
        cursor + 1
    } else {
        cursor
    };
    let text = format!("{}{reference} ", if lead { " " } else { "" });
    (
        format!("{}{text}{}", &prompt[..cursor], &prompt[end..]),
        cursor + text.len(),
    )
}

/// `renderTerminalEntry`.
fn terminal_entry(record: &Value) -> String {
    let start = record["lineStart"].as_u64().unwrap_or(1);
    let end = record["lineEnd"].as_u64().unwrap_or(start);
    let range = if start == end {
        format!("line {start}")
    } else {
        format!("lines {start}-{end}")
    };
    let body: Vec<String> = record["text"]
        .as_str()
        .unwrap_or_default()
        .split('\n')
        .take((end - start + 1) as usize)
        .enumerate()
        .map(|(index, line)| {
            let numbered = format!("{} | {line}", start + index as u64);
            format!("  {numbered}")
        })
        .collect();
    format!(
        "- {} {range}:\n{}",
        record["terminalLabel"].as_str().unwrap_or_default(),
        body.join("\n")
    )
}

/// `inlineLabel`: a terminal excerpt as `@terminal-1:4-9`, else its label.
fn inline_label(record: &Value) -> String {
    if record["kind"] == "terminal" {
        let slug = record["terminalLabel"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");
        let start = record["lineStart"].as_u64().unwrap_or(1);
        let end = record["lineEnd"].as_u64().unwrap_or(start);
        let range = if start == end {
            start.to_string()
        } else {
            format!("{start}-{end}")
        };
        return format!("@{slug}:{range}");
    }
    record["label"].as_str().unwrap_or_default().to_owned()
}

/// `serializeLegacyContextMessage` (terminal excerpts).
pub(crate) fn legacy_text(text: &str, records: &[Value]) -> String {
    let mut out = text.to_owned();
    for reference in collect(text).into_iter().rev() {
        if let Some(record) = records
            .iter()
            .find(|r| r["contextId"] == reference.context_id.as_str())
        {
            out.replace_range(reference.range, &inline_label(record));
        }
    }
    let entries: Vec<String> = records
        .iter()
        .filter(|r| r["kind"] == "terminal")
        .map(terminal_entry)
        .filter(|e| !e.is_empty())
        .collect();
    let mut parts = vec![out.trim_end().to_owned()];
    if !entries.is_empty() {
        parts.push(format!(
            "<terminal_context>\n{}\n</terminal_context>",
            entries.join("\n")
        ));
    }
    parts.retain(|p| !p.is_empty());
    parts.join("\n\n")
}

/// The records `text` references, once each, in order.
pub(crate) fn referenced(text: &str, records: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for reference in collect(text) {
        if let Some(record) = records
            .iter()
            .find(|r| r["contextId"] == reference.context_id.as_str())
            && !out.iter().any(|r| r["contextId"] == record["contextId"])
        {
            out.push(record.clone());
        }
    }
    out
}

/// What a message sends: its text and context records, or the legacy text.
pub(crate) fn message_payload(
    text: &str,
    records: &[Value],
    inline: bool,
) -> (String, Option<Value>) {
    let records = referenced(text, records);
    if records.is_empty() {
        return (text.to_owned(), None);
    }
    if inline {
        (
            text.to_owned(),
            Some(json!({"version": 1, "records": records})),
        )
    } else {
        (legacy_text(text, &records), None)
    }
}

/// A chip's accent (`contextChipVariants`): `oklch(0.62 chroma hue)`.
pub(crate) fn kind_accent(kind: &str) -> (f32, f32) {
    match kind {
        "image" => (0.16, 16.0),
        "video" => (0.16, 48.0),
        "file" => (0.136, 237.0),
        "mention" => (0.11, 215.0),
        "terminal" | "pr-open" => (0.134, 163.0),
        "element" | "preview-annotation" => (0.134, 70.0),
        "review-comment" | "pr-merged" => (0.16, 292.0),
        "pull-request" => (0.16, 277.0),
        "pr-draft" => (0.02, 259.0),
        "pr-closed" => (0.16, 16.0),
        "skill" => (0.16, 322.0),
        _ => (0.16, 259.0),
    }
}

/// A chip's icon by kind.
pub(crate) fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "terminal" => "terminal",
        "image" => "image",
        "skill" => "skill-chip",
        "review-comment" => "message-square",
        _ => "paperclip",
    }
}

/// Sets a `thread.turn.start`'s message text and context records.
pub(crate) fn apply(command: &mut Value, text: &str, records: &[Value], inline: bool) {
    let (text, context) = message_payload(text, records, inline);
    command["message"]["text"] = json!(text);
    if let Some(context) = context {
        command["message"]["context"] = context;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Value {
        json!({"version": 1, "contextId": "terminal_abc", "kind": "terminal",
            "label": "Terminal 1 lines 3-4", "terminalId": "default",
            "terminalLabel": "Terminal 1", "lineStart": 3, "lineEnd": 4,
            "text": "$ cargo build\nerror: oops\n"})
    }

    #[test]
    fn references_round_trip() {
        let link = format_reference("terminal", "terminal_abc", "Terminal [1]\nlines 3-4");
        assert_eq!(
            link,
            "[Terminal 1 lines 3-4](t3-context://v1/terminal/terminal_abc)"
        );
        let text = format!("voir {link} et [x](https://a.b)");
        let found = collect(&text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].context_id, "terminal_abc");
        assert_eq!(&text[found[0].range.clone()], link);
        assert_eq!(
            collect("![shot](t3-context://v1/image/img_1)")[0].range,
            0..36
        );
        assert!(collect("[x](t3-context://v1/Bad/id)").is_empty());
    }

    #[test]
    fn insertion_keeps_word_boundaries() {
        assert_eq!(
            insert_reference("voir", 4, "[L](t)"),
            ("voir [L](t) ".into(), 12)
        );
        assert_eq!(
            insert_reference("a  b", 2, "[L](t)"),
            ("a [L](t) b".into(), 9)
        );
        assert_eq!(insert_reference("", 0, "[L](t)").0, "[L](t) ");
    }

    #[test]
    fn legacy_text_follows_upstream() {
        let text = "regarde [Terminal 1 lines 3-4](t3-context://v1/terminal/terminal_abc) stp";
        assert_eq!(
            legacy_text(text, &[record()]),
            "regarde @terminal-1:3-4 stp\n\n<terminal_context>\n- Terminal 1 lines 3-4:\n  3 | $ cargo build\n  4 | error: oops\n</terminal_context>"
        );
        let (sent, context) = message_payload(text, &[record()], true);
        assert_eq!(sent, text);
        assert_eq!(context.unwrap()["records"][0]["contextId"], "terminal_abc");
        assert_eq!(
            message_payload("plain", &[record()], true),
            ("plain".into(), None)
        );
    }

    #[test]
    fn terminal_records_normalize_their_text() {
        let record = terminal_record("default", "Terminal 1", 0, 0, "\r\nls\r\n").unwrap();
        assert_eq!(record["text"], "ls");
        assert_eq!(record["label"], "Terminal 1 line 1");
        assert!(terminal_record("default", "Terminal 1", 1, 1, "\n\n").is_none());
    }
}
