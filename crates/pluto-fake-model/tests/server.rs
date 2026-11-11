use pluto_fake_model::script::Script;
use pluto_fake_model::server::{
    CONNECTION_TIMEOUT, Config, Handle, JournalEntry, Kind, OLLAMA_VERSION, spawn,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

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

fn start_with(script: &str, speed: f64, journal: Option<std::path::PathBuf>) -> Handle {
    let config = Config {
        script: Script::from_json(script).unwrap(),
        speed,
        journal_path: journal,
        models: vec!["fake-model".into()],
        connection_timeout: CONNECTION_TIMEOUT,
    };
    spawn(config, &["127.0.0.1:0".parse().unwrap()]).unwrap()
}

fn start() -> Handle {
    start_with(SCRIPT, f64::INFINITY, None)
}

struct Response {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Response {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
    fn text(&self) -> String {
        String::from_utf8(self.body.clone()).unwrap()
    }
}

/// Decodes a chunked body, or returns it as is.
fn dechunk(head: &str, body: &[u8]) -> Vec<u8> {
    if !head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return body.to_vec();
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let line_end = rest.windows(2).position(|w| w == b"\r\n").unwrap();
        let size =
            usize::from_str_radix(std::str::from_utf8(&rest[..line_end]).unwrap(), 16).unwrap();
        rest = &rest[line_end + 2..];
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

fn call(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Response {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(raw[..split].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let body = dechunk(&head, &raw[split + 4..]);
    Response { status, head, body }
}

fn post(addr: SocketAddr, path: &str, body: &Value) -> Response {
    call(
        addr,
        "POST",
        path,
        &[("Content-Type", "application/json")],
        body.to_string().as_bytes(),
    )
}

fn last(handle: &Handle) -> JournalEntry {
    handle.journal().last().cloned().unwrap()
}

fn sse_events(body: &str) -> Vec<(String, Value)> {
    body.split("\n\n")
        .filter(|e| !e.is_empty())
        .map(|e| {
            let (kind, data) = e
                .strip_prefix("event: ")
                .unwrap()
                .split_once("\ndata: ")
                .unwrap();
            (kind.to_string(), serde_json::from_str(data).unwrap())
        })
        .collect()
}

fn bash_tool() -> Value {
    json!([{"name": "Bash", "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}])
}

#[test]
fn listings_answer_what_clients_ask_at_start() {
    let handle = start();
    let addr = handle.addrs[0];
    let root = call(addr, "GET", "/", &[], b"");
    assert_eq!(
        (root.status, root.text().as_str()),
        (200, "Ollama is running")
    );
    assert_eq!(
        call(addr, "GET", "/api/version", &[], b"").json(),
        json!({"version": OLLAMA_VERSION})
    );
    let tags = call(addr, "GET", "/api/tags", &[], b"").json();
    assert_eq!(tags["models"][0]["name"], "fake-model");
    assert_eq!(tags["models"][0]["model"], "fake-model");
    let show = post(addr, "/api/show", &json!({"model": "fake-model"})).json();
    assert_eq!(show["capabilities"], json!(["completion", "tools"]));
    assert!(show["model_info"]["general.architecture"].is_string());
    for path in ["/v1/models", "/models", "/v1/models?limit=1000"] {
        let models = call(addr, "GET", path, &[], b"").json();
        assert_eq!(models["object"], "list");
        assert_eq!(models["has_more"], false);
        assert_eq!(models["data"][0]["id"], "fake-model");
        assert_eq!(models["data"][0]["type"], "model");
        assert_eq!(models["data"][0]["object"], "model");
    }
    let hello = call(addr, "HEAD", "/api/hello", &[], b"");
    assert_eq!(hello.status, 200);
    assert!(hello.body.is_empty());
    let preflight = call(addr, "OPTIONS", "/v1/chat/completions", &[], b"");
    assert_eq!(preflight.status, 204);
    assert!(preflight.head.contains("Access-Control-Allow-Origin: *"));
    let count = post(
        addr,
        "/v1/messages/count_tokens",
        &json!({"model": "m", "messages": [{"role": "user", "content": "12345678"}]}),
    );
    assert_eq!(count.status, 200);
    assert!(count.json()["input_tokens"].as_u64().unwrap() > 0);
    let missing = call(addr, "GET", "/nowhere", &[], b"");
    assert_eq!(missing.status, 404);
    assert_eq!(missing.json()["error"]["type"], "not_found");
    let journal = handle.journal();
    assert_eq!(last(&handle).kind, Kind::Unknown);
    assert!(
        journal
            .iter()
            .filter(|e| e.path == "/api/tags")
            .all(|e| e.kind == Kind::Listing)
    );
    assert!(journal.iter().any(|e| e.kind == Kind::Preflight));
    handle.stop();
}

#[test]
fn an_anthropic_turn_with_its_tool() {
    let handle = start();
    let addr = handle.addrs[0];
    let mut body = json!({
        "model": "claude-fake", "max_tokens": 100, "stream": true, "system": "You are Claude Code.",
        "tools": bash_tool(),
        "messages": [{"role": "user", "content": "[turn 0] look"}]
    });
    let first = post(addr, "/v1/messages?beta=true", &body);
    assert_eq!(first.status, 200);
    assert!(first.head.contains("Content-Type: text/event-stream"));
    let events = sse_events(&first.text());
    assert_eq!(events[0].0, "message_start");
    assert_eq!(events.last().unwrap().0, "message_stop");
    let tool = events
        .iter()
        .find(|(_, d)| d["content_block"]["type"] == "tool_use")
        .unwrap();
    assert_eq!(tool.1["content_block"]["id"], "toolu_fake_0_0");
    assert_eq!(tool.1["content_block"]["name"], "Bash");
    let entry = last(&handle);
    assert_eq!(
        (entry.kind, entry.api, entry.turn, entry.segment),
        (
            Kind::Main,
            Some(pluto_fake_model::conversation::Api::Anthropic),
            Some(0),
            Some(0)
        )
    );
    assert_eq!(entry.tool.as_deref(), Some("Bash"));
    assert!(!entry.tool_missing);
    assert_eq!(entry.tool_result_ok, None);
    assert_eq!(entry.path, "/v1/messages");
    assert!(entry.parts.unwrap().tools > 0);
    assert!(entry.response_bytes > 0);

    body["messages"].as_array_mut().unwrap().extend([
        json!({"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_fake_0_0", "name": "Bash", "input": {"command": "ls -la"}}]}),
        json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_fake_0_0", "content": "a"}]}),
    ]);
    let second = post(addr, "/v1/messages", &body);
    let text: String = sse_events(&second.text())
        .iter()
        .filter_map(|(_, d)| d["delta"]["text"].as_str().map(str::to_string))
        .collect();
    assert_eq!(text, "Here is **the answer**.");
    let entry = last(&handle);
    assert_eq!(
        (entry.segment, entry.tool.as_deref(), entry.tool_result_ok),
        (Some(1), None, Some(true))
    );
    assert!(entry.repeated_prefix > 0);

    body["messages"][2]["content"][0]["tool_use_id"] = json!("toolu_other");
    post(addr, "/v1/messages", &body);
    assert_eq!(last(&handle).tool_result_ok, Some(false));
    handle.stop();
}

#[test]
fn side_requests_get_the_side_answer() {
    let handle = start();
    let reply = post(
        handle.addrs[0],
        "/v1/messages",
        &json!({
            "model": "haiku", "max_tokens": 50, "messages": [{"role": "user", "content": "[turn 0] title this"}]
        }),
    );
    let message = reply.json();
    assert_eq!(
        message["content"],
        json!([{"type": "text", "text": "A short title"}])
    );
    assert_eq!(message["model"], "haiku");
    let entry = last(&handle);
    assert_eq!(entry.kind, Kind::Side);
    assert_eq!(entry.tool, None);
    handle.stop();
}

#[test]
fn a_responses_turn() {
    let handle = start();
    let addr = handle.addrs[0];
    let body = json!({
        "model": "gpt-fake", "stream": true, "instructions": "You are Codex.",
        "tools": [{"type": "function", "name": "shell", "parameters": {"type": "object", "properties": {"command": {"type": "array", "items": {"type": "string"}}}, "required": ["command"]}}],
        "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "[turn 0] go"}]}]
    });
    for path in ["/v1/responses", "/responses"] {
        let events = sse_events(&post(addr, path, &body).text());
        assert_eq!(events[0].0, "response.created");
        assert_eq!(events.last().unwrap().0, "response.completed");
        let call = events
            .iter()
            .find(|(k, d)| k == "response.output_item.done" && d["item"]["type"] == "function_call")
            .unwrap();
        assert_eq!(call.1["item"]["call_id"], "call_fake_0_0");
        let arguments: Value =
            serde_json::from_str(call.1["item"]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments, json!({"command": ["bash", "-lc", "ls -la"]}));
    }
    let mut whole = body.clone();
    whole["stream"] = json!(false);
    let response = post(addr, "/v1/responses", &whole).json();
    assert_eq!(response["status"], "completed");
    assert_eq!(
        last(&handle).api,
        Some(pluto_fake_model::conversation::Api::Responses)
    );
    handle.stop();
}

#[test]
fn an_ollama_turn() {
    let handle = start();
    let reply = post(
        handle.addrs[0],
        "/api/chat",
        &json!({
            "model": "fake-model",
            "messages": [{"role": "user", "content": "[turn 0] hi"}],
            "tools": [{"type": "function", "function": {"name": "bash", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}}]
        }),
    );
    assert!(reply.head.contains("Content-Type: application/x-ndjson"));
    let lines: Vec<Value> = reply
        .text()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.last().unwrap()["done"], true);
    assert!(
        lines
            .iter()
            .any(|l| l["message"]["tool_calls"][0]["function"]["name"] == "bash")
    );
    handle.stop();
}

#[test]
fn refused_requests_are_journaled() {
    let handle = start();
    let addr = handle.addrs[0];
    let encoded = call(
        addr,
        "POST",
        "/v1/responses",
        &[("Content-Encoding", "zstd")],
        b"{}",
    );
    assert_eq!(encoded.status, 415);
    assert_eq!(encoded.json()["error"]["type"], "invalid_request_error");
    let entry = last(&handle);
    assert_eq!(entry.kind, Kind::Refused);
    assert!(entry.error.unwrap().contains("zstd"));
    assert_eq!(
        call(addr, "POST", "/v1/messages", &[], b"not json").status,
        400
    );
    assert_eq!(post(addr, "/api/chat", &json!({"model": "m"})).status, 400);
    assert_eq!(last(&handle).kind, Kind::Refused);
    handle.stop();
}

#[test]
fn the_journal_file_has_a_line_per_request() {
    let dir = std::env::temp_dir().join(format!("pluto-fake-model-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.jsonl");
    let _ = std::fs::remove_file(&path);
    let handle = start_with(SCRIPT, f64::INFINITY, Some(path.clone()));
    let addr = handle.addrs[0];
    call(addr, "GET", "/api/version", &[], b"");
    call(addr, "GET", "/api/tags", &[], b"");
    post(
        addr,
        "/v1/messages",
        &json!({"model": "m", "max_tokens": 5, "messages": [{"role": "user", "content": "x"}]}),
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines
            .iter()
            .map(|l| l["request"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(lines[2]["kind"], "side");
    assert_eq!(lines[2]["api"], "anthropic");
    handle.stop();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_pace_is_kept_and_speed_divides_it() {
    let paced = SCRIPT.replace(
        "\"first_token_ms\": 0, \"tokens_per_second\": 1000000",
        "\"first_token_ms\": 300, \"tokens_per_second\": 1000000",
    );
    let body = json!({"model": "m", "max_tokens": 5, "stream": true, "tools": bash_tool(), "messages": [{"role": "user", "content": "[turn 0] x"}]});
    let slow = start_with(&paced, 1.0, None);
    let begun = Instant::now();
    post(slow.addrs[0], "/v1/messages", &body);
    assert!(
        begun.elapsed() >= Duration::from_millis(300),
        "{:?}",
        begun.elapsed()
    );
    slow.stop();
    let fast = start_with(&paced, 10.0, None);
    let begun = Instant::now();
    post(fast.addrs[0], "/v1/messages", &body);
    let elapsed = begun.elapsed();
    assert!(
        elapsed >= Duration::from_millis(30) && elapsed < Duration::from_millis(250),
        "{elapsed:?}"
    );
    fast.stop();
}

#[test]
fn fifteen_streams_at_once() {
    // 15 streams of about 2 seconds each: together they must take about 2 seconds, not 30.
    let paced = SCRIPT.replace("\"tokens_per_second\": 1000000", "\"tokens_per_second\": 4");
    let handle = start_with(&paced, 1.0, None);
    let addr = handle.addrs[0];
    let begun = Instant::now();
    let threads: Vec<_> = (0..15).map(|_| std::thread::spawn(move || {
        let body = json!({"model": "m", "max_tokens": 5, "stream": true, "tools": bash_tool(), "messages": [{"role": "user", "content": "[turn 0] x"}]});
        post(addr, "/v1/messages", &body).status
    })).collect();
    for thread in threads {
        assert_eq!(thread.join().unwrap(), 200);
    }
    assert!(
        begun.elapsed() < Duration::from_secs(10),
        "{:?}",
        begun.elapsed()
    );
    assert_eq!(
        handle
            .journal()
            .iter()
            .filter(|e| e.kind == Kind::Main)
            .count(),
        15
    );
    handle.stop();
}

#[test]
fn listens_on_several_addresses() {
    let config = Config {
        script: Script::from_json(SCRIPT).unwrap(),
        speed: f64::INFINITY,
        journal_path: None,
        models: vec!["a".into(), "b".into()],
        connection_timeout: CONNECTION_TIMEOUT,
    };
    let handle = spawn(
        config,
        &[
            "127.0.0.1:0".parse().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(handle.addrs.len(), 2);
    assert_ne!(handle.addrs[0].port(), handle.addrs[1].port());
    for addr in handle.addrs.clone() {
        let models = call(addr, "GET", "/v1/models", &[], b"").json();
        assert_eq!(models["data"].as_array().unwrap().len(), 2);
        assert_eq!(models["first_id"], "a");
        assert_eq!(models["last_id"], "b");
    }
    handle.stop();
}
