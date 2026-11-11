//! Ollama's chat API (`POST /api/chat`).
//!
//! A stream is NDJSON: one JSON object a line, each with `model`, `created_at` and `message`
//! (`role` `assistant`). One line per piece of text (`message.content`, `done` false); then, if a
//! tool is called, one line whose `message.content` is empty and whose `message.tool_calls` is
//! `[{"function": {"name", "arguments": {...}}}]`, `done` false; then a last line, `done` true,
//! `done_reason` `stop`, empty content, with `prompt_eval_count` (the input tokens), `eval_count`
//! (the output tokens) and the durations at 0.

use super::{Reply, WireEvent, arguments_json, output_tokens, pieces};
use crate::json;
use serde_json::{Value, json};

/// A fixed time, so that every client receives the same bytes.
const CREATED_AT: &str = "1970-01-01T00:00:00Z";

/// The stream's lines.
pub fn lines(reply: &Reply) -> Vec<WireEvent> {
    let mut lines: Vec<WireEvent> = pieces(&reply.text, reply.chunk_bytes)
        .into_iter()
        .map(|piece| {
            line(
                reply,
                json!({"role": "assistant", "content": piece}),
                false,
                piece.len(),
            )
        })
        .collect();
    if let Some(tool_calls) = tool_calls(reply) {
        let message = json!({"role": "assistant", "content": "", "tool_calls": tool_calls});
        let arguments_bytes = reply
            .tool
            .as_ref()
            .map_or(0, |tool| arguments_json(tool).len());
        lines.push(line(reply, message, false, arguments_bytes));
    }
    let end = json!({"role": "assistant", "content": ""});
    lines.push(line(reply, end, true, 0));
    lines
}

/// The whole response, for `stream: false`: one object, `done` true, `message.content` the whole
/// text, `message.tool_calls` if a tool is called, and the counts of the last line.
pub fn chat(reply: &Reply) -> Value {
    let mut message = json!({"role": "assistant", "content": reply.text});
    if let Some(tool_calls) = tool_calls(reply) {
        message["tool_calls"] = tool_calls;
    }
    object(reply, message, true)
}

fn tool_calls(reply: &Reply) -> Option<Value> {
    let tool = reply.tool.as_ref()?;
    Some(json!([{"function": {"name": tool.name, "arguments": tool.arguments}}]))
}

fn line(reply: &Reply, message: Value, done: bool, text_bytes: usize) -> WireEvent {
    WireEvent {
        data: format!("{}\n", json::compact(&object(reply, message, done))),
        text_bytes,
    }
}

/// One JSON object of the stream, or the whole response; the last carries the counts.
fn object(reply: &Reply, message: Value, done: bool) -> Value {
    let mut object = json!({
        "model": reply.model,
        "created_at": CREATED_AT,
        "message": message,
        "done": done
    });
    if done {
        object["done_reason"] = json!("stop");
        object["total_duration"] = json!(0);
        object["load_duration"] = json!(0);
        object["prompt_eval_count"] = json!(reply.input_tokens);
        object["prompt_eval_duration"] = json!(0);
        object["eval_count"] = json!(output_tokens(reply));
        object["eval_duration"] = json!(0);
    }
    object
}
