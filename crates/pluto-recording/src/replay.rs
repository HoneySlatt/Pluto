//! The fake agent's logic: given a recording and the live client's lines, which recorded agent
//! lines to give back, and when ([ADR-0025]). The binary `pluto-replay-agent` does the I/O.
//!
//! **Matching.** Each live client line is matched with the earliest recorded client record of
//! the same kind not yet matched, so that frames the client sends in a different order (Claude's
//! SDK writes a prompt without waiting for its `initialize` answer) still match. Kinds and the
//! key fields compared within a kind:
//!
//! | Protocol | Kind | Key fields |
//! |---|---|---|
//! | Claude | `user` | the text of `message.content` (a string, or its `text` blocks joined) |
//! | Claude | `control_request:<request.subtype>` | `request.mode` when present |
//! | Claude | `control_response` | `response.request_id`, `response.subtype`, `response.response.behavior` and `response.response.updatedInput.answers` when present |
//! | Claude | any other `type` | none |
//! | ACP | `request:<method>` (has `method` and `id`) | for `session/prompt`, the `text` of `params.prompt`'s parts |
//! | ACP | `notify:<method>` (has `method`, no `id`) | none |
//! | ACP | `response` (has `id`, no `method`) | `id`, `result.outcome`, and whether it is an error |
//! | both | `line` (not JSON) | the whole text |
//!
//! Everything else in a client frame is the client's own and is not compared.
//!
//! **Placeholders.** Where the recorded client frame holds `<id:N>`, the live frame's value at
//! the same place is learned for `<id:N>`, whatever its JSON type. The spawn's arguments are
//! learned the same way, by flag (`--session-id=<id:1>` against `--session-id=…`, or a flag and
//! the argument after it). A placeholder learned twice with two values is a mismatch. In agent
//! frames given back, `<id:N>` is replaced by its live value (keeping its type when it is the
//! whole string), `<workspace>` and `<home>` by the live folders; an unlearned placeholder stays.
//!
//! **Release.** Records are given back in their recorded order. An agent record is given back once
//! every client record and `stdin_closed` before it has been matched. The recorded `exit` comes
//! last.
//!
//! The details the text above leaves open:
//!
//! - A line is a frame when it parses as JSON; a frame without a kind the table names has the
//!   kind `untyped`.
//! - A key field is compared when either side has it; the recorded one with its placeholders
//!   replaced first. A user's text blocks are joined with newlines, and so are a prompt's parts.
//! - `<id:N>` inside a longer recorded string is learned when it is the only id there and the
//!   text around it matches.
//! - Closing stdin matches the earliest recorded `stdin_closed` not yet matched; when none is
//!   left it changes nothing.
//! - `waiting_for` names `stdin_closed` when that is what comes first.
//!
//! [ADR-0025]: ../../../docs/adr/0025-agents-sessions-recorded-and-replayed.md

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use serde_json::Value;

use crate::format::{Protocol, Record, Recording, Side, Stream};

/// The live run's folders, which replace `<workspace>` and `<home>` in agent frames.
#[derive(Clone, Debug, Default)]
pub struct LivePlaces {
    pub workspace: Option<String>,
    pub home: Option<String>,
}

/// What the fake agent must do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Write this line, then a newline, on stdout.
    Stdout(String),
    /// Write this line, then a newline, on stderr.
    Stderr(String),
    /// Exit like the recorded process: with this code, or killed by this signal.
    Exit {
        code: Option<i32>,
        signal: Option<i32>,
    },
}

/// Where a replay stands; the fake agent writes it to its state folder when it stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub process: u32,
    /// Records of this process given back or matched.
    pub consumed: usize,
    /// Records of this process, actions excluded.
    pub total: usize,
    /// Whether every record was consumed.
    pub complete: bool,
    /// The kind of the first recorded client record not yet matched, if any.
    pub waiting_for: Option<String>,
}

/// Why a replay failed. Messages name kinds and fields, never a whole frame's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// The recording has no process with this number.
    NoSuchProcess(u32),
    /// The live client sent a frame of a kind the recording does not have left.
    Unexpected { kind: String },
    /// The live frame's key field differs from the recorded one.
    Mismatch {
        kind: String,
        field: String,
        expected: String,
        found: String,
    },
    /// A placeholder was learned with two different values.
    Conflict { placeholder: String },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::NoSuchProcess(process) => {
                write!(f, "the recording has no process {process}")
            }
            ReplayError::Unexpected { kind } => {
                write!(
                    f,
                    "the client sent a {kind} frame the recording does not expect"
                )
            }
            ReplayError::Mismatch {
                kind,
                field,
                expected,
                found,
            } => write!(
                f,
                "a {kind} frame differs at {field}: recorded {expected}, live {found}"
            ),
            ReplayError::Conflict { placeholder } => {
                write!(f, "{placeholder} was learned with two different values")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

/// The replay of one process of a recording.
pub struct Replayer {
    protocol: Protocol,
    process: u32,
    /// The process's records, actions excluded, in recorded order.
    steps: Vec<Step>,
    /// The first step not yet given back or passed; release stops at a client step not matched.
    cursor: usize,
    /// Whether the recorded exit was given back; nothing comes after it.
    exited: bool,
    substitutions: Substitutions,
}

/// One record of the process, and whether it was given back or matched.
struct Step {
    record: Record,
    /// The kind of what the client sends, for a client frame or line.
    client_kind: Option<String>,
    consumed: bool,
}

impl Step {
    /// Whether the replay waits on the client for this step: a frame, a line or the end of stdin.
    fn is_the_clients(&self) -> bool {
        self.client_kind.is_some() || matches!(self.record, Record::StdinClosed { .. })
    }
}

impl Replayer {
    /// Starts replaying `process`, given the live arguments (without the program's name). Returns
    /// the replayer and what the agent gives back before any client input.
    pub fn new(
        recording: &Recording,
        process: u32,
        live_args: &[String],
        places: LivePlaces,
    ) -> Result<(Replayer, Vec<Output>), ReplayError> {
        let protocol = recording.header.protocol;
        let steps: Vec<Step> = recording
            .records
            .iter()
            .filter(|record| record.process() == Some(process))
            .map(|record| Step {
                client_kind: client_message(record).map(|message| kind_of(protocol, &message)),
                record: record.clone(),
                consumed: false,
            })
            .collect();
        if steps.is_empty() {
            return Err(ReplayError::NoSuchProcess(process));
        }
        let mut substitutions = Substitutions {
            ids: HashMap::new(),
            places,
        };
        let spawn_argv = steps.iter().find_map(|step| match &step.record {
            Record::Spawn { argv, .. } => Some(argv),
            _ => None,
        });
        if let Some(argv) = spawn_argv {
            substitutions.learn_args(argv, live_args)?;
        }
        let mut replayer = Replayer {
            protocol,
            process,
            steps,
            cursor: 0,
            exited: false,
            substitutions,
        };
        let outputs = replayer.release();
        Ok((replayer, outputs))
    }

    /// One line the live client wrote, without its newline. Blank lines are ignored.
    pub fn client_line(&mut self, line: &str) -> Result<Vec<Output>, ReplayError> {
        if line.trim().is_empty() {
            return Ok(Vec::new());
        }
        let frame: Option<Value> = serde_json::from_str(line).ok();
        let live = match &frame {
            Some(frame) => Message::Frame(frame),
            None => Message::Line(line),
        };
        let kind = kind_of(self.protocol, &live);
        let Some(index) = self
            .steps
            .iter()
            .position(|step| !step.consumed && step.client_kind.as_deref() == Some(kind.as_str()))
        else {
            return Err(ReplayError::Unexpected { kind });
        };
        let Some(recorded) = client_message(&self.steps[index].record) else {
            return Err(ReplayError::Unexpected { kind });
        };
        if let (Message::Frame(recorded), Message::Frame(live)) = (&recorded, &live) {
            self.substitutions.learn(recorded, live)?;
        }
        let recorded_fields = key_fields(self.protocol, &kind, &recorded);
        let live_fields = key_fields(self.protocol, &kind, &live);
        for ((field, recorded), (_, live)) in recorded_fields.into_iter().zip(live_fields) {
            let expected = recorded.map(|value| self.substitutions.value(&value));
            if expected != live {
                return Err(ReplayError::Mismatch {
                    kind,
                    field: field.to_owned(),
                    expected: show(expected.as_ref()),
                    found: show(live.as_ref()),
                });
            }
        }
        self.steps[index].consumed = true;
        Ok(self.release())
    }

    /// The live client closed the agent's stdin.
    pub fn stdin_closed(&mut self) -> Result<Vec<Output>, ReplayError> {
        let recorded = self
            .steps
            .iter_mut()
            .find(|step| !step.consumed && matches!(step.record, Record::StdinClosed { .. }));
        if let Some(step) = recorded {
            step.consumed = true;
        }
        Ok(self.release())
    }

    pub fn status(&self) -> Status {
        let consumed = self.steps.iter().filter(|step| step.consumed).count();
        let waiting_for = self
            .steps
            .iter()
            .find(|step| !step.consumed && step.is_the_clients())
            .map(|step| {
                step.client_kind
                    .clone()
                    .unwrap_or_else(|| "stdin_closed".to_owned())
            });
        Status {
            process: self.process,
            consumed,
            total: self.steps.len(),
            complete: consumed == self.steps.len(),
            waiting_for,
        }
    }

    /// Gives back the agent's records from the cursor on, up to the first client step not yet
    /// matched, or through the exit.
    fn release(&mut self) -> Vec<Output> {
        let mut outputs = Vec::new();
        while !self.exited {
            let Some(step) = self.steps.get_mut(self.cursor) else {
                break;
            };
            if step.is_the_clients() {
                if !step.consumed {
                    break;
                }
            } else {
                outputs.extend(self.substitutions.output(&step.record));
                self.exited = matches!(step.record, Record::Exit { .. });
                step.consumed = true;
            }
            self.cursor += 1;
        }
        outputs
    }
}

/// Replays every process of a recording with a client of its own: fresh ids in place of the
/// recorded ones (numbers for ACP's `id`, strings elsewhere), frames sent in the recorded order.
/// Checks that every agent record comes back, with the fresh ids in place, and that every
/// process completes. This is what shows a driver other than the fork's can use a recording. A
/// recording without a process shows nothing, and is an error.
pub fn with_another_client(recording: &Recording) -> Result<(), String> {
    let mut processes: Vec<u32> = Vec::new();
    for process in recording.records.iter().filter_map(Record::process) {
        if !processes.contains(&process) {
            processes.push(process);
        }
    }
    if processes.is_empty() {
        return Err("the recording has no process, so it shows nothing".to_owned());
    }
    for process in processes {
        replay_process_with_another_client(recording, process)?;
    }
    Ok(())
}

/// The folders the other client works in: unlike any recorded one.
const ANOTHER_WORKSPACE: &str = "/another/workspace";
const ANOTHER_HOME: &str = "/another/home";
/// ACP ids the other client chooses start here, far from the small numbers clients use.
const ANOTHER_FIRST_NUMBER: u64 = 1000;

fn replay_process_with_another_client(recording: &Recording, process: u32) -> Result<(), String> {
    let records: Vec<(usize, &Record)> = recording
        .records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.process() == Some(process))
        .collect();
    let at = |index: usize, problem: &dyn fmt::Display| {
        format!(
            "process {process}, record {index} (line {}): {problem}",
            index + 2
        )
    };
    let fresh = Substitutions {
        ids: fresh_ids(recording.header.protocol, &records),
        places: LivePlaces {
            workspace: Some(ANOTHER_WORKSPACE.to_owned()),
            home: Some(ANOTHER_HOME.to_owned()),
        },
    };
    let live_args: Vec<String> = records
        .iter()
        .find_map(|(_, record)| match record {
            Record::Spawn { argv, .. } => {
                Some(argv.iter().skip(1).map(|arg| fresh.text(arg)).collect())
            }
            _ => None,
        })
        .unwrap_or_default();
    let first = records.first().map_or(0, |(index, _)| *index);
    let (mut replayer, mut released) =
        Replayer::new(recording, process, &live_args, fresh.places.clone())
            .map_err(|error| at(first, &error))?;
    let mut expected: Vec<(usize, Output)> = Vec::new();
    for (index, record) in &records {
        let sent = match record {
            Record::Frame {
                from: Side::Client,
                json,
                ..
            } => replayer.client_line(&fresh.value(json).to_string()),
            Record::Line {
                from: Stream::Client,
                text,
                ..
            } => replayer.client_line(&fresh.text(text)),
            Record::StdinClosed { .. } => replayer.stdin_closed(),
            agent => {
                expected.extend(fresh.output(agent).map(|output| (*index, output)));
                continue;
            }
        };
        released.extend(sent.map_err(|error| at(*index, &error))?);
    }
    for (position, (index, output)) in expected.iter().enumerate() {
        match released.get(position) {
            Some(given) if given == output => {}
            Some(given) => {
                return Err(at(
                    *index,
                    &format!(
                        "the agent gave back {} where {} was recorded, or with other values",
                        output_kind(given),
                        output_kind(output)
                    ),
                ));
            }
            None => return Err(at(*index, &"this agent record was never given back")),
        }
    }
    if released.len() > expected.len() {
        return Err(format!(
            "process {process}: the agent gave back {} outputs, {} were recorded",
            released.len(),
            expected.len()
        ));
    }
    // A process its client killed, as the fork's ACP runtime does when it closes a session, has
    // no recorded exit: consuming every record is then its end.
    let exit_recorded = records
        .iter()
        .any(|(_, record)| matches!(record, Record::Exit { .. }));
    let exit_released = released
        .iter()
        .any(|output| matches!(output, Output::Exit { .. }));
    if exit_recorded && !exit_released {
        return Err(format!(
            "process {process}: the replay never reached an exit"
        ));
    }
    let status = replayer.status();
    if !status.complete {
        return Err(format!(
            "process {process}: {} of {} records consumed, waiting for {}",
            status.consumed,
            status.total,
            status.waiting_for.as_deref().unwrap_or("nothing")
        ));
    }
    Ok(())
}

/// A fresh value for every placeholder the client provides, in its frames or the arguments: a
/// number where ACP's client puts it in a request's `id`, `fresh-N` elsewhere.
fn fresh_ids(protocol: Protocol, records: &[(usize, &Record)]) -> HashMap<u32, Value> {
    let mut provided = BTreeSet::new();
    let mut numeric = BTreeSet::new();
    for (_, record) in records {
        match record {
            Record::Spawn { argv, .. } => {
                for arg in argv {
                    provided.extend(ids_in(arg));
                }
            }
            Record::Frame {
                from: Side::Client,
                json,
                ..
            } => {
                for_each_string(json, &mut |text| provided.extend(ids_in(text)));
                if protocol == Protocol::AcpJsonRpc
                    && let Some(id) = json.get("id").and_then(Value::as_str)
                    && let [Piece::Id(number)] = pieces(id)[..]
                {
                    numeric.insert(number);
                }
            }
            Record::Line {
                from: Stream::Client,
                text,
                ..
            } => provided.extend(ids_in(text)),
            _ => {}
        }
    }
    provided
        .into_iter()
        .map(|number| {
            let value = if numeric.contains(&number) {
                Value::from(ANOTHER_FIRST_NUMBER + u64::from(number))
            } else {
                Value::String(format!("fresh-{number}"))
            };
            (number, value)
        })
        .collect()
}

fn output_kind(output: &Output) -> &'static str {
    match output {
        Output::Stdout(_) => "a stdout line",
        Output::Stderr(_) => "a stderr line",
        Output::Exit { .. } => "an exit",
    }
}

/// What the client sent: a JSON frame, or a line that is not JSON.
enum Message<'a> {
    Frame(&'a Value),
    Line(&'a str),
}

/// The client's message a record holds, if it holds one.
fn client_message(record: &Record) -> Option<Message<'_>> {
    match record {
        Record::Frame {
            from: Side::Client,
            json,
            ..
        } => Some(Message::Frame(json)),
        Record::Line {
            from: Stream::Client,
            text,
            ..
        } => Some(Message::Line(text)),
        _ => None,
    }
}

/// A client message's kind, as the module's table names it.
fn kind_of(protocol: Protocol, message: &Message) -> String {
    let frame = match message {
        Message::Line(_) => return "line".to_owned(),
        Message::Frame(frame) => frame,
    };
    match protocol {
        Protocol::ClaudeStreamJson => match frame.get("type").and_then(Value::as_str) {
            Some("control_request") => {
                let subtype = frame
                    .pointer("/request/subtype")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                format!("control_request:{subtype}")
            }
            Some(frame_type) => frame_type.to_owned(),
            None => "untyped".to_owned(),
        },
        Protocol::AcpJsonRpc => {
            let method = frame.get("method").and_then(Value::as_str);
            match (method, frame.get("id").is_some()) {
                (Some(method), true) => format!("request:{method}"),
                (Some(method), false) => format!("notify:{method}"),
                (None, true) => "response".to_owned(),
                (None, false) => "untyped".to_owned(),
            }
        }
    }
}

/// The key fields of a message of `kind`, by name, each with its value when present.
fn key_fields(
    protocol: Protocol,
    kind: &str,
    message: &Message,
) -> Vec<(&'static str, Option<Value>)> {
    let frame = match message {
        Message::Line(text) => return vec![("text", Some(Value::String((*text).to_owned())))],
        Message::Frame(frame) => frame,
    };
    let at = |field: &'static str| {
        (
            field,
            frame
                .pointer(&format!("/{}", field.replace('.', "/")))
                .cloned(),
        )
    };
    match (protocol, kind) {
        (Protocol::ClaudeStreamJson, "user") => vec![(
            "message.content",
            Some(Value::String(joined_text(
                frame.pointer("/message/content"),
            ))),
        )],
        (Protocol::ClaudeStreamJson, "control_response") => vec![
            at("response.request_id"),
            at("response.subtype"),
            at("response.response.behavior"),
            at("response.response.updatedInput.answers"),
        ],
        (Protocol::ClaudeStreamJson, kind) if kind.starts_with("control_request:") => {
            vec![at("request.mode")]
        }
        (Protocol::AcpJsonRpc, "request:session/prompt") => vec![(
            "params.prompt",
            Some(Value::String(joined_text(frame.pointer("/params/prompt")))),
        )],
        (Protocol::AcpJsonRpc, "response") => vec![
            at("id"),
            at("result.outcome"),
            ("error", Some(Value::Bool(frame.get("error").is_some()))),
        ],
        _ => Vec::new(),
    }
}

/// A content's text: a string as it is, or the `text` of its parts joined with newlines.
fn joined_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A key field's value for a message: its JSON, or `absent`.
fn show(value: Option<&Value>) -> String {
    value.map_or_else(|| "absent".to_owned(), Value::to_string)
}

/// The live values that replace a recording's placeholders: ids as they are learned, and the
/// live folders.
struct Substitutions {
    ids: HashMap<u32, Value>,
    places: LivePlaces,
}

impl Substitutions {
    /// A recorded value with its placeholders replaced; an id that is a whole string keeps its
    /// live type.
    fn value(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => {
                if let [Piece::Id(number)] = pieces(text)[..]
                    && let Some(live) = self.ids.get(&number)
                {
                    return live.clone();
                }
                Value::String(self.text(text))
            }
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.value(item)).collect())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, child)| (key.clone(), self.value(child)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// A recorded string with its placeholders replaced by text; an unlearned one stays.
    fn text(&self, text: &str) -> String {
        if !text.contains('<') {
            return text.to_owned();
        }
        self.join(&pieces(text))
    }

    fn join(&self, pieces: &[Piece]) -> String {
        let mut joined = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => joined.push_str(text),
                Piece::Id(number) => match self.ids.get(number) {
                    Some(Value::String(live)) => joined.push_str(live),
                    Some(live) => joined.push_str(&live.to_string()),
                    None => joined.push_str(&format!("<id:{number}>")),
                },
                Piece::Workspace => {
                    joined.push_str(self.places.workspace.as_deref().unwrap_or("<workspace>"));
                }
                Piece::Home => joined.push_str(self.places.home.as_deref().unwrap_or("<home>")),
            }
        }
        joined
    }

    /// What an agent's record gives back, with its placeholders replaced.
    fn output(&self, record: &Record) -> Option<Output> {
        match record {
            Record::Frame {
                from: Side::Agent,
                json,
                ..
            } => Some(Output::Stdout(self.value(json).to_string())),
            Record::Line {
                from: Stream::Agent,
                text,
                ..
            } => Some(Output::Stdout(self.text(text))),
            Record::Line {
                from: Stream::Stderr,
                text,
                ..
            } => Some(Output::Stderr(self.text(text))),
            Record::Exit { code, signal, .. } => Some(Output::Exit {
                code: *code,
                signal: *signal,
            }),
            _ => None,
        }
    }

    /// Learns the ids a recorded client frame holds from the live frame, place by place.
    fn learn(&mut self, recorded: &Value, live: &Value) -> Result<(), ReplayError> {
        match (recorded, live) {
            (Value::String(text), _) => self.learn_text(text, live),
            (Value::Object(fields), Value::Object(live_fields)) => {
                for (key, child) in fields {
                    if let Some(live_child) = live_fields.get(key) {
                        self.learn(child, live_child)?;
                    }
                }
                Ok(())
            }
            (Value::Array(items), Value::Array(live_items)) => {
                for (item, live_item) in items.iter().zip(live_items) {
                    self.learn(item, live_item)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Learns from one recorded string: the whole live value when the string is one id, or the
    /// live text between the same surroundings when it holds one id among other text.
    fn learn_text(&mut self, recorded: &str, live: &Value) -> Result<(), ReplayError> {
        if !recorded.contains("<id:") {
            return Ok(());
        }
        let pieces = pieces(recorded);
        let mut ids = pieces
            .iter()
            .enumerate()
            .filter_map(|(index, piece)| match piece {
                Piece::Id(number) => Some((index, *number)),
                _ => None,
            });
        let (Some((index, number)), None) = (ids.next(), ids.next()) else {
            return Ok(());
        };
        if pieces.len() == 1 {
            return self.learn_id(number, live.clone());
        }
        let Some(live_text) = live.as_str() else {
            return Ok(());
        };
        let before = self.join(&pieces[..index]);
        let after = self.join(&pieces[index + 1..]);
        let between = live_text
            .strip_prefix(before.as_str())
            .and_then(|rest| rest.strip_suffix(after.as_str()));
        match between {
            Some(between) => self.learn_id(number, Value::String(between.to_owned())),
            None => Ok(()),
        }
    }

    /// Learns the ids in the recorded arguments, by flag: `--flag=<id:N>` from the live
    /// `--flag=…`, and a flag followed by `<id:N>` from the live argument after the same flag.
    fn learn_args(
        &mut self,
        recorded_argv: &[String],
        live_args: &[String],
    ) -> Result<(), ReplayError> {
        let recorded_args = recorded_argv.get(1..).unwrap_or_default();
        for (index, arg) in recorded_args.iter().enumerate() {
            if !arg.contains("<id:") {
                continue;
            }
            let joined_flag = arg
                .split_once('=')
                .map(|(flag, _)| flag)
                .filter(|flag| flag.starts_with('-'));
            let live = match joined_flag {
                Some(flag) => live_args.iter().find(|live| {
                    live.strip_prefix(flag)
                        .is_some_and(|rest| rest.starts_with('='))
                }),
                None => index
                    .checked_sub(1)
                    .map(|previous| &recorded_args[previous])
                    .filter(|flag| flag.starts_with('-'))
                    .and_then(|flag| live_args.iter().position(|live| live == flag))
                    .and_then(|position| live_args.get(position + 1)),
            };
            if let Some(live) = live {
                self.learn_text(arg, &Value::String(live.clone()))?;
            }
        }
        Ok(())
    }

    fn learn_id(&mut self, number: u32, live: Value) -> Result<(), ReplayError> {
        match self.ids.get(&number) {
            Some(known) if *known != live => Err(ReplayError::Conflict {
                placeholder: format!("<id:{number}>"),
            }),
            Some(_) => Ok(()),
            None => {
                self.ids.insert(number, live);
                Ok(())
            }
        }
    }
}

/// A piece of a recorded string: text, or one of the placeholders replay replaces.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece<'a> {
    Text(&'a str),
    Id(u32),
    Workspace,
    Home,
}

/// Splits a recorded string into text and placeholders; `<masked:…>` and any other `<…>` stay
/// text.
fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut text_start = 0;
    let mut search_from = 0;
    while let Some(offset) = text[search_from..].find('<') {
        let at = search_from + offset;
        match placeholder_at(&text[at..]) {
            Some((piece, length)) => {
                if text_start < at {
                    pieces.push(Piece::Text(&text[text_start..at]));
                }
                pieces.push(piece);
                text_start = at + length;
                search_from = text_start;
            }
            None => search_from = at + 1,
        }
    }
    if text_start < text.len() {
        pieces.push(Piece::Text(&text[text_start..]));
    }
    pieces
}

/// The placeholder `text` starts with, and its length.
fn placeholder_at(text: &str) -> Option<(Piece<'static>, usize)> {
    if text.starts_with("<workspace>") {
        return Some((Piece::Workspace, "<workspace>".len()));
    }
    if text.starts_with("<home>") {
        return Some((Piece::Home, "<home>".len()));
    }
    let digits = text.strip_prefix("<id:")?;
    let length = digits.bytes().take_while(u8::is_ascii_digit).count();
    if length == 0 || digits.as_bytes().get(length) != Some(&b'>') {
        return None;
    }
    let number = digits[..length].parse().ok()?;
    Some((Piece::Id(number), "<id:".len() + length + 1))
}

/// The id numbers a string holds.
fn ids_in(text: &str) -> Vec<u32> {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Id(number) => Some(number),
            _ => None,
        })
        .collect()
}

fn for_each_string(value: &Value, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => items.iter().for_each(|item| for_each_string(item, visit)),
        Value::Object(fields) => fields
            .values()
            .for_each(|child| for_each_string(child, visit)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_split_from_text_and_other_brackets_stay() {
        assert_eq!(
            pieces("<home>/a/<id:12>.jsonl <masked:token> <id:x> <"),
            [
                Piece::Home,
                Piece::Text("/a/"),
                Piece::Id(12),
                Piece::Text(".jsonl <masked:token> <id:x> <"),
            ]
        );
    }

    #[test]
    fn an_id_inside_a_longer_string_is_learned_between_its_surroundings() {
        let mut substitutions = Substitutions {
            ids: HashMap::new(),
            places: LivePlaces::default(),
        };
        substitutions
            .learn_text("run-<id:3>-end", &Value::String("run-abc-end".to_owned()))
            .unwrap();
        assert_eq!(substitutions.ids[&3], "abc");
        substitutions
            .learn_args(
                &[
                    "claude".to_owned(),
                    "--resume".to_owned(),
                    "<id:4>".to_owned(),
                ],
                &["-p".to_owned(), "--resume".to_owned(), "live-4".to_owned()],
            )
            .unwrap();
        assert_eq!(substitutions.ids[&4], "live-4");
    }
}
