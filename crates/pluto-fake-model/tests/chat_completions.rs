//! OpenAI's Chat Completions (`POST /v1/chat/completions`, `POST /chat/completions`): the request
//! read like the other APIs, the answer encoded as OpenAI documents it, streamed or whole.

use pluto_fake_model::conversation::{Api, ToolResult, read};
use pluto_fake_model::script::{Position, Script};
use pluto_fake_model::server::{CONNECTION_TIMEOUT, Config, Handle, JournalEntry, Kind, spawn};
use pluto_fake_model::tools::MappedTool;
use pluto_fake_model::wire::{Reply, WireEvent, chat_completions, output_tokens, pieces};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

fn compact(value: &Value) -> usize {
    serde_json::to_string(value).unwrap().len()
}

fn reply(tool: bool) -> Reply {
    Reply {
        model: "fake-model".into(),
        id: "r7".into(),
        text: "Héllo, wörld: a list\n- one\n- two".into(),
        tool: tool.then(|| MappedTool {
            call_id: "call_x".into(),
            name: "bash".into(),
            arguments: json!({"command": "ls -la"}),
        }),
        input_tokens: 250,
        chunk_bytes: 5,
    }
}

/// A Chat Completions stream's event: `data: <json>\n\n`, no `event:` line.
fn data(event: &WireEvent) -> String {
    let rest = event
        .data
        .strip_prefix("data: ")
        .expect("a data line, no event line");
    let json = rest.strip_suffix("\n\n").expect("ends with a blank line");
    assert!(!json.contains('\n'), "one data line");
    json.to_owned()
}

fn chunks(events: &[WireEvent]) -> Vec<Value> {
    let (last, chunks) = events.split_last().unwrap();
    assert_eq!(data(last), "[DONE]", "the stream ends with [DONE]");
    assert_eq!(last.text_bytes, 0);
    chunks
        .iter()
        .map(|event| serde_json::from_str(&data(event)).unwrap())
        .collect()
}

#[test]
fn stream_with_a_tool() {
    let reply = reply(true);
    let events = chat_completions::events(&reply, false);
    let chunks = chunks(&events);
    for chunk in &chunks {
        assert_eq!(chunk["id"], "r7");
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert_eq!(chunk["model"], "fake-model");
        assert_eq!(
            chunk["created"], 0,
            "a fixed time: every client gets the same bytes"
        );
        assert_eq!(chunk["choices"].as_array().unwrap().len(), 1);
        assert_eq!(chunk["choices"][0]["index"], 0);
    }
    // The first chunk gives the role, with empty content.
    assert_eq!(
        chunks[0]["choices"][0]["delta"],
        json!({"role": "assistant", "content": ""})
    );
    assert_eq!(chunks[0]["choices"][0]["finish_reason"], Value::Null);

    let text_pieces = pieces(&reply.text, 5);
    let text: Vec<&str> = chunks
        .iter()
        .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
        .filter(|piece| !piece.is_empty())
        .collect();
    assert_eq!(text, text_pieces, "one chunk a piece of text");

    // The call: its id, type and name first, with empty arguments; then the arguments in pieces.
    let calls: Vec<&Value> = chunks
        .iter()
        .filter_map(|c| c["choices"][0]["delta"].get("tool_calls"))
        .collect();
    assert_eq!(
        calls[0],
        &json!([{"index": 0, "id": "call_x", "type": "function", "function": {"name": "bash", "arguments": ""}}])
    );
    let arguments: String = calls[1..]
        .iter()
        .map(|call| {
            assert_eq!(call[0]["index"], 0);
            assert!(call[0].get("id").is_none(), "the id comes once");
            call[0]["function"]["arguments"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&arguments).unwrap(),
        json!({"command": "ls -la"})
    );
    assert_eq!(calls.len() - 1, pieces(&arguments, 5).len());

    // The last chunk: an empty delta and the reason. Not asked for, the usage is in no chunk.
    let end = chunks.last().unwrap();
    assert_eq!(end["choices"][0]["delta"], json!({}));
    assert_eq!(end["choices"][0]["finish_reason"], "tool_calls");
    assert!(chunks.iter().all(|c| c.get("usage").is_none()));

    let carried: usize = events.iter().map(|e| e.text_bytes).sum();
    assert_eq!(carried, reply.text.len() + arguments.len());
}

/// With `stream_options.include_usage`, as OpenAI documents it: every chunk carries `usage: null`,
/// and one more chunk, with no choice, carries the usage before `[DONE]`.
#[test]
fn stream_with_its_usage_asked() {
    let reply = reply(true);
    let events = chat_completions::events(&reply, true);
    let chunks = chunks(&events);
    let (usage, others) = chunks.split_last().unwrap();
    assert_eq!(usage["choices"], json!([]));
    assert_eq!(usage["object"], "chat.completion.chunk");
    assert_eq!(
        usage["usage"],
        json!({
            "prompt_tokens": 250,
            "completion_tokens": output_tokens(&reply),
            "total_tokens": 250 + output_tokens(&reply)
        })
    );
    assert!(
        others
            .iter()
            .all(|c| c["usage"].is_null() && c.get("usage").is_some())
    );
    assert_eq!(
        others.last().unwrap()["choices"][0]["finish_reason"],
        "tool_calls"
    );
    let carried: usize = events.iter().map(|e| e.text_bytes).sum();
    let without: usize = chat_completions::events(&reply, false)
        .iter()
        .map(|e| e.text_bytes)
        .sum();
    assert_eq!(carried, without, "the usage carries no paced bytes");
}

/// A reply with only a tool has no text: its content is `null`, as OpenAI sends it.
#[test]
fn a_tool_without_text_has_null_content() {
    let mut tool_only = reply(true);
    tool_only.text.clear();
    let whole = chat_completions::completion(&tool_only);
    assert_eq!(whole["choices"][0]["message"]["content"], Value::Null);
    assert_eq!(whole["choices"][0]["finish_reason"], "tool_calls");
    let chunks = chunks(&chat_completions::events(&tool_only, false));
    assert_eq!(
        chunks[0]["choices"][0]["delta"],
        json!({"role": "assistant", "content": null})
    );
    assert!(
        chunks[1..]
            .iter()
            .all(|c| c["choices"][0]["delta"].get("content").is_none())
    );
}

#[test]
fn stream_without_a_tool_stops() {
    let reply = reply(false);
    let events = chat_completions::events(&reply, false);
    let chunks = chunks(&events);
    assert!(
        chunks
            .iter()
            .all(|c| c["choices"][0]["delta"].get("tool_calls").is_none())
    );
    assert_eq!(
        chunks.last().unwrap()["choices"][0]["finish_reason"],
        "stop"
    );
    let text: String = chunks
        .iter()
        .filter_map(|c| c["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, reply.text);
}

#[test]
fn an_empty_reply_still_has_its_role_and_end() {
    let mut empty = reply(false);
    empty.text.clear();
    let chunks = chunks(&chat_completions::events(&empty, false));
    assert_eq!(chunks.len(), 2, "the role, then the end");
    assert_eq!(chunks[0]["choices"][0]["delta"]["content"], "");
    assert_eq!(chunks[1]["choices"][0]["finish_reason"], "stop");
}

#[test]
fn whole_completion() {
    let with_tool = chat_completions::completion(&reply(true));
    assert_eq!(with_tool["id"], "r7");
    assert_eq!(with_tool["object"], "chat.completion");
    assert_eq!(with_tool["created"], 0);
    assert_eq!(with_tool["model"], "fake-model");
    let choice = &with_tool["choices"][0];
    assert_eq!(choice["index"], 0);
    assert_eq!(choice["finish_reason"], "tool_calls");
    assert_eq!(choice["message"]["role"], "assistant");
    assert_eq!(choice["message"]["content"], reply(true).text);
    let call = &choice["message"]["tool_calls"][0];
    assert_eq!(call["id"], "call_x");
    assert_eq!(call["type"], "function");
    assert_eq!(call["function"]["name"], "bash");
    let arguments = call["function"]["arguments"]
        .as_str()
        .expect("arguments are a JSON string");
    assert_eq!(
        serde_json::from_str::<Value>(arguments).unwrap(),
        json!({"command": "ls -la"})
    );
    assert_eq!(
        with_tool["usage"]["completion_tokens"],
        output_tokens(&reply(true))
    );
    assert_eq!(with_tool["usage"]["prompt_tokens"], 250);

    let plain = chat_completions::completion(&reply(false));
    assert_eq!(plain["choices"][0]["finish_reason"], "stop");
    assert!(plain["choices"][0]["message"].get("tool_calls").is_none());
}

fn chat_body() -> Value {
    json!({
        "model": "fake-model",
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": [
            {"role": "system", "content": "Be useful."},
            {"role": "developer", "content": [{"type": "text", "text": "Use tools."}]},
            {"role": "user", "content": "[turn 0] first"},
            {"role": "assistant", "content": "done"},
            {"role": "user", "content": [{"type": "text", "text": "[turn 1] look"}]},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_fake_1_0", "type": "function", "function": {"name": "bash", "arguments": "{\"command\":\"ls\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "call_fake_1_0", "content": "a\nb"}
        ],
        "tools": [{"type": "function", "function": {"name": "bash", "description": "Runs a command.", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}}]
    })
}

#[test]
fn request_position_tools_results_and_parts() {
    let body = chat_body();
    let conversation = read(Api::ChatCompletions, &body, 999).unwrap();
    assert!(conversation.stream);
    assert_eq!(conversation.model, "fake-model");
    assert_eq!(
        conversation.position,
        Position {
            turn: 1,
            segment: 1
        }
    );
    assert_eq!(
        conversation.tool_results,
        vec![ToolResult {
            call_id: "call_fake_1_0".into()
        }]
    );
    assert_eq!(conversation.tools.len(), 1);
    assert_eq!(conversation.tools[0].name, "bash");
    assert_eq!(
        conversation.tools[0].schema,
        body["tools"][0]["function"]["parameters"]
    );
    let messages = body["messages"].as_array().unwrap();
    let parts = conversation.parts;
    assert_eq!(parts.total, 999);
    // System and developer messages are the instructions.
    assert_eq!(
        parts.instructions,
        compact(&messages[0]) + compact(&messages[1])
    );
    assert_eq!(parts.tools, compact(&body["tools"]));
    assert_eq!(parts.new_message, compact(&messages[6]));
    assert_eq!(
        parts.history,
        messages[2..6].iter().map(compact).sum::<usize>()
    );
}

#[test]
fn request_defaults_and_errors() {
    // Chat Completions answers whole unless asked to stream.
    let quiet = json!({"model": "m", "messages": [{"role": "user", "content": "x"}]});
    let conversation = read(Api::ChatCompletions, &quiet, 1).unwrap();
    assert!(!conversation.stream);
    assert_eq!(
        conversation.position,
        Position {
            turn: 0,
            segment: 0
        }
    );
    assert!(conversation.tools.is_empty());
    // A last assistant message is history, not a new message.
    let ends_with_assistant = json!({"model": "m", "messages": [
        {"role": "user", "content": "[turn 0] x"},
        {"role": "assistant", "content": "y"}
    ]});
    let parts = read(Api::ChatCompletions, &ends_with_assistant, 1)
        .unwrap()
        .parts;
    assert_eq!(parts.new_message, 0);
    for body in [
        json!({"messages": []}),
        json!({"model": "m"}),
        json!({"model": "m", "messages": "no"}),
        json!([1]),
    ] {
        assert!(!read(Api::ChatCompletions, &body, 1).unwrap_err().is_empty());
    }
}

// Through the server.

const SCRIPT: &str = r#"{
  "version": 1,
  "pace": {"first_token_ms": 0, "tokens_per_second": 1000000, "chunk_bytes": 8},
  "side_answer": "A short title",
  "turns": [
    { "segments": [
      { "text": "Looking at it.", "tool": { "role": "shell", "command": "ls -la" } },
      { "text": "Here is **the answer**." }
    ] }
  ]
}"#;

fn start() -> Handle {
    let config = Config {
        script: Script::from_json(SCRIPT).unwrap(),
        speed: f64::INFINITY,
        journal_path: None,
        models: vec!["fake-model".into()],
        connection_timeout: CONNECTION_TIMEOUT,
        chat: false,
    };
    spawn(config, &["127.0.0.1:0".parse().unwrap()]).unwrap()
}

/// Posts JSON; gives the status and the body, dechunked.
fn post(addr: SocketAddr, path: &str, body: &Value) -> (u16, String) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(raw[..split].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let mut rest = &raw[split + 4..];
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = Vec::new();
        loop {
            let end = rest.windows(2).position(|w| w == b"\r\n").unwrap();
            let size =
                usize::from_str_radix(std::str::from_utf8(&rest[..end]).unwrap(), 16).unwrap();
            rest = &rest[end + 2..];
            if size == 0 {
                break out;
            }
            out.extend_from_slice(&rest[..size]);
            rest = &rest[size + 2..];
        }
    } else {
        rest.to_vec()
    };
    (status, String::from_utf8(body).unwrap())
}

fn last(handle: &Handle) -> JournalEntry {
    handle.journal().last().cloned().unwrap()
}

fn tools() -> Value {
    json!([{"type": "function", "function": {"name": "bash", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}}])
}

#[test]
fn a_streamed_turn_through_the_server() {
    let handle = start();
    let addr = handle.addrs[0];
    for path in ["/v1/chat/completions", "/chat/completions"] {
        let body = json!({
            "model": "fake-model", "stream": true, "tools": tools(),
            "stream_options": {"include_usage": true},
            "messages": [{"role": "user", "content": "[turn 0] look"}]
        });
        let (status, text) = post(addr, path, &body);
        assert_eq!(status, 200, "{path}");
        let datas: Vec<&str> = text
            .split("\n\n")
            .filter(|e| !e.is_empty())
            .map(|e| e.strip_prefix("data: ").unwrap())
            .collect();
        assert_eq!(*datas.last().unwrap(), "[DONE]");
        let chunks: Vec<Value> = datas[..datas.len() - 1]
            .iter()
            .map(|d| serde_json::from_str(d).unwrap())
            .collect();
        let call = chunks
            .iter()
            .find_map(|c| c["choices"][0]["delta"].get("tool_calls"))
            .unwrap();
        let usage = chunks.last().unwrap();
        assert_eq!(
            usage["choices"],
            json!([]),
            "the usage asked for comes last"
        );
        assert!(usage["usage"]["prompt_tokens"].as_u64().unwrap() > 0);
        assert_eq!(call[0]["id"], "call_fake_0_0");
        assert_eq!(call[0]["function"]["name"], "bash");
        let entry = last(&handle);
        assert_eq!(entry.api, Some(Api::ChatCompletions));
        assert_eq!(entry.kind, Kind::Main);
        assert_eq!(entry.turn, Some(0));
        assert_eq!(entry.segment, Some(0));
        assert_eq!(entry.tool.as_deref(), Some("bash"));
        assert_eq!(entry.tools_offered, vec!["bash".to_owned()]);
    }

    // The result sent back: the second segment, its call checked by id.
    let body = json!({
        "model": "fake-model", "stream": false, "tools": tools(),
        "messages": [
            {"role": "user", "content": "[turn 0] look"},
            {"role": "assistant", "content": "Looking at it.", "tool_calls": [{"id": "call_fake_0_0", "type": "function", "function": {"name": "bash", "arguments": "{\"command\":\"ls -la\"}"}}]},
            {"role": "tool", "tool_call_id": "call_fake_0_0", "content": "a"}
        ]
    });
    let (status, text) = post(addr, "/v1/chat/completions", &body);
    assert_eq!(status, 200);
    let whole: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(whole["object"], "chat.completion");
    assert_eq!(
        whole["choices"][0]["message"]["content"],
        "Here is **the answer**."
    );
    assert_eq!(whole["choices"][0]["finish_reason"], "stop");
    let entry = last(&handle);
    assert_eq!(entry.segment, Some(1));
    assert_eq!(entry.tool_result_ok, Some(true));

    // A wrong id is noticed.
    let mut wrong = body.clone();
    wrong["messages"][2]["tool_call_id"] = json!("call_other");
    post(addr, "/v1/chat/completions", &wrong);
    assert_eq!(last(&handle).tool_result_ok, Some(false));
    handle.stop();
}

#[test]
fn a_title_request_without_tools_gets_the_side_answer() {
    let handle = start();
    let body =
        json!({"model": "fake-model", "messages": [{"role": "user", "content": "Title this."}]});
    let (status, text) = post(handle.addrs[0], "/v1/chat/completions", &body);
    assert_eq!(status, 200);
    let whole: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(whole["choices"][0]["message"]["content"], "A short title");
    assert_eq!(last(&handle).kind, Kind::Side);
    assert_eq!(
        post(
            handle.addrs[0],
            "/v1/chat/completions",
            &json!({"model": "m"})
        )
        .0,
        400
    );
    handle.stop();
}
