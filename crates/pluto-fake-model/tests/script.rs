use pluto_fake_model::script::{Pace, Position, SCRIPT_VERSION, Script, ScriptError, ToolCall};

const TWO_TURNS: &str = r#"{
  "version": 1,
  "side_answer": "A title",
  "turns": [
    { "segments": [
      { "text": "Looking.", "tool": { "role": "shell", "command": "ls -la" } },
      { "text": "Reading.", "tool": { "role": "read", "path": "README.md" } },
      { "text": "Done: **one**." }
    ] },
    { "segments": [ { "text": "Second turn." } ] }
  ]
}"#;

#[test]
fn parses_a_script_with_the_default_pace() {
    let script = Script::from_json(TWO_TURNS).unwrap();
    assert_eq!(script.version, SCRIPT_VERSION);
    assert_eq!(
        script.pace,
        Pace {
            first_token_ms: 300,
            tokens_per_second: 60.0,
            chunk_bytes: 16
        }
    );
    assert_eq!(script.side_answer, "A title");
    assert_eq!(script.turns.len(), 2);
    assert_eq!(
        script.turns[0].segments[0].tool,
        Some(ToolCall::Shell {
            command: "ls -la".into()
        })
    );
    assert_eq!(
        script.turns[0].segments[1].tool,
        Some(ToolCall::Read {
            path: "README.md".into()
        })
    );
}

#[test]
fn reads_an_explicit_pace() {
    let text = TWO_TURNS.replace(
        "\"side_answer\"",
        "\"pace\": {\"first_token_ms\": 5, \"tokens_per_second\": 1000, \"chunk_bytes\": 4}, \"side_answer\"",
    );
    let script = Script::from_json(&text).unwrap();
    assert_eq!(
        script.pace,
        Pace {
            first_token_ms: 5,
            tokens_per_second: 1000.0,
            chunk_bytes: 4
        }
    );
}

#[test]
fn refuses_another_version() {
    let text = TWO_TURNS.replace("\"version\": 1", "\"version\": 2");
    assert_eq!(
        Script::from_json(&text).unwrap_err(),
        ScriptError::Version(2)
    );
}

#[test]
fn refuses_malformed_and_unknown_fields() {
    assert!(matches!(
        Script::from_json("not json"),
        Err(ScriptError::Malformed(_))
    ));
    let text = TWO_TURNS.replace("\"side_answer\"", "\"extra\": 1, \"side_answer\"");
    assert!(matches!(
        Script::from_json(&text),
        Err(ScriptError::Malformed(_))
    ));
}

#[test]
fn refuses_scripts_that_cannot_play() {
    let none = r#"{"version": 1, "side_answer": "t", "turns": []}"#;
    assert_eq!(Script::from_json(none).unwrap_err(), ScriptError::NoTurns);
    let empty = r#"{"version": 1, "side_answer": "t", "turns": [{"segments": [{"text": "a"}]}, {"segments": []}]}"#;
    assert_eq!(
        Script::from_json(empty).unwrap_err(),
        ScriptError::EmptyTurn(1)
    );
    let endless = r#"{"version": 1, "side_answer": "t", "turns": [{"segments": [{"text": "a", "tool": {"role": "shell", "command": "ls"}}]}]}"#;
    assert_eq!(
        Script::from_json(endless).unwrap_err(),
        ScriptError::TurnEndsWithTool(0)
    );
    let stalled = TWO_TURNS.replace(
        "\"side_answer\"",
        "\"pace\": {\"tokens_per_second\": 0}, \"side_answer\"",
    );
    assert_eq!(Script::from_json(&stalled).unwrap_err(), ScriptError::Pace);
    let no_chunks = TWO_TURNS.replace(
        "\"side_answer\"",
        "\"pace\": {\"chunk_bytes\": 0}, \"side_answer\"",
    );
    assert_eq!(
        Script::from_json(&no_chunks).unwrap_err(),
        ScriptError::Pace
    );
}

#[test]
fn errors_say_what_is_wrong() {
    assert!(ScriptError::Version(2).to_string().contains('2'));
    assert!(ScriptError::TurnEndsWithTool(3).to_string().contains('3'));
    assert!(!ScriptError::NoTurns.to_string().is_empty());
}

#[test]
fn segments_wrap_turns_and_clamp_segments() {
    let script = Script::from_json(TWO_TURNS).unwrap();
    assert_eq!(
        script
            .segment(Position {
                turn: 0,
                segment: 0
            })
            .text,
        "Looking."
    );
    assert_eq!(
        script
            .segment(Position {
                turn: 0,
                segment: 2
            })
            .text,
        "Done: **one**."
    );
    assert_eq!(
        script
            .segment(Position {
                turn: 0,
                segment: 9
            })
            .text,
        "Done: **one**."
    );
    assert_eq!(
        script
            .segment(Position {
                turn: 1,
                segment: 0
            })
            .text,
        "Second turn."
    );
    assert_eq!(
        script
            .segment(Position {
                turn: 2,
                segment: 1
            })
            .text,
        "Reading."
    );
    assert_eq!(
        script
            .segment(Position {
                turn: 5,
                segment: 0
            })
            .text,
        "Second turn."
    );
}

#[test]
fn the_shipped_workload_is_valid() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/workload.json"
    ))
    .unwrap();
    let script = Script::from_json(&text).unwrap();
    assert!(script.turns.len() >= 3);
    assert!(
        script
            .turns
            .iter()
            .all(|turn| turn.segments.iter().any(|s| s.tool.is_some()))
    );
}
