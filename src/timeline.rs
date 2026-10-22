//! Timeline rows for the chat, ported from upstream
//! `session-logic.ts` (work log) and `MessagesTimeline.logic.ts` (rows,
//! settled-turn folds, tool group summaries). Subagent batches, user input
//! answers and proposed plans are not ported yet.
use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::Value;

use crate::model::{Activity, LatestTurn, Message, Thread};

#[derive(Clone, Debug, PartialEq)]
pub struct WorkEntry {
    pub id: String,
    pub created_at: String,
    pub turn_id: Option<String>,
    pub label: String,
    pub tone: String,
    pub detail: Option<String>,
    pub command: Option<String>,
    pub item_type: Option<String>,
    pub changed_files: Vec<String>,
    pub status: Option<String>,
    pub source_kind: String,
    pub question: Option<QuestionAnswer>,
}

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Message(Message),
    Work(WorkEntry),
}

impl Entry {
    fn id(&self) -> &str {
        match self {
            Entry::Message(m) => &m.id,
            Entry::Work(w) => &w.id,
        }
    }

    fn created_at(&self) -> &str {
        match self {
            Entry::Message(m) => &m.created_at,
            Entry::Work(w) => &w.created_at,
        }
    }

    fn turn_id(&self) -> Option<&str> {
        match self {
            Entry::Message(m) if m.role == "assistant" || m.role == "reasoning" => {
                m.turn_id.as_deref()
            }
            Entry::Message(_) => None,
            Entry::Work(w) => w.turn_id.as_deref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    User(Message),
    Assistant {
        message: Message,
        show_meta: bool,
        /// Turn diff bound to this message (`turnDiffSummaryByAssistantMessageId`).
        changes: Option<(String, Vec<crate::model::CheckpointFile>)>,
    },
    TurnFold {
        turn_id: String,
        label: String,
        expanded: bool,
        created_at: String,
    },
    /// A single tool call shown inline.
    Work {
        id: String,
        label: String,
        entry: Box<WorkEntry>,
    },
    /// A collapsed group of tool calls.
    WorkToggle {
        group_id: String,
        summary: String,
        expanded: bool,
        has_failure: bool,
    },
    /// Rows of an expanded tool group.
    WorkDetails {
        group_id: String,
        entries: Vec<WorkEntry>,
    },
    Working {
        started_at: Option<String>,
    },
    /// An image Codex may have generated (an `image_view` call that viewed
    /// no workspace image), read from Codex's saved file by call id. Not
    /// shown upstream: this client adds it after the turn's answer.
    GeneratedImage {
        tool_call_id: String,
    },
}

#[derive(Default)]
pub struct Expansion {
    pub turns: BTreeSet<String>,
    pub groups: BTreeSet<String>,
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn strip_trailing_exit_code(detail: &str) -> String {
    let trimmed = detail.trim_end();
    if let Some(index) = trimmed.rfind("<exited with exit code") {
        return trimmed[..index].trim_end().to_owned();
    }
    trimmed.to_owned()
}

/// Question/answer summary of a folded user-input request.
#[derive(Clone, Debug, PartialEq)]
pub struct QuestionAnswer {
    pub questions: Vec<String>,
    pub answers: Vec<String>,
}

fn answer_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(list) => list
            .iter()
            .map(answer_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(map) => map.get("answers").map(answer_text).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Port of `foldUserInputActivities`: each request becomes one
/// `user-input.answer-submitted` activity at its first position.
fn fold_user_input(ordered: Vec<&Activity>) -> Vec<Activity> {
    let is_input = |a: &Activity| {
        matches!(
            a.kind.as_str(),
            "user-input.requested" | "user-input.resolved" | "user-input.answer-submitted"
        ) && a.payload["requestId"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    };
    let mut groups: BTreeMap<String, Vec<&Activity>> = BTreeMap::new();
    for activity in ordered.iter().filter(|a| is_input(a)) {
        let id = activity.payload["requestId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        groups.entry(id).or_default().push(activity);
    }
    let mut out = Vec::new();
    for activity in ordered {
        if !is_input(activity) {
            out.push(activity.clone());
            continue;
        }
        let id = activity.payload["requestId"].as_str().unwrap_or_default();
        let group = &groups[id];
        if group[0].id != activity.id {
            continue;
        }
        let mut questions: Vec<(String, String)> = Vec::new();
        let mut labels: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for entry in group {
            for question in entry.payload["questions"].as_array().into_iter().flatten() {
                let Some(qid) = question["id"].as_str() else {
                    continue;
                };
                if let Some(text) = question["question"].as_str()
                    && !questions.iter().any(|(id, _)| id == qid)
                {
                    questions.push((qid.to_owned(), text.to_owned()));
                }
                let map = labels.entry(qid.to_owned()).or_default();
                for option in question["options"].as_array().into_iter().flatten() {
                    if let (Some(value), Some(label)) =
                        (option["value"].as_str(), option["label"].as_str())
                    {
                        map.insert(value.to_owned(), label.to_owned());
                    }
                }
            }
        }
        let raw = group
            .iter()
            .rev()
            .find(|a| a.kind == "user-input.answer-submitted" && a.payload["answers"].is_object())
            .or_else(|| {
                group
                    .iter()
                    .rev()
                    .find(|a| a.payload["answers"].is_object())
            })
            .map(|a| a.payload["answers"].clone())
            .unwrap_or_default();
        let answers: Vec<String> = raw
            .as_object()
            .into_iter()
            .flatten()
            .map(|(qid, value)| {
                let text = answer_text(value);
                labels
                    .get(qid)
                    .and_then(|m| m.get(&text))
                    .cloned()
                    .unwrap_or(text)
            })
            .filter(|s| !s.is_empty())
            .collect();
        let summary = if !answers.is_empty() {
            "User input submitted"
        } else if group.iter().any(|a| a.kind == "user-input.resolved") {
            "User input dismissed"
        } else {
            "User input requested"
        };
        let mut folded = activity.clone();
        folded.kind = "user-input.answer-submitted".into();
        folded.tone = "tool".into();
        folded.summary = summary.into();
        folded.payload = serde_json::json!({
            "questions": questions.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
            "answers": answers,
        });
        out.push(folded);
    }
    out
}

/// Port of `deriveWorkLogEntries` for the activity kinds providers emit today.
pub fn work_entries(activities: &[Activity]) -> Vec<WorkEntry> {
    let mut ordered: Vec<&Activity> = activities.iter().collect();
    ordered.sort_by(|a, b| {
        let seq = |a: &Activity| a.sequence.unwrap_or(u64::MAX);
        seq(a)
            .cmp(&seq(b))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut entries = Vec::new();
    for activity in &fold_user_input(ordered) {
        let skip = matches!(
            activity.kind.as_str(),
            "tool.started"
                | "task.started"
                | "task.updated"
                | "tool.progress"
                | "context-window.updated"
                | "turn.plan.updated"
        ) || activity.summary == "Checkpoint captured"
            || activity.kind.starts_with("worktree-setup");
        if skip {
            continue;
        }
        let payload = &activity.payload;
        let item_type = text(&payload["itemType"]);
        let detail_raw = text(&payload["detail"]);
        let command = [
            &payload["data"]["item"]["command"],
            &payload["data"]["item"]["input"]["command"],
            &payload["data"]["item"]["result"]["command"],
            &payload["data"]["command"],
        ]
        .into_iter()
        .find_map(command_text)
        .or_else(|| {
            (item_type.as_deref() == Some("command_execution"))
                .then(|| detail_raw.as_deref().map(strip_trailing_exit_code))
                .flatten()
        });
        let changed_files = payload["data"]["changes"]
            .as_array()
            .map(|changes| {
                changes
                    .iter()
                    .filter_map(|c| text(&c["path"]))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let title = text(&payload["title"]);
        let label = if activity.kind.starts_with("task.") {
            text(&payload["summary"])
                .or_else(|| text(&payload["detail"]))
                .unwrap_or_else(|| activity.summary.clone())
        } else {
            activity.summary.clone()
        };
        let detail = detail_raw
            .map(|d| strip_trailing_exit_code(&d))
            .filter(|d| Some(d) != title.as_ref() && *d != activity.summary)
            .filter(|d| Some(d) != command.as_ref());
        let tone = match activity.tone.as_str() {
            _ if activity.kind == "task.progress" => "thinking",
            "approval" => "info",
            other => other,
        };
        entries.push(WorkEntry {
            id: activity.id.clone(),
            created_at: activity.created_at.clone(),
            turn_id: activity.turn_id.clone(),
            label,
            tone: tone.to_owned(),
            detail,
            command,
            item_type,
            changed_files,
            status: text(&payload["status"]),
            source_kind: activity.kind.clone(),
            question: (activity.kind == "user-input.answer-submitted").then(|| QuestionAnswer {
                questions: string_list(&payload["questions"]),
                answers: string_list(&payload["answers"]),
            }),
        });
    }
    entries
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn command_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => text(&Value::String(s.clone())),
        Value::Array(parts) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        _ => None,
    }
}

/// `workLogEntryIsToolLike`.
pub fn is_tool_like(entry: &WorkEntry) -> bool {
    matches!(entry.tone.as_str(), "tool" | "thinking" | "error")
        || entry.command.as_ref().is_some_and(|c| !c.trim().is_empty())
        || entry.item_type.is_some()
}

/// `workEntryDisplayIndicatesToolFailure`.
pub fn indicates_failure(entry: &WorkEntry) -> bool {
    entry.tone == "error" || matches!(entry.status.as_deref(), Some("failed"))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Read,
    Edit,
    Command,
    Search,
    Other,
    Update,
}

fn action(entry: &WorkEntry) -> Action {
    let title = entry.label.trim().to_lowercase();
    if title == "read file" {
        Action::Read
    } else if entry.item_type.as_deref() == Some("file_change") || !entry.changed_files.is_empty() {
        Action::Edit
    } else if entry.item_type.as_deref() == Some("command_execution") || entry.command.is_some() {
        Action::Command
    } else if entry.item_type.as_deref() == Some("web_search") {
        Action::Search
    } else if is_tool_like(entry) {
        Action::Other
    } else {
        Action::Update
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Port of `summarizeToolGroup`, e.g. "Ran 2 commands and changed 1 file".
pub fn summarize_tool_group(entries: &[WorkEntry]) -> String {
    let mut groups: Vec<(Action, Vec<&WorkEntry>)> = Vec::new();
    for entry in entries {
        let action = action(entry);
        match groups.iter_mut().find(|(a, _)| *a == action) {
            Some((_, list)) => list.push(entry),
            None => groups.push((action, vec![entry])),
        }
    }
    let labels: Vec<String> = groups
        .iter()
        .map(|(action, list)| {
            let count = if *action == Action::Edit {
                let mut files = HashSet::new();
                let mut without = 0;
                for entry in list {
                    if entry.changed_files.is_empty() {
                        without += 1;
                    }
                    files.extend(entry.changed_files.iter());
                }
                files.len() + without
            } else {
                list.len()
            };
            match action {
                Action::Read => format!("Read {}", plural(count, "file", "files")),
                Action::Edit => format!("Changed {}", plural(count, "file", "files")),
                Action::Command => format!("Ran {}", plural(count, "command", "commands")),
                Action::Search => format!("Searched the web {}", plural(count, "time", "times")),
                Action::Other => format!("Used {}", plural(count, "tool", "tools")),
                Action::Update => format!("Received {}", plural(count, "update", "updates")),
            }
        })
        .collect();
    let sentence: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            if i == 0 {
                label.clone()
            } else {
                let mut chars = label.chars();
                chars
                    .next()
                    .map(|c| c.to_lowercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            }
        })
        .collect();
    match sentence.len() {
        0 => String::new(),
        1 => sentence[0].clone(),
        2 => sentence.join(" and "),
        n => format!("{}, and {}", sentence[..n - 1].join(", "), sentence[n - 1]),
    }
}

/// `workEntryDisplayLabel`: what a standalone row shows.
pub fn display_label(entry: &WorkEntry) -> String {
    if let Some(question) = &entry.question {
        let preview: Vec<String> = question
            .questions
            .iter()
            .map(|q| q.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|q| !q.is_empty())
            .collect();
        if !preview.is_empty() {
            return preview.join(" · ");
        }
    }
    entry
        .command
        .clone()
        .or_else(|| entry.detail.clone())
        .or_else(|| match entry.changed_files.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            [first, rest @ ..] => Some(format!("{first} +{} more", rest.len())),
        })
        .unwrap_or_else(|| capitalize(entry.label.trim()))
}

fn single_tool_call_label(entry: &WorkEntry) -> String {
    if let Some(command) = entry
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        return command.to_owned();
    }
    capitalize(entry.label.trim())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// Port of `@t3tools/shared/orchestrationTiming.formatDuration`.
pub fn format_duration(ms: i64) -> String {
    if ms < 0 {
        return "0ms".into();
    }
    if ms < 1_000 {
        return format!("{}ms", ms.max(1));
    }
    if ms < 10_000 {
        let tenths = (ms as f64 / 100.0).round() / 10.0;
        return if tenths >= 10.0 {
            "10s".into()
        } else {
            format!("{tenths:.1}s")
        };
    }
    if ms < 60_000 {
        return format!("{}s", (ms as f64 / 1_000.0).round() as i64);
    }
    let total = (ms as f64 / 1_000.0).round() as i64;
    let (hours, minutes, seconds) = (total / 3_600, (total % 3_600) / 60, total % 60);
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

pub fn parse_time(iso: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

fn elapsed_ms(start: &str, end: &str) -> Option<i64> {
    let (start, end) = (parse_time(start)?, parse_time(end)?);
    let ms = (end - start).num_milliseconds();
    (ms >= 0).then_some(ms)
}

fn unsettled_turn_id(latest: Option<&LatestTurn>, running: Option<&str>) -> Option<String> {
    if let Some(running) = running {
        return Some(running.to_owned());
    }
    let latest = latest?;
    let settled = latest.completed_at.is_some() && latest.state != "running";
    (!settled).then(|| latest.turn_id.clone())
}

fn terminal_assistant_ids(entries: &[Entry]) -> HashSet<String> {
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    let mut unkeyed = 0;
    for entry in entries {
        let Entry::Message(message) = entry else {
            continue;
        };
        if message.role == "user" {
            unkeyed += 1;
            continue;
        }
        if message.role != "assistant" {
            continue;
        }
        let key = match &message.turn_id {
            Some(turn) => format!("turn:{turn}"),
            None => format!("unkeyed:{unkeyed}"),
        };
        last.insert(key, message.id.clone());
    }
    last.into_values().collect()
}

struct Fold {
    turn_id: String,
    created_at: String,
    hidden: HashSet<String>,
    label: String,
}

fn turn_folds(
    entries: &[Entry],
    terminal: &HashSet<String>,
    latest: Option<&LatestTurn>,
    unfolded: &HashSet<String>,
) -> BTreeMap<String, Fold> {
    struct Group<'a> {
        entries: Vec<&'a Entry>,
        terminal: Option<&'a Message>,
        streaming: bool,
        start: Option<String>,
    }
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    let mut boundary: Option<String> = None;
    for entry in entries {
        if let Entry::Message(m) = entry
            && m.role == "user"
        {
            boundary = Some(m.created_at.clone());
            continue;
        }
        let Some(turn) = entry.turn_id() else {
            continue;
        };
        let group = groups.entry(turn.to_owned()).or_insert_with(|| {
            order.push(turn.to_owned());
            Group {
                entries: Vec::new(),
                terminal: None,
                streaming: false,
                start: boundary.take(),
            }
        });
        group.entries.push(entry);
        if let Entry::Message(m) = entry {
            if terminal.contains(&m.id) {
                group.terminal = Some(m);
            }
            if m.streaming && m.role != "reasoning" {
                group.streaming = true;
            }
        }
    }
    let mut folds = BTreeMap::new();
    for turn in order {
        let group = &groups[&turn];
        if unfolded.contains(&turn) || group.streaming {
            continue;
        }
        let terminal_index = group
            .terminal
            .and_then(|t| group.entries.iter().position(|e| e.id() == t.id))
            .unwrap_or(group.entries.len());
        let trailing = group
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                *i > terminal_index && !matches!(e, Entry::Message(m) if m.role == "reasoning")
            })
            .count();
        let mut hidden = HashSet::new();
        for (index, entry) in group.entries.iter().enumerate() {
            if Some(entry.id()) == group.terminal.map(|t| t.id.as_str()) {
                continue;
            }
            let single_trailing =
                trailing == 1 && matches!(entry, Entry::Work(w) if !indicates_failure(w));
            let reasoning = matches!(entry, Entry::Message(m) if m.role == "reasoning");
            if !reasoning && index > terminal_index && !single_trailing {
                continue;
            }
            // User input stays visible after its turn settles.
            if matches!(entry, Entry::Work(w) if w.question.is_some()) {
                continue;
            }
            hidden.insert(entry.id().to_owned());
        }
        let hides_work = group.entries.iter().any(|e| {
            hidden.contains(e.id()) && !matches!(e, Entry::Message(m) if m.role == "reasoning")
        });
        if hidden.is_empty() || !hides_work {
            continue;
        }
        let (Some(first), Some(first_hidden), Some(last)) = (
            group.entries.first(),
            group.entries.iter().find(|e| hidden.contains(e.id())),
            group.entries.last(),
        ) else {
            continue;
        };
        let last_end = match last {
            Entry::Message(m) => m.updated_at.clone(),
            Entry::Work(w) => w.created_at.clone(),
        };
        let latest_for_turn = latest.filter(|l| l.turn_id == turn);
        let elapsed = match latest_for_turn {
            Some(LatestTurn {
                started_at: Some(start),
                completed_at: Some(end),
                ..
            }) => elapsed_ms(start, end),
            _ => {
                let end = group
                    .terminal
                    .map(|t| t.updated_at.clone())
                    .filter(|t| *t > last_end)
                    .unwrap_or(last_end);
                elapsed_ms(group.start.as_deref().unwrap_or(first.created_at()), &end)
            }
        };
        let duration = elapsed.map(format_duration);
        let interrupted = latest_for_turn.is_some_and(|l| l.state == "interrupted");
        let label = match (interrupted, duration) {
            (true, Some(d)) => format!("You stopped after {d}"),
            (true, None) => "You stopped this response".into(),
            (false, Some(d)) => format!("Worked for {d}"),
            (false, None) => "Worked".into(),
        };
        folds.insert(
            first_hidden.id().to_owned(),
            Fold {
                turn_id: turn.clone(),
                created_at: first_hidden.created_at().to_owned(),
                hidden,
                label,
            },
        );
    }
    folds
}

pub fn rows(thread: &Thread, expansion: &Expansion) -> Vec<Row> {
    let mut entries: Vec<Entry> = thread
        .messages
        .iter()
        .filter(|m| m.role != "system")
        .cloned()
        .map(Entry::Message)
        .chain(
            work_entries(&thread.activities)
                .into_iter()
                .map(Entry::Work),
        )
        .collect();
    entries.sort_by(|a, b| a.created_at().cmp(b.created_at()));

    let is_working = thread.is_running();
    let running_turn = thread
        .session
        .as_ref()
        .filter(|s| s.status == "running")
        .and_then(|s| s.active_turn_id.as_deref());
    let terminal = terminal_assistant_ids(&entries);
    let unsettled = unsettled_turn_id(thread.latest_turn.as_ref(), running_turn);
    let mut active_turns: HashSet<String> = HashSet::new();
    if let Some(turn) = &unsettled {
        active_turns.insert(turn.clone());
        if is_working {
            let last_user = entries
                .iter()
                .rposition(|e| matches!(e, Entry::Message(m) if m.role == "user"));
            for entry in entries.iter().skip(last_user.map_or(0, |i| i + 1)) {
                if let Some(turn) = entry.turn_id() {
                    active_turns.insert(turn.to_owned());
                }
            }
        }
    }
    let folds = turn_folds(
        &entries,
        &terminal,
        thread.latest_turn.as_ref(),
        &active_turns,
    );
    let mut collapsed: HashSet<String> = HashSet::new();
    for fold in folds.values() {
        if !expansion.turns.contains(&fold.turn_id) {
            collapsed.extend(fold.hidden.iter().cloned());
        }
    }
    let working_index = if is_working {
        entries
            .iter()
            .rposition(|e| matches!(e, Entry::Message(m) if m.role == "user"))
            .map_or(0, |i| i + 1)
    } else {
        usize::MAX
    };
    let started_at = thread
        .latest_turn
        .as_ref()
        .filter(|l| Some(&l.turn_id) == unsettled.as_ref())
        .map(|l| l.started_at.clone().unwrap_or(l.requested_at.clone()))
        .or_else(|| thread.session.as_ref().map(|s| s.updated_at.clone()));

    let mut out = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        if index == working_index {
            out.push(Row::Working {
                started_at: started_at.clone(),
            });
        }
        let entry = &entries[index];
        if let Some(fold) = folds.get(entry.id()) {
            out.push(Row::TurnFold {
                turn_id: fold.turn_id.clone(),
                label: fold.label.clone(),
                expanded: expansion.turns.contains(&fold.turn_id),
                created_at: fold.created_at.clone(),
            });
        }
        if collapsed.contains(entry.id()) {
            index += 1;
            continue;
        }
        match entry {
            Entry::Work(first) => {
                let mut group = vec![first.clone()];
                let mut cursor = index + 1;
                while let Some(Entry::Work(next)) = entries.get(cursor) {
                    if next.tone == "error"
                        || next.question.is_some()
                        || collapsed.contains(&next.id)
                        || folds.contains_key(&next.id)
                    {
                        break;
                    }
                    group.push(next.clone());
                    cursor += 1;
                }
                let group_id = format!("work-group:{}", first.id);
                if first.tone == "error" || first.question.is_some() {
                    out.push(Row::Work {
                        id: first.id.clone(),
                        label: display_label(first),
                        entry: Box::new(first.clone()),
                    });
                    cursor = index + 1;
                } else if group.len() == 1 && is_tool_like(&group[0]) {
                    let label = if action(&group[0]) == Action::Edit {
                        summarize_tool_group(&group)
                    } else {
                        single_tool_call_label(&group[0])
                    };
                    out.push(Row::Work {
                        id: first.id.clone(),
                        label,
                        entry: Box::new(group[0].clone()),
                    });
                } else {
                    let expanded = expansion.groups.contains(&group_id);
                    let summary = if group.len() == 1 && !is_tool_like(&group[0]) {
                        group[0].label.clone()
                    } else {
                        summarize_tool_group(&group)
                    };
                    out.push(Row::WorkToggle {
                        group_id: group_id.clone(),
                        summary,
                        expanded,
                        has_failure: group
                            .iter()
                            .rev()
                            .find(|e| is_tool_like(e))
                            .is_some_and(indicates_failure),
                    });
                    if expanded {
                        out.push(Row::WorkDetails {
                            group_id,
                            entries: group,
                        });
                    }
                }
                index = cursor;
                continue;
            }
            Entry::Message(message) if message.role == "user" => {
                out.push(Row::User(message.clone()))
            }
            Entry::Message(message) if message.role == "assistant" => {
                let in_progress = message
                    .turn_id
                    .as_ref()
                    .is_some_and(|t| active_turns.contains(t));
                let changes = thread
                    .checkpoints
                    .iter()
                    .find(|c| c.assistant_message_id.as_deref() == Some(message.id.as_str()))
                    .filter(|c| !c.files.is_empty())
                    .map(|c| (c.turn_id.clone(), c.files.clone()));
                out.push(Row::Assistant {
                    message: message.clone(),
                    show_meta: terminal.contains(&message.id) && !in_progress,
                    changes,
                });
            }
            Entry::Message(_) => {}
        }
        index += 1;
    }
    if is_working && working_index >= entries.len() {
        out.push(Row::Working { started_at });
    }
    insert_generated_images(&mut out, &thread.activities);
    out
}

/// Completed `image_view` calls without a workspace image path (Codex's
/// image generation, its item stripped by the server): call id and turn.
pub fn generated_images(activities: &[Activity]) -> Vec<(&str, Option<&str>)> {
    activities
        .iter()
        .filter(|a| {
            a.kind == "tool.completed"
                && a.payload["itemType"] == "image_view"
                && a.payload["status"] != "failed"
                && a.payload["data"]["imagePath"].is_null()
        })
        .filter_map(|a| Some((a.payload["toolCallId"].as_str()?, a.turn_id.as_deref())))
        .collect()
}

/// Each generated image follows its turn's last answer (or ends the
/// timeline before the working row), so a folded turn still shows it.
fn insert_generated_images(out: &mut Vec<Row>, activities: &[Activity]) {
    for (tool_call_id, turn) in generated_images(activities) {
        let row = Row::GeneratedImage {
            tool_call_id: tool_call_id.to_owned(),
        };
        let after_answer = out.iter().rposition(|r| {
            matches!(r, Row::Assistant { message, .. }
                if turn.is_some() && message.turn_id.as_deref() == turn)
        });
        match after_answer {
            Some(index) => {
                // After the answer and the images already placed there.
                let mut at = index + 1;
                while matches!(out.get(at), Some(Row::GeneratedImage { .. })) {
                    at += 1;
                }
                out.insert(at, row);
            }
            None => {
                let at = out
                    .iter()
                    .position(|r| matches!(r, Row::Working { .. }))
                    .unwrap_or(out.len());
                out.insert(at, row);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn activity(id: &str, at: &str, title: &str, item_type: &str, detail: &str) -> Activity {
        Activity {
            id: id.into(),
            tone: "tool".into(),
            kind: "tool.completed".into(),
            summary: title.into(),
            payload: json!({"itemType":item_type,"title":title,"detail":detail,"status":"completed"}),
            turn_id: Some("turn".into()),
            sequence: None,
            created_at: at.into(),
        }
    }

    fn message(id: &str, role: &str, at: &str, turn: Option<&str>) -> Message {
        Message {
            id: id.into(),
            role: role.into(),
            text: "text".into(),
            attachments: Vec::new(),
            turn_id: turn.map(Into::into),
            streaming: false,
            created_at: at.into(),
            updated_at: at.into(),
        }
    }

    fn settled_thread() -> Thread {
        Thread {
            messages: vec![
                message("u", "user", "2026-01-01T00:00:00.000Z", None),
                message("a", "assistant", "2026-01-01T00:04:00.000Z", Some("turn")),
            ],
            activities: vec![
                activity(
                    "c1",
                    "2026-01-01T00:01:00.000Z",
                    "Ran command",
                    "command_execution",
                    "cargo build",
                ),
                activity(
                    "c2",
                    "2026-01-01T00:02:00.000Z",
                    "Ran command",
                    "command_execution",
                    "cargo test",
                ),
                activity(
                    "f1",
                    "2026-01-01T00:03:00.000Z",
                    "File change",
                    "file_change",
                    "src/app.rs",
                ),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn settled_turn_folds_work_behind_worked_for() {
        let rows = rows(&settled_thread(), &Expansion::default());
        assert!(matches!(&rows[0], Row::User(_)));
        assert!(matches!(&rows[1], Row::TurnFold { label, .. } if label == "Worked for 4m"));
        assert!(matches!(
            &rows[2],
            Row::Assistant {
                show_meta: true,
                ..
            }
        ));
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn expanded_fold_summarizes_the_tool_group() {
        let mut expansion = Expansion::default();
        expansion.turns.insert("turn".into());
        let rows = rows(&settled_thread(), &expansion);
        assert!(matches!(&rows[2], Row::WorkToggle { summary, .. }
            if summary == "Ran 2 commands and changed 1 file"));
    }

    #[test]
    fn durations_match_upstream_formatting() {
        assert_eq!(format_duration(1), "1ms");
        assert_eq!(format_duration(4_250), "4.3s");
        assert_eq!(format_duration(42_000), "42s");
        assert_eq!(format_duration(3_723_000), "1h 2m 3s");
    }

    #[test]
    fn generated_images_follow_their_turn_answer() {
        let mut generated = activity("a", "2026-01-01T00:00:01Z", "Image view", "image_view", "");
        generated.payload["toolCallId"] = json!("exec-1");
        let mut viewed = activity("b", "2026-01-01T00:00:02Z", "Image view", "image_view", "");
        viewed.payload["toolCallId"] = json!("exec-2");
        viewed.payload["data"] = json!({"imagePath": "shot.png"});
        assert_eq!(
            generated_images(&[generated.clone(), viewed]),
            [("exec-1", Some("turn"))]
        );
        let mut out = vec![
            Row::Assistant {
                message: message("m", "assistant", "2026-01-01T00:00:03Z", Some("turn")),
                show_meta: true,
                changes: None,
            },
            Row::Working { started_at: None },
        ];
        insert_generated_images(&mut out, &[generated]);
        assert!(
            matches!(&out[1], Row::GeneratedImage { tool_call_id } if tool_call_id == "exec-1")
        );
    }
}
