//! The script: the answers the fake model gives, turn by turn, and how fast.
//!
//! A script is a JSON file, versioned by its `version` field:
//!
//! ```json
//! {
//!   "version": 1,
//!   "pace": { "first_token_ms": 300, "tokens_per_second": 60, "chunk_bytes": 16 },
//!   "side_answer": "A short title",
//!   "turns": [
//!     { "segments": [
//!       { "text": "Let me look.", "tool": { "role": "shell", "command": "ls" } },
//!       { "text": "The answer, in Markdown." }
//!     ] }
//!   ]
//! }
//! ```
//!
//! A turn is a list of segments. Each segment's text is streamed; a segment with a tool ends with
//! that tool's call, and the next segment answers once the client sends the tool's result back.
//! The last segment of a turn has no tool, so that every turn ends. In the server's chat mode, a
//! turn is one answer: its segments' texts joined ([`Script::chat_text`]), without the tool calls.
//! A request that is not a turn's (a client's title or summary request) gets `side_answer`, shaped
//! as the JSON it asks for when it asks for structured output (the routes in [`crate::server`]).

use serde::Deserialize;
use std::fmt;

/// The version this code reads. A script of another version is refused.
pub const SCRIPT_VERSION: u32 = 1;

/// Just the `version` field, to refuse another version before reading the rest.
#[derive(Deserialize)]
struct VersionOnly {
    version: u32,
}

/// A parsed, validated script. Its fields are public to read: build one only with
/// [`Script::from_json`], whose checks the rest of the code relies on.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub version: u32,
    #[serde(default)]
    pub pace: Pace,
    pub side_answer: String,
    pub turns: Vec<Turn>,
}

/// How fast the answer flows. A model's first token comes after `first_token_ms`; then text flows
/// at `tokens_per_second`, counted as four bytes a token, in pieces of about `chunk_bytes` bytes,
/// never splitting a character.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Pace {
    pub first_token_ms: u64,
    pub tokens_per_second: f64,
    pub chunk_bytes: usize,
}

impl Default for Pace {
    /// 300 ms to the first token and 60 tokens a second: a frontier model's usual pace.
    fn default() -> Self {
        Pace {
            first_token_ms: 300,
            tokens_per_second: 60.0,
            chunk_bytes: 16,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub text: String,
    #[serde(default)]
    pub tool: Option<ToolCall>,
}

/// A tool call, named by what it does rather than by any client's tool name: [`crate::tools`] maps
/// it to the tool the client offers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase", deny_unknown_fields)]
pub enum ToolCall {
    /// Run a shell command.
    Shell { command: String },
    /// Read a file.
    Read { path: String },
}

/// Where a request stands in the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub turn: usize,
    pub segment: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScriptError {
    /// Not JSON, or not the script's shape; serde's message.
    Malformed(String),
    /// The `version` is not [`SCRIPT_VERSION`].
    Version(u32),
    /// The script has no turn.
    NoTurns,
    /// A turn has no segment (its index).
    EmptyTurn(usize),
    /// A turn's last segment calls a tool, so the turn would never end (its index).
    TurnEndsWithTool(usize),
    /// The pace cannot stream: `tokens_per_second` not above zero, or `chunk_bytes` zero.
    Pace,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::Malformed(message) => write!(f, "the script is malformed: {message}"),
            ScriptError::Version(version) => write!(
                f,
                "script version {version} is not supported; this code reads version {SCRIPT_VERSION}"
            ),
            ScriptError::NoTurns => write!(f, "the script has no turn"),
            ScriptError::EmptyTurn(turn) => write!(f, "turn {turn} has no segment"),
            ScriptError::TurnEndsWithTool(turn) => {
                write!(
                    f,
                    "turn {turn} ends with a tool call, so it would never end"
                )
            }
            ScriptError::Pace => write!(
                f,
                "the pace cannot stream: tokens_per_second must be above zero and chunk_bytes at least 1"
            ),
        }
    }
}

impl std::error::Error for ScriptError {}

impl Script {
    /// Parses and validates a script.
    pub fn from_json(text: &str) -> Result<Script, ScriptError> {
        // The version is read alone first: a script of another version may have another shape, and
        // "wrong version" tells the author more than "unknown field".
        if let Ok(VersionOnly { version }) = serde_json::from_str(text)
            && version != SCRIPT_VERSION
        {
            return Err(ScriptError::Version(version));
        }
        let script: Script = serde_json::from_str(text)
            .map_err(|error| ScriptError::Malformed(error.to_string()))?;
        script.validate()?;
        Ok(script)
    }

    fn validate(&self) -> Result<(), ScriptError> {
        if self.turns.is_empty() {
            return Err(ScriptError::NoTurns);
        }
        for (index, turn) in self.turns.iter().enumerate() {
            match turn.segments.last() {
                None => return Err(ScriptError::EmptyTurn(index)),
                Some(last) if last.tool.is_some() => {
                    return Err(ScriptError::TurnEndsWithTool(index));
                }
                Some(_) => {}
            }
        }
        let speed = self.pace.tokens_per_second;
        if speed.is_nan() || speed <= 0.0 || self.pace.chunk_bytes == 0 {
            return Err(ScriptError::Pace);
        }
        Ok(())
    }

    /// A turn's texts as one chat answer: its segments' texts, in order, each a paragraph apart
    /// (joined by a blank line), its tool calls left out. The turn wraps around the script as in
    /// [`Script::segment`].
    pub fn chat_text(&self, turn: usize) -> String {
        let Some(turn) = self.turns.get(turn % self.turns.len().max(1)) else {
            return String::new();
        };
        turn.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// The segment for a position. The turn wraps around the script (`turn % turns`); a segment
    /// past the turn's last gives the last, so that a client sending extra tool results still gets
    /// the turn's end. A script that [`Script::from_json`] did not validate may have no turn or an
    /// empty one: then the segment is empty (no text, no tool).
    pub fn segment(&self, position: Position) -> &Segment {
        static EMPTY: Segment = Segment {
            text: String::new(),
            tool: None,
        };
        let Some(turn) = self.turns.get(position.turn % self.turns.len().max(1)) else {
            return &EMPTY;
        };
        let last = turn.segments.len().saturating_sub(1);
        turn.segments
            .get(position.segment.min(last))
            .unwrap_or(&EMPTY)
    }
}
