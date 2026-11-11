//! Edge cases: the script's lookup, the tool mapping, the Responses events and the HTTP framing
//! of a response without a body.

use pluto_fake_model::conversation::ToolSpec;
use pluto_fake_model::http::write_full;
use pluto_fake_model::script::{Pace, Position, Script, ToolCall};
use pluto_fake_model::tools::{self, MappedTool};
use pluto_fake_model::wire::{Reply, responses};
use serde_json::{Value, json};

#[test]
fn a_script_that_was_not_validated_gives_an_empty_segment_instead_of_a_panic() {
    let position = Position {
        turn: 3,
        segment: 2,
    };
    let mut script = Script {
        version: 1,
        pace: Pace::default(),
        side_answer: String::new(),
        turns: Vec::new(),
    };
    let segment = script.segment(position);
    assert!(segment.text.is_empty() && segment.tool.is_none());

    script.turns.push(pluto_fake_model::script::Turn {
        segments: Vec::new(),
    });
    let segment = script.segment(position);
    assert!(segment.text.is_empty() && segment.tool.is_none());
}

#[test]
fn the_read_fallback_quotes_a_path_the_shell_would_split() {
    let shell = ToolSpec {
        name: "shell".into(),
        schema: json!({"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}),
    };
    let command = |path: &str| -> Value {
        let call = ToolCall::Read { path: path.into() };
        tools::map(&call, std::slice::from_ref(&shell), "call_x".into())
            .unwrap()
            .arguments["command"]
            .clone()
    };
    assert_eq!(
        command("/tmp/it's here; rm"),
        "cat '/tmp/it'\\''s here; rm'"
    );
    assert_eq!(command("my file.txt"), "cat 'my file.txt'");
    assert_eq!(command("src/main.rs"), "cat src/main.rs");
}

fn reply() -> Reply {
    Reply {
        model: "m".into(),
        id: "r7".into(),
        text: "Hello, world".into(),
        tool: Some(MappedTool {
            call_id: "call_x".into(),
            name: "Bash".into(),
            arguments: json!({"command": "ls"}),
        }),
        input_tokens: 10,
        chunk_bytes: 5,
    }
}

fn parsed(reply: &Reply) -> Vec<Value> {
    responses::events(reply)
        .iter()
        .map(|event| {
            let data = event.data.split_once("\ndata: ").unwrap().1;
            serde_json::from_str(data.trim_end()).unwrap()
        })
        .collect()
}

#[test]
fn a_responses_message_has_its_content_part_events() {
    let reply = reply();
    let events = parsed(&reply);
    let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
    let deltas = "response.output_text.delta";
    let mut expected = vec![
        "response.created",
        "response.output_item.added",
        "response.content_part.added",
    ];
    expected.extend(std::iter::repeat_n(deltas, 3));
    expected.extend([
        "response.output_text.done",
        "response.content_part.done",
        "response.output_item.done",
        "response.output_item.added",
        "response.output_item.done",
        "response.completed",
    ]);
    assert_eq!(kinds, expected);
    for (n, event) in events.iter().enumerate() {
        assert_eq!(event["sequence_number"], n);
    }
    let empty_part = json!({"type": "output_text", "text": "", "annotations": []});
    let whole_part = json!({"type": "output_text", "text": "Hello, world", "annotations": []});
    let added = &events[2];
    assert_eq!(
        (
            &added["item_id"],
            &added["output_index"],
            &added["content_index"],
            &added["part"]
        ),
        (&json!("msg_r7"), &json!(0), &json!(0), &empty_part)
    );
    let text_done = &events[6];
    assert_eq!(
        (
            &text_done["item_id"],
            &text_done["output_index"],
            &text_done["content_index"],
            &text_done["text"]
        ),
        (
            &json!("msg_r7"),
            &json!(0),
            &json!(0),
            &json!("Hello, world")
        )
    );
    let part_done = &events[7];
    assert_eq!(
        (
            &part_done["item_id"],
            &part_done["output_index"],
            &part_done["content_index"],
            &part_done["part"]
        ),
        (&json!("msg_r7"), &json!(0), &json!(0), &whole_part)
    );
    assert_eq!(events[8]["item"]["type"], "message");
    // The extra events carry no text: only the deltas and the arguments are paced.
    let carried: usize = responses::events(&reply).iter().map(|e| e.text_bytes).sum();
    assert_eq!(carried, "Hello, world".len() + r#"{"command":"ls"}"#.len());
}

#[test]
fn a_response_without_content_has_no_content_headers() {
    let mut out = Vec::new();
    write_full(&mut out, 204, "text/plain", b"").unwrap();
    let text = String::from_utf8(out).unwrap().to_ascii_lowercase();
    assert!(text.starts_with("http/1.1 204 no content\r\n"));
    assert!(!text.contains("content-type"), "{text}");
    assert!(!text.contains("content-length"), "{text}");
    assert!(text.contains("\r\nconnection: close\r\n"));
}
