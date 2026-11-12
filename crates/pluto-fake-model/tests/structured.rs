//! Side requests that ask for structured output (a thread's title as JSON) get JSON that fits
//! their schema, as a model would give it, so that a client does not fail its title and retry it
//! elsewhere: OpenAI's Responses `text.format`, Chat Completions' `response_format`, Ollama's
//! `format`, and Claude's `StructuredOutput` tool.

use pluto_fake_model::script::Script;
use pluto_fake_model::server::{CONNECTION_TIMEOUT, Config, Handle, JournalEntry, Kind, spawn};
use pluto_fake_model::structured::example;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

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

/// The title schema the fork asks for (provider-core's textGenerationPrompts.ts).
fn title_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"title": {"type": "string"}, "needsRefinement": {"type": "boolean"}},
        "required": ["title", "needsRefinement"],
        "additionalProperties": false
    })
}

fn title() -> Value {
    json!({"title": "A short title", "needsRefinement": false})
}

#[test]
fn an_example_fits_its_schema() {
    assert_eq!(example(&title_schema(), "A short title"), title());
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "count": {"type": "integer"},
            "ratio": {"type": "number"},
            "tags": {"type": "array", "items": {"type": "string"}},
            "kind": {"type": "string", "enum": ["fix", "feature"]},
            "fixed": {"const": "always"},
            "maybe": {"type": ["string", "null"]},
            "either": {"anyOf": [{"type": "boolean"}, {"type": "string"}]},
            "inner": {"type": "object", "properties": {"deep": {"type": "string"}}}
        }
    });
    assert_eq!(
        example(&schema, "t"),
        json!({
            "name": "t", "count": 0, "ratio": 0, "tags": [], "kind": "fix", "fixed": "always",
            "maybe": "t", "either": false, "inner": {"deep": "t"}
        })
    );
    // A schema that says nothing gives the text itself; `true` (anything) too.
    assert_eq!(example(&json!({}), "t"), json!("t"));
    assert_eq!(example(&json!(true), "t"), json!("t"));
    assert_eq!(example(&json!({"type": "null"}), "t"), Value::Null);
}

#[test]
fn responses_text_format_gets_json() {
    let handle = start();
    let format = json!({"format": {"type": "json_schema", "name": "codex_output_schema", "strict": true, "schema": title_schema()}});
    for stream in [false, true] {
        let body = json!({
            "model": "fake-model", "stream": stream, "text": format,
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "Generate a title.\n\nUser message:\n[turn 0] look"}]}]
        });
        let (status, text) = post(handle.addrs[0], "/v1/responses", &body);
        assert_eq!(status, 200);
        let answer = if stream {
            text.split("\n\n")
                .filter_map(|event| event.split_once("\ndata: "))
                .map(|(_, data)| serde_json::from_str::<Value>(data).unwrap())
                .filter(|data| data["type"] == "response.output_text.delta")
                .map(|data| data["delta"].as_str().unwrap().to_owned())
                .collect::<String>()
        } else {
            let whole: Value = serde_json::from_str(&text).unwrap();
            whole["output"][0]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(serde_json::from_str::<Value>(&answer).unwrap(), title());
        assert_eq!(last(&handle).kind, Kind::Side);
    }
    handle.stop();
}

#[test]
fn chat_completions_response_format_gets_json() {
    let handle = start();
    let body = json!({
        "model": "fake-model",
        "response_format": {"type": "json_schema", "json_schema": {"name": "title", "schema": title_schema()}},
        "messages": [{"role": "user", "content": "Title this: [turn 0] look"}]
    });
    let (status, text) = post(handle.addrs[0], "/v1/chat/completions", &body);
    assert_eq!(status, 200);
    let whole: Value = serde_json::from_str(&text).unwrap();
    let content = whole["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(serde_json::from_str::<Value>(content).unwrap(), title());
    assert_eq!(last(&handle).kind, Kind::Side);
    handle.stop();
}

#[test]
fn ollama_format_gets_json() {
    let handle = start();
    let body = json!({
        "model": "fake-model", "stream": false, "format": title_schema(),
        "messages": [{"role": "user", "content": "Title this."}]
    });
    let (status, text) = post(handle.addrs[0], "/api/chat", &body);
    assert_eq!(status, 200);
    let whole: Value = serde_json::from_str(&text).unwrap();
    let content = whole["message"]["content"].as_str().unwrap();
    assert_eq!(serde_json::from_str::<Value>(content).unwrap(), title());
    handle.stop();
}

/// Claude's Agent SDK asks for structured output with a tool, `StructuredOutput`, beside a
/// placeholder; the model answers by calling it, then ends its turn once the call's result comes
/// back.
#[test]
fn claude_structured_output_tool_is_called_then_the_turn_ends() {
    let handle = start();
    let tools = json!([
        {"name": "DeferredToolPlaceholder", "input_schema": {"type": "object", "properties": {}}},
        {"name": "StructuredOutput", "input_schema": title_schema()}
    ]);
    let first = json!({
        "model": "fake-model", "max_tokens": 1000, "tools": tools,
        "messages": [{"role": "user", "content": "Generate a title.\n\nUser message:\n[turn 0] look"}]
    });
    let (status, text) = post(handle.addrs[0], "/v1/messages", &first);
    assert_eq!(status, 200);
    let whole: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(whole["stop_reason"], "tool_use");
    let call = whole["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["type"] == "tool_use")
        .expect("a tool call");
    assert_eq!(call["name"], "StructuredOutput");
    assert_eq!(call["input"], title());
    let entry = last(&handle);
    assert_eq!(entry.kind, Kind::Side);
    assert_eq!(entry.tool.as_deref(), Some("StructuredOutput"));

    let mut second = first.clone();
    second["messages"] = json!([
        first["messages"][0],
        {"role": "assistant", "content": [call]},
        {"role": "user", "content": [{"type": "tool_result", "tool_use_id": call["id"], "content": "ok"}]}
    ]);
    let (status, text) = post(handle.addrs[0], "/v1/messages", &second);
    assert_eq!(status, 200);
    let whole: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(whole["stop_reason"], "end_turn");
    assert!(
        whole["content"]
            .as_array()
            .unwrap()
            .iter()
            .all(|block| block["type"] != "tool_use")
    );
    assert_eq!(last(&handle).kind, Kind::Side);
    handle.stop();
}

#[test]
fn a_side_request_without_a_schema_keeps_the_plain_answer() {
    let handle = start();
    let body =
        json!({"model": "fake-model", "messages": [{"role": "user", "content": "Title this."}]});
    let (_, text) = post(handle.addrs[0], "/v1/chat/completions", &body);
    let whole: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(whole["choices"][0]["message"]["content"], "A short title");
    handle.stop();
}

/// A turn's request that also asks for a format still plays the script: the schema only shapes
/// side requests, which offer no tool the script can call.
#[test]
fn a_main_request_ignores_the_format() {
    let handle = start();
    let body = json!({
        "model": "fake-model", "format": title_schema(), "stream": false,
        "tools": [{"type": "function", "function": {"name": "bash", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}}],
        "messages": [{"role": "user", "content": "[turn 0] look"}]
    });
    let (_, text) = post(handle.addrs[0], "/api/chat", &body);
    let whole: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(whole["message"]["content"], "Looking at it.");
    assert_eq!(last(&handle).kind, Kind::Main);
    handle.stop();
}
