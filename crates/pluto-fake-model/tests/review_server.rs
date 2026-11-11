//! Tests added by the review of the server: pacing, the journal, refusals and the routes.

use pluto_fake_model::http::MAX_BODY;
use pluto_fake_model::script::Script;
use pluto_fake_model::server::{CONNECTION_TIMEOUT, Config, Handle, JournalEntry, Kind, spawn};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// A script of one turn with one segment, which has no tool.
fn script(first_token_ms: u64, tokens_per_second: f64, chunk_bytes: usize, text: &str) -> String {
    json!({
        "version": 1,
        "pace": {
            "first_token_ms": first_token_ms,
            "tokens_per_second": tokens_per_second,
            "chunk_bytes": chunk_bytes
        },
        "side_answer": "A short title",
        "turns": [{"segments": [{"text": text}]}]
    })
    .to_string()
}

fn start_with(script: &str, speed: f64, journal: Option<std::path::PathBuf>) -> Handle {
    start_timed(script, speed, journal, CONNECTION_TIMEOUT)
}

fn start_timed(
    script: &str,
    speed: f64,
    journal: Option<std::path::PathBuf>,
    connection_timeout: Duration,
) -> Handle {
    let config = Config {
        script: Script::from_json(script).unwrap(),
        speed,
        journal_path: journal,
        models: vec!["fake-model".into()],
        connection_timeout,
    };
    spawn(config, &["127.0.0.1:0".parse().unwrap()]).unwrap()
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
}

fn request_bytes(method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    let mut bytes = request.into_bytes();
    bytes.extend_from_slice(body);
    bytes
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
    stream
        .write_all(&request_bytes(method, path, headers, body))
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(raw[..split].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Response {
        status,
        head,
        body: raw[split + 4..].to_vec(),
    }
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

fn journal_json(entry: &JournalEntry) -> Value {
    serde_json::to_value(entry).unwrap()
}

fn bash_tool() -> Value {
    json!({"name": "Bash", "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}})
}

fn anthropic_body(stream: bool, tools: Value) -> Value {
    json!({"model": "m", "max_tokens": 5, "stream": stream, "tools": tools,
        "messages": [{"role": "user", "content": "[turn 0] x"}]})
}

fn responses_body(stream: bool) -> Value {
    json!({"model": "m", "stream": stream,
        "tools": [{"type": "function", "name": "shell", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}],
        "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "[turn 0] x"}]}]})
}

fn ollama_body(stream: bool) -> Value {
    json!({"model": "m", "stream": stream,
        "messages": [{"role": "user", "content": "[turn 0] x"}],
        "tools": [{"type": "function", "function": {"name": "bash", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}}]})
}

/// The three APIs' requests for the script's first segment, each with a tool it can call.
fn requests(stream: bool) -> [(&'static str, Value); 3] {
    [
        ("/v1/messages", anthropic_body(stream, json!([bash_tool()]))),
        ("/v1/responses", responses_body(stream)),
        ("/api/chat", ollama_body(stream)),
    ]
}

fn timed(addr: SocketAddr, path: &str, body: &Value) -> Duration {
    let begun = Instant::now();
    assert_eq!(post(addr, path, body).status, 200);
    begun.elapsed()
}

#[test]
fn many_pieces_take_the_paced_time_without_drift() {
    // 8000 bytes at 4000 tokens a second: 0.5 s, in 2000 pieces of 0.25 ms each. A sleep that
    // overshoots by tens of microseconds, or a loaded machine's late wake-up, must not push every
    // later piece back: without chained deadlines this takes about a quarter longer.
    let text = "a".repeat(8000);
    let handle = start_with(&script(0, 4000.0, 4, &text), 1.0, None);
    let elapsed = timed(
        handle.addrs[0],
        "/v1/messages",
        &anthropic_body(true, json!([bash_tool()])),
    );
    let paced = Duration::from_millis(500);
    assert!(
        elapsed >= paced.mul_f64(0.95) && elapsed <= paced.mul_f64(1.15),
        "{elapsed:?} for a paced time of {paced:?}"
    );
    handle.stop();
}

#[test]
fn whole_answers_wait_like_streams() {
    // 300 ms to the first token, then the text and the arguments (14 + 20 bytes) at 100 tokens a
    // second: 340 ms.
    let script = json!({
        "version": 1,
        "pace": {"first_token_ms": 300, "tokens_per_second": 100, "chunk_bytes": 16},
        "side_answer": "A short title",
        "turns": [{"segments": [
            {"text": "Looking at it.", "tool": {"role": "shell", "command": "ls -la"}},
            {"text": "Done."}
        ]}]
    })
    .to_string();
    let slow = start_with(&script, 1.0, None);
    for (path, body) in requests(false) {
        let elapsed = timed(slow.addrs[0], path, &body);
        assert!(
            elapsed >= Duration::from_millis(340) && elapsed < Duration::from_millis(600),
            "{path}: {elapsed:?}"
        );
    }
    // A listing is not a model's answer: it leaves at once.
    let begun = Instant::now();
    call(slow.addrs[0], "GET", "/api/tags", &[], b"");
    assert!(begun.elapsed() < Duration::from_millis(150));
    slow.stop();

    let fast = start_with(&script, 4.0, None);
    for (path, body) in requests(false) {
        let elapsed = timed(fast.addrs[0], path, &body);
        assert!(
            elapsed >= Duration::from_millis(85) && elapsed < Duration::from_millis(250),
            "{path}: {elapsed:?}"
        );
    }
    fast.stop();
}

#[test]
fn a_segment_with_only_a_tool_takes_the_same_time_on_every_api() {
    // 100 ms to the first token, then about 100 bytes of arguments at 100 tokens a second.
    let command = format!("echo {}", "x".repeat(80));
    let script = json!({
        "version": 1,
        "pace": {"first_token_ms": 100, "tokens_per_second": 100, "chunk_bytes": 16},
        "side_answer": "A short title",
        "turns": [{"segments": [
            {"text": "", "tool": {"role": "shell", "command": command}},
            {"text": "Done."}
        ]}]
    })
    .to_string();
    let handle = start_with(&script, 1.0, None);
    let times: Vec<(&str, Duration)> = requests(true)
        .iter()
        .map(|(path, body)| (*path, timed(handle.addrs[0], path, body)))
        .collect();
    let slowest = times.iter().map(|(_, t)| *t).max().unwrap();
    let fastest = times.iter().map(|(_, t)| *t).min().unwrap();
    assert!(fastest >= Duration::from_millis(300), "{times:?}");
    assert!(fastest.mul_f64(1.1) >= slowest, "{times:?}");
    handle.stop();
}

#[test]
fn a_speed_too_small_to_count_waits_instead_of_answering_at_once() {
    // The wait overflows a duration: it must saturate, not fall back on no wait at all.
    let handle = start_with(&script(300, 60.0, 16, "Hello."), 1e-300, None);
    let mut stream = TcpStream::connect(handle.addrs[0]).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    let body = anthropic_body(false, json!([bash_tool()]));
    stream
        .write_all(&request_bytes(
            "POST",
            "/v1/messages",
            &[("Content-Type", "application/json")],
            body.to_string().as_bytes(),
        ))
        .unwrap();
    let mut buffer = [0u8; 16];
    let error = stream.read(&mut buffer).unwrap_err();
    assert!(
        matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ),
        "{error:?}"
    );
    handle.stop();
}

#[test]
fn a_client_that_stops_reading_does_not_hold_its_thread_forever() {
    // 24 MB of text: far more than the sockets' buffers hold, so the writes block. A connection
    // timeout of one second stands for the binary's thirty.
    let text = "a".repeat(24 * 1024 * 1024);
    let handle = start_timed(
        &script(0, 1_000_000.0, 65_536, &text),
        f64::INFINITY,
        None,
        Duration::from_secs(1),
    );
    let mut client = TcpStream::connect(handle.addrs[0]).unwrap();
    let body = anthropic_body(true, json!([bash_tool()]));
    client
        .write_all(&request_bytes(
            "POST",
            "/v1/messages",
            &[("Content-Type", "application/json")],
            body.to_string().as_bytes(),
        ))
        .unwrap();
    // Writing a piece may take the timeout, and the thread gives up after the one that outlasts
    // it: twice the timeout at most. The client never reads.
    let begun = Instant::now();
    let entry = loop {
        if let Some(entry) = handle.journal().first().cloned() {
            break entry;
        }
        assert!(
            begun.elapsed() < Duration::from_secs(5),
            "the connection's thread still waits for a client that does not read"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(entry.kind, Kind::Main);
    assert!(entry.response_bytes > 0 && entry.response_bytes < text.len());
    drop(client);
    handle.stop();
}

#[test]
fn refused_requests_keep_their_method_and_path_when_the_head_was_read() {
    let handle = start_with(&script(0, 1e6, 16, "x"), f64::INFINITY, None);
    let addr = handle.addrs[0];

    let encoded = call(
        addr,
        "POST",
        "/v1/responses?x=1",
        &[("Content-Encoding", "zstd")],
        b"{}",
    );
    assert_eq!(encoded.status, 415);
    let entry = last(&handle);
    assert_eq!(
        (entry.kind, entry.method.as_str(), entry.path.as_str()),
        (Kind::Refused, "POST", "/v1/responses")
    );

    // The body is refused from its declared length, before any of it is sent.
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let head = format!(
        "POST /v1/messages HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        MAX_BODY + 1
    );
    stream.write_all(head.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    assert!(String::from_utf8_lossy(&raw).starts_with("HTTP/1.1 413"));
    let entry = last(&handle);
    assert_eq!(
        (
            entry.kind,
            entry.status,
            entry.method.as_str(),
            entry.path.as_str()
        ),
        (Kind::Refused, 413, "POST", "/v1/messages")
    );

    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(b"NONSENSE\r\n\r\n").unwrap();
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let entry = last(&handle);
    assert_eq!(
        (
            entry.kind,
            entry.status,
            entry.method.as_str(),
            entry.path.as_str()
        ),
        (Kind::Refused, 400, "", "")
    );
    handle.stop();
}

#[test]
fn a_preflight_answer_has_no_content_headers() {
    let handle = start_with(&script(0, 1e6, 16, "x"), f64::INFINITY, None);
    let preflight = call(handle.addrs[0], "OPTIONS", "/v1/messages", &[], b"");
    assert_eq!(preflight.status, 204);
    let head = preflight.head.to_ascii_lowercase();
    assert!(!head.contains("content-type"), "{}", preflight.head);
    assert!(!head.contains("content-length"), "{}", preflight.head);
    assert!(head.contains("access-control-allow-origin: *"));
    assert!(preflight.body.is_empty());
    handle.stop();
}

#[test]
fn a_request_in_flight_counts_for_the_next_ones_repeated_prefix() {
    // A streams for about two seconds. B starts while it does, and ends before it.
    let handle = start_with(
        &script(0, 4.0, 16, "Looking at it, and then some."),
        1.0,
        None,
    );
    let addr = handle.addrs[0];
    let stream_body = anthropic_body(true, json!([bash_tool()]));
    let long = std::thread::spawn(move || post(addr, "/v1/messages", &stream_body).status);
    std::thread::sleep(Duration::from_millis(300));
    let side = json!({"model": "m", "max_tokens": 5, "stream": false,
        "messages": [{"role": "user", "content": "[turn 0] x"}]});
    post(addr, "/v1/messages", &side);
    let entry = last(&handle);
    assert_eq!(entry.kind, Kind::Side);
    assert!(
        entry.repeated_prefix > 0,
        "the first request has not ended, but it was read before this one"
    );
    assert_eq!(long.join().unwrap(), 200);
    handle.stop();
}

#[test]
fn the_journal_lists_the_tools_a_request_offers() {
    let handle = start_with(&script(0, 1e6, 16, "x"), f64::INFINITY, None);
    let addr = handle.addrs[0];
    let read = json!({"name": "Read", "input_schema": {"type": "object", "properties": {"file_path": {"type": "string"}}, "required": ["file_path"]}});
    post(
        addr,
        "/v1/messages",
        &anthropic_body(false, json!([read, bash_tool()])),
    );
    let entry = journal_json(&last(&handle));
    assert_eq!(entry["tools_offered"], json!(["Read", "Bash"]));
    assert_eq!(entry["kind"], "main");

    call(addr, "GET", "/api/tags", &[], b"");
    assert_eq!(journal_json(&last(&handle))["tools_offered"], json!([]));
    handle.stop();
}

#[test]
fn a_request_is_main_only_if_it_offers_a_shell_or_a_read_tool() {
    let handle = start_with(&script(0, 1e6, 16, "The answer."), f64::INFINITY, None);
    let addr = handle.addrs[0];
    let structured_output = json!({"name": "StructuredOutput", "input_schema": {"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"]}});
    let reply = post(
        addr,
        "/v1/messages",
        &anthropic_body(false, json!([structured_output])),
    );
    assert_eq!(
        reply.json()["content"],
        json!([{"type": "text", "text": "A short title"}])
    );
    let entry = last(&handle);
    assert_eq!((entry.kind, entry.tool.as_deref()), (Kind::Side, None));
    assert_eq!(
        journal_json(&entry)["tools_offered"],
        json!(["StructuredOutput"])
    );

    let reply = post(
        addr,
        "/v1/messages",
        &anthropic_body(false, json!([structured_output, bash_tool()])),
    );
    assert_eq!(reply.json()["content"][0]["text"], "The answer.");
    assert_eq!(last(&handle).kind, Kind::Main);
    handle.stop();
}
