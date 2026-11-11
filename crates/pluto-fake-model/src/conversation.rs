//! What a request carries, read the same way for each API: where it stands in the script, the
//! tools it offers, and its bytes by part.
//!
//! The turn is read from the last user message whose text holds the marker `[turn N]`; the segment
//! is the number of tool results after that message. A request without a marker stands at turn 0.
//! Messages a client adds on its own (an environment's description, reminders) carry no marker, so
//! they move nothing.
//!
//! Bytes are counted on each part's compact JSON (`serde_json::to_string`), so that they compare
//! between clients whatever their own formatting:
//! - instructions: Anthropic's `system`; Responses' `instructions`; Ollama's messages of role
//!   `system`; Chat Completions' messages of role `system` or `developer`;
//! - tools: the `tools` array;
//! - new message: the last message (Anthropic, Ollama, Chat Completions) or input item
//!   (Responses), if it is the user's or a tool's result;
//! - history: every other message or input item.

use crate::json;
use crate::script::Position;
use serde::Serialize;
use serde_json::Value;

/// The API a request came in on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Api {
    /// `POST /v1/messages`.
    Anthropic,
    /// `POST /v1/responses`.
    Responses,
    /// `POST /v1/chat/completions`.
    #[serde(rename = "chat_completions")]
    ChatCompletions,
    /// `POST /api/chat`.
    Ollama,
}

/// A tool the client offers: its name and the JSON Schema of its arguments (Anthropic's
/// `input_schema`, or the `parameters` of an OpenAI or Ollama function).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub schema: Value,
}

/// A request's bytes, by part. `total` is the whole body's length as received.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Parts {
    pub total: usize,
    pub instructions: usize,
    pub tools: usize,
    pub history: usize,
    pub new_message: usize,
}

/// A tool result the request sends back: the call id it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub call_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub model: String,
    /// Whether the client asked for a stream (`stream: true`; Ollama streams unless told not to).
    pub stream: bool,
    /// Whether a Chat Completions stream must end with its usage
    /// (`stream_options.include_usage`); false for the other APIs.
    pub include_usage: bool,
    pub position: Position,
    /// The tool results after the turn's prompt, in order.
    pub tool_results: Vec<ToolResult>,
    pub tools: Vec<ToolSpec>,
    pub parts: Parts,
}

/// One message (Anthropic, Ollama, Chat Completions) or input item (Responses), as the four APIs
/// have in common.
struct Item<'a> {
    value: &'a Value,
    /// The prompt's text, if the user wrote it; empty for every other item.
    text: String,
    /// The call ids of the tool results it holds.
    tool_results: Vec<String>,
    /// Whether it is the user's or a tool's: the kind of item that is a request's new message.
    from_user_or_tool: bool,
}

/// Reads a request's body. `body_len` is the body's length in bytes, for [`Parts::total`]. An
/// error says what is missing, for the journal and the client's 400.
pub fn read(api: Api, body: &Value, body_len: usize) -> Result<Conversation, String> {
    let fields = body.as_object().ok_or("the body is not a JSON object")?;
    let model = fields
        .get("model")
        .and_then(Value::as_str)
        .ok_or("`model` is missing or not a string")?
        .to_owned();
    let items = match api {
        Api::Anthropic => anthropic_items(body)?,
        Api::Responses => responses_items(body)?,
        // Ollama's results carry no id of their own; a client may add one, or the tool's name.
        Api::Ollama => message_items(api, body, &["tool_call_id", "tool_name"])?,
        Api::ChatCompletions => message_items(api, body, &["tool_call_id"])?,
    };

    // The last prompt with a marker decides the turn; only what follows it moves the segment.
    let marker = items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| turn_marker(&item.text).map(|turn| (index, turn)));
    let (turn, first_after_prompt) = match marker {
        Some((index, turn)) => (turn, index + 1),
        None => (0, 0),
    };
    let tool_results: Vec<ToolResult> = items[first_after_prompt..]
        .iter()
        .flat_map(|item| &item.tool_results)
        .map(|call_id| ToolResult {
            call_id: call_id.clone(),
        })
        .collect();

    let (history, new_message) = match items.split_last() {
        Some((last, earlier)) if last.from_user_or_tool => (earlier, Some(last)),
        _ => (&items[..], None),
    };
    let parts = Parts {
        total: body_len,
        instructions: instructions_bytes(api, body),
        tools: fields.get("tools").map_or(0, compact_len),
        history: history.iter().map(|item| compact_len(item.value)).sum(),
        new_message: new_message.map_or(0, |item| compact_len(item.value)),
    };

    let streams_by_default = api == Api::Ollama;
    let stream = match fields.get("stream") {
        Some(Value::Bool(asked)) => *asked,
        _ => streams_by_default,
    };
    let tools = fields
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| tools.iter().filter_map(tool_spec).collect())
        .unwrap_or_default();

    let include_usage = api == Api::ChatCompletions
        && body["stream_options"]["include_usage"].as_bool() == Some(true);

    Ok(Conversation {
        model,
        stream,
        include_usage,
        position: Position {
            turn,
            segment: tool_results.len(),
        },
        tool_results,
        tools,
        parts,
    })
}

/// The turn number in a text holding `[turn N]`, the first such marker.
pub fn turn_marker(text: &str) -> Option<usize> {
    const OPENING: &str = "[turn ";
    text.match_indices(OPENING).find_map(|(at, _)| {
        let after = &text[at + OPENING.len()..];
        let digits = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if after[digits..].starts_with(']') {
            after[..digits].parse().ok()
        } else {
            None
        }
    })
}

fn compact_len(value: &Value) -> usize {
    json::compact(value).len()
}

fn instructions_bytes(api: Api, body: &Value) -> usize {
    match api {
        Api::Anthropic => body.get("system").map_or(0, compact_len),
        Api::Responses => body.get("instructions").map_or(0, compact_len),
        Api::Ollama | Api::ChatCompletions => body
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|message| is_instruction(api, message))
            .map(compact_len)
            .sum(),
    }
}

/// A client's tool, from any API's form: Anthropic's flat `input_schema`, Responses' flat
/// `parameters`, or the `function` object of Ollama and OpenAI's chat. A tool without a name
/// (Responses' built-in tools) is not one the script can call.
fn tool_spec(tool: &Value) -> Option<ToolSpec> {
    let definition = tool.get("function").unwrap_or(tool);
    let name = definition.get("name")?.as_str()?.to_owned();
    let schema = definition
        .get("input_schema")
        .or_else(|| definition.get("parameters"))
        .cloned()
        .unwrap_or(Value::Null);
    Some(ToolSpec { name, schema })
}

/// The text of a message's content: a string, or the `text` of each part, one per line.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.as_str().or_else(|| part.get("text")?.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn anthropic_items(body: &Value) -> Result<Vec<Item<'_>>, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("`messages` is missing or not an array")?;
    Ok(messages
        .iter()
        .map(|message| {
            let from_user = message["role"] == "user";
            let blocks = message["content"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            Item {
                value: message,
                text: if from_user {
                    text_of(&message["content"])
                } else {
                    String::new()
                },
                tool_results: blocks
                    .iter()
                    .filter(|block| block["type"] == "tool_result")
                    .map(|block| block["tool_use_id"].as_str().unwrap_or_default().to_owned())
                    .collect(),
                from_user_or_tool: from_user,
            }
        })
        .collect())
}

fn responses_items(body: &Value) -> Result<Vec<Item<'_>>, String> {
    match body.get("input") {
        // A bare string is one user message.
        Some(prompt @ Value::String(text)) => Ok(vec![Item {
            value: prompt,
            text: text.clone(),
            tool_results: Vec::new(),
            from_user_or_tool: true,
        }]),
        Some(Value::Array(input)) => Ok(input
            .iter()
            .map(|item| {
                let is_user_message = item["role"] == "user"
                    && (item.get("type").is_none() || item["type"] == "message");
                let is_tool_result = item["type"] == "function_call_output";
                Item {
                    value: item,
                    text: if is_user_message {
                        text_of(&item["content"])
                    } else {
                        String::new()
                    },
                    tool_results: if is_tool_result {
                        vec![item["call_id"].as_str().unwrap_or_default().to_owned()]
                    } else {
                        Vec::new()
                    },
                    from_user_or_tool: is_user_message || is_tool_result,
                }
            })
            .collect()),
        _ => Err("`input` is missing or not a string or an array".to_owned()),
    }
}

/// Ollama's and Chat Completions' messages: their instructions (role `system`, and Chat
/// Completions' `developer`) are not items; a `tool` message is a result, whose id is its first
/// key of `id_keys` present.
fn message_items<'a>(api: Api, body: &'a Value, id_keys: &[&str]) -> Result<Vec<Item<'a>>, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("`messages` is missing or not an array")?;
    Ok(messages
        .iter()
        .filter(|message| !is_instruction(api, message))
        .map(|message| {
            let from_user = message["role"] == "user";
            let from_tool = message["role"] == "tool";
            let call_id = id_keys
                .iter()
                .find_map(|key| message[*key].as_str())
                .unwrap_or_default();
            Item {
                value: message,
                text: if from_user {
                    text_of(&message["content"])
                } else {
                    String::new()
                },
                tool_results: if from_tool {
                    vec![call_id.to_owned()]
                } else {
                    Vec::new()
                },
                from_user_or_tool: from_user || from_tool,
            }
        })
        .collect())
}

/// Whether a message is an instruction: role `system`, or, for Chat Completions, `developer`,
/// which OpenAI's newer models take in its place.
fn is_instruction(api: Api, message: &Value) -> bool {
    message["role"] == "system" || (api == Api::ChatCompletions && message["role"] == "developer")
}
