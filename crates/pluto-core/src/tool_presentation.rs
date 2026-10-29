//! Tool presentation: the T3 tool inventory and its labels
//! (`packages/shared/src/t3McpToolPresentation.ts`), how a tool call is
//! classified and named (`packages/shared/src/toolActivity.ts`), and the
//! T3 result envelope (`client-runtime/src/t3ToolSummary.ts` `readResult`).
//!
//! The labels keep upstream's "T3 thread" wording, as the server fork does.

use serde_json::{Map, Value};

/// `T3McpToolDefinition`: `labels` are action, running, completed, detail.
#[derive(Debug, PartialEq)]
pub struct T3Tool {
    pub name: &'static str,
    pub labels: [&'static str; 4],
    /// `T3McpToolSummaryAction`.
    pub summary: &'static str,
    /// `t3-code`, `browser`, `device` or `pull-request`.
    pub icon: &'static str,
}

/// `T3_MCP_TOOLS`.
#[rustfmt::skip]
pub const TOOLS: &[T3Tool] = &[
    T3Tool { name: "link_pull_request", labels: ["Link", "Linking", "Linked", "a pull request"], summary: "link-pr", icon: "pull-request" },
    T3Tool { name: "unlink_pull_request", labels: ["Unlink", "Unlinking", "Unlinked", "a pull request"], summary: "unlink-pr", icon: "pull-request" },
    T3Tool { name: "list_thread_pull_requests", labels: ["Check", "Checking", "Checked", "linked pull requests"], summary: "list-prs", icon: "pull-request" },
    T3Tool { name: "watch_pull_request", labels: ["Watch", "Watching", "Watching", "a pull request"], summary: "watch-pr", icon: "pull-request" },
    T3Tool { name: "unwatch_pull_request", labels: ["Stop watching", "Stopping watching", "Stopped watching", "a pull request"], summary: "unwatch-pr", icon: "pull-request" },
    T3Tool { name: "write_pull_request_guides", labels: ["Write", "Writing", "Started", "review guides"], summary: "write-pr-guides", icon: "pull-request" },
    T3Tool { name: "read_pull_request_guide", labels: ["Read", "Reading", "Read", "a review guide"], summary: "read-pr-guide", icon: "pull-request" },
    T3Tool { name: "orchestrator_capabilities", labels: ["Get", "Getting", "Got", "orchestration capabilities"], summary: "capabilities", icon: "t3-code" },
    T3Tool { name: "delegate_task", labels: ["Delegate", "Delegating", "Delegated", "a child task"], summary: "delegate", icon: "t3-code" },
    T3Tool { name: "task_status", labels: ["Get", "Getting", "Got", "delegated task status"], summary: "task-status", icon: "t3-code" },
    T3Tool { name: "task_cancel", labels: ["Cancel", "Canceling", "Requested cancellation of", "delegated task"], summary: "task-cancel", icon: "t3-code" },
    T3Tool { name: "schedule_task", labels: ["Schedule", "Scheduling", "Scheduled", "a recurring task"], summary: "schedule-create", icon: "t3-code" },
    T3Tool { name: "list_scheduled_tasks", labels: ["List", "Listing", "Listed", "scheduled tasks"], summary: "schedule-list", icon: "t3-code" },
    T3Tool { name: "update_scheduled_task", labels: ["Update", "Updating", "Updated", "a scheduled task"], summary: "schedule-update", icon: "t3-code" },
    T3Tool { name: "delete_scheduled_task", labels: ["Delete", "Deleting", "Requested deletion of", "a scheduled task"], summary: "schedule-delete", icon: "t3-code" },
    T3Tool { name: "request_secret", labels: ["Ask for", "Asking for", "Asked for", "a secret"], summary: "secret-request", icon: "t3-code" },
    T3Tool { name: "create_threads", labels: ["Create", "Creating", "Created", "T3 threads"], summary: "thread-create", icon: "t3-code" },
    T3Tool { name: "t3_thread_start", labels: ["Start", "Starting", "Started", "a T3 thread"], summary: "thread-create", icon: "t3-code" },
    T3Tool { name: "t3_thread_list", labels: ["List", "Listing", "Listed", "T3 threads"], summary: "thread-list", icon: "t3-code" },
    T3Tool { name: "t3_thread_read", labels: ["Read", "Reading", "Read", "a T3 thread"], summary: "thread-read", icon: "t3-code" },
    T3Tool { name: "t3_thread_send", labels: ["Send", "Sending", "Sent", "to a T3 thread"], summary: "thread-send", icon: "t3-code" },
    T3Tool { name: "t3_thread_wait", labels: ["Wait", "Waiting", "Waited", "for a T3 thread"], summary: "thread-wait", icon: "t3-code" },
    T3Tool { name: "t3_workflow_stop", labels: ["Stop", "Stopping", "Requested a stop for", "a workflow"], summary: "workflow-stop", icon: "t3-code" },
    T3Tool { name: "t3_thread_interrupt", labels: ["Interrupt", "Interrupting", "Requested an interrupt of", "a T3 thread"], summary: "thread-interrupt", icon: "t3-code" },
    T3Tool { name: "t3_worktree_handoff", labels: ["Hand off", "Handing off", "Handed off", "thread to a git worktree"], summary: "worktree-handoff", icon: "t3-code" },
    T3Tool { name: "t3_worktree_status", labels: ["Get", "Getting", "Got", "thread worktree status"], summary: "worktree-status", icon: "t3-code" },
    T3Tool { name: "preview_status", labels: ["Get", "Getting", "Got", "preview browser status"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_open", labels: ["Open", "Opening", "Opened", "a page in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_navigate", labels: ["Navigate", "Navigating", "Navigated", "the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_dialog", labels: ["Respond", "Responding", "Responded", "to a preview browser dialog"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_snapshot", labels: ["Take a snapshot of", "Taking a snapshot of", "Took a snapshot of", "the preview page"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_click", labels: ["Click", "Clicking", "Clicked", "in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_press", labels: ["Press", "Pressing", "Pressed", "a key in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_type", labels: ["Type", "Typing", "Typed", "in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_hover", labels: ["Hover", "Hovering", "Hovered", "in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_select", labels: ["Choose", "Choosing", "Chose", "an option in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_drag", labels: ["Drag", "Dragging", "Dragged", "in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_upload", labels: ["Upload", "Uploading", "Uploaded", "files to the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_scroll", labels: ["Scroll", "Scrolling", "Scrolled", "the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_resize", labels: ["Resize", "Resizing", "Resized", "the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_evaluate", labels: ["Evaluate", "Evaluating", "Evaluated", "script in the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_wait_for", labels: ["Wait", "Waiting", "Waited", "for the preview page"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_set_appearance", labels: ["Set", "Setting", "Set", "preview browser appearance"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_recording_start", labels: ["Start", "Starting", "Started", "recording the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "preview_recording_stop", labels: ["Stop", "Stopping", "Stopped", "recording the preview browser"], summary: "browser", icon: "browser" },
    T3Tool { name: "device_list", labels: ["List", "Listing", "Listed", "simulators and emulators"], summary: "device", icon: "device" },
    T3Tool { name: "device_open", labels: ["Open", "Opening", "Opened", "a device in the Device panel"], summary: "device", icon: "device" },
    T3Tool { name: "device_screenshot", labels: ["Take a screenshot of", "Taking a screenshot of", "Took a screenshot of", "the device"], summary: "device", icon: "device" },
    T3Tool { name: "device_close", labels: ["Close", "Closing", "Closed", "a device"], summary: "device", icon: "device" },
    T3Tool { name: "run_scheduled_task_now", labels: ["Run", "Running", "Requested a run of", "a scheduled task"], summary: "schedule-run", icon: "t3-code" },
    T3Tool { name: "t3_queue_list", labels: ["List", "Listing", "Listed", "queued messages"], summary: "queue-list", icon: "t3-code" },
    T3Tool { name: "t3_queue_read", labels: ["Read", "Reading", "Read", "a queued message"], summary: "queue-read", icon: "t3-code" },
    T3Tool { name: "t3_queue_edit", labels: ["Edit", "Editing", "Edited", "a queued message"], summary: "queue-edit", icon: "t3-code" },
    T3Tool { name: "t3_queue_cancel", labels: ["Cancel", "Canceling", "Requested cancellation of", "a queued run"], summary: "queue-cancel", icon: "t3-code" },
    T3Tool { name: "t3_queue_reorder", labels: ["Reorder", "Reordering", "Reordered", "a queued run"], summary: "queue-reorder", icon: "t3-code" },
    T3Tool { name: "t3_queue_promote_to_steer", labels: ["Steer with", "Steering with", "Requested steering with", "a queued message"], summary: "queue-steer", icon: "t3-code" },
    T3Tool { name: "t3_pending_request_list", labels: ["List", "Listing", "Listed", "pending questions"], summary: "question-list", icon: "t3-code" },
    T3Tool { name: "t3_pending_request_read", labels: ["Read", "Reading", "Read", "pending questions"], summary: "question-read", icon: "t3-code" },
    T3Tool { name: "t3_pending_request_respond", labels: ["Answer", "Answering", "Answered", "pending questions"], summary: "question-respond", icon: "t3-code" },
    T3Tool { name: "t3_thread_configuration", labels: ["Read", "Reading", "Read", "thread configuration"], summary: "thread-configuration", icon: "t3-code" },
    T3Tool { name: "t3_thread_configure", labels: ["Set", "Setting", "Set", "thread model"], summary: "thread-configure", icon: "t3-code" },
    T3Tool { name: "t3_thread_fork", labels: ["Fork", "Forking", "Requested a fork of", "this thread"], summary: "thread-fork", icon: "t3-code" },
    T3Tool { name: "t3_thread_merge_back", labels: ["Merge", "Merging", "Requested a merge of", "thread context"], summary: "thread-merge", icon: "t3-code" },
    T3Tool { name: "t3_thread_search", labels: ["Search", "Searching", "Searched", "thread content"], summary: "thread-search", icon: "t3-code" },
    T3Tool { name: "t3_thread_transfers", labels: ["Read", "Reading", "Read", "thread transfers"], summary: "thread-transfers", icon: "t3-code" },
    T3Tool { name: "t3_thread_organize", labels: ["Organize", "Organizing", "Organized", "a thread"], summary: "thread-organize", icon: "t3-code" },
    T3Tool { name: "t3_thread_update", labels: ["Update", "Updating", "Updated", "T3 thread metadata"], summary: "thread-update", icon: "t3-code" },
    T3Tool { name: "t3_worktree_list", labels: ["List", "Listing", "Listed", "workspace branches"], summary: "worktree-list", icon: "t3-code" },
    T3Tool { name: "t3_preview_list", labels: ["List", "Listing", "Listed", "preview tabs"], summary: "browser", icon: "browser" },
    T3Tool { name: "t3_preview_close", labels: ["Close", "Closing", "Closed", "a preview tab"], summary: "browser", icon: "browser" },
    T3Tool { name: "t3_environment_read", labels: ["Read", "Reading", "Read", "environment preferences"], summary: "environment-read", icon: "t3-code" },
    T3Tool { name: "t3_environment_preferences_update", labels: ["Update", "Updating", "Updated", "environment preferences"], summary: "environment-update", icon: "t3-code" },
    T3Tool { name: "t3_thread_launch", labels: ["Launch", "Launching", "Launched", "a project thread"], summary: "thread-create", icon: "t3-code" },
    T3Tool { name: "t3_project_list", labels: ["List", "Listing", "Listed", "projects"], summary: "project-list", icon: "t3-code" },
    T3Tool { name: "t3_project_read", labels: ["Read", "Reading", "Read", "a project"], summary: "project-read", icon: "t3-code" },
    T3Tool { name: "t3_project_create", labels: ["Register", "Registering", "Registered", "a project"], summary: "project-create", icon: "t3-code" },
    T3Tool { name: "t3_project_update", labels: ["Update", "Updating", "Updated", "a project"], summary: "project-update", icon: "t3-code" },
    T3Tool { name: "t3_project_delete", labels: ["Delete", "Deleting", "Deleted", "a project"], summary: "project-delete", icon: "t3-code" },
    T3Tool { name: "t3_project_clone", labels: ["Clone", "Cloning", "Cloned", "a repository"], summary: "project-clone", icon: "t3-code" },
    T3Tool { name: "t3_attachment_prepare_upload", labels: ["Prepare", "Preparing", "Prepared", "an attachment upload"], summary: "attachment-prepare", icon: "t3-code" },
    T3Tool { name: "t3_attachment_discard", labels: ["Discard", "Discarding", "Discarded", "a pending attachment"], summary: "attachment-discard", icon: "t3-code" },
    T3Tool { name: "t3_thread_send_attachments", labels: ["Send", "Sending", "Sent", "attachments"], summary: "attachment-send", icon: "t3-code" },
    T3Tool { name: "html_preview", labels: ["Preview", "Previewing", "Previewed", "an HTML page"], summary: "html-preview", icon: "t3-code" },
    T3Tool { name: "html_render", labels: ["Render", "Rendering", "Rendered", "an HTML page"], summary: "html-render", icon: "t3-code" },
    T3Tool { name: "memory_save", labels: ["Save", "Saving", "Saved", "to memory"], summary: "memory-save", icon: "t3-code" },
    T3Tool { name: "memory_update", labels: ["Update", "Updating", "Updated", "a memory"], summary: "memory-update", icon: "t3-code" },
    T3Tool { name: "memory_delete", labels: ["Delete", "Deleting", "Deleted", "a memory"], summary: "memory-delete", icon: "t3-code" },
    T3Tool { name: "code_run", labels: ["Run", "Running", "Ran", "Python"], summary: "code-run", icon: "t3-code" },
];

const SERVERS: [&str; 3] = ["t3-code", "t3_code", "t3code"];

fn known(name: &str) -> Option<&'static T3Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// `normalizeT3McpToolLabel`.
fn normalize_label(value: &str) -> &str {
    let trimmed = value.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    for suffix in ["completed", "complete"] {
        if lower.ends_with(suffix) {
            let rest = &trimmed[..trimmed.len() - suffix.len()];
            if rest.ends_with(char::is_whitespace) {
                return rest.trim();
            }
        }
    }
    trimmed.trim()
}

/// `resolveT3McpToolDefinition`: T3's tool under any provider's spelling
/// (`mcp__t3-code__x`, `t3-code.x`, `t3_code:x`, `t3code/x`,
/// `t3-code · x`, `mcp-t3-code_x`, OpenCode's `t3-code-<thread>_x`, `x`).
pub fn t3_tool(value: &str) -> Option<&'static T3Tool> {
    let label = normalize_label(value);
    let lower = label.to_lowercase();
    if lower.starts_with("mcp__") {
        let rest = &label["mcp__".len()..];
        let (server, tool) = rest.split_once("__")?;
        return SERVERS
            .contains(&server.to_lowercase().as_str())
            .then(|| known(tool))
            .flatten();
    }
    for server in SERVERS {
        if let Some(rest) = lower.strip_prefix(server) {
            let rest_original = &label[server.len()..];
            let separated = rest.starts_with(['.', ':', '/']);
            let dotted = rest.trim_start().starts_with('·');
            if separated {
                return known(&rest_original[1..]);
            }
            if dotted {
                let tool = rest_original
                    .trim_start()
                    .trim_start_matches('·')
                    .trim_start();
                return known(tool);
            }
        }
    }
    // `^(?:mcp[-_]{1,2})?t3[-_ ]?code(?:__|[-_.:/ ])(?<tool>.+)$`.
    let candidate = prefixed_tool(label).unwrap_or(label);
    if let Some(tool) = known(candidate) {
        return Some(tool);
    }
    // OpenCode 2: one server per thread, `t3-code-<thread>_<tool>`; the
    // longest known tool ending the label.
    if !lower.starts_with("t3-code-") {
        return None;
    }
    TOOLS
        .iter()
        .filter(|tool| label.ends_with(&format!("_{}", tool.name)))
        .max_by_key(|tool| tool.name.len())
}

fn prefixed_tool(label: &str) -> Option<&str> {
    let lower = label.to_ascii_lowercase();
    let mut at = 0;
    if let Some(after) = lower.strip_prefix("mcp") {
        let dashes = after.chars().take_while(|c| matches!(c, '-' | '_')).count();
        if (1..=2).contains(&dashes) {
            at = 3 + dashes;
        }
    }
    let rest = lower.get(at..)?.strip_prefix("t3")?;
    let mut at = label.len() - rest.len();
    let rest = match rest.strip_prefix(['-', '_', ' ']) {
        Some(stripped) => {
            at += 1;
            stripped
        }
        None => rest,
    };
    rest.strip_prefix("code")?;
    at += 4;
    let tail = &label[at..];
    let tool = tail
        .strip_prefix("__")
        .or_else(|| tail.strip_prefix(['-', '_', '.', ':', '/', ' ']))?;
    (!tool.is_empty()).then_some(tool)
}

/// `readResult`: an MCP result envelope's data, and whether it failed.
pub fn read_result(value: &Value) -> (Option<Map<String, Value>>, bool) {
    read_result_at(value, 0)
}

fn read_result_at(value: &Value, depth: usize) -> (Option<Map<String, Value>>, bool) {
    if depth > 4 {
        return (None, false);
    }
    match value {
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(parsed) => read_result_at(&parsed, depth + 1),
            Err(_) => (None, false),
        },
        Value::Array(blocks) => {
            let mut data = None;
            let mut failed = false;
            for block in blocks {
                let text = if block["text"].is_null() {
                    &block["content"]["text"]
                } else {
                    &block["text"]
                };
                let inner = if text["text"].is_null() {
                    text
                } else {
                    &text["text"]
                };
                let (result, block_failed) = read_result_at(inner, depth + 1);
                if data.is_none() {
                    data = result;
                }
                failed |= block_failed;
            }
            (data, failed)
        }
        Value::Object(record) => {
            let failed = record.get("isError") == Some(&Value::Bool(true))
                || record.get("is_error") == Some(&Value::Bool(true))
                || record
                    .get("_tag")
                    .and_then(Value::as_str)
                    .is_some_and(|tag| tag.ends_with("Error") || tag.ends_with("Failure"))
                || record.get("error").is_some_and(|e| !e.is_null());
            match record
                .get("structuredContent")
                .or_else(|| record.get("content"))
            {
                Some(content) => {
                    let (data, inner) = read_result_at(content, depth + 1);
                    (data, failed || inner)
                }
                None => (Some(record.clone()), failed),
            }
        }
        _ => (None, false),
    }
}

/// `readInput`: a call's arguments; Cursor keeps them in its MCP envelope.
pub fn read_input(value: &Value) -> Option<Map<String, Value>> {
    let input = read_result(value).0?;
    if input.get("toolName").is_some_and(Value::is_string) {
        return input.get("args").and_then(Value::as_object).cloned();
    }
    Some(input)
}

/// `t3ToolResultIndicatesFailure`.
pub fn t3_result_failed(output: &Value) -> bool {
    read_result(output).1
}

/// The PR number of a pull or merge request URL (`parseChangeRequestUrl`'s
/// `/pull/N`, `/pulls/N`, `/-/merge_requests/N`, `/pull-requests/N`,
/// `/pullrequest/N`).
pub fn pull_request_number(url: &str) -> Option<u64> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    let path = url.split(['?', '#']).next()?;
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.windows(2).find_map(|pair| {
        matches!(
            pair[0],
            "pull" | "pulls" | "merge_requests" | "pull-requests" | "pullrequest"
        )
        .then(|| pair[1].parse().ok())
        .flatten()
        .filter(|n| *n > 0)
    })
}

/// `resolveT3McpToolPresentation`'s `displayName`: the tense follows the
/// status; a pull request action names its PR.
pub fn t3_label(tool: &T3Tool, status: &str, input: &Value) -> String {
    let [action, running, completed, detail] = tool.labels;
    let verb = match status {
        "inProgress" => running.to_owned(),
        "completed" => completed.to_owned(),
        "failed" => format!("Failed to {}", action.to_lowercase()),
        "declined" => format!("Declined to {}", action.to_lowercase()),
        "stopped" => format!("Stopped {}", running.to_lowercase()),
        _ => running.to_owned(),
    };
    let pr_action = matches!(
        tool.summary,
        "link-pr" | "unlink-pr" | "watch-pr" | "unwatch-pr"
    );
    let args = read_input(input).unwrap_or_default();
    let number = args
        .get("url")
        .and_then(Value::as_str)
        .and_then(pull_request_number)
        .or_else(|| {
            args.get("number")
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
        });
    match number.filter(|_| pr_action) {
        Some(number) => format!("{verb} PR #{number}"),
        None => format!("{verb} {detail}"),
    }
}

/// `ToolActivityAction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Command,
    Read,
    FileChange,
    Search,
    Other,
}

/// `toolNameToken`: server-prefixed names are never local reads or searches.
fn tool_token(value: Option<&str>) -> Option<String> {
    let trimmed = value.map(str::trim).filter(|v| !v.is_empty())?;
    if trimmed.contains("__") || trimmed.contains(['.', '/']) {
        return None;
    }
    Some(
        trimmed
            .chars()
            .filter(|c| !matches!(c, '_' | '-') && !c.is_whitespace())
            .collect::<String>()
            .to_lowercase(),
    )
}

/// `classifyToolActivity`.
pub fn classify(
    item_type: Option<&str>,
    request_kind: Option<&str>,
    kind: Option<&str>,
    tool_name: Option<&str>,
) -> Activity {
    let request_kind = request_kind.map(|k| k.trim().to_lowercase());
    let kind = kind.map(|k| k.trim().to_lowercase());
    let (request_kind, kind) = (request_kind.as_deref(), kind.as_deref());
    let tool = tool_token(tool_name);
    let tool = tool.as_deref();
    match item_type {
        Some("command_execution") => return Activity::Command,
        Some("image_view") => return Activity::Read,
        Some("file_change") => return Activity::FileChange,
        Some("web_search") => return Activity::Search,
        _ => {}
    }
    if request_kind == Some("command") || kind == Some("execute") {
        Activity::Command
    } else if request_kind == Some("file-change")
        || matches!(kind, Some("edit" | "move" | "delete" | "write"))
    {
        Activity::FileChange
    } else if kind == Some("search") || matches!(tool, Some("find" | "grep" | "glob" | "rg" | "ls"))
    {
        Activity::Search
    } else if request_kind == Some("file-read") || kind == Some("read") {
        Activity::Read
    } else if matches!(tool, Some("terminal" | "bash" | "shell")) {
        Activity::Command
    } else if matches!(tool, Some("read" | "readfile")) {
        Activity::Read
    } else {
        Activity::Other
    }
}

/// `isJavaScriptReplTool`.
pub fn is_js_repl(tool_name: Option<&str>) -> bool {
    matches!(tool_name, Some("js" | "cua_repl.js" | "node_repl.js"))
}

fn trimmed(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|v| !v.is_empty())
}

/// `dynamicToolSurface`: computer use, before any result says so.
pub fn dynamic_tool_surface(tool_name: Option<&str>, input: &Value) -> Option<&'static str> {
    if tool_name == Some("cua_repl.js") {
        return Some("computer");
    }
    let code = input["code"].as_str()?;
    let sky = code.match_indices("sky").any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let word_start = !before.is_some_and(|c| c.is_alphanumeric() || c == '_');
        word_start && code[at + 3..].trim_start().starts_with('.')
    });
    (is_js_repl(tool_name) && (sky || code.contains("\"@oai/sky\"") || code.contains("'@oai/sky'")))
        .then_some("computer")
}

/// `dynamicToolTitle`: a REPL's title, or the skill a Claude `Skill` loads.
pub fn dynamic_tool_title(
    tool_name: Option<&str>,
    input: &Value,
    retained: Option<&str>,
) -> Option<String> {
    if is_js_repl(tool_name) {
        let title = retained.map(str::trim).filter(|t| {
            !t.is_empty()
                && !t.eq_ignore_ascii_case("js")
                && !tool_name.is_some_and(|n| t.eq_ignore_ascii_case(n))
        });
        return Some(
            trimmed(&input["title"])
                .or(title)
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    if dynamic_tool_surface(tool_name, input) == Some("computer") {
                        "Computer use".into()
                    } else {
                        "Run JavaScript".into()
                    }
                }),
        );
    }
    claude_skill(tool_name, input).map(|(name, _)| format!("Skill: {name}"))
}

/// `claudeSkillInvocation`: the skill a Claude `Skill` call loads, and its
/// arguments.
pub fn claude_skill<'a>(
    tool_name: Option<&str>,
    input: &'a Value,
) -> Option<(&'a str, Option<&'a str>)> {
    if tool_name != Some("Skill") {
        return None;
    }
    Some((trimmed(&input["skill"])?, trimmed(&input["args"])))
}

const PATH_KEYS: [&str; 8] = [
    "path",
    "filePath",
    "file_path",
    "relativePath",
    "filename",
    "fileName",
    "newPath",
    "oldPath",
];

/// `collectToolFilePaths`: structured paths, never file bodies; at most 8.
pub fn collect_file_paths(value: &Value) -> Vec<String> {
    fn walk(value: &Value, paths: &mut Vec<String>, depth: usize) {
        if depth > 4 || paths.len() >= 8 {
            return;
        }
        if let Some(list) = value.as_array() {
            for entry in list {
                walk(entry, paths, depth + 1);
                if paths.len() >= 8 {
                    return;
                }
            }
            return;
        }
        let Some(record) = value.as_object() else {
            return;
        };
        for key in PATH_KEYS {
            if let Some(path) = record.get(key).and_then(trimmed)
                && !paths.iter().any(|p| p == path)
            {
                paths.push(path.to_owned());
                if paths.len() >= 8 {
                    return;
                }
            }
        }
        for nested in [
            "locations",
            "item",
            "input",
            "result",
            "rawInput",
            "data",
            "changes",
        ] {
            if let Some(value) = record.get(nested) {
                walk(value, paths, depth + 1);
                if paths.len() >= 8 {
                    return;
                }
            }
        }
    }
    let mut paths = Vec::new();
    walk(value, &mut paths, 0);
    paths
}

fn first_string<'a>(record: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| trimmed(&record[*key]))
}

/// `formatSearchToolLabel`: "Searched files *.{ts,tsx} in pluto".
pub fn search_label(data: &Value) -> Option<String> {
    let has_keys = |v: &Value| v.as_object().is_some_and(|o| !o.is_empty());
    let input = [&data["rawInput"], &data["input"], &data["item"]["input"]]
        .into_iter()
        .find(|v| has_keys(v))
        .unwrap_or(data);
    let query = first_string(
        input,
        &["pattern", "query", "searchTerm", "regex", "grep", "needle"],
    );
    let glob = first_string(
        input,
        &[
            "glob",
            "globPattern",
            "glob_pattern",
            "include",
            "filePattern",
            "file_pattern",
        ],
    );
    let target = first_string(
        input,
        &[
            "path",
            "target_directory",
            "targetDirectory",
            "directory",
            "cwd",
            "root",
        ],
    )
    .and_then(|t| t.split(['/', '\\']).rfind(|p| !p.is_empty() && *p != "."));
    Some(match (query, glob, target) {
        (Some(q), _, Some(t)) => format!("Searched {q} in {t}"),
        (None, Some(g), Some(t)) => format!("Searched files {g} in {t}"),
        (None, Some(g), None) => format!("Searched files {g}"),
        (Some(q), _, None) => format!("Searched {q}"),
        (None, None, Some(t)) => format!("Searched in {t}"),
        (None, None, None) => return None,
    })
}

/// `formatReadToolLabel`: the verb and the path, never the path alone.
pub fn read_label(path: &str, extra: usize) -> String {
    let suffix = if extra > 0 {
        format!(" +{extra} more")
    } else {
        String::new()
    };
    match path.trim() {
        "" => format!("Read file{suffix}"),
        path => format!("Read {path}{suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_t3_tools_under_every_spelling() {
        for name in [
            "mcp__t3-code__delegate_task",
            "t3-code.delegate_task",
            "t3_code:delegate_task",
            "t3code/delegate_task",
            "t3-code · delegate_task",
            "mcp-t3-code_delegate_task",
            "t3-code-thread_1_delegate_task",
            "delegate_task",
            "delegate_task completed",
        ] {
            assert_eq!(
                t3_tool(name).map(|t| t.name),
                Some("delegate_task"),
                "{name}"
            );
        }
        assert!(t3_tool("mcp__github__delegate_task").is_none());
        assert!(t3_tool("read_file").is_none());
        assert_eq!(TOOLS.len(), 88);
    }

    #[test]
    fn labels_follow_the_status_and_name_the_pull_request() {
        let link = t3_tool("link_pull_request").unwrap();
        let input = json!({"url": "https://github.com/a/b/pull/12"});
        assert_eq!(t3_label(link, "completed", &input), "Linked PR #12");
        assert_eq!(
            t3_label(link, "failed", &json!({})),
            "Failed to link a pull request"
        );
        let delegate = t3_tool("delegate_task").unwrap();
        assert_eq!(
            t3_label(delegate, "inProgress", &json!({})),
            "Delegating a child task"
        );
        assert_eq!(
            t3_label(t3_tool("code_run").unwrap(), "completed", &json!({})),
            "Ran Python"
        );
        assert!(t3_result_failed(
            &json!({"content": [{"type": "text", "text": "{\"error\": \"no\"}"}]})
        ));
        assert!(!t3_result_failed(
            &json!({"content": [{"type": "text", "text": "ok"}]})
        ));
    }

    #[test]
    fn classifies_and_names_tool_calls() {
        assert_eq!(classify(None, None, None, Some("Grep")), Activity::Search);
        assert_eq!(
            classify(None, None, None, Some("github.read_file")),
            Activity::Other
        );
        assert_eq!(classify(None, None, None, Some("Read")), Activity::Read);
        assert_eq!(
            classify(None, None, Some("execute"), None),
            Activity::Command
        );
        let search = json!({"input": {"pattern": "fn main", "path": "/src/pluto/"}});
        assert_eq!(
            search_label(&search).as_deref(),
            Some("Searched fn main in pluto")
        );
        assert_eq!(read_label("", 2), "Read file +2 more");
        assert_eq!(
            collect_file_paths(&json!({"input": {"file_path": "/a.rs", "content": "x"}})),
            ["/a.rs"]
        );
        assert_eq!(
            dynamic_tool_title(
                Some("js"),
                &json!({"code": "await sky.click()"}),
                Some("js")
            )
            .as_deref(),
            Some("Computer use")
        );
        assert_eq!(
            dynamic_tool_title(Some("Skill"), &json!({"skill": "review"}), None).as_deref(),
            Some("Skill: review")
        );
    }
}
