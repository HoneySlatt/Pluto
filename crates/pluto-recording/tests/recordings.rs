//! Every committed recording, in CI: it reads at its version, the scan finds nothing in it or in
//! its reference, and it replays with a client other than the fork's
//! (docs/adr/0025-agents-sessions-recorded-and-replayed.md). The harness's synthetic recordings
//! pass the same checks; they have no reference, since a reference comes from a live run.

use std::path::{Path, PathBuf};

use pluto_recording::{format, replay, scan};

fn recordings_in(folder: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(folder);
    let mut found = Vec::new();
    for agent in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        if !agent.path().is_dir() {
            continue;
        }
        for file in std::fs::read_dir(agent.path()).unwrap().flatten() {
            let path = file.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn check_recording(path: &Path) {
    let name = path.display();
    let text = std::fs::read_to_string(path).unwrap();
    let recording = format::read(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
    let findings = scan::recording(&recording);
    assert!(findings.is_empty(), "{name}: {findings:?}");
    replay::with_another_client(&recording).unwrap_or_else(|error| panic!("{name}: {error}"));
}

#[test]
fn every_recording_reads_scans_clean_and_replays_with_another_client() {
    for path in recordings_in("recordings") {
        check_recording(&path);
        let reference = path.with_extension("thread.json");
        let text = std::fs::read_to_string(&reference)
            .unwrap_or_else(|error| panic!("{}: {error}", reference.display()));
        let value = serde_json::from_str(&text).unwrap();
        let findings = scan::document(&value);
        assert!(findings.is_empty(), "{}: {findings:?}", reference.display());
    }
}

#[test]
fn every_synthetic_recording_passes_the_same_checks() {
    let synthetic = recordings_in("fork-harness/synthetic");
    assert_eq!(
        synthetic.len(),
        11,
        "the harness's scenarios: five for Claude, six for Grok"
    );
    for path in synthetic {
        check_recording(&path);
    }
}
