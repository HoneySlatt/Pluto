//! OpenAI's Chat Completions API (`POST /v1/chat/completions`).
//!
//! A stream is a series of SSE events, each `data: <json>\n\n` with no `event:` line, the JSON a
//! `chat.completion.chunk` (`id` the reply's, `created` 0, `model`, one choice of `index` 0):
//! a first chunk whose `delta` is `{"role": "assistant", "content": ""}`; one chunk per piece of
//! text (`delta.content`); for the tool, if any, one chunk whose `delta.tool_calls` is
//! `[{"index": 0, "id", "type": "function", "function": {"name", "arguments": ""}}]`, then one
//! chunk per piece of the arguments (compact JSON, cut like the text) whose `delta.tool_calls` is
//! `[{"index": 0, "function": {"arguments": <piece>}}]`; a last chunk with an empty `delta` and a
//! `finish_reason` (`tool_calls` if a tool is called, else `stop`; `null` on every other chunk);
//! then `data: [DONE]\n\n`. A reply with only a tool has no text chunk, and its content is `null`,
//! as OpenAI sends it.
//!
//! The usage (`prompt_tokens`, `completion_tokens`, `total_tokens`) streams only when the request
//! asks for it (`stream_options.include_usage`), as OpenAI documents: every chunk then carries
//! `"usage": null`, and one more chunk, whose `choices` is empty, carries it before `[DONE]`.

use super::{Reply, WireEvent, arguments_json, has_text_part, output_tokens, pieces};
use crate::json;
use serde_json::{Value, json};

/// The stream's events; `include_usage` is the request's `stream_options.include_usage`.
pub fn events(reply: &Reply, include_usage: bool) -> Vec<WireEvent> {
    // Asked for, the usage is in every chunk: `null`, until a last chunk of its own.
    let chunk = |delta: Value| {
        let mut chunk = object(reply, "chat.completion.chunk", delta, "delta");
        if include_usage {
            chunk["usage"] = Value::Null;
        }
        chunk
    };
    // The role first, with empty content, or `null` when only a tool follows.
    let empty = if has_text_part(reply) {
        json!("")
    } else {
        Value::Null
    };
    let mut events = vec![event(
        &chunk(json!({"role": "assistant", "content": empty})),
        0,
    )];
    for piece in pieces(&reply.text, reply.chunk_bytes) {
        events.push(event(&chunk(json!({"content": piece})), piece.len()));
    }
    if let Some(tool) = &reply.tool {
        let begin = json!({"tool_calls": [{
            "index": 0,
            "id": tool.call_id,
            "type": "function",
            "function": {"name": tool.name, "arguments": ""}
        }]});
        events.push(event(&chunk(begin), 0));
        for piece in pieces(&arguments_json(tool), reply.chunk_bytes) {
            let delta = json!({"tool_calls": [{"index": 0, "function": {"arguments": piece}}]});
            events.push(event(&chunk(delta), piece.len()));
        }
    }
    let mut end = chunk(json!({}));
    end["choices"][0]["finish_reason"] = json!(finish_reason(reply));
    events.push(event(&end, 0));
    if include_usage {
        let mut last = object(reply, "chat.completion.chunk", Value::Null, "delta");
        last["choices"] = json!([]);
        last["usage"] = usage(reply);
        events.push(event(&last, 0));
    }
    events.push(WireEvent {
        data: "data: [DONE]\n\n".to_owned(),
        text_bytes: 0,
    });
    events
}

/// The whole response, for `stream: false`: one `chat.completion`, `message.content` the whole
/// text, `message.tool_calls` if a tool is called, `finish_reason` and `usage`.
pub fn completion(reply: &Reply) -> Value {
    let mut message = json!({"role": "assistant", "content": content(reply)});
    if let Some(tool) = &reply.tool {
        message["tool_calls"] = json!([{
            "id": tool.call_id,
            "type": "function",
            "function": {"name": tool.name, "arguments": arguments_json(tool)}
        }]);
    }
    let mut completion = object(reply, "chat.completion", message, "message");
    completion["choices"][0]["finish_reason"] = json!(finish_reason(reply));
    completion["usage"] = usage(reply);
    completion
}

/// The answer's text, or `null` for a reply with only a tool.
fn content(reply: &Reply) -> Value {
    if has_text_part(reply) {
        json!(reply.text)
    } else {
        Value::Null
    }
}

fn finish_reason(reply: &Reply) -> &'static str {
    if reply.tool.is_some() {
        "tool_calls"
    } else {
        "stop"
    }
}

fn usage(reply: &Reply) -> Value {
    let completion_tokens = output_tokens(reply);
    json!({
        "prompt_tokens": reply.input_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": reply.input_tokens + completion_tokens
    })
}

/// The object with its one choice, whose `field` (`delta` or `message`) holds `content`.
fn object(reply: &Reply, kind: &str, content: Value, field: &str) -> Value {
    json!({
        "id": reply.id,
        "object": kind,
        "created": 0,
        "model": reply.model,
        "choices": [{"index": 0, field: content, "finish_reason": null}]
    })
}

fn event(data: &Value, text_bytes: usize) -> WireEvent {
    WireEvent {
        data: format!("data: {}\n\n", json::compact(data)),
        text_bytes,
    }
}
