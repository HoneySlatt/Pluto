//! Secrets in harder places (after an escape or URL encoding, Basic credentials, passwords in
//! URLs, secret keys inside text), account values learned once stripped, and inputs refused: a
//! recording without a process, relative folders.

use std::process::Command;

use pluto_recording::format::{self, Agent, Side};
use pluto_recording::mask::{Masker, Places};
use pluto_recording::{replay, scan};
use serde_json::{Value, json};

const RECORD: &str = env!("CARGO_BIN_EXE_pluto-record");

fn claude() -> Masker {
    Masker::new(Agent::Claude, Places::default())
}

fn text_of(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn a_token_after_an_escape_or_url_encoding_is_masked() {
    for (holder, secret) in [
        (
            "a\\nsk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
            "AbCdEfGhIjKlMnOp",
        ),
        (
            "a\\tghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
            "AbCdEfGhIjKlMnOp",
        ),
        (
            "k%3Dsk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
            "AbCdEfGhIjKlMnOp",
        ),
        ("x\\nBearer abcdefgh1234", "abcdefgh1234"),
        ("?h=Bearer%20abcdefgh12345678", "abcdefgh12345678"),
    ] {
        let mut frame = json!({"type": "result", "text": holder});
        assert!(claude().frame(Side::Agent, &mut frame), "{holder}");
        assert!(
            !text_of(&frame).contains(secret),
            "{holder} -> {}",
            text_of(&frame)
        );
    }
}

#[test]
fn the_scan_finds_a_bearer_after_an_escape_or_encoding() {
    for leak in ["x\\nBearer abcdefgh1234", "?h=Bearer%20abcdefgh12345678"] {
        let found = scan::document(&json!({"text": leak}));
        assert!(!found.is_empty(), "{leak}");
    }
}

#[test]
fn stripped_account_values_are_learned() {
    let mut masker = claude();
    let mut answer = json!({"type": "control_response", "response": {"subtype": "success",
        "request_id": "r1", "response": {"account": {"email": "alice@corp.example",
        "organization": "Alice Corp Ltd"}}}});
    masker.frame(Side::Agent, &mut answer);
    let mut later = json!({"type": "assistant", "message": {"content": [{"type": "text",
        "text": "You are alice@corp.example at Alice Corp Ltd"}]}});
    assert!(masker.frame(Side::Agent, &mut later));
    let text = text_of(&later);
    assert!(!text.contains("corp.example"), "{text}");
    assert!(!text.contains("Alice Corp Ltd"), "{text}");
}

#[test]
fn grok_hostname_is_learned() {
    let mut masker = Masker::new(Agent::Grok, Places::default());
    let mut init =
        json!({"jsonrpc": "2.0", "id": 0, "result": {"_meta": {"hostname": "build-box-77"}}});
    masker.frame(Side::Agent, &mut init);
    let (line, masked) = masker.line("connected to build-box-77");
    assert!(masked);
    assert!(!line.contains("build-box-77"), "{line}");
}

#[test]
fn basic_auth_and_passwords_in_urls_are_masked() {
    let (line, masked) = claude().line("Authorization: Basic dXNlcjpodW50ZXIyaHVudGVyMg==");
    assert!(masked);
    assert!(!line.contains("dXNlcjpodW50"), "{line}");
    let (line, masked) = claude().line("clone https://bob:hunter2hunter2@git.example.com/r.git");
    assert!(masked);
    assert!(!line.contains("hunter2hunter2"), "{line}");
}

#[test]
fn secret_keys_inside_text_are_masked() {
    let (line, masked) = claude().line(r#"{"level":"warn","api_key":"plainsecretvalue"}"#);
    assert!(masked);
    assert!(!line.contains("plainsecretvalue"), "{line}");
    let mut frame = json!({"type": "result", "text": "export PASSWORD=correcthorsebattery; API_KEY=\"abcd1234efgh\""});
    assert!(claude().frame(Side::Agent, &mut frame));
    let text = text_of(&frame);
    assert!(!text.contains("correcthorsebattery"), "{text}");
    assert!(!text.contains("abcd1234efgh"), "{text}");
}

#[test]
fn a_recording_without_a_process_proves_nothing() {
    let header = r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"turn","meta":{}}"#;
    let recording = format::read(header).unwrap();
    assert!(replay::with_another_client(&recording).is_err());
}

#[test]
fn relative_folders_are_refused() {
    for (flag, value) in [("--workspace", "."), ("--home", "home")] {
        let mut command = Command::new(RECORD);
        command.args([
            "record",
            "--out",
            "/tmp/never-written.jsonl",
            "--agent",
            "claude",
            "--scenario",
            "x",
        ]);
        if flag == "--workspace" {
            command.args(["--workspace", value, "--home", "/tmp/h"]);
        } else {
            command.args(["--workspace", "/tmp/w", "--home", value]);
        }
        let output = command.args(["--", "true"]).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{flag} {value}");
        let output = Command::new(RECORD)
            .args(["mask-document", flag, value])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "mask-document {flag} {value}"
        );
    }
}
