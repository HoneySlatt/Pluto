use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_pluto-measure");

#[test]
fn measures_its_own_cgroup_briefly() {
    // The test's own cgroup, read from /proc/self/cgroup ("0::/path").
    let line = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let path = line.lines().find_map(|l| l.strip_prefix("0::")).unwrap();
    let cgroup = format!("/sys/fs/cgroup{path}");
    let out = std::env::temp_dir().join(format!("pluto-measure-bin-{}.json", std::process::id()));
    let status = Command::new(BIN)
        .args([
            "--cgroup",
            &cgroup,
            "--seconds",
            "1",
            "--interval-ms",
            "200",
            "--memory-every",
            "2",
            "--out",
            out.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let figures: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    std::fs::remove_file(&out).unwrap();
    assert!(figures["samples"].as_u64().unwrap() >= 5, "{figures}");
    assert!(
        figures["app"]["memory"]["headline"].as_u64().unwrap() > 0,
        "{figures}"
    );
    assert!(
        figures["app"]["processes"].as_u64().unwrap() >= 1,
        "{figures}"
    );
}

#[test]
fn bad_command_lines() {
    let code = |args: &[&str]| {
        Command::new(BIN)
            .args(args)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .code()
    };
    assert_eq!(code(&[]), Some(2));
    assert_eq!(code(&["--cgroup", "/sys/fs/cgroup"]), Some(2));
    assert_eq!(
        code(&["--cgroup", "/sys/fs/cgroup", "--seconds", "x"]),
        Some(2)
    );
    assert_eq!(
        code(&["--cgroup", "/nonexistent", "--seconds", "1"]),
        Some(1)
    );
}
