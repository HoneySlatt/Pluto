//! The fake agent's logic (src/replay.rs): matching, placeholders and release.

use pluto_recording::format::{self, Recording};
use pluto_recording::replay::{self, LivePlaces, Output, ReplayError, Replayer};
use serde_json::{Value, json};

const CLAUDE: &str = r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"approval","meta":{}}
{"kind":"spawn","process":0,"t_ms":0,"argv":["claude","--output-format","stream-json","--session-id=<id:1>"]}
{"kind":"frame","process":0,"t_ms":1,"from":"client","json":{"type":"control_request","request_id":"<id:2>","request":{"subtype":"initialize","systemPrompt":"recorded prompt"}}}
{"kind":"frame","process":0,"t_ms":2,"from":"agent","json":{"type":"control_response","response":{"subtype":"success","request_id":"<id:2>","response":{}}}}
{"kind":"frame","process":0,"t_ms":3,"from":"client","json":{"type":"user","session_id":"","message":{"role":"user","content":[{"type":"text","text":"Run ls"}]},"parent_tool_use_id":null}}
{"kind":"frame","process":0,"t_ms":4,"from":"agent","json":{"type":"system","subtype":"init","session_id":"<id:1>","cwd":"<workspace>","transcript":"<home>/.claude/projects/<workspace>/<id:1>.jsonl"}}
{"kind":"line","process":0,"t_ms":5,"from":"stderr","text":"debug: started"}
{"kind":"frame","process":0,"t_ms":6,"from":"agent","json":{"type":"control_request","request_id":"tool-req-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}}
{"kind":"action","name":"approve","detail":{"tool":"Bash"}}
{"kind":"frame","process":0,"t_ms":7,"from":"client","json":{"type":"control_response","response":{"subtype":"success","request_id":"tool-req-1","response":{"behavior":"allow","updatedInput":{"command":"ls"}}}}}
{"kind":"frame","process":0,"t_ms":8,"from":"agent","json":{"type":"result","subtype":"success","session_id":"<id:1>","is_error":false}}
{"kind":"stdin_closed","process":0,"t_ms":9}
{"kind":"exit","process":0,"t_ms":10,"code":0}
"#;

const GROK: &str = r#"{"format":"pluto-agent-recording","version":1,"agent":"grok","protocol":"acp-json-rpc","recorded_on":"2026-11-12","scenario":"turn","meta":{}}
{"kind":"spawn","process":0,"t_ms":0,"argv":["grok","agent","stdio"]}
{"kind":"frame","process":0,"t_ms":1,"from":"client","json":{"jsonrpc":"2.0","id":"<id:1>","method":"initialize","params":{"protocolVersion":1}}}
{"kind":"frame","process":0,"t_ms":2,"from":"agent","json":{"jsonrpc":"2.0","id":"<id:1>","result":{"protocolVersion":1}}}
{"kind":"frame","process":0,"t_ms":3,"from":"client","json":{"jsonrpc":"2.0","id":"<id:2>","method":"session/prompt","params":{"sessionId":"g-1","prompt":[{"type":"text","text":"Say hi"}]}}}
{"kind":"frame","process":0,"t_ms":4,"from":"agent","json":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"g-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}}
{"kind":"line","process":0,"t_ms":5,"from":"agent","text":"not json on stdout"}
{"kind":"frame","process":0,"t_ms":6,"from":"agent","json":{"jsonrpc":"2.0","id":3,"method":"session/request_permission","params":{"sessionId":"g-1"}}}
{"kind":"frame","process":0,"t_ms":7,"from":"client","json":{"jsonrpc":"2.0","id":3,"result":{"outcome":{"outcome":"selected","optionId":"allow"}}}}
{"kind":"frame","process":0,"t_ms":8,"from":"agent","json":{"jsonrpc":"2.0","id":"<id:2>","result":{"stopReason":"end_turn"}}}
{"kind":"stdin_closed","process":0,"t_ms":9}
{"kind":"exit","process":0,"t_ms":10,"signal":15}
"#;

fn read(text: &str) -> Recording {
    format::read(text).unwrap()
}

fn places() -> LivePlaces {
    LivePlaces {
        workspace: Some("/tmp/live/ws".to_owned()),
        home: Some("/tmp/live/home".to_owned()),
    }
}

fn claude_args() -> Vec<String> {
    [
        "--output-format",
        "stream-json",
        "--session-id=live-session-9",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn stdout_json(outputs: &[Output]) -> Vec<Value> {
    outputs
        .iter()
        .filter_map(|output| match output {
            Output::Stdout(line) => serde_json::from_str(line).ok(),
            _ => None,
        })
        .collect()
}

fn line(value: Value) -> String {
    serde_json::to_string(&value).unwrap()
}

fn user(text: &str) -> String {
    line(
        json!({"type": "user", "session_id": "", "message": {"role": "user",
        "content": [{"type": "text", "text": text}]}, "parent_tool_use_id": null}),
    )
}

fn initialize(request_id: &str) -> String {
    line(json!({"type": "control_request", "request_id": request_id,
        "request": {"subtype": "initialize", "systemPrompt": "another client's prompt"}}))
}

fn approval(behavior: &str) -> String {
    line(
        json!({"type": "control_response", "response": {"subtype": "success",
        "request_id": "tool-req-1", "response": {"behavior": behavior, "updatedInput": {"command": "ls"}}}}),
    )
}

#[test]
fn nothing_comes_back_before_the_first_client_frame() {
    let (replayer, outputs) = Replayer::new(&read(CLAUDE), 0, &claude_args(), places()).unwrap();
    assert_eq!(outputs, vec![]);
    let status = replayer.status();
    assert_eq!(
        status.waiting_for.as_deref(),
        Some("control_request:initialize")
    );
    assert!(!status.complete);
}

#[test]
fn a_whole_claude_process_replays_with_live_ids_and_folders() {
    let (mut replayer, _) = Replayer::new(&read(CLAUDE), 0, &claude_args(), places()).unwrap();

    // The SDK may write the prompt before its initialize: nothing is released yet.
    assert_eq!(replayer.client_line(&user("Run ls")).unwrap(), vec![]);
    assert_eq!(
        replayer.client_line("   ").unwrap(),
        vec![],
        "blank lines are ignored"
    );

    let outputs = replayer.client_line(&initialize("live-req-1")).unwrap();
    let frames = stdout_json(&outputs);
    assert_eq!(frames.len(), 3, "{outputs:?}");
    assert_eq!(frames[0]["response"]["request_id"], "live-req-1");
    assert_eq!(frames[1]["session_id"], "live-session-9");
    assert_eq!(frames[1]["cwd"], "/tmp/live/ws");
    assert_eq!(
        frames[1]["transcript"],
        "/tmp/live/home/.claude/projects//tmp/live/ws/live-session-9.jsonl"
    );
    assert!(outputs.contains(&Output::Stderr("debug: started".to_owned())));
    assert_eq!(frames[2]["request"]["subtype"], "can_use_tool");

    let outputs = replayer.client_line(&approval("allow")).unwrap();
    assert_eq!(stdout_json(&outputs)[0]["session_id"], "live-session-9");

    let outputs = replayer.stdin_closed().unwrap();
    assert_eq!(
        outputs,
        vec![Output::Exit {
            code: Some(0),
            signal: None
        }]
    );
    let status = replayer.status();
    assert!(status.complete, "{status:?}");
    assert_eq!(status.consumed, status.total);
    assert_eq!(
        status.total, 11,
        "every record of the process but the action"
    );
}

#[test]
fn a_different_answer_is_a_mismatch() {
    let (mut replayer, _) = Replayer::new(&read(CLAUDE), 0, &claude_args(), places()).unwrap();
    replayer.client_line(&initialize("r")).unwrap();
    replayer.client_line(&user("Run ls")).unwrap();
    let error = replayer.client_line(&approval("deny")).unwrap_err();
    match error {
        ReplayError::Mismatch {
            kind,
            field,
            expected,
            found,
        } => {
            assert_eq!(kind, "control_response");
            assert_eq!(field, "response.response.behavior");
            assert_eq!(expected, "\"allow\"");
            assert_eq!(found, "\"deny\"");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_different_prompt_is_a_mismatch() {
    let (mut replayer, _) = Replayer::new(&read(CLAUDE), 0, &claude_args(), places()).unwrap();
    let error = replayer.client_line(&user("Run rm -rf")).unwrap_err();
    assert!(
        matches!(error, ReplayError::Mismatch { ref kind, .. } if kind == "user"),
        "{error:?}"
    );
}

#[test]
fn an_unrecorded_kind_is_unexpected() {
    let (mut replayer, _) = Replayer::new(&read(CLAUDE), 0, &claude_args(), places()).unwrap();
    let interrupt = line(json!({"type": "control_request", "request_id": "x",
        "request": {"subtype": "interrupt"}}));
    assert_eq!(
        replayer.client_line(&interrupt).unwrap_err(),
        ReplayError::Unexpected {
            kind: "control_request:interrupt".to_owned()
        }
    );
}

#[test]
fn a_placeholder_learned_twice_differently_is_a_conflict() {
    let text = CLAUDE.replace(
        r#""type":"user","session_id":"""#,
        r#""type":"user","session_id":"<id:1>""#,
    );
    let (mut replayer, _) = Replayer::new(&read(&text), 0, &claude_args(), places()).unwrap();
    let wrong = line(
        json!({"type": "user", "session_id": "other-session", "message": {
        "role": "user", "content": [{"type": "text", "text": "Run ls"}]}, "parent_tool_use_id": null}),
    );
    assert_eq!(
        replayer.client_line(&wrong).unwrap_err(),
        ReplayError::Conflict {
            placeholder: "<id:1>".to_owned()
        }
    );
}

#[test]
fn acp_ids_keep_their_live_type_and_text_lines_come_back_raw() {
    let recording = read(GROK);
    let args = ["agent", "stdio"].map(str::to_owned).to_vec();
    let (mut replayer, _) = Replayer::new(&recording, 0, &args, places()).unwrap();
    let outputs = replayer
        .client_line(&line(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": 1}})))
        .unwrap();
    assert_eq!(
        stdout_json(&outputs)[0]["id"],
        json!(0),
        "a number stays a number"
    );
    let outputs = replayer
        .client_line(&line(
            json!({"jsonrpc": "2.0", "id": 1, "method": "session/prompt",
            "params": {"sessionId": "g-1", "prompt": [{"type": "text", "text": "Say hi"}]}}),
        ))
        .unwrap();
    assert!(
        outputs.contains(&Output::Stdout("not json on stdout".to_owned())),
        "{outputs:?}"
    );
    let wrong =
        line(json!({"jsonrpc": "2.0", "id": 3, "result": {"outcome": {"outcome": "cancelled"}}}));
    assert!(matches!(
        replayer.client_line(&wrong).unwrap_err(),
        ReplayError::Mismatch { ref field, .. } if field == "result.outcome"
    ));
}

#[test]
fn an_acp_process_ends_killed_like_the_recorded_one() {
    let recording = read(GROK);
    let (mut replayer, _) = Replayer::new(&recording, 0, &[], places()).unwrap();
    replayer
        .client_line(&line(
            json!({"jsonrpc": "2.0", "id": "a", "method": "initialize", "params": {}}),
        ))
        .unwrap();
    replayer
        .client_line(&line(
            json!({"jsonrpc": "2.0", "id": "b", "method": "session/prompt",
            "params": {"prompt": [{"type": "text", "text": "Say hi"}]}}),
        ))
        .unwrap();
    let outputs = replayer
        .client_line(&line(json!({"jsonrpc": "2.0", "id": 3, "result": {"outcome": {"outcome": "selected", "optionId": "allow"}}})))
        .unwrap();
    assert_eq!(stdout_json(&outputs)[0]["id"], "b");
    assert_eq!(
        replayer.stdin_closed().unwrap(),
        vec![Output::Exit {
            code: None,
            signal: Some(15)
        }]
    );
    assert!(replayer.status().complete);
}

#[test]
fn a_missing_process_is_refused() {
    assert!(matches!(
        Replayer::new(&read(CLAUDE), 1, &[], places()),
        Err(ReplayError::NoSuchProcess(1))
    ));
}

#[test]
fn every_sample_replays_with_another_client() {
    replay::with_another_client(&read(CLAUDE)).unwrap();
    replay::with_another_client(&read(GROK)).unwrap();
}

#[test]
fn another_client_finds_a_recording_that_cannot_complete() {
    // A record after the exit is never given back: the process has ended.
    let broken = CLAUDE.replace(
        r#"{"kind":"exit","process":0,"t_ms":10,"code":0}"#,
        "{\"kind\":\"exit\",\"process\":0,\"t_ms\":10,\"code\":0}\n{\"kind\":\"frame\",\"process\":0,\"t_ms\":11,\"from\":\"agent\",\"json\":{\"type\":\"late\"}}",
    );
    assert!(replay::with_another_client(&read(&broken)).is_err());
}
