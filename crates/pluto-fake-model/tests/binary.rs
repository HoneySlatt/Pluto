use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_pluto-fake-model");
const WORKLOAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/workload.json");

#[test]
fn serves_the_workload_and_names_its_address() {
    let mut child = Command::new(BIN)
        .args([
            "--script",
            WORKLOAD,
            "--listen",
            "127.0.0.1:0",
            "--speed",
            "max",
            "--model",
            "m1",
        ])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    stderr.read_line(&mut line).unwrap();
    let addr = line
        .trim()
        .strip_prefix("pluto-fake-model: listening on http://")
        .expect(&line)
        .to_string();
    let mut stream = TcpStream::connect(&addr).unwrap();
    stream
        .write_all(b"GET /api/tags HTTP/1.1\r\nHost: x\r\n\r\n")
        .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(raw.starts_with("HTTP/1.1 200 OK"), "{raw}");
    assert!(raw.contains("\"m1\""), "{raw}");
}

#[test]
fn bad_command_lines_and_scripts() {
    let status = |args: &[&str]| {
        Command::new(BIN)
            .args(args)
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .code()
    };
    assert_eq!(status(&[]), Some(2));
    assert_eq!(status(&["--script"]), Some(2));
    assert_eq!(status(&["--script", WORKLOAD, "--speed", "fast"]), Some(2));
    assert_eq!(
        status(&["--script", WORKLOAD, "--listen", "nowhere"]),
        Some(2)
    );
    assert_eq!(status(&["--script", WORKLOAD, "--bogus"]), Some(2));
    assert_eq!(status(&["--script", "/nonexistent/script.json"]), Some(1));
    let dir = std::env::temp_dir().join(format!("pluto-fake-model-bin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.json");
    std::fs::write(&bad, r#"{"version": 9, "side_answer": "t", "turns": []}"#).unwrap();
    assert_eq!(status(&["--script", bad.to_str().unwrap()]), Some(1));
    std::fs::remove_dir_all(&dir).unwrap();
}
