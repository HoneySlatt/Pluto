//! Tests added by the review: the command line's `--speed`.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_pluto-fake-model");
const WORKLOAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/workload.json");

/// The exit code of a run, or `None` if the server started listening (so the command line was
/// accepted); it is then stopped at once.
fn exit_code(speed: &str) -> Option<i32> {
    let mut child = Command::new(BIN)
        .args([
            "--script",
            WORKLOAD,
            "--listen",
            "127.0.0.1:0",
            "--speed",
            speed,
        ])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The first line on stderr is either the address it listens on, or the error it exits with.
    let mut line = String::new();
    BufReader::new(child.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    if line.starts_with("pluto-fake-model: listening on") {
        child.kill().unwrap();
        child.wait().unwrap();
        return None;
    }
    child.wait().unwrap().code()
}

#[test]
fn a_speed_must_be_a_finite_number_above_zero_or_max() {
    for refused in ["0", "-1", "nan", "inf", "infinity", "1e999", "fast", ""] {
        assert_eq!(exit_code(refused), Some(2), "--speed {refused:?}");
    }
    for accepted in ["max", "0.5", "10"] {
        assert_eq!(exit_code(accepted), None, "--speed {accepted:?}");
    }
}
