//! The chat mode (`--chat`), for chat applications that send no tool, and the journal's begin
//! lines and end times, which tell the bench which answers are in flight.

use pluto_fake_model::conversation::{Api, leading_turn_marker, read};
use pluto_fake_model::script::Script;
use pluto_fake_model::server::{CONNECTION_TIMEOUT, Config, Handle, Kind, spawn};
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
    ] },
    { "segments": [ { "text": "Turn one." } ] }
  ]
}"#;

fn start(script: &str, chat: bool, journal: Option<std::path::PathBuf>) -> Handle {
    let config = Config {
        script: Script::from_json(script).unwrap(),
        speed: 1.0,
        journal_path: journal,
        models: vec!["fake-model".into()],
        connection_timeout: CONNECTION_TIMEOUT,
        chat,
    };
    spawn(config, &["127.0.0.1:0".parse().unwrap()]).unwrap()
}

/// Posts a JSON body and returns the whole response, its head included: the tests look for the
/// answer's words in an answer that is not streamed, whose text is whole.
fn post(addr: SocketAddr, path: &str, body: &Value) -> String {
    let body = body.to_string();
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn chat_request(messages: Value) -> Value {
    json!({"model": "m", "stream": false, "messages": messages})
}

#[test]
fn a_chat_turn_gets_the_turns_texts_joined_without_its_tools() {
    let handle = start(SCRIPT, true, None);
    let response = post(
        handle.addrs[0],
        "/v1/chat/completions",
        &chat_request(json!([
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "[turn 0] Look at this project."}
        ])),
    );
    assert!(
        response.contains(r#"Looking at it.\n\nHere is **the answer**."#),
        "{response}"
    );
    assert!(!response.contains("tool_calls"), "{response}");
    let entry = handle.journal().last().cloned().unwrap();
    assert_eq!(entry.kind, Kind::Main);
    assert_eq!((entry.turn, entry.segment), (Some(0), Some(0)));
    assert_eq!(entry.tool, None);
    assert_eq!(entry.tool_result_ok, None);
    let begins = handle.begins();
    assert_eq!(begins.len(), 1);
    assert_eq!(begins[0].begin, entry.request);
    assert_eq!((begins[0].turn, begins[0].segment), (0, 0));
    assert_eq!(begins[0].api, Api::ChatCompletions);
    handle.stop();
}

#[test]
fn in_chat_mode_everything_else_is_a_side_request() {
    let handle = start(SCRIPT, true, None);
    let addr = handle.addrs[0];
    // A title request that wraps the prompt in its own template.
    let title = post(
        addr,
        "/v1/chat/completions",
        &chat_request(json!([
            {"role": "user", "content": "Write a title for this chat:\n[turn 0] Look at this project."}
        ])),
    );
    assert!(title.contains("A short title"), "{title}");
    // The marker in the history, but the last message is the assistant's.
    post(
        addr,
        "/v1/chat/completions",
        &chat_request(json!([
            {"role": "user", "content": "[turn 1] hello"},
            {"role": "assistant", "content": "Turn one."}
        ])),
    );
    // A tool offered and its result last: chat mode plays no tool loop.
    post(
        addr,
        "/v1/messages",
        &json!({"model": "m", "max_tokens": 5,
        "tools": [{"name": "Bash", "input_schema": {"type": "object"}}],
        "messages": [
            {"role": "user", "content": "[turn 0] x"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_fake_0_0", "name": "Bash", "input": {}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_fake_0_0", "content": "ok"}]}
        ]}),
    );
    let kinds: Vec<Kind> = handle.journal().iter().map(|entry| entry.kind).collect();
    assert_eq!(kinds, vec![Kind::Side, Kind::Side, Kind::Side]);
    assert!(handle.begins().is_empty());
    handle.stop();
}

#[test]
fn a_chat_turn_offering_tools_is_still_a_chat_turn() {
    let handle = start(SCRIPT, true, None);
    let response = post(
        handle.addrs[0],
        "/v1/messages",
        &json!({"model": "m", "max_tokens": 5,
            "tools": [{"name": "Bash", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": [{"type": "text", "text": "[turn 1] hi"}]}]}),
    );
    assert!(response.contains("Turn one."), "{response}");
    assert!(!response.contains("tool_use"), "{response}");
    let entry = handle.journal().last().cloned().unwrap();
    assert_eq!((entry.kind, entry.turn), (Kind::Main, Some(1)));
    handle.stop();
}

#[test]
fn without_chat_mode_a_bare_prompt_stays_a_side_request() {
    let handle = start(SCRIPT, false, None);
    post(
        handle.addrs[0],
        "/v1/chat/completions",
        &chat_request(json!([{"role": "user", "content": "[turn 0] title this"}])),
    );
    assert_eq!(handle.journal().last().unwrap().kind, Kind::Side);
    assert!(handle.begins().is_empty());
    handle.stop();
}

#[test]
fn a_main_request_is_journaled_when_it_begins_and_when_it_ends() {
    // About a second for the answer: long enough to see it in flight.
    let slow = SCRIPT.replace(
        "\"tokens_per_second\": 1000000",
        "\"tokens_per_second\": 10",
    );
    let dir = std::env::temp_dir().join(format!("pluto-fake-model-chat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.jsonl");
    let _ = std::fs::remove_file(&path);
    let handle = start(&slow, false, Some(path.clone()));
    let addr = handle.addrs[0];
    let request = std::thread::spawn(move || {
        post(
            addr,
            "/v1/messages",
            &json!({"model": "m", "max_tokens": 5, "stream": true,
                "tools": [{"name": "Bash", "input_schema": {"type": "object"}}],
                "messages": [{"role": "user", "content": "[turn 0] x"}]}),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while handle.begins().is_empty() {
        assert!(Instant::now() < deadline, "no begin line");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(handle.journal().is_empty(), "the answer has not ended yet");
    let in_flight = std::fs::read_to_string(&path).unwrap();
    assert!(in_flight.starts_with("{\"begin\":1,"), "{in_flight}");
    request.join().unwrap();

    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        json!({"begin": 1, "at_ms": lines[0]["at_ms"], "api": "anthropic", "turn": 0, "segment": 0})
    );
    assert_eq!(lines[1]["request"], 1);
    assert_eq!(lines[1]["kind"], "main");
    let (at, ended) = (
        lines[1]["at_ms"].as_u64().unwrap(),
        lines[1]["ended_ms"].as_u64().unwrap(),
    );
    assert!(ended >= at + 200, "began at {at}, ended at {ended}");
    handle.stop();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn chat_turns_are_read_from_the_last_item_of_each_api() {
    let turn = |api, body: Value| read(api, &body, 0).unwrap().chat_turn;
    assert_eq!(
        turn(Api::Responses, json!({"model": "m", "input": "[turn 2] a"})),
        Some(2)
    );
    assert_eq!(
        turn(
            Api::Responses,
            json!({"model": "m", "input": [{"role": "user", "content": [{"type": "input_text", "text": "[turn 3] a"}]}]})
        ),
        Some(3)
    );
    assert_eq!(
        turn(
            Api::Ollama,
            json!({"model": "m", "messages": [{"role": "user", "content": "[turn 1] a"}, {"role": "system", "content": "b"}]})
        ),
        Some(1),
        "an instruction after the prompt is not an item"
    );
    assert_eq!(
        turn(
            Api::Anthropic,
            json!({"model": "m", "messages": [{"role": "user", "content": " [turn 1] a"}]})
        ),
        None,
        "the marker must begin the message"
    );
    assert_eq!(
        turn(Api::ChatCompletions, json!({"model": "m", "messages": []})),
        None
    );
}

#[test]
fn a_leading_marker_is_the_first_thing_in_the_text() {
    assert_eq!(leading_turn_marker("[turn 4] a"), Some(4));
    assert_eq!(leading_turn_marker("[turn 4]"), Some(4));
    assert_eq!(leading_turn_marker("[turn x] [turn 4]"), None);
    assert_eq!(leading_turn_marker("a [turn 4]"), None);
    assert_eq!(leading_turn_marker("[turn 4"), None);
    assert_eq!(leading_turn_marker(""), None);
}

#[test]
fn a_chat_text_joins_the_turns_texts_and_wraps_around() {
    let script = Script::from_json(SCRIPT).unwrap();
    assert_eq!(
        script.chat_text(0),
        "Looking at it.\n\nHere is **the answer**."
    );
    assert_eq!(script.chat_text(3), "Turn one.");
}

/// The rivals' bench reads the journal with `grep` (scripts/rivals/lib.sh, validity.sh): the text of
/// its lines, field order included, is a contract, kept here and in test-validity.sh.
#[test]
fn the_journal_lines_keep_their_text() {
    let dir = std::env::temp_dir().join(format!("pluto-fake-model-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.jsonl");
    let _ = std::fs::remove_file(&path);
    let handle = start(SCRIPT, false, Some(path.clone()));
    post(
        handle.addrs[0],
        "/v1/messages",
        &json!({"model": "m", "max_tokens": 5,
            "tools": [{"name": "Bash", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "[turn 0] x"}]}),
    );
    handle.stop();
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    let at = lines[0]
        .strip_prefix("{\"begin\":1,\"at_ms\":")
        .and_then(|rest| rest.split_once(','))
        .map(|(ms, rest)| (ms.parse::<u64>().is_ok(), rest))
        .unwrap_or_else(|| panic!("{}", lines[0]));
    assert_eq!(
        at,
        (true, "\"api\":\"anthropic\",\"turn\":0,\"segment\":0}"),
        "{}",
        lines[0]
    );
    assert!(lines[1].starts_with("{\"request\":1,"), "{}", lines[1]);
    assert!(
        lines[1].contains("\"kind\":\"main\",\"status\":200,\"turn\":0,\"segment\":0,"),
        "{}",
        lines[1]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
