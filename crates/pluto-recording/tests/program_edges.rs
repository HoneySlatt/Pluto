//! The programs at their edges: a recorder whose agent left a process holding its output, and a
//! fake agent given a recorded signal it cannot or must not raise.

use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RECORD: &str = env!("CARGO_BIN_EXE_pluto-record");
const REPLAY: &str = env!("CARGO_BIN_EXE_pluto-replay-agent");

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_recorder_obeys_a_signal_after_its_agent_ended() {
    let dir = scratch("orphan");
    let mut child = Command::new(RECORD)
        .args(["record", "--agent", "claude", "--scenario", "x"])
        .arg("--out")
        .arg(dir.join("rec.jsonl"))
        .arg("--workspace")
        .arg(&dir)
        .arg("--home")
        .arg(&dir)
        .args(["--", "sh", "-c", "sleep 5 & echo hi; exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 3];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut first)
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    // SAFETY: kill only signals the recorder this test spawned, by its pid.
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    let status = child.wait().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert!(
        status.signal() == Some(libc::SIGTERM) || status.code().is_some(),
        "{status:?}"
    );
}

fn replay_exit_with(signal: &str) -> std::process::ExitStatus {
    let dir = scratch(&format!("sig{}", signal.len()));
    let recording = dir.join("rec.jsonl");
    std::fs::write(
        &recording,
        format!(
            "{}\n{}\n{{\"kind\":\"exit\",\"process\":0,\"t_ms\":1,\"signal\":{signal}}}\n",
            r#"{"format":"pluto-agent-recording","version":1,"agent":"claude","protocol":"claude-stream-json","recorded_on":"2026-11-12","scenario":"x","meta":{}}"#,
            r#"{"kind":"spawn","process":0,"t_ms":0,"argv":["claude"]}"#
        ),
    )
    .unwrap();
    let mut child = Command::new(REPLAY)
        .arg("--recording")
        .arg(&recording)
        .arg("--state")
        .arg(dir.join("state"))
        .args(["--timeout-ms", "2000", "--"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            panic!("the fake agent did not end for signal {signal}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_malformed_recorded_signal_neither_panics_nor_stops_the_fake_agent() {
    for signal in ["2147483647", "19", "0"] {
        let status = replay_exit_with(signal);
        assert_ne!(status.code(), Some(101), "panic for {signal}: {status:?}");
    }
}
