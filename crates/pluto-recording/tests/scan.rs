//! The scan's contract (src/scan.rs): its detectors and the joined streams.

use pluto_recording::format::{Agent, Header, Record, Recording, Side, Stream};
use pluto_recording::scan;
use serde_json::{Value, json};

fn recording(frames: Vec<Value>) -> Recording {
    Recording {
        header: Header::new(Agent::Claude, "turn", "2026-11-12"),
        records: frames
            .into_iter()
            .enumerate()
            .map(|(index, json)| Record::Frame {
                process: 0,
                t_ms: index as u64,
                from: Side::Agent,
                json,
                masked: false,
            })
            .collect(),
    }
}

fn detectors(found: &[scan::Finding]) -> Vec<&'static str> {
    found.iter().map(|finding| finding.detector).collect()
}

fn text(value: &str) -> Value {
    json!({"type": "assistant", "message": {"content": [{"type": "text", "text": value}]}})
}

#[test]
fn a_clean_recording_has_no_finding() {
    let clean = recording(vec![
        text("Edited <workspace>/src/main.rs; see <home>/.config. Mail recorder@example.invalid."),
        json!({"type": "assistant", "message": {"id": "msg_01XyZaBcDeFgHiJkLmNoPqRsTuVw",
            "content": [{"type": "tool_use", "id": "toolu_01AbCdEfGhIjKlMnOpQrStUvWx"}]}}),
        json!({"type": "assistant", "message": {"content": [{"type": "thinking", "thinking": "hm",
            "signature": "EqQBCkgIBhABGAIiQL7xZp0Kq3N2vB8mW4tY9cR1sD6fG5hJ2kL8nM0pQ3rS7tU1vW4xY6zA"}]}}),
        text(
            "commit 9fceb02d0ae598e95dc970b74767f19372d61af8 and 3f1c2a9e-0000-4000-8000-123456789abc",
        ),
        text("Co-Authored-By: someone <noreply@anthropic.com>"),
    ]);
    assert_eq!(scan::recording(&clean), vec![]);
}

#[test]
fn an_email_is_found_with_its_line_and_without_its_value() {
    let found = scan::recording(&recording(vec![
        text("ok"),
        text("write to alice@corp.example"),
    ]));
    assert_eq!(detectors(&found), ["email"]);
    assert_eq!(found[0].line, 3, "the header is line 1");
    assert!(!found[0].hint.contains("corp.example"), "{}", found[0].hint);
}

#[test]
fn home_paths_in_every_form_are_found() {
    for leak in [
        "/home/alice/project",
        "/Users/alice/project",
        "/root/.ssh",
        "%2Fhome%2Falice%2Fx",
        "projects/-home-alice-project/s.jsonl",
    ] {
        let found = scan::recording(&recording(vec![text(leak)]));
        assert_eq!(detectors(&found), ["home-path"], "{leak}");
    }
}

#[test]
fn known_key_shapes_are_found() {
    let found = scan::recording(&recording(vec![text(
        "key sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
    )]));
    assert!(detectors(&found).contains(&"key-shape"), "{found:?}");
}

#[test]
fn a_random_token_is_found_but_not_hex_or_uuids() {
    let found = scan::recording(&recording(vec![text(
        "token Zx9Qw3Er7Ty1Ui5Op2As8Df4Gh6Jk0Lz3Xc",
    )]));
    assert_eq!(detectors(&found), ["random"]);
    let hex = scan::recording(&recording(vec![text(
        "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678abcdef",
    )]));
    assert_eq!(hex, vec![]);
}

#[test]
fn a_secret_split_across_streamed_deltas_is_found() {
    let delta = |text: &str| {
        json!({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": text}}})
    };
    let split = recording(vec![
        json!({"type": "stream_event", "event": {"type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}}}),
        delta("mail me at alice@co"),
        delta("rp.exa"),
        delta("mple please"),
    ]);
    assert_eq!(detectors(&scan::recording(&split)), ["email"]);
}

#[test]
fn acp_message_chunks_are_joined_too() {
    let chunk = |text: &str| {
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s",
            "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}})
    };
    let mut split = recording(vec![chunk("see /ho"), chunk("me/alice/x")]);
    split.header = Header::new(Agent::Grok, "turn", "2026-11-12");
    assert_eq!(detectors(&scan::recording(&split)), ["home-path"]);
}

#[test]
fn stderr_lines_and_arguments_are_scanned() {
    let mut leaky = recording(vec![]);
    leaky.records.push(Record::Spawn {
        process: 0,
        t_ms: 0,
        argv: vec![
            "claude".to_owned(),
            "--settings=/home/alice/s.json".to_owned(),
        ],
    });
    leaky.records.push(Record::Line {
        process: 0,
        t_ms: 1,
        from: Stream::Stderr,
        text: "fail at /home/alice/x".to_owned(),
        masked: false,
    });
    assert_eq!(
        detectors(&scan::recording(&leaky)),
        ["home-path", "home-path"]
    );
}

#[test]
fn a_document_is_scanned() {
    let found = scan::document(&json!({"a": ["alice@corp.example"]}));
    assert_eq!(detectors(&found), ["email"]);
    assert_eq!(found[0].line, 1);
}
