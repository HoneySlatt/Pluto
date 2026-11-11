//! Anthropic's Messages API (`POST /v1/messages`).
//!
//! A stream is a series of SSE events, each `event: <type>\ndata: <json>\n\n`, in this order:
//! `message_start` (a message with empty content and `usage.input_tokens`); for the text, if any,
//! `content_block_start` (a `text` block with empty text), one `content_block_delta` (`text_delta`)
//! per piece, `content_block_stop`; for the tool, if any, `content_block_start` (a `tool_use`
//! block with the call's id and name and empty `input`), one `content_block_delta`
//! (`input_json_delta`, `partial_json`) per piece of the arguments' compact JSON,
//! `content_block_stop`; `message_delta` (`stop_reason` `tool_use` or `end_turn`,
//! `usage.output_tokens`); `message_stop`. Blocks are numbered from 0 in that order.

use super::{Reply, WireEvent, arguments_json, has_text_part, output_tokens, pieces, sse};
use serde_json::{Value, json};

/// The stream's events.
pub fn events(reply: &Reply) -> Vec<WireEvent> {
    let mut events = vec![sse(
        &json!({
            "type": "message_start",
            "message": {
                "id": reply.id,
                "type": "message",
                "role": "assistant",
                "model": reply.model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {"input_tokens": reply.input_tokens, "output_tokens": 0}
            }
        }),
        0,
    )];
    let mut index = 0;
    if has_text_part(reply) {
        events.push(block_start(index, json!({"type": "text", "text": ""})));
        for piece in pieces(&reply.text, reply.chunk_bytes) {
            events.push(block_delta(
                index,
                json!({"type": "text_delta", "text": piece}),
                piece.len(),
            ));
        }
        events.push(block_stop(index));
        index += 1;
    }
    if let Some(tool) = &reply.tool {
        let content_block =
            json!({"type": "tool_use", "id": tool.call_id, "name": tool.name, "input": {}});
        events.push(block_start(index, content_block));
        for piece in pieces(&arguments_json(tool), reply.chunk_bytes) {
            let delta = json!({"type": "input_json_delta", "partial_json": piece});
            events.push(block_delta(index, delta, piece.len()));
        }
        events.push(block_stop(index));
    }
    events.push(sse(
        &json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason(reply), "stop_sequence": null},
            "usage": {"output_tokens": output_tokens(reply)}
        }),
        0,
    ));
    events.push(sse(&json!({"type": "message_stop"}), 0));
    events
}

/// The whole response, for `stream: false`: `{"id", "type": "message", "role": "assistant",
/// "model", "content": [text block?, tool_use block?], "stop_reason", "stop_sequence": null,
/// "usage": {"input_tokens", "output_tokens"}}`.
pub fn message(reply: &Reply) -> Value {
    let mut content = Vec::new();
    if has_text_part(reply) {
        content.push(json!({"type": "text", "text": reply.text}));
    }
    if let Some(tool) = &reply.tool {
        content.push(json!({"type": "tool_use", "id": tool.call_id, "name": tool.name, "input": tool.arguments}));
    }
    json!({
        "id": reply.id,
        "type": "message",
        "role": "assistant",
        "model": reply.model,
        "content": content,
        "stop_reason": stop_reason(reply),
        "stop_sequence": null,
        "usage": {"input_tokens": reply.input_tokens, "output_tokens": output_tokens(reply)}
    })
}

fn stop_reason(reply: &Reply) -> &'static str {
    if reply.tool.is_some() {
        "tool_use"
    } else {
        "end_turn"
    }
}

fn block_start(index: usize, content_block: Value) -> WireEvent {
    sse(
        &json!({"type": "content_block_start", "index": index, "content_block": content_block}),
        0,
    )
}

fn block_delta(index: usize, delta: Value, text_bytes: usize) -> WireEvent {
    sse(
        &json!({"type": "content_block_delta", "index": index, "delta": delta}),
        text_bytes,
    )
}

fn block_stop(index: usize) -> WireEvent {
    sse(&json!({"type": "content_block_stop", "index": index}), 0)
}
