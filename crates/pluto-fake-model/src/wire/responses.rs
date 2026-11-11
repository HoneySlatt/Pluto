//! OpenAI's Responses API (`POST /v1/responses`), as Codex reads it.
//!
//! A stream is a series of SSE events, each `event: <type>\ndata: <json>\n\n`, the JSON's `type`
//! the same as the event's, each with a `sequence_number` counting from 0:
//! `response.created` (a response with `status` `in_progress` and empty `output`); for the text,
//! if any, `response.output_item.added` (an assistant `message` item, `id` `msg_<reply id>`, empty
//! content), `response.content_part.added` (`item_id`, `output_index`, `content_index` 0, and
//! `part`: an `output_text` with empty text and empty `annotations`), one
//! `response.output_text.delta` (`item_id`, `output_index`, `content_index` 0, `delta`) per piece,
//! `response.output_text.done` (the same ids, and the whole `text`), `response.content_part.done`
//! (the same ids, and the `part` whole), `response.output_item.done` (the message whole, its
//! content that same part); for the tool, if any,
//! `response.output_item.added` then `response.output_item.done`, both a `function_call` item
//! (`id` `fc_<reply id>`, `call_id`, `name`, `arguments` as a string of compact JSON, empty in
//! `added`), the arguments carried whole by `done`; `response.completed` (the response whole,
//! `status` `completed`, its `output` every item, `usage` with `input_tokens`, `output_tokens`,
//! `total_tokens`). Items are numbered from 0 (`output_index`) in that order.

use super::{Reply, WireEvent, arguments_json, has_text_part, output_tokens, pieces, sse};
use serde_json::{Value, json};

/// The stream's events.
pub fn events(reply: &Reply) -> Vec<WireEvent> {
    let mut sequence = Sequence::default();
    let created = response_object(reply, "in_progress", &[]);
    sequence.push(json!({"type": "response.created", "response": created}), 0);

    let mut output_index = 0;
    if has_text_part(reply) {
        let item_id = message_id(reply);
        let added = message_item(reply, "in_progress", json!([]));
        sequence.push(
            item_event("response.output_item.added", output_index, added),
            0,
        );
        let part_event = |kind: &str, field: &str, value: Value| {
            json!({
                "type": kind,
                "item_id": item_id,
                "output_index": output_index,
                "content_index": 0,
                field: value
            })
        };
        sequence.push(
            part_event("response.content_part.added", "part", text_part("")),
            0,
        );
        for piece in pieces(&reply.text, reply.chunk_bytes) {
            sequence.push(
                json!({
                    "type": "response.output_text.delta",
                    "item_id": item_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "delta": piece
                }),
                piece.len(),
            );
        }
        sequence.push(
            part_event("response.output_text.done", "text", json!(reply.text)),
            0,
        );
        sequence.push(
            part_event("response.content_part.done", "part", text_part(&reply.text)),
            0,
        );
        sequence.push(
            item_event(
                "response.output_item.done",
                output_index,
                message_done(reply),
            ),
            0,
        );
        output_index += 1;
    }
    if let Some(tool) = &reply.tool {
        let added = call_item(reply, "in_progress", "");
        sequence.push(
            item_event("response.output_item.added", output_index, added),
            0,
        );
        let arguments = arguments_json(tool);
        let done = call_item(reply, "completed", &arguments);
        sequence.push(
            item_event("response.output_item.done", output_index, done),
            arguments.len(),
        );
    }
    sequence.push(
        json!({"type": "response.completed", "response": response(reply)}),
        0,
    );
    sequence.events
}

/// The whole response, for `stream: false`: the object `response.completed` carries.
pub fn response(reply: &Reply) -> Value {
    let mut output = Vec::new();
    if has_text_part(reply) {
        output.push(message_done(reply));
    }
    if let Some(tool) = &reply.tool {
        output.push(call_item(reply, "completed", &arguments_json(tool)));
    }
    response_object(reply, "completed", &output)
}

/// The events of a stream, each numbered in the order it was pushed.
#[derive(Default)]
struct Sequence {
    events: Vec<WireEvent>,
}

impl Sequence {
    fn push(&mut self, event: Value, text_bytes: usize) {
        let numbered = match event {
            Value::Object(mut fields) => {
                fields.insert("sequence_number".to_owned(), json!(self.events.len()));
                Value::Object(fields)
            }
            other => other,
        };
        self.events.push(sse(&numbered, text_bytes));
    }
}

fn item_event(kind: &str, output_index: usize, item: Value) -> Value {
    json!({"type": kind, "output_index": output_index, "item": item})
}

fn message_id(reply: &Reply) -> String {
    format!("msg_{}", reply.id)
}

fn message_item(reply: &Reply, status: &str, content: Value) -> Value {
    json!({
        "type": "message",
        "id": message_id(reply),
        "status": status,
        "role": "assistant",
        "content": content
    })
}

fn text_part(text: &str) -> Value {
    json!({"type": "output_text", "text": text, "annotations": []})
}

fn message_done(reply: &Reply) -> Value {
    message_item(reply, "completed", json!([text_part(&reply.text)]))
}

/// The tool's call; `arguments` is a string of compact JSON, empty while the call is only begun.
fn call_item(reply: &Reply, status: &str, arguments: &str) -> Value {
    let (call_id, name) = reply
        .tool
        .as_ref()
        .map_or(("", ""), |tool| (&tool.call_id, &tool.name));
    json!({
        "type": "function_call",
        "id": format!("fc_{}", reply.id),
        "status": status,
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    })
}

fn response_object(reply: &Reply, status: &str, output: &[Value]) -> Value {
    let mut response = json!({
        "id": reply.id,
        "object": "response",
        "created_at": 0,
        "status": status,
        "model": reply.model,
        "output": output
    });
    if status == "completed" {
        let output_tokens = output_tokens(reply);
        response["usage"] = json!({
            "input_tokens": reply.input_tokens,
            "input_tokens_details": {"cached_tokens": 0},
            "output_tokens": output_tokens,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": reply.input_tokens + output_tokens
        });
    }
    response
}
