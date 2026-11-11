use pluto_fake_model::conversation::ToolSpec;
use pluto_fake_model::script::ToolCall;
use pluto_fake_model::tools::{MappedTool, map};
use serde_json::json;

fn spec(name: &str, schema: serde_json::Value) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        schema,
    }
}

fn shell(command: &str) -> ToolCall {
    ToolCall::Shell {
        command: command.into(),
    }
}

#[test]
fn claude_codes_bash() {
    let offered = [
        spec(
            "Read",
            json!({"type": "object", "properties": {"file_path": {"type": "string"}}, "required": ["file_path"]}),
        ),
        spec(
            "Bash",
            json!({"type": "object", "properties": {"command": {"type": "string"}, "timeout": {"type": "number"}}, "required": ["command"]}),
        ),
    ];
    assert_eq!(
        map(&shell("ls -la"), &offered, "toolu_1".into()),
        Some(MappedTool {
            call_id: "toolu_1".into(),
            name: "Bash".into(),
            arguments: json!({"command": "ls -la"})
        })
    );
    assert_eq!(
        map(
            &ToolCall::Read {
                path: "README.md".into()
            },
            &offered,
            "toolu_2".into()
        )
        .unwrap()
        .arguments,
        json!({"file_path": "README.md"})
    );
}

#[test]
fn codexs_shell_takes_an_array() {
    let offered = [spec(
        "shell",
        json!({"type": "object", "properties": {"command": {"type": "array", "items": {"type": "string"}}, "workdir": {"type": "string"}}, "required": ["command"]}),
    )];
    let mapped = map(&shell("ls"), &offered, "call_1".into()).unwrap();
    assert_eq!(mapped.name, "shell");
    assert_eq!(mapped.arguments, json!({"command": ["bash", "-lc", "ls"]}));
}

#[test]
fn codexs_exec_command_takes_cmd() {
    let offered = [spec(
        "exec_command",
        json!({"type": "object", "properties": {"cmd": {"type": "string"}, "yield_time_ms": {"type": "number"}}, "required": ["cmd"]}),
    )];
    assert_eq!(
        map(&shell("ls"), &offered, "c".into()).unwrap().arguments,
        json!({"cmd": "ls"})
    );
}

#[test]
fn required_properties_get_placeholders() {
    let offered = [spec(
        "bash",
        json!({"type": "object", "properties": {
        "command": {"type": "string"}, "description": {"type": "string"}, "timeout": {"type": "integer"},
        "background": {"type": "boolean"}, "env": {"type": "object"}, "args": {"type": "array"}, "ratio": {"type": "number"}
    }, "required": ["command", "description", "timeout", "background", "env", "args", "ratio"]}),
    )];
    assert_eq!(
        map(&shell("ls"), &offered, "c".into()).unwrap().arguments,
        json!({"command": "ls", "description": "fake", "timeout": 0, "background": false, "env": {}, "args": [], "ratio": 0})
    );
}

#[test]
fn the_order_of_names_decides_not_the_order_offered() {
    let schema = json!({"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]});
    let offered = [
        spec("terminal", schema.clone()),
        spec("bash", schema.clone()),
        spec("Bash", schema),
    ];
    assert_eq!(
        map(&shell("ls"), &offered, "c".into()).unwrap().name,
        "Bash"
    );
}

#[test]
fn no_fitting_tool_gives_no_call() {
    let offered = [spec(
        "Write",
        json!({"type": "object", "properties": {"file_path": {"type": "string"}}}),
    )];
    assert_eq!(map(&shell("ls"), &offered, "c".into()), None);
    assert_eq!(map(&shell("ls"), &[], "c".into()), None);
    let no_command = [spec(
        "Bash",
        json!({"type": "object", "properties": {"script": {"type": "string"}}}),
    )];
    assert_eq!(map(&shell("ls"), &no_command, "c".into()), None);
    let no_path = [spec(
        "read",
        json!({"type": "object", "properties": {"offset": {"type": "integer"}}}),
    )];
    assert_eq!(
        map(&ToolCall::Read { path: "a".into() }, &no_path, "c".into()),
        None
    );
    let not_a_schema = [spec("Bash", json!("anything"))];
    assert_eq!(map(&shell("ls"), &not_a_schema, "c".into()), None);
}

#[test]
fn a_required_property_comes_first() {
    let offered = [spec(
        "read",
        json!({"type": "object", "properties": {"filePath": {"type": "string"}, "path": {"type": "string"}}, "required": ["filePath"]}),
    )];
    assert_eq!(
        map(
            &ToolCall::Read {
                path: "a.md".into()
            },
            &offered,
            "c".into()
        )
        .unwrap()
        .arguments,
        json!({"filePath": "a.md"})
    );
}

#[test]
fn a_read_without_a_read_tool_goes_through_the_shell() {
    let offered = [spec(
        "exec_command",
        json!({"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]}),
    )];
    let mapped = map(
        &ToolCall::Read {
            path: "README.md".into(),
        },
        &offered,
        "c".into(),
    )
    .unwrap();
    assert_eq!(mapped.name, "exec_command");
    assert_eq!(mapped.arguments, json!({"cmd": "cat README.md"}));
}
