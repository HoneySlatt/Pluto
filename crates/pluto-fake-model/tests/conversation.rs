use pluto_fake_model::conversation::{Api, ToolResult, read, turn_marker};
use pluto_fake_model::script::Position;
use serde_json::{Value, json};

fn compact(value: &Value) -> usize {
    serde_json::to_string(value).unwrap().len()
}

fn anthropic_body() -> Value {
    json!({
        "model": "claude-fake",
        "stream": true,
        "max_tokens": 32000,
        "system": [{"type": "text", "text": "You are Claude Code."}],
        "tools": [{"name": "Bash", "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}],
        "messages": [
            {"role": "user", "content": "[turn 0] first"},
            {"role": "assistant", "content": [{"type": "text", "text": "done"}]},
            {"role": "user", "content": [
                {"type": "text", "text": "<system-reminder>be brief</system-reminder>"},
                {"type": "text", "text": "[turn 2] look at the project"}
            ]},
            {"role": "assistant", "content": [
                {"type": "text", "text": "Looking."},
                {"type": "tool_use", "id": "toolu_fake_2_0", "name": "Bash", "input": {"command": "ls"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_fake_2_0", "content": "a\nb"},
                {"type": "text", "text": "<system-reminder>x</system-reminder>"}
            ]}
        ]
    })
}

#[test]
fn anthropic_position_tools_and_results() {
    let body = anthropic_body();
    let conversation = read(Api::Anthropic, &body, 1234).unwrap();
    assert_eq!(conversation.model, "claude-fake");
    assert!(conversation.stream);
    assert_eq!(
        conversation.position,
        Position {
            turn: 2,
            segment: 1
        }
    );
    assert_eq!(
        conversation.tool_results,
        vec![ToolResult {
            call_id: "toolu_fake_2_0".into()
        }]
    );
    assert_eq!(conversation.tools.len(), 1);
    assert_eq!(conversation.tools[0].name, "Bash");
    assert_eq!(
        conversation.tools[0].schema,
        body["tools"][0]["input_schema"]
    );
}

#[test]
fn anthropic_parts() {
    let body = anthropic_body();
    let parts = read(Api::Anthropic, &body, 1234).unwrap().parts;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(parts.total, 1234);
    assert_eq!(parts.instructions, compact(&body["system"]));
    assert_eq!(parts.tools, compact(&body["tools"]));
    assert_eq!(parts.new_message, compact(&messages[4]));
    assert_eq!(
        parts.history,
        messages[..4].iter().map(compact).sum::<usize>()
    );
}

#[test]
fn anthropic_string_system_no_tools_and_no_marker() {
    let body = json!({
        "model": "m", "max_tokens": 10, "system": "Write a title.",
        "messages": [{"role": "user", "content": "hello"}]
    });
    let conversation = read(Api::Anthropic, &body, 10).unwrap();
    assert!(!conversation.stream);
    assert!(conversation.tools.is_empty());
    assert_eq!(
        conversation.position,
        Position {
            turn: 0,
            segment: 0
        }
    );
    assert_eq!(
        conversation.parts.instructions,
        compact(&json!("Write a title."))
    );
    assert_eq!(conversation.parts.history, 0);
}

#[test]
fn a_last_assistant_message_is_history() {
    let body = json!({
        "model": "m", "max_tokens": 10,
        "messages": [{"role": "user", "content": "[turn 1] a"}, {"role": "assistant", "content": "prefill"}]
    });
    let parts = read(Api::Anthropic, &body, 10).unwrap().parts;
    assert_eq!(parts.new_message, 0);
    assert_eq!(
        parts.history,
        compact(&body["messages"][0]) + compact(&body["messages"][1])
    );
}

fn responses_body() -> Value {
    json!({
        "model": "gpt-fake",
        "stream": true,
        "instructions": "You are Codex.",
        "tools": [{"type": "function", "name": "shell", "parameters": {"type": "object", "properties": {"command": {"type": "array", "items": {"type": "string"}}}, "required": ["command"]}}],
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "<environment_context>cwd</environment_context>"}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "[turn 3] go"}]},
            {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Looking."}]},
            {"type": "function_call", "call_id": "call_fake_3_0", "name": "shell", "arguments": "{\"command\":[\"bash\",\"-lc\",\"ls\"]}"},
            {"type": "function_call_output", "call_id": "call_fake_3_0", "output": "a"},
            {"type": "function_call", "call_id": "call_fake_3_1", "name": "shell", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "call_fake_3_1", "output": "b"}
        ]
    })
}

#[test]
fn responses_position_tools_results_and_parts() {
    let body = responses_body();
    let conversation = read(Api::Responses, &body, 999).unwrap();
    assert_eq!(conversation.model, "gpt-fake");
    assert!(conversation.stream);
    assert_eq!(
        conversation.position,
        Position {
            turn: 3,
            segment: 2
        }
    );
    assert_eq!(
        conversation.tool_results,
        vec![
            ToolResult {
                call_id: "call_fake_3_0".into()
            },
            ToolResult {
                call_id: "call_fake_3_1".into()
            }
        ]
    );
    assert_eq!(conversation.tools[0].name, "shell");
    assert_eq!(conversation.tools[0].schema, body["tools"][0]["parameters"]);
    let items = body["input"].as_array().unwrap();
    let parts = conversation.parts;
    assert_eq!(parts.instructions, compact(&body["instructions"]));
    assert_eq!(parts.tools, compact(&body["tools"]));
    assert_eq!(parts.new_message, compact(&items[7]));
    assert_eq!(parts.history, items[..7].iter().map(compact).sum::<usize>());
}

#[test]
fn responses_input_as_a_string() {
    let body = json!({"model": "m", "input": "[turn 4] hi"});
    let conversation = read(Api::Responses, &body, 5).unwrap();
    assert!(!conversation.stream);
    assert_eq!(
        conversation.position,
        Position {
            turn: 4,
            segment: 0
        }
    );
    assert_eq!(conversation.parts.new_message, compact(&body["input"]));
    assert_eq!(conversation.parts.history, 0);
}

#[test]
fn ollama_position_tools_results_and_parts() {
    let body = json!({
        "model": "fake-model",
        "messages": [
            {"role": "system", "content": "Be useful."},
            {"role": "user", "content": "[turn 1] hi"},
            {"role": "assistant", "content": "", "tool_calls": [{"function": {"name": "bash", "arguments": {"command": "ls"}}}]},
            {"role": "tool", "content": "a", "tool_name": "bash"}
        ],
        "tools": [{"type": "function", "function": {"name": "bash", "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}}]
    });
    let conversation = read(Api::Ollama, &body, 77).unwrap();
    assert!(conversation.stream, "Ollama streams unless told not to");
    assert_eq!(
        conversation.position,
        Position {
            turn: 1,
            segment: 1
        }
    );
    assert_eq!(
        conversation.tool_results,
        vec![ToolResult {
            call_id: "bash".into()
        }]
    );
    assert_eq!(conversation.tools[0].name, "bash");
    assert_eq!(
        conversation.tools[0].schema,
        body["tools"][0]["function"]["parameters"]
    );
    let messages = body["messages"].as_array().unwrap();
    let parts = conversation.parts;
    assert_eq!(parts.instructions, compact(&messages[0]));
    assert_eq!(parts.new_message, compact(&messages[3]));
    assert_eq!(parts.history, compact(&messages[1]) + compact(&messages[2]));
    let quiet =
        json!({"model": "m", "stream": false, "messages": [{"role": "user", "content": "x"}]});
    assert!(!read(Api::Ollama, &quiet, 1).unwrap().stream);
}

#[test]
fn ollama_result_ids_prefer_the_call_id() {
    let body = json!({"model": "m", "messages": [
        {"role": "user", "content": "[turn 0] x"},
        {"role": "tool", "content": "a", "tool_call_id": "call_fake_0_0", "tool_name": "bash"},
        {"role": "tool", "content": "b"}
    ]});
    let conversation = read(Api::Ollama, &body, 1).unwrap();
    assert_eq!(
        conversation.tool_results,
        vec![
            ToolResult {
                call_id: "call_fake_0_0".into()
            },
            ToolResult {
                call_id: String::new()
            }
        ]
    );
    assert_eq!(conversation.position.segment, 2);
}

#[test]
fn malformed_bodies_say_what_is_missing() {
    for (api, body) in [
        (Api::Anthropic, json!({"messages": []})),
        (Api::Anthropic, json!({"model": "m"})),
        (Api::Anthropic, json!({"model": "m", "messages": "no"})),
        (Api::Responses, json!({"model": "m"})),
        (Api::Ollama, json!({"model": "m"})),
        (Api::Ollama, json!([1, 2])),
    ] {
        let error = read(api, &body, 1).unwrap_err();
        assert!(!error.is_empty(), "{api:?} {body}");
    }
}

#[test]
fn turn_markers() {
    assert_eq!(turn_marker("[turn 0] a"), Some(0));
    assert_eq!(turn_marker("text [turn 12] more [turn 3]"), Some(12));
    assert_eq!(turn_marker("[turn] a"), None);
    assert_eq!(turn_marker("[turn x] a"), None);
    assert_eq!(turn_marker("[turn 99999999999999999999999] a"), None);
    assert_eq!(turn_marker("turn 3"), None);
    assert_eq!(turn_marker(""), None);
}
