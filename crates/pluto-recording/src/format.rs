//! The recording's file: JSON Lines, one record per line.
//!
//! The first line is a [`Header`]; it names the format and its version, and a reader refuses a
//! format or a version it does not know. Every other line is a [`Record`]:
//!
//! ```text
//! {"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"turn","meta":{}}
//! {"kind":"spawn","process":0,"t_ms":0,"argv":["claude","--output-format","stream-json","--session-id=<id:1>"]}
//! {"kind":"frame","process":0,"t_ms":12,"from":"client","json":{"type":"user","session_id":"<id:1>"}}
//! {"kind":"frame","process":0,"t_ms":40,"from":"agent","json":{"type":"system","subtype":"init"},"masked":true}
//! {"kind":"line","process":0,"t_ms":41,"from":"stderr","text":"warning"}
//! {"kind":"action","name":"approve","detail":{}}
//! {"kind":"stdin_closed","process":0,"t_ms":900}
//! {"kind":"exit","process":0,"t_ms":950,"code":0}
//! ```
//!
//! Placeholders stand for what must not be committed or differs at each run: `<workspace>` and
//! `<home>` for the project's and the home's folders, `<id:N>` for the N-th id the client chose
//! in a process, `<masked:KIND>` for a value masked away (see [`crate::mask`]).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The format's name, in every header.
pub const FORMAT: &str = "pluto-agent-recording";

/// The format's version. A change that an older reader would misread raises it, and the reader
/// learns to migrate the older version.
pub const VERSION: u32 = 1;

/// The agent a recording was made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    Claude,
    Grok,
}

/// The protocol on the agent's stdio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    /// Claude Code's `--input-format stream-json --output-format stream-json`.
    ClaudeStreamJson,
    /// The Agent Client Protocol: JSON-RPC 2.0, one message per line.
    AcpJsonRpc,
}

impl Agent {
    /// The protocol this agent speaks on its stdio.
    pub fn protocol(self) -> Protocol {
        match self {
            Agent::Claude => Protocol::ClaudeStreamJson,
            Agent::Grok => Protocol::AcpJsonRpc,
        }
    }
}

/// The first line of a recording.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub format: String,
    pub version: u32,
    pub agent: Agent,
    pub protocol: Protocol,
    /// The day of the recording, `YYYY-MM-DD`.
    pub recorded_on: String,
    /// The scenario's name, such as `turn` or `approval`.
    pub scenario: String,
    /// What the recording's maker knew: the SDK's version, the CLI's, the harness's options.
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
}

impl Header {
    pub fn new(agent: Agent, scenario: &str, recorded_on: &str) -> Header {
        Header {
            format: FORMAT.to_owned(),
            version: VERSION,
            agent,
            protocol: agent.protocol(),
            recorded_on: recorded_on.to_owned(),
            scenario: scenario.to_owned(),
            meta: BTreeMap::new(),
        }
    }
}

/// Which end of the stdio a frame came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Written by the server to the agent's stdin.
    Client,
    /// Written by the agent to its stdout.
    Agent,
}

/// Where a line that is not JSON came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    Client,
    Agent,
    Stderr,
}

/// One line of a recording after its header. `process` counts the processes spawned during the
/// scenario, from 0; `t_ms` is the time since that process was spawned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Record {
    /// The agent's CLI was started, with these arguments; the first is the program's file name
    /// without its folder.
    Spawn {
        process: u32,
        t_ms: u64,
        argv: Vec<String>,
    },
    /// One JSON message on the agent's stdin or stdout.
    Frame {
        process: u32,
        t_ms: u64,
        from: Side,
        json: Value,
        /// Whether masking changed the frame, beyond placeholders for ids and folders.
        #[serde(default, skip_serializing_if = "is_false")]
        masked: bool,
    },
    /// A line that is not JSON, or anything the agent wrote on its stderr.
    Line {
        process: u32,
        t_ms: u64,
        from: Stream,
        text: String,
        #[serde(default, skip_serializing_if = "is_false")]
        masked: bool,
    },
    /// What the scenario's script did, such as approving or interrupting; written by the
    /// harness, not the recorder, so it carries no process and no time.
    Action {
        name: String,
        #[serde(default)]
        detail: Value,
    },
    /// The server closed the agent's stdin.
    StdinClosed { process: u32, t_ms: u64 },
    /// The agent's process ended, with an exit code or killed by a signal.
    Exit {
        process: u32,
        t_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Record {
    /// The process a record belongs to; actions belong to none.
    pub fn process(&self) -> Option<u32> {
        match self {
            Record::Spawn { process, .. }
            | Record::Frame { process, .. }
            | Record::Line { process, .. }
            | Record::StdinClosed { process, .. }
            | Record::Exit { process, .. } => Some(*process),
            Record::Action { .. } => None,
        }
    }
}

/// A whole recording, read from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    pub header: Header,
    pub records: Vec<Record>,
}

/// Why a recording could not be read. Line numbers count from 1, the header being line 1.
#[derive(Debug, PartialEq)]
pub enum FormatError {
    Empty,
    NotARecording { found: String },
    UnknownVersion { found: u32 },
    BadLine { line: usize, reason: String },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Empty => write!(f, "the file is empty: no header"),
            FormatError::NotARecording { found } => {
                write!(
                    f,
                    "not a recording: the header's format is {found:?}, not {FORMAT:?}"
                )
            }
            FormatError::UnknownVersion { found } => write!(
                f,
                "recording version {found} is unknown to this reader, which reads version {VERSION}"
            ),
            FormatError::BadLine { line, reason } => write!(f, "line {line}: {reason}"),
        }
    }
}

impl std::error::Error for FormatError {}

/// Reads a whole recording. Blank lines are skipped; any other line must parse.
pub fn read(text: &str) -> Result<Recording, FormatError> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (_, first) = lines.next().ok_or(FormatError::Empty)?;
    let header = read_header(first)?;
    let records = lines
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| FormatError::BadLine {
                line: index + 1,
                reason: error.to_string(),
            })
        })
        .collect::<Result<Vec<Record>, _>>()?;
    Ok(Recording { header, records })
}

/// Reads the header line, checking the format and the version before the other fields, so that
/// a newer file says it is newer rather than failing on a field it added.
pub fn read_header(line: &str) -> Result<Header, FormatError> {
    let bad = |reason: String| FormatError::BadLine { line: 1, reason };
    let value: Value = serde_json::from_str(line).map_err(|error| bad(error.to_string()))?;
    let format = value.get("format").and_then(Value::as_str).unwrap_or("");
    if format != FORMAT {
        return Err(FormatError::NotARecording {
            found: format.to_owned(),
        });
    }
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| bad("the header has no version".to_owned()))?;
    if version != u64::from(VERSION) {
        return Err(FormatError::UnknownVersion {
            found: u32::try_from(version).unwrap_or(u32::MAX),
        });
    }
    serde_json::from_value(value).map_err(|error| bad(error.to_string()))
}

/// One line of the file, without its newline.
pub fn header_line(header: &Header) -> String {
    serde_json::to_string(header).expect("a header always serialises")
}

/// One line of the file, without its newline.
pub fn record_line(record: &Record) -> String {
    serde_json::to_string(record).expect("a record always serialises")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Recording {
        let mut header = Header::new(Agent::Claude, "turn", "2026-11-12");
        header
            .meta
            .insert("sdk".to_owned(), "claude-agent-sdk 0.3.276".to_owned());
        Recording {
            header,
            records: vec![
                Record::Spawn {
                    process: 0,
                    t_ms: 0,
                    argv: vec!["claude".to_owned(), "--session-id=<id:1>".to_owned()],
                },
                Record::Frame {
                    process: 0,
                    t_ms: 5,
                    from: Side::Client,
                    json: json!({"type": "user", "z": 1, "a": 2}),
                    masked: false,
                },
                Record::Line {
                    process: 0,
                    t_ms: 6,
                    from: Stream::Stderr,
                    text: "warn".to_owned(),
                    masked: true,
                },
                Record::Action {
                    name: "interrupt".to_owned(),
                    detail: json!({"after": "tool_use"}),
                },
                Record::StdinClosed {
                    process: 0,
                    t_ms: 9,
                },
                Record::Exit {
                    process: 0,
                    t_ms: 10,
                    code: None,
                    signal: Some(15),
                },
            ],
        }
    }

    fn write(recording: &Recording) -> String {
        let mut text = header_line(&recording.header);
        for record in &recording.records {
            text.push('\n');
            text.push_str(&record_line(record));
        }
        text.push('\n');
        text
    }

    #[test]
    fn a_recording_reads_back_as_written() {
        let recording = sample();
        assert_eq!(read(&write(&recording)).unwrap(), recording);
    }

    #[test]
    fn frames_keep_their_keys_order() {
        let text = write(&sample());
        assert!(
            text.contains(r#""json":{"type":"user","z":1,"a":2}"#),
            "{text}"
        );
    }

    #[test]
    fn an_unmasked_frame_omits_the_flag() {
        let line = record_line(&sample().records[1]);
        assert!(!line.contains("masked"), "{line}");
    }

    #[test]
    fn a_newer_version_is_refused_plainly() {
        let line =
            r#"{"format":"pluto-agent-recording","version":2,"agent":"claude","new_field":[]}"#;
        let error = read(line).unwrap_err();
        assert_eq!(error, FormatError::UnknownVersion { found: 2 });
        assert!(error.to_string().contains("version 2"));
    }

    #[test]
    fn another_format_is_refused() {
        let error = read(r#"{"format":"asciicast","version":1}"#).unwrap_err();
        assert!(matches!(error, FormatError::NotARecording { .. }));
        assert_eq!(read("").unwrap_err(), FormatError::Empty);
    }

    #[test]
    fn a_bad_line_names_its_number() {
        let text = format!("{}\n\nnot json\n", header_line(&sample().header));
        assert!(matches!(
            read(&text),
            Err(FormatError::BadLine { line: 3, .. })
        ));
    }
}
