//! The answer encoded for each API.
//!
//! Each encoder takes one [`Reply`] and gives either the whole response's JSON, for a request that
//! did not ask for a stream, or the stream's pieces in order. A streamed answer's text is cut into
//! pieces of about [`Reply::chunk_bytes`] bytes, never inside a character, so that every API
//! carries the same text in the same pieces.
//!
//! Tokens are counted as four bytes each, rounded up, for every API: the usage a client reads is
//! `input_tokens` (from the request's bytes) and the reply's own bytes divided the same way.

pub mod anthropic;
pub mod chat_completions;
pub mod ollama;
pub mod responses;

use crate::json;
use crate::tools::MappedTool;
use serde_json::Value;

/// One answer, ready to encode.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub model: String,
    /// The response's id, unique per request.
    pub id: String,
    pub text: String,
    pub tool: Option<MappedTool>,
    pub input_tokens: u64,
    pub chunk_bytes: usize,
}

/// One piece of a stream, as written on the wire: an SSE event (`event: …\ndata: …\n\n`, or
/// `data: …\n\n`), or an NDJSON line. `text_bytes` is how many bytes of the answer's text or of
/// the tool's arguments it carries, which the server's pacing spends time on; framing carries
/// none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireEvent {
    pub data: String,
    pub text_bytes: usize,
}

/// Four bytes a token, rounded up; zero bytes are zero tokens.
pub fn tokens(bytes: usize) -> u64 {
    bytes.div_ceil(4) as u64
}

/// The text cut into pieces of at most `chunk_bytes` bytes, each ending on a character boundary;
/// a character longer than `chunk_bytes` is a piece of its own. Empty text gives no piece.
pub fn pieces(text: &str, chunk_bytes: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = chunk_bytes.max(1).min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 {
            // The first character is longer than the piece: it goes whole.
            end = rest.chars().next().map_or(rest.len(), char::len_utf8);
        }
        let (piece, after) = rest.split_at(end);
        pieces.push(piece);
        rest = after;
    }
    pieces
}

/// The bytes of a reply that take time to produce: its text's and its tool arguments' (compact
/// JSON). Every API carries the same bytes, so the server's pacing charges the same time for the
/// same reply, streamed or whole.
pub fn paced_bytes(reply: &Reply) -> usize {
    let arguments = reply
        .tool
        .as_ref()
        .map_or(0, |tool| arguments_json(tool).len());
    reply.text.len() + arguments
}

/// The reply's own tokens: its [`paced_bytes`].
pub fn output_tokens(reply: &Reply) -> u64 {
    tokens(paced_bytes(reply))
}

/// The tool's arguments as compact JSON, which streams in pieces and counts as tokens.
fn arguments_json(tool: &MappedTool) -> String {
    json::compact(&tool.arguments)
}

/// An SSE event whose JSON `type` is also its `event:` name, as Anthropic's and OpenAI's
/// Responses' are.
fn sse(data: &Value, text_bytes: usize) -> WireEvent {
    let kind = data["type"].as_str().unwrap_or_default();
    WireEvent {
        data: format!("event: {kind}\ndata: {}\n\n", json::compact(data)),
        text_bytes,
    }
}

/// Whether the reply has a text part. A reply with neither text nor tool still has an empty one,
/// so that the response is never without content.
fn has_text_part(reply: &Reply) -> bool {
    !reply.text.is_empty() || reply.tool.is_none()
}
