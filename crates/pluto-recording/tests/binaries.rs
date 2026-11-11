//! The two programs, run as a server would run them.
//!
//! `pluto-record record --out F --agent A --scenario S --workspace W --home H
//!  [--meta K=V]... [--keep-env NAME]... -- PROGRAM ARGS...`
//! `pluto-record check FILE...`
//! `pluto-record review FILE`
//! `pluto-record mask-document --workspace W --home H [--home H]...` (stdin to stdout)
//! `pluto-replay-agent --recording F --state D [--workspace W] [--home H] [--timeout-ms N] -- ARGS...`

use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use pluto_recording::format::{self, Record, Side, Stream};
use serde_json::{Value, json};

const RECORD: &str = env!("CARGO_BIN_EXE_pluto-record");
const REPLAY: &str = env!("CARGO_BIN_EXE_pluto-replay-agent");

fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Setup {
    out: PathBuf,
    workspace: PathBuf,
    home: PathBuf,
}

fn setup(name: &str) -> Setup {
    let dir = scratch(name);
    let workspace = dir.join("project");
    let home = dir.join("home");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    Setup {
        out: dir.join("rec.jsonl"),
        workspace,
        home,
    }
}

fn record_command(setup: &Setup, script: &str) -> Command {
    let mut command = Command::new(RECORD);
    command
        .args([
            "record",
            "--agent",
            "claude",
            "--scenario",
            "turn",
            "--meta",
            "sdk=test 1.0",
        ])
        .arg("--out")
        .arg(&setup.out)
        .arg("--workspace")
        .arg(&setup.workspace)
        .arg("--home")
        .arg(&setup.home)
        .args(["--", "sh", "-c", script, "fake-claude"])
        .current_dir(&setup.workspace)
        .env("SECRET_VAR", "must-not-reach-the-agent");
    command
}

const FAKE_AGENT: &str = r#"read line
printf '{"type":"system","subtype":"init","cwd":"%s","home":"%s","secret":"%s"}\n' "$PWD" "$HOME" "$SECRET_VAR"
echo oops >&2
echo '{"type":"result","token":"sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}'
echo 'plain text'
read rest
exit 3"#;

#[test]
fn the_recorder_forwards_raw_bytes_and_writes_a_masked_recording() {
    let setup = setup("record");
    let mut child = record_command(&setup, FAKE_AGENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"type\":\"user\",\"uuid\":\"u-123456\",\"message\":{\"content\":\"hi\"}}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(3),
        "the agent's exit code is the recorder's"
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("sk-ant-api03-AbCdEf"),
        "the server gets the raw bytes: {stdout}"
    );
    assert!(
        stdout.contains(&*setup.workspace.to_string_lossy()),
        "{stdout}"
    );
    assert!(stdout.ends_with("plain text\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "oops\n");

    let text = std::fs::read_to_string(&setup.out).unwrap();
    assert!(!text.contains("sk-ant-api03"), "{text}");
    assert!(
        !text.contains("must-not-reach"),
        "the environment is not passed: {text}"
    );
    let recording = format::read(&text).unwrap();
    assert_eq!(recording.header.scenario, "turn");
    assert_eq!(recording.header.meta["sdk"], "test 1.0");
    let records = &recording.records;
    assert!(matches!(&records[0], Record::Spawn { process: 0, argv, .. } if argv[0] == "sh"));
    let frames: Vec<(&Side, &Value, bool)> = records
        .iter()
        .filter_map(|record| match record {
            Record::Frame {
                from, json, masked, ..
            } => Some((from, json, *masked)),
            _ => None,
        })
        .collect();
    assert_eq!(frames[0].0, &Side::Client);
    assert_eq!(frames[0].1["uuid"], "<id:1>");
    assert_eq!(frames[1].1["cwd"], "<workspace>");
    assert_eq!(
        frames[1].1["home"], "<home>",
        "the agent runs with the disposable home"
    );
    assert_eq!(frames[1].1["secret"], "");
    assert!(frames[2].2, "the token frame is flagged as masked");
    assert!(records.iter().any(|record| matches!(record,
        Record::Line { from: Stream::Stderr, text, .. } if text == "oops")));
    assert!(records.iter().any(|record| matches!(record,
        Record::Line { from: Stream::Agent, text, .. } if text == "plain text")));
    // Each line is written before it is forwarded, so a frame never follows its answer; the end
    // of stdin may come before or after the agent's last lines.
    let client = records.iter().position(|record| {
        matches!(
            record,
            Record::Frame {
                from: Side::Client,
                ..
            }
        )
    });
    let closed = records
        .iter()
        .position(|record| matches!(record, Record::StdinClosed { process: 0, .. }));
    assert!(client.unwrap() < closed.unwrap());
    assert!(matches!(
        records.last(),
        Some(Record::Exit {
            process: 0,
            code: Some(3),
            signal: None,
            ..
        })
    ));
}

#[test]
fn a_second_spawn_appends_a_second_process_under_one_header() {
    let setup = setup("append");
    for _ in 0..2 {
        let status = record_command(&setup, "exit 0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }
    let text = std::fs::read_to_string(&setup.out).unwrap();
    assert_eq!(text.matches("pluto-agent-recording").count(), 1, "{text}");
    let recording = format::read(&text).unwrap();
    let spawns: Vec<u32> = recording
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Spawn { process, .. } => Some(*process),
            _ => None,
        })
        .collect();
    assert_eq!(spawns, [0, 1]);
}

#[test]
fn a_signal_to_the_recorder_reaches_the_agent_and_is_recorded() {
    let setup = setup("signal");
    let mut child = record_command(&setup, "echo '{\"type\":\"ready\"}'; exec sleep 30")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 17];
    std::io::Read::read_exact(child.stdout.as_mut().unwrap(), &mut first).unwrap();
    // SAFETY: kill only sends a signal to the recorder this test spawned, by its pid.
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    let status = child.wait().unwrap();
    assert_eq!(
        status.signal(),
        Some(15),
        "the recorder ends like the agent: {status:?}"
    );
    let recording = format::read(&std::fs::read_to_string(&setup.out).unwrap()).unwrap();
    assert!(matches!(
        recording.records.last(),
        Some(Record::Exit {
            signal: Some(15),
            ..
        })
    ));
}

#[test]
fn check_passes_a_clean_file_and_names_a_leak_without_showing_it() {
    let dir = scratch("check");
    let header = r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"turn","meta":{}}"#;
    let clean = dir.join("clean.jsonl");
    std::fs::write(
        &clean,
        format!("{header}\n{{\"kind\":\"exit\",\"process\":0,\"t_ms\":1,\"code\":0}}\n"),
    )
    .unwrap();
    let leaky = dir.join("leaky.jsonl");
    std::fs::write(
        &leaky,
        format!("{header}\n{{\"kind\":\"line\",\"process\":0,\"t_ms\":1,\"from\":\"stderr\",\"text\":\"mail alice@corp.example\"}}\n"),
    )
    .unwrap();
    let ok = Command::new(RECORD)
        .arg("check")
        .arg(&clean)
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let bad = Command::new(RECORD)
        .arg("check")
        .arg(&clean)
        .arg(&leaky)
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    let report =
        String::from_utf8_lossy(&bad.stdout).to_string() + &String::from_utf8_lossy(&bad.stderr);
    assert!(report.contains("leaky.jsonl:2"), "{report}");
    assert!(report.contains("email"), "{report}");
    assert!(!report.contains("corp.example"), "{report}");
    let newer = dir.join("newer.jsonl");
    std::fs::write(&newer, header.replace("\"version\":1", "\"version\":9")).unwrap();
    let refused = Command::new(RECORD)
        .arg("check")
        .arg(&newer)
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("version 9"));
}

#[test]
fn mask_document_masks_places_and_shapes() {
    let mut child = Command::new(RECORD)
        .args(["mask-document", "--workspace", "/tmp/w", "--home", "/tmp/h"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"text":"/tmp/w/a.rs and xai-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let text = value["text"].as_str().unwrap();
    assert!(text.starts_with("<workspace>/a.rs and <masked:"), "{text}");
}

const CLAUDE: &str = r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"turn","meta":{}}
{"kind":"spawn","process":0,"t_ms":0,"argv":["claude","--session-id=<id:1>"]}
{"kind":"frame","process":0,"t_ms":1,"from":"client","json":{"type":"user","message":{"content":"hi"}}}
{"kind":"frame","process":0,"t_ms":2,"from":"agent","json":{"type":"result","session_id":"<id:1>","cwd":"<workspace>"}}
{"kind":"line","process":0,"t_ms":3,"from":"stderr","text":"bye"}
{"kind":"stdin_closed","process":0,"t_ms":4}
{"kind":"exit","process":0,"t_ms":5,"code":7}
"#;

fn replay_command(dir: &Path, recording: &Path, timeout_ms: u32) -> Command {
    let mut command = Command::new(REPLAY);
    command
        .arg("--recording")
        .arg(recording)
        .arg("--state")
        .arg(dir.join("state"))
        .args(["--workspace", "/live/ws", "--home", "/live/home"])
        .arg("--timeout-ms")
        .arg(timeout_ms.to_string())
        .args(["--", "--session-id=live-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn status_file(dir: &Path, process: u32) -> Value {
    let text =
        std::fs::read_to_string(dir.join("state").join(format!("process-{process}.json"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn the_fake_agent_plays_a_process_and_ends_like_it() {
    let dir = scratch("replay");
    let recording = dir.join("rec.jsonl");
    std::fs::write(&recording, CLAUDE).unwrap();
    let mut child = replay_command(&dir, &recording, 2000).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(7));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let frame: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(
        frame,
        json!({"type": "result", "session_id": "live-s", "cwd": "/live/ws"})
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "bye\n");
    let status = status_file(&dir, 0);
    assert_eq!(status["complete"], true, "{status}");
    assert_eq!(status["error"], Value::Null);

    // The next spawn takes the next process, which this recording does not have.
    let second = replay_command(&dir, &recording, 2000)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(second.status.code(), Some(3));
    assert!(
        status_file(&dir, 1)["error"]
            .as_str()
            .unwrap()
            .contains("no process 1")
    );
}

#[test]
fn the_fake_agent_fails_fast_on_a_mismatch_and_on_silence() {
    let dir = scratch("replay-fail");
    let recording = dir.join("rec.jsonl");
    std::fs::write(&recording, CLAUDE).unwrap();
    let mut child = replay_command(&dir, &recording, 2000).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(b"{\"type\":\"user\",\"message\":{\"content\":\"bye\"}}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    drop(stdin);
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("differs"));
    assert_eq!(status_file(&dir, 0)["complete"], false);

    let silent = scratch("replay-silent");
    let recording = silent.join("rec.jsonl");
    std::fs::write(&recording, CLAUDE).unwrap();
    let mut child = replay_command(&silent, &recording, 200).spawn().unwrap();
    let started = std::time::Instant::now();
    let _stdin = child.stdin.take().unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(4));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(status_file(&silent, 0)["waiting_for"], "user");
}

/// The account's home in the user database, read the way a test can: it does not depend on
/// `HOME`, which a server may point at the disposable home.
fn account_home() -> String {
    // SAFETY: getpwuid returns null or an entry in static storage valid until the next getpw*
    // call; this test copies the folder out at once and calls nothing else in between.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        assert!(!entry.is_null() && !(*entry).pw_dir.is_null());
        std::ffi::CStr::from_ptr((*entry).pw_dir)
            .to_string_lossy()
            .into_owned()
    }
}

#[test]
fn the_real_home_is_masked_even_when_home_is_the_disposable_one() {
    let real = account_home();
    let mut child = Command::new(RECORD)
        .args([
            "mask-document",
            "--workspace",
            "/tmp/w",
            "--home",
            "/tmp/disposable",
        ])
        .env_clear()
        .env("HOME", "/tmp/disposable")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!(r#"{{"text":"{real}/notes and /tmp/disposable/x"}}"#).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["text"], "<home>/notes and <home>/x");
}

#[test]
fn remask_changes_only_what_a_new_rule_covers() {
    let dir = scratch("remask");
    let synthetic = Path::new(env!("CARGO_MANIFEST_DIR")).join("fork-harness/synthetic");
    let mut copies = Vec::new();
    for agent in ["claude", "grok"] {
        for entry in std::fs::read_dir(synthetic.join(agent)).unwrap() {
            let path = entry.unwrap().path();
            let copy = dir.join(format!(
                "{agent}-{}",
                path.file_name().unwrap().to_string_lossy()
            ));
            std::fs::copy(&path, &copy).unwrap();
            copies.push((path, copy));
        }
    }
    let header = r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"turn","meta":{}}"#;
    let older = dir.join("older.jsonl");
    std::fs::write(
        &older,
        format!(
            "{header}\n{}\n",
            r#"{"kind":"frame","process":0,"t_ms":1,"from":"agent","json":{"type":"system","subtype":"commands_changed","commands":[{"name":"obsidian-vault"}]}}"#
        ),
    )
    .unwrap();
    let status = Command::new(RECORD)
        .arg("remask")
        .args(copies.iter().map(|(_, copy)| copy))
        .arg(&older)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    for (original, copy) in &copies {
        assert_eq!(
            std::fs::read_to_string(original).unwrap(),
            std::fs::read_to_string(copy).unwrap(),
            "{}: masking a masked recording again changes nothing",
            original.display()
        );
    }
    let remasked = std::fs::read_to_string(&older).unwrap();
    assert!(!remasked.contains("obsidian-vault"), "{remasked}");
    assert!(remasked.contains(r#""masked":true"#), "{remasked}");
}
