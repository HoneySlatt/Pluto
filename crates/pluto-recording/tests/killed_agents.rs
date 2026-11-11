//! Agents the fork kills when it closes a session (its ACP runtime kills the whole process tree),
//! and the trace ids it adds to every ACP frame.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use pluto_recording::format::{self, Agent, Side};
use pluto_recording::mask::{Masker, Places};
use pluto_recording::replay;
use serde_json::{Value, json};

const REPLAY: &str = env!("CARGO_BIN_EXE_pluto-replay-agent");

/// A Grok process the client killed after its last answer: no `stdin_closed`, no `exit`.
const KILLED: &str = r#"{"format":"pluto-agent-recording","version":1,"agent":"grok","protocol":"acp-json-rpc","recorded_on":"2026-11-12","scenario":"turn","meta":{}}
{"kind":"spawn","process":0,"t_ms":0,"argv":["grok","agent","stdio"]}
{"kind":"frame","process":0,"t_ms":1,"from":"client","json":{"jsonrpc":"2.0","id":"<id:1>","method":"initialize","params":{},"traceId":"<id:2>","spanId":"<id:3>"}}
{"kind":"frame","process":0,"t_ms":2,"from":"agent","json":{"jsonrpc":"2.0","id":"<id:1>","result":{}}}
{"kind":"frame","process":0,"t_ms":3,"from":"client","json":{"jsonrpc":"2.0","id":"<id:4>","method":"session/close","params":{},"traceId":"<id:5>","spanId":"<id:6>"}}
{"kind":"frame","process":0,"t_ms":4,"from":"agent","json":{"jsonrpc":"2.0","id":"<id:4>","result":{}}}
"#;

#[test]
fn the_forks_trace_and_span_ids_become_placeholders() {
    let mut masker = Masker::new(Agent::Grok, Places::default());
    let mut request = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {},
        "traceId": "fa7064c380a467087d4b0c0fbfaa2084", "spanId": "4d70bb1f5156cf47"});
    masker.frame(Side::Client, &mut request);
    assert_eq!(request["traceId"], "<id:2>");
    assert_eq!(request["spanId"], "<id:3>");
}

#[test]
fn a_killed_process_is_complete_once_every_record_is_consumed() {
    let recording = format::read(KILLED).unwrap();
    replay::with_another_client(&recording).unwrap();
}

#[test]
fn a_killed_fake_agent_leaves_its_status() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("killed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let recording = dir.join("rec.jsonl");
    std::fs::write(&recording, KILLED).unwrap();
    let mut child = Command::new(REPLAY)
        .arg("--recording")
        .arg(&recording)
        .arg("--state")
        .arg(dir.join("state"))
        .args(["--", "agent", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut answer = String::new();
    for request in [
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}, "traceId": "t1", "spanId": "s1"}),
        json!({"jsonrpc": "2.0", "id": 1, "method": "session/close", "params": {}, "traceId": "t2", "spanId": "s2"}),
    ] {
        writeln!(stdin, "{request}").unwrap();
        answer.clear();
        stdout.read_line(&mut answer).unwrap();
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let status: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("state").join("process-0.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(status["complete"], true, "{status}");
}
