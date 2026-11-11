//! The script's tool calls mapped to the tools a client offers.
//!
//! Each client names its tools its own way: Claude Code's `Bash` and `Read`, Codex's `shell` or
//! `exec_command`, OpenCode's `bash` and `read`. A call is mapped to the first offered tool whose
//! name is in its role's list, in the order of the list; its arguments are built from that tool's
//! schema:
//! - the command goes into the first of `command` or `cmd` that the schema requires, else the
//!   first it has: as a string, or, if that property is an array, as `["bash", "-lc", command]`;
//! - the path goes the same way into one of `file_path`, `path`, `filePath` or `file`;
//! - every other property the schema requires gets a placeholder of its type: `"fake"`, `0`,
//!   `false`, `[]` or `{}`.
//!
//! A read that no offered tool fits becomes the shell command `cat <path>`, as Codex, which has
//! no read tool, reads a file. A path with a character the shell would not take as it is (a space,
//! a quote, `;`, `$`) goes in single quotes, a `'` in it written `'\''`; a path that starts with
//! `-` gets `./` before it. A client that offers no fitting tool, or a tool whose schema has no
//! property for the command or the path, gets no call: the segment's text alone.

use crate::conversation::ToolSpec;
use crate::script::ToolCall;
use serde_json::{Map, Value, json};

/// The properties that hold a shell command, in order of preference.
const COMMAND_KEYS: &[&str] = &["command", "cmd"];

/// The properties that hold a file's path, in order of preference.
const PATH_KEYS: &[&str] = &["file_path", "path", "filePath", "file"];

/// The names a shell tool goes by, in order of preference.
pub const SHELL_NAMES: &[&str] = &[
    "Bash",
    "bash",
    "shell",
    "exec_command",
    "shell_command",
    "run_shell_command",
    "run_terminal_cmd",
    "terminal",
];

/// The names a read tool goes by, in order of preference.
pub const READ_NAMES: &[&str] = &["Read", "read", "read_file", "ReadFile", "view"];

/// A call ready to encode: the id the client will send back with the result, the client's tool
/// name, and its arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct MappedTool {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

/// Maps a script's call to one of the offered tools, or `None` if none fits.
pub fn map(call: &ToolCall, offered: &[ToolSpec], call_id: String) -> Option<MappedTool> {
    map_exactly(call, offered, call_id.clone()).or_else(|| match call {
        // Codex offers no read tool: it reads files through its shell, so the fake does too.
        ToolCall::Read { path } => map_exactly(
            &ToolCall::Shell {
                command: format!("cat {}", shell_quoted(path)),
            },
            offered,
            call_id,
        ),
        ToolCall::Shell { .. } => None,
    })
}

/// The path as one shell word: unchanged if every character is plain, else in single quotes.
fn shell_quoted(path: &str) -> String {
    // A path that starts with `-` would be read by `cat` as an option.
    let path = if path.starts_with('-') {
        format!("./{path}")
    } else {
        path.to_owned()
    };
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./+@:,=%".contains(c);
    if !path.is_empty() && path.chars().all(plain) {
        path
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

/// Whether the client offers a tool the script can call: one named like a shell or a read tool.
/// A request that offers none (a title request with a structured-output tool) is not a turn.
pub fn offers_a_callable_tool(offered: &[ToolSpec]) -> bool {
    offered.iter().any(|tool| {
        SHELL_NAMES.contains(&tool.name.as_str()) || READ_NAMES.contains(&tool.name.as_str())
    })
}

/// The call mapped to a tool of its own role only.
fn map_exactly(call: &ToolCall, offered: &[ToolSpec], call_id: String) -> Option<MappedTool> {
    let (names, keys) = match call {
        ToolCall::Shell { .. } => (SHELL_NAMES, COMMAND_KEYS),
        ToolCall::Read { .. } => (READ_NAMES, PATH_KEYS),
    };
    let tool = names
        .iter()
        .find_map(|name| offered.iter().find(|tool| tool.name == *name))?;
    let properties = tool.schema.get("properties")?.as_object()?;
    let required = required_names(&tool.schema);
    let (key, property) = property_for(keys, properties, &required)?;

    let mut arguments = Map::new();
    arguments.insert(key.to_owned(), argument(call, property));
    for name in required {
        if !arguments.contains_key(name) {
            arguments.insert(name.to_owned(), placeholder(properties.get(name)));
        }
    }
    Some(MappedTool {
        call_id,
        name: tool.name.clone(),
        arguments: Value::Object(arguments),
    })
}

fn required_names(schema: &Value) -> Vec<&str> {
    schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// The first key the schema requires, else the first it has.
fn property_for<'a>(
    keys: &[&'a str],
    properties: &'a Map<String, Value>,
    required: &[&str],
) -> Option<(&'a str, &'a Value)> {
    let find = |only_required: bool| {
        keys.iter().find_map(|key| {
            let property = properties.get(*key)?;
            (!only_required || required.contains(key)).then_some((*key, property))
        })
    };
    find(true).or_else(|| find(false))
}

/// The call's value for its property: a command goes as `["bash", "-lc", command]` into an array.
fn argument(call: &ToolCall, property: &Value) -> Value {
    match call {
        ToolCall::Shell { command } if type_name(property) == Some("array") => {
            json!(["bash", "-lc", command])
        }
        ToolCall::Shell { command: text } | ToolCall::Read { path: text } => json!(text),
    }
}

/// A value of the property's type, for a required property the script has nothing to say about.
fn placeholder(property: Option<&Value>) -> Value {
    match property.and_then(type_name) {
        Some("integer" | "number") => json!(0),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => json!("fake"),
    }
}

/// A property's JSON Schema type; for a list of types, the first that is not `null`.
fn type_name(property: &Value) -> Option<&str> {
    match &property["type"] {
        Value::String(name) => Some(name),
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .find(|name| *name != "null"),
        _ => None,
    }
}
