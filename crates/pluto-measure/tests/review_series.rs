//! Findings of the review that concern a run: a subject that ends before the end, `--out`, and
//! the command line.

use pluto_measure::sample::Reader;
use pluto_measure::series::{Schedule, take_series};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_pluto-measure");

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pluto-review-{name}-{}", std::process::id()))
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn a_cgroup_removed_mid_series_ends_it_with_what_was_taken() {
    let root = temp("early");
    let _ = fs::remove_dir_all(&root);
    let cgroup = root.join("cgroup");
    fs::create_dir_all(&cgroup).unwrap();
    fs::create_dir_all(root.join("proc")).unwrap();
    fs::write(cgroup.join("cgroup.procs"), "").unwrap();
    fs::write(cgroup.join("cpu.stat"), "usage_usec 1\n").unwrap();
    let to_remove = cgroup.clone();
    let remover = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(350));
        fs::remove_dir_all(to_remove).unwrap();
    });
    let reader = Reader::new(&root.join("proc"), &cgroup);
    let schedule = Schedule {
        seconds: 2,
        interval_ms: 100,
        memory_every: 5,
    };
    let started = Instant::now();
    let series = take_series(&reader, &schedule).unwrap();
    remover.join().unwrap();
    let _ = fs::remove_dir_all(&root);
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "it did not wait to the end"
    );
    assert!(series.samples.len() >= 2 && series.samples.len() < 21);
    let ended = series.ended_early_at_ms.expect("ended early");
    assert!(ended >= series.samples.last().unwrap().at_ms);
}

#[test]
fn a_cgroup_that_cannot_be_read_at_first_is_an_error() {
    let root = temp("never");
    let reader = Reader::new(&root.join("proc"), &root.join("nowhere"));
    let schedule = Schedule {
        seconds: 1,
        interval_ms: 100,
        memory_every: 5,
    };
    assert!(take_series(&reader, &schedule).is_err());
}

#[test]
fn a_full_series_does_not_end_early() {
    let root = temp("full");
    let _ = fs::remove_dir_all(&root);
    let cgroup = root.join("cgroup");
    fs::create_dir_all(&cgroup).unwrap();
    fs::create_dir_all(root.join("proc")).unwrap();
    fs::write(cgroup.join("cgroup.procs"), "").unwrap();
    fs::write(cgroup.join("cpu.stat"), "usage_usec 1\n").unwrap();
    let reader = Reader::new(&root.join("proc"), &cgroup);
    let schedule = Schedule {
        seconds: 1,
        interval_ms: 250,
        memory_every: 2,
    };
    let series = take_series(&reader, &schedule).unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(series.samples.len(), 5);
    assert_eq!(series.ended_early_at_ms, None);
    assert!(series.samples[0].read_memory);
    assert!(!series.samples[1].read_memory);
    assert!(series.samples[2].read_memory);
    assert!(series.samples[4].read_memory, "the last one reads memory");
}

/// The test's own cgroup: sampling it reads only the tests' processes, never the whole machine's.
fn own_cgroup() -> String {
    cgroup_of("self").expect("the test runs in a cgroup")
}

/// The cgroup of a process, as `/sys/fs/cgroup` shows it.
fn cgroup_of(pid: impl std::fmt::Display) -> Option<String> {
    let text = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    let path = text.lines().find_map(|line| line.strip_prefix("0::"))?;
    Some(format!("/sys/fs/cgroup{path}"))
}

#[test]
fn a_scope_that_ends_early_still_gives_its_figures() {
    let out = temp("scope.json");
    let mut scope = match Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "--", "sleep", "1"])
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("systemd-run is not available here: {error}");
            return;
        }
    };
    // `systemd-run --scope` moves itself into the scope, then becomes the command.
    let started = Instant::now();
    let own_cgroup = cgroup_of("self");
    let cgroup = loop {
        // The test may itself run in a scope: wait for the child to leave it for its own.
        match cgroup_of(scope.id()) {
            Some(cgroup) if cgroup.ends_with(".scope") && Some(&cgroup) != own_cgroup.as_ref() => {
                break cgroup;
            }
            _ => {}
        }
        // A child that already ended never reached a scope: no user manager here (as on CI).
        let ended = matches!(scope.try_wait(), Ok(Some(_)));
        if ended || started.elapsed() > Duration::from_secs(5) {
            let _ = scope.kill();
            let _ = scope.wait();
            eprintln!("no user scope could be made here");
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let status = Command::new(BIN)
        .args([
            "--cgroup",
            &cgroup,
            "--seconds",
            "3",
            "--interval-ms",
            "500",
            "--out",
            out.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    let _ = scope.wait();
    assert_eq!(status.code(), Some(0));
    let figures = json(&fs::read_to_string(&out).unwrap());
    let _ = fs::remove_file(&out);
    assert!(figures["samples"].as_u64().unwrap() >= 1, "{figures}");
    let ended = figures["ended_early_at_ms"]
        .as_u64()
        .expect("ended_early_at_ms is set");
    assert!(ended < 3000, "{figures}");
}

#[test]
fn an_output_that_cannot_be_written_is_found_before_sampling() {
    let out = temp("missing-dir").join("figures.json");
    let started = Instant::now();
    let status = Command::new(BIN)
        .args(["--cgroup", &own_cgroup(), "--seconds", "30", "--out"])
        .arg(&out)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it sampled first"
    );
}

#[test]
fn the_output_is_written_whole_and_leaves_no_temporary() {
    let dir = temp("out-dir");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let out = dir.join("figures.json");
    let status = Command::new(BIN)
        .args([
            "--cgroup",
            &own_cgroup(),
            "--seconds",
            "1",
            "--interval-ms",
            "500",
            "--out",
        ])
        .arg(&out)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(0));
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(names, vec!["figures.json"]);
}

#[test]
fn an_output_that_cannot_be_renamed_leaves_no_temporary() {
    // The target is a directory: the rename fails at the end.
    let dir = temp("rename-dir");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("figures.json")).unwrap();
    let status = Command::new(BIN)
        .args([
            "--cgroup",
            &own_cgroup(),
            "--seconds",
            "1",
            "--interval-ms",
            "500",
            "--out",
        ])
        .arg(dir.join("figures.json"))
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(status.code(), Some(1));
    assert_eq!(names, vec!["figures.json"]);
}

#[test]
fn a_flag_given_twice_is_an_error() {
    let code = |args: &[&str]| {
        Command::new(BIN)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .code()
    };
    let cgroup = own_cgroup();
    for flag in [
        ["--cgroup", cgroup.as_str()],
        ["--seconds", "1"],
        ["--interval-ms", "100"],
        ["--memory-every", "2"],
        ["--out", "/tmp/never-written.json"],
    ] {
        let mut args = vec![flag[0], flag[1], flag[0], flag[1]];
        for required in [["--cgroup", cgroup.as_str()], ["--seconds", "1"]] {
            if required[0] != flag[0] {
                args.extend(required);
            }
        }
        assert_eq!(code(&args), Some(2), "{flag:?} twice");
    }
    // `--agent` is meant to be repeated.
    assert_eq!(
        code(&[
            "--cgroup",
            &own_cgroup(),
            "--seconds",
            "1",
            "--interval-ms",
            "500",
            "--agent",
            "a",
            "--agent",
            "b"
        ]),
        Some(0)
    );
}
