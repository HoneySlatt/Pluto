//! The commands that start a turn, shared by the clients
//! (`startThreadTurn`, `orchestration.launchThread`).

use serde_json::{Value, json};

/// `startThreadTurn`'s `message.dispatch` for an existing thread. The
/// server resolves how a follow-up reaches a running turn
/// (`serverResolvedCommandContext`): `mode` is `auto`, `steer`, `queue`
/// (`queue_after_active`) or `start` (`start_immediately`, no intent).
pub fn message_dispatch(thread_id: &str, text: &str, model: &Value, mode: &str) -> Value {
    let mut command = json!({
        "type": "message.dispatch",
        "commandId": uuid::Uuid::new_v4().to_string(),
        "createdBy": "user",
        "creationSource": "web",
        "threadId": thread_id,
        "messageId": uuid::Uuid::new_v4().to_string(),
        "text": text,
        "attachments": [],
        "modelSelection": model,
        "dispatchMode": {"type": if mode == "queue" { "queue_after_active" } else { "start_immediately" }},
    });
    if mode != "queue" && mode != "start" {
        command["deliveryIntent"] = json!(mode);
    }
    command
}

/// `orchestration.launchThread`'s input: a new thread with its first
/// message, in the project's root checkout.
pub fn launch_input(
    thread_id: &str,
    project_id: &str,
    title: &str,
    text: &str,
    model: &Value,
    runtime: &str,
    interaction: &str,
) -> Value {
    json!({
        "commandId": uuid::Uuid::new_v4().to_string(),
        "creationSource": "web",
        "threadId": thread_id,
        "projectId": project_id,
        "title": title,
        "generateTitle": true,
        "modelSelection": model,
        "runtimeMode": runtime,
        "interactionMode": if interaction.is_empty() { "default" } else { interaction },
        "workspaceStrategy": {"type": "root"},
        "initialMessage": {
            "messageId": uuid::Uuid::new_v4().to_string(),
            "text": text,
            "attachments": [],
        },
    })
}
